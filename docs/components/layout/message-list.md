---
id: layout.message-list
title: Message list
area: layout
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/message_list.rs, crates/lune-ui-core/tests/widget_message_list.rs, apps/gallery/src/pages/message_list.rs
owner: —
summary: A scrolling list of messages for a popover inbox (title, age, two lines of body, meta, an unread dot) with one roving tab stop, and the header above it
---

## What it is

`Ui::message_list` draws a `MessageRow` per message in a scroll area of at most `max_height` and reports the row a click, Enter or Space activated and the row a secondary click reached; `Ui::message_list_header` draws the title and an optional small ghost action above it. `message_list::height` sizes the popover before it opens. The app owns the rows, what a click does (open the message, mark it read) and the context menu; the list opens nothing [D444].

## Needs

- An inbox of notifications in a popover that an icon button opens, with an unread mark in the attention colour (R4 of the task, R-widget-3, R-layout-3).
- Keyboard use as a list with one tab stop (R-widget-3).

## Existing code

The rows scroll in `Ui::scroll_area` with `Bars::unless_fits` as `Ui::toast_history` does (`crates/lune-ui-core/src/widgets/toast.rs`); roving focus is `Ui::rove_stop` and `rove_keys` as in `crates/lune-ui-core/src/widgets/menu.rs` and `nav_item.rs`; the focused row scrolls in with `Ui::scroll_to`.

## Spec

### Evidence

Pinned: this repo at the task branch `orch/tab-attention-cue` (base cbfeab4). Read: `crates/lune-ui-core/src/widgets/toast.rs` (`toast_history`), `crates/lune-ui-core/src/widgets/menu.rs` (`menu_rows`), `crates/lune-ui-core/src/widgets/nav_item.rs`, `crates/lune-ui-core/src/widgets/scroll_area.rs`.
The look follows the consumer's mockup (NotifInbox): 12 px by 9 px padding, a 6 px dot and a 10 px gap, 2 px between lines, radius 6; neither Lunegit nor Wane has an inbox, so it is a deliberate deviation (hard rule 8) [D444]. Conflicts: none.

### Anatomy

- Row: the list's width, as tall as its lines: 9 px, the title line, 2 px and the body (at most two lines) when there is one, 2 px and the meta line when there is one, 9 px. Rows touch.
- Dot slot: 6 px wide at 12 px from the row's left, then a 10 px gap; the dot (6 px, `attention()`, centred on the title line) only when `unread`, the slot blank otherwise.
- Title: `font_base` Medium `fg_bright`, one line, elided to the room the age leaves (8 px gap). Age: `font_s` `fg_muted`, right-aligned at 12 px from the right edge, centred on the title line.
- Body: `font_base` `fg_primary`, wrapped to the text width, at most two lines; when more remain the second line ends in an ellipsis (built from the text system's lines, then `Painter::elide`). Meta: `font_s` `fg_muted`, one line, elided.
- Read rows draw every text at alpha 0.6.
- Fill: `surface_hover` on hover, `surface_active` pressed, radius 6; a focus ring (`RingKind::Edge`, radius 6) on the focused row when focus is visible.
- Scroll: the rows are in `scroll_area` within the rect, height at most `max_height` and the rect's height; vertical bar `Bars::Auto.unless_fits`, horizontal disabled.
- Header: `HEADER_HEIGHT` (40 px); the title `font_base` bold `fg_bright` at 12 px from the left, centred; the action a small (24 px) ghost button right-aligned 6 px from the edge.

### States

| state | look | cite |
|---|---|---|
| unread | the dot and full-strength text | [D444] |
| read | no dot, text at 0.6 | [D444] |
| hover, pressed | `surface_hover`, `surface_active` fill | [D444] |
| focus visible | the ring on the row | [D444] |
| disabled scope | no hover, no focus, keys consumed, nodes disabled | [D444] |
| empty | nothing is drawn, height 0, no tab stop | [D444] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| hover and press | the row's fill | hover | fast | linear | keep | [D444] |
| focus | the ring's opacity | focus | fast | linear | keep | [D444] |

Never animated: the dot (it appears with the row), the rows' content, the reveal of the focused row (the scroll area's own).

### Input and focus

- Pointer: a click on a row reports `clicked`; a secondary click or the show-menu action reports `context`; the wheel scrolls the rows (the wheel reaches the area around a list that fits). Keys: on a row, Up and Down (wrapping), Home and End move focus and scroll the row into view (`Reveal::Nearest`); Enter and Space report `clicked`, consumed before the enabled gate. Tab stop: one, entering at the first row (roving focus, D113). Layer: the layer the app draws on (a popover's content in the inbox).
- Overlays: none; the app's popover owns focus scope and dismissal.

### Retained state

- Transient: each row's fill and ring animations.
- Persistent: the scroll offset (the scroll area's, under the list's key).
- App-owned: the rows, which are unread, `max_height`.
- Exit retention: none.

### Accessibility

- A `List` named `MessageListOptions::label`; a `ListItem` per row named by its title, described by "Unread" (when it is), the body and the meta line joined by ", "; states focusable, disabled; actions default, focus, scroll into view and show menu (the show-menu action reports `context`, D292) [infra.accessibility]. The header's button is a `Button` named by its label.

### Strings

- `luneui.message-list.unread`: "Unread" [D444]

### Gallery scenes

- `message-list-rest`: the inbox in a 360 px glass popover under its bell button: the header with "Clear all", four rows (three unread, one with a two-line body, one read), settled.
- `message-list-scrolled`: twenty rows in a list capped at 320 px, scrolled by 130 px, a row cut at each end.

### Painter primitives

- Existing: fill, text, text_in, elide, ring, clip-rect (through the scroll area)
- New: none

### Acceptance

- Behaviour (`crates/lune-ui-core/tests/widget_message_list.rs`): a click, the keys (Down, Home, End, Enter, Space) and the one tab stop, the unread dot and the read alpha, a long body cut to two lines with an ellipsis, `row_height` and `height` (sum, cap, empty), a capped list that scrolls by wheel and keeps the focused row in view as Down walks past the bottom, the secondary click, the header action, the nodes, no rows, a disabled scope.
- Theme reach: `m10` in `c4.rs` draws a header and a list.

### Build

- Milestone: none (added for the notification cues of the 0.8 release)
- Depends on: core.ui, layout.scroll-area
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_message_list.rs::a_click_on_a_row_reports_its_index, crates/lune-ui-core/tests/widget_message_list.rs::the_arrows_and_the_end_keys_move_focus_and_enter_and_space_activate, crates/lune-ui-core/tests/widget_message_list.rs::a_capped_list_scrolls_and_keeps_the_focused_row_in_view, crates/lune-ui-core/tests/widget_message_list.rs::a_disabled_list_reports_nothing_and_marks_every_node`; page `message-list`; unverified here: a touch device (the rows follow the toolkit's 44 pt touch reach, with no test of their own).

### Limits

- The heights of every row are measured each frame (a body wraps, so a row's height needs its shaping; the text system caches it); only the rows in view are drawn. A very long inbox (thousands of rows) wants `virtualized-list`.
- A body is plain text, at most two lines; a row has no actions of its own (the app uses `context`).
- A row's text colours come from the theme's `fg_*` roles; a read row is the same colours at 0.6 alpha.

## Open questions

None.

## Notes for the planner

The list reports; the app acts. An inbox is the list inside a popover the app sizes with `message_list::height`, with the header above; the app's bell button is `Ui::icon_button` (a badge on it is a separate task step).
