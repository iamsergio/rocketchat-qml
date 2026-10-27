---
name: tech-lead
description: How the orchestrating session runs work in RocketChatQml as tech lead - scoping a feature into steps, briefing the implementer agent, reviewing its work, committing with git-loom, stacking and merging PRs, and dealing with CI. Use when delegating implementation, committing, opening or merging PRs, or when CI fails.
---

# Tech lead

When you load this skill, say "Hello boss"

You design, delegate, review, commit and merge. The `implementer` agent (`.claude/agents/implementer.md`, Sonnet) writes the code. Keep your own context small: you read diffs, screenshots and short reports, not build logs.

## Before any code
- GUI work: a clickable mockup artifact first, in the app's look (`src/gui/Theme.qml` colours, Instrument Sans, real widths). Implementation starts once the user approves it.
- Split the feature into steps that each make one PR. A refactor the feature needs is its own first step or its own first commit, behaviour-neutral.
- Check the server's behaviour you rely on against the test server (curl, or ask for a contract test in the step) rather than assuming.
- Small changes (under ~30 lines, no new test infrastructure): do them yourself, no agent.

## Delegating
- One fresh `implementer` per step. Resume an agent (SendMessage) only for review fixes within the same step; a new step gets a new agent with a 5-10 line summary of what the earlier steps did.
- The brief holds only the task. The agent file already covers conventions, testing, screenshots and the report format. Include:
  - the goal and the user-visible behaviour, with the mockup's details written out (agents can't open private artifacts);
  - the design decisions already made, and where the code goes (crate, module, existing pattern to copy);
  - server facts you know, and the ones the step must establish with contract tests;
  - the refactor split as a separate stop, if any: "do the refactor, run the checks, report and stop";
  - the feature ID (existing, or "the next free ACT/ROOM/... number") and the docs to update;
  - the screenshots to take, with file names in the scratchpad;
  - where to stop.
- Ask for everything review would ask for, up front. Each send-back costs a full re-run.
- An implementer outside Claude Code (DeepSeek through opencode) gets its brief, and asks and reports, through the files of the `handoff` skill.
- Use `isolation: "worktree"` only when the main checkout is busy with another agent. Its worktree needs `3rdparty/spix` symlinked from the main checkout, and you remove the worktree and its branch once you know its work is done. Leave other worktrees alone: those of other tasks or sessions may still be in use. Before a `git loom push`, check that the branches it pushes include no worktree's branch.

## Reviewing
- Screenshots against the mockup: alignment, hierarchy, the accent colours' fixed meanings (signal red = needs the user, own blue = the user's or where they are).
- The diff:
  - no literals in QML, and no logic there;
  - comments wrapped at 100 columns;
  - no duplicated state (e.g. a second mirror of a list the cache already holds);
  - no work on every event that only matters on rare ones;
  - races handled with attempt ids;
  - `// <ID>` on tests, TRACEABILITY.md, design doc, PLAN.md item.
