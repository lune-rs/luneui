---
id: platform.input
title: winit input translation and output
area: platform
status: done
consumers: any
source: crates/lune-ui-winit/src/input.rs, crates/lune-ui-winit/src/platform.rs, crates/lune-ui-platform/src/web.rs
owner: —
summary: Translator turns winit events into RawInput; OutputState applies FrameOutput to a window
---

## What it is

`Translator` (one per window: `window_event` or the per-event methods, then `take(time)` to get a `RawInput`) divides physical positions by the scale factor and ignores gestures, theme and occlusion. `OutputState` (one per window) applies a `FrameOutput` and touches the cursor and the IME only when they changed; `apply` returns true when the frame asked to close. `cursor_icon` maps the core `CursorIcon` to winit's. The app owns the event loop and calls these from it.

## Needs

- Input and output as data between winit and the toolkit (R-nfr-8, R-platform-5 for cursors).
- Inbound OS file-drop events translated from winit (R-platform-5, D43).
- The accessibility bridge: tree out, `AccessAction` in, behind a cargo feature (R-infra-4, D46).
- In a browser (R-platform-8, D429, D430): `lune_ui_platform::web::TextInput`, a hidden `<textarea>`, gives the canvas an input method and a phone's keyboard as `Ime` values for `Translator::ime`, and `web::keymap()` gives a web app the Mac or PC chords by the platform name the browser reports (`lune_ui_winit::keymap()` is `Keymap::PC` on wasm). Phone keyboards and AccessKit on web are unverified (`docs/web.md`).

## Existing code

`crates/lune-ui-winit/src/input.rs:31` (`Translator`), `:215` (`take`), `:285` (`cursor_icon`), `:367` (`OutputState`), `:380` (`plan_output`), `:406` (`apply`); 516 lines; the OS-specific parts in `src/platform/{macos,windows,other}.rs`. Tests: `crates/lune-ui-winit/tests/input.rs` (translator and `ScriptedInput` agree; every cursor icon maps to the same named winit icon).

## Spec

### Evidence

Pinned: Lunegit 33a1dd41, Wane 69931d6, this repo fa2958b. Read: `crates/lune-ui-winit/src/input.rs` (all 334 lines),
`crates/lune-ui-winit/tests/input.rs:95-210`, `crates/lune-ui-core/src/input.rs` (all), `apps/gallery/src/main.rs:205-245`;
winit 0.30.13 `src/event.rs:178-192` in the local cargo registry (the file-drop events; not a pinned-repo source).
Searched: the local cargo registry holds no AccessKit crate, so nothing in this repo or the registry documents its API.
Conflict: none. Lunegit and Wane specify no windowing adapter; the entry rests on the moved code and decisions D35, D43, D46.
No consumer in the repo calls `Translator` or `OutputState` yet: the gallery handles winit events itself
[R apps/gallery/src/main.rs:210-239].

### Anatomy

