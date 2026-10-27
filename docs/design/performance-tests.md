# Performance tests

`rc_gui_perf` runs micro performance scenarios against the Qt app and the Slint app and writes one JSON file per run. It is a binary, not a `cargo test` target, and CI does not run it: the apps run visibly with GPU rendering on the developer's display. It drives the apps through Spix like the GUI tests (`rc_gui_tests`, `rc_test_server`) but runs only its own scenarios. `rc_gui_perf report` and `scripts/run_perf_test.sh` read the JSON files and the command line described here.

## How a run works

Every run is a fresh app launch under `perf stat`:

```
perf stat -D -1 --control fifo:<dir>/ctl,<dir>/ack -x, -o <dir>/perf.csv \
    -e instructions:u,branch-misses:u,ref-cycles:u,cycles:u,task-clock -- <app> --spix-port <port>
```

`-D -1` starts with the counters off. The runner logs in, opens the channel and loads everything with the counters off, then writes `enable` to the control FIFO and waits for perf's `ack`, plays the scenario, and writes `disable` and waits for `ack`. So login, setup and shutdown are not counted. Afterwards the app is quit through Spix and perf, which writes its CSV when the app exits, is read.

- **FIFO order.** perf opens its end of the acknowledgement FIFO for writing before it starts the app, and blocks until somebody reads it, while the harness waits for the app. So `Control::create` opens the reading end without blocking before the launch, and `Control::connect` opens the command end after it (it fails until perf has its end open, so it retries). perf ends its acknowledgement with a NUL byte, which is dropped.
- **Counters.** `:u` counts user space only, which `perf_event_paranoid` 2 allows without root. `instructions` is the stable metric; `ref-cycles` and `task-clock` are noisy. An event perf can't count (`<not supported>`, `<not counted>`, or no line) is recorded with that state and no value, never as 0. Without `perf` on `PATH` the runner stops with the reason.
- **Platforms.** `--platform wayland` (default) or `xcb`: Qt gets `QT_QPA_PLATFORM`; Slint gets `SLINT_BACKEND=winit-femtovg` (winit with the femtovg GPU renderer the app builds), and for `xcb` empty `WAYLAND_DISPLAY` and `WAYLAND_SOCKET`, as winit takes Wayland whenever it can. `--platform offscreen` (Qt's offscreen platform, Slint's testing backend, which draws nothing and has no frame count) is a smoke mode only: it checks the code path without a display and its numbers mean nothing. The JSON says `"smoke": true`.
- **Order.** Seeding happens first, once per invocation, before any app starts. The runs then go run by run across the selected apps, so drift of the machine falls on both alike.
- **What is inside the counters besides the scenario.** Qt's C++ Spix has a 10 ms timer in the GUI thread (`QtQmlBot`) that looks for commands, so the Qt app wakes 100 times a second also when idle. Slint's Spix server waits for connections on its own thread and has no timer. Both count frames from the first `frameCount` read, which is made just before `enable`: a `frameSwapped` handler in Qt, a rendering notifier in Slint. Each Spix call is also app work inside the counters, so the scenario keeps its calls few and slow (below).

## Command line

```
rc_gui_perf --list
rc_gui_perf [--qt <binary>] [--slint <binary>] [--platform wayland|xcb|offscreen] [--runs N] [--out <dir>] [scenario...]
```

`--list` prints `qt.scroll_main_channel`, `qt.drag_main_channel`, `slint.scroll_main_channel` and `slint.drag_main_channel`, one per line. A prefixed name runs that one, a bare name (`scroll_main_channel`) runs it in both apps, no name runs everything. The binaries come from `--qt` / `--slint` or `RC_PERF_QT_APP` / `RC_PERF_SLINT_APP`; a selected app without one is an error. `--runs` defaults to 5, `--out` to `target/perf`. The server is the one `RC_TEST_SERVER_URL` names.

The apps are built with the `perf` and `perf-slint` presets: RelWithDebInfo with Spix and without developer mode. Corrosion builds RelWithDebInfo with cargo's release profile (optimised, no debug info), so the presets turn on `ROCKETCHATQML_RUST_DEBUG_INFO`, which sets `CARGO_PROFILE_RELEASE_DEBUG=true` and leaves the code optimised with symbols, for flame graphs later. For a smoke run `build-dev` and `build-dev-slint` do.

