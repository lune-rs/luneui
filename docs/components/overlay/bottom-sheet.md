---
id: overlay.bottom-sheet
title: Bottom sheet
area: overlay
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/bottom_sheet.rs, crates/lune-ui-core/tests/widget_bottom_sheet.rs, apps/gallery/src/pages/bottom_sheet.rs
owner: —
summary: Modal bottom sheet of a phone screen: slides up to a detent, follows a finger, springs to the detent a release picks or off the screen to dismiss
---

## What it is

`Ui::bottom_sheet` draws a modal surface that slides up from the window's bottom edge to its first detent over a scrim, follows a finger that drags it, and springs to the detent the lift picks, or off the screen to dismiss. It is an overlay (`Ui::overlay`, on the modal layer, focus held inside, Escape dismisses) that the app calls every frame while its `open` flag is set and until the sheet has slid out after the flag is cleared. A mouse or a key never drags it: a click on the grabber steps to the next detent, Escape and a click on the scrim close [D371].

## Needs

- A phone screen's secondary surface that keeps the page it covers recognisable and moves with the finger (R-overlay-5, R-widget-6).
- A way for a mouse and a keyboard to do everything a finger does: step the detent, close, reach the content (R-widget-3).

## Existing code

`crates/lune-ui-core/src/widgets/dialog.rs` (the modal host, the scrim) and `crates/lune-ui-core/src/widgets/popover.rs:174` (`glass_surface`) are the neighbours; the motion is the crate-private `Track` of `crates/lune-ui-core/src/widgets/gesture_track.rs` [D369].

## Spec

### Evidence

Pinned: Lunegit 33a1dd41, Wane 69931d6, this repo 2898b58. Read: `crates/lune-ui-core/src/widgets/dialog.rs`, `crates/lune-ui-core/src/widgets/popover.rs`, `crates/lune-ui-core/src/ui/overlay.rs`, `crates/lune-ui-core/src/widgets/gesture_track.rs`.
Neither Lunegit nor Wane has a bottom sheet; the look is a deliberate deviation (hard rule 8) with the theme's frost and radii and Apple's commonly cited sizes (a 36 by 5 grabber) as starting values [D370]. Conflicts: none.

### Anatomy

