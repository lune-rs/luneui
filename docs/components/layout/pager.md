---
id: layout.pager
title: Pager
area: layout
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/pager.rs, crates/lune-ui-core/tests/widget_pager.rs, apps/gallery/src/pages/pager.rs
owner: —
summary: Pager of a phone screen: pages side by side that a finger swipes between one at a time with a rubber band at the ends, and a row of dots with one tab stop
---

## What it is

`Ui::pager` draws `count` pages side by side in a rect, the current one filling it. A touch pan on x moves the pages 1:1 and the lift moves at most one page; a page the app sets slides in under the page-swap motion; a row of dots at the bottom shows the page and lets a mouse or a keyboard choose one. The app owns the pages (what each draws, `*page`); the pager owns the motion between them [D373].

## Needs

- The swipeable pages of a phone app (an onboarding, a photo strip), with the finger's follow, rubber band and one-page snap of a native one (R-widget-6, R-layout-3, R-layout-5).
- A way for a mouse and a keyboard to do what the swipe does: the dots (a click, Left, Right, Home, End) and the app's own `*page` (R-widget-3).

## Existing code

The motion is the crate-private `Track` and `Ui::pan_region` of `crates/lune-ui-core/src/widgets/gesture_track.rs` [D369]; the dots' roving focus is `Ui::rove_stop` and `rove_keys` as in `crates/lune-ui-core/src/widgets/tab_bar.rs`; the dots' tint moves with `nav_item::SELECTION`.

## Spec

### Evidence

