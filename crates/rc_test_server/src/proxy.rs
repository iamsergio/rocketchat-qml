// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! A TCP proxy in front of the test server, to cut connections on demand. Tests of reconnecting
//! point the client at the proxy, and the server cannot make itself unreachable.

use std::io;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::server_url;

/// Forwards every connection to the test server, byte for byte, until told to cut them. Stops, and
/// cuts what is left, when dropped.
pub struct Proxy {
    addr: SocketAddr,
    shared: Arc<Shared>,
    /// Ends with the handles of the threads that pump the connections.
    acceptor: Option<JoinHandle<Vec<JoinHandle<()>>>>,
}

struct Shared {
    accepting: AtomicBool,
    stopping: AtomicBool,
    /// The proxy's end of every live connection: `(client side, server side)`. Shutting down these
    /// handles ends the pumps, which hold clones of the same sockets.
    live: Mutex<Vec<(TcpStream, TcpStream)>>,
}

impl Shared {
    fn cut_all(&self) {
        for (client, server) in self.live.lock().unwrap().drain(..) {
            // Already closed by the other end is fine.
            let _ = client.shutdown(Shutdown::Both);
            let _ = server.shutdown(Shutdown::Both);
        }
    }
}

impl Proxy {
    /// A proxy to the server named by `RC_TEST_SERVER_URL`, listening on a free port of the
    /// loopback interface.
    ///
    /// # Panics
    ///
    /// If the server is unset or its address can't be resolved, or no port can be bound.
    #[must_use]
    pub fn start() -> Self {
        Self::start_to(&server_url())
    }

    /// As [`start`](Self::start), for the server at `url` (`http://host:port`).
    fn start_to(url: &str) -> Self {
        let upstream = resolve(url);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let shared = Arc::new(Shared {
            accepting: AtomicBool::new(true),
            stopping: AtomicBool::new(false),
            live: Mutex::new(Vec::new()),
        });
        let acceptor = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || accept_loop(&listener, upstream, &shared))
        };
        Self {
            addr,
            shared,
            acceptor: Some(acceptor),
        }
    }

    /// What to give the client instead of the server's URL.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Closes every connection that is open now. New ones are accepted as before, unless
    /// [`set_accepting`](Self::set_accepting) said otherwise.
    pub fn drop_connections(&self) {
        self.shared.cut_all();
    }

    /// Whether new connections are forwarded. While not, they are accepted and closed at once, as
    /// a server that is up but not answering would. Connections open already are not touched.
    pub fn set_accepting(&self, accepting: bool) {
        self.shared.accepting.store(accepting, Ordering::SeqCst);
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        // `accept` blocks: a connection makes it look at the flag.
        let _ = TcpStream::connect(self.addr);
        if let Some(acceptor) = self.acceptor.take() {
            self.shared.cut_all();
            if let Ok(pumps) = acceptor.join() {
                // A connection accepted after the cut above is cut by now.
                self.shared.cut_all();
                for pump in pumps {
                    let _ = pump.join();
                }
            }
        }
    }
}

/// The socket address in a `http://host:port` URL.
fn resolve(url: &str) -> SocketAddr {
    let (scheme, rest) = url.split_once("://").unwrap_or(("http", url));
    let authority = rest.split('/').next().unwrap();
    let with_port = if authority.contains(':') {
        authority.to_owned()
    } else {
        format!("{authority}:{}", if scheme == "https" { 443 } else { 80 })
    };
    with_port
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap_or_else(|| panic!("{url} has no address"))
}

fn accept_loop(
    listener: &TcpListener,
    upstream: SocketAddr,
    shared: &Arc<Shared>,
) -> Vec<JoinHandle<()>> {
    let mut pumps = Vec::new();
    for client in listener.incoming() {
        if shared.stopping.load(Ordering::SeqCst) {
            break;
        }
        let Ok(client) = client else { continue };
        if !shared.accepting.load(Ordering::SeqCst) {
            // Dropped: closed.
            continue;
        }
        let Ok(server) = TcpStream::connect(upstream) else {
            continue;
        };
        let _ = client.set_nodelay(true);
        let _ = server.set_nodelay(true);
        let (Ok(client_cut), Ok(server_cut), Ok(client_back), Ok(server_back)) = (
            client.try_clone(),
            server.try_clone(),
            client.try_clone(),
            server.try_clone(),
        ) else {
            continue;
        };
        // Registered before any byte moves, so that a cut never misses a pair.
        shared.live.lock().unwrap().push((client_cut, server_cut));
        pumps.push(thread::spawn(move || pump(client, server_back)));
        pumps.push(thread::spawn(move || pump(server, client_back)));
    }
    pumps
}

