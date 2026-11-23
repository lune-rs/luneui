---
id: data.sparkline
title: Sparkline
area: data
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/sparkline.rs, crates/lune-ui-core/tests/widget_sparkline.rs, apps/gallery/src/pages/sparkline.rs
owner: —
summary: A history graph with no axes, as a line with a soft fill or a strip of bars, with an optional second series mirrored below the baseline
---

## What it is

A sparkline: a history of samples in an inset rounded frame, drawn as a line with a soft fill under it or as a strip of bars, with an optional second series mirrored below a baseline through the middle. The newest sample is at the right edge. One component covers a single history, a pair (two directions on one scale) and a per-slot strip. It draws from the painter only (`stroke`, `fill_path`, `fill`), so it runs unchanged on every target the toolkit builds for. Design choices: [D436].

## Needs

- Custom drawing through the painter (R-data-1).
- Series from app data with a colour each (R-data-2).

## Existing code

None (new in 0.7.8). The nearest is `data.bar-chart` (stacked columns with grid lines, labels and a legend).

## Spec

### Evidence

Pinned: this repo `orch/sysmon-widgets` on main `cce0037`. Read: `crates/lune-ui-core/src/paint.rs` (`stroke`, `fill_path`, `fill`), `crates/lune-ui-core/src/paint/path.rs` (`MAX_FILL_POINTS`), `crates/lune-ui-core/src/widgets/bar_chart.rs` (node).
Conflicts: none. The look follows the user's reference screenshot of a menu-bar monitor panel (traffic graphs mirrored about a baseline, a per-core strip); no Lunegit or Wane source has one.

### Anatomy

