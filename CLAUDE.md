# RocketChatQml

- Plan and progress: [PLAN.md](PLAN.md). Design: [ARCHITECTURE.md](ARCHITECTURE.md). Features and their tests: [TRACEABILITY.md](TRACEABILITY.md).
- cxx-qt limitations, our workarounds and possible upstream patches: [CXX-QT.md](CXX-QT.md). Add to it when you hit a new one.
- QML takes every size, spacing, margin, padding, border width, radius, font size, opacity and
  colour from `src/gui/Theme.qml` (or the Controls palette), so the GUI can be themed in one place.
  No literals, not even `0`, and don't lean on a default (layout spacing, control padding,
  indicator size): name it in Theme, with the default as its value if that's what you want.
  The same holds for `.slint` files, which take everything from `crates/rc_slint/ui/theme.slint`.
- Run `git push` (and other networked git commands) with `GIT_ALLOWED=1` set, so the git wrapper (`git-host.sh`) doesn't show its zenity confirmation popup.