/// Copies `from` to `to` until either ends, then closes both so that the other direction ends too.
fn pump(mut from: TcpStream, mut to: TcpStream) {
    // How it ended doesn't matter: a cut and a close look the same.
    let _ = io::copy(&mut from, &mut to);
    let _ = from.shutdown(Shutdown::Both);
    let _ = to.shutdown(Shutdown::Both);
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::time::Duration;

    use super::*;

    /// Echoes every connection until it closes. Lives as long as the process: it is a test.
    fn echo_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                thread::spawn(move || {
                    let mut buffer = [0_u8; 1024];
                    while let Ok(read) = stream.read(&mut buffer) {
                        if read == 0 || stream.write_all(&buffer[..read]).is_err() {
                            break;
                        }
                    }
                });
            }
        });
        url
    }

    fn connect(proxy: &Proxy) -> TcpStream {
        let stream = TcpStream::connect(proxy.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
    }

    /// Sends `text` and reads the echo. `None` if the connection is closed.
    fn round_trip(stream: &mut TcpStream, text: &str) -> Option<String> {
        stream.write_all(text.as_bytes()).ok()?;
        let mut buffer = vec![0_u8; text.len()];
        stream.read_exact(&mut buffer).ok()?;
        Some(String::from_utf8(buffer).unwrap())
    }

    // RT-003
    #[test]
    fn it_forwards_both_ways() {
        let proxy = Proxy::start_to(&echo_server());
        let mut stream = connect(&proxy);
        assert_eq!(round_trip(&mut stream, "hello").as_deref(), Some("hello"));
        assert_eq!(round_trip(&mut stream, "again").as_deref(), Some("again"));
    }

    // RT-003
    #[test]
    fn dropping_connections_closes_the_open_ones_and_accepts_new_ones() {
        let proxy = Proxy::start_to(&echo_server());
        let mut first = connect(&proxy);
        let mut second = connect(&proxy);
        assert!(round_trip(&mut first, "a").is_some());
        assert!(round_trip(&mut second, "b").is_some());
        proxy.drop_connections();
        assert_eq!(round_trip(&mut first, "c"), None);
        assert_eq!(round_trip(&mut second, "d"), None);
        let mut third = connect(&proxy);
        assert_eq!(round_trip(&mut third, "e").as_deref(), Some("e"));
    }

    // RT-003
    #[test]
    fn while_not_accepting_new_connections_are_closed_at_once() {
        let proxy = Proxy::start_to(&echo_server());
        let mut open = connect(&proxy);
        assert!(round_trip(&mut open, "a").is_some());
        proxy.set_accepting(false);
        // The open one carries on; new ones are closed.
        assert!(round_trip(&mut open, "b").is_some());
        let mut refused = connect(&proxy);
        assert_eq!(round_trip(&mut refused, "c"), None);
        proxy.set_accepting(true);
        let mut again = connect(&proxy);
        assert_eq!(round_trip(&mut again, "d").as_deref(), Some("d"));
    }

    // RT-003
    #[test]
    fn dropping_the_proxy_closes_everything() {
        let proxy = Proxy::start_to(&echo_server());
        let addr = proxy.addr;
        let mut stream = connect(&proxy);
        assert!(round_trip(&mut stream, "a").is_some());
        drop(proxy);
        assert_eq!(round_trip(&mut stream, "b"), None);
        assert!(TcpStream::connect(addr).is_err());
    }

    #[test]
    fn the_server_url_is_resolved_with_or_without_a_port_or_path() {
        assert_eq!(
            resolve("http://127.0.0.1:3000"),
            "127.0.0.1:3000".parse().unwrap()
        );
        assert_eq!(
            resolve("http://127.0.0.1:3000/"),
            "127.0.0.1:3000".parse().unwrap()
        );
        assert_eq!(resolve("http://127.0.0.1"), "127.0.0.1:80".parse().unwrap());
        assert_eq!(resolve("127.0.0.1:81"), "127.0.0.1:81".parse().unwrap());
    }
}
