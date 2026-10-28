// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the GitHub models and view model are given, without a Qt type: the rows of the list as the
//! model keeps them, the changes of the service's list turned into changes of those rows, and the
//! state of the `Github` view model. The hub builds them from the `GithubCache` when an event comes,
//! so that a model never reads the cache, which may be ahead of the changes still queued to it.

use jiff::Timestamp;
use rc_core::github::{PrKey, PullRequest};
use rc_core::list_change::ListChange;
use rc_view::github::fold::Change;
use rc_view::github::{
    Button, CommandStatus, ConnectState, GithubCache, Glyph, PrPageView, PrRowView, Segment, Tab,
    text,
};
use rc_view::rich_html::escaped;

/// A row of the list, as the model keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub view: PrRowView,
    /// When the pull request changed, to show its age again later.
    pub updated_at: Timestamp,
    pub draft: bool,
}

impl Entry {
    /// The row of `pr`, a pull request of the list `tab`.
    #[must_use]
    pub fn new(cache: &GithubCache, tab: Tab, pr: &PullRequest, now: Timestamp) -> Self {
        Self {
            view: cache.view_of(tab, pr, now),
            updated_at: pr.updated_at,
            draft: pr.is_draft,
        }
    }

    /// "repo" on Mine, "repo, author" on Watched.
    #[must_use]
    pub fn repo_line(&self) -> String {
        if self.view.author.is_empty() {
            self.view.repo.clone()
        } else {
            format!("{}, {}", self.view.repo, self.view.author)
        }
    }

    /// The title as styled text, which the row follows with the number in its own colour.
    #[must_use]
    pub fn title_styled(&self) -> String {
        escaped(&self.view.title)
    }

    /// The key as the QML side names a pull request: `owner/name#number`.
    #[must_use]
    pub fn id(&self) -> String {
        pr_id(&self.view.key)
    }

    /// Shows the age again as of `now`. Returns whether it changed.
    pub fn refresh_age(&mut self, now: Timestamp) -> bool {
        let age = text::age(now, self.updated_at);
        let changed = age != self.view.age;
        self.view.age = age;
        changed
    }
}

/// `owner/name#number`.
#[must_use]
pub fn pr_id(key: &PrKey) -> String {
    format!("{}#{}", key.repo, key.number)
}

/// The name QML knows a glyph by.
#[must_use]
pub const fn glyph_name(glyph: Glyph) -> &'static str {
    match glyph {
        Glyph::Draft => "draft",
        Glyph::Failed => "failed",
        Glyph::WaitingApproval => "waiting",
        Glyph::Running => "running",
        Glyph::Passed => "passed",
        Glyph::NoChecks => "none",
    }
}

/// The name QML knows a segment of the check bar by.
#[must_use]
pub const fn segment_name(segment: Segment) -> &'static str {
    match segment {
        Segment::Failed => "failed",
        Segment::Running => "running",
        Segment::Cancelled => "cancelled",
        Segment::Passed => "passed",
        Segment::Waiting => "waiting",
    }
}

/// All the rows of the list `tab`.
#[must_use]
pub fn entries(cache: &GithubCache, tab: Tab, now: Timestamp) -> Vec<Entry> {
    cache
        .list(tab)
        .iter()
        .map(|pr| Entry::new(cache, tab, pr, now))
        .collect()
}

/// The changes of the service's list `tab` as changes of the rows, built against the cache as it
/// is now. A change the cache doesn't know of becomes a reset to the whole list, which is always
/// right.
#[must_use]
pub fn row_changes(
    cache: &GithubCache,
    tab: Tab,
    changes: &[ListChange<PullRequest>],
    now: Timestamp,
) -> Vec<ListChange<Entry>> {
    let entry = |pr: &PullRequest| Entry::new(cache, tab, pr, now);
    changes
        .iter()
        .map(|change| match change {
            ListChange::Reset(all) => ListChange::Reset(all.iter().map(entry).collect()),
            ListChange::Inserted { index, item } => ListChange::Inserted {
                index: *index,
                item: entry(item),
            },
            ListChange::Updated { index, item } => ListChange::Updated {
                index: *index,
                item: entry(item),
            },
            ListChange::Moved { from, to, item } => ListChange::Moved {
                from: *from,
                to: *to,
                item: entry(item),
            },
            ListChange::Removed { index } => ListChange::Removed { index: *index },
            _ => ListChange::Reset(entries(cache, tab, now)),
        })
        .collect()
}

