---
id: data.battery
title: Battery
area: data
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/battery.rs, crates/lune-ui-core/tests/widget_battery.rs, apps/gallery/src/pages/battery.rs
owner: —
summary: A battery glyph: a rounded outline with a nub on its right end, a fill to the level coloured by the level, and a bolt while charging
---

## What it is

A battery: a rounded outline with a small nub on its right end, a fill inside it to the level, coloured green, amber or red by the level (or by the app), and a bolt over the fill while charging. It draws from the painter only (`ring`, `fill`, `fill_path`), so it runs unchanged on every target the toolkit builds for, and the bolt is a polygon, never a font glyph. Design choices: [D437].

## Needs

- Custom drawing through the painter (R-data-1).
- A level and a charging flag from app data (R-data-2).

## Existing code

None (new in 0.7.8). The nearest is `widget.progress` (a bar with the same value semantics and no animation of a determinate value).

## Spec

### Evidence

Pinned: this repo `orch/sysmon-widgets` on main `cce0037`. Read: `crates/lune-ui-core/src/paint.rs` (`ring`, `fill`, `fill_path`), `crates/lune-ui-core/src/widgets/progress.rs` (`clamp`, node).
Conflicts: none. The look follows the user's reference screenshot of a menu-bar monitor panel (a green battery with a bolt at 80 percent); no Lunegit or Wane source has one.

### Anatomy

- Body: the rect less the nub's width, the rect's full height; radius 28 percent of the height; a 1 px outline in `fg_tertiary` [D437].
- Nub: 8 percent of the rect's width (2 to 4 px) by 40 percent of its height, in `fg_tertiary`, against the body's right end, centred vertically.
- Fill: inset 2 px from the outline, `level` of the inner width, full inner height, in the level colour; radius the body's less the inset.
- Bolt: six corners, 72 percent of the inner height high and 0.6 of that wide, centred in the body, in `on_accent`, over the fill.
- Level colour: `ok` while charging or above 0.2; `warn` from 0.2 down to above 0.1; `err` at 0.1 or below.

### States

| state | look | cite |
|---|---|---|
| level above 0.2 | green fill | [D437] |
| 0.2 down to above 0.1 | amber fill | [D437] |
| 0.1 or below | red fill | [D437] |
| level 0, or not finite | outline and nub only | [D437] |
| level past 1 or below 0 | cut to the end | [D437] |
| charging | green fill at any level and the bolt | [D437] |
| a colour from the app | that colour in place of the level colour | [D437] |
| a rect with no size | nothing drawn; the node stays | [D437] |

### Transitions

Never animates: a new level redraws at once; a level is a reading, as a determinate progress bar [D437].

### Input and focus

- Pointer: none. Keys: none. Tab stop: no. Layer: whatever the owner draws it on.
- Overlays: none.

### Retained state

- Transient: none.
- Persistent: none.
- App-owned: the level, the charging flag, the colour.
- Exit retention: no.

### Accessibility

- Role `progress` named by `label` (fallback `luneui.battery.name`), value `Number` the level as a percentage from 0 to 100 (step 1), description `luneui.battery.charging` while charging; no children.

### Strings

- `luneui.battery.name`: "Battery" (the fallback name).
- `luneui.battery.charging`: "Charging" (the description while charging).

### Gallery scenes

- `battery-full`: level 1, settled.
- `battery-charging`: level 0.8, charging, settled.
- `battery-low`: level 0.15, settled.
- `battery-critical`: level 0.05, settled.
- `battery-empty`: level 0, settled.

### Painter primitives

- Existing: `ring` (the outline), `fill` (the nub and the fill), `fill_path` (the bolt). New: none.

### Acceptance

- The colour bands at each boundary (1, 0.21, 0.2, 0.1001, 0.1, 0, not finite, charging); the fill's width and place; the level clamped and no fill at 0; a colour from the app; the nub at the right end, centred, and the outline as tall as the rect; the bolt only while charging and centred in the body; any aspect stays inside the rect and a rect with no size draws nothing; no animation.
- Node: a `progress` with a name, a percentage value and a charging description; the fallback names through the table; a disabled scope marks it.
- Theme: `c4::m10` draws it charging, low and critical under a theme of distinct colours.
- Tests, bullet by bullet. `crates/lune-ui-core/tests/widget_battery.rs`: `the_colour_bands_hold_at_each_boundary`, `the_fill_is_the_level_of_the_inner_width_in_the_level_colour`, `a_level_not_usable_is_clamped_and_zero_draws_no_fill`, `a_colour_from_the_app_replaces_the_level_colour`, `the_nub_is_on_the_right_end_centred_vertically`, `the_bolt_is_drawn_only_while_charging_and_centred_in_the_body`, `any_aspect_and_a_rect_with_no_size_do_not_draw_outside_or_panic`, `a_new_level_redraws_at_once_and_nothing_animates`, `the_battery_is_a_progress_with_the_level_as_a_percentage`, `an_empty_label_falls_to_the_toolkit_string_through_the_table`, `a_disabled_scope_marks_the_node`; `crates/lune-ui-core/tests/c4.rs::no_fill_or_text_holds_a_preset_colour_under_a_theme_of_distinct_colours`.

### Build

- Milestone: sysmon-widgets step 3.
- Depends on: core.painter, core.theme, infra.accessibility.
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_battery.rs::the_colour_bands_hold_at_each_boundary, crates/lune-ui-core/tests/widget_battery.rs::the_fill_is_the_level_of_the_inner_width_in_the_level_colour, crates/lune-ui-core/tests/widget_battery.rs::the_nub_is_on_the_right_end_centred_vertically, crates/lune-ui-core/tests/widget_battery.rs::the_bolt_is_drawn_only_while_charging_and_centred_in_the_body, crates/lune-ui-core/tests/widget_battery.rs::the_battery_is_a_progress_with_the_level_as_a_percentage`; page `battery`; unverified here: none.

## Open questions

None.

## Notes for the planner

`Ui::battery(key, rect, &BatteryOptions::new(0.8, "Charge").with_charging(true).with_color(c))` returns nothing; `battery::level_color(&theme, level, charging)` is the colour the fill takes without an app colour. The outline and nub use `fg_tertiary`, the bolt `on_accent`. The options are `#[non_exhaustive]`: build them with `new` and `with_*`. Limit: the bolt is centred in the body, so at a low level it can lie over the empty part.
