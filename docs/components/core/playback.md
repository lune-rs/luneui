---
id: core.playback
title: Input player
area: core
status: done
consumers: any
source: crates/lune-ui-core/src/playback.rs
owner: —
summary: A script of timed touch gestures and marks played against the app's time through RawInput.events, with the next frame it needs handed back, and an opt-in touch indicator
---

## What it is

`lune_ui_core::playback` (also `lune_ui::playback`; D415). A `Script` is a list of timed cues built by a cursor: `tap`, `press`, `swipe`, `drag`, `wait`, `event` (a raw event at the cursor), `mark` (a name the player reports) and `end_hold`. A `Player` takes the app's time and appends the due events to `RawInput.events`, returning the marks passed and the time it needs a frame at (`Advance.next`, for `Ui::request_repaint_at`). `draw_touches` draws a disc under each scripted contact for a recording. Nothing reads a clock: the app passes the time, so a script plays the same events at 60 Hz, at 120 Hz and under a hitch.

## Needs

- Scripted input in app time through `RawInput.events`, with the next frame time out, for demos, recordings and tests that must not depend on the frame rate (R-core-13, D415).
- Gestures that land on the pointer machine as a finger does: a tap well under the long press, a swipe that flings, a drag that leaves no speed (R-core-12, D348).
- A background of any length that replays no burst, a repeat that cancels a contact still down, and an indicator that costs nothing when the app does not call it (D415).

## Existing code

`crates/lune-ui-core/src/playback.rs`. The player emits `Event::Touch` (`core.input`) that the pointer machine reduces (`core.gestures`). The scripted fake `testing::ScriptedInput` (`core.testing`) is frame-indexed and is a test double; the player is for the app.

## Spec

### Evidence

