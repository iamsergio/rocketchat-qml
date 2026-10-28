// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use jiff::Timestamp;
use rc_core::github::{
    AttemptId, CiState, CommandState, ConnectFailure, Connection, GithubError, GithubEvent, Job,
    JobState, MergeState, PrAction, PrKey, PullRequest, RefreshState, RepoRunners, ReviewDecision,
    Runner,
};
use rc_core::list_change::ListChange;

use super::page::merge_reason;
use super::row::{CiKind, Glyph, Segment, check_bar, glyph};
use super::{
    Applied, BaseNote, CommandStatus, ConnectState, GithubCache, RunnerState, Sections, Tab, text,
};

const NOW: i64 = 1_800_000_000;

fn at(seconds_ago: i64) -> Timestamp {
    Timestamp::from_second(NOW - seconds_ago).unwrap()
}

fn now() -> Timestamp {
    at(0)
}

fn job(name: &str, state: JobState) -> Job {
    Job {
        id: Some(name.len() as u64 * 100),
        name: name.to_owned(),
        workflow: Some("CI".to_owned()),
        state,
        url: Some(format!("https://github.com/o/r/runs/{name}")),
        started_at: None,
        completed_at: None,
    }
}

fn pr(number: u64, ci: CiState, jobs: Vec<Job>) -> PullRequest {
    PullRequest {
        key: PrKey::new("o/r", number),
        title: format!("Title {number}"),
        author: "me".to_owned(),
        url: format!("https://github.com/o/r/pull/{number}"),
        updated_at: at(660),
        is_draft: false,
        review: None,
        merge_state: MergeState::Clean,
        head_branch: format!("feature-{number}"),
        base_branch: "main".to_owned(),
        head_sha: "abc".to_owned(),
        ci,
        jobs,
        needs_approval: false,
    }
}

fn draft(number: u64) -> PullRequest {
    PullRequest {
        is_draft: true,
        ..pr(number, CiState::None, Vec::new())
    }
}

fn connected(cache: &mut GithubCache) {
    cache.apply(&GithubEvent::Connection(Connection::Connected {
        login: "me".to_owned(),
    }));
}

fn fill(cache: &mut GithubCache, tab: Tab, items: Vec<PullRequest>) {
    let changes = vec![ListChange::Reset(items)];
    cache.apply(&match tab {
        Tab::Mine => GithubEvent::Mine(changes),
        Tab::Watched => GithubEvent::Watched(changes),
    });
}

fn with_mine(items: Vec<PullRequest>) -> GithubCache {
    let mut cache = GithubCache::new();
    connected(&mut cache);
    fill(&mut cache, Tab::Mine, items);
    cache
}

fn command(cache: &mut GithubCache, key: &PrKey, action: PrAction, n: u64, state: CommandState) {
    cache.apply(&GithubEvent::Command {
        pr: key.clone(),
        action,
        attempt: attempt(n),
        state,
    });
}

fn attempt(n: u64) -> AttemptId {
    AttemptId::new(n)
}

// --- text ---

// GH-001
#[test]
fn ages_change_unit_at_their_boundaries() {
    let age = |seconds: i64| text::age(now(), at(seconds));
    assert_eq!(age(-5), "0 s");
    assert_eq!(age(0), "0 s");
    assert_eq!(age(59), "59 s");
    assert_eq!(age(60), "1 min");
    assert_eq!(age(11 * 60 + 30), "11 min");
    assert_eq!(age(3599), "59 min");
    assert_eq!(age(3600), "1 h");
    assert_eq!(age(86_399), "23 h");
    assert_eq!(age(86_400), "1 d");
    assert_eq!(age(5 * 86_400), "5 d");
    assert_eq!(age(7 * 86_400 - 1), "6 d");
    assert_eq!(age(7 * 86_400), "1 w");
    assert_eq!(age(3 * 7 * 86_400), "3 w");
    assert_eq!(age(30 * 86_400 - 1), "4 w");
    assert_eq!(age(30 * 86_400), "1 mo");
    assert_eq!(age(8 * 30 * 86_400), "8 mo");
    assert_eq!(age(365 * 86_400 - 1), "12 mo");
    assert_eq!(age(365 * 86_400), "1 y");
}

// GH-001
#[test]
fn durations_drop_zero_parts_and_running_ones_count_minutes() {
    assert_eq!(text::duration(-3), "0 s");
    assert_eq!(text::duration(48), "48 s");
    assert_eq!(text::duration(59), "59 s");
    assert_eq!(text::duration(60), "1 m");
    assert_eq!(text::duration(9 * 60 + 12), "9 m 12 s");
    assert_eq!(text::duration(3599), "59 m 59 s");
    assert_eq!(text::duration(3600), "1 h");
    assert_eq!(text::duration(3600 + 5 * 60 + 30), "1 h 5 m");
    assert_eq!(text::so_far(30), "30 s so far");
    assert_eq!(text::so_far(59), "59 s so far");
    assert_eq!(text::so_far(60), "1 m so far");
    assert_eq!(text::so_far(6 * 60 + 40), "6 m so far");
    assert_eq!(text::so_far(3600), "1 h so far");
    assert_eq!(text::so_far(3600 + 125), "1 h 2 m so far");
}

