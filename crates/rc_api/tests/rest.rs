// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The REST client against a mock HTTP server serving real Rocket.Chat responses.

use std::net::TcpListener;

use rc_api::{Credentials, Error, RestClient, RestSession, RoomType, ServerVersion, ThreadsKind};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const EMOJI_CUSTOM: &str = include_str!("fixtures/emoji_custom.json");
const HISTORY: &str = include_str!("fixtures/history.json");
const INFO: &str = include_str!("fixtures/info.json");
const LOGIN: &str = include_str!("fixtures/login.json");
const LOGIN_UNAUTHORIZED: &str = include_str!("fixtures/login_unauthorized.json");
const ME: &str = include_str!("fixtures/me.json");
const NOT_LOGGED_IN: &str = include_str!("fixtures/not_logged_in.json");
const ROOMS: &str = include_str!("fixtures/rooms.json");
const SUBSCRIPTIONS: &str = include_str!("fixtures/subscriptions.json");
const THREAD_MESSAGES: &str = include_str!("fixtures/thread_messages.json");

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

// ROOM-004
#[tokio::test]
async fn mark_read_posts_the_room_id_with_the_auth_headers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/subscriptions.read"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(body_json(json!({ "rid": "room1" })))
        .respond_with(json_response(200, r#"{"success":true}"#))
        .expect(1)
        .mount(&server)
        .await;
    session(&RestClient::new(&server.uri()).unwrap())
        .mark_read("room1")
        .await
        .unwrap();
}

// ROOM-004
#[tokio::test]
async fn mark_read_with_an_expired_token_is_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/subscriptions.read",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .mark_read("room1")
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// ROOM-004
#[tokio::test]
async fn mark_read_of_a_room_without_subscription_is_a_server_error_with_the_message() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/subscriptions.read",
        json_response(
            400,
            r#"{"success":false,"error":"error-invalid-subscription"}"#,
        ),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .mark_read("nope")
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message }) if message == "error-invalid-subscription"),
        "{result:?}"
    );
}

/// Mounts a POST that only matches with the auth headers and `body`, and expects it once.
async fn mount_post(server: &MockServer, route: &str, body: serde_json::Value, status: u16) {
    let answer = if status == 200 {
        r#"{"success":true}"#
    } else {
        r#"{"success":false,"error":"nope [error-nope]"}"#
    };
    Mock::given(method("POST"))
        .and(path(route))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(body_json(body))
        .respond_with(json_response(status, answer))
        .expect(1)
        .mount(server)
        .await;
}

// ROOM-007
#[tokio::test]
async fn mark_unread_posts_the_room_id() {
    let server = MockServer::start().await;
    mount_post(
        &server,
        "/api/v1/subscriptions.unread",
        json!({ "roomId": "room1" }),
        200,
    )
    .await;
    session(&RestClient::new(&server.uri()).unwrap())
        .mark_unread("room1")
        .await
        .unwrap();
}

// ROOM-007
#[tokio::test]
async fn mark_unread_of_a_room_without_messages_is_a_server_error_with_the_message() {
    let server = MockServer::start().await;
    mount_post(
        &server,
        "/api/v1/subscriptions.unread",
        json!({ "roomId": "room1" }),
        400,
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .mark_unread("room1")
        .await;
    let Err(Error::Server { status, message }) = result else {
        panic!("{result:?}");
    };
    assert_eq!((status, message.as_str()), (400, "nope [error-nope]"));
}

// ROOM-006
#[tokio::test]
async fn set_favorite_says_which_of_the_two_is_wanted() {
    let server = MockServer::start().await;
    mount_post(
        &server,
        "/api/v1/rooms.favorite",
        json!({ "roomId": "room1", "favorite": true }),
        200,
    )
    .await;
    mount_post(
        &server,
        "/api/v1/rooms.favorite",
        json!({ "roomId": "room1", "favorite": false }),
        200,
    )
    .await;
    let session = session(&RestClient::new(&server.uri()).unwrap());
    session.set_favorite("room1", true).await.unwrap();
    session.set_favorite("room1", false).await.unwrap();
}

// ROOM-006
#[tokio::test]
async fn set_favorite_of_a_room_without_subscription_is_a_server_error() {
    let server = MockServer::start().await;
    mount_post(
        &server,
        "/api/v1/rooms.favorite",
        json!({ "roomId": "room1", "favorite": true }),
        400,
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .set_favorite("room1", true)
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, .. })),
        "{result:?}"
    );
}

// ROOM-007
#[tokio::test]
async fn leave_uses_the_call_of_the_room_type() {
    let server = MockServer::start().await;
    mount_post(
        &server,
        "/api/v1/channels.leave",
        json!({ "roomId": "chan" }),
        200,
    )
    .await;
    mount_post(
        &server,
        "/api/v1/groups.leave",
        json!({ "roomId": "group" }),
        200,
    )
    .await;
    let session = session(&RestClient::new(&server.uri()).unwrap());
    session.leave("chan", &RoomType::Channel).await.unwrap();
    session.leave("group", &RoomType::Private).await.unwrap();
}

