// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The GitHub pull requests page: the tile in the rail, the connect page and the lists. There is no
//! Rocket.Chat server in these tests, and no real GitHub either: the app talks to `rc_test_github`
//! (`RC_GITHUB_API`) and runs a script of the test in place of `gh` (`RC_GH_PROGRAM`), so nothing
//! reaches the user's token or the real API.

// The helpers fail the test when a step fails.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use rc_github::JobState;
use rc_gui_tests::paths::github as gh;
use rc_gui_tests::{App, Gui, Modifiers, MouseButton};
use rc_test_github::{GithubStub, Job, PullRequest};

/// The app and the stub answer asynchronously: tests wait this long before they fail.
const TIMEOUT: Duration = Duration::from_secs(15);

/// The token the fake `gh` prints, and the one the stub accepts.
const TOKEN: &str = "ghp_fake_for_tests";

/// A directory with the fake `gh` in it, removed with this.
struct FakeGh {
    dir: PathBuf,
}

impl FakeGh {
    /// A `gh` that prints `token` whatever it is asked.
    fn printing(token: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rc_gui_tests-gh-{}-{}",
            std::process::id(),
            Timestamp::now().as_nanosecond()
        ));
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("gh");
        fs::write(&script, format!("#!/bin/sh\necho {token}\n")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn program(&self) -> String {
        self.dir.join("gh").to_str().unwrap().to_owned()
    }
}

impl Drop for FakeGh {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The app, pointed at the stub and the fake `gh`, with the stub and the script kept alive.
struct Setup {
    app: App,
    stub: GithubStub,
    _gh: FakeGh,
}

impl Setup {
    fn gui(&self) -> &Gui {
        self.app.gui()
    }
}

/// The user's pull requests on the stub: a failing one, a passing one and a draft, and another
/// author's in a watched repository.
fn stub_with_pull_requests() -> GithubStub {
    let stub = GithubStub::start();
    stub.set_viewer("me");
    stub.require_token(Some(TOKEN));
    stub.set_watched(&["acme/web"]);
    let ago = |minutes: i64| Timestamp::now() - SignedDuration::from_mins(minutes);
    stub.add_pull_request(
        PullRequest::new("acme/web", "Fix the flaky test")
            .author("me")
            .head("fix-flaky")
            .updated_at(ago(1))
            .job(Job::new("build").state(JobState::Failed))
            .job(Job::new("lint").state(JobState::Passed)),
    );
    stub.add_pull_request(
        PullRequest::new("acme/web", "Add dark mode")
            .author("me")
            .updated_at(ago(2))
            .job(Job::new("build").state(JobState::Passed))
            .job(Job::new("lint").state(JobState::Passed)),
    );
    stub.add_pull_request(
        PullRequest::new("acme/web", "WIP: cache the lists")
            .author("me")
            .draft()
            .updated_at(ago(3)),
    );
    stub.add_pull_request(
        PullRequest::new("acme/web", "Rework the router")
            .author("bob")
            .updated_at(ago(4))
            .job(Job::new("build").state(JobState::Running)),
    );
    stub
}

fn launch(stub: GithubStub) -> Setup {
    let gh = FakeGh::printing(TOKEN);
    let app = App::launch_with(&[
        ("TZ", "UTC"),
        ("RC_GITHUB_API", stub.url()),
        ("RC_GH_PROGRAM", &gh.program()),
    ])
    .unwrap();
    Setup { app, stub, _gh: gh }
}

/// Opens the GitHub page from its tile, once the rail is there.
fn open_page(gui: &Gui) {
    gui.wait_until_visible(gh::TILE, TIMEOUT).unwrap();
    gui.mouse_click(gh::TILE).unwrap();
    gui.wait_until(&format!("{} checked", gh::TILE), TIMEOUT, || {
        gui.checked(gh::TILE)
    })
    .unwrap();
}

/// Connects with a pasted token and waits for the list.
fn connect_with_token(gui: &Gui, token: &str) {
    gui.wait_until_visible(gh::CONNECT_TOKEN_FIELD, TIMEOUT)
        .unwrap();
    gui.set_text(gh::CONNECT_TOKEN_FIELD, token).unwrap();
    gui.mouse_click(gh::CONNECT_TOKEN).unwrap();
}

fn wait_for_text(gui: &Gui, path: &str, expected: &str) {
    gui.wait_until_text(path, expected, TIMEOUT).unwrap();
}

/// Waits until the text of `path` contains `part`.
fn wait_for_text_containing(gui: &Gui, path: &str, part: &str) {
    gui.wait_until(&format!("{path} contains {part:?}"), TIMEOUT, || {
        Ok(gui.exists_and_visible(path)? && gui.text(path)?.contains(part))
    })
    .unwrap();
}

// GH-009
#[test]
#[ignore = "needs the app: run through ctest"]
fn the_tile_is_always_there_and_the_page_asks_to_connect() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();

