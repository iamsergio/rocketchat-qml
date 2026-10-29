// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The drafts fold of a list. The service keeps the pull requests that are not drafts first and
//! the drafts after them, and the page shows a heading "Drafts n" between the two, with the drafts
//! under it only while it is open. A frontend that keeps the service's list as it is and shows
//! this has to turn each change of the list into the notifications of a list model whose rows
//! are the ready pull requests, the heading and, when open, the drafts. [`Fold::plan`] does that,
//! one change at a time, so that nothing resets for an incremental change. Plain Rust: the model
//! only carries out the steps in order, and the tests carry them out on a plain list.

use crate::row_ops::qt_move_destination;

/// A change of the service's list, as far as the fold cares: where it happens and whether the
/// item it concerns is a draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The whole list: `ready` pull requests that are not drafts, then `drafts` drafts.
    Reset {
        ready: usize,
        drafts: usize,
    },
    Inserted {
        index: usize,
        draft: bool,
    },
    /// Same position, new content; `draft` is the new state.
    Updated {
        index: usize,
        draft: bool,
    },
    /// `to` is the index once the move is done; `draft` is the item's new state.
    Moved {
        from: usize,
        to: usize,
        draft: bool,
    },
    Removed {
        index: usize,
    },
}

/// What the list model announces around an edit of its data, in rows of the folded list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Around {
    /// `beginResetModel` / `endResetModel`.
    Reset,
    /// `beginInsertRows(first, last)` / `endInsertRows`.
    Insert { first: usize, last: usize },
    /// `beginRemoveRows(first, last)` / `endRemoveRows`.
    Remove { first: usize, last: usize },
    /// `beginMoveRows(from, from, destination)` / `endMoveRows`; Qt's destination.
    Move { from: usize, destination: usize },
}

/// The edit of the service's list that goes between the begin and the end of [`Around`]. The item
/// to insert or set is the one of the change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Reset,
    Insert {
        index: usize,
    },
    Remove {
        index: usize,
    },
    Set {
        index: usize,
    },
    Move {
        from: usize,
        to: usize,
    },
    /// Only the fold opens or closes.
    None,
}

/// One notification, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub around: Option<Around>,
    pub edit: Edit,
    /// The fold once the step is done: the model takes it before it ends the notification.
    pub after: Fold,
    /// The row of the folded list whose content changed (`dataChanged`), counted after the step.
    pub changed: Option<usize>,
}

/// What a row of the folded list is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The pull request at this index of the service's list.
    Pull(usize),
    /// The heading "Drafts n".
    Heading,
}

/// How a list is folded now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fold {
    /// Rows `0..ready` of the service's list are not drafts.
    pub ready: usize,
    /// The drafts that follow them.
    pub drafts: usize,
    /// The drafts show under the heading.
    pub open: bool,
}

