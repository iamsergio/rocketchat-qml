// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The REST client against a mock HTTP server serving real Rocket.Chat responses.

use std::net::TcpListener;

use rc_api::{Credentials, Error, RestClient, RestSession, RoomType, ServerVersion};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const HISTORY: &str = include_str!("fixtures/history.json");
const INFO: &str = include_str!("fixtures/info.json");
const LOGIN: &str = include_str!("fixtures/login.json");
const LOGIN_UNAUTHORIZED: &str = include_str!("fixtures/login_unauthorized.json");
const ME: &str = include_str!("fixtures/me.json");
const NOT_LOGGED_IN: &str = include_str!("fixtures/not_logged_in.json");
const SUBSCRIPTIONS: &str = include_str!("fixtures/subscriptions.json");

const USER_ID: &str = "u8paxsGQXMH9vSnSc";
const TOKEN: &str = "fake-token";

fn json_response(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .insert_header("content-type", "application/json")
        .set_body_string(body)
}

fn session(client: &RestClient) -> RestSession {
    client.session(Credentials {
        user_id: USER_ID.to_owned(),
        token: SecretString::from(TOKEN),
    })
}

/// Mounts `response` for `verb path` that only matches when the auth headers are present.
async fn mount_authenticated(
    server: &MockServer,
    verb: &str,
    route: &str,
    response: ResponseTemplate,
) {
    Mock::given(method(verb))
        .and(path(route))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn mount(server: &MockServer, verb: &str, route: &str, response: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(response)
        .mount(server)
        .await;
}

// AUTH-003
#[tokio::test]
async fn info_returns_the_version() {
    let server = MockServer::start().await;
    mount(&server, "GET", "/api/info", json_response(200, INFO)).await;
    let info = RestClient::new(&server.uri())
        .unwrap()
        .info()
        .await
        .unwrap();
    assert_eq!(info.version, ServerVersion { major: 8, minor: 0 });
}

// AUTH-003
#[tokio::test]
async fn info_on_an_html_page_is_not_rocketchat() {
    let server = MockServer::start().await;
    let page = ResponseTemplate::new(200)
        .insert_header("content-type", "text/html")
        .set_body_string("<html><body>Hello</body></html>");
    mount(&server, "GET", "/api/info", page).await;
    let result = RestClient::new(&server.uri()).unwrap().info().await;
    assert!(matches!(result, Err(Error::NotRocketChat)), "{result:?}");
}

// AUTH-003
#[tokio::test]
async fn info_on_a_404_is_not_rocketchat() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/info",
        ResponseTemplate::new(404).set_body_string("404 Not Found"),
    )
    .await;
    let result = RestClient::new(&server.uri()).unwrap().info().await;
    assert!(matches!(result, Err(Error::NotRocketChat)), "{result:?}");
}

// AUTH-003
#[tokio::test]
async fn info_on_a_closed_port_is_unreachable() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let result = RestClient::new(&format!("http://127.0.0.1:{port}"))
        .unwrap()
        .info()
        .await;
    assert!(matches!(result, Err(Error::Unreachable(_))), "{result:?}");
}

// AUTH-003
#[tokio::test]
async fn server_errors_carry_the_message() {
    let server = MockServer::start().await;
    let body = r#"{"success":false,"error":"Internal error","message":"Something broke"}"#;
    mount(&server, "POST", "/api/v1/login", json_response(500, body)).await;
    let result = RestClient::new(&server.uri())
        .unwrap()
        .login("a", &SecretString::from("b"))
        .await;
    match result {
        Err(Error::Server {
            status: 500,
            message,
        }) => assert_eq!(message, "Something broke"),
        other => panic!("{other:?}"),
    }
}

// AUTH-001
#[tokio::test]
async fn login_returns_credentials_and_user() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/login"))
        .and(body_json(json!({ "user": "admin", "password": "secret" })))
        .respond_with(json_response(200, LOGIN))
        .mount(&server)
        .await;
    let client = RestClient::new(&server.uri()).unwrap();
    let result = client
        .login("admin", &SecretString::from("secret"))
        .await
        .unwrap();
    assert_eq!(result.credentials.user_id, "6ad649cecaea8883e7e075ec");
    assert_eq!(
        result.credentials.token.expose_secret(),
        "fake-auth-token-0123456789abcdefghijklmnopq"
    );
    assert_eq!(result.user.username, "admin");
}

// AUTH-001
#[tokio::test]
async fn login_works_under_a_path_prefix() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/chat/api/v1/login",
        json_response(200, LOGIN),
    )
    .await;
    let client = RestClient::new(&format!("{}/chat", server.uri())).unwrap();
    let result = client
        .login("admin", &SecretString::from("secret"))
        .await
        .unwrap();
    assert_eq!(result.user.username, "admin");
}

// AUTH-002
#[tokio::test]
async fn login_with_wrong_credentials_is_invalid_credentials() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/login",
        json_response(401, LOGIN_UNAUTHORIZED),
    )
    .await;
    let result = RestClient::new(&server.uri())
        .unwrap()
        .login("admin", &SecretString::from("nope"))
        .await;
    assert!(
        matches!(result, Err(Error::InvalidCredentials)),
        "{result:?}"
    );
}

// AUTH-004
#[tokio::test]
async fn me_sends_the_auth_headers() {
    let server = MockServer::start().await;
    mount_authenticated(&server, "GET", "/api/v1/me", json_response(200, ME)).await;
    let user = session(&RestClient::new(&server.uri()).unwrap())
        .me()
        .await
        .unwrap();
    assert_eq!(user.id, USER_ID);
    assert_eq!(user.username, "fx9300");
}

