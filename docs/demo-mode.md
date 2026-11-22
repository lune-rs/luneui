# Demo mode

`apps/demo` plays its showcase tour hands-free: a script of touches, waits and marks, played against the app's own time and handed to the frame through `RawInput.events` (D415 to D419). The same events reach the screens whatever the frame rate, so a recording of the tour looks the same on a 60 Hz window, a 120 Hz panel and a simulator. Source: `crates/lune-ui-core/src/playback.rs` (the player, R-core-13; catalogue `docs/components/core/playback.md`), `apps/demo/src/showcase.rs` (the tour: `script`, `MARKS`, `Showcase::check`, `Demo::frame`), `apps/demo/src/lib.rs` (the window shell), `apps/demo/src/dump.rs` (the headless PNGs).

## What it is

- The player is in the toolkit's core and reads no clock. The app passes its time to `Player::advance`, which appends the events due by then to `RawInput.events` and returns the marks passed and the time it needs a frame at. The shell gives that time to `Ui::request_repaint_at`, so there is no second scheduling path.
- The script is the demo's own (`showcase::script`). It is built on the first frame from the window's size and safe area, and its coordinates come from the same `Layout` the screens use, so one script fits the desktop window (no insets) and the phone (safe area 0, 62, 0, 34).
- Nothing is drawn that an app would not draw: the tour is taps, swipes, drags and a pause on the tour's own state. The only extra is `playback::draw_touches`, the opt-in disc under each scripted touch (`--touches`), which uses public painter calls.
- Each mark names a state the app must be in (a tab, a nav depth, a sheet detent, a pager page, a list offset, a row action, a toggle, the slider). A mark is checked on the frame it passes, against the state that frame drew, and printed as `demo mark <name> ok`, or `demo mark <name> FAIL <reason>`. The script waits for the motion a mark names to come to rest before it.
- A touch sent as a sample every 8 ms (120 Hz) is the same list whatever the frame rate; a 60 Hz window sees every other sample, as a finger's events are reduced per frame.
- Time is app time: the tour's clock stops on `suspended` and resumes on `resumed`, so a background never replays a burst of the tour.

## Running it

The flags are the ones `apps/demo`'s usage line names:

| flag | with | what it does |
|---|---|---|
| `--demo` | | play the tour once, from the first frame |
| `--loop` | `--demo` | play it again when it ends (a restart starts from a fresh state) |
| `--touches` | `--demo` | draw a disc under each scripted touch |
| `--exit-after-demo` | `--demo`, not `--loop` | desktop: exit after `demo end`; exit code 1 when a mark failed |
| `--dump <dir>` | | headless, no window: five PNGs (`home`, `detail`, `sheet`, `cards`, `settings`) at 402x874 points, scale 3, the phone's safe area |
| `--dump <dir>` | `--demo`, optionally `--touches` | headless: one PNG per mark, `<mark>.png`, taken on the frame the mark passes |

Desktop (a 402x874 point window; `--exit-after-demo` for a machine check):

- `cargo run -p demo -- --demo` plays the tour and prints one line per mark, then `demo end <ok>/<total>` (16 marks).
- `cargo run -p demo -- --demo --exit-after-demo` does the same and exits 1 if a mark failed.
- `cargo run -p demo -- --demo --loop --touches` is the recording mode below.
- `cargo run -p demo -- --demo --dump <dir>` renders the tour's marks headless, no window.

Headless proof: `cargo test -p demo --test showcase` runs the same tour and checks with no window and no GPU, at 60 and 120 Hz of app time, with jittered frames and 100 ms hitches, with frames asked for only when due, and a repeating tour, on the desktop layout and the phone's.

iOS simulator: `scripts/ios-sim.sh --demo` builds the demo, runs it on the simulator device `luneui-demo-iphone` (iOS 18.0; `LUNEUI_IOS_RUNTIME=iOS-27-2` for 27.2, on `luneui-demo-iphone-iOS-27-2`) with `--demo --touches`, follows the log, takes three screenshots (`shot-1.png` to `shot-3.png`) at marks `sheet-half`, `page-2` and `settings`, and requires `demo end 16/16` in time. It exits 1 on a `demo mark ... FAIL` line, quoting it. Its `demo` lines go to `target/ios-sim/demo.txt`. `--demo-arg <arg>` passes one more argument to the demo. The script touches only its own device; it never opens Simulator.app and never records video.

Android emulator: `scripts/android-emu.sh --demo` builds the APK (`--build-only`), runs it on its own emulator `luneui-demo-pixel7` (AVD, port 5580) with `--demo --touches`, follows logcat's `RustStdoutStderr` tag, takes three screenshots (`shot-1.png` to `shot-3.png`) at the same marks and requires `demo end 16/16`. A `demo mark ... FAIL` line exits 1, quoting it. The `demo` lines go to `target/android-emu/demo.txt`. The tour's touches are `RawInput.events` as on the desktop; `--demo-arg <arg>` passes one more argument. On the M8 emulator run it passed 16/16 in 43 s (`docs/mobile.md`, "M8 reading: demo mark"). The script never records video and never runs `adb kill-server`.