Moved, today (`crates/lune-ui-winit/src/input.rs`):
- `Translator` (one per window) keeps the window state between frames: physical size, scale, pointer, modifiers, focus and the event list [R crates/lune-ui-winit/src/input.rs:26-33].
- `window_event` dispatches resize, scale-factor, cursor move and leave, mouse button, wheel, keyboard, modifiers, IME, focus, file hover (and its cancel) and file drop, and touch once `set_touch_enabled(true)` is called; every other `WindowEvent` (touch before that, gestures, theme, occlusion, axis motion) is ignored [R crates/lune-ui-winit/src/input.rs:90-115].
- Positions and wheel pixels are divided by the latest scale factor; an event before the first pointer move carries (0, 0) [R crates/lune-ui-winit/src/input.rs:19-24,48-51,87-91,108-122].
- `key` pushes `Event::Key`, then `Event::Text` when the press typed something `printable` returns [R crates/lune-ui-winit/src/input.rs:125-137]; `map_key` maps 27 named keys, passes characters through and turns everything else into `Key::Unidentified` [R crates/lune-ui-winit/src/input.rs:194-232].
- `take(time)` builds the `RawInput` (logical size, scale, the app's time, focus, the drained events) and leaves the window state in place [R crates/lune-ui-winit/src/input.rs:171-181].
- `OutputState` (one per window) remembers the last cursor and IME area; `apply(window, out, ui_scale)` sets the cursor only when it changed, enables IME and sets its cursor area when an area first appears (in physical pixels, `area * ui_scale`, by `ime_area_physical`; a non-finite area sends nothing, D266), disables IME when it goes away, then runs the commands (drag move, drag resize, minimize, toggle maximize, redraw) and returns true for `Close` [R crates/lune-ui-winit/src/input.rs:281-334].
- `cursor_icon` maps each of the 36 core `CursorIcon`s to winit's; a new core icon shows the default cursor until mapped [R crates/lune-ui-winit/src/input.rs:235-278].

Additions:
- M10, platform keymap: `lune_ui_winit::input::keymap()` (re-exported as `lune_ui::input::keymap`) answers `Keymap::MAC` on macOS and `Keymap::PC` elsewhere, through `platform/{macos,windows,other}.rs`; the gallery no longer branches on the OS (D212).
- M10, file drop: `WindowEvent::HoveredFile`, `HoveredFileCancelled` and `DroppedFile` become the core file-drop events, stamped with the translator's last pointer position (winit gives none) [D43], [derived: winit 0.30.13 `src/event.rs:180-192`], [core.input].
- M2 (mobile), touch: `Translator::set_touch_enabled(true)` (off at start; `touch_enabled()` reads it back) turns `WindowEvent::Touch` into `Event::Touch` at its logical position, with `Force::normalized` as the force, and leaves the translator's pointer alone; off, the event is dropped as at 0.7.0; turning it off while forwarded touches are down pushes a `Cancelled` for each [D346], [core.input]. Tests (`crates/lune-ui-winit/tests/input.rs`, module `touch`): `a_touch_script_equals_the_translated_touch_events`, `with_the_switch_off_touch_is_dropped_and_the_mouse_is_unchanged`, `force_is_normalised`, `switching_off_mid_touch_cancels_each_touch_once_and_then_drops_touch`, `the_switch_reads_back_and_touch_leaves_the_pointer_alone`.
- M10, accessibility bridge: `lune_ui_winit::access`, behind the feature `accesskit` (`lune_ui::access` through the facade's feature of that name), takes the `FrameOutput` tree and focused id and feeds an AccessKit adapter, and turns AccessKit action requests into `Translator`-queued `Event::Access` [D46], [D2], [D216], [infra.accessibility].
- M10, clipboard: `OutputState::apply` executes the frame's clipboard requests through the clipboard entry; paste text re-enters as an event [platform.clipboard].
- Web, text input (D429): a browser's winit sends no IME events and has no input method for a canvas, so `lune_ui_platform::web::TextInput` (re-exported as `lune_ui::web`) owns a hidden `<textarea>` and yields the `Ime` values `Translator::ime` takes. The app feeds it what it would give the window: `set_allowed` and `set_area` from the `OutputState::plan_output` plan (`ime_area_physical`'s tuple), `take` before the translator's `take`, and every `WindowEvent` through `passes` first (it keeps `Focused` from the translator on the web; `take_focus` gives the window's focus as a whole, so the canvas-to-textarea hand-over is not a lost focus). Non-text keys and Ctrl/Cmd chords are re-dispatched on the canvas, so winit's key mapping is still the only one. Off the web it is inert. Tests: `crates/lune-ui-platform/src/platform/web/keys.rs` (routing, composition, focus accounting) and `scripts/web-run.sh demo --smoke`, [platform.input].
- M4, the new `FrameOutput` fields (`repaint`, `tree`, `focused`) are ignored by `apply` except `repaint`, which the app's event loop reads to schedule the next frame; the adapter never starts a loop [R crates/lune-ui-winit/src/input.rs:289-293], [D3], [core.ui].

### States

n/a — an adapter, not a component.

### Transitions

n/a — no animation.

### Input and focus

- Pointer, wheel, key, text, IME and window focus come from winit as listed in Anatomy; the adapter gives `RawInput.focused` the window's focus and starts unfocused until winit says otherwise [R crates/lune-ui-winit/src/input.rs:19-24,164-168].
- Keys the adapter does not map become `Key::Unidentified`; Tab stop and layer are n/a (they belong to widgets).
- The app owns the event loop and calls the adapter from it [R crates/lune-ui-winit/src/input.rs:1-3].

### Retained state

- Transient: none beyond the event list, which `take` drains.
- Persistent: per window, the translator's pointer, modifiers, focus, size and scale, and the output state's last cursor and IME area [R crates/lune-ui-winit/src/input.rs:26-33,281-285].
- App-owned: the winit event loop, the window and the time passed to `take`.
- Exit retention: no.

### Accessibility

The AccessKit bridge lives here from M10 (feature-gated); the node model, the enums and the bridge's limits are `infra.accessibility`. Without the feature the adapter exposes nothing to assistive technology [D46], [D216].

### Strings

None: no user-visible text.

### Gallery scenes