// ROOM-007
#[tokio::test]
async fn direct_messages_and_other_rooms_cannot_be_left_and_no_request_is_made() {
    let server = MockServer::start().await;
    let session = session(&RestClient::new(&server.uri()).unwrap());
    for kind in [RoomType::Direct, RoomType::Other("l".into())] {
        let result = session.leave("room1", &kind).await;
        assert!(
            matches!(result, Err(Error::UnsupportedRoomType(_))),
            "{result:?}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ROOM-007
#[tokio::test]
async fn leaving_as_the_last_owner_is_a_server_error_with_the_message() {
    let server = MockServer::start().await;
    mount_post(
        &server,
        "/api/v1/groups.leave",
        json!({ "roomId": "group" }),
        400,
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .leave("group", &RoomType::Private)
        .await;
    let Err(Error::Server { status, message }) = result else {
        panic!("{result:?}");
    };
    assert_eq!((status, message.as_str()), (400, "nope [error-nope]"));
}

// RICH-003
#[tokio::test]
async fn custom_emoji_lists_the_emoji_of_the_server() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "GET",
        "/api/v1/emoji-custom.list",
        json_response(200, EMOJI_CUSTOM),
    )
    .await;
    let emoji = session(&RestClient::new(&server.uri()).unwrap())
        .custom_emoji()
        .await
        .unwrap();
    assert_eq!(emoji.len(), 1);
    assert_eq!(emoji[0].name, "probe_emoji");
    assert_eq!(emoji[0].extension, "png");
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
    mount_authenticated(
        &server,
        "GET",
        "/api/v1/rooms.get",
        json_response(200, ROOMS),
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

// ROOM-005
#[tokio::test]
async fn subscriptions_have_the_last_message_time_of_their_room() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/subscriptions.get",
        json_response(200, SUBSCRIPTIONS),
    )
    .await;
    mount(
        &server,
        "GET",
        "/api/v1/rooms.get",
        json_response(200, ROOMS),
    )
    .await;
    let rooms = session(&RestClient::new(&server.uri()).unwrap())
        .subscriptions()
        .await
        .unwrap();
    let times: Vec<_> = rooms.iter().map(|r| r.last_message).collect();
    let at = |text: &str| Some(text.parse::<jiff::Timestamp>().unwrap());
    assert_eq!(
        times,
        [
            at("2026-10-19T22:38:04.040Z"),
            at("2026-10-19T22:38:04.360Z"),
            None,
            at("2026-10-19T22:38:04.420Z"),
        ]
    );
}

// ROOM-005
#[tokio::test]
async fn subscriptions_fail_if_the_rooms_cannot_be_listed() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/subscriptions.get",
        json_response(200, SUBSCRIPTIONS),
    )
    .await;
    mount(
        &server,
        "GET",
        "/api/v1/rooms.get",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .subscriptions()
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
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

// RICH-006
#[tokio::test]
async fn sync_messages_asks_for_a_room_since_a_time_and_reads_both_lists() {
    let server = MockServer::start().await;
    let body = r#"{"result":{
        "updated":[{"_id":"m1","rid":"rid","msg":"edited","ts":"2026-10-20T14:00:00.000Z",
            "u":{"_id":"u","username":"bob"},"editedAt":"2026-10-20T14:05:00.000Z"}],
        "deleted":[{"_id":"m2","_deletedAt":"2026-10-20T23:36:21.795Z"}]},"success":true}"#;
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.syncMessages"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(query_param("roomId", "rid"))
        .and(query_param("lastUpdate", "2026-10-20T14:03:00Z"))
        .respond_with(json_response(200, body))
        .expect(1)
        .mount(&server)
        .await;
    let since = "2026-10-20T14:03:00Z".parse().unwrap();
    let changes = session(&RestClient::new(&server.uri()).unwrap())
        .sync_messages("rid", since)
        .await
        .unwrap();
    assert_eq!(changes.updated.len(), 1);
    assert_eq!(changes.updated[0].text, "edited");
    assert!(changes.updated[0].edited);
    assert_eq!(changes.deleted, ["m2"]);
}