// GH-009
#[test]
fn the_connect_page_says_why_it_failed() {
    let reason = |failure| {
        let mut cache = GithubCache::new();
        cache.apply(&GithubEvent::Connection(Connection::Failed(failure)));
        let view = cache.connect_view();
        assert_eq!(view.state, ConnectState::Failed);
        view.reason
    };
    assert_eq!(reason(ConnectFailure::BadToken), "GitHub refused the token");
    assert_eq!(reason(ConnectFailure::GhMissing), "gh isn't installed");
    assert_eq!(
        reason(ConnectFailure::GhNotLoggedIn),
        "gh isn't logged in. Run gh auth login"
    );
    assert_eq!(
        reason(ConnectFailure::RateLimited),
        "GitHub's rate limit is reached"
    );
    assert_eq!(reason(ConnectFailure::Unreachable), "Can't reach GitHub");
    assert_eq!(reason(ConnectFailure::Other("odd".to_owned())), "odd");
}

// GH-009
#[test]
fn the_connect_page_follows_the_connection() {
    let mut cache = GithubCache::new();
    assert_eq!(cache.connect_view().state, ConnectState::NotConnected);
    assert!(cache.connect_view().reason.is_empty());
    cache.apply(&GithubEvent::Connection(Connection::Connecting));
    assert_eq!(cache.connect_view().state, ConnectState::Connecting);
    connected(&mut cache);
    assert_eq!(cache.connect_view().state, ConnectState::Connected);
    assert_eq!(cache.login(), Some("me"));
}

// --- lists ---

// GH-001
#[test]
fn the_lists_follow_the_services_changes_and_split_at_the_drafts() {
    let mut cache = with_mine(vec![
        pr(1, CiState::Success, vec![]),
        pr(2, CiState::Success, vec![]),
        draft(3),
    ]);
    assert_eq!(
        cache.sections(Tab::Mine),
        Sections {
            ready: 2,
            drafts: 1
        }
    );
    assert_eq!(cache.sections(Tab::Mine).drafts_title(), "Drafts 1");

    let moved = pr(2, CiState::Failure, vec![]);
    let applied = cache.apply(&GithubEvent::Mine(vec![
        ListChange::Moved {
            from: 1,
            to: 0,
            item: moved,
        },
        ListChange::Inserted {
            index: 3,
            item: draft(4),
        },
        ListChange::Updated {
            index: 1,
            item: pr(1, CiState::Pending, vec![]),
        },
        ListChange::Removed { index: 2 },
    ]));
    assert_eq!(applied, Applied::Mine);
    let numbers: Vec<u64> = cache.list(Tab::Mine).iter().map(|p| p.key.number).collect();
    assert_eq!(numbers, [2, 1, 4]);
    assert_eq!(
        cache.sections(Tab::Mine),
        Sections {
            ready: 2,
            drafts: 1
        }
    );
}

// GH-002
#[test]
fn the_watched_list_is_apart_and_shows_the_author() {
    let mut cache = with_mine(vec![pr(1, CiState::Success, vec![])]);
    let mut other = pr(9, CiState::Pending, vec![]);
    other.author = "bob".to_owned();
    fill(&mut cache, Tab::Watched, vec![other, draft(10)]);
    assert_eq!(
        cache.sections(Tab::Watched),
        Sections {
            ready: 1,
            drafts: 1
        }
    );
    assert_eq!(cache.row(Tab::Watched, 0, now()).unwrap().author, "bob");
    assert_eq!(cache.row(Tab::Mine, 0, now()).unwrap().author, "");
    assert!(cache.row(Tab::Watched, 2, now()).is_none());
    assert_eq!(cache.header(now()).watched_count, 1);
    assert_eq!(cache.header(now()).mine_count, 1);
    // A watched pull request has a page too.
    assert!(cache.page(&PrKey::new("o/r", 9), now()).is_some());
}

// GH-001
#[test]
fn a_row_is_built_for_one_pull_request_only() {
    let items: Vec<PullRequest> = (1..=300).map(|n| pr(n, CiState::Success, vec![])).collect();
    let cache = with_mine(items);
    let row = cache.row(Tab::Mine, 250, now()).unwrap();
    assert_eq!(row.key, PrKey::new("o/r", 251));
    assert_eq!(row.number_text, "#251");
    assert_eq!(row.title, "Title 251");
    assert_eq!(row.repo, "o/r");
    assert_eq!(row.age, "11 min");
}

// GH-001
#[test]
fn the_glyph_follows_the_draft_flag_the_ci_and_the_approval() {
    let waiting = PullRequest {
        needs_approval: true,
        ..pr(1, CiState::None, vec![])
    };
    let failed_and_waiting = PullRequest {
        needs_approval: true,
        ..pr(1, CiState::Failure, vec![])
    };
    let draft_failing = PullRequest {
        is_draft: true,
        ..pr(1, CiState::Failure, vec![])
    };
    assert_eq!(glyph(&pr(1, CiState::Success, vec![])), Glyph::Passed);
    assert_eq!(glyph(&pr(1, CiState::Failure, vec![])), Glyph::Failed);
    assert_eq!(glyph(&pr(1, CiState::Pending, vec![])), Glyph::Running);
    assert_eq!(glyph(&pr(1, CiState::None, vec![])), Glyph::NoChecks);
    assert_eq!(glyph(&waiting), Glyph::WaitingApproval);
    assert_eq!(glyph(&failed_and_waiting), Glyph::Failed);
    assert_eq!(glyph(&draft_failing), Glyph::Draft);
}