- Scrim: the dialog's plain scrim (`dialog_backdrop` over the whole window, no blur), faded by how far the sheet is up [D371].
- Surface: the parts of `glass_surface` (the blurred scene under it, `menu_glass` frost, a 1 px `menu_popup_border` ring kept to the top 12 px, so no line runs down the window's sides), the window's width, top corners radius 12; it runs 12 px below the window's bottom edge, so no bottom corner shows [D371].
- Largest height `H`: the window's height less the top safe area less 10 px. `Fraction(f)` is `f * H`, `Height(h)` is `min(h, H)`; detents are sorted by height, equal ones joined and any under half a px dropped; an empty list is `[Fraction(1.0)]` [D371].
- Grabber: a 36 by 5 pill (radius 2.5) in `fg_faint`, centred in a 20 px band at the sheet's top; its button is 72 by 20 and a finger's hit is the 44 by 44 target. Off, there is no band and the content starts at the top [D371].
- Content rect: inside the sheet under the band, ending at `window bottom - max(safe_area().bottom, keyboard_inset().bottom)`; the content is clipped to it [D365], [D371].

### States

| state | look | cite |
|---|---|---|
| opening, closing | the surface slides, the scrim fades with it | [D371] |
| rest at a detent | surface top at `window bottom - height` | [D371] |
| dragged | surface top follows the finger 1:1, past the top detent by the rubber band | [D369] |
| grabber hover, focus-visible | the ghost button's fill and ring | [R crates/lune-ui-core/src/widgets/nav_bar.rs:130] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| open, close by the app, grabber step | surface top | drawer | moderate | sine in-out | fade | [D3] |
| release after a drag | surface top | a critically damped spring from the finger's speed (every tier) | 0.3 s period | n/a | n/a | [D369] |

Never animated: the content, the grabber pill. Under Reduced the slide is a fade of the scrim and the sheet and under Off the sheet snaps.

### Input and focus

- Pointer: a touch pan on the sheet drags it, except where a scroll area with vertical content on top of the sheet takes the pan (the sheet then stays); a touch press on a moving sheet catches it; a tap on the scrim, Escape and an assistive technology's dismiss close it; a click on the grabber steps to the next detent and wraps. A click on the sheet reaches nothing under it. A mouse never drags.
- Keys: the grabber is the first tab stop (Enter and Space step); Tab never leaves the sheet. Layer: Modal (the Ui raises it above an older modal).
- Overlays: a modal overlay; focus returns where it was when the sheet is out. A drag released to closed sets `*open = false` and `dismissed`, with the spring already carrying it out.

### Retained state

- Transient: the track (position, spring or tween, the detent it rests at), whether it is closing, a caught finger.
- Persistent: none.
- App-owned: `open`.
- Exit retention: the sheet keeps calling the overlay itself while it slides out; its own open and close motions are none, so the overlay ends the frame the sheet stops calling it.

### Accessibility

- Role `dialog` named by `BottomSheet::name`, state modal, action dismiss; the grabber a `button` with the same name [infra.accessibility]. No new role.

### Strings

None: the name is the app's [D371].

### Gallery scenes

- `phone-sheet-medium`: the first detent at half height over the page, settled.
- `phone-sheet-large`: one full-height detent, settled.
- `phone-sheet-dragging`: a finger held on the grabber, the sheet between its detents.
- `phone-sheet-scrolling-content`: a list in the sheet, a finger holding it scrolled while the sheet stays.
- `phone-sheet-keyboard`: a focused field in the full-height sheet and the keyboard 300 px up: the content ends above it.

### Painter primitives

- Existing: fill, frost, backdrop, ring, push-xform, clip-rect
- New: none

### Acceptance

- Behaviour (`crates/lune-ui-core/tests/widget_bottom_sheet.rs`): the geometry per detent kind; open and close by the app, Escape and the scrim; a drag and its release to each detent and to closed; the rubber band; a catch; scrolling content and the grabber; the grabber click and Enter; focus held; the content rect above the keyboard; a disabled scope; no menu; the Reduced and Off tiers; a window resize.
- Motion (`crates/lune-ui-core/tests/mobile_motion.rs`, `sheet::`): the open, the app close, a release from between the detents and a fling to closed conform at 60 and 120 Hz.
- Theme reach: `m10` in `c4.rs` draws the sheet under a theme of distinct colours.

### Build

- Milestone: M6
- Depends on: overlay.dialog, widget.nav-bar, core.ui
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_bottom_sheet.rs::opening_slides_up_from_below_the_window_and_settles_at_the_first_detent, crates/lune-ui-core/tests/widget_bottom_sheet.rs::the_app_closing_slides_it_out_and_the_overlay_then_ends, crates/lune-ui-core/tests/widget_bottom_sheet.rs::a_drag_follows_the_finger_and_a_slow_release_picks_the_nearest_detent, crates/lune-ui-core/tests/widget_bottom_sheet.rs::a_pan_on_scrolling_content_scrolls_it_and_leaves_the_sheet_a_pan_on_the_grabber_moves_it, crates/lune-ui-core/tests/mobile_motion.rs::a_fling_release_to_closed_conforms_at_60_and_120`; page `bottom-sheet`; unverified here: a real finger and a real keyboard (no device).

### Limits

- Content that scrolls vertically keeps its own pans: a pull down at its top scrolls it back and does not drag the sheet (UIKit hands an at-top pull to the sheet) [D371].
- A sheet that is not modal (the page behind stays live) is not offered; it needs a pass-through rule for the page behind and can be added as an option [D371].
- A drag cancelled by the system settles on the nearest detent like a lift at rest, and dismisses when that is closed.

## Open questions

None.

## Notes for the planner

The sheet is the only M6 component that is an overlay; the others draw on the page. It reads `Ui::safe_area` and `Ui::keyboard_inset` itself because it owns the window's bottom edge [D371].
