// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The GitHub pull requests page: the tile in the rail, the connect page, the lists, and the page of
//! a pull request with its checks and actions. There is no
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
use rc_github::{JobState, MergeState, PrState, ReviewDecision};
use rc_gui_tests::paths::github as gh;
use rc_gui_tests::{App, Gui, Key, Modifiers, MouseButton};
use rc_test_github::{Failure, GithubStub, Job, PullRequest};

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

// --- the page of a pull request ---

/// A stub with these pull requests of the user's, oldest last (the first is row 0).
fn stub_with(prs: Vec<PullRequest>) -> GithubStub {
    let stub = GithubStub::start();
    stub.set_viewer("me");
    stub.require_token(Some(TOKEN));
    let count = i64::try_from(prs.len()).unwrap();
    for (at, pr) in (0..count).zip(prs) {
        let ago = Timestamp::now() - SignedDuration::from_mins(at + 1);
        stub.add_pull_request(pr.author("me").updated_at(ago));
    }
    stub
}

/// A pull request whose jobs all passed.
fn passing(title: &str) -> PullRequest {
    PullRequest::new("acme/web", title)
        .head("ship-it")
        .head_sha("abc123")
        .job(Job::new("build").state(JobState::Passed))
        .job(Job::new("lint").state(JobState::Passed))
}

/// Connects, opens the pull request at `row` of the list and waits for its page.
fn open_pull_request(setup: &Setup, row: usize, title: &str) {
    let gui = setup.gui();
    open_page(gui);
    connect_with_token(gui, TOKEN);
    wait_for_text_containing(gui, &gh::pr_row(row, gh::pr_row_part::TITLE), title);
    gui.mouse_click(&gh::pr_row(row, "")).unwrap();
    wait_for_text(gui, gh::PAGE_TITLE, title);
}

fn enabled(gui: &Gui, path: &str) -> bool {
    gui.exists_and_visible(path).unwrap() && gui.enabled(path).unwrap()
}

fn wait_for_enabled(gui: &Gui, path: &str) {
    gui.wait_until(&format!("{path} enabled"), TIMEOUT, || {
        Ok(enabled(gui, path))
    })
    .unwrap();
}

/// Waits for the toast, then returns its text.
fn toast(gui: &Gui) -> String {
    gui.wait_until_visible(gh::TOAST, TIMEOUT).unwrap();
    gui.text(gh::TOAST).unwrap()
}

