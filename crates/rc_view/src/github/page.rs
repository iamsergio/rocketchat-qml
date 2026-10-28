// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The page of one pull request: its facts, the buttons with which are shown, enabled or busy and
//! why one is disabled, and the checks section.

use jiff::Timestamp;
use rc_core::github::{
    CiState, Job, JobState, MergeState, PrAction, PrKey, PullRequest, ReviewDecision,
};

use super::cache::CommandStatus;
use super::row::{CiKind, Segment, check_bar, ci_kind, ordered_jobs};
use super::text;

/// A button of the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub label: &'static str,
    pub visible: bool,
    /// Off while a command runs on the pull request, and for Merge while it can't be merged.
    pub enabled: bool,
    /// Its command is the one that runs.
    pub busy: bool,
}

impl Button {
    fn new(label: &'static str, visible: bool, enabled: bool, busy: bool) -> Self {
        Self {
            label,
            visible,
            enabled,
            busy,
        }
    }
}

/// What the head is in relation to the base, when it matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseNote {
    Behind,
    Conflicts,
}

/// One job row of the checks section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRowView {
    /// Position among the jobs in check bar order: the segment `index` of the bar is this job,
    /// so that hovering one lights the other.
    pub index: usize,
    pub state: JobState,
    /// "<workflow> / ", empty for a job without a workflow.
    pub workflow_prefix: String,
    pub name: String,
    /// "48 s", "9 m 12 s", "6 m so far"; empty when the job has no times.
    pub duration: String,
    /// The Rerun button: shown for failed Actions jobs (those with an id).
    pub rerun: Button,
    pub job_id: Option<u64>,
    /// The job's log page, when GitHub gave one.
    pub url: Option<String>,
}

/// "Failed 2", "Running 1", "Cancelled 3", "Passed 26".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobGroupView {
    pub state: JobState,
    pub title: String,
    pub count: usize,
    /// Whether it starts closed: cancelled jobs always, passed ones unless everything passed.
    pub folded: bool,
    pub jobs: Vec<JobRowView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecksView {
    /// "Checks passed", "Checks failed", "Checks running", "Runs waiting for approval" or "No
    /// checks".
    pub heading: &'static str,
    pub kind: CiKind,
    /// "1 failed, 2 running, 26 passed"; empty without jobs.
    pub tally: String,
    pub bar: Vec<Segment>,
    /// The non-empty groups, failed first.
    pub groups: Vec<JobGroupView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrPageView {
    pub key: PrKey,
    pub repo: String,
    pub number: u64,
    pub url: String,
    /// "updated 3 min ago".
    pub updated: String,
    /// Set when the author isn't the user.
    pub author: Option<String>,
    pub title: String,
    /// "<head> into <base>".
    pub branches: String,
    /// The two branches alone, for a frontend that sets them apart.
    pub head_branch: String,
    pub base_branch: String,
    pub is_draft: bool,
    pub review: Option<ReviewDecision>,
    /// "Approved", "Changes requested" or "Review required"; empty without a decision.
    pub review_text: &'static str,
    pub base_note: Option<BaseNote>,
    /// "Behind <base>" or "Conflicts with <base>"; empty without a note.
    pub base_note_text: String,
    /// The banner above the checks, shown with the Approve runs button; empty when none.
    pub approval_text: &'static str,
    pub approve: Button,
    pub merge: Button,
    /// Why Merge is disabled; empty when it can be used (and while a command runs).
    pub merge_reason: String,
    /// The question Merge asks first.
    pub merge_confirm: String,
    pub merge_anyway: Button,
    pub merge_anyway_confirm: String,
    pub update_branch: Button,
    pub rerun_failed: Button,
    /// "Notify me when done" or "Notifying you"; only visible while the checks run.
    pub notify: Button,
    pub notifying: bool,
    /// How the last command on the pull request stands: busy, its reason, or its toast text.
    pub status: Option<CommandStatus>,
    pub checks: ChecksView,
}

/// The state of the page that doesn't come from the pull request.
pub(super) struct PageContext<'a> {
    pub login: Option<&'a str>,
    pub notifying: bool,
    /// The command that runs on the pull request, if one does.
    pub running: Option<PrAction>,
    pub status: Option<CommandStatus>,
    pub now: Timestamp,
}