// RICH-006
#[tokio::test]
async fn sync_messages_with_an_expired_token_is_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/chat.syncMessages",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .sync_messages("rid", "2026-10-20T14:03:00Z".parse().unwrap())
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// RICH-006
#[tokio::test]
async fn sync_messages_with_a_body_that_is_not_the_expected_one_is_a_decode_error() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/chat.syncMessages",
        json_response(200, r#"{"success":true}"#),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .sync_messages("rid", "2026-10-20T14:03:00Z".parse().unwrap())
        .await;
    assert!(matches!(result, Err(Error::Decode(_))), "{result:?}");
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

// ACT-004
#[tokio::test]
async fn thread_messages_asks_for_the_newest_replies_at_an_offset() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.getThreadMessages"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(query_param("tmid", "parent"))
        .and(query_param("count", "2"))
        .and(query_param("offset", "0"))
        .and(query_param("sort", r#"{"ts":-1}"#))
        .respond_with(json_response(200, THREAD_MESSAGES))
        .expect(1)
        .mount(&server)
        .await;

    let page = session(&RestClient::new(&server.uri()).unwrap())
        .thread_messages("parent", 2, 0)
        .await
        .unwrap();

    let texts: Vec<_> = page.messages.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(texts, ["reply 3", "reply 2"]);
    assert_eq!(page.total, 3);
    assert!(!page.reached_start);
}

// ACT-004
#[tokio::test]
async fn the_last_page_of_a_thread_reaches_the_start() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.getThreadMessages"))
        .and(query_param("offset", "1"))
        .respond_with(json_response(200, THREAD_MESSAGES))
        .expect(1)
        .mount(&server)
        .await;

    let page = session(&RestClient::new(&server.uri()).unwrap())
        .thread_messages("parent", 2, 1)
        .await
        .unwrap();

    assert!(page.reached_start);
}

// ACT-004
#[tokio::test]
async fn thread_messages_of_a_missing_thread_is_a_server_error_with_the_message() {
    let server = MockServer::start().await;
    let body = r#"{"success":false,"error":"Invalid Message [error-invalid-message]","errorType":"error-invalid-message"}"#;
    mount(
        &server,
        "GET",
        "/api/v1/chat.getThreadMessages",
        json_response(400, body),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .thread_messages("nope", 50, 0)
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message }) if message.contains("error-invalid-message")),
        "{result:?}"
    );
}

// ACT-004
#[tokio::test]
async fn message_asks_for_the_id_and_reads_the_stored_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.getMessage"))
        .and(query_param("msgId", "m1"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .respond_with(json_response(200, SENT))
        .expect(1)
        .mount(&server)
        .await;

    let message = session(&RestClient::new(&server.uri()).unwrap())
        .message("m1")
        .await
        .unwrap();

    assert_eq!(
        (message.id.as_str(), message.text.as_str()),
        ("m1", "hello")
    );
}

// ACT-004
#[tokio::test]
async fn a_missing_message_is_a_server_error_and_an_expired_token_is_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/chat.getMessage",
        json_response(404, r#"{"success":false,"error":"Not Found"}"#),
    )
    .await;
    let client = RestClient::new(&server.uri()).unwrap();
    let result = session(&client).message("nope").await;
    assert!(
        matches!(result, Err(Error::Server { status: 404, .. })),
        "{result:?}"
    );

    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/chat.getMessage",
        json_response(
            401,
            r#"{"status":"error","message":"You must be logged in"}"#,
        ),
    )
    .await;
    let client = RestClient::new(&server.uri()).unwrap();
    let result = session(&client).message("m1").await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

const THREADS_LIST: &str = include_str!("fixtures/threads_list.json");

// ACT-010
#[tokio::test]
async fn threads_list_asks_for_the_room_the_kind_and_the_page() {
    let server = MockServer::start().await;
    // Every thread is what the server lists without a type: it refuses `all`.
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.getThreadsList"))
        .and(query_param("rid", "room"))
        .and(query_param("count", "2"))
        .and(query_param("offset", "0"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .respond_with(json_response(200, THREADS_LIST))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.getThreadsList"))
        .and(query_param("type", "following"))
        .and(query_param("offset", "4"))
        .respond_with(json_response(200, THREADS_LIST))
        .expect(1)
        .mount(&server)
        .await;
    let client = RestClient::new(&server.uri()).unwrap();

    let all = session(&client)
        .threads_list("room", ThreadsKind::All, 2, 0)
        .await
        .unwrap();
    let following = session(&client)
        .threads_list("room", ThreadsKind::Following, 2, 4)
        .await
        .unwrap();

    assert_eq!(all.threads.len(), 2);
    assert!(all.reached_end);
    assert_eq!(following.threads.len(), 2);
}

// ACT-011
#[tokio::test]
async fn threads_list_asks_for_the_unread_threads() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.getThreadsList"))
        .and(query_param("type", "unread"))
        .and(query_param("rid", "room"))
        .respond_with(json_response(200, THREADS_LIST))
        .expect(1)
        .mount(&server)
        .await;

    let page = session(&RestClient::new(&server.uri()).unwrap())
        .threads_list("room", ThreadsKind::Unread, 50, 0)
        .await
        .unwrap();

    assert_eq!(page.threads.len(), 2);
}

// ACT-010
#[tokio::test]
async fn threads_list_of_a_missing_room_is_a_server_error_and_an_expired_token_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/chat.getThreadsList",
        json_response(400, r#"{"success":false,"error":"error-invalid-room"}"#),
    )
    .await;
    let client = RestClient::new(&server.uri()).unwrap();
    let result = session(&client)
        .threads_list("nope", ThreadsKind::All, 50, 0)
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, .. })),
        "{result:?}"
    );

    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/chat.getThreadsList",
        json_response(
            401,
            r#"{"status":"error","message":"You must be logged in"}"#,
        ),
    )
    .await;
    let client = RestClient::new(&server.uri()).unwrap();
    let result = session(&client)
        .threads_list("room", ThreadsKind::All, 50, 0)
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// ACT-004
#[tokio::test]
async fn thread_messages_with_an_expired_token_is_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/chat.getThreadMessages",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .thread_messages("parent", 50, 0)
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// ACT-004
#[tokio::test]
async fn a_thread_reply_posts_the_thread_and_whether_it_is_shown_in_the_channel() {
    let server = MockServer::start().await;
    for shown in [false, true] {
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.sendMessage"))
            .and(header("X-User-Id", USER_ID))
            .and(header("X-Auth-Token", TOKEN))
            .and(body_json(json!({ "message": {
                "_id": "m1", "rid": "rid", "msg": "hello", "tmid": "parent", "tshow": shown,
            } })))
            .respond_with(json_response(200, SENT))
            .expect(1)
            .mount(&server)
            .await;
    }
    let session = session(&RestClient::new(&server.uri()).unwrap());
    for shown in [false, true] {
        let message = session
            .send_thread_reply("rid", "m1", "hello", "parent", shown)
            .await
            .unwrap();
        assert_eq!(message.id, "m1");
    }
}

