---
id: widget.swipe-actions
title: Swipe row actions
area: widget
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/swipe_actions.rs, crates/lune-ui-core/tests/widget_swipe_actions.rs, apps/gallery/src/pages/swipe_actions.rs
owner: —
summary: Swipe row actions of a phone list: a row whose content a finger slides sideways to uncover trailing or leading buttons, with a full swipe, and the secondary click or long press the app answers with its menu
---

## What it is

`Ui::swipe_actions` wraps one row of a list. The app draws the row's content in the rect the closure is given, which moves with the finger and carries its own opaque background; the component draws the buttons in the room the content leaves and owns the motion. A leftward swipe uncovers the trailing buttons, a rightward one the leading buttons; the lift springs the row open or closed; with a full swipe a long pull commits the edge's outermost button. A mouse and a keyboard reach the same actions through `context`, the app's menu (D292) [D374].

## Needs

- The swipe-to-reveal row of a native phone list, with the finger's follow, rubber band and spring (R-widget-6).
- A way for a mouse and a keyboard to reach the same actions: the response's `context` and the app's menu (R-widget-3).

## Existing code

The motion is the crate-private `Track` and `Ui::pan_region` of `crates/lune-ui-core/src/widgets/gesture_track.rs` [D369]; the fill of a button is `Tone::color` of `crates/lune-ui-core/src/widgets/tone.rs`; `context` follows `crates/lune-ui-core/src/widgets/tab_strip.rs`'s `secondary_clicked`.

## Spec

### Evidence

Pinned: Lunegit 33a1dd41, Wane 69931d6, this repo 4dddeb7. Read: `crates/lune-ui-core/src/widgets/gesture_track.rs`, `crates/lune-ui-core/src/widgets/pager.rs`, `crates/lune-ui-core/src/widgets/tone.rs`, `crates/lune-ui-core/src/ui/pointer.rs`.
Neither Lunegit nor Wane has a swipe row; the look is a deliberate deviation (hard rule 8): solid fills, each button as tall as the row, at least 74 px wide (the commonly cited size), `Tone::Err` in the theme's error colour, the neutral tone in `accent`, `on_accent` glyph and label [D374]. Conflicts: none.

### Anatomy

- Rect: the row's slot the app gives; everything is clipped to it. The component paints nothing of the row itself: the app's content carries an opaque background so it covers the buttons at rest.
- Content: the rect shifted left by the offset (positive uncovers the trailing edge, negative the leading one).
- Buttons: the row's height, flush with its top and bottom and with no gap between them; the content's edge touches the first, the outermost touches the window edge. Width at least 74 px and the label's own width plus 12 px each side; while the row shows less than the total they share the uncovered room in proportion; past it the outermost takes the rest. A glyph of 20 px over a `font_s` label, centred in the button and clipped to it; the label elided to the button's natural width.
- Pan region: the whole rect (x), registered before the content.

### States

| state | look | cite |
|---|---|---|
| closed | the content fills the row, no button | [D374] |
| swipe, spring | the content moved, the buttons flush behind it | [D374] |
| open | the content moved by the edge's total width | [D374] |
| full swipe | the outermost fills the uncovered width, the others gone | [D374] |
| past an edge with no buttons | the content shifted by the rubber band's share of the pull | [D374] |
| button pressed | the fill shaded by `dialog_backdrop` | [D374] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| release of a swipe | the content's offset | a critically damped spring from the finger's speed (every tier) | 0.3 s period | n/a | n/a | [D369] |
| a tap on a button, a press outside, a press on the content, the close after a full swipe | the content's offset | drawer | moderate | per the motion table | snap | [D3] |

Never animated: the buttons' fills; the content.

### Input and focus