// GH-001
#[test]
fn the_check_bar_orders_failed_running_cancelled_passed() {
    let p = pr(
        1,
        CiState::Failure,
        vec![
            job("a", JobState::Passed),
            job("b", JobState::Cancelled),
            job("c", JobState::Running),
            job("d", JobState::Failed),
            job("e", JobState::Passed),
            job("f", JobState::Failed),
        ],
    );
    assert_eq!(
        check_bar(&p),
        [
            Segment::Failed,
            Segment::Failed,
            Segment::Running,
            Segment::Cancelled,
            Segment::Passed,
            Segment::Passed,
        ]
    );
    let waiting = PullRequest {
        needs_approval: true,
        ..pr(2, CiState::None, vec![])
    };
    assert_eq!(check_bar(&waiting), [Segment::Waiting]);
    assert!(check_bar(&pr(3, CiState::None, vec![])).is_empty());
}

// GH-001
#[test]
fn a_row_is_bold_when_its_state_changed_since_it_was_seen() {
    let mut cache = with_mine(vec![pr(1, CiState::Pending, vec![])]);
    let key = PrKey::new("o/r", 1);
    assert!(!cache.row(Tab::Mine, 0, now()).unwrap().changed);

    cache.apply(&GithubEvent::Mine(vec![ListChange::Updated {
        index: 0,
        item: pr(1, CiState::Failure, vec![]),
    }]));
    assert!(cache.row(Tab::Mine, 0, now()).unwrap().changed);

    cache.select(&key);
    assert!(!cache.row(Tab::Mine, 0, now()).unwrap().changed);

    cache.apply(&GithubEvent::Mine(vec![ListChange::Updated {
        index: 0,
        item: pr(1, CiState::Success, vec![]),
    }]));
    assert!(cache.row(Tab::Mine, 0, now()).unwrap().changed);
    // Back to what was seen: not bold any more.
    cache.apply(&GithubEvent::Mine(vec![ListChange::Updated {
        index: 0,
        item: pr(1, CiState::Failure, vec![]),
    }]));
    assert!(!cache.row(Tab::Mine, 0, now()).unwrap().changed);
    assert!(!cache.changed(&key));
    // The row built for a copy of the pull request says the same as the list's.
    let copy = pr(1, CiState::Failure, vec![]);
    assert_eq!(
        cache.view_of(Tab::Mine, &copy, now()).changed,
        cache.changed(&key)
    );
}

// GH-001
#[test]
fn a_pull_request_that_comes_later_starts_unchanged_and_a_reset_forgets_the_gone() {
    let mut cache = with_mine(vec![pr(1, CiState::Pending, vec![])]);
    cache.apply(&GithubEvent::Mine(vec![ListChange::Inserted {
        index: 1,
        item: pr(2, CiState::Failure, vec![]),
    }]));
    assert!(!cache.row(Tab::Mine, 1, now()).unwrap().changed);

    // Disconnected and connected again: pull request 1 comes back in another state.
    cache.apply(&GithubEvent::Mine(vec![ListChange::Reset(vec![])]));
    fill(&mut cache, Tab::Mine, vec![pr(1, CiState::Success, vec![])]);
    assert!(!cache.row(Tab::Mine, 0, now()).unwrap().changed);
}

// GH-008
#[test]
fn the_bell_shows_the_pull_requests_to_notify() {
    let mut cache = with_mine(vec![
        pr(1, CiState::Pending, vec![]),
        pr(2, CiState::Pending, vec![]),
    ]);
    cache.apply(&GithubEvent::Notifying(vec![PrKey::new("o/r", 2)]));
    assert!(!cache.row(Tab::Mine, 0, now()).unwrap().bell);
    assert!(cache.row(Tab::Mine, 1, now()).unwrap().bell);
    assert!(cache.page(&PrKey::new("o/r", 2), now()).unwrap().notifying);
    assert_eq!(
        cache
            .page(&PrKey::new("o/r", 2), now())
            .unwrap()
            .notify
            .label,
        "Notifying you"
    );
}

// --- tile and header ---

// GH-001
#[test]
fn the_tile_is_grey_until_connected_and_counts_open_and_failing() {
    let mut cache = GithubCache::new();
    let tile = cache.tile();
    assert!(!tile.connected);
    assert_eq!(tile.tooltip_title, "Pull requests");
    assert_eq!(tile.tooltip, "Connect GitHub to see them");

    connected(&mut cache);
    fill(
        &mut cache,
        Tab::Mine,
        vec![
            pr(1, CiState::Failure, vec![]),
            pr(2, CiState::Success, vec![]),
            draft(3),
        ],
    );
    cache.apply(&GithubEvent::Failing(1));
    let tile = cache.tile();
    assert!(tile.connected);
    assert_eq!(tile.failing, 1);
    assert_eq!(tile.tooltip, "2 open, 1 failing");
}

// GH-008
#[test]
fn the_pip_comes_with_a_finished_ci_and_goes_when_the_page_is_looked_at() {
    let key = PrKey::new("o/r", 1);
    let mut cache = with_mine(vec![pr(1, CiState::Success, vec![])]);
    assert!(!cache.tile().pip);

    let applied = cache.apply(&GithubEvent::CiEnded {
        pr: key.clone(),
        state: CiState::Success,
    });
    assert_eq!(
        applied,
        Applied::CiEnded {
            pr: key.clone(),
            state: CiState::Success
        }
    );
    assert!(cache.tile().pip);
    // Still there after other events.
    cache.apply(&GithubEvent::Failing(0));
    assert!(cache.tile().pip);

    cache.set_on_screen(true);
    assert!(!cache.tile().pip);
    // Looked at: nothing to flag. It comes back for one that ends while away.
    cache.apply(&GithubEvent::CiEnded {
        pr: key.clone(),
        state: CiState::Failure,
    });
    assert!(!cache.tile().pip);
    cache.set_on_screen(false);
    cache.apply(&GithubEvent::CiEnded {
        pr: key,
        state: CiState::Failure,
    });
    assert!(cache.tile().pip);
}