## The scenario `scroll_main_channel`

**Content**, posted once per invocation and shared by both apps and every run (`seed.rs`): a fresh channel of 200 messages by three users with avatars (the viewer the apps log in as is a fourth). The text is a function of the message's index only, in a cycle of ten kinds: plain, markdown (bold, italic, strike, code, link), a code block, emoji (standard and a custom one, `perf_party`, which is created once and kept), a message with reactions, a thread parent with two replies (so its thread count shows; the replies are not in the room's list), an image upload (a generated 480x300 PNG that depends on the index), a quote and list, a long two-paragraph text, and a mixed one with a link, emoji and a reaction. Only ids, timestamps and the unique names the server needs differ between invocations.

**Before the counters:** log in, open the channel, press Home until the start of the conversation is in view (each press loads a page above), wait until the app's file cache has stopped growing (images and avatars are in), wheel back down until the list stops moving, and let it settle. Both apps use their default window, 964x600 logical px (`Theme.windowWidth` / `windowHeight`, `Theme.window-width` / `window-height`); the JSON has the size the window reports. The list must be longer than the distance below, or the run stops: the content would be too short to compare.

**Measured:** read the offset and the frame count, `enable`, wheel up in batches of 10 notches (20 ms apart, one `mouse_wheel` call per batch, so no round trip falls in the middle of a batch) and read the offset after each batch, until the list has moved `DISTANCE` = 8000 logical px (about 13 screens, under half of the roughly 20000 px list, so no run reaches the oldest page and loads history inside the counters); then poll the offset every 250 ms until two reads agree (the list glides on after the last notch, and the counters stay on until it stops), `disable`, read the offset and the frame count again.

The apps are compared over the same distance, not the same wheel input (that is what `drag_main_channel` below avoids): a notch is 60 px in Slint and about 30 in Qt. The report divides by the pixels scrolled and the frames drawn. The offset is `Gui::scroll_offset`. Qt's `contentY` is off by a constant once older messages were loaded above the view, so only differences between reads are distances, and the bottom is where the offset stops changing.

## The scenario `drag_main_channel`

The wheel scenario cannot compare frames or wall time between the apps: for the same input Qt glides (about 23 px a frame over 11.6 s) and Slint jumps 60 px a notch (about 99 px a frame over 3 s). Dragging the list's content moves both lists 1:1 with the pointer, so both cover the same distance in the same time, and both draw about the same number of frames on a real display: per-frame numbers (and wall time, frames, px per frame) compare directly, which the report says.

**Content and preparation** are those of `scroll_main_channel`: the same seeded channel and `main_channel::prepare`. The seeding is per content, not per scenario, so one invocation that runs both scenarios seeds one channel (`content_of` and `seed_for` in `main.rs`).

