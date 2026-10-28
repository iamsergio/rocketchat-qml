// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The rows of the `JobList` model without a Qt type: the job groups of the checks section as one
//! flat list, a heading per group followed by its jobs unless the group is folded.

use std::collections::HashMap;

use rc_core::github::JobState;
use rc_view::github::{JobGroupView, JobRowView};

/// The name QML knows a state by.
#[must_use]
pub const fn state_name(state: JobState) -> &'static str {
    match state {
        JobState::Failed => "failed",
        JobState::Running => "running",
        JobState::Cancelled => "cancelled",
        JobState::Passed => "passed",
    }
}

/// A group's name as the element names have it: `prJobGroup<Name>`.
#[must_use]
pub const fn group_name(state: JobState) -> &'static str {
    match state {
        JobState::Failed => "Failed",
        JobState::Running => "Running",
        JobState::Cancelled => "Cancelled",
        JobState::Passed => "Passed",
    }
}

/// The state a group name from QML stands for.
#[must_use]
pub fn state_of(name: &str) -> Option<JobState> {
    [
        JobState::Failed,
        JobState::Running,
        JobState::Cancelled,
        JobState::Passed,
    ]
    .into_iter()
    .find(|state| state_name(*state) == name)
}

/// A row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobRow {
    Group {
        state: JobState,
        title: String,
        open: bool,
    },
    Job(JobRowView),
}

impl JobRow {
    /// Whether both rows are the same item (the same group, the same job), whatever they say.
    #[must_use]
    pub fn same_item(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Group { state: a, .. }, Self::Group { state: b, .. }) => a == b,
            (Self::Job(a), Self::Job(b)) => a.index == b.index,
            _ => false,
        }
    }
}

/// Whether the group shows its jobs: what the user chose, else the view's default.
fn is_open(group: &JobGroupView, chosen: &HashMap<&'static str, bool>) -> bool {
    chosen
        .get(state_name(group.state))
        .copied()
        .unwrap_or(!group.folded)
}

/// The list of `groups`, with the jobs of the groups that are open.
#[must_use]
pub fn rows(groups: &[JobGroupView], chosen: &HashMap<&'static str, bool>) -> Vec<JobRow> {
    let mut rows = Vec::new();
    for group in groups {
        let open = is_open(group, chosen);
        rows.push(JobRow::Group {
            state: group.state,
            title: group.title.clone(),
            open,
        });
        if open {
            rows.extend(group.jobs.iter().cloned().map(JobRow::Job));
        }
    }
    rows
}

/// Where the heading of the group of `state` is, and how many job rows follow it when it is
/// open.
#[must_use]
pub fn group_position(
    groups: &[JobGroupView],
    chosen: &HashMap<&'static str, bool>,
    state: JobState,
) -> Option<(usize, usize)> {
    let mut at = 0;
    for group in groups {
        if group.state == state {
            return Some((at, group.jobs.len()));
        }
        at += 1;
        if is_open(group, chosen) {
            at += group.jobs.len();
        }
    }
    None
}

/// The state of the group the job `index` (its position in the bar) is in.
#[must_use]
pub fn group_of_job(groups: &[JobGroupView], index: usize) -> Option<JobState> {
    groups
        .iter()
        .find(|group| group.jobs.iter().any(|job| job.index == index))
        .map(|group| group.state)
}

/// Whether the group of `state` shows its jobs.
#[must_use]
pub fn group_is_open(
    groups: &[JobGroupView],
    chosen: &HashMap<&'static str, bool>,
    state: JobState,
) -> bool {
    groups
        .iter()
        .find(|group| group.state == state)
        .is_some_and(|group| is_open(group, chosen))
}

#[cfg(test)]
mod tests {
    use rc_view::github::Button;

    use super::*;

    fn job(index: usize, state: JobState) -> JobRowView {
        JobRowView {
            index,
            state,
            workflow_prefix: "CI / ".to_owned(),
            name: format!("job{index}"),
            duration: String::new(),
            rerun: Button {
                label: "Rerun",
                visible: false,
                enabled: true,
                busy: false,
            },
            job_id: None,
            url: None,
        }
    }

    fn group(state: JobState, folded: bool, jobs: Vec<JobRowView>) -> JobGroupView {
        JobGroupView {
            state,
            title: format!("{} {}", group_name(state), jobs.len()),
            count: jobs.len(),
            folded,
            jobs,
        }
    }

    fn groups() -> Vec<JobGroupView> {
        vec![
            group(JobState::Failed, false, vec![job(0, JobState::Failed)]),
            group(
                JobState::Passed,
                true,
                vec![job(1, JobState::Passed), job(2, JobState::Passed)],
            ),
        ]
    }

    // GH-001
    #[test]
    fn a_folded_group_shows_its_heading_only() {
        let rows = rows(&groups(), &HashMap::new());
        assert_eq!(rows.len(), 3);
        assert!(matches!(&rows[0], JobRow::Group { open: true, .. }));
        assert!(matches!(&rows[1], JobRow::Job(job) if job.index == 0));
        assert!(matches!(&rows[2], JobRow::Group { open: false, .. }));
    }

    // GH-001
    #[test]
    fn the_choice_of_the_user_beats_the_default() {
        let chosen = HashMap::from([("passed", true), ("failed", false)]);
        let rows = rows(&groups(), &chosen);
        assert_eq!(rows.len(), 4);
        assert!(matches!(&rows[0], JobRow::Group { open: false, .. }));
        assert!(matches!(&rows[3], JobRow::Job(job) if job.index == 2));
    }

    // GH-001
    #[test]
    fn a_group_is_found_by_its_state_and_by_one_of_its_jobs() {
        let groups = groups();
        let none = HashMap::new();
        assert_eq!(
            group_position(&groups, &none, JobState::Failed),
            Some((0, 1))
        );
        // The failed group is open, so the heading of the passed one comes after its job.
        assert_eq!(
            group_position(&groups, &none, JobState::Passed),
            Some((2, 2))
        );
        assert_eq!(group_position(&groups, &none, JobState::Running), None);
        assert_eq!(group_of_job(&groups, 2), Some(JobState::Passed));
        assert_eq!(group_of_job(&groups, 7), None);
        assert!(group_is_open(&groups, &none, JobState::Failed));
        assert!(!group_is_open(&groups, &none, JobState::Passed));
        assert_eq!(state_of("cancelled"), Some(JobState::Cancelled));
        assert_eq!(state_of("nope"), None);
    }

    // GH-001
    #[test]
    fn rows_are_the_same_item_by_group_or_job_index() {
        let a = JobRow::Job(job(1, JobState::Passed));
        let mut renamed = job(1, JobState::Passed);
        renamed.name = "other".to_owned();
        assert!(a.same_item(&JobRow::Job(renamed)));
        assert!(!a.same_item(&JobRow::Job(job(2, JobState::Passed))));
        let heading = JobRow::Group {
            state: JobState::Passed,
            title: "Passed 1".to_owned(),
            open: false,
        };
        assert!(!a.same_item(&heading));
    }
}
