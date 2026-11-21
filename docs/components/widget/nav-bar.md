---
id: widget.nav-bar
title: Navigation bar
area: widget
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/nav_bar.rs, crates/lune-ui-core/tests/widget_nav_bar.rs, apps/gallery/src/pages/nav_bar.rs
owner: —
summary: Navigation bar of a phone screen: a 44 px row at the bottom of the rect the app gives it, with a back button, an elided title and up to three action buttons
---

## What it is

`Ui::nav_bar` draws the top bar of a phone screen: a background that fills the rect it is given, a hairline on its bottom edge, and a 44 px row at the rect's bottom holding an optional back button, the title and up to three action buttons. The app gives it `safe_area().top + HEIGHT` tall at the window's top, so the row sits under the notch and the colour runs up behind it. Titles, labels and names are the app's: the bar has no string of its own [D370].

## Needs

- A phone screen's header, drawn from the theme with every text elided to its room (R-widget-6).
- Every button reachable by touch within a 44 by 44 target, by mouse and by keyboard, with a semantic node (R-widget-3, R-widget-6).

## Existing code

none. Lunegit has no phone reference; the toolbar (`widget.toolbar`) is the nearest action row [R crates/lune-ui-core/src/widgets/toolbar.rs:88].

## Spec

### Evidence

Pinned: Lunegit 33a1dd41, Wane 69931d6, this repo e4d6ab7. Read: `crates/lune-ui-core/src/widgets/toolbar.rs`, `crates/lune-ui-core/src/widgets/icon_rail.rs`, `crates/lune-ui-core/src/widgets/button.rs`.
Neither Lunegit nor Wane has a phone navigation bar; the look is a deliberate deviation (hard rule 8) with Apple's commonly cited sizes (a 44 pt row, a 20 pt glyph) as starting values [D370]. Conflicts: none.

### Anatomy

- Background: `surface_overlay` over the whole rect, the safe-area band included [D370]. Hairline: one device pixel (`1 / scale` px) in `surface_separator` on the bottom edge [D370].
- Row: the rect's bottom 44 px (`HEIGHT`); a rect shorter than 44 lays the row from its top and clips [D370].
- Back button: `icon::ARROW_LEFT` at 20 px, 8 px from the left edge, then 4 px, then the app's label at `font_title`; both `accent`; the label elided to 35 % of the bar's width; the button is as wide as its content plus 8 on each side and 44 high [D370].
- Title: `font_title`, bold, `fg_primary`, one line; centred on the bar's width when the back button and the actions leave 8 px clear on both sides at that place, else in the space between them; elided to that space [D370], [D114].
- Actions: up to three 44 by 44 cells, the last ending 8 px from the right edge, each a 20 px glyph in `accent` [D370].

### States

| state | look | cite |
|---|---|---|
| rest | transparent buttons | [R crates/lune-ui-core/src/widgets/button.rs:121] |
| hover | `surface_hover` cell, radius `item_radius` | [R crates/lune-ui-core/src/widgets/button.rs:121] |
| pressed | `surface_active` cell | [R crates/lune-ui-core/src/widgets/button.rs:121] |
| focus-visible | the button ring on the cell | [R crates/lune-ui-core/src/widgets/button.rs:121] |
| disabled | inert, node marked disabled | [core.ui] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| rest ↔ hover or press | cell fill | hover | instant | linear | keep | [motion] |
| focus-visible | ring opacity | focus | instant | linear | keep | [motion] |

Never animated: the background, the hairline, the title and the glyphs.

### Input and focus

- Pointer: a click or a tap on back sets `back`; on action `i` sets `action = Some(i)`. A finger's hit is the touch target's 44 by 44 around a smaller button, in hit testing only [D361].
- Keys: each button is its own tab stop; Enter and Space activate it [core.focus]. No hover-only action.
- Layer: the layer the app draws on (no overlay, no focus scope).

### Retained state

- Transient: hover, press and ring tweens [core.ui].
- Persistent: none.
- App-owned: the title, the back label, the actions, and what each does.
- Exit retention: no.

### Accessibility

- Role `toolbar` named by the title; each button a `button` named by the app's label (`back` the back label, an action its `label`); states focusable, disabled; action `default` [infra.accessibility]. No new role.

### Strings

None: the title, the back label and the action names are the app's [D370].

### Gallery scenes

- `phone-nav-bar-title`: a title alone on the bar, settled.
- `phone-nav-bar-back-actions`: a back button, a title and two actions, settled.
- `phone-nav-bar-long-title`: the same with a title too long for the room: elided with an ellipsis, 8 px clear of both sides, settled.

### Painter primitives

- Existing: fill, text-centered, elide, clip-rect, icon
- New: none

### Acceptance

- Behaviour (`crates/lune-ui-core/tests/widget_nav_bar.rs`): the row, the whole-rect fill and the hairline; a short rect; the centred and the elided title; a click and a tap on back and on an action; Enter and Space; a finger beside a narrow button; a disabled scope and `disabled_violations`; at most three actions.
- Theme reach: `m10` in `c4.rs` draws the bar under a theme of distinct colours.
- Semantic node: the toolbar named by the title, the buttons named by the app.

### Build

- Milestone: M6
- Depends on: widget.button, widget.toolbar, core.focus, infra.accessibility
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_nav_bar.rs::the_row_is_the_bottom_44_and_the_background_fills_the_whole_rect, crates/lune-ui-core/tests/widget_nav_bar.rs::a_long_title_is_elided_between_the_buttons_and_never_wrapped, crates/lune-ui-core/tests/widget_nav_bar.rs::a_tap_or_click_on_back_and_on_an_action_reports_it, crates/lune-ui-core/tests/widget_nav_bar.rs::a_disabled_scope_is_inert_and_leaves_no_violation`; page `nav-bar`; unverified here: a real notch and a real finger (no device).

### Limits

- The buttons' accent is the theme's: the dark preset's is a dim grey, so the back and action glyphs read quiet on it; a theme with a vivid accent shows them as links [D370].

## Open questions

None.

## Notes for the planner

The pair of the bottom tab bar (`widget.tab-bar`). The bar never reads `safe_area()`: a bar inside a sheet or a split must not pad for the notch [D370].