    gui.wait_until_visible(gh::TILE, TIMEOUT).unwrap();
    assert!(!gui.checked(gh::TILE).unwrap());
    assert!(!gui.exists_and_visible(gh::TILE_CURRENT).unwrap());
    assert!(!gui.exists_and_visible(gh::TILE_FAILING).unwrap());

    open_page(gui);
    // The tall pip, and the connect page with no list.
    assert!(gui.exists_and_visible(gh::TILE_CURRENT).unwrap());
    gui.wait_until_visible(gh::CONNECT_TITLE, TIMEOUT).unwrap();
    assert_eq!(gui.text(gh::CONNECT_TITLE).unwrap(), "Connect GitHub");
    assert!(!gui.exists_and_visible(gh::LIST).unwrap());
    assert!(!gui.exists_and_visible(gh::CONNECT_ERROR).unwrap());

    // Nothing to disconnect from yet: the right-click opens no menu.
    gui.mouse_click_with(gh::TILE, MouseButton::Right, Modifiers::NONE)
        .unwrap();
    gui.wait(Duration::from_millis(300)).unwrap();
    assert!(!gui.exists_and_visible(gh::DISCONNECT_ACTION).unwrap());
}

// GH-009
#[test]
#[ignore = "needs the app: run through ctest"]
fn a_pasted_token_connects_and_the_list_shows() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);

    connect_with_token(gui, TOKEN);
    gui.wait_until_visible(gh::LIST, TIMEOUT).unwrap();
    assert!(!gui.exists_and_visible(gh::CONNECT_TITLE).unwrap());
    assert_eq!(gui.text(gh::TITLE).unwrap(), "Pull requests");
    wait_for_text_containing(gui, gh::STATUS, "me, updated");
    // The stub accepts only the token the test pasted, so the list is proof that it was sent.
    assert!(!setup.stub.requests().is_empty());
}

// GH-009
#[test]
#[ignore = "needs the app: run through ctest"]
fn the_login_of_the_gh_command_connects() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);

    gui.wait_until_visible(gh::CONNECT_GH, TIMEOUT).unwrap();
    gui.mouse_click(gh::CONNECT_GH).unwrap();
    gui.wait_until_visible(gh::LIST, TIMEOUT).unwrap();
    wait_for_text_containing(gui, gh::STATUS, "me, updated");
    wait_for_text_containing(
        gui,
        &gh::pr_row(0, gh::pr_row_part::TITLE),
        "Fix the flaky test",
    );
}