fn failed_count(pr: &PullRequest) -> usize {
    pr.jobs
        .iter()
        .filter(|job| job.state == JobState::Failed)
        .count()
}

/// Why the pull request can't be merged, `None` when it can. The first rule that holds wins.
#[must_use]
pub fn merge_reason(pr: &PullRequest) -> Option<String> {
    let base = &pr.base_branch;
    if pr.is_draft {
        return Some("Drafts can't be merged".to_owned());
    }
    if pr.needs_approval {
        return Some("The runs need approval".to_owned());
    }
    match pr.ci {
        CiState::Pending => return Some("Waiting for the checks".to_owned()),
        CiState::Failure => return Some(text::checks_failed(failed_count(pr).max(1))),
        CiState::None => return Some("There are no checks".to_owned()),
        CiState::Success => {}
    }
    match pr.merge_state {
        MergeState::Behind => Some(format!("{base} is ahead")),
        MergeState::Dirty => Some(format!("Conflicts with {base}")),
        MergeState::Blocked => Some(
            if matches!(
                pr.review,
                Some(ReviewDecision::ReviewRequired | ReviewDecision::ChangesRequested)
            ) {
                "Needs an approving review".to_owned()
            } else {
                "Blocked by a branch rule".to_owned()
            },
        ),
        MergeState::Clean
        | MergeState::Unstable
        | MergeState::HasHooks
        | MergeState::Draft
        | MergeState::Unknown => None,
    }
}

fn review_text(review: Option<ReviewDecision>) -> &'static str {
    match review {
        Some(ReviewDecision::Approved) => "Approved",
        Some(ReviewDecision::ChangesRequested) => "Changes requested",
        Some(ReviewDecision::ReviewRequired) => "Review required",
        None => "",
    }
}

fn job_row(index: usize, job: &Job, ctx: &PageContext<'_>) -> JobRowView {
    let duration = match (job.state, job.started_at, job.completed_at) {
        (JobState::Running, Some(start), _) => {
            text::so_far(ctx.now.as_second() - start.as_second())
        }
        (_, None, _) | (_, _, None) => String::new(),
        (_, Some(start), Some(end)) => text::duration(end.as_second() - start.as_second()),
    };
    let can_rerun = job.state == JobState::Failed && job.id.is_some();
    let busy = job.id.is_some()
        && matches!(ctx.running, Some(PrAction::RerunJob { job_id }) if Some(job_id) == job.id);
    JobRowView {
        index,
        state: job.state,
        workflow_prefix: job
            .workflow
            .as_deref()
            .map_or_else(String::new, |workflow| format!("{workflow} / ")),
        name: job.name.clone(),
        duration,
        rerun: Button::new("Rerun", can_rerun, ctx.running.is_none(), busy),
        job_id: job.id,
        url: job.url.clone(),
    }
}