// GH-008
#[test]
fn the_notification_says_what_ended() {
    let cache = with_mine(vec![pr(1, CiState::Success, vec![])]);
    let key = PrKey::new("o/r", 1);
    assert_eq!(
        cache.ended_text(&key, CiState::Success),
        ("Checks passed".to_owned(), "o/r #1 Title 1".to_owned())
    );
    assert_eq!(cache.ended_text(&key, CiState::Failure).0, "Checks failed");
    // Gone from the lists already: still a body.
    assert_eq!(
        cache.ended_text(&PrKey::new("o/r", 5), CiState::Success).1,
        "o/r #5"
    );
}

// GH-001
#[test]
fn the_header_says_who_and_how_fresh() {
    let mut cache = GithubCache::new();
    assert_eq!(cache.header(now()).status, "");
    assert_eq!(cache.header(now()).title, "Pull requests");
    connected(&mut cache);
    assert_eq!(cache.header(now()).status, "me");

    cache.apply(&GithubEvent::Refresh(RefreshState::Started));
    assert_eq!(cache.header(now()).status, "updating…");
    assert!(cache.header(now()).refreshing);

    cache.apply(&GithubEvent::Refresh(RefreshState::Done { at: at(40) }));
    assert_eq!(cache.header(now()).status, "me, updated 40 s ago");
    assert!(!cache.header(now()).refreshing);

    cache.apply(&GithubEvent::Refresh(RefreshState::Failed {
        error: GithubError::Unreachable,
        last_good: Some(at(300)),
    }));
    assert_eq!(
        cache.header(now()).status,
        "Can't reach GitHub, last updated 5 min ago"
    );
    cache.apply(&GithubEvent::Refresh(RefreshState::Failed {
        error: GithubError::Unreachable,
        last_good: None,
    }));
    assert_eq!(cache.header(now()).status, "Can't reach GitHub");

    // The age moves with `now` alone.
    cache.apply(&GithubEvent::Refresh(RefreshState::Done { at: at(0) }));
    assert_eq!(cache.header(at(-90)).status, "me, updated 1 min ago");
}

// GH-009
#[test]
fn losing_the_connection_empties_the_state_that_belongs_to_it() {
    let key = PrKey::new("o/r", 1);
    let mut cache = with_mine(vec![pr(1, CiState::Pending, vec![])]);
    cache.apply(&GithubEvent::Refresh(RefreshState::Started));
    command(
        &mut cache,
        &key,
        PrAction::UpdateBranch,
        1,
        CommandState::Started,
    );
    cache.apply(&GithubEvent::CiEnded {
        pr: key.clone(),
        state: CiState::Success,
    });
    cache.apply(&GithubEvent::Mine(vec![ListChange::Reset(vec![])]));
    cache.apply(&GithubEvent::Connection(Connection::NotConnected));
    assert!(cache.command(&key).is_none());
    assert!(!cache.tile().pip);
    assert!(!cache.header(now()).refreshing);
    assert_eq!(cache.header(now()).status, "");
}

// --- the page ---

fn page(p: &PullRequest) -> super::PrPageView {
    let cache = with_mine(vec![p.clone()]);
    cache.page(&p.key, now()).unwrap()
}

// GH-001
#[test]
fn the_page_names_the_pull_request() {
    let mut p = pr(41, CiState::Success, vec![]);
    p.updated_at = at(180);
    p.review = Some(ReviewDecision::ChangesRequested);
    let view = page(&p);
    assert_eq!(view.repo, "o/r");
    assert_eq!(view.number, 41);
    assert_eq!(view.updated, "updated 3 min ago");
    assert_eq!(view.author, None);
    assert_eq!(view.title, "Title 41");
    assert_eq!(view.branches, "feature-41 into main");
    assert_eq!(view.review_text, "Changes requested");
    assert_eq!(view.url, "https://github.com/o/r/pull/41");

    p.author = "Bob".to_owned();
    assert_eq!(page(&p).author.as_deref(), Some("Bob"));
    // The login compares without case.
    p.author = "ME".to_owned();
    assert_eq!(page(&p).author, None);
}

// GH-003
#[test]
fn the_review_state_has_its_words() {
    let text = |review| {
        let mut p = pr(1, CiState::Success, vec![]);
        p.review = review;
        page(&p).review_text
    };
    assert_eq!(text(Some(ReviewDecision::Approved)), "Approved");
    assert_eq!(
        text(Some(ReviewDecision::ChangesRequested)),
        "Changes requested"
    );
    assert_eq!(
        text(Some(ReviewDecision::ReviewRequired)),
        "Review required"
    );
    assert_eq!(text(None), "");
}

