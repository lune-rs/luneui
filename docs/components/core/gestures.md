---
id: core.gestures
title: Touch gestures
area: core
status: done
consumers: any
source: crates/lune-ui-core/src/ui/pointer.rs, crates/lune-ui-core/src/ui/touch.rs, crates/lune-ui-core/src/motion/physics.rs
owner: —
summary: A touch press in the pointer machine: a 10 px slop, a long press as a secondary click, a release velocity, a pinch reading, the pan to a scroll region, and the motion physics under a fling
---

## What it is

What a finger does that a mouse does not, in the core and as a pure function of events and app time (D348 to D350). A touch press is told from a mouse press in the pointer machine, and only a touch press changes: it starts a drag at 10 px (a mouse at 4), a hold of 500 ms within that slop is a secondary click, the path it travels is read once on the lift as a velocity, and a second touch reads as a pinch. At the slop the pan is arbitrated per axis between the widget pressed and the scroll regions under it, and the region that wins follows the finger. The motion under a lift is new public items in `motion` (`Decay`, `DampedSpring`, `rubber_band` and its inverse, `conformance_at`). The scroll area's own use of them is in `layout.scroll-area`.

## Needs

- A touch press that starts a drag at 10 px, a long press that is a secondary click (so every widget's `context` answers it, D292) and clicks nothing on the lift, a release velocity (`Ui::release_velocity`), and a pinch reading over the widget the first touch pressed (`Ui::pinch`, `ui::Pinch`), all from the events and the app's time (R-core-12, D348).
- A touch pan that goes to the topmost scroll region live on the dominant axis, unless the pressed widget claims drags on that axis (`Ui::set_drag_axes`; a drag widget that never calls it claims both, as in 0.7.0), with an axis lock, a catch of a moving region and the lift's velocity (R-core-12, D349).
- Motion a lift can run on, closed forms of the time since the lift so that 60 and 120 Hz agree: an exponential decay, a damped spring, a rubber band (R-core-12, D347).

## Existing code

`crates/lune-ui-core/src/ui/pointer.rs` (the press kind, the slop, the long press, the velocity tracker), `ui/touch.rs` (the touch reduction of D344 and the pinch), `ui.rs` (`release_velocity`, `pinch`, `set_drag_axes`, the crate-private pan reading) and `motion/physics.rs` (`Decay`, `DampedSpring`, `rubber_band`, `rubber_band_inverse`); the conformance at a rate is in `motion/conformance.rs`. The page is `apps/gallery/src/pages/touch.rs`.

## Spec

### Evidence

Pinned: this repo at the `orch/mobile-support` branch (the step commits for D347 to D351). No Lunegit or Wane source reads a finger (neither has a touch UI), so every constant is the toolkit's own starting value: slop 10 px, long press 500 ms, velocity window 100 ms and cap 8000 px/s, axis lock 22.5 degrees, decay rate 0.998 per ms, rubber band 0.55, spring response 0.4 s. Read: `docs/DECISIONS.md` D347 to D350, `crates/lune-ui-core/tests/ui_gestures.rs`, `ui/pan_tests.rs`, `tests/motion_physics.rs`.
Conflicts: none. Unverified here: the feel of a real finger (no finger can be driven), and a touchscreen on Windows or Linux (D12).

### Anatomy

- A press has a kind. A touch press is the primary touch's synthesized press; a mouse press runs today's code path, so every rule here tests the kind and a 0.7.0 app sees no change.
- Slop: 10 logical px of travel from the press. Inside it a press is a tap (a click on the lift) or a long press; past it a drag widget starts a drag with the whole travel as the first `drag_delta`, or the pan is arbitrated (D348, D349).
- Long press: a touch held 500 ms of app time within the slop sets `secondary_clicked` once on the widget it holds, on the first frame at or after that time; the core asks for that frame (`request_repaint_at`). The press is then spent: its lift clicks nothing and a pan can no longer take it.
- Release velocity: a least-squares slope over the primary pointer's frame samples of the last 100 ms, a release sample included; zero with fewer than two samples or no movement in the last 40 ms; at most 8000 px/s. `Ui::release_velocity(id)` reads it on the frame `drag_stopped` is set for `id` or `id`'s pan ends.
- Pinch: while two touches are down, `Ui::pinch(id)` on the widget the first touch held returns the scale (distance now over the start distance, at least 1 px), the centre and the start centre, with `started` and `ended`. A reading only: nothing is cancelled.
- Pan: at the slop the dominant axis is x when the travel is wider than tall, else y. The region that takes it is the topmost whose pan axes (live axes with content past the viewport) include that axis, unless the pressed widget senses drag and claims the axis. The pan locks to its axis unless the region has both and the minor travel is past 0.414 of the major. A press on a region that was moving catches it and reaches nothing under it.
- Motion: `Decay { rate }` (offset, velocity, duration and `time_to` of a fling), `DampedSpring` (the bounce, 0.4 s critically damped), `rubber_band(over, viewport)` and `rubber_band_inverse`.

### States

| state | look | cite |
|---|---|---|
| a touch press, inside the slop | the widget's pressed look | [derived: the pointer path of R-core-11] |
| a long press | the widget's `context` fires once; the app shows its menu | [derived: D292, D348] |
| a pan | the region follows the finger 1:1 less the slop; the widget pressed is no longer pressed | [derived: roadmap D5] |
| a pinch | the app scales what it holds from `Ui::pinch` | [derived: roadmap D5] |

### Transitions

n/a — a touch gesture is the finger's own motion and takes no `MotionTier` (the physics is a closed form of time; D350, `docs/motion.md`, "Scroll physics").

### Input and focus

- Touch: the primary touch drives the pointer (R-core-11); this entry adds what it does at the slop, at 500 ms and at the lift. A second touch reads as a pinch and reaches no widget. The mouse is untouched. No key and no focus rule: a long press and a pan move no focus.
- Tab stop: no. Layer: the layer of the widgets it reads; a pan over a modal goes to no region below it.

### Retained state

- Transient: the press kind, the samples of the primary pointer, the long press deadline, the pinch run, the pan reading of the frame. Persistent: none of its own (the scroll area keeps the offset, the fling and the bounce).
- App-owned: what a long press opens, what a pinch scales.
- Exit retention: no.

### Accessibility

n/a — a gesture has no node. The show-menu action and the scroll actions are the assistive route to what a long press and a pan do [infra.accessibility].

### Strings

None.

### Gallery scenes

- `phone-touch-tap`: a tap on the first button in the phone window (402 by 874 at scale 3, a safe area of 62 above and 34 below): the counter reads 1, settled.
- `phone-touch-long-press`: the finger holds the hold tile past 500 ms: the menu is open under it, settled.
- `phone-touch-pan`: the finger has moved 200 px up the list and rests: the list moved 190, settled.
- `phone-touch-arbitration`: a vertical pan that started on a pin button in a row: the list moved 130 and the button was not pressed, settled.
- `phone-touch-pinch`: two touches spread from 100 to 160 px: the box is at scale 1.6, settled.
- `phone-widgets-hit-area`: a tap 12 px beside the 16 px box of a small checkbox, 4 px outside its 24 px hit, in the phone window: the 44 pt touch target takes it and the box is checked, settled.

### Painter primitives

- Existing: fill, text, clip-rect, xform.
- New: none.

### Acceptance

- Slop and velocity (`tests/ui_gestures.rs`): a tap with an 8 px wobble clicks and a mouse press with it drags; a drag past the slop starts with the whole travel; a pan at 600 px/s leaves 600 px/s on the stop frame only, at 60 and at 120 Hz; a finger held still before the lift leaves none; a very fast pan is clamped to 8000.
- Long press: held 500 ms it is a secondary click once and its lift clicks nothing; lifted at 499 ms it clicks; a mouse press held long is none; a pending long press asks for its frame; a press that left the slop and came back is none; a cancel fires nothing; a long press on a tab is its `context`.
- Pinch: two touches that spread read a pinch that ends with the lift; a third touch changes none and a new pinch needs every touch up.
- Pan (`ui/pan_tests.rs`): a vertical pan from a button goes to the region and the button never clicks; a drag widget that never claims axes keeps the touch; one that claims horizontal only yields a vertical pan; nested regions split a pan by axis; a region on both axes locks a shallow start and pans a diagonal free; a press on a moving region catches it; the pan's end carries the finger's velocity; a mouse drag is as it was at 0.7.0.
- Motion (`tests/motion_physics.rs`): the decay covers its documented distance and knows when it reaches a distance; the spring starts where told and ends exactly; the rubber band is odd, monotonic and bounded and its inverse undoes it; every kind and tier conforms at 120 Hz as at 60.
- The gallery: `phone-touch-fling` and `phone-touch-fling-120` (the same flick scripted every 16.7 ms and every 8 ms) build equal scenes at the capture frame (`the_flick_at_60_and_at_120_hz_builds_equal_scenes`), the tap counter reads 1, the long press opens the menu, the arbitration pan scrolls the list by 130 and presses nothing, a pull stretches the list by less than the pull, the pinch box reads 1.6x.

### Build

- Milestone: mobile M3 (steps 2 to 6, D347 to D351). Built: the page is `touch`, 10 scenes, all `phone-touch-*` at scale 3 in the phone window.
- Depends on: core.input, core.ui, layout.scroll-area, motion.transitions
- Done (D47, M11): tests `crates/lune-ui-core/tests/ui_gestures.rs::a_touch_tap_with_a_wobble_clicks_and_a_mouse_press_with_it_drags, crates/lune-ui-core/tests/ui_gestures.rs::a_touch_drag_past_the_slop_starts_with_the_whole_travel, crates/lune-ui-core/tests/ui_gestures.rs::a_touch_pan_leaves_its_velocity_at_both_rates_on_the_stop_frame_only, crates/lune-ui-core/tests/ui_gestures.rs::a_touch_held_500_ms_is_a_secondary_click_once_and_its_lift_clicks_nothing, crates/lune-ui-core/tests/ui_gestures.rs::two_touches_that_spread_read_a_pinch_that_ends_with_the_lift, crates/lune-ui-core/src/ui/pan_tests.rs::a_vertical_pan_from_a_button_goes_to_the_region_and_the_button_never_clicks, crates/lune-ui-core/src/ui/pan_tests.rs::a_mouse_drag_in_the_same_layout_is_as_it_was_at_0_7_0, crates/lune-ui-core/tests/motion_physics.rs::a_decay_covers_the_documented_distance, crates/lune-ui-core/tests/motion_physics.rs::every_kind_and_tier_conforms_at_120_hz_as_at_60, apps/gallery/src/pages/touch.rs::the_flick_at_60_and_at_120_hz_builds_equal_scenes`; page `touch` (10 scenes); unverified here: the feel of a real finger, and a touchscreen on Windows or Linux (D12).

## Open questions

- None.

## Notes for the planner

New in mobile M3. A widget that must keep a touch from a pan senses drag and claims its axes with `Ui::set_drag_axes`; a widget that pinches senses drag too, since a pinch reads over the widget the first touch pressed. Each widget's own touch rules (target sizes, what a press does before a pan takes over) are the mobile components' (M5, M6).