// ACT-004
#[tokio::test]
async fn a_refused_duplicate_thread_reply_is_a_success_when_the_stored_message_is_ours() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.sendMessage",
        json_response(400, DUPLICATE),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.getMessage"))
        .and(query_param("msgId", "m1"))
        .respond_with(json_response(200, SENT))
        .expect(1)
        .mount(&server)
        .await;

    let message = session(&RestClient::new(&server.uri()).unwrap())
        .send_thread_reply("rid", "m1", "hello again", "parent", false)
        .await
        .unwrap();

    assert_eq!(message.text, "hello");
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

const SENT: &str = r#"{"message":{"_id":"m1","rid":"rid","msg":"hello","ts":"2026-10-20T17:00:57.427Z","u":{"_id":"u8paxsGQXMH9vSnSc","username":"fx9300","name":"Fixture User"},"_updatedAt":"2026-10-20T17:00:57.435Z","urls":[],"mentions":[],"channels":[]},"success":true}"#;
const DUPLICATE: &str =
    r#"{"success":false,"error":"Cannot read properties of undefined (reading 'starred')"}"#;

// SEND-001
#[tokio::test]
async fn send_message_posts_the_id_the_room_and_the_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat.sendMessage"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(body_json(
            json!({ "message": { "_id": "m1", "rid": "rid", "msg": "hello" } }),
        ))
        .respond_with(json_response(200, SENT))
        .expect(1)
        .mount(&server)
        .await;

    let message = session(&RestClient::new(&server.uri()).unwrap())
        .send_message("rid", "m1", "hello")
        .await
        .unwrap();

    assert_eq!(
        (message.id.as_str(), message.text.as_str()),
        ("m1", "hello")
    );
    assert_eq!(message.ts.to_string(), "2026-10-20T17:00:57.427Z");
}

// SEND-002
#[tokio::test]
async fn a_refused_duplicate_is_a_success_when_the_stored_message_is_ours() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.sendMessage",
        json_response(400, DUPLICATE),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/chat.getMessage"))
        .and(query_param("msgId", "m1"))
        .respond_with(json_response(200, SENT))
        .expect(1)
        .mount(&server)
        .await;

    let message = session(&RestClient::new(&server.uri()).unwrap())
        .send_message("rid", "m1", "hello again")
        .await
        .unwrap();

    assert_eq!(message.text, "hello");
}

// SEND-002
#[tokio::test]
async fn a_refusal_stays_an_error_when_the_stored_message_is_not_ours() {
    let elsewhere = SENT.replace(r#""rid":"rid""#, r#""rid":"other""#);
    let someone_elses = SENT.replace(r#""_id":"u8paxsGQXMH9vSnSc""#, r#""_id":"u2""#);
    for stored in [elsewhere, someone_elses] {
        let server = MockServer::start().await;
        mount(
            &server,
            "POST",
            "/api/v1/chat.sendMessage",
            json_response(400, DUPLICATE),
        )
        .await;
        mount(
            &server,
            "GET",
            "/api/v1/chat.getMessage",
            json_response(200, &stored),
        )
        .await;
        let result = session(&RestClient::new(&server.uri()).unwrap())
            .send_message("rid", "m1", "hello")
            .await;
        assert!(
            matches!(result, Err(Error::Server { status: 400, .. })),
            "{result:?}"
        );
    }
}

// SEND-002
#[tokio::test]
async fn a_refusal_stays_an_error_when_nothing_is_stored_under_the_id() {
    let server = MockServer::start().await;
    let no_room = r#"{"success":false,"error":"Room not found [error-invalid-room]"}"#;
    mount(
        &server,
        "POST",
        "/api/v1/chat.sendMessage",
        json_response(400, no_room),
    )
    .await;
    mount(
        &server,
        "GET",
        "/api/v1/chat.getMessage",
        json_response(400, DUPLICATE),
    )
    .await;

    let result = session(&RestClient::new(&server.uri()).unwrap())
        .send_message("rid", "m1", "hello")
        .await;

    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message }) if message.contains("error-invalid-room")),
        "{result:?}"
    );
}

