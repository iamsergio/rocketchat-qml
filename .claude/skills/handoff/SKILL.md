---
name: handoff
description: The file protocol between the lead (Claude) and an implementer that is not Claude (DeepSeek through opencode) in RocketChatQml - task directory, numbered messages, whose turn it is, when to ask and what a brief, question, answer, report and review must hold. Both sides load it before writing or reading a hand-off message. Not for the Claude implementer subagent.
---

# Hand-off protocol

Only for an implementer that is not Claude. The `implementer` subagent (Claude, spawned from Claude
Code) gets its brief as the agent prompt and reports in its reply: it doesn't load this skill, and
the lead doesn't use these files for it.

Two roles. The **lead** (Claude) designs, writes the brief, answers, reviews and commits. The
**implementer** (DeepSeek through opencode) implements in a worktree and reports. The user starts
each turn ("your turn"); the files say on what. The implementer also follows
`.claude/agents/implementer.md`, except where this file or the brief says otherwise.

## Files

One directory per task, outside the repo, never committed:

```
/tmp/handoff/<TASK>/
  STATE                       whose turn it is
  000-lead-brief.md           the single source of truth for the task
  001-implementer-question.md
  002-lead-answer.md
  003-implementer-report.md
  004-lead-review.md
```

- `<TASK>` is a short upper-case id (`ELIDE`). It also names the worktree
  (`/pub_data/sources/rocketchat-qml-<task>`, lower case) and the message ids (`ELIDE-003`).
- Messages are numbered in the order they're written and never edited afterwards, so "ELIDE-003"
  means one thing for good. The one exception is the brief: an answer that changes it edits it in
  place, marks the changed text `(amended by 002)` and lists the change.
- Each message starts with a header, then a blank line, then markdown:

  ```
  id: ELIDE-001
  from: implementer
  kind: question
  refs: ELIDE-000
  blocking: yes
  ```

- `STATE` holds two lines, rewritten by whoever just wrote a message:

  ```
  next: lead | implementer | user | done
  waiting-on: ELIDE-001
  ```

  `user` is for a decision that is the user's to make; `done` once the lead has committed.

## Messages

**brief** (lead):
- the goal and the user-visible behaviour;
- the diagnosis, split into *verified* (with how: a repro, a test, a source line) and *assumed*;
- what to read: repo files, skills, design docs, and what not to read;
- steps, with explicit stop points ("report and stop after step 2");
- must and must not, and the files the task may touch;
- the verification commands;
- anything the report needs beyond the format below.

**question** (implementer):
- what blocks, and which step;
- the evidence: the command, the failing lines of its output, `file:line`;
- the options and what each one leads to, then the one it recommends;
- what it did meanwhile: nothing, or which steps it carried on with because they don't depend on
  the answer.

**answer** (lead): the decision, the brief amendments (listed), and where to resume.

**report** (implementer), short, in this order:
1. what changed, one line per file;
2. decisions the brief didn't settle;
3. doubts, and anything the brief got wrong;
4. `git status --short --ignore-submodules=all` from the worktree;
5. the check summary lines (`scripts/check.sh full`, `ninja -C build-dev all_qmllint`, reruns);
6. screenshot paths.

**review** (lead): findings numbered for the next round, or "accepted".

No code listings in any message: name the file and line, quote only failing lines.

## When to ask

- Ask (a blocking question) when the brief contradicts itself or the repo, when a step would break a
  must-not or a check (INFRA-005, qmllint, a test that isn't yours), or when a step is impossible as
  written. Stop that step; carry on only with steps that don't depend on it, and say which.
- Decide yourself on names and details within the brief's rules, and list them in the report.
- Doubts that don't block go in the report, not in a question.
- "Impossible", "a Qt/Slint bug" or "blocked upstream" needs the evidence: the library source line
  or a minimal repro. Without it, it's a question, not a conclusion.
- Never work around a guard the brief set ("if X complains, stop"). Ask.

## Before sending a brief (lead)

- Reproduce the bug, or check the server behaviour the task relies on.
- Dry-run every claim about tooling the brief makes: the test list a new test lands in and what
  INFRA-005 says about it, the `check.sh` flags, the worktree recipe.
- Mark what you didn't check as *assumed*. A wrong brief costs a full round.

## Worktree

The lead creates it (or gives the user the command) before writing `STATE`:

```sh
cd /pub_data/sources/rocketchat-qml
git loom wt new <task>          # /pub_data/sources/rocketchat-qml-<task> on integration-<task>
cd /pub_data/sources/rocketchat-qml-<task>
rmdir 3rdparty/spix && ln -s /pub_data/sources/rocketchat-qml/3rdparty/spix 3rdparty/spix
git submodule update --init 3rdparty/corrosion 3rdparty/cxx-qt-cmake 3rdparty/vcpkg
```

- With the spix symlink a plain `git status` fails; use `git status --short --ignore-submodules=all`.
- The first `scripts/check.sh` run there builds `build-dev` (and `build-dev-slint`) from scratch.
- The implementer works only in the worktree and runs no git writes: no commit, stash, checkout,
  branch or `git loom`. The symlink is never committed.
- The lead commits per the tech-lead skill. Once the task is done (committed, nothing more goes
  back to the implementer), the lead drops its worktree (`git loom wt drop`) and deletes the task
  directory. Other worktrees stay: those of other tasks or sessions may still be in use.
- Before a `git loom push`, check that the branches it pushes (the one named and those below it)
  include no worktree's branch.