// GH-004
#[test]
fn the_base_note_says_behind_or_conflicts() {
    let note = |state| {
        let mut p = pr(1, CiState::Success, vec![]);
        p.merge_state = state;
        let view = page(&p);
        (view.base_note, view.base_note_text)
    };
    assert_eq!(
        note(MergeState::Behind),
        (Some(BaseNote::Behind), "Behind main".to_owned())
    );
    assert_eq!(
        note(MergeState::Dirty),
        (Some(BaseNote::Conflicts), "Conflicts with main".to_owned())
    );
    assert_eq!(note(MergeState::Clean), (None, String::new()));
    assert_eq!(note(MergeState::Blocked), (None, String::new()));
}

// GH-003
#[test]
fn merge_says_why_it_is_disabled() {
    let reason = |edit: &dyn Fn(&mut PullRequest)| {
        let mut p = pr(1, CiState::Success, vec![]);
        edit(&mut p);
        merge_reason(&p)
    };
    assert_eq!(reason(&|_| {}), None);
    assert_eq!(
        reason(&|p| p.is_draft = true).as_deref(),
        Some("Drafts can't be merged")
    );
    assert_eq!(
        reason(&|p| p.ci = CiState::Pending).as_deref(),
        Some("Waiting for the checks")
    );
    assert_eq!(
        reason(&|p| p.needs_approval = true).as_deref(),
        Some("The runs need approval")
    );
    assert_eq!(
        reason(&|p| p.merge_state = MergeState::Behind).as_deref(),
        Some("main is ahead")
    );
    assert_eq!(
        reason(&|p| p.merge_state = MergeState::Dirty).as_deref(),
        Some("Conflicts with main")
    );
    assert_eq!(
        reason(&|p| {
            p.merge_state = MergeState::Blocked;
            p.review = Some(ReviewDecision::ReviewRequired);
        })
        .as_deref(),
        Some("Needs an approving review")
    );
    assert_eq!(
        reason(&|p| {
            p.merge_state = MergeState::Blocked;
            p.review = Some(ReviewDecision::ChangesRequested);
        })
        .as_deref(),
        Some("Needs an approving review")
    );
    assert_eq!(
        reason(&|p| {
            p.merge_state = MergeState::Blocked;
            p.review = Some(ReviewDecision::Approved);
        })
        .as_deref(),
        Some("Blocked by a branch rule")
    );
    assert_eq!(
        reason(&|p| p.merge_state = MergeState::Blocked).as_deref(),
        Some("Blocked by a branch rule")
    );
    assert_eq!(
        reason(&|p| p.ci = CiState::None).as_deref(),
        Some("There are no checks")
    );
}

// GH-003
#[test]
fn merge_is_enabled_for_a_mergeable_state_and_the_failed_checks_count() {
    for state in [
        MergeState::Clean,
        MergeState::Unstable,
        MergeState::HasHooks,
        MergeState::Unknown,
    ] {
        let mut p = pr(1, CiState::Success, vec![]);
        p.merge_state = state;
        let view = page(&p);
        assert!(view.merge.enabled, "{state:?}");
        assert!(view.merge.visible);
        assert_eq!(view.merge_reason, "");
        assert!(!view.merge_anyway.visible);
    }
    let failing = pr(
        1,
        CiState::Failure,
        vec![
            job("a", JobState::Failed),
            job("b", JobState::Failed),
            job("c", JobState::Passed),
        ],
    );
    let view = page(&failing);
    assert!(!view.merge.enabled);
    assert_eq!(view.merge_reason, "2 checks failed");
    assert!(view.merge_anyway.visible);
    assert!(view.merge_anyway.enabled);
    assert_eq!(view.merge_anyway_confirm, "2 checks failed. Merge anyway?");
    assert_eq!(view.merge_confirm, "Rebase feature-1 onto main and merge?");

    let one = pr(1, CiState::Failure, vec![job("a", JobState::Failed)]);
    assert_eq!(
        page(&one).merge_anyway_confirm,
        "1 check failed. Merge anyway?"
    );
    // A failing rollup without a job we know of.
    assert_eq!(
        page(&pr(1, CiState::Failure, vec![])).merge_anyway_confirm,
        "The checks failed. Merge anyway?"
    );
    // Not for a draft.
    let mut d = pr(1, CiState::Failure, vec![job("a", JobState::Failed)]);
    d.is_draft = true;
    assert!(!page(&d).merge_anyway.visible);
    assert!(!page(&pr(1, CiState::Pending, vec![])).merge_anyway.visible);
}

// GH-004
#[test]
fn update_branch_shows_only_when_behind() {
    let mut p = pr(1, CiState::Success, vec![]);
    assert!(!page(&p).update_branch.visible);
    p.merge_state = MergeState::Behind;
    let view = page(&p);
    assert!(view.update_branch.visible);
    assert!(view.update_branch.enabled);
    p.merge_state = MergeState::Dirty;
    assert!(!page(&p).update_branch.visible);
}

// GH-005
#[test]
fn rerun_failed_shows_when_a_job_failed() {
    let none = pr(1, CiState::Success, vec![job("a", JobState::Passed)]);
    assert!(!page(&none).rerun_failed.visible);
    let cancelled = pr(1, CiState::Pending, vec![job("a", JobState::Cancelled)]);
    assert!(!page(&cancelled).rerun_failed.visible);
    let failed = pr(1, CiState::Failure, vec![job("a", JobState::Failed)]);
    assert!(page(&failed).rerun_failed.visible);
    assert_eq!(page(&failed).rerun_failed.label, "Rerun failed jobs");
}