// SEND-002
#[tokio::test]
async fn send_message_does_not_look_for_the_message_after_other_failures() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.sendMessage",
        json_response(500, "{}"),
    )
    .await;

    let result = session(&RestClient::new(&server.uri()).unwrap())
        .send_message("rid", "m1", "hello")
        .await;

    assert!(
        matches!(result, Err(Error::Server { status: 500, .. })),
        "{result:?}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// SEND-002
#[tokio::test]
async fn send_message_with_an_expired_token_is_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.sendMessage",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;

    let result = session(&RestClient::new(&server.uri()).unwrap())
        .send_message("rid", "m1", "hello")
        .await;

    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

const EDITED: &str = r#"{"message":{"_id":"m1","rid":"rid","msg":"hello again","ts":"2026-10-20T17:00:57.427Z","u":{"_id":"u8paxsGQXMH9vSnSc","username":"fx9300","name":"Fixture User"},"_updatedAt":"2026-10-20T17:05:00.000Z","editedAt":"2026-10-20T17:05:00.000Z","editedBy":{"_id":"u8paxsGQXMH9vSnSc","username":"fx9300"},"urls":[],"mentions":[],"channels":[]},"success":true}"#;
const SETTINGS: &str = r#"{"settings":[{"_id":"Message_AllowDeleting","value":true},{"_id":"Message_AllowDeleting_BlockDeleteInMinutes","value":30},{"_id":"Message_AllowEditing","value":false},{"_id":"Message_AllowEditing_BlockEditInMinutes","value":0}],"success":true}"#;

// ACT-001
#[tokio::test]
async fn edit_message_posts_the_ids_and_the_text_and_returns_the_edited_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat.update"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(body_json(
            json!({ "roomId": "rid", "msgId": "m1", "text": "hello again" }),
        ))
        .respond_with(json_response(200, EDITED))
        .expect(1)
        .mount(&server)
        .await;

    let message = session(&RestClient::new(&server.uri()).unwrap())
        .edit_message("rid", "m1", "hello again")
        .await
        .unwrap();

    assert_eq!(message.text, "hello again");
    assert!(message.edited);
}

// ACT-001
#[tokio::test]
async fn a_refused_edit_is_a_server_error_with_the_message() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.update",
        json_response(
            400,
            r#"{"success":false,"error":"Action Not allowed [error-action-not-allowed]"}"#,
        ),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .edit_message("rid", "m1", "x")
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message }) if message.contains("error-action-not-allowed")),
        "{result:?}"
    );
}

// ACT-001
#[tokio::test]
async fn edit_message_with_an_expired_token_is_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.update",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .edit_message("rid", "m1", "x")
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// ACT-002
#[tokio::test]
async fn delete_message_posts_the_ids() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat.delete"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(body_json(json!({ "roomId": "rid", "msgId": "m1" })))
        .respond_with(json_response(
            200,
            r#"{"_id":"m1","ts":1760000000000,"message":{"_id":"m1"},"success":true}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    session(&RestClient::new(&server.uri()).unwrap())
        .delete_message("rid", "m1")
        .await
        .unwrap();
}

// ACT-002
#[tokio::test]
async fn a_refused_deletion_is_a_server_error_and_an_expired_token_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.delete",
        json_response(
            400,
            r#"{"success":false,"error":"Not allowed [error-action-not-allowed]"}"#,
        ),
    )
    .await;
    let client = RestClient::new(&server.uri()).unwrap();
    let result = session(&client).delete_message("rid", "m1").await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, .. })),
        "{result:?}"
    );

    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.delete",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .delete_message("rid", "m1")
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// ACT-003
#[tokio::test]
async fn react_posts_the_id_the_emoji_and_the_wish() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat.react"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .and(body_json(
            json!({ "messageId": "m1", "emoji": ":smile:", "shouldReact": true }),
        ))
        .respond_with(json_response(200, r#"{"success":true}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat.react"))
        .and(body_json(
            json!({ "messageId": "m1", "emoji": ":smile:", "shouldReact": false }),
        ))
        .respond_with(json_response(200, r#"{"success":true}"#))
        .expect(1)
        .mount(&server)
        .await;

    let client = RestClient::new(&server.uri()).unwrap();
    session(&client).react("m1", ":smile:", true).await.unwrap();
    session(&client)
        .react("m1", ":smile:", false)
        .await
        .unwrap();
}

// ACT-003
#[tokio::test]
async fn a_refused_reaction_is_a_server_error_and_an_expired_token_unauthorized() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.react",
        json_response(
            400,
            r#"{"success":false,"error":"Invalid emoji provided. [error-not-allowed]"}"#,
        ),
    )
    .await;
    let client = RestClient::new(&server.uri()).unwrap();
    let result = session(&client).react("m1", ":nope:", true).await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message }) if message.contains("Invalid emoji")),
        "{result:?}"
    );

    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/v1/chat.react",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .react("m1", ":smile:", true)
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// ACT-001, ACT-002
#[tokio::test]
async fn message_settings_asks_for_the_four_settings() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/settings.public"))
        .and(query_param(
            "_id",
            "Message_AllowEditing,Message_AllowEditing_BlockEditInMinutes,Message_AllowDeleting,\
             Message_AllowDeleting_BlockDeleteInMinutes",
        ))
        .respond_with(json_response(200, SETTINGS))
        .expect(1)
        .mount(&server)
        .await;

    let settings = session(&RestClient::new(&server.uri()).unwrap())
        .message_settings()
        .await
        .unwrap();

    assert!(!settings.editing_allowed);
    assert!(settings.deleting_allowed);
    assert_eq!((settings.edit_minutes, settings.delete_minutes), (0, 30));
}