// GH-009
#[test]
#[ignore = "needs the app: run through ctest"]
fn a_refused_token_says_why_and_the_page_stays() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);

    connect_with_token(gui, "ghp_not_the_one");
    gui.wait_until_visible(gh::CONNECT_ERROR, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(gh::CONNECT_ERROR).unwrap(),
        "GitHub refused the token"
    );
    assert!(!gui.exists_and_visible(gh::LIST).unwrap());
    // Still the connect page, and the tile is still grey.
    assert!(gui.exists_and_visible(gh::CONNECT_TOKEN).unwrap());
    assert!(!gui.exists_and_visible(gh::TILE_FAILING).unwrap());
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn the_tile_counts_the_failing_pull_requests_once_connected() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);
    connect_with_token(gui, TOKEN);

    gui.wait_until_visible(gh::TILE_FAILING, TIMEOUT).unwrap();
    assert_eq!(gui.text(gh::TILE_FAILING).unwrap(), "1");

    // Fixed on GitHub: the count goes when the refresh finds it.
    setup
        .stub
        .set_job_state("acme/web", 1, "build", JobState::Passed);
    gui.mouse_click(gh::REFRESH_BUTTON).unwrap();
    gui.wait_until_hidden(gh::TILE_FAILING, TIMEOUT).unwrap();
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn mine_lists_the_pull_requests_with_their_state_and_bar() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);
    connect_with_token(gui, TOKEN);

    // Newest first: the failing one, the passing one, then the drafts' heading.
    gui.wait_until_visible(&gh::pr_row(0, ""), TIMEOUT).unwrap();
    wait_for_text_containing(
        gui,
        &gh::pr_row(0, gh::pr_row_part::TITLE),
        "Fix the flaky test",
    );
    assert!(
        gui.text(&gh::pr_row(0, gh::pr_row_part::TITLE))
            .unwrap()
            .contains("#1")
    );
    assert_eq!(
        gui.text(&gh::pr_row(0, gh::pr_row_part::GLYPH)).unwrap(),
        "failed"
    );
    assert_eq!(
        gui.text(&gh::pr_row(0, gh::pr_row_part::REPO)).unwrap(),
        "acme/web"
    );
    // One segment per job, failed first.
    assert_eq!(gui.text(&gh::pr_segment(0, 0)).unwrap(), "failed");
    assert_eq!(gui.text(&gh::pr_segment(0, 1)).unwrap(), "passed");
    assert!(!gui.exists_and_visible(&gh::pr_segment(0, 2)).unwrap());

    wait_for_text_containing(gui, &gh::pr_row(1, gh::pr_row_part::TITLE), "Add dark mode");
    assert_eq!(
        gui.text(&gh::pr_row(1, gh::pr_row_part::GLYPH)).unwrap(),
        "passed"
    );

    // The counts are of the pull requests that are not drafts; the drafts are folded.
    assert_eq!(gui.text(gh::TAB_MINE_COUNT).unwrap(), "2");
    assert_eq!(gui.text(gh::TAB_WATCHED_COUNT).unwrap(), "1");
    assert!(gui.checked(gh::TAB_MINE).unwrap());
    assert_eq!(gui.text(gh::DRAFTS_TITLE).unwrap(), "Drafts 1");
    assert_eq!(gui.count(gh::LIST).unwrap(), 3);
    assert!(!gui.exists_and_visible(gh::EMPTY).unwrap());
}

// GH-002
#[test]
#[ignore = "needs the app: run through ctest"]
fn watched_lists_the_pull_requests_of_others_with_their_author() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);
    connect_with_token(gui, TOKEN);
    gui.wait_until_visible(&gh::pr_row(0, ""), TIMEOUT).unwrap();

    gui.mouse_click(gh::TAB_WATCHED).unwrap();
    wait_for_text(gui, &gh::pr_row(0, gh::pr_row_part::REPO), "acme/web, bob");
    wait_for_text_containing(
        gui,
        &gh::pr_row(0, gh::pr_row_part::TITLE),
        "Rework the router",
    );
    assert_eq!(
        gui.text(&gh::pr_row(0, gh::pr_row_part::GLYPH)).unwrap(),
        "running"
    );
    assert!(gui.checked(gh::TAB_WATCHED).unwrap());
    assert!(!gui.checked(gh::TAB_MINE).unwrap());
    assert_eq!(gui.count(gh::LIST).unwrap(), 1);

    // Back to Mine.
    gui.mouse_click(gh::TAB_MINE).unwrap();
    wait_for_text(gui, &gh::pr_row(0, gh::pr_row_part::REPO), "acme/web");
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn the_drafts_fold_opens_and_closes() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);
    connect_with_token(gui, TOKEN);
    gui.wait_until_visible(gh::DRAFTS, TIMEOUT).unwrap();
    assert!(!gui.checked(gh::DRAFTS).unwrap());
    // The heading is the third row; no draft under it.
    assert!(!gui.exists_and_visible(&gh::pr_row(3, "")).unwrap());

    gui.mouse_click(gh::DRAFTS).unwrap();
    gui.wait_until_visible(&gh::pr_row(3, ""), TIMEOUT).unwrap();
    assert!(gui.checked(gh::DRAFTS).unwrap());
    wait_for_text_containing(
        gui,
        &gh::pr_row(3, gh::pr_row_part::TITLE),
        "WIP: cache the lists",
    );
    assert_eq!(
        gui.text(&gh::pr_row(3, gh::pr_row_part::GLYPH)).unwrap(),
        "draft"
    );
    // A draft has no jobs and so no bar.
    assert!(
        !gui.exists_and_visible(&gh::pr_row(3, gh::pr_row_part::BAR))
            .unwrap()
    );

    gui.mouse_click(gh::DRAFTS).unwrap();
    gui.wait_until_hidden(&gh::pr_row(3, ""), TIMEOUT).unwrap();
    assert!(!gui.checked(gh::DRAFTS).unwrap());
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn a_change_of_the_ci_shows_after_the_refresh_button() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);
    connect_with_token(gui, TOKEN);
    wait_for_text(gui, &gh::pr_row(0, gh::pr_row_part::GLYPH), "failed");

    setup
        .stub
        .set_job_state("acme/web", 1, "build", JobState::Passed);
    gui.mouse_click(gh::REFRESH_BUTTON).unwrap();
    wait_for_text(gui, &gh::pr_row(0, gh::pr_row_part::GLYPH), "passed");
    assert_eq!(gui.text(&gh::pr_segment(0, 0)).unwrap(), "passed");
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn selecting_a_row_shows_the_header_of_its_pull_request() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);
    connect_with_token(gui, TOKEN);
    gui.wait_until_visible(&gh::pr_row(0, ""), TIMEOUT).unwrap();
    assert!(gui.exists_and_visible(gh::PICK_HINT).unwrap());
    assert!(!gui.exists_and_visible(gh::PAGE_TITLE).unwrap());

    gui.mouse_click(&gh::pr_row(0, "")).unwrap();
    gui.wait_until_visible(gh::PAGE_TITLE, TIMEOUT).unwrap();
    assert_eq!(gui.text(gh::PAGE_TITLE).unwrap(), "Fix the flaky test");
    assert_eq!(gui.text(gh::PAGE_REPO).unwrap(), "acme/web");
    assert_eq!(gui.text(gh::PAGE_NUMBER).unwrap(), "#1");
    assert_eq!(gui.text(gh::PAGE_HEAD).unwrap(), "fix-flaky");
    assert_eq!(gui.text(gh::PAGE_BASE).unwrap(), "main");
    assert!(gui.text(gh::PAGE_UPDATED).unwrap().starts_with("updated "));
    assert!(gui.checked(&gh::pr_row(0, "")).unwrap());
    assert!(!gui.checked(&gh::pr_row(1, "")).unwrap());
    assert!(!gui.exists_and_visible(gh::PICK_HINT).unwrap());
    assert!(!gui.exists_and_visible(gh::PAGE_DRAFT).unwrap());
    assert!(gui.exists_and_visible(gh::PAGE_OPEN_BUTTON).unwrap());

    // Another row, another page.
    gui.mouse_click(&gh::pr_row(1, "")).unwrap();
    wait_for_text(gui, gh::PAGE_TITLE, "Add dark mode");
    assert!(gui.checked(&gh::pr_row(1, "")).unwrap());
    assert!(!gui.checked(&gh::pr_row(0, "")).unwrap());
}