/// How the fold sees a change of the rows.
#[must_use]
pub fn fold_change(change: &ListChange<Entry>) -> Change {
    match change {
        ListChange::Inserted { index, item } => Change::Inserted {
            index: *index,
            draft: item.draft,
        },
        ListChange::Updated { index, item } => Change::Updated {
            index: *index,
            draft: item.draft,
        },
        ListChange::Moved { from, to, item } => Change::Moved {
            from: *from,
            to: *to,
            draft: item.draft,
        },
        ListChange::Removed { index } => Change::Removed { index: *index },
        ListChange::Reset(all) => {
            let ready = all.iter().filter(|entry| !entry.draft).count();
            Change::Reset {
                ready,
                drafts: all.len() - ready,
            }
        }
        // Whatever the service adds later is a reset, as `row_changes` makes it.
        _ => Change::Reset {
            ready: 0,
            drafts: 0,
        },
    }
}

/// A button of the page as the view model shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Btn {
    pub visible: bool,
    pub enabled: bool,
    pub busy: bool,
}

impl From<&Button> for Btn {
    fn from(button: &Button) -> Self {
        Self {
            visible: button.visible,
            enabled: button.enabled,
            busy: button.busy,
        }
    }
}

/// What Merge or Merge anyway has asked the user, while it waits for the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub force: bool,
}

/// The page of the selected pull request: its header, its buttons and the top of the checks. The
/// job rows are the `JobList` model's.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageHead {
    pub id: String,
    pub repo: String,
    /// "#41".
    pub number: String,
    /// "updated 3 min ago".
    pub updated: String,
    pub title: String,
    pub head: String,
    pub base: String,
    pub review: String,
    pub base_note: String,
    pub draft: bool,
    pub url: String,
    /// The banner above the actions; empty when no run waits for approval.
    pub approval: String,
    pub approve: Btn,
    pub merge: Btn,
    /// Why Merge is disabled; empty when it isn't.
    pub merge_reason: String,
    pub merge_anyway: Btn,
    pub update_branch: Btn,
    pub rerun_failed: Btn,
    pub notify: Btn,
    pub notify_label: String,
    pub notifying: bool,
    /// "" when no confirmation is due, else "merge" or "merge-anyway".
    pub confirm_kind: String,
    pub confirm_text: String,
    pub heading: String,
    /// "1 failed, 2 running"; and the same by state: the names of the states and the words.
    pub tally: String,
    pub tally_states: Vec<String>,
    pub tally_texts: Vec<String>,
    /// The segments of the page's check bar.
    pub bar: Vec<String>,
}