// ACT-005
#[tokio::test]
async fn site_url_asks_for_the_setting_without_the_login_and_trims_it() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/settings.public"))
        .and(query_param("_id", "Site_Url"))
        .respond_with(json_response(
            200,
            r#"{"settings":[{"_id":"Site_Url","value":"http://chat.test:3000/"}],"success":true}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let url = RestClient::new(&server.uri()).unwrap().site_url().await;

    assert_eq!(url.unwrap().as_deref(), Some("http://chat.test:3000"));
    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("X-Auth-Token").is_none());
}

// ACT-005
#[tokio::test]
async fn a_server_without_a_site_url_has_none_and_an_error_status_is_an_error() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/settings.public",
        json_response(200, r#"{"settings":[],"success":true}"#),
    )
    .await;
    let client = RestClient::new(&server.uri()).unwrap();
    assert_eq!(client.site_url().await.unwrap(), None);

    let failing = MockServer::start().await;
    mount(
        &failing,
        "GET",
        "/api/v1/settings.public",
        json_response(500, r#"{"success":false}"#),
    )
    .await;
    let result = RestClient::new(&failing.uri()).unwrap().site_url().await;
    assert!(matches!(result, Err(Error::Server { .. })), "{result:?}");
}

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200"></svg>"#;

fn svg_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(SVG, "Image/SVG+XML; charset=utf-8")
}

// RICH-004
#[tokio::test]
async fn a_user_avatar_is_downloaded_with_the_login_headers() {
    let server = MockServer::start().await;
    mount_authenticated(&server, "GET", "/avatar/alice", svg_response()).await;
    let avatar = session(&RestClient::new(&server.uri()).unwrap())
        .user_avatar("alice")
        .await
        .unwrap();
    assert_eq!(avatar.bytes, SVG.as_bytes());
    assert_eq!(avatar.content_type.as_deref(), Some("image/svg+xml"));
}

// RICH-004
#[tokio::test]
async fn the_avatar_routes_get_the_login_as_cookies_too() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/avatar/alice"))
        .and(header(
            "Cookie",
            format!("rc_uid={USER_ID}; rc_token={TOKEN}").as_str(),
        ))
        .respond_with(svg_response())
        .expect(1)
        .mount(&server)
        .await;
    session(&RestClient::new(&server.uri()).unwrap())
        .user_avatar("alice")
        .await
        .unwrap();
}

// RICH-004
#[tokio::test]
async fn the_api_routes_get_no_cookie() {
    let server = MockServer::start().await;
    mount_authenticated(&server, "GET", "/api/v1/me", ResponseTemplate::new(404)).await;
    let _ = session(&RestClient::new(&server.uri()).unwrap()).me().await;
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].headers.contains_key("cookie"));
}

// RICH-004
#[tokio::test]
async fn a_room_avatar_is_downloaded_from_the_room_path() {
    let server = MockServer::start().await;
    mount_authenticated(&server, "GET", "/avatar/room/rid1", svg_response()).await;
    let avatar = session(&RestClient::new(&server.uri()).unwrap())
        .room_avatar("rid1")
        .await
        .unwrap();
    assert_eq!(avatar.bytes, SVG.as_bytes());
}

// RICH-004
#[tokio::test]
async fn avatars_keep_the_path_prefix_of_the_server() {
    let server = MockServer::start().await;
    mount_authenticated(&server, "GET", "/chat/avatar/alice", svg_response()).await;
    let client = RestClient::new(&format!("{}/chat", server.uri())).unwrap();
    session(&client).user_avatar("alice").await.unwrap();
}

