---
id: widget.tab-bar
title: Bottom tab bar
area: widget
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/tab_bar.rs, crates/lune-ui-core/tests/widget_tab_bar.rs, apps/gallery/src/pages/tab_bar.rs
owner: —
summary: Bottom tab bar of a phone screen: a 49 px row of equal items (glyph, label, optional badge) at the top of the rect the app gives it, one tab stop with roving focus
---

## What it is

`Ui::tab_bar` draws the bottom bar of a phone screen: a background that fills the rect it is given, a hairline on its top edge, and a 49 px row at the rect's top of equal-width items, each a glyph over a label with an optional badge. The app gives it `HEIGHT + safe_area().bottom` tall at the window's bottom, so the colour runs down behind the home indicator. It is the mirror of `widget.nav-bar` [D370].

## Needs

- A phone app's primary navigation, drawn from the theme with every label elided to its item (R-widget-6).
- One tab stop with roving focus, a tap or key that selects, `reselected`, and `context` for a long press or secondary click without opening a menu (R-widget-3, R-widget-6).

## Existing code

none. Lunegit has no phone reference; the icon rail is the nearest roving composite [R crates/lune-ui-core/src/widgets/icon_rail.rs:104].

## Spec

### Evidence

Pinned: Lunegit 33a1dd41, Wane 69931d6, this repo e4d6ab7. Read: `crates/lune-ui-core/src/widgets/icon_rail.rs`, `crates/lune-ui-core/src/widgets/tab_strip.rs`, `crates/lune-ui-core/src/widgets/nav_item.rs`.
Neither Lunegit nor Wane has a bottom tab bar; the look is a deliberate deviation (hard rule 8) with Apple's commonly cited sizes (a 49 pt row) as starting values [D370]. Conflicts: the plan's wording tints the selected item with `accent`; the preset's accent is a dim grey that reads darker than the unselected text, so the winner is the icon rail's look (`fg_primary` and a `surface_selected` cell) [D370].

### Anatomy

- Background: `surface_overlay` over the whole rect, the home-indicator band included. Hairline: one device pixel in `surface_separator` on the top edge [D370].
- Row: the rect's top 49 px (`HEIGHT`); items equally wide across the rect [D370].
- Item: a 24 px glyph centred horizontally with its line box 2 px below the row's top; a `font_s` label centred, its line box ending 5 px above the row's bottom, elided to the cell less 4 px [D370], [D114].
- Selected cell: a `surface_selected` fill inset 6 by 4 from the item's cell at radius `item_radius`; glyph and label `fg_primary`. Other items: `fg_tertiary` [D370].
- Badge: the app's text in a `count_badge` pill, `on_accent`, 11 px bold, 16 high and at least 16 wide, its left edge 6 px right of the glyph's centre and its top 2 px below the row's top, kept inside the cell [D370].

### States

| state | look | cite |
|---|---|---|
| rest | `fg_tertiary` glyph and label | [D370] |
| selected | `fg_primary` and the `surface_selected` cell | [D370] |
| hover | `surface_hover` over the cell | [R crates/lune-ui-core/src/widgets/button.rs:121] |
| pressed | `surface_active` over the cell; the selection does not move | [R crates/lune-ui-core/src/widgets/button.rs:121] |
| focus-visible | the button ring on the cell | [R crates/lune-ui-core/src/widgets/button.rs:121] |
| disabled | inert, node marked disabled | [core.ui] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| select another item | cell fill, glyph and label colour | selection | fast | quad-out | keep | [motion] |
| rest ↔ hover or press | cell fill | hover | instant | linear | keep | [motion] |

Never animated: the background, the hairline, the badge.

### Input and focus

- Pointer: a click or a tap selects the item and sets `changed`; on the selected item it sets `reselected`; a secondary click, the show-menu action or a long press sets `context = Some(i)` and opens no menu, and the release after a long press selects nothing [D292], [D361]. A finger's hit is the touch target's 44 by 44 [D361].
- Keys: one tab stop, at the selected item; Left and Right move focus (wrapping), Home and End go to the ends; Enter and Space select the focused item [core.focus]. No hover-only action.
- Layer: the layer the app draws on (no overlay, no focus scope).

### Retained state

- Transient: hover, press, selection tweens [core.ui].
- Persistent: none.
- App-owned: the selected index (`&mut usize`, left alone when out of range and clamped for drawing), the items and the badges.
- Exit retention: no.

### Accessibility

- Role `tab-list` named `luneui.tab-bar.name`, containing `tab`s named by the labels, the selected one marked selected, the badge text as the description; states focusable, disabled; actions default, focus and show-menu [infra.accessibility]. No new role.

### Strings

- `luneui.tab-bar.name`: "Tabs" [D370]. The labels and badges are the app's.

### Gallery scenes

- `phone-tab-bar-five`: five items, the first selected, settled.
- `phone-tab-bar-badge`: the third selected, badges "3" and "99+" on two items, settled.
- `phone-tab-bar-pressed`: a finger down on the fourth item, the first still selected; captured mid-press (the long press is due later).

### Painter primitives

- Existing: fill, text-centered, elide, clip-rect, icon
- New: none

### Acceptance

- Behaviour (`crates/lune-ui-core/tests/widget_tab_bar.rs`): the row, the whole-rect fill and the hairline; the selected marks and tints; a click and a tap select, a second one reselects; the press moves no selection; roving by the arrows, Home and End, Enter and Space; one tab stop among other widgets; a secondary click and a long press report `context`; an out-of-range selection; the badge; an elided label; a disabled scope.
- Theme reach: `m10` in `c4.rs`.
- Semantic nodes: a `tab-list` of `tab`s.

### Build

- Milestone: M6
- Depends on: widget.nav-bar, widget.tab-strip, core.focus, infra.accessibility
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_tab_bar.rs::the_row_is_the_top_49_and_the_background_fills_the_whole_rect, crates/lune-ui-core/tests/widget_tab_bar.rs::one_tab_stop_roves_with_arrows_home_and_end_and_enter_selects, crates/lune-ui-core/tests/widget_tab_bar.rs::a_click_or_a_tap_selects_and_a_second_one_reselects, crates/lune-ui-core/tests/widget_tab_bar.rs::a_secondary_click_and_a_long_press_report_context_and_select_nothing`; page `tab-bar`; unverified here: a real home indicator and a real finger (no device).

## Open questions

None.

## Notes for the planner

The pair of the navigation bar (`widget.nav-bar`). The bar never reads `safe_area()` [D370].