## Recording

Agents never record. A recording is the user's.

On macOS: run `cargo run -p demo -- --demo --loop --touches` with the window (402x874 points) in view. Record that window with OBS (a window capture) or the system's screen recorder. Keep the pointer outside the window, so the only pointer on the recording is the touch discs. Stop the recording by hand; a loop never ends on its own.

On the simulator: run `scripts/ios-sim.sh --demo --keep-booted`, which leaves `luneui-demo-iphone` booted once the tour has run. Find the device's id with `xcrun simctl list devices | grep luneui-demo-iphone`. Then start the recording on that id, `xcrun simctl io <udid> recordVideo <file>`, and in a second shell relaunch the app with the loop: `xcrun simctl launch --terminate-running-process <udid> rs.lune.luneui.demo --demo --touches --loop`. Stop the recording with Ctrl-C. The run with `--keep-booted` leaves the device up; `scripts/ios-sim.sh --delete-device` deletes it when you have finished.

On the Android emulator: a recording is the user's, made with `adb shell screenrecord` on the emulator's serial (`adb -s emulator-5580 shell screenrecord /sdcard/demo.mp4`, stopped by Ctrl-C, then `adb pull`). `scripts/android-emu.sh --demo --keep-booted` leaves the emulator up after the tour, so the recording can start on it; agents never run `screenrecord`. The script runs one tour only; `--demo-arg --loop` would pass the loop flag to the app, unverified on Android.

## A script for your own app

What an app adds to play its own tour (`apps/demo/src/showcase.rs` is the reference). The sketch below is illustrative; the marks and points are the app's own:

1. Build the script on the first frame, from that frame's size and safe area. The coordinates come from the app's own layout function, so the script lands on the desktop and on a phone alike.

   ```rust
   use lune_ui::playback::{Player, Script};
   use std::time::Duration;

   let mut script = Script::new();
   script
       .wait(Duration::from_millis(600))
       .tap(layout.tab_item_center(1))
       .mark("cards")
       .swipe(layout.card_point(0.9, 0.5), layout.card_point(0.1, 0.5), Duration::from_millis(300))
       .mark("page-2");
   let mut player = Player::new(script).with_repeat(false);
   ```

   Builder calls: `tap(pos)` (80 ms down), `press(pos, hold)` (a long press when `hold` is past 500 ms), `swipe(from, to, d)` (a fling: it lifts at once), `drag(from, to, d)` (it holds still for 80 ms before the lift, so nothing flings), `wait(d)`, `event(e)` (any raw `Event` at the cursor), `mark(name)`, `end_hold(d)` (1.5 s by default, the hold after the last cue). Each gesture starts at the cursor and moves it to its end.
2. Each frame, in this order: the shell pushes the frame's insets (`Event::SafeArea`, `Event::KeyboardInset`) after `translator.take`; `player.advance(input.time, &mut input.events)`; `Ui::begin`; the screens; `playback::draw_touches(&mut ui, &player)` when on, last and at the top level; `ui.request_repaint_at(adv.next)` when it is `Some`. `demo`'s `Demo::frame` is the reference for this order.
3. Check the marks. For each `Advance.marks` name, compare the app's state with what the name says and print `demo mark <name> ok` or `... FAIL <reason>`; a failed check should fail the run (`--exit-after-demo` does this on the desktop; `ios-sim.sh --demo` does it on the simulator).
4. Pause on `suspended` (`player.pause(now)`) and resume on `resumed` (`player.resume(now)`), so a background replays no burst. The demo does this in its `suspended` and `resumed` handlers.
5. Touch must be on for the script's touches to reach the widgets: `translator.set_touch_enabled(true)` (D16), as `apps/demo` does in `setup`.
6. A repeating tour (`with_repeat(true)`) reports `Advance.restarted`; the app decides what a restart resets. The demo starts again from a fresh `Showcase`, keeping its toolkit store.

## Unverified here

- A real device: the tour's feel and frame rate on a 120 Hz panel, the GPU's cost, and a real finger. The simulator's frame rate was 60000 mHz, and the tour presented about 23 frames a second on average (`docs/mobile.md`, M7 reading).
- A live finger on the simulator: not driven by an agent. The tour's touches enter through `RawInput.events`, not through the simulator's input.
- The desktop live run (`cargo run -p demo -- --demo --exit-after-demo`) on a logged-in, unlocked session: unverified here (the windows were occluded in the session that ran it); the headless test runs the same marks.
- A recording: the user's, not made here.
