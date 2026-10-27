# Performance tests

`rc_gui_perf` runs micro performance scenarios against the Qt app and the Slint app and writes one JSON file per run. It is a binary, not a `cargo test` target, and CI does not run it: the apps run visibly with GPU rendering on the developer's display. It drives the apps through Spix like the GUI tests (`rc_gui_tests`, `rc_test_server`) but runs only its own scenarios. The HTML report and `scripts/run_perf_test.sh` read the JSON files and the command line described here.

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

`--list` prints `qt.scroll_main_channel` and `slint.scroll_main_channel`, one per line. A prefixed name runs that one, a bare name (`scroll_main_channel`) runs it in both apps, no name runs everything. The binaries come from `--qt` / `--slint` or `RC_PERF_QT_APP` / `RC_PERF_SLINT_APP`; a selected app without one is an error. `--runs` defaults to 5, `--out` to `target/perf`. The server is the one `RC_TEST_SERVER_URL` names.

The apps are built with the `perf` and `perf-slint` presets: RelWithDebInfo with Spix and without developer mode. Corrosion builds RelWithDebInfo with cargo's release profile (optimised, no debug info), so the presets turn on `ROCKETCHATQML_RUST_DEBUG_INFO`, which sets `CARGO_PROFILE_RELEASE_DEBUG=true` and leaves the code optimised with symbols, for flame graphs later. For a smoke run `build-dev` and `build-dev-slint` do.

## The scenario `scroll_main_channel`

**Content**, posted once per invocation and shared by both apps and every run (`seed.rs`): a fresh channel of 200 messages by three users with avatars (the viewer the apps log in as is a fourth). The text is a function of the message's index only, in a cycle of ten kinds: plain, markdown (bold, italic, strike, code, link), a code block, emoji (standard and a custom one, `perf_party`, which is created once and kept), a message with reactions, a thread parent with two replies (so its thread count shows; the replies are not in the room's list), an image upload (a generated 480x300 PNG that depends on the index), a quote and list, a long two-paragraph text, and a mixed one with a link, emoji and a reaction. Only ids, timestamps and the unique names the server needs differ between invocations.

**Before the counters:** log in, open the channel, press Home until the start of the conversation is in view (each press loads a page above), wait until the app's file cache has stopped growing (images and avatars are in), wheel back down until the list stops moving, and let it settle. Both apps use their default window, 964x600 logical px (`Theme.windowWidth` / `windowHeight`, `Theme.window-width` / `window-height`); the JSON has the size the window reports. The list must be longer than the distance below, or the run stops: the content would be too short to compare.

**Measured:** read the offset and the frame count, `enable`, wheel up in batches of 10 notches (20 ms apart, one `mouse_wheel` call per batch, so no round trip falls in the middle of a batch) and read the offset after each batch, until the list has moved `DISTANCE` = 8000 logical px (about 13 screens, under half of the roughly 20000 px list, so no run reaches the oldest page and loads history inside the counters); then poll the offset every 250 ms until two reads agree (the list glides on after the last notch, and the counters stay on until it stops), `disable`, read the offset and the frame count again.

The apps are compared over the same distance, not the same wheel input: a notch is 60 px in Slint and about 30 in Qt. The report divides by the pixels scrolled and the frames drawn. The offset is `Gui::scroll_offset`. Qt's `contentY` is off by a constant once older messages were loaded above the view, so only differences between reads are distances, and the bottom is where the offset stops changing.

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
| `wheel_batches`, `wheel_notches` | what was sent |
| `wall_time_ms` | from `enable` to `disable`, including the wait for the list to stop |
| `counters` | one entry per event: `{event, state: "counted", value, unit, running_ns, running_pct}`, or `{event, state}` with `not_supported`, `not_counted` or `missing`. `running_pct` below 100 means perf multiplexed the counter and scaled it |

## Adding a scenario

1. Name it in `SCENARIOS` in `cli.rs`; `--list` and the selection follow.
2. Write `<scenario>.rs` with `prepare` (everything before the counters) and `measure` (read what is needed, `control.enable()`, the input, wait for the app to be at rest, `control.disable()`, read again), returning a measurement like `scroll_main_channel::Measured`.
3. Seed its content in `seed.rs`, deterministic in the index, and add the arms in `seed_for` and `run_once` in `main.rs`.
4. Keep Spix calls inside the counters few and slow, and say how slow.