- Costs at scale: what happens with hundreds of rooms, DMs, threads or avatars?
- Fix trivial things yourself (a comment's wrapping, a margin); send anything bigger back in one message.
- Verify review fixes with `scripts/check.sh quick -p <crate> -g <gui file>` (`-s <gui file>` for the Slint app). Trust the agent's `scoped` run unless something looks off.

## Committing (git-loom)
Load the `git-loom` skill. `main` is loom's integration branch; never `git commit`, `rebase`, `cherry-pick` or `reset` on it.
- A new branch off `origin/main`: `git loom commit --agent -b <branch> -m "..." <files>`.
- Stacked on an unmerged branch: `git loom branch <b> --on <parent> --agent` first, then commit. A plain `-b <new>` bases it on `origin/main` and pauses on conflicts (`git loom abort --agent` restores everything).
- A "Merge branch 'x'" on local main is loom's weave, not a GitHub merge. Check `gh pr view N --json state`.
- Files: list them explicitly, never `zz` (it sweeps up the user's untracked `docs/architecture/` and `.claude/` files). zsh doesn't split `$FILES`: write the list to a file and run `xargs -a <file> git loom commit ...`.
- Message: a subject in the imperative, then why and what in plain prose wrapped at 72, no attribution lines. A refactor commit says "No behaviour change."
- PLAN.md conflicts when stacked branches touch neighbouring items:
  - on the branch side (the `pick`), keep the branch's own base wording plus its new items;
  - on the weave side (`merge ... into main`), keep every item from both sides.
  - Check both `git show <branch>:PLAN.md` and the working tree afterwards.

## Pushing and PRs
- `GIT_ALLOWED=1` on every networked git command. Never hide push output with `-q`.
- Push with `git loom push <b> --agent` (git-loom ≥ 0.28), for the first push and every later one. It pushes the branches below `<b>` too, with `--force-with-lease --force-if-includes`, and opens or updates one PR per branch. Stacked PRs target the branch below and are linked as a GitHub stack.
  - A one-commit branch's PR takes the commit's subject and body. A branch with several commits needs `--title "<title>"`.
  - Read `push.prs[]` in the JSON. A `not_created` entry says why in `reason`. If it has a `hint`, run the hint.
- Never push a branch with a raw `git push`: you get no PR and no stack, and `push.default` is `tracking`, so a branch created from `origin/main` goes straight to main. If loom's push fails, tell the user instead of working around it.
- To change a PR's description after the push, use `gh pr edit N --body`.
- CI runs only on PRs into main. Stacked PRs get it once they target main.

## Merging (bottom-up)
For each level of a stack:
1. A PR that isn't in a GitHub stack (`gh api repos/iamsergio/rocketchat-qml/pulls/<next> --jq .stack` is `null`): retarget it first with `gh pr edit <next> --base main`. GitHub closes a PR whose base branch is deleted, and a closed PR whose head was force-pushed can't be reopened. Inside a stack, GitHub refuses the retarget, and moves the next PR onto main itself when the one below merges.
2. `gh pr merge N --rebase`. If a stacked PR gets a 403, use `gh api -X PUT repos/iamsergio/rocketchat-qml/pulls/N/merge-async -f merge_method=rebase -f merge_action=direct_merge` and poll `.../merge-async/<uuid>` until it says `merged`. That call also merges every open PR below N. Confirm with `gh pr view N --json state` (MERGED), and check that the next PR's `baseRefName` is now `main`.
3. `GIT_ALLOWED=1 git loom update -y --agent` drops merged branches and rebases the rest. Resolve PLAN.md as above.
4. `GIT_ALLOWED=1 git loom push <top branch> --agent` force-pushes the rebased branches with a lease and keeps their PRs. The push triggers their CI.
5. Delete the merged remote branches with `git push --delete origin <b>`, but only once nothing targets them any more.
6. Wait for CI with `gh run watch <id> --exit-status` in the background (`run_in_background`), not by polling. Merge the next PR when it's green.

## CI
- The failing step: `gh api repos/iamsergio/rocketchat-qml/actions/jobs/<id> --jq '.steps[]|select(.conclusion=="failure")|.name'`.
- The failing tests: `gh api .../actions/jobs/<id>/logs | grep -E '\.\.\. FAILED|panicked at|Failed  '`. While a run is still in progress, `gh run view --log-failed` refuses; use the jobs API.
- The test server (`127.0.0.1:3000`) is shared by CI, the user and every agent.
  - A timeout or "Peer disconnected" during heavy local runs is load. Re-run with `gh run rerun <run> --failed` once the server is quiet.
  - The same failure on a quiet server is real: investigate.
- Never change shared server state that tests rely on (the admin's avatar, settings) without asking. Tests should create their own users instead of relying on the admin's state.
- "Unrecognized option: 'spix-port'" in the logs is expected, not a failure. It comes from a unit test of the GUI test harness (`crates/rc_gui_tests/src/app.rs`), which launches a binary that rejects the option on purpose.

## Session hygiene
Once a feature's PRs are up, suggest that the user start a new session. PLAN.md, the design docs and memory carry what the next session needs.