No scene exercises the adapter: a scene's hover is a field of the scene, not translated input [R apps/gallery/src/main.rs:260], [R apps/gallery/src/scenes.rs:90-94]. In window mode the gallery's own event loop is the consumer (`Translator` and `OutputState`, for the main window and the tray's surface window); the `file-drop` page (M10) shows a file dragged from the OS in the window, and its two scenes (`core.input` lists them) send the same events through the script.

### Painter primitives

n/a — no drawing.

### Acceptance

- Existing tests (`crates/lune-ui-winit/tests/input.rs`): `translator_and_scripted_input_agree`, `state_persists_and_events_drain`, `unmapped_keys_are_unidentified`, `other_mouse_button_keeps_its_number`, `resize_edges_map_to_the_same_named_direction`, `every_cursor_icon_maps_to_the_same_named_winit_icon`.
- Done in M10 step 4 (`crates/lune-ui-winit/tests/input.rs`, module `files`): file-drop translation (`hover_cancel_and_drop_give_the_three_events_at_the_last_pointer_position`, scale 2; `before_any_pointer_move_the_position_is_the_origin`; `two_files_give_two_events_in_order`; `the_translator_and_the_script_agree_on_file_events`); `Translator::access` queues an `Access` event that appears in the next `take` (`an_access_action_queued_on_the_translator_appears_in_the_next_take`; the AccessKit bridge itself is step 6); the change detection of `OutputState::apply` factored into the pure `OutputState::plan_output` (`plan_output_is_public_pure_and_starts_from_nothing_set`, and the sequences in the unit tests of `src/input.rs`); `keymap()` is `Keymap::MAC` on macOS and `Keymap::PC` elsewhere (`platform/{macos,windows,other}.rs` own tests, D212); a non-UTF-8 path passes through unchanged (`platform/macos.rs` test). The AccessKit bridge's tests are `crates/lune-ui-winit/tests/access.rs` (step 6, `infra.accessibility`). Originally: file-drop translation tests (a hover, a cancel and a drop give the three core events at the last pointer position) in M10; an `Access` event queued by the bridge appears in the next `take`; the change-detection of `OutputState::apply` (cursor and IME area set only when changed) factored into a pure function so a test runs without a live window [derived: `apply` against a live window is not run by the gates].
- A feature matrix: the crate builds and its tests pass with and without the accessibility feature (M10). Done in M10 step 6: the gate's `-p lune-ui-winit` line runs it without, `--workspace` with (the gallery turns it on).
- No clock read in the adapter: the app passes `time` to `take` [R crates/lune-ui-winit/src/input.rs:215] (`scripts/check-boundaries.sh`).
- The planned output, without a window: `crates/lune-ui-winit/src/input.rs`, module `tests`: `the_cursor_is_set_on_the_first_frame_and_on_a_change_only`, `the_ime_is_touched_when_its_area_appears_moves_or_goes`, `planning_changes_nothing`.

### Build

- Milestone: M10 (file drop, AccessKit bridge, clipboard execution); `take` and `apply` extensions for M4's `FrameOutput` fields land with M4.
- Depends on: core.input, infra.accessibility, platform.clipboard.
- Done (D47, M11): tests `crates/lune-ui-winit/tests/input.rs::translator_and_scripted_input_agree, crates/lune-ui-winit/tests/input.rs::state_persists_and_events_drain, crates/lune-ui-winit/tests/input.rs::unmapped_keys_are_unidentified, crates/lune-ui-winit/tests/input.rs::every_cursor_icon_maps_to_the_same_named_winit_icon, crates/lune-ui-winit/tests/input.rs::hover_cancel_and_drop_give_the_three_events_at_the_last_pointer_position, crates/lune-ui-winit/tests/input.rs::an_access_action_queued_on_the_translator_appears_in_the_next_take, crates/lune-ui-winit/tests/input.rs::plan_output_is_public_pure_and_starts_from_nothing_set, crates/lune-ui-winit/src/input.rs::the_ime_is_touched_when_its_area_appears_moves_or_goes`; page n/a (an adapter; the gallery's event loop is its consumer, and the `file-drop` page shows the events); unverified here: real OS events and windows (the platform smoke exercises one on macOS), Windows and Linux run time (D15).

## Open questions

- Still open, a limit: unmapped named keys become `Unidentified`. None of the catalogue specs written so far needs another named key; `text.text-edit` (M5) is the one that could, and it extends `NamedKey` and `map_key` together if it does.
- Answered: `OutputState::apply` against a live window is not run by the gates (a window cannot be built headless); the pure change-detection split `plan_output` is what the tests above run. The tray popover window runs `apply` for real in the platform smoke.
- Answered in M10 ([D216]): AccessKit's adapter and `Translator` both get every `WindowEvent`; the app calls `AccessBridge::window_event` first.

## Notes for the planner

Added in M2 step 6. The core model is `core.input`.