Pinned: Lunegit 33a1dd41, Wane 69931d6, this repo f379c2c. Read: `crates/lune-ui-core/src/widgets/gesture_track.rs`, `crates/lune-ui-core/src/widgets/nav_stack.rs`, `crates/lune-ui-core/src/widgets/tab_bar.rs`.
Neither Lunegit nor Wane has a pager; the look is a deliberate deviation (hard rule 8): 7 px dots at a 16 px pitch (the page control's commonly cited size), the current one in the theme's `accent`, the others in `fg_faint`, on a scrim pill (`dialog_backdrop`) so they read on any page fill [D373]. Conflicts: none.

### Anatomy

- Rect: the area the app gives the pager; everything is clipped to it. No background of its own: a pull past an end shows what is under the rect.
- Pages: each as wide as the rect and as tall, page `i` at `rect.x + i * width - offset`; adjacent pages touch (no gutter). Only pages that intersect the rect are drawn, each in its own id scope.
- Dots (with `dots` on and more than one page): a row centred on the rect's width, centres 16 px apart (narrower when the rect is, to fit), their centres 14 px above the rect's bottom; each dot 7 px across, a 16 by 24 px cell; behind the row a pill 20 px high with 8 px of padding at each end in `dialog_backdrop`. The row draws over the pages and reserves no room.
- Pan region: the whole rect (x), registered before the pages, while `count` is more than one.

### States

| state | look | cite |
|---|---|---|
| rest | the current page alone | [D373] |
| swipe, spring, page set | two pages joined at an edge | [D373] |
| past an end | the first or last page shifted by the rubber band's share of the pull | [D373] |
| dot current | `accent` | [D373] |
| dot other | `fg_faint` | [D373] |
| dot focused | the ring around the dot | [D373] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| a page set by the app, a dot or a key, one page away | the pages' offset | page swap | moderate | quad out | snap | [D3] |
| release of a swipe | the pages' offset | a critically damped spring from the finger's speed (every tier) | 0.3 s period | n/a | n/a | [D369] |
| the current dot | the dots' tint | selection | fast | linear | keep | [D373] |

Never animated: the pages' content; a page set more than one page away, the first frame, a change of the rect's width or of `count` (they cut).

### Input and focus

- Pointer: a touch pan on x follows the finger 1:1, past the first or the last page through the rubber band; the lift moves at most one page, by the speed and the position it projects to (a fling over 300 px/s moves one page); a press on the moving pages catches them where they show; a vertical pan goes to a scroll area inside a page or around the pager, a page's own horizontal scroller takes a pan that starts on it; a mouse press, a drag, a wheel and a key never pan (the dots and `*page` are the mouse's and keyboard's way); a click or tap on a dot selects it.
- Keys: on the dots, Left and Right (no wrap), Home and End move focus to a page and select it; Enter and Space select the focused dot. Tab stop: the dots, one, entering at the current page's dot (roving focus, D113). Layer: the layer the app draws on.

### Retained state

- Transient: none beyond the frame.
- Persistent: the track, the page the last frame ended on, the rect's width and `count`, and whether a finger holds the pages.
- App-owned: `count`, `*page`, the pages' content.
- Exit retention: none.

### Accessibility

- The pager is a `Group` named by the app (`PagerOptions::name`); the dots a `TabList` named `luneui.pager.dots` holding `Tab` nodes named `luneui.pager.dot`, the current page's marked selected; states focusable, disabled; actions default and focus [infra.accessibility]. No new role.

### Strings

- `luneui.pager.dots`: "Pages" [D373]
- `luneui.pager.dot`: "Page {0} of {1}" [D373]

### Gallery scenes

- `phone-pager-first`: the first of four pages, dots over its bottom edge, settled.
- `phone-pager-mid-swipe`: a finger held a little over half a page across: two pages joined at one edge.
- `phone-pager-last-rubber`: a finger pulling past the last page: the page shifted by the rubber band's share of the pull.
- `phone-pager-no-dots`: the first page with the dots off.

### Painter primitives

- Existing: fill, clip-rect, text
- New: none

### Acceptance

- Behaviour (`crates/lune-ui-core/tests/widget_pager.rs`): no pages and the clamped page, only the current page at rest, the 1:1 follow with two pages joined, a fast fling moving one page and `changed` on the lift, a slow release past the middle and one short of it, a flick back, the rubber band at both ends, a catch, a page set by the app (slide, no `changed`, a cut across more than one page, the Reduced and Off tiers), a vertical pan reaching a list in a page, a page's horizontal scroller keeping its pan, a mouse drag, wheel and click, a tap and a swipe that start on a button, the dots (nodes, a click, a tap, one tab stop, the keys, Enter and Space), no dots, a disabled scope, id scopes of the pages, a resize or a changed `count`.
- Motion (`crates/lune-ui-core/tests/mobile_motion.rs`, `pager::`): a fling release to the next page, a short release springing back and a page set by the app conform at 60 and 120 Hz.
- Theme reach: `m10` in `c4.rs` draws a pager.

### Build

- Milestone: M6
- Depends on: core.ui
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_pager.rs::a_touch_pan_follows_the_finger_one_to_one_and_shows_two_pages_joined, crates/lune-ui-core/tests/widget_pager.rs::a_fast_fling_moves_exactly_one_page_and_reports_changed_on_the_lift, crates/lune-ui-core/tests/widget_pager.rs::the_pull_past_either_end_shows_the_rubber_band_and_springs_back, crates/lune-ui-core/tests/widget_pager.rs::the_dots_are_one_tab_stop_and_the_arrows_home_and_end_page, crates/lune-ui-core/tests/mobile_motion.rs::a_fling_release_to_the_next_page_conforms_at_60_and_120`; page `pager`; unverified here: a real finger (no device).

### Limits

- The pager paints no background and a page fills its rect itself.
- Dots show with more than one page; one page has nothing to swipe and no dots.
- A step of two or more pages (a far dot, the app's `*page`) cuts: no page flies past. Under the Reduced and Off tiers a page the app sets cuts.
- A dot's tap and its swipe share the pager's rect: a swipe that starts on a dot pages, as it does anywhere.

## Open questions

None.

## Notes for the planner

The pager is the component that moves app content by a track the finger drives on one axis; a page that draws at coordinates of its own, not the rect it is given, does not move.