// GH-009
#[test]
#[ignore = "needs the app: run through ctest"]
fn disconnecting_from_the_tile_goes_back_to_the_connect_page() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);
    connect_with_token(gui, TOKEN);
    gui.wait_until_visible(&gh::pr_row(0, ""), TIMEOUT).unwrap();
    gui.mouse_click(&gh::pr_row(0, "")).unwrap();
    gui.wait_until_visible(gh::PAGE_TITLE, TIMEOUT).unwrap();

    gui.mouse_click_with(gh::TILE, MouseButton::Right, Modifiers::NONE)
        .unwrap();
    gui.wait_until_visible(gh::DISCONNECT_ACTION, TIMEOUT)
        .unwrap();
    gui.mouse_click(gh::DISCONNECT_ACTION).unwrap();

    gui.wait_until_visible(gh::CONNECT_TITLE, TIMEOUT).unwrap();
    assert!(!gui.exists_and_visible(gh::LIST).unwrap());
    assert!(!gui.exists_and_visible(gh::TILE_FAILING).unwrap());
    // What was open is closed: connecting again starts with none.
    connect_with_token(gui, TOKEN);
    gui.wait_until_visible(gh::PICK_HINT, TIMEOUT).unwrap();
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn an_account_tile_leads_back_from_github_and_the_github_tile_goes_dark() {
    let setup = launch(stub_with_pull_requests());
    let gui = setup.gui();
    open_page(gui);
    assert!(gui.checked(gh::TILE).unwrap());

    // The login page of the account is what the server tile leads back to.
    let tile = rc_gui_tests::paths::server_tile(0, "");
    gui.mouse_click(&tile).unwrap();
    gui.wait_until_visible(rc_gui_tests::paths::main_window::LOGIN_BUTTON, TIMEOUT)
        .unwrap();
    assert!(!gui.checked(gh::TILE).unwrap());
    assert!(!gui.exists_and_visible(gh::TILE_CURRENT).unwrap());
}