// AUTH-004
#[tokio::test]
async fn me_with_an_expired_token_is_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/me",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap()).me().await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// AUTH-005
#[tokio::test]
async fn logout_accepts_an_empty_body() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "POST",
        "/api/v1/logout",
        ResponseTemplate::new(200),
    )
    .await;
    session(&RestClient::new(&server.uri()).unwrap())
        .logout()
        .await
        .unwrap();
}

// AUTH-005
#[tokio::test]
async fn logout_accepts_a_json_body() {
    let server = MockServer::start().await;
    let body = r#"{"status":"success","data":{"message":"You've been logged out!"}}"#;
    mount_authenticated(&server, "POST", "/api/v1/logout", json_response(200, body)).await;
    session(&RestClient::new(&server.uri()).unwrap())
        .logout()
        .await
        .unwrap();
}

// ROOM-001
#[tokio::test]
async fn subscriptions_lists_the_rooms() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "GET",
        "/api/v1/subscriptions.get",
        json_response(200, SUBSCRIPTIONS),
    )
    .await;
    let rooms = session(&RestClient::new(&server.uri()).unwrap())
        .subscriptions()
        .await
        .unwrap();
    let names: Vec<_> = rooms.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["general", "fx9300-chan", "fx9300-priv", "admin"]);
    assert_eq!(rooms[3].room_type, RoomType::Direct);
}

/// The history of `route` for the room `rid`, asking for `count` messages, expected once.
fn history_mock(route: &str, count: &str) -> Mock {
    Mock::given(method("GET"))
        .and(path(route))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(query_param("roomId", "rid"))
        .and(query_param("count", count))
        .respond_with(json_response(200, HISTORY))
        .expect(1)
}

// MSG-001
#[tokio::test]
async fn history_picks_the_endpoint_by_room_type() {
    let server = MockServer::start().await;
    for route in [
        "/api/v1/channels.history",
        "/api/v1/groups.history",
        "/api/v1/im.history",
    ] {
        history_mock(route, "50").mount(&server).await;
    }
    let session = session(&RestClient::new(&server.uri()).unwrap());
    for room_type in [RoomType::Channel, RoomType::Private, RoomType::Direct] {
        let page = session.history("rid", &room_type, None, 50).await.unwrap();
        assert_eq!(page.messages.len(), 4);
        assert!(page.reached_start);
    }
    // Each endpoint was called exactly once (checked when the server drops).
}

// MSG-002
#[tokio::test]
async fn history_pages_back_with_an_inclusive_latest() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/channels.history"))
        .and(query_param("latest", "2026-10-20T14:12:59.379Z"))
        .and(query_param("inclusive", "true"))
        .and(query_param("count", "4"))
        .respond_with(json_response(200, HISTORY))
        .expect(1)
        .mount(&server)
        .await;
    let latest = "2026-10-20T14:12:59.379Z".parse().unwrap();
    let page = session(&RestClient::new(&server.uri()).unwrap())
        .history("rid", &RoomType::Channel, Some(latest), 4)
        .await
        .unwrap();
    assert!(!page.reached_start);
}

// MSG-001
#[tokio::test]
async fn history_without_latest_sends_no_latest() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/channels.history"))
        .respond_with(json_response(200, HISTORY))
        .mount(&server)
        .await;
    session(&RestClient::new(&server.uri()).unwrap())
        .history("rid", &RoomType::Channel, None, 50)
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let query = requests[0].url.query().unwrap_or_default();
    assert!(
        !query.contains("latest") && !query.contains("inclusive"),
        "{query}"
    );
}

// MSG-001
#[tokio::test]
async fn history_of_an_unlisted_room_type_is_an_error_not_a_guess() {
    let server = MockServer::start().await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .history("rid", &RoomType::Other("l".into()), None, 50)
        .await;
    assert!(
        matches!(result, Err(Error::UnsupportedRoomType(ref code)) if code == "l"),
        "{result:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

// MSG-001
#[tokio::test]
async fn history_with_an_expired_token_is_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/channels.history",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .history("rid", &RoomType::Channel, None, 50)
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// MSG-001
#[tokio::test]
async fn history_of_a_missing_room_is_a_server_error_with_the_message() {
    let server = MockServer::start().await;
    let body = r#"{"success":false,"error":"The required \"roomId\" or \"roomName\" param provided does not match any channel [error-room-not-found]","errorType":"error-room-not-found"}"#;
    mount(
        &server,
        "GET",
        "/api/v1/channels.history",
        json_response(400, body),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .history("nope", &RoomType::Channel, None, 50)
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message }) if message.contains("error-room-not-found")),
        "{result:?}"
    );
}

// AUTH-004
#[tokio::test]
async fn redirects_to_another_origin_are_not_followed() {
    let other = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(json_response(200, ME))
        .expect(0)
        .mount(&other)
        .await;
    let server = MockServer::start().await;
    let target = format!("{}/api/v1/me", other.uri());
    mount(
        &server,
        "GET",
        "/api/v1/me",
        ResponseTemplate::new(302).insert_header("location", target.as_str()),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap()).me().await;
    assert!(
        matches!(result, Err(Error::Server { status: 302, .. })),
        "{result:?}"
    );
}

// AUTH-004
#[tokio::test]
async fn same_origin_redirects_keep_the_auth_headers() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/me",
        ResponseTemplate::new(302).insert_header("location", "/moved/me"),
    )
    .await;
    mount_authenticated(&server, "GET", "/moved/me", json_response(200, ME)).await;
    let user = session(&RestClient::new(&server.uri()).unwrap())
        .me()
        .await
        .unwrap();
    assert_eq!(user.id, USER_ID);
}