// RICH-004
#[tokio::test]
async fn a_name_is_one_path_segment_whatever_it_contains() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(svg_response())
        .mount(&server)
        .await;
    let session = session(&RestClient::new(&server.uri()).unwrap());
    session.user_avatar("../../api/v1/me").await.unwrap();
    session.user_avatar("a b?c#d").await.unwrap();
    let requests = server.received_requests().await.unwrap();
    let paths: Vec<_> = requests.iter().map(|r| r.url.path()).collect();
    assert_eq!(
        paths,
        ["/avatar/..%2F..%2Fapi%2Fv1%2Fme", "/avatar/a%20b%3Fc%23d"]
    );
    assert!(requests.iter().all(|r| r.url.query().is_none()));
}

// RICH-004
#[tokio::test]
async fn a_missing_avatar_is_a_server_error() {
    let server = MockServer::start().await;
    mount(&server, "GET", "/avatar/alice", ResponseTemplate::new(404)).await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .user_avatar("alice")
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 404, .. })),
        "{result:?}"
    );
}

// RICH-004
#[tokio::test]
async fn an_avatar_redirected_to_another_origin_is_not_followed() {
    let other = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(svg_response())
        .expect(0)
        .mount(&other)
        .await;
    let server = MockServer::start().await;
    let target = format!("{}/elsewhere.svg", other.uri());
    mount(
        &server,
        "GET",
        "/avatar/alice",
        ResponseTemplate::new(307).insert_header("location", target.as_str()),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .user_avatar("alice")
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 307, .. })),
        "{result:?}"
    );
}

// RICH-004
#[tokio::test]
async fn an_avatar_redirected_within_the_server_is_followed() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/avatar/alice",
        ResponseTemplate::new(307).insert_header("location", "/avatar/alice.svg"),
    )
    .await;
    mount_authenticated(&server, "GET", "/avatar/alice.svg", svg_response()).await;
    let avatar = session(&RestClient::new(&server.uri()).unwrap())
        .user_avatar("alice")
        .await
        .unwrap();
    assert_eq!(avatar.bytes, SVG.as_bytes());
}

// RICH-004
#[tokio::test]
async fn an_avatar_without_a_content_type_has_none() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/avatar/alice",
        ResponseTemplate::new(200).set_body_bytes(vec![1, 2, 3]),
    )
    .await;
    let avatar = session(&RestClient::new(&server.uri()).unwrap())
        .user_avatar("alice")
        .await
        .unwrap();
    assert_eq!(avatar.bytes, [1, 2, 3]);
    assert_eq!(avatar.content_type, None);
}

// RICH-004
#[tokio::test]
async fn subscriptions_carry_the_avatar_etag_of_the_rooms_that_have_one() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "GET",
        "/api/v1/subscriptions.get",
        json_response(200, SUBSCRIPTIONS),
    )
    .await;
    let subscriptions: serde_json::Value = serde_json::from_str(SUBSCRIPTIONS).unwrap();
    let ids: Vec<_> = subscriptions["update"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["rid"].as_str().unwrap().to_owned())
        .collect();
    let rooms = json!({ "update": [{ "_id": ids[0], "avatarETag": "etag-1" }, { "_id": ids[1] }], "remove": [] });
    mount_authenticated(
        &server,
        "GET",
        "/api/v1/rooms.get",
        json_response(200, &rooms.to_string()),
    )
    .await;

    let listed = session(&RestClient::new(&server.uri()).unwrap())
        .subscriptions()
        .await
        .unwrap();

    let etag_of = |id: &str| {
        listed
            .iter()
            .find(|s| s.room_id == id)
            .unwrap()
            .avatar_etag
            .clone()
    };
    assert_eq!(etag_of(&ids[0]).as_deref(), Some("etag-1"));
    assert_eq!(etag_of(&ids[1]), None);
}

// ROOM-009
#[tokio::test]
async fn subscriptions_carry_the_topic_of_the_rooms_that_have_one() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "GET",
        "/api/v1/subscriptions.get",
        json_response(200, SUBSCRIPTIONS),
    )
    .await;
    let subscriptions: serde_json::Value = serde_json::from_str(SUBSCRIPTIONS).unwrap();
    let ids: Vec<_> = subscriptions["update"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["rid"].as_str().unwrap().to_owned())
        .collect();
    let rooms = json!({ "update": [{ "_id": ids[0], "topic": "Plans" }, { "_id": ids[1], "topic": "" }], "remove": [] });
    mount_authenticated(
        &server,
        "GET",
        "/api/v1/rooms.get",
        json_response(200, &rooms.to_string()),
    )
    .await;

    let listed = session(&RestClient::new(&server.uri()).unwrap())
        .subscriptions()
        .await
        .unwrap();

    let topic_of = |id: &str| {
        listed
            .iter()
            .find(|s| s.room_id == id)
            .unwrap()
            .topic
            .clone()
    };
    assert_eq!(topic_of(&ids[0]), "Plans");
    assert_eq!(topic_of(&ids[1]), "");
    assert_eq!(topic_of(&ids[2]), "");
}