impl PageHead {
    /// The page of `page`, with `confirming` (Merge, or Merge anyway if `force`) when it asks.
    #[must_use]
    pub fn new(page: &PrPageView, confirming: Option<Confirm>) -> Self {
        let (confirm_kind, confirm_text) = match confirming {
            None => (String::new(), String::new()),
            Some(Confirm { force: false }) => ("merge".to_owned(), page.merge_confirm.clone()),
            Some(Confirm { force: true }) => {
                ("merge-anyway".to_owned(), page.merge_anyway_confirm.clone())
            }
        };
        Self {
            id: pr_id(&page.key),
            repo: page.repo.clone(),
            number: format!("#{}", page.number),
            updated: page.updated.clone(),
            title: page.title.clone(),
            head: page.head_branch.clone(),
            base: page.base_branch.clone(),
            review: page.review_text.to_owned(),
            base_note: page.base_note_text.clone(),
            draft: page.is_draft,
            url: page.url.clone(),
            approval: page.approval_text.to_owned(),
            approve: Btn::from(&page.approve),
            merge: Btn::from(&page.merge),
            merge_reason: page.merge_reason.clone(),
            merge_anyway: Btn::from(&page.merge_anyway),
            update_branch: Btn::from(&page.update_branch),
            rerun_failed: Btn::from(&page.rerun_failed),
            notify: Btn::from(&page.notify),
            notify_label: page.notify.label.to_owned(),
            notifying: page.notifying,
            confirm_kind,
            confirm_text,
            heading: page.checks.heading.to_owned(),
            tally: page.checks.tally.clone(),
            tally_states: page
                .checks
                .tally_parts
                .iter()
                .map(|part| crate::job_rows::state_name(part.state).to_owned())
                .collect(),
            tally_texts: page
                .checks
                .tally_parts
                .iter()
                .map(|part| part.text.clone())
                .collect(),
            bar: page
                .checks
                .bar
                .iter()
                .map(|segment| segment_name(*segment).to_owned())
                .collect(),
        }
    }
}

/// What the `Github` view model shows. A field for each of its properties, flags and all.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmState {
    /// The GitHub screen is on.
    pub shown: bool,
    pub tile_connected: bool,
    pub tile_failing: i32,
    pub tile_pip: bool,
    pub tip_title: String,
    pub tip_line: String,
    pub connect: ConnectState,
    pub connect_reason: String,
    pub header_title: String,
    pub header_status: String,
    pub refreshing: bool,
    pub mine_count: i32,
    pub watched_count: i32,
    pub page: PageHead,
    /// Why the last command on the open pull request failed, empty when it didn't.
    pub command_error: String,
    /// What the last command on it did, to show for a moment; empty when none is due.
    pub toast: String,
}

impl Default for VmState {
    fn default() -> Self {
        Self {
            shown: false,
            tile_connected: false,
            tile_failing: 0,
            tile_pip: false,
            tip_title: String::new(),
            tip_line: String::new(),
            connect: ConnectState::NotConnected,
            connect_reason: String::new(),
            header_title: String::new(),
            header_status: String::new(),
            refreshing: false,
            mine_count: 0,
            watched_count: 0,
            page: PageHead::default(),
            command_error: String::new(),
            toast: String::new(),
        }
    }
}