**Measured:** read the offset and the frame count, `enable`, then the number of strokes that drag `DISTANCE` (8000 px, the same constant): 22 of 370 px. Both apps get the same strokes rather than as many as each needs to move its list that far, so the input and its time are the same: Qt loses the first moves of each stroke before its list follows and would need about two strokes more. Each app's distance is measured and the report divides by it. A stroke presses at x = `MESSAGE_LIST_DRAG_X` (8 px, the gutter left of the avatars, where a press drags the list in both apps) and `MARGIN` = 40 px below the list's top, and drags straight down to 40 px above its bottom: `list height - 80` px, 370 px in the 450 px list both apps have (the length comes from the list's bounding box, read before `enable`, and the JSON has it). Down, because the content follows the pointer and shows older messages, as the wheel up does. It has 30 moves 16 ms apart (about 770 px/s, the same in both apps), a 250 ms hold (over the 100 ms after which Slint's flickable ignores the pointer's velocity, so the list does not flick on at release) and the release; a stroke lasts about 0.73 s. One `Gui::mouse_drag` call per stroke, then a property read, which in the Qt app waits for the stroke to end (the app only queues it), and 100 ms pass before the next stroke. Then the offset is polled every 250 ms until two reads agree, `disable`, read the offset and the frame count again.

The strokes are not the same count in both apps: Qt's Flickable loses about the first two moves of every stroke before the content follows, so it needs a stroke or two more for the same distance (24 against 22 in a smoke run); the wall time stays within a few %, as every stroke has the same duration.

## The JSON of a run

`<out>/<app>.<scenario>.<run>.json`, for example `slint.scroll_main_channel.0.json`:

| Field | |
|---|---|
| `schema` | 1 |
| `app`, `scenario`, `run` | `qt` or `slint`; the scenario name; the run index |
| `platform`, `smoke` | the `--platform` value; `true` for offscreen |
| `binary`, `git_revision` | absolute path of the binary; `git describe --always --dirty` of the checkout (tracked files only) |
| `window` | `{width, height}` as the app reports it, or null |
| `target_distance_px`, `distance_px` | the scenario's `DISTANCE`; what the offsets say was scrolled |
| `frames` | frames drawn in the measured part; null where the app can't count |
| `wheel_batches`, `wheel_notches` | what was sent, `scroll_main_channel` only |
| `drag_strokes`, `drag_length_px`, `drag_steps`, `drag_interval_ms`, `drag_hold_ms` | what was sent, `drag_main_channel` only: the strokes and the parameters of each |
| `wall_time_ms` | from `enable` to `disable`, including the wait for the list to stop |
| `counters` | one entry per event: `{event, state: "counted", value, unit, running_ns, running_pct}`, or `{event, state}` with `not_supported`, `not_counted` or `missing`. `running_pct` below 100 means perf multiplexed the counter and scaled it |

## Adding a scenario

1. Name it in `SCENARIOS` in `cli.rs`; `--list` and the selection follow.
2. Write `<scenario>.rs` with a `measure` that calls `main_channel::measure` with a function that plays the input until the list has moved `DISTANCE`, waits for the app to be at rest and returns the end offset and an `Input` (a new variant, with its fields in `report::to_json` and, if the report shows it, in `aggregate.rs` and `html.rs`). `main_channel::measure` reads the offset and frames, switches the counters and reads again. A scenario on other content also needs its own `prepare`.
3. A scenario on new content seeds it in `seed.rs`, deterministic in the index, and adds that content to `content_of` and `seed_for` in `main.rs`; one on the main channel only adds its name to `content_of`. Add the arm in `run_once`.
4. Keep Spix calls inside the counters few and slow, and say how slow.

## The script

`scripts/run_perf_test.sh` is what a developer runs: `--list`, scenario names, `--platform` (default `wayland`), `--runs`, and `--qt` / `--slint` to use a given binary (which is then not built; handy for a smoke run against `build-dev`). It first configures (`cmake --preset perf` / `perf-slint`, only if the build directory has no cache) and builds the apps the names select (only Qt for `qt.*`, both for a bare name or none) and `rc_gui_perf` in release mode; `--no-build` skips that. The run goes into `target/perf/<UTC timestamp>-<platform>/`, then `rc_gui_perf report` writes `report.html` there, against `target/perf/baseline` if that exists, and the script prints its path. `RC_TEST_SERVER_URL` defaults to `http://127.0.0.1:3000`.

**Baseline.** `--save-baseline` makes `target/perf/baseline` a symlink to the run's directory (not a copy: the directories are small, and the link shows which run it is). Compare runs of the same platform, on a quiet machine.

## The report

`rc_gui_perf report <dir> [--baseline <dir>] [-o <file>]` reads every `*.json` of the directory and writes one self-contained HTML file (default `<dir>/report.html`): inline CSS, no script, no external resource, light and dark through `prefers-color-scheme`. The aggregation is in `aggregate.rs` (plain functions, unit tested), the page in `html.rs`. Per scenario:

- a header: date (when the newest run file was written), git revision, platform, window size, runs per app; smoke runs get a banner saying the numbers mean nothing;
- the scroll facts per app (medians): distance, frames, px per frame, wall time, and wheel batches or, for `drag_main_channel`, drag strokes;
- the counters per app: the median per pixel scrolled and per frame drawn, each with its spread, (max - min) / median. A counter perf couldn't count, or per frame in an app without a frame count, says so instead of showing 0;
- with both apps, the Slint/Qt ratio per counter (per pixel and per frame) with a bar, 1 marked;
- with `--baseline`, the change of each median against the baseline's for the same app, scenario and counter, in %. Less is better, so a drop is better and a rise worse, but only when the change is larger than the two spreads added up; otherwise "within noise". A baseline without that app, scenario or counter shows "no baseline".