- Frame: the rect filled with `surface_input`, radius 4; the graph is inset 4 px on every side [D436].
- Slots: `capacity` (0: the longer series' length), at most `MAX_CAPACITY` = 1024; each `graph width / slots` wide; the newest sample is in the last slot.
- Baseline: a 1 px `border` line across the graph, at the bottom of the graph for one series and at its middle for a mirrored pair (the series then grow up and down, half the graph high each).
- Line: a 1 px stroke through the slot centres in the series colour, and a filled polygon between it and the baseline in the same colour at a quarter of its alpha.
- Bars: one cell per slot (`surface_overlay`, width the pitch less 1 px, radius at most 2), and a fill of the sample's share of the height in the series colour from the baseline.

### States

| state | look | cite |
|---|---|---|
| samples | the line and its fill, or the bars | [D436] |
| fewer samples than slots | right-aligned; the left stays empty (bars: empty cells) | [D436] |
| more samples than slots | only the newest `capacity` are drawn | [D436] |
| no samples, all 0, or none finite | the frame and the baseline only | [D436] |
| a sample not finite or below 0 | 0 | [D436] |
| a sample above `max` | clamped to the top | [D436] |
| a rect with no graph inside its inset | nothing drawn; the node stays | [D436] |

### Transitions

Never animates: a new sample redraws the graph at once; the history moves by the app's samples [D436].

### Input and focus

- Pointer: none. Keys: none. Tab stop: no. Layer: whatever the owner draws it on.
- Overlays: none.

### Retained state

- Transient: none.
- Persistent: none.
- App-owned: the samples, the colours, the scale and the slot count.
- Exit retention: no.

### Accessibility

- Role `image` named by `label` (fallback `luneui.sparkline.name`), `description` the app's summary of the data; no children.

### Strings

- `luneui.sparkline.name`: "Sparkline" (the fallback name).

### Gallery scenes

- `sparkline-line`: one series as a line, 60 samples, settled.
- `sparkline-mirror`: two series about a middle baseline, settled.
- `sparkline-bars`: 10 samples as bars, settled.
- `sparkline-sparse`: 3 samples in 20 slots, settled.
- `sparkline-empty`: no samples, settled.

### Painter primitives

- Existing: `fill` (frame, baseline, cells, bars), `stroke` (the line), `fill_path` (the line's fill). New: none.

### Acceptance

- `scale` for none, all 0, not finite, below 0, above `max` and an automatic top; the newest sample at the last slot's centre and a short series leaving the left empty; only the newest slots of a long series, with the automatic top taken over those; the baseline at the bottom inset or the rect's middle; a line with a fill in its colour at a quarter alpha; a series with runs of 0 still filled; nothing to draw leaves the frame and baseline; bars (one cell per slot, a zero sample shows only its cell, filled from the baseline, a mirrored strip both ways); the clamp; nothing outside the rect; no animation.
- Node: an `image` with a name and a description; the fallback name through the table; a disabled scope marks it.
- Theme: `c4::m10` draws both styles under a theme of distinct colours.
- Tests, bullet by bullet. `crates/lune-ui-core/tests/widget_sparkline.rs`: `scale_maps_to_zero_one`, `the_newest_sample_sits_at_the_last_slots_centre_and_a_short_series_leaves_the_left_empty`, `a_longer_series_shows_only_the_newest_slots`, `one_series_has_its_baseline_at_the_bottom_inset`, `a_mirrored_pair_has_its_baseline_in_the_middle_and_grows_both_ways_on_one_scale`, `a_line_is_a_stroke_with_a_fill_at_a_lower_alpha_in_the_same_hue`, `a_series_with_zero_runs_still_fills`, `nothing_to_draw_leaves_the_frame_and_the_baseline`, `bars_are_one_cell_per_slot_filled_from_the_baseline`, `a_short_strip_leaves_empty_cells_on_the_left`, `a_mirrored_strip_fills_both_ways_from_the_middle`, `a_capacity_past_the_clamp_draws_the_clamped_slots_and_a_drawable_fill`, `nothing_is_drawn_outside_the_rect`, `a_new_sample_redraws_at_once_and_nothing_animates`, `the_sparkline_is_an_image_named_by_the_app_with_a_description`, `a_disabled_scope_marks_the_node`; `crates/lune-ui-core/tests/c4.rs::no_fill_or_text_holds_a_preset_colour_under_a_theme_of_distinct_colours`.

### Build

- Milestone: sysmon-widgets step 2.
- Depends on: core.painter, core.theme, infra.accessibility.
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_sparkline.rs::scale_maps_to_zero_one, crates/lune-ui-core/tests/widget_sparkline.rs::the_newest_sample_sits_at_the_last_slots_centre_and_a_short_series_leaves_the_left_empty, crates/lune-ui-core/tests/widget_sparkline.rs::a_mirrored_pair_has_its_baseline_in_the_middle_and_grows_both_ways_on_one_scale, crates/lune-ui-core/tests/widget_sparkline.rs::bars_are_one_cell_per_slot_filled_from_the_baseline, crates/lune-ui-core/tests/widget_sparkline.rs::a_capacity_past_the_clamp_draws_the_clamped_slots_and_a_drawable_fill, crates/lune-ui-core/tests/widget_sparkline.rs::the_sparkline_is_an_image_named_by_the_app_with_a_description`; page `sparkline`; unverified here: none.

## Open questions

None.

## Notes for the planner

`Ui::sparkline(key, rect, &SparklineOptions::new(&samples, color, label).with_mirror(&other, color2).with_max(top).with_capacity(slots).with_style(SparkStyle::Bars))` returns nothing; `sparkline::scale(&[f32], max)` maps samples to 0..=1. Limits: `capacity` is clamped to `MAX_CAPACITY` (1024 slots), because `fill_path` draws nothing above 4096 points and its ear clipping is quadratic in them; a history longer than that shows its newest 1024 samples (the app resamples if it wants the whole span). The line has no points to join in a single slot, so a capacity of 1 draws only the baseline. Both series share one top. The options and `SparkStyle` are `#[non_exhaustive]`: build them with `new` and `with_*`.