- Pointer: a touch pan on x follows the finger 1:1; an edge with no buttons rubber-bands; the lift springs to open or closed by the speed and position it projects to (a flick over 300 px/s moves one stop); with a full swipe a lift with more than 60 % of the row's width uncovered springs to the full width and reports the outermost button on arrival; a vertical pan goes to the list around the row; a press on a row in motion catches it; a tap on a button reports it and closes the row; a press on the open row's content closes it and reaches nothing; a press outside an open row closes it and goes on where it landed; a secondary click or a long press on the row (not on a button) sets `context` and opens no menu (D292). A mouse press, a drag, a wheel and a key never swipe.
- Keys: none. Tab stop: none. Layer: the layer the app draws on.

### Retained state

- Transient: none beyond the frame.
- Persistent: the track, where the row rests, the row's width and whether a finger holds the row.
- App-owned: the actions, the content, what an action does and the menu on `context`.
- Exit retention: none.

### Accessibility

- Each shown button a `Button` node named by its label with the default action; no node for a hidden one; the row's own nodes are the app's. The app's menu on `context` is the accessible path to the same actions. No new role.

### Strings

- None: the labels are the app's.

### Gallery scenes

- `phone-swipe-closed`: a list of mail rows, every row closed.
- `phone-swipe-trailing-open`: the third row opened to its trailing edge: More and Delete (the error colour).
- `phone-swipe-leading-open`: the third row opened to its leading edge: Read.
- `phone-swipe-full-mid`: a finger held past 60 % of the row's width: Delete stretched across the uncovered width.
- `phone-swipe-context-menu`: a long press on a row: the app's menu holding the row's actions.

### Painter primitives

- Existing: fill, clip-rect, text
- New: none

### Acceptance

- Behaviour (`crates/lune-ui-core/tests/widget_swipe_actions.rs`): a row with no actions, the row at rest, the 1:1 follow with the buttons flush, the open widths and the nodes, a slow release past the middle and a short one, a flick and a flick back, the leading edge, the rubber band at an edge with no buttons and past the open width, the full swipe (trailing and leading, reported once on arrival, then the close; short of 60 % it opens), a tap and a click on a button, a tap on the closed content, a press on the open content, a press outside, opening a second row closing the first, a catch, a mouse that does not swipe, `context` from a secondary click and a long press and not from a button, a vertical pan in a list and a swipe in a list, a disabled scope, a resize, the Reduced and Off tiers, the theme's colours.
- Motion (`crates/lune-ui-core/tests/mobile_motion.rs`, `swipe::`): a release opening the trailing edge, a release closing the row and a full swipe with the close after it conform at 60 and 120 Hz.
- Theme reach: `m10` in `c4.rs` draws a row at rest; `the_buttons_take_their_fills_from_the_theme` draws them open.

### Build

- Milestone: M6
- Depends on: core.ui
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_swipe_actions.rs::a_leftward_pan_follows_the_finger_and_uncovers_the_trailing_buttons_flush, crates/lune-ui-core/tests/widget_swipe_actions.rs::a_slow_release_past_the_middle_opens_and_a_short_one_springs_closed, crates/lune-ui-core/tests/widget_swipe_actions.rs::a_full_swipe_stretches_the_outermost_and_reports_it_on_arrival_then_closes, crates/lune-ui-core/tests/widget_swipe_actions.rs::a_press_outside_closes_an_open_row_and_goes_on_where_it_landed, crates/lune-ui-core/tests/mobile_motion.rs::a_release_opening_the_trailing_edge_conforms_at_60_and_120`; page `swipe-actions`; unverified here: a real finger (no device).

### Limits

- The component paints no row background: the app's content is opaque, or the buttons would show through it.
- A press on the open row's content closes the row and reaches nothing; the row's own click region is under the content at rest, so a widget in the content takes a secondary click or a long press before the row does.
- Keyboard focus is not part of the row: the app's menu on `context` is the mouse and keyboard way to an action.
- The leading and trailing lists are read from their slices every frame, so a list that changes while a row is open is laid out with the new widths.

## Open questions

None.

## Notes for the planner

The row is the component that moves the app's content by a track the finger drives on one axis; content that draws at coordinates of its own, not the rect it is given, does not move.