// GH-006
#[test]
fn the_banner_and_approve_button_show_while_runs_wait() {
    let mut p = pr(1, CiState::None, vec![]);
    let view = page(&p);
    assert_eq!(view.approval_text, "");
    assert!(!view.approve.visible);
    p.needs_approval = true;
    let view = page(&p);
    assert_eq!(view.approval_text, "Runs are waiting for your approval");
    assert!(view.approve.visible);
    assert_eq!(view.approve.label, "Approve runs");
    assert_eq!(view.checks.heading, "Runs waiting for approval");
    assert_eq!(view.checks.bar, [Segment::Waiting]);
    assert!(view.checks.groups.is_empty());
    assert_eq!(view.checks.tally, "");
}

// GH-008
#[test]
fn notify_shows_only_while_the_checks_run() {
    let running = page(&pr(1, CiState::Pending, vec![job("a", JobState::Running)]));
    assert!(running.notify.visible);
    assert_eq!(running.notify.label, "Notify me when done");
    assert!(!running.notifying);
    for ci in [CiState::Success, CiState::Failure, CiState::None] {
        assert!(!page(&pr(1, ci, vec![])).notify.visible);
    }
}

// GH-003
#[test]
fn a_running_command_makes_its_button_busy_and_disables_the_others() {
    let key = PrKey::new("o/r", 1);
    let mut p = pr(
        1,
        CiState::Failure,
        vec![job("a", JobState::Failed), job("bb", JobState::Failed)],
    );
    p.merge_state = MergeState::Behind;
    p.needs_approval = true;
    let mut cache = with_mine(vec![p]);
    command(
        &mut cache,
        &key,
        PrAction::UpdateBranch,
        1,
        CommandState::Started,
    );
    let view = cache.page(&key, now()).unwrap();
    assert!(view.update_branch.busy);
    assert!(!view.update_branch.enabled);
    for other in [
        &view.merge,
        &view.merge_anyway,
        &view.rerun_failed,
        &view.approve,
    ] {
        assert!(!other.enabled, "{}", other.label);
        assert!(!other.busy, "{}", other.label);
    }
    assert!(view.checks.groups[0].jobs.iter().all(|j| !j.rerun.enabled));
    assert_eq!(view.status, Some(CommandStatus::Running));
    // Open on GitHub and the notify toggle are not commands: the URL stays.
    assert_eq!(view.url, "https://github.com/o/r/pull/1");

    command(
        &mut cache,
        &key,
        PrAction::UpdateBranch,
        1,
        CommandState::Done,
    );
    let view = cache.page(&key, now()).unwrap();
    assert!(!view.update_branch.busy);
    assert!(view.update_branch.enabled);
    assert!(view.merge_anyway.enabled);
}

// GH-003
#[test]
fn merge_and_merge_anyway_are_busy_apart() {
    let key = PrKey::new("o/r", 1);
    let mut cache = with_mine(vec![
        pr(1, CiState::Success, vec![]),
        pr(2, CiState::Failure, vec![job("a", JobState::Failed)]),
    ]);
    command(
        &mut cache,
        &key,
        PrAction::Merge { force: false },
        1,
        CommandState::Started,
    );
    let view = cache.page(&key, now()).unwrap();
    assert!(view.merge.busy);
    assert!(!view.merge_anyway.busy);
    assert_eq!(view.merge_reason, "");

    let key2 = PrKey::new("o/r", 2);
    command(
        &mut cache,
        &key2,
        PrAction::Merge { force: true },
        2,
        CommandState::Started,
    );
    let view = cache.page(&key2, now()).unwrap();
    assert!(view.merge_anyway.busy);
    assert!(!view.merge.busy);
    // The other pull request is not held up by this one.
    assert!(cache.page(&key, now()).unwrap().merge.busy);
}

// GH-005
#[test]
fn rerunning_a_job_makes_its_row_busy() {
    let key = PrKey::new("o/r", 1);
    let failed = job("alpha", JobState::Failed);
    let id = failed.id.unwrap();
    let mut cache = with_mine(vec![pr(
        1,
        CiState::Failure,
        vec![failed, job("be", JobState::Failed)],
    )]);
    command(
        &mut cache,
        &key,
        PrAction::RerunJob { job_id: id },
        1,
        CommandState::Started,
    );
    let view = cache.page(&key, now()).unwrap();
    let rows = &view.checks.groups[0].jobs;
    assert!(rows[0].rerun.busy);
    assert!(!rows[1].rerun.busy);
    assert!(!rows[1].rerun.enabled);
    assert!(!view.rerun_failed.enabled);

    command(
        &mut cache,
        &key,
        PrAction::RerunJob { job_id: id },
        1,
        CommandState::Done,
    );
    assert_eq!(
        cache.command(&key).unwrap().status,
        CommandStatus::Done("Rerunning alpha".to_owned())
    );
}

