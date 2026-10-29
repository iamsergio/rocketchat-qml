// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! A list of pull requests in display order, and the index-based changes that bring a frontend's
//! copy from one refresh to the next.

use std::collections::{HashMap, HashSet};

use super::domain::{PrKey, PullRequest, display_order};
use crate::list_change::ListChange;

/// The pull requests of one tab, as the frontend shows them.
#[derive(Default)]
pub(crate) struct PrList {
    shown: Vec<PullRequest>,
}

impl PrList {
    pub(crate) fn items(&self) -> &[PullRequest] {
        &self.shown
    }

    pub(crate) fn get(&self, key: &PrKey) -> Option<&PullRequest> {
        self.shown.iter().find(|pr| pr.key == *key)
    }

    /// Empties the list. `None` if it was empty already.
    pub(crate) fn clear(&mut self) -> Option<ListChange<PullRequest>> {
        if self.shown.is_empty() {
            return None;
        }
        self.shown.clear();
        Some(ListChange::Reset(Vec::new()))
    }

    /// Makes the list equal to `fresh`, whatever order it comes in, with the fewest changes: the
    /// removals first (the highest index first, so that the indices stay valid), then the moves,
    /// the inserts and the updates. A pull request that kept its place and its content costs
    /// nothing, so a refresh of hundreds of them that changed a few is a few changes. The first
    /// fill is a reset.
    pub(crate) fn sync(&mut self, mut fresh: Vec<PullRequest>) -> Vec<ListChange<PullRequest>> {
        fresh.sort_by(display_order);
        let mut seen = HashSet::new();
        fresh.retain(|pr| seen.insert(pr.key.clone()));
        if self.shown.is_empty() {
            if fresh.is_empty() {
                return Vec::new();
            }
            self.shown.clone_from(&fresh);
            return vec![ListChange::Reset(fresh)];
        }

        let wanted: HashSet<&PrKey> = fresh.iter().map(|pr| &pr.key).collect();
        let mut changes = Vec::new();
        for index in (0..self.shown.len()).rev() {
            if !wanted.contains(&self.shown[index].key) {
                self.shown.remove(index);
                changes.push(ListChange::Removed { index });
            }
        }

        // The ones that stay are in the right order among themselves, except for a few that
        // moved: the longest run already in order keeps its place, the others are moved.
        let old_index: HashMap<PrKey, usize> = self
            .shown
            .iter()
            .enumerate()
            .map(|(index, pr)| (pr.key.clone(), index))
            .collect();
        let kept: Vec<&PullRequest> = fresh
            .iter()
            .filter(|pr| old_index.contains_key(&pr.key))
            .collect();
        let order: Vec<usize> = kept.iter().map(|pr| old_index[&pr.key]).collect();
        let in_place = longest_increasing_run(&order);
        for (rank, pr) in kept.iter().enumerate() {
            if in_place[rank] {
                continue;
            }
            let from = self.position(&pr.key);
            self.shown.remove(from);
            // Right behind its predecessor, which is in place by now.
            let to = match rank.checked_sub(1) {
                Some(before) => self.position(&kept[before].key) + 1,
                None => 0,
            };
            self.shown.insert(to, (*pr).clone());
            changes.push(ListChange::Moved {
                from,
                to,
                item: (*pr).clone(),
            });
        }

        // What is left to add fits between the ones in place.
        for (index, pr) in fresh.iter().enumerate() {
            if self
                .shown
                .get(index)
                .is_some_and(|shown| shown.key == pr.key)
            {
                continue;
            }
            self.shown.insert(index, pr.clone());
            changes.push(ListChange::Inserted {
                index,
                item: pr.clone(),
            });
        }

        for (index, pr) in fresh.iter().enumerate() {
            if self.shown[index] != *pr {
                self.shown[index].clone_from(pr);
                changes.push(ListChange::Updated {
                    index,
                    item: pr.clone(),
                });
            }
        }
        changes
    }

    /// Only called for a key the list holds.
    fn position(&self, key: &PrKey) -> usize {
        self.shown
            .iter()
            .position(|pr| pr.key == *key)
            .unwrap_or_default()
    }
}