// RICH-004
#[tokio::test]
async fn a_user_has_an_avatar_of_their_own_when_users_info_has_an_etag() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users.info"))
        .and(query_param("username", "a b"))
        .and(header("X-User-Id", USER_ID))
        .and(header("X-Auth-Token", TOKEN))
        .respond_with(json_response(
            200,
            r#"{"user":{"_id":"u","avatarETag":"e1"},"success":true}"#,
        ))
        .mount(&server)
        .await;
    mount(
        &server,
        "GET",
        "/api/v1/users.info",
        json_response(200, r#"{"user":{"_id":"u"},"success":true}"#),
    )
    .await;
    let session = session(&RestClient::new(&server.uri()).unwrap());

    assert_eq!(
        session.user_avatar_etag("a b").await.unwrap().as_deref(),
        Some("e1")
    );
    assert_eq!(session.user_avatar_etag("other").await.unwrap(), None);
}

// RICH-004
#[tokio::test]
async fn a_room_has_an_avatar_of_its_own_when_rooms_info_has_an_etag() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/rooms.info"))
        .and(query_param("roomId", "r1"))
        .respond_with(json_response(
            200,
            r#"{"room":{"_id":"r1","avatarETag":"e2"},"success":true}"#,
        ))
        .mount(&server)
        .await;
    mount(
        &server,
        "GET",
        "/api/v1/rooms.info",
        json_response(200, r#"{"room":{"_id":"r2"},"success":true}"#),
    )
    .await;
    let session = session(&RestClient::new(&server.uri()).unwrap());

    assert_eq!(
        session.room_avatar_etag("r1").await.unwrap().as_deref(),
        Some("e2")
    );
    assert_eq!(session.room_avatar_etag("r2").await.unwrap(), None);
}

// RICH-004
#[tokio::test]
async fn avatar_etags_need_a_login_and_an_existing_user() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v1/users.info",
        json_response(401, NOT_LOGGED_IN),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .user_avatar_etag("x")
        .await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

fn png_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(PNG, "image/png")
}

// RICH-005
#[tokio::test]
async fn an_uploaded_file_is_downloaded_with_the_login_headers() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "GET",
        "/file-upload/abc123/big.png",
        png_response(),
    )
    .await;
    let file = session(&RestClient::new(&server.uri()).unwrap())
        .file("/file-upload/abc123/big.png")
        .await
        .unwrap();
    assert_eq!(file.bytes, PNG);
    assert_eq!(file.content_type.as_deref(), Some("image/png"));
}

// RICH-007
#[tokio::test]
async fn the_image_of_a_custom_emoji_is_downloaded_with_the_login_headers() {
    let server = MockServer::start().await;
    mount_authenticated(&server, "GET", "/emoji-custom/party.png", png_response()).await;
    let image = session(&RestClient::new(&server.uri()).unwrap())
        .emoji_image("party", "png")
        .await
        .unwrap();
    assert_eq!(image.bytes, PNG);
    assert_eq!(image.content_type.as_deref(), Some("image/png"));
}

// RICH-007
#[tokio::test]
async fn the_name_of_a_custom_emoji_cannot_leave_its_place_in_the_path() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "GET",
        "/emoji-custom/..%2Fapi%2Fv1%2Fme.png",
        png_response(),
    )
    .await;
    let image = session(&RestClient::new(&server.uri()).unwrap())
        .emoji_image("../api/v1/me", "png")
        .await
        .unwrap();
    assert_eq!(image.bytes, PNG);
}

// RICH-005
#[tokio::test]
async fn an_encoded_file_name_is_not_encoded_twice() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "GET",
        "/file-upload/abc123/my%20file%C3%A9.png",
        png_response(),
    )
    .await;
    let file = session(&RestClient::new(&server.uri()).unwrap())
        .file("/file-upload/abc123/my%20file%C3%A9.png")
        .await
        .unwrap();
    assert_eq!(file.bytes, PNG);
}

// RICH-005
#[tokio::test]
async fn files_keep_the_path_prefix_of_the_server() {
    let server = MockServer::start().await;
    mount_authenticated(
        &server,
        "GET",
        "/team/file-upload/abc/a.png",
        png_response(),
    )
    .await;
    let client = RestClient::new(&format!("{}/team/", server.uri())).unwrap();
    session(&client)
        .file("/file-upload/abc/a.png")
        .await
        .unwrap();
}

// RICH-005
#[tokio::test]
async fn paths_outside_file_upload_are_refused_without_a_request() {
    let server = MockServer::start().await;
    let session = session(&RestClient::new(&server.uri()).unwrap());
    for path in [
        "https://evil.example/file-upload/abc/a.png",
        "/api/v1/me",
        "/file-upload/../api/v1/me",
        "/file-upload/%2E%2E/api/v1/me",
    ] {
        let result = session.file(path).await;
        assert!(matches!(result, Err(Error::InvalidUrl(_))), "{path}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// RICH-005
#[tokio::test]
async fn a_forbidden_file_is_a_server_error() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/file-upload/abc/a.png",
        ResponseTemplate::new(403),
    )
    .await;
    let result = session(&RestClient::new(&server.uri()).unwrap())
        .file("/file-upload/abc/a.png")
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 403, .. })),
        "{result:?}"
    );
}