// GH-003
#[test]
fn a_done_command_has_a_toast_and_a_failed_one_its_reason() {
    let key = PrKey::new("o/r", 1);
    let mut cache = with_mine(vec![pr(1, CiState::Success, vec![])]);
    let toast = |cache: &mut GithubCache, action, n| {
        command(cache, &key, action, n, CommandState::Done);
        match &cache.command(&key).unwrap().status {
            CommandStatus::Done(text) => text.clone(),
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(
        toast(&mut cache, PrAction::Merge { force: false }, 1),
        "Merged o/r #1"
    );
    assert_eq!(
        toast(&mut cache, PrAction::Merge { force: true }, 2),
        "Merged o/r #1"
    );
    assert_eq!(
        toast(&mut cache, PrAction::UpdateBranch, 3),
        "Updated feature-1. The checks run again."
    );
    assert_eq!(
        toast(&mut cache, PrAction::RerunFailed, 4),
        "Rerunning the failed jobs"
    );
    assert_eq!(
        toast(&mut cache, PrAction::RerunJob { job_id: 7 }, 5),
        "Rerunning the job"
    );
    assert_eq!(toast(&mut cache, PrAction::Approve, 6), "Approved the runs");

    command(
        &mut cache,
        &key,
        PrAction::Merge { force: false },
        7,
        CommandState::Failed(GithubError::MergeFailed("conflict".to_owned())),
    );
    let view = cache.page(&key, now()).unwrap();
    assert_eq!(
        view.status,
        Some(CommandStatus::Failed(
            "The merge failed: conflict".to_owned()
        ))
    );
    // A failed command leaves the buttons usable again.
    assert!(view.merge.enabled);

    cache.dismiss_command(&key);
    assert!(cache.page(&key, now()).unwrap().status.is_none());
}

// GH-003
#[test]
fn the_failure_reasons_are_sentences() {
    use text::error_text;
    assert_eq!(
        error_text(&GithubError::Unauthorized),
        "GitHub refused the token"
    );
    assert_eq!(
        error_text(&GithubError::Forbidden("no".to_owned())),
        "Not allowed: no"
    );
    assert_eq!(
        error_text(&GithubError::NotOpen("merged".to_owned())),
        "The pull request is already merged"
    );
    assert_eq!(
        error_text(&GithubError::ChecksNotPassed(CiState::Pending)),
        "The checks haven't passed"
    );
    assert_eq!(
        error_text(&GithubError::Partial {
            failed: 1,
            total: 3
        }),
        "1 of 3 requests failed"
    );
    assert_eq!(
        error_text(&GithubError::NothingToRerun),
        "There are no failed jobs to rerun"
    );
    assert_eq!(error_text(&GithubError::Other("x".to_owned())), "x");
}

// GH-003
#[test]
fn a_late_answer_of_an_older_attempt_is_ignored() {
    let key = PrKey::new("o/r", 1);
    let mut cache = with_mine(vec![pr(1, CiState::Success, vec![])]);
    let action = PrAction::RerunFailed;
    command(&mut cache, &key, action, 1, CommandState::Started);
    command(
        &mut cache,
        &key,
        action,
        1,
        CommandState::Failed(GithubError::Unreachable),
    );
    command(&mut cache, &key, action, 2, CommandState::Started);
    assert_eq!(cache.command(&key).unwrap().status, CommandStatus::Running);

    // Attempt 1 answers again, late: ignored, and so is its start.
    let applied = cache.apply(&GithubEvent::Command {
        pr: key.clone(),
        action,
        attempt: attempt(1),
        state: CommandState::Done,
    });
    assert_eq!(applied, Applied::Nothing);
    command(&mut cache, &key, action, 1, CommandState::Started);
    assert_eq!(cache.command(&key).unwrap().attempt, attempt(2));
    assert_eq!(cache.command(&key).unwrap().status, CommandStatus::Running);

    // A start that comes after its own end doesn't bring the spinner back.
    command(&mut cache, &key, action, 2, CommandState::Done);
    command(&mut cache, &key, action, 2, CommandState::Started);
    assert!(matches!(
        cache.command(&key).unwrap().status,
        CommandStatus::Done(_)
    ));
}

// GH-003
#[test]
fn a_refused_command_changes_nothing() {
    let key = PrKey::new("o/r", 1);
    let mut cache = with_mine(vec![pr(1, CiState::Success, vec![])]);
    command(
        &mut cache,
        &key,
        PrAction::Approve,
        1,
        CommandState::Started,
    );
    let applied = cache.apply(&GithubEvent::Command {
        pr: key.clone(),
        action: PrAction::UpdateBranch,
        attempt: attempt(1),
        state: CommandState::Refused,
    });
    assert_eq!(applied, Applied::Nothing);
    assert_eq!(cache.command(&key).unwrap().action, PrAction::Approve);
    assert_eq!(cache.command(&key).unwrap().status, CommandStatus::Running);
    // A toast stays until dismissed; a running command is not dismissed.
    cache.dismiss_command(&key);
    assert!(cache.command(&key).is_some());
}

// GH-001
#[test]
fn the_checks_section_tallies_groups_and_folds() {
    let mut failed = job("lint", JobState::Failed);
    failed.started_at = Some(at(600));
    failed.completed_at = Some(at(600 - 48));
    let mut running = job("build", JobState::Running);
    running.started_at = Some(at(400));
    let mut passed = job("test", JobState::Passed);
    passed.started_at = Some(at(1000));
    passed.completed_at = Some(at(1000 - 552));
    let mut status = job("ext", JobState::Passed);
    status.id = None;
    status.workflow = None;
    status.url = None;
    let p = pr(
        1,
        CiState::Failure,
        vec![
            passed,
            running,
            job("cancel", JobState::Cancelled),
            failed,
            status,
        ],
    );
    let view = page(&p);
    let checks = &view.checks;
    assert_eq!(checks.heading, "Checks failed");
    assert_eq!(checks.kind, CiKind::Failed);
    assert_eq!(checks.tally, "1 failed, 1 running, 1 cancelled, 2 passed");
    // The tally by state, for a frontend that puts a swatch before each word.
    assert_eq!(
        checks
            .tally_parts
            .iter()
            .map(|part| (part.state, part.text.as_str()))
            .collect::<Vec<_>>(),
        [
            (JobState::Failed, "1 failed"),
            (JobState::Running, "1 running"),
            (JobState::Cancelled, "1 cancelled"),
            (JobState::Passed, "2 passed"),
        ]
    );
    assert_eq!(
        checks.bar,
        [
            Segment::Failed,
            Segment::Running,
            Segment::Cancelled,
            Segment::Passed,
            Segment::Passed
        ]
    );
    let titles: Vec<&str> = checks.groups.iter().map(|g| g.title.as_str()).collect();
    assert_eq!(titles, ["Failed 1", "Running 1", "Cancelled 1", "Passed 2"]);
    let folded: Vec<bool> = checks.groups.iter().map(|g| g.folded).collect();
    assert_eq!(folded, [false, false, true, true]);

    // The rows run in the bar's order, through the groups.
    let indices: Vec<usize> = checks
        .groups
        .iter()
        .flat_map(|g| g.jobs.iter().map(|j| j.index))
        .collect();
    assert_eq!(indices, [0, 1, 2, 3, 4]);

    let lint = &checks.groups[0].jobs[0];
    assert_eq!(lint.workflow_prefix, "CI / ");
    assert_eq!(lint.name, "lint");
    assert_eq!(lint.duration, "48 s");
    assert!(lint.rerun.visible);
    assert!(lint.rerun.enabled);
    assert_eq!(
        lint.url.as_deref(),
        Some("https://github.com/o/r/runs/lint")
    );
    let build = &checks.groups[1].jobs[0];
    assert_eq!(build.duration, "6 m so far");
    assert!(!build.rerun.visible);
    let test = &checks.groups[3].jobs[0];
    assert_eq!(test.duration, "9 m 12 s");
    let ext = &checks.groups[3].jobs[1];
    assert_eq!(ext.workflow_prefix, "");
    assert_eq!(ext.duration, "");
    assert!(ext.url.is_none());
    assert!(!ext.rerun.visible);
}

// GH-001
#[test]
fn passed_jobs_are_open_when_everything_passed() {
    let p = pr(
        1,
        CiState::Success,
        vec![job("a", JobState::Passed), job("b", JobState::Passed)],
    );
    let checks = page(&p).checks;
    assert_eq!(checks.heading, "Checks passed");
    assert_eq!(checks.tally, "2 passed");
    assert!(!checks.groups[0].folded);

    let mixed = pr(
        1,
        CiState::Pending,
        vec![job("a", JobState::Passed), job("b", JobState::Running)],
    );
    let checks = page(&mixed).checks;
    assert_eq!(checks.heading, "Checks running");
    assert!(checks.groups[1].folded);
}

// GH-001
#[test]
fn a_failed_job_without_an_id_cannot_be_rerun_and_no_checks_say_so() {
    let mut external = job("ext", JobState::Failed);
    external.id = None;
    let view = page(&pr(1, CiState::Failure, vec![external]));
    assert!(!view.checks.groups[0].jobs[0].rerun.visible);
    // The pull request still has a failed job: the page's button shows, the service decides.
    assert!(view.rerun_failed.visible);

    let none = page(&pr(1, CiState::None, vec![]));
    assert_eq!(none.checks.heading, "No checks");
    assert!(none.checks.groups.is_empty());
    assert!(none.checks.bar.is_empty());
}

// GH-001
#[test]
fn the_page_is_gone_with_the_pull_request() {
    let mut cache = with_mine(vec![pr(1, CiState::Success, vec![])]);
    cache.apply(&GithubEvent::Mine(vec![ListChange::Removed { index: 0 }]));
    assert!(cache.page(&PrKey::new("o/r", 1), now()).is_none());
}

// --- runners ---

fn runner(name: &str, os: &str, online: bool, busy: bool) -> Runner {
    Runner {
        name: name.to_owned(),
        os: os.to_owned(),
        online,
        busy,
        labels: Vec::new(),
        job: None,
    }
}

// GH-007
#[test]
fn the_runner_footer_has_a_line_per_runner() {
    let mut cache = GithubCache::new();
    assert!(cache.runner_lines().is_empty());
    cache.apply(&GithubEvent::Runners(vec![
        RepoRunners {
            repo: "kdab/a".to_owned(),
            runners: vec![
                runner("box-1", "Linux", true, true),
                runner("box-2", "Linux", true, false),
            ],
        },
        RepoRunners {
            repo: "kdab/b".to_owned(),
            runners: vec![runner("box-3", "linux", false, true)],
        },
    ]));
    let lines: Vec<(String, String)> = cache
        .runner_lines()
        .into_iter()
        .map(|l| (l.name, l.status))
        .collect();
    let states: Vec<RunnerState> = cache.runner_lines().iter().map(|l| l.state).collect();
    assert_eq!(
        states,
        [RunnerState::Busy, RunnerState::Idle, RunnerState::Offline]
    );
    assert_eq!(
        lines,
        [
            (
                "box-1".to_owned(),
                "Linux runner, busy with kdab/a".to_owned()
            ),
            ("box-2".to_owned(), "idle".to_owned()),
            ("box-3".to_owned(), "offline".to_owned()),
        ]
    );
    cache.apply(&GithubEvent::Runners(Vec::new()));
    assert!(cache.runner_lines().is_empty());
}
