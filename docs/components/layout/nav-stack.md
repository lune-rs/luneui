---
id: layout.nav-stack
title: Navigation stack
area: layout
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/nav_stack.rs, crates/lune-ui-core/tests/widget_nav_stack.rs, apps/gallery/src/pages/nav_stack.rs
owner: —
summary: Navigation stack of a phone app: the app's levels, the top one pushed in from the right, popped back out by a call or an edge swipe, the level below following by a third with a shade and a shadow
---

## What it is

`Ui::nav_stack` moves between the levels of a drill-down. The app owns the levels (`depth` and what each draws); the stack draws the top one, pushes a new one in from the right edge when `depth` grows by one, pops the top out by `Ui::nav_stack_pop` or a swipe from the left edge, and keeps every covered level alive so it keeps its scroll offset. While it moves, the level below is drawn too, 30 % of the width to the left of its rest, dimmed, under a soft shadow along the top level's leading edge [D372].

## Needs

- The drill-down of a phone app, with the back gesture of a native one (R-widget-6, R-layout-3).
- A way for a mouse and a keyboard to do what the swipe does: the app's back button calls `nav_stack_pop` (R-widget-3).

## Existing code

The motion is the crate-private `Track` and `Ui::pan_region` of `crates/lune-ui-core/src/widgets/gesture_track.rs` [D369]; the bars a level draws are `widget.nav-bar` (`crates/lune-ui-core/src/widgets/nav_bar.rs`); the keep-alive scopes are `Ui::keep_alive` (`crates/lune-ui-core/src/ui.rs`); the shadow is `Painter::shadow` (`core.soft-shadow`).

## Spec

### Evidence

Pinned: Lunegit 33a1dd41, Wane 69931d6, this repo 7515977. Read: `crates/lune-ui-core/src/widgets/gesture_track.rs`, `crates/lune-ui-core/src/widgets/bottom_sheet.rs`, `crates/lune-ui-core/src/ui.rs` (`scope`, `keep_alive`), `crates/lune-ui-core/src/ui/pointer.rs` (`arbitrate`, `region_at`, `catch`).
Neither Lunegit nor Wane has a stack of screens; the look is a deliberate deviation (hard rule 8): 30 % parallax, a shade and a soft shadow from the theme's scrim, as the common phone convention [D370], [D372]. Conflicts: none.

### Anatomy

- Rect: the area the app gives the stack (the whole window for a phone screen); everything is clipped to it.
- Top level: drawn in the rect moved right by `x` (0 at rest, the width when gone); the app's `page` gets the moved rect.
- Lower level (while moving): drawn at `rect.x - 0.3 * width * (1 - x / width)`, clipped to the part left of the top's edge (its text draws above every fill of the layer and would otherwise show through the top level).
- Shade: an over fill on the lower level's uncovered part (so it dims text as well as fills), `dialog_backdrop` with its alpha times `0.4 * (1 - x / width)`.
- Shadow: `Painter::shadow` of the top level's rect, sigma 10, `dialog_backdrop` with its alpha times `0.7 * (1 - x / width)`, the shape 30 px taller each way so the edge does not fade at the rect's top and bottom; drawn under the top level, cut by the clip.
- Edge band: the rect's left 24 px, a pan region (x) registered before the levels, while `depth` is at least 2.
- Hold region: the whole rect, registered after the levels, only while it moves.
- Backgrounds: none of its own; a level fills its rect opaquely [D372].

### States

| state | look | cite |
|---|---|---|
| rest | the top level alone | [D372] |
| push, pop, swipe, spring back | both levels, parallax, shade and shadow | [D372] |
| popped (one frame) | the level below alone, at rest | [D372] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| push, queued pop | the top level's left edge | page swap | moderate | quad out | snap | [D3] |
| release of a swipe | the top level's left edge | a critically damped spring from the finger's speed (every tier) | 0.3 s period | n/a | n/a | [D369] |

Never animated: the levels' content; a depth the app changes by itself, and a depth up by more than one, cut.

### Input and focus

- Pointer: a touch pan that starts in the left 24 px band, with `depth` at least 2, drags the top level across 1:1 (clamped to the rect) and the lift picks a pop or a spring back by the projected position (a fling over 300 px/s completes); a touch press on the moving stack catches it; a pan that starts elsewhere never pops; a vertical pan in the band goes to a scroll area under it; a mouse press in the band clicks the widget under it; presses on the moving stack are swallowed, none taps or scrolls a level.
- Keys: none of its own; the levels' widgets take theirs. The app's back button calls `Ui::nav_stack_pop` (after the stack call, in the stack's scope).
- Layer: the layer the app draws on.

### Retained state

- Transient: the queued pop.
- Persistent: the track, the mode and the last depth; each level's own persistent state, inside its keep-alive scope, ended when the app removes the level.
- App-owned: `depth`, the levels' content.
- Exit retention: none.

### Accessibility

- None of its own: the stack owns no string and emits no node; the levels' nodes are the app's. No new role [D372].

### Strings

None.

### Gallery scenes

- `phone-nav-stack-root`: the root level, a list under a title bar, settled.
- `phone-nav-stack-pushed`: the pushed level at rest, its bar with a back button.
- `phone-nav-stack-push-mid`: a push captured with the new level about half way in.
- `phone-nav-stack-swipe-mid`: a finger held half way across the edge swipe.

### Painter primitives

- Existing: fill, shadow, clip-rect
- New: none

### Acceptance

- Behaviour (`crates/lune-ui-core/tests/widget_nav_stack.rs`): push, a queued pop and its one `popped`, the app lowering or keeping its depth after it, a cut on a depth the app changes, the Reduced and Off tiers, the edge swipe and where it starts, completing and springing back, a fast flick, a catch, a vertical pan in the band, presses swallowed while it moves, a mouse press in the band, kept-alive levels, a removed level's state, a disabled scope, id scopes of the levels.
- Motion (`crates/lune-ui-core/tests/mobile_motion.rs`, `stack::`): a push, a queued pop, a swipe released past the middle and one released short conform at 60 and 120 Hz.
- Theme reach: `m10` in `c4.rs` draws a stack.

### Build

- Milestone: M6
- Depends on: widget.nav-bar, core.ui
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_nav_stack.rs::a_push_slides_the_new_top_in_and_parallaxes_the_level_below, crates/lune-ui-core/tests/widget_nav_stack.rs::a_queued_pop_reverses_the_push_and_reports_popped_once, crates/lune-ui-core/tests/widget_nav_stack.rs::a_swipe_released_past_the_middle_completes_and_one_short_of_it_springs_back, crates/lune-ui-core/tests/widget_nav_stack.rs::a_covered_level_keeps_its_scroll_offset_through_a_push_and_a_pop, crates/lune-ui-core/tests/mobile_motion.rs::a_push_conforms_at_60_and_120`; page `nav-stack`; unverified here: a real finger (no device).

### Limits

- A level must fill its rect opaquely: the stack paints no background.
- The key of `nav_stack_pop` resolves in the scope of the call, so it is called after the stack call, not from inside a `page` closure.
- Under the Reduced and Off tiers a push and a pop cut: the page swap's Reduced form is a fade, which two sliding levels have no form of.
- A push while a pop or a swipe moves cuts.

## Open questions

None.

## Notes for the planner

The stack is the one M6 component that draws app content in a rect it moves: a page that draws at coordinates of its own, not the rect it is given, does not move.
