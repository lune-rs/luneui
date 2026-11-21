---
id: layout.pull-to-refresh
title: Pull to refresh
area: layout
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/pull_to_refresh.rs, crates/lune-ui-core/tests/widget_pull_to_refresh.rs, apps/gallery/src/pages/pull_to_refresh.rs
owner: —
summary: Pull to refresh of a phone list: a scroll area that reports refresh when a finger pulls its content 64 px past the top and lets go, and holds a spinner in a 56 px band while the app refreshes
---

## What it is

`Ui::pull_to_refresh` is a `Ui::scroll_area` with the same options and content that reports `refresh` on the frame a finger's pan ends with at least 64 px of the content shown past its top. While the app passes `refreshing` the area starts 56 px lower and the toolkit's spinner turns in the band above it; the band opens and closes with the drawer motion. The app owns the refresh (it starts its work on `refresh` and passes `refreshing` until it is done) [D375].

## Needs

- The pull-down refresh of a phone list, with the finger, rubber band and way back of the scroll area's own touch scrolling (R-widget-6, R-motion-4).
- A refresh a mouse and a keyboard reach: the app's own control (a button, a menu entry) passing the same `refreshing`; the pull never fires from a wheel, a key or a mouse (R-widget-3).

## Existing code

The area is `Ui::scroll_area` (`crates/lune-ui-core/src/widgets/scroll_area.rs`); the pull's overscroll is read through its crate-private `Ui::scroll_overscroll`; the band's motion is the crate-private `Track` of `crates/lune-ui-core/src/widgets/gesture_track.rs` [D369]; the indicator is `Ui::spinner` and `spinner::paint` (`crates/lune-ui-core/src/widgets/spinner.rs`).

## Spec

### Evidence

Pinned: Lunegit 33a1dd41, Wane 69931d6, this repo b030e35. Read: `crates/lune-ui-core/src/widgets/scroll_area.rs`, `crates/lune-ui-core/src/widgets/spinner.rs`, `crates/lune-ui-core/src/widgets/gesture_track.rs`.
Neither Lunegit nor Wane has a pull to refresh; the look is a deliberate deviation (hard rule 8): a 64 px trigger and a 56 px band (the commonly cited phone values), the spinner glyph at 20 px in `fg_secondary`, a ring of 8 px radius and 2 px stroke for the pull's progress [D375]. Conflicts: none.

### Anatomy

- Rect: the area the app gives; the scroll area takes `rect` less the band at its top (`rect.y + band`, `rect.h - band`).
- Band: 0 to 56 px at the rect's top; 56 while the app refreshes.
- Strip: from the rect's top to the content's top (the band plus what the pull shows past the top); the indicator is clipped to it, so it never draws over what is above the rect or over the content.
- Indicator: centred on the rect's width and on the strip's height (the band's centre at rest while refreshing); while pulling, a ring from 12 o'clock clockwise over `min(shown / 64, 1)` of a turn in `fg_secondary` faded by the same share; at or past 64 the spinner glyph; while the band is open the spinner, turning while busy.

### States

| state | look | cite |
|---|---|---|
| rest | the scroll area alone, no indicator | [D375] |
| pulling | the ring, its sweep `shown / 64` of a turn, in the strip above the content | [D375] |
| armed (64 or more shown) | the whole spinner glyph, unturned | [D375] |
| refreshing | the band open, the spinner turning in it | [D375] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| `refresh` or `refreshing` on | the band's height, 0 to 56 | drawer | moderate | sine in-out | snap | [D3] |
| `refreshing` off | the band's height, 56 to 0 | drawer | moderate | sine in-out | snap | [D3] |
| the release of a pull | the content's overscroll | the scroll area's spring (every tier) | n/a | n/a | n/a | [D350] |

Never animated: the trigger (one frame), the first frame (the band starts where the flag says).

### Input and focus

- Pointer: a touch pan is the scroll area's own; the frame the pan ends with 64 px or more shown past the top fires `refresh`, once per pan; never from a wheel, a key or a mouse, never under `Overscroll::Clamp`, never while `refreshing`.
- Keys: the scroll area's own. Tab stop: as the scroll area. Layer: the layer the app draws on.

### Retained state

- Transient: the spinner's phase (`Ui::spinner`).
- Persistent: the band's track and its target.
- App-owned: `refreshing`, the refresh itself, the content.
- Exit retention: none.

### Accessibility

- While the band is open the spinner is a `Progress` node named `luneui.refresh.busy`, `Busy` while the refresh runs; the content's nodes are the scroll area's [infra.accessibility]. No new role.

### Strings

- `luneui.refresh.busy`: "Refreshing" [D375]

### Gallery scenes

- `phone-refresh-pulling`: a finger down with the list pulled part way: the ring partly drawn in the strip under the bar.
- `phone-refresh-armed`: the finger past the threshold: the whole spinner glyph in the strip.
- `phone-refresh-holding`: after the release with the app refreshing: the spinner in the 56 px band, the first row directly under it.
- `phone-refresh-idle`: the list at rest, no indicator.

### Painter primitives

- Existing: arc, clip-rect, rotated text (the spinner glyph)
- New: none

### Acceptance

- Behaviour (`crates/lune-ui-core/tests/widget_pull_to_refresh.rs`): rest with nothing repainting, a release at 64 refreshing and one at 63 not, one trigger per pan, no trigger while refreshing, none from a wheel, a mouse or a clamped area, the content's top continuous across the release and ending 56 lower, the spinner a busy progress node centred in the band, the band closing when the flag drops, a flag on from the first frame and the Reduced and Off tiers cutting, the scroll offset unchanged by the band, a disabled scope.
- Motion (`crates/lune-ui-core/tests/mobile_motion.rs`, `refresh::`): the release into the hold and the band closing conform at 60 and 120 Hz, read from the content's first row node.
- Theme reach: `m10` in `c4.rs` draws a pull to refresh holding its band.

### Build

- Milestone: M6
- Depends on: layout.scroll-area, widget.spinner
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_pull_to_refresh.rs::a_release_at_64_px_refreshes_and_one_at_63_does_not, crates/lune-ui-core/tests/widget_pull_to_refresh.rs::the_wheel_the_mouse_and_a_clamped_area_never_fire, crates/lune-ui-core/tests/widget_pull_to_refresh.rs::the_content_top_is_continuous_across_the_release_and_ends_56_lower, crates/lune-ui-core/tests/mobile_motion.rs::the_release_into_the_hold_conforms_at_60_and_120`; page `pull-to-refresh`; unverified here: a real finger (no device).

### Limits

- Only a pull past the top refreshes; a pull past the bottom is the scroll area's rubber band alone.
- A mouse, a wheel and a keyboard never pull: the app gives them a refresh control of its own.
- Under `Overscroll::Clamp` nothing shows past the top, so the pull never fires.

## Open questions

None.

## Notes for the planner

The component reads the scroll area's overscroll through a crate-private reader; `ScrollResponse` is unchanged.