fn checks(pr: &PullRequest, ctx: &PageContext<'_>) -> ChecksView {
    let kind = ci_kind(pr);
    let heading = match kind {
        CiKind::Failed => "Checks failed",
        CiKind::WaitingApproval => "Runs waiting for approval",
        CiKind::Running => "Checks running",
        CiKind::Passed => "Checks passed",
        CiKind::NoChecks => "No checks",
    };
    let rows: Vec<JobRowView> = ordered_jobs(pr)
        .into_iter()
        .enumerate()
        .map(|(index, job)| job_row(index, job, ctx))
        .collect();
    let all_passed = !rows.is_empty() && rows.iter().all(|row| row.state == JobState::Passed);
    let mut groups = Vec::new();
    let mut tally = Vec::new();
    for (state, title, tally_word, folded) in [
        (JobState::Failed, "Failed", "failed", false),
        (JobState::Running, "Running", "running", false),
        (JobState::Cancelled, "Cancelled", "cancelled", true),
        (JobState::Passed, "Passed", "passed", !all_passed),
    ] {
        let jobs: Vec<JobRowView> = rows.iter().filter(|r| r.state == state).cloned().collect();
        if jobs.is_empty() {
            continue;
        }
        tally.push(format!("{} {tally_word}", jobs.len()));
        groups.push(JobGroupView {
            state,
            title: format!("{title} {}", jobs.len()),
            count: jobs.len(),
            folded,
            jobs,
        });
    }
    ChecksView {
        heading,
        kind,
        tally: tally.join(", "),
        bar: check_bar(pr),
        groups,
    }
}

pub(super) fn page_view(pr: &PullRequest, ctx: &PageContext<'_>) -> PrPageView {
    let idle = ctx.running.is_none();
    let busy = |action: PrAction| ctx.running == Some(action);
    let reason = merge_reason(pr);
    let can_merge = reason.is_none();
    let base = &pr.base_branch;
    let is_author = ctx
        .login
        .is_some_and(|login| login.eq_ignore_ascii_case(&pr.author));
    let (base_note, base_note_text) = match pr.merge_state {
        MergeState::Behind => (Some(BaseNote::Behind), format!("Behind {base}")),
        MergeState::Dirty => (Some(BaseNote::Conflicts), format!("Conflicts with {base}")),
        _ => (None, String::new()),
    };
    let failed = failed_count(pr);
    PrPageView {
        key: pr.key.clone(),
        repo: pr.key.repo.clone(),
        number: pr.key.number,
        url: pr.url.clone(),
        updated: format!("updated {} ago", text::age(ctx.now, pr.updated_at)),
        author: (!is_author).then(|| pr.author.clone()),
        title: pr.title.clone(),
        branches: format!("{} into {base}", pr.head_branch),
        head_branch: pr.head_branch.clone(),
        base_branch: pr.base_branch.clone(),
        is_draft: pr.is_draft,
        review: pr.review,
        review_text: review_text(pr.review),
        base_note,
        base_note_text,
        approval_text: if pr.needs_approval {
            "Runs are waiting for your approval"
        } else {
            ""
        },
        approve: Button::new(
            "Approve runs",
            pr.needs_approval,
            idle,
            busy(PrAction::Approve),
        ),
        merge: Button::new(
            "Merge",
            true,
            can_merge && idle,
            busy(PrAction::Merge { force: false }),
        ),
        merge_reason: if idle {
            reason.unwrap_or_default()
        } else {
            String::new()
        },
        merge_confirm: format!("Rebase {} onto {base} and merge?", pr.head_branch),
        merge_anyway: Button::new(
            "Merge anyway",
            !pr.is_draft && pr.ci == CiState::Failure,
            idle,
            busy(PrAction::Merge { force: true }),
        ),
        merge_anyway_confirm: if failed == 0 {
            "The checks failed. Merge anyway?".to_owned()
        } else {
            format!("{}. Merge anyway?", text::checks_failed(failed))
        },
        update_branch: Button::new(
            "Update branch",
            pr.merge_state == MergeState::Behind,
            idle,
            busy(PrAction::UpdateBranch),
        ),
        rerun_failed: Button::new(
            "Rerun failed jobs",
            failed > 0,
            idle,
            busy(PrAction::RerunFailed),
        ),
        notify: Button::new(
            if ctx.notifying {
                "Notifying you"
            } else {
                "Notify me when done"
            },
            pr.ci == CiState::Pending,
            true,
            false,
        ),
        notifying: ctx.notifying,
        status: ctx.status.clone(),
        checks: checks(pr, ctx),
    }
}