// GH-003
#[test]
#[ignore = "needs the app: run through ctest"]
fn merge_asks_first_then_merges_with_the_head_sha_and_the_pull_request_goes() {
    let setup = launch(stub_with(vec![passing("Ship it")]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Ship it");

    wait_for_enabled(gui, gh::MERGE);
    assert!(!gui.exists_and_visible(gh::MERGE_REASON).unwrap());
    assert!(!gui.exists_and_visible(gh::MERGE_ANYWAY).unwrap());
    assert!(!gui.exists_and_visible(gh::CONFIRM).unwrap());
    assert!(!gui.exists_and_visible(gh::APPROVAL).unwrap());
    // Nothing is merged by the first click: it asks.
    gui.mouse_click(gh::MERGE).unwrap();
    gui.wait_until_visible(gh::CONFIRM, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(gh::CONFIRM).unwrap(),
        "Rebase ship-it onto main and merge?"
    );
    assert!(!gui.exists_and_visible(gh::MERGE).unwrap());
    assert!(gui.exists_and_visible(gh::CONFIRM_MERGE).unwrap());
    assert!(!gui.exists_and_visible(gh::CONFIRM_MERGE_ANYWAY).unwrap());
    assert!(
        setup
            .stub
            .requests_to("PUT", "/repos/acme/web/pulls/1/merge")
            .is_empty()
    );

    gui.mouse_click(gh::CONFIRM_MERGE).unwrap();
    assert_eq!(toast(gui), "Merged acme/web #1");
    let merges = setup
        .stub
        .requests_to("PUT", "/repos/acme/web/pulls/1/merge-async");
    assert_eq!(merges.len(), 1);
    assert!(
        merges[0]
            .body
            .as_ref()
            .unwrap()
            .to_string()
            .contains("abc123")
    );
    assert_eq!(
        setup.stub.pull_request("acme/web", 1).unwrap().state,
        PrState::Merged
    );
    // The refresh that follows the command drops the row, which closes the page; the toast
    // outlives it for a moment.
    gui.wait_until_visible(gh::EMPTY, TIMEOUT).unwrap();
    gui.wait_until_visible(gh::PICK_HINT, TIMEOUT).unwrap();
    gui.wait_until_hidden(gh::TOAST, TIMEOUT).unwrap();
}

// GH-003
#[test]
#[ignore = "needs the app: run through ctest"]
fn cancel_escape_and_another_pull_request_drop_the_question_of_merge() {
    let setup = launch(stub_with(vec![passing("Ship it"), passing("Ship that")]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Ship it");
    wait_for_enabled(gui, gh::MERGE);

    gui.mouse_click(gh::MERGE).unwrap();
    gui.wait_until_visible(gh::CONFIRM, TIMEOUT).unwrap();
    gui.mouse_click(gh::CONFIRM_CANCEL).unwrap();
    gui.wait_until_hidden(gh::CONFIRM, TIMEOUT).unwrap();
    assert!(gui.exists_and_visible(gh::MERGE).unwrap());

    gui.mouse_click(gh::MERGE).unwrap();
    gui.wait_until_visible(gh::CONFIRM, TIMEOUT).unwrap();
    gui.enter_key("mainWindow", Key::ESCAPE, Modifiers::NONE)
        .unwrap();
    gui.wait_until_hidden(gh::CONFIRM, TIMEOUT).unwrap();

    gui.mouse_click(gh::MERGE).unwrap();
    gui.wait_until_visible(gh::CONFIRM, TIMEOUT).unwrap();
    gui.mouse_click(&gh::pr_row(1, "")).unwrap();
    wait_for_text(gui, gh::PAGE_TITLE, "Ship that");
    gui.wait_until_hidden(gh::CONFIRM, TIMEOUT).unwrap();
    assert!(gui.exists_and_visible(gh::MERGE).unwrap());

    // Nothing was merged all along.
    assert!(
        setup
            .stub
            .requests_to("PUT", "/repos/acme/web/pulls")
            .is_empty()
    );
}

// GH-003
#[test]
#[ignore = "needs the app: run through ctest"]
fn merge_anyway_after_failed_checks_asks_and_merges() {
    let setup = launch(stub_with(vec![
        PullRequest::new("acme/web", "Risky")
            .head("risky")
            .job(Job::new("build").state(JobState::Failed))
            .job(Job::new("lint").state(JobState::Failed))
            .job(Job::new("docs").state(JobState::Passed)),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Risky");

    gui.wait_until_visible(gh::MERGE_ANYWAY, TIMEOUT).unwrap();
    assert!(!gui.enabled(gh::MERGE).unwrap());
    assert_eq!(gui.text(gh::MERGE_REASON).unwrap(), "2 checks failed");
    gui.mouse_click(gh::MERGE_ANYWAY).unwrap();
    gui.wait_until_visible(gh::CONFIRM, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(gh::CONFIRM).unwrap(),
        "2 checks failed. Merge anyway?"
    );
    assert!(gui.exists_and_visible(gh::CONFIRM_MERGE_ANYWAY).unwrap());
    assert!(!gui.exists_and_visible(gh::CONFIRM_MERGE).unwrap());

    gui.mouse_click(gh::CONFIRM_MERGE_ANYWAY).unwrap();
    assert_eq!(toast(gui), "Merged acme/web #1");
    assert_eq!(
        setup.stub.pull_request("acme/web", 1).unwrap().state,
        PrState::Merged
    );
}

// GH-003
#[test]
#[ignore = "needs the app: run through ctest"]
fn merge_is_disabled_with_the_reason_of_each_kind() {
    // Newest first: rows 0 to 3 are ready pull requests, the draft is under the fold.
    let setup = launch(stub_with(vec![
        PullRequest::new("acme/web", "Running").job(Job::new("build").state(JobState::Running)),
        PullRequest::new("acme/web", "Behind")
            .merge_state(MergeState::Behind)
            .job(Job::new("build").state(JobState::Passed)),
        PullRequest::new("acme/web", "Conflicts")
            .merge_state(MergeState::Dirty)
            .job(Job::new("build").state(JobState::Passed)),
        PullRequest::new("acme/web", "Review")
            .merge_state(MergeState::Blocked)
            .review(ReviewDecision::ReviewRequired)
            .job(Job::new("build").state(JobState::Passed)),
        PullRequest::new("acme/web", "Draft")
            .draft()
            .job(Job::new("build").state(JobState::Passed)),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Running");
    gui.mouse_click(gh::DRAFTS).unwrap();

    for (row, title, reason) in [
        (0, "Running", "Waiting for the checks"),
        (1, "Behind", "main is ahead"),
        (2, "Conflicts", "Conflicts with main"),
        (3, "Review", "Needs an approving review"),
        (5, "Draft", "Drafts can't be merged"),
    ] {
        wait_for_text_containing(gui, &gh::pr_row(row, gh::pr_row_part::TITLE), title);
        gui.mouse_click(&gh::pr_row(row, "")).unwrap();
        wait_for_text(gui, gh::PAGE_TITLE, title);
        wait_for_text(gui, gh::MERGE_REASON, reason);
        assert!(!gui.enabled(gh::MERGE).unwrap(), "{title}");
        assert!(
            !gui.exists_and_visible(gh::MERGE_ANYWAY).unwrap(),
            "{title}"
        );
    }
}

// GH-004
#[test]
#[ignore = "needs the app: run through ctest"]
fn update_branch_rebases_a_pull_request_that_is_behind() {
    let setup = launch(stub_with(vec![
        passing("Stale").merge_state(MergeState::Behind),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Stale");

    gui.wait_until_visible(gh::UPDATE_BRANCH, TIMEOUT).unwrap();
    assert_eq!(gui.text(gh::PAGE_BASE_NOTE).unwrap(), "Behind main");
    assert!(!gui.exists_and_visible(gh::RERUN_FAILED).unwrap());
    gui.mouse_click(gh::UPDATE_BRANCH).unwrap();
    assert_eq!(toast(gui), "Updated ship-it. The checks run again.");
    assert_eq!(setup.stub.operations("rebase").len(), 1);

    // The refresh shows what GitHub did: clean, with the checks running again.
    wait_for_text(gui, &gh::pr_row(0, gh::pr_row_part::GLYPH), "running");
    gui.wait_until_hidden(gh::UPDATE_BRANCH, TIMEOUT).unwrap();
    assert!(!gui.exists_and_visible(gh::PAGE_BASE_NOTE).unwrap());
    wait_for_text(gui, gh::MERGE_REASON, "Waiting for the checks");
}

// GH-005
#[test]
#[ignore = "needs the app: run through ctest"]
fn rerun_failed_jobs_starts_them_again() {
    let setup = launch(stub_with(vec![
        PullRequest::new("acme/web", "Flaky")
            .job(Job::new("build").state(JobState::Failed))
            .job(Job::new("test").state(JobState::Failed))
            .job(Job::new("lint").state(JobState::Passed)),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Flaky");

    gui.wait_until_visible(gh::RERUN_FAILED, TIMEOUT).unwrap();
    gui.mouse_click(gh::RERUN_FAILED).unwrap();
    assert_eq!(toast(gui), "Rerunning the failed jobs");
    assert_eq!(
        setup
            .stub
            .requests_to("POST", "/repos/acme/web/actions/runs/")
            .len(),
        1
    );
    wait_for_text(gui, &gh::pr_row(0, gh::pr_row_part::GLYPH), "running");
    gui.wait_until_hidden(gh::RERUN_FAILED, TIMEOUT).unwrap();
}

// GH-005
#[test]
#[ignore = "needs the app: run through ctest"]
fn rerun_of_one_failed_job_starts_only_that_job() {
    let setup = launch(stub_with(vec![
        PullRequest::new("acme/web", "Flaky")
            .job(Job::new("build").state(JobState::Failed))
            .job(Job::new("test").state(JobState::Failed))
            .job(Job::new("lint").state(JobState::Passed)),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Flaky");

    // The failed jobs come first; a passed one has no Rerun.
    gui.wait_until_visible(&gh::job_rerun(1), TIMEOUT).unwrap();
    assert!(gui.exists_and_visible(&gh::job_rerun(0)).unwrap());
    assert!(!gui.exists_and_visible(&gh::job_rerun(2)).unwrap());
    let name = gui.text(&gh::job(1, "prJobName")).unwrap();
    gui.mouse_click(&gh::job_rerun(1)).unwrap();
    assert_eq!(toast(gui), format!("Rerunning {name}"));
    let reruns = setup
        .stub
        .requests_to("POST", "/repos/acme/web/actions/jobs/");
    assert_eq!(reruns.len(), 1);
    assert!(reruns[0].path.ends_with("/rerun"));
    // One failed job is left, and one runs.
    wait_for_text(gui, &gh::job(1, "prJobState"), "running");
    assert_eq!(gui.text(&gh::job(0, "prJobState")).unwrap(), "failed");
}

// GH-006
#[test]
#[ignore = "needs the app: run through ctest"]
fn approve_runs_clears_the_banner() {
    let setup = launch(stub_with(vec![
        PullRequest::new("acme/web", "From a fork")
            .head("fork")
            .needs_approval(),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "From a fork");

    gui.wait_until_visible(gh::APPROVAL, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(gh::APPROVAL_TEXT).unwrap(),
        "Runs are waiting for your approval"
    );
    assert_eq!(
        gui.text(gh::CHECKS_HEADING).unwrap(),
        "Runs waiting for approval"
    );
    assert_eq!(
        gui.text(gh::MERGE_REASON).unwrap(),
        "The runs need approval"
    );
    gui.mouse_click(gh::APPROVE).unwrap();
    assert_eq!(toast(gui), "Approved the runs");
    assert!(
        !setup
            .stub
            .pull_request("acme/web", 1)
            .unwrap()
            .needs_approval
    );
    gui.wait_until_hidden(gh::APPROVAL, TIMEOUT).unwrap();
    wait_for_text(gui, gh::CHECKS_HEADING, "Checks running");
}

// GH-008
#[test]
#[ignore = "needs the app: run through ctest"]
fn notify_when_done_is_a_toggle_with_a_bell_on_the_row_until_the_ci_ends() {
    let setup = launch(stub_with(vec![
        PullRequest::new("acme/web", "Slow").job(Job::new("build").state(JobState::Running)),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Slow");

    gui.wait_until_visible(gh::NOTIFY, TIMEOUT).unwrap();
    assert_eq!(gui.text(gh::NOTIFY).unwrap(), "Notify me when done");
    assert!(
        !gui.exists_and_visible(&gh::pr_row(0, gh::pr_row_part::BELL))
            .unwrap()
    );
    gui.mouse_click(gh::NOTIFY).unwrap();
    wait_for_text(gui, gh::NOTIFY, "Notifying you");
    gui.wait_until_visible(&gh::pr_row(0, gh::pr_row_part::BELL), TIMEOUT)
        .unwrap();

    // Off again, and on.
    gui.mouse_click(gh::NOTIFY).unwrap();
    wait_for_text(gui, gh::NOTIFY, "Notify me when done");
    gui.wait_until_hidden(&gh::pr_row(0, gh::pr_row_part::BELL), TIMEOUT)
        .unwrap();
    gui.mouse_click(gh::NOTIFY).unwrap();
    wait_for_text(gui, gh::NOTIFY, "Notifying you");

    // The CI ends: the refresh shows it, the bell and the toggle go with the watch.
    setup
        .stub
        .set_job_state("acme/web", 1, "build", JobState::Passed);
    gui.mouse_click(gh::REFRESH_BUTTON).unwrap();
    wait_for_text(gui, &gh::pr_row(0, gh::pr_row_part::GLYPH), "passed");
    gui.wait_until_hidden(&gh::pr_row(0, gh::pr_row_part::BELL), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(gh::NOTIFY, TIMEOUT).unwrap();
    wait_for_text(gui, gh::CHECKS_HEADING, "Checks passed");
    wait_for_enabled(gui, gh::MERGE);
}

// GH-003
#[test]
#[ignore = "needs the app: run through ctest"]
fn a_failed_command_says_why_until_it_is_dismissed() {
    let setup = launch(stub_with(vec![passing("Ship it")]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Ship it");
    wait_for_enabled(gui, gh::MERGE);
    assert!(!gui.exists_and_visible(gh::ERROR).unwrap());

    setup.stub.fail_next(
        "/repos/acme/web/pulls/1/merge-async",
        1,
        Failure::rate_limited(),
    );
    gui.mouse_click(gh::MERGE).unwrap();
    gui.mouse_click(gh::CONFIRM_MERGE).unwrap();
    gui.wait_until_visible(gh::ERROR, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(gh::ERROR).unwrap(),
        "GitHub's rate limit is reached"
    );
    assert!(!gui.exists_and_visible(gh::TOAST).unwrap());
    // The buttons are back, nothing was merged.
    assert!(gui.exists_and_visible(gh::MERGE).unwrap());
    assert_eq!(
        setup.stub.pull_request("acme/web", 1).unwrap().state,
        PrState::Open
    );

    gui.mouse_click(gh::ERROR_DISMISS).unwrap();
    gui.wait_until_hidden(gh::ERROR, TIMEOUT).unwrap();
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn the_checks_show_the_heading_tally_bar_and_groups_that_fold() {
    let setup = launch(stub_with(vec![
        PullRequest::new("acme/web", "Mixed")
            .job(Job::new("build").state(JobState::Failed).workflow("CI"))
            .job(
                Job::new("deploy")
                    .state(JobState::Running)
                    .workflow("Release"),
            )
            .job(Job::new("slow").state(JobState::Cancelled))
            .job(Job::new("lint").state(JobState::Passed))
            .job(Job::new("test").state(JobState::Passed))
            .job(Job::new("docs").state(JobState::Passed)),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Mixed");

    gui.wait_until_visible(gh::CHECKS_HEADING, TIMEOUT).unwrap();
    assert_eq!(gui.text(gh::CHECKS_HEADING).unwrap(), "Checks failed");
    assert_eq!(
        gui.text(gh::TALLY).unwrap(),
        "1 failed, 1 running, 1 cancelled, 3 passed"
    );
    // The bar has the colours of the row's, failed first.
    for (index, state) in [
        "failed",
        "running",
        "cancelled",
        "passed",
        "passed",
        "passed",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(gui.text(&gh::checks_segment(index)).unwrap(), state);
    }
    assert!(!gui.exists_and_visible(&gh::checks_segment(6)).unwrap());

    // Failed and running show their jobs; cancelled and passed are folded.
    assert_eq!(
        gui.text(&gh::job_group_title("Failed")).unwrap(),
        "Failed 1"
    );
    assert_eq!(
        gui.text(&gh::job_group_title("Running")).unwrap(),
        "Running 1"
    );
    assert_eq!(
        gui.text(&gh::job_group_title("Cancelled")).unwrap(),
        "Cancelled 1"
    );
    assert_eq!(
        gui.text(&gh::job_group_title("Passed")).unwrap(),
        "Passed 3"
    );
    assert!(gui.checked(&gh::job_group("Failed")).unwrap());
    assert!(gui.checked(&gh::job_group("Running")).unwrap());
    assert!(!gui.checked(&gh::job_group("Cancelled")).unwrap());
    assert!(!gui.checked(&gh::job_group("Passed")).unwrap());
    assert_eq!(gui.text(&gh::job(0, "prJobName")).unwrap(), "build");
    assert_eq!(gui.text(&gh::job(0, "prJobWorkflow")).unwrap(), "CI / ");
    assert_eq!(gui.text(&gh::job(0, "prJobState")).unwrap(), "failed");
    assert_eq!(gui.text(&gh::job(1, "prJobName")).unwrap(), "deploy");
    assert_eq!(
        gui.text(&gh::job(1, "prJobWorkflow")).unwrap(),
        "Release / "
    );
    assert!(!gui.exists_and_visible(&gh::job(2, "")).unwrap());
    assert!(!gui.exists_and_visible(&gh::job(3, "")).unwrap());

    gui.mouse_click(&gh::job_group("Passed")).unwrap();
    gui.wait_until_visible(&gh::job(3, ""), TIMEOUT).unwrap();
    assert!(gui.checked(&gh::job_group("Passed")).unwrap());
    gui.mouse_click(&gh::job_group("Cancelled")).unwrap();
    gui.wait_until_visible(&gh::job(2, ""), TIMEOUT).unwrap();
    assert_eq!(gui.text(&gh::job(2, "prJobState")).unwrap(), "cancelled");
    gui.mouse_click(&gh::job_group("Passed")).unwrap();
    gui.wait_until_hidden(&gh::job(3, ""), TIMEOUT).unwrap();
    assert!(!gui.checked(&gh::job_group("Passed")).unwrap());
    assert!(gui.exists_and_visible(&gh::job(2, "")).unwrap());
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn a_click_on_a_segment_opens_its_group() {
    let setup = launch(stub_with(vec![
        PullRequest::new("acme/web", "Mixed")
            .job(Job::new("build").state(JobState::Failed))
            .job(Job::new("lint").state(JobState::Passed))
            .job(Job::new("test").state(JobState::Passed)),
    ]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Mixed");
    gui.wait_until_visible(&gh::checks_segment(2), TIMEOUT)
        .unwrap();
    assert!(!gui.checked(&gh::job_group("Passed")).unwrap());
    assert!(!gui.exists_and_visible(&gh::job(2, "")).unwrap());

    gui.mouse_click(&gh::checks_segment(2)).unwrap();
    gui.wait_until_visible(&gh::job(2, ""), TIMEOUT).unwrap();
    assert!(gui.checked(&gh::job_group("Passed")).unwrap());
    // The other passed job shows too, and the failed group is as it was.
    assert!(gui.exists_and_visible(&gh::job(1, "")).unwrap());
    assert!(gui.checked(&gh::job_group("Failed")).unwrap());
}

// GH-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn when_everything_passed_the_passed_group_is_open() {
    let setup = launch(stub_with(vec![passing("Ship it")]));
    let gui = setup.gui();
    open_pull_request(&setup, 0, "Ship it");

    gui.wait_until_visible(gh::CHECKS_HEADING, TIMEOUT).unwrap();
    assert_eq!(gui.text(gh::CHECKS_HEADING).unwrap(), "Checks passed");
    assert_eq!(gui.text(gh::TALLY).unwrap(), "2 passed");
    assert!(gui.checked(&gh::job_group("Passed")).unwrap());
    assert!(!gui.exists_and_visible(&gh::job_group("Failed")).unwrap());
    gui.wait_until_visible(&gh::job(0, ""), TIMEOUT).unwrap();
    assert!(!gui.exists_and_visible(&gh::job_rerun(0)).unwrap());
    assert!(!gui.exists_and_visible(gh::NOTIFY).unwrap());
}