impl Fold {
    /// How many rows the folded list has.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.ready + self.heading_rows() + if self.open { self.drafts } else { 0 }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    const fn heading_rows(&self) -> usize {
        if self.drafts > 0 { 1 } else { 0 }
    }

    /// What the folded list has at `row`.
    #[must_use]
    pub const fn row(&self, row: usize) -> Row {
        if row < self.ready {
            Row::Pull(row)
        } else if row == self.ready && self.drafts > 0 {
            Row::Heading
        } else {
            Row::Pull(row - 1)
        }
    }

    /// The row of the pull request at `index` of the service's list, if it shows.
    #[must_use]
    pub const fn row_of(&self, index: usize) -> Option<usize> {
        if index < self.ready {
            Some(index)
        } else if self.open {
            Some(index + 1)
        } else {
            None
        }
    }

    /// The steps that follow `change`, which `self` takes in as it goes: afterwards it is the fold
    /// the last step says.
    pub fn plan(&mut self, change: Change) -> Vec<Step> {
        let mut steps = Vec::new();
        match change {
            Change::Reset { ready, drafts } => {
                self.ready = ready;
                self.drafts = drafts;
                steps.push(self.step(Some(Around::Reset), Edit::Reset, None));
            }
            Change::Inserted { index, draft } => self.insert(index, draft, &mut steps),
            Change::Removed { index } => self.remove(index, &mut steps),
            Change::Updated { index, draft } => {
                if (index >= self.ready) == draft {
                    let changed = self.row_of(index);
                    steps.push(self.step(None, Edit::Set { index }, changed));
                } else {
                    // Rare: the item crosses the heading while it stays at its index.
                    self.remove(index, &mut steps);
                    self.insert(index, draft, &mut steps);
                }
            }
            Change::Moved { from, to, draft } => self.moved(from, to, draft, &mut steps),
        }
        steps
    }

    /// The step that opens or closes the drafts under the heading. None if it is so already.
    pub fn set_open(&mut self, open: bool) -> Vec<Step> {
        if self.open == open {
            return Vec::new();
        }
        let (ready, drafts) = (self.ready, self.drafts);
        self.open = open;
        if drafts == 0 {
            return vec![self.step(None, Edit::None, None)];
        }
        let (first, last) = (ready + 1, ready + drafts);
        let around = if open {
            Around::Insert { first, last }
        } else {
            Around::Remove { first, last }
        };
        // The heading's chevron turns.
        vec![self.step(Some(around), Edit::None, Some(ready))]
    }

    fn step(&self, around: Option<Around>, edit: Edit, changed: Option<usize>) -> Step {
        Step {
            around,
            edit,
            after: *self,
            changed,
        }
    }

    fn insert(&mut self, index: usize, draft: bool, steps: &mut Vec<Step>) {
        let open = usize::from(self.open);
        let (around, changed) = if !draft {
            self.ready += 1;
            (
                Some(Around::Insert {
                    first: index,
                    last: index,
                }),
                None,
            )
        } else if self.drafts == 0 {
            // The heading comes with the first draft, and the draft under it if open.
            let first = self.ready;
            self.drafts = 1;
            (
                Some(Around::Insert {
                    first,
                    last: first + open,
                }),
                None,
            )
        } else {
            // The heading's count changes.
            self.drafts += 1;
            let around = self.open.then_some(Around::Insert {
                first: index + 1,
                last: index + 1,
            });
            (around, Some(self.ready))
        };
        steps.push(self.step(around, Edit::Insert { index }, changed));
    }

    fn remove(&mut self, index: usize, steps: &mut Vec<Step>) {
        let open = usize::from(self.open);
        let (around, changed) = if index < self.ready {
            self.ready -= 1;
            (
                Some(Around::Remove {
                    first: index,
                    last: index,
                }),
                None,
            )
        } else if self.drafts == 1 {
            // The heading goes with the last draft.
            let first = self.ready;
            self.drafts = 0;
            (
                Some(Around::Remove {
                    first,
                    last: first + open,
                }),
                None,
            )
        } else {
            self.drafts -= 1;
            let around = self.open.then_some(Around::Remove {
                first: index + 1,
                last: index + 1,
            });
            (around, Some(self.ready))
        };
        steps.push(self.step(around, Edit::Remove { index }, changed));
    }

    fn moved(&mut self, from: usize, to: usize, draft: bool, steps: &mut Vec<Step>) {
        if (from >= self.ready) != draft {
            self.remove(from, steps);
            self.insert(to, draft, steps);
            return;
        }
        // Within one section, so the fold keeps its shape.
        let rows = self.row_of(from).zip(self.row_of(to));
        let around = rows.map(|(from_row, to_row)| Around::Move {
            from: from_row,
            destination: qt_move_destination(from_row, to_row),
        });
        // The content may have changed along with the position.
        steps.push(self.step(around, Edit::Move { from, to }, rows.map(|rows| rows.1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a row of the folded list holds in the tests: the id of a pull request, or `None` for
    /// the heading.
    type Shown = Option<u32>;

    /// A model of the folded list, kept up to date from the steps alone, beside the service's
    /// list it is supposed to show.
    #[derive(Default)]
    struct Sim {
        /// Id and whether it is a draft.
        list: Vec<(u32, bool)>,
        /// The fold the model has taken from the last step.
        fold: Fold,
        /// The fold the planner is at, after all the changes it was given.
        planner: Fold,
        shown: Vec<Shown>,
    }

    impl Sim {
        fn expected(&self) -> Vec<Shown> {
            (0..self.fold.len())
                .map(|row| match self.fold.row(row) {
                    Row::Pull(index) => Some(self.list[index].0),
                    Row::Heading => None,
                })
                .collect()
        }

        /// Carries out the steps of `change` and checks that the rows the model kept are the ones
        /// the list says. `item` is what an insert, update or move brings, `all` what a reset does.
        fn apply(&mut self, change: Change, item: (u32, bool), all: &[(u32, bool)]) {
            let steps = self.planner.plan(change);
            for step in steps {
                // The placeholder is what an inserted row holds until the model asks for its data.
                let unset = Some(u32::MAX);
                match step.around {
                    Some(Around::Reset) | None => {}
                    Some(Around::Insert { first, last }) => {
                        for _ in first..=last {
                            self.shown.insert(first, unset);
                        }
                    }
                    Some(Around::Remove { first, last }) => {
                        self.shown.drain(first..=last);
                    }
                    Some(Around::Move { from, destination }) => {
                        // Qt refuses a move onto itself.
                        if destination != from && destination != from + 1 {
                            let row = self.shown.remove(from);
                            let at = if destination > from {
                                destination - 1
                            } else {
                                destination
                            };
                            self.shown.insert(at, row);
                        }
                    }
                }
                match step.edit {
                    Edit::Reset => self.list = all.to_vec(),
                    Edit::Insert { index } => self.list.insert(index, item),
                    Edit::Remove { index } => {
                        self.list.remove(index);
                    }
                    Edit::Set { index } => self.list[index] = item,
                    Edit::Move { from, to } => {
                        self.list.remove(from);
                        self.list.insert(to, item);
                    }
                    Edit::None => {}
                }
                self.fold = step.after;
                let expected = self.expected();
                if step.around == Some(Around::Reset) {
                    self.shown.clone_from(&expected);
                }
                // A changed row is asked for its data again, and so is an inserted one.
                for (row, shown) in self.shown.iter_mut().enumerate() {
                    if *shown == unset || Some(row) == step.changed {
                        *shown = expected[row];
                    }
                }
                assert_eq!(self.shown, expected, "after {change:?}: {step:?}");
            }
            assert_eq!(self.fold, self.planner, "{change:?}");
        }

        fn toggle(&mut self, open: bool) {
            let steps = self.planner.set_open(open);
            for step in steps {
                match step.around {
                    Some(Around::Insert { first, last }) => {
                        for _ in first..=last {
                            self.shown.insert(first, Some(u32::MAX));
                        }
                    }
                    Some(Around::Remove { first, last }) => {
                        self.shown.drain(first..=last);
                    }
                    _ => {}
                }
                self.fold = step.after;
                let expected = self.expected();
                for (row, shown) in self.shown.iter_mut().enumerate() {
                    if *shown == Some(u32::MAX) || Some(row) == step.changed {
                        *shown = expected[row];
                    }
                }
                assert_eq!(self.shown, expected, "opening {open}: {step:?}");
            }
        }

        fn reset(&mut self, all: &[(u32, bool)]) {
            let ready = all.iter().filter(|item| !item.1).count();
            self.apply(
                Change::Reset {
                    ready,
                    drafts: all.len() - ready,
                },
                (0, false),
                all,
            );
        }
    }

    fn ready(id: u32) -> (u32, bool) {
        (id, false)
    }

    fn draft(id: u32) -> (u32, bool) {
        (id, true)
    }

    // GH-001
    #[test]
    fn the_heading_comes_with_the_first_draft_and_goes_with_the_last() {
        let mut sim = Sim::default();
        sim.reset(&[ready(1), ready(2)]);
        assert_eq!(sim.shown, [Some(1), Some(2)]);

        // Closed: the heading alone.
        sim.apply(
            Change::Inserted {
                index: 2,
                draft: true,
            },
            draft(3),
            &[],
        );
        assert_eq!(sim.shown, [Some(1), Some(2), None]);

        // A second draft changes the heading and shows nowhere else.
        sim.apply(
            Change::Inserted {
                index: 3,
                draft: true,
            },
            draft(4),
            &[],
        );
        assert_eq!(sim.shown, [Some(1), Some(2), None]);

        sim.toggle(true);
        assert_eq!(sim.shown, [Some(1), Some(2), None, Some(3), Some(4)]);
        sim.apply(Change::Removed { index: 2 }, (0, false), &[]);
        assert_eq!(sim.shown, [Some(1), Some(2), None, Some(4)]);
        sim.apply(Change::Removed { index: 2 }, (0, false), &[]);
        assert_eq!(sim.shown, [Some(1), Some(2)]);

        // Open with no drafts: the first one comes with its heading.
        sim.apply(
            Change::Inserted {
                index: 0,
                draft: false,
            },
            ready(9),
            &[],
        );
        sim.apply(
            Change::Inserted {
                index: 3,
                draft: true,
            },
            draft(5),
            &[],
        );
        assert_eq!(sim.shown, [Some(9), Some(1), Some(2), None, Some(5)]);
        assert_eq!(sim.fold.drafts, 1);
    }

    // GH-001
    #[test]
    fn a_closed_fold_shows_no_draft_rows_and_a_move_among_them_nothing() {
        let mut sim = Sim::default();
        sim.reset(&[ready(1), draft(2), draft(3)]);
        assert_eq!(sim.shown, [Some(1), None]);
        let before = sim.planner.clone().plan(Change::Moved {
            from: 1,
            to: 2,
            draft: true,
        });
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].around, None);
    }

    // GH-001
    #[test]
    fn a_move_inside_a_section_is_one_move() {
        let mut sim = Sim::default();
        sim.reset(&[ready(1), ready(2), ready(3), draft(4), draft(5), draft(6)]);
        sim.toggle(true);
        let steps = sim.planner.clone().plan(Change::Moved {
            from: 2,
            to: 0,
            draft: false,
        });
        assert_eq!(
            steps.iter().map(|step| step.around).collect::<Vec<_>>(),
            [Some(Around::Move {
                from: 2,
                destination: 0
            })]
        );
        sim.apply(
            Change::Moved {
                from: 2,
                to: 0,
                draft: false,
            },
            ready(3),
            &[],
        );
        sim.apply(
            Change::Moved {
                from: 5,
                to: 3,
                draft: true,
            },
            draft(6),
            &[],
        );
        assert_eq!(
            sim.shown,
            [Some(3), Some(1), Some(2), None, Some(6), Some(4), Some(5)]
        );
    }

    // GH-001
    #[test]
    fn a_draft_made_ready_crosses_the_heading() {
        let mut sim = Sim::default();
        sim.reset(&[ready(1), draft(2), draft(3)]);
        sim.toggle(true);
        // Updated in place: first draft becomes the last ready one.
        sim.apply(
            Change::Updated {
                index: 1,
                draft: false,
            },
            ready(2),
            &[],
        );
        assert_eq!(sim.shown, [Some(1), Some(2), None, Some(3)]);
        // Moved to the top while it turns ready.
        sim.apply(
            Change::Moved {
                from: 2,
                to: 0,
                draft: false,
            },
            ready(3),
            &[],
        );
        assert_eq!(sim.shown, [Some(3), Some(1), Some(2)]);
    }

    /// A sequence of changes that keeps its own list, to compare the model with.
    struct Random(u64);

    impl Random {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(self.0 >> 33).unwrap() % n
        }
    }

    // GH-001
    #[test]
    fn random_changes_keep_the_model_in_step_with_the_list() {
        for seed in 0..40 {
            let mut random = Random(seed);
            let mut sim = Sim::default();
            let mut next = 0;
            sim.reset(&[]);
            for _ in 0..300 {
                let ready_count = sim.planner.ready;
                let len = sim.list.len();
                match random.below(7) {
                    0 | 1 => {
                        next += 1;
                        let is_draft = random.below(3) == 0;
                        let index = if is_draft {
                            ready_count + random.below(len - ready_count + 1)
                        } else {
                            random.below(ready_count + 1)
                        };
                        sim.apply(
                            Change::Inserted {
                                index,
                                draft: is_draft,
                            },
                            (next, is_draft),
                            &[],
                        );
                    }
                    2 if len > 0 => {
                        let index = random.below(len);
                        sim.apply(Change::Removed { index }, (0, false), &[]);
                    }
                    3 if len > 0 => {
                        let index = random.below(len);
                        let id = sim.list[index].0;
                        // Only the two items next to the heading can change side in place.
                        let flips = index + 1 == ready_count || index == ready_count;
                        let is_draft = (index >= ready_count) != (flips && random.below(2) == 0);
                        sim.apply(
                            Change::Updated {
                                index,
                                draft: is_draft,
                            },
                            (id, is_draft),
                            &[],
                        );
                    }
                    4 if len > 1 => {
                        let from = random.below(len);
                        let id = sim.list[from].0;
                        let crosses = random.below(4) == 0;
                        let was_draft = from >= ready_count;
                        let is_draft = was_draft != crosses;
                        // The list without the item, and the side the item ends on.
                        let others_ready = ready_count - usize::from(!was_draft);
                        let others = len - 1;
                        let to = if is_draft {
                            others_ready + random.below(others - others_ready + 1)
                        } else {
                            random.below(others_ready + 1)
                        };
                        if to != from || crosses {
                            sim.apply(
                                Change::Moved {
                                    from,
                                    to,
                                    draft: is_draft,
                                },
                                (id, is_draft),
                                &[],
                            );
                        }
                    }
                    5 => {
                        let open = !sim.planner.open;
                        sim.toggle(open);
                    }
                    6 if random.below(10) == 0 => {
                        let count = random.below(8);
                        let mut all: Vec<(u32, bool)> = (0..count)
                            .map(|_| {
                                next += 1;
                                (next, random.below(3) == 0)
                            })
                            .collect();
                        all.sort_by_key(|item| item.1);
                        sim.reset(&all);
                    }
                    _ => {}
                }
            }
        }
    }
}