/// Which of the distinct numbers belong to a longest strictly increasing subsequence.
fn longest_increasing_run(numbers: &[usize]) -> Vec<bool> {
    // `tails[k]` is the index of the smallest number that ends an increasing run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; numbers.len()];
    for (index, &number) in numbers.iter().enumerate() {
        let length = tails.partition_point(|&tail| numbers[tail] < number);
        before[index] = length.checked_sub(1).map(|k| tails[k]);
        if length == tails.len() {
            tails.push(index);
        } else {
            tails[length] = index;
        }
    }
    let mut mask = vec![false; numbers.len()];
    let mut at = tails.last().copied();
    while let Some(index) = at {
        mask[index] = true;
        at = before[index];
    }
    mask
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use super::*;
    use crate::github::domain::{CiState, MergeState};

    fn pr(number: u64, minute: i64, draft: bool) -> PullRequest {
        PullRequest {
            key: PrKey::new("o/r", number),
            title: format!("pr {number}"),
            author: "me".to_owned(),
            url: format!("https://github.com/o/r/pull/{number}"),
            updated_at: Timestamp::from_second(minute * 60).unwrap(),
            is_draft: draft,
            review: None,
            merge_state: MergeState::Clean,
            head_branch: "b".to_owned(),
            base_branch: "main".to_owned(),
            head_sha: "abc".to_owned(),
            ci: CiState::Success,
            jobs: Vec::new(),
            needs_approval: false,
        }
    }

    fn numbers(list: &PrList) -> Vec<u64> {
        list.items().iter().map(|pr| pr.key.number).collect()
    }

    // GH-001
    #[test]
    fn the_first_fill_is_a_reset_in_display_order() {
        let mut list = PrList::default();
        let changes = list.sync(vec![pr(1, 5, true), pr(2, 1, false), pr(3, 9, false)]);
        // Ready ones first, newest change first, then the drafts.
        assert_eq!(numbers(&list), [3, 2, 1]);
        assert_eq!(changes, [ListChange::Reset(list.items().to_vec())]);
    }

    // GH-001
    #[test]
    fn an_empty_list_stays_quiet() {
        let mut list = PrList::default();
        assert!(list.sync(Vec::new()).is_empty());
    }

    // GH-001
    #[test]
    fn the_same_list_changes_nothing() {
        let mut list = PrList::default();
        list.sync(vec![pr(1, 5, false), pr(2, 1, false)]);
        assert!(list.sync(vec![pr(2, 1, false), pr(1, 5, false)]).is_empty());
    }

    // GH-001
    #[test]
    fn a_changed_ci_updates_the_row_in_place() {
        let mut list = PrList::default();
        list.sync(vec![pr(1, 5, false), pr(2, 1, false)]);
        let mut failed = pr(2, 1, false);
        failed.ci = CiState::Failure;
        let changes = list.sync(vec![pr(1, 5, false), failed.clone()]);
        assert_eq!(
            changes,
            [ListChange::Updated {
                index: 1,
                item: failed
            }]
        );
    }

    // GH-001
    #[test]
    fn a_changed_merge_state_updates_the_row_in_place() {
        let mut list = PrList::default();
        list.sync(vec![pr(1, 5, false)]);
        let mut behind = pr(1, 5, false);
        behind.merge_state = MergeState::Behind;
        assert_eq!(
            list.sync(vec![behind.clone()]),
            [ListChange::Updated {
                index: 0,
                item: behind.clone()
            }]
        );
        let mut approved = behind;
        approved.review = Some(crate::github::domain::ReviewDecision::Approved);
        assert_eq!(list.sync(vec![approved]).len(), 1);
    }

    // GH-001
    #[test]
    fn a_push_moves_the_row_to_the_top() {
        let mut list = PrList::default();
        list.sync(vec![pr(1, 5, false), pr(2, 4, false), pr(3, 3, false)]);
        let pushed = pr(3, 9, false);
        let changes = list.sync(vec![pr(1, 5, false), pr(2, 4, false), pushed.clone()]);
        assert_eq!(
            changes,
            [ListChange::Moved {
                from: 2,
                to: 0,
                item: pushed
            }]
        );
        assert_eq!(numbers(&list), [3, 1, 2]);
    }

    // GH-001
    #[test]
    fn a_row_that_falls_behind_is_one_move_too() {
        let mut list = PrList::default();
        list.sync(vec![pr(1, 9, false), pr(2, 8, false), pr(3, 7, false)]);
        // The first one becomes a draft: it goes behind the others.
        let draft = pr(1, 9, true);
        let changes = list.sync(vec![draft.clone(), pr(2, 8, false), pr(3, 7, false)]);
        assert_eq!(
            changes,
            [ListChange::Moved {
                from: 0,
                to: 2,
                item: draft
            }]
        );
        assert_eq!(numbers(&list), [2, 3, 1]);
    }

    // GH-001
    #[test]
    fn inserts_and_removals_use_indices_of_the_list_as_it_is_then() {
        let mut list = PrList::default();
        list.sync(vec![pr(1, 9, false), pr(2, 8, false), pr(3, 7, false)]);
        let new = pr(4, 8, false);
        let changes = list.sync(vec![pr(1, 9, false), new.clone(), pr(3, 7, false)]);
        // 2 goes (index 1), then 4 takes its place in the order.
        assert_eq!(
            changes,
            [
                ListChange::Removed { index: 1 },
                ListChange::Inserted {
                    index: 1,
                    item: new
                }
            ]
        );
    }

    // GH-001
    #[test]
    fn emptying_the_list_removes_the_rows_and_clear_resets() {
        let mut list = PrList::default();
        list.sync(vec![pr(1, 9, false), pr(2, 8, false)]);
        assert_eq!(
            list.sync(Vec::new()),
            [
                ListChange::Removed { index: 1 },
                ListChange::Removed { index: 0 }
            ]
        );
        list.sync(vec![pr(1, 9, false)]);
        assert_eq!(list.clear(), Some(ListChange::Reset(Vec::new())));
        assert_eq!(list.clear(), None);
    }

    // GH-001
    #[test]
    fn a_pull_request_listed_twice_counts_once() {
        let mut list = PrList::default();
        list.sync(vec![pr(1, 9, false), pr(1, 9, false)]);
        assert_eq!(numbers(&list), [1]);
    }

    // GH-001
    #[test]
    fn any_change_of_a_list_applies_to_a_copy_and_gives_the_new_order() {
        // A fixed pseudo-random walk over lists of up to 40 pull requests: whatever the changes
        // are, applying them to a copy must reproduce the list.
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |bound: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % bound
        };
        let mut list = PrList::default();
        let mut mirror: Vec<PullRequest> = Vec::new();
        for _ in 0..300 {
            let count = next(40);
            let fresh: Vec<PullRequest> = (0..count)
                .map(|_| {
                    let number = next(60);
                    let mut one = pr(number, i64::try_from(next(30)).unwrap(), next(4) == 0);
                    one.title = format!("v{}", next(2));
                    one
                })
                .collect();
            for change in list.sync(fresh) {
                change.apply(&mut mirror);
            }
            assert_eq!(mirror, list.items());
            let ordered = mirror
                .windows(2)
                .all(|w| display_order(&w[0], &w[1]).is_lt());
            assert!(ordered, "{:?}", numbers(&list));
        }
    }
}