/// Counts are `usize`, Qt's are `int`: saturate.
fn count(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

impl VmState {
    /// What the cache says, with the GitHub screen `shown` and the pull request `selected` open:
    /// `page` is its page (`None` once it is gone from the lists, e.g. merged). The result of the
    /// last command on the selected pull request is kept while the pull request is gone, so the
    /// toast of a merge shows.
    #[must_use]
    pub fn from_cache(
        cache: &GithubCache,
        shown: bool,
        selected: Option<&PrKey>,
        page: Option<&PrPageView>,
        confirming: Option<Confirm>,
        now: Timestamp,
    ) -> Self {
        let tile = cache.tile();
        let header = cache.header(now);
        let connect = cache.connect_view();
        let (mut command_error, mut toast) = (String::new(), String::new());
        match selected
            .and_then(|key| cache.command(key))
            .map(|command| &command.status)
        {
            Some(CommandStatus::Failed(reason)) => command_error.clone_from(reason),
            Some(CommandStatus::Done(text)) => toast.clone_from(text),
            Some(CommandStatus::Running) | None => {}
        }
        Self {
            shown,
            tile_connected: tile.connected,
            tile_failing: count(tile.failing),
            tile_pip: tile.pip,
            tip_title: tile.tooltip_title.to_owned(),
            tip_line: tile.tooltip,
            connect: connect.state,
            connect_reason: connect.reason,
            header_title: header.title.to_owned(),
            header_status: header.status,
            refreshing: header.refreshing,
            mine_count: count(header.mine_count),
            watched_count: count(header.watched_count),
            page: page
                .map(|page| PageHead::new(page, confirming))
                .unwrap_or_default(),
            command_error,
            toast,
        }
    }
}

#[cfg(test)]
mod tests {
    use rc_core::github::{
        CiState, Connection, GithubEvent, Job, JobState, MergeState, PrKey, PullRequest,
    };

    use super::*;

    fn at(seconds: i64) -> Timestamp {
        Timestamp::from_second(seconds).unwrap()
    }

    fn pr(number: u64, draft: bool) -> PullRequest {
        PullRequest {
            key: PrKey::new("o/r", number),
            title: format!("A <b> & {number}"),
            author: "ann".to_owned(),
            url: format!("https://github.com/o/r/pull/{number}"),
            updated_at: at(100),
            is_draft: draft,
            review: None,
            merge_state: MergeState::Clean,
            head_branch: "topic".to_owned(),
            base_branch: "main".to_owned(),
            head_sha: "abc".to_owned(),
            ci: CiState::Success,
            jobs: Vec::new(),
            needs_approval: false,
        }
    }

    fn cache_with(mine: Vec<PullRequest>) -> GithubCache {
        let mut cache = GithubCache::new();
        cache.apply(&GithubEvent::Connection(Connection::Connected {
            login: "me".to_owned(),
        }));
        cache.apply(&GithubEvent::Mine(vec![ListChange::Reset(mine)]));
        cache
    }

    // GH-001
    #[test]
    fn the_changes_of_the_list_become_changes_of_the_rows() {
        let list = vec![pr(1, false), pr(2, true)];
        let cache = cache_with(list.clone());
        let changes = row_changes(
            &cache,
            Tab::Mine,
            &[
                ListChange::Reset(list),
                ListChange::Moved {
                    from: 1,
                    to: 0,
                    item: pr(2, false),
                },
                ListChange::Removed { index: 1 },
            ],
            at(160),
        );
        assert!(matches!(&changes[0], ListChange::Reset(all) if all.len() == 2));
        assert!(matches!(&changes[1], ListChange::Moved { from: 1, to: 0, item } if !item.draft));
        assert_eq!(changes[2], ListChange::Removed { index: 1 });
        assert_eq!(
            fold_change(&changes[0]),
            Change::Reset {
                ready: 1,
                drafts: 1
            }
        );
        assert_eq!(
            fold_change(&changes[1]),
            Change::Moved {
                from: 1,
                to: 0,
                draft: false
            }
        );
    }

    // GH-001
    #[test]
    fn a_row_says_its_repository_and_on_watched_its_author_and_escapes_the_title() {
        let cache = cache_with(vec![pr(1, false)]);
        let mine = Entry::new(&cache, Tab::Mine, &pr(1, false), at(160));
        assert_eq!(mine.repo_line(), "o/r");
        assert_eq!(mine.title_styled(), "A &lt;b&gt; &amp; 1");
        assert_eq!(mine.id(), "o/r#1");
        let watched = Entry::new(&cache, Tab::Watched, &pr(1, false), at(160));
        assert_eq!(watched.repo_line(), "o/r, ann");
    }

    // GH-001
    #[test]
    fn the_age_of_a_row_is_shown_again_and_says_whether_it_changed() {
        let cache = cache_with(vec![pr(1, false)]);
        let mut entry = Entry::new(&cache, Tab::Mine, &pr(1, false), at(220));
        assert_eq!(entry.view.age, "2 min");
        assert!(!entry.refresh_age(at(225)));
        assert!(entry.refresh_age(at(280)));
        assert_eq!(entry.view.age, "3 min");
    }

    // GH-001
    #[test]
    fn the_state_of_the_view_model_follows_the_cache_and_the_selection() {
        let cache = cache_with(vec![pr(1, false)]);
        let key = PrKey::new("o/r", 1);
        let page = cache.page(&key, at(160));
        let state = VmState::from_cache(&cache, true, Some(&key), page.as_ref(), None, at(160));
        assert!(state.shown && state.tile_connected);
        assert_eq!(state.connect, ConnectState::Connected);
        assert_eq!(state.mine_count, 1);
        assert_eq!(state.page.id, "o/r#1");
        assert_eq!(state.page.number, "#1");
        assert_eq!(state.page.head, "topic");
        assert_eq!(state.page.base, "main");
        // A pull request that is gone from the lists has no page.
        let gone = PrKey::new("o/r", 9);
        let state = VmState::from_cache(&cache, true, Some(&gone), None, None, at(160));
        assert_eq!(state.page, PageHead::default());
    }

    // GH-003
    #[test]
    fn the_page_head_carries_the_buttons_the_question_and_the_tally() {
        let mut failing = pr(1, false);
        failing.ci = CiState::Failure;
        failing.jobs = vec![
            Job {
                id: Some(7),
                name: "build".to_owned(),
                workflow: Some("CI".to_owned()),
                state: JobState::Failed,
                url: None,
                started_at: None,
                completed_at: None,
            },
            Job {
                id: Some(8),
                name: "lint".to_owned(),
                workflow: None,
                state: JobState::Passed,
                url: None,
                started_at: None,
                completed_at: None,
            },
        ];
        let cache = cache_with(vec![failing]);
        let key = PrKey::new("o/r", 1);
        let page = cache.page(&key, at(160)).unwrap();

        let head = PageHead::new(&page, None);
        assert!(!head.merge.enabled);
        assert_eq!(head.merge_reason, "1 check failed");
        assert!(head.merge_anyway.visible && head.merge_anyway.enabled);
        assert!(head.rerun_failed.visible);
        assert!(!head.update_branch.visible && !head.notify.visible);
        assert_eq!(head.approval, "");
        assert_eq!(head.confirm_kind, "");
        assert_eq!(head.heading, "Checks failed");
        assert_eq!(head.tally, "1 failed, 1 passed");
        assert_eq!(head.tally_states, ["failed", "passed"]);
        assert_eq!(head.tally_texts, ["1 failed", "1 passed"]);
        assert_eq!(head.bar, ["failed", "passed"]);

        let head = PageHead::new(&page, Some(Confirm { force: false }));
        assert_eq!(head.confirm_kind, "merge");
        assert_eq!(head.confirm_text, "Rebase topic onto main and merge?");
        let head = PageHead::new(&page, Some(Confirm { force: true }));
        assert_eq!(head.confirm_kind, "merge-anyway");
        assert_eq!(head.confirm_text, "1 check failed. Merge anyway?");
    }

    // GH-003
    #[test]
    fn the_toast_of_a_command_outlives_the_pull_request_and_a_failure_is_the_error() {
        use rc_core::github::{AttemptId, CommandState, PrAction};
        let key = PrKey::new("o/r", 1);
        let mut cache = cache_with(vec![pr(1, false)]);
        let mut command = |attempt: u64, state: CommandState| {
            cache.apply(&GithubEvent::Command {
                pr: key.clone(),
                action: PrAction::Merge { force: false },
                attempt: AttemptId::new(attempt),
                state,
            });
        };
        command(1, CommandState::Started);
        command(1, CommandState::Done);
        // Merged: the next refresh drops it from the list.
        cache.apply(&GithubEvent::Mine(vec![ListChange::Reset(Vec::new())]));
        assert!(cache.page(&key, at(160)).is_none());
        let state = VmState::from_cache(&cache, true, Some(&key), None, None, at(160));
        assert_eq!(state.toast, "Merged o/r #1");
        assert_eq!(state.command_error, "");
        assert_eq!(state.page, PageHead::default());

        cache.dismiss_command(&key);
        let state = VmState::from_cache(&cache, true, Some(&key), None, None, at(160));
        assert_eq!(state.toast, "");
    }
}