Pinned: this repo at the `orch/mobile-support` branch (M7 step 1, D415). No source outside the toolkit: the constants are its own starting values, chosen against the pointer machine's (slop 10 px, long press 500 ms, velocity window 100 ms and still time 40 ms, `core.gestures`). Read: `crates/lune-ui-core/src/ui/pointer.rs`, `docs/DECISIONS.md` D415.
Conflicts: none. Unverified here: a live run on a phone (the demo's, M7 steps 3 and 4).

### Anatomy

- A gesture takes a fresh touch id (`TOUCH_ID_BASE` + its number in the script) and expands at build time into `Event::Touch` samples every `SAMPLE` (8 ms), so the event list never depends on the frame rate; the reducer keeps the last sample of a frame, so a 60 Hz app sees every other one.
- `tap`: a press at the cursor, the lift `TAP_HOLD` (80 ms) later. `press(pos, hold)`: the same held `hold`; past 500 ms it is a long press.
- `swipe`: a straight line sampled every 8 ms, the last move exactly at the end point and the lift with it: a fling. `drag`: eased in and out, then `Moved` at the end point every 8 ms for `DRAG_SETTLE` (80 ms), then the lift: the release velocity reads zero and nothing flings.
- `Player::advance(now, events)`: the first call latches the start; it appends every cue due by `now` in builder order (a mark written after a gesture comes after its lift) and returns `Advance { marks, next, restarted, finished }`. `next` is the app time of the next cue, the wrap of a repeating script, or the end of a running fade, whichever is first; `None` while paused, before the first call and once finished with nothing fading. While paused, or when `now` is earlier than the last call's, nothing is appended.
- `pause(now)` and `resume(now)` shift every later cue by the gap. `with_repeat(true)` wraps after `Script::length()` (the last cue plus `end_hold`, 1.5 s unless set), first cancels any contact still down, skips whole cycles it finds past and reports `restarted`; the app decides what a restart resets.
- `Player::contacts(now)`: one `Contact` per scripted touch that is down (alpha 1) or lifted less than `FADE` (250 ms) ago (alpha falling linearly to 0), at its last position.
- `draw_touches(ui, &player)`: on `LayerId::Toast`, a 44 pt disc per contact in `fg_primary` at 22 % and a 1.5 pt ring at 55 %, times the contact's alpha; a lifted contact is skipped unless the tier is `Full`. It keeps the clip stack (`Painter::on`), so call it last in a frame at the top level. It asks for no frame: `Advance.next` carries the fade.

### States

| state | look | cite |
|---|---|---|
| a contact down | a disc 44 pt across, fill 22 % and ring 55 % of `fg_primary` | [derived: D415] |
| a contact lifted, `MotionTier::Full` | the same, fading to nothing over 250 ms | [derived: D415] |
| a contact lifted, a lower tier | gone at once | [derived: `docs/motion.md` tiers] |

### Transitions

The fade of a lifted contact is closed-form in app time (`1 - (now - lift) / 250 ms`) and plays only at `MotionTier::Full`; there is no other motion. It follows the app's time, so it is the same at 60 and 120 Hz.

### Input and focus

- None of its own: the player only produces events; a finger it plays reaches widgets through the pointer path of `core.gestures`. It adds no key, no focus and no tab stop.

### Retained state

- The player holds its own state (start, pause shift, cue index, cycle, the contacts it emitted); it keeps nothing in `UiStore`. App-owned: the `Player` itself, and what a restart resets.
- Exit retention: no.

### Accessibility

n/a — the player emits touch events and the indicator is a picture of them; neither is a node.

### Strings

None.

### Gallery scenes

n/a — not a widget; the demo app's `--demo` plays a script of it end to end (M7 steps 2 to 4).

### Painter primitives

- Existing: fill, ring, on.
- New: none.

### Acceptance

- `crates/lune-ui-core/tests/playback.rs`: a tap is a press and a lift 80 ms apart with one id; a 200 ms swipe is 25 moves, the last exactly at the end and its lift at 200 ms; a drag ends with 80 ms of moves at the end point and its lift, which through a real `Ui` at 60 Hz leaves a release velocity of zero where the same swipe leaves a fling; the events are the same at 60 and at 120 Hz; a pause emits nothing and moves the next cue by its length; time that goes back emits nothing; the start is the first call; a repeat wraps once per cycle, plays the same events again, cancels a contact down at the wrap and skips whole cycles; `next` is later than `now` on every call and runs cue, fade, end; a tap through `Player` and `Ui` clicks once at 60 and 120 Hz and a long press is one secondary click; `draw_touches` draws a disc at each window corner on the toast layer, none for a lifted contact under `Reduced`, and takes its colour from the theme.

### Build

- Milestone: mobile M7 step 1 (D415). Built: `playback.rs` and the facade re-export.
- Depends on: core.input, core.gestures, core.painter
- Done (D47, M11): tests `crates/lune-ui-core/tests/playback.rs::a_tap_expands_to_a_press_and_a_lift_80_ms_apart_with_one_id, crates/lune-ui-core/tests/playback.rs::a_drag_lifts_with_no_speed_and_the_same_swipe_flings, crates/lune-ui-core/tests/playback.rs::the_events_are_the_same_at_60_and_at_120_hz, crates/lune-ui-core/tests/playback.rs::a_pause_emits_nothing_and_the_next_cue_comes_the_pause_later, crates/lune-ui-core/tests/playback.rs::a_repeat_wraps_once_per_cycle_and_plays_the_first_cycle_again, crates/lune-ui-core/tests/playback.rs::a_tap_through_the_player_clicks_once_and_a_long_press_is_a_secondary_click, crates/lune-ui-core/tests/playback.rs::a_contact_at_each_window_corner_draws_a_disc_on_the_toast_layer_there, crates/lune-ui-core/tests/playback.rs::the_disc_takes_its_colour_from_the_theme`; page n/a (no scene of its own; the demo's `--demo` plays it, M7 steps 3 and 4); unverified here: a live phone (M7 step 4).

## Open questions

- None.

## Notes for the planner

New in mobile M7. A script's coordinates must come from the layout the screen uses, so one script fits every size and safe area (the demo's `showcase::Layout`). A gesture that must not fling is a `drag`, one that must is a `swipe`. The first scripted gesture should not start at the same frame the widget it aims at first appears: the pointer reads the previous frame's hit regions, so start the script a frame or two in (a `wait`).
