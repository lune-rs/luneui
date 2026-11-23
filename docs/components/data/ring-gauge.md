---
id: data.ring-gauge
title: Ring gauge
area: data
status: done
consumers: any
source: crates/lune-ui-core/src/widgets/ring_gauge.rs, crates/lune-ui-core/tests/widget_ring_gauge.rs, apps/gallery/src/pages/ring_gauge.rs
owner: —
summary: A circular track with coloured arc segments laid end to end from the top and a centred reading; the gauge of a menu-bar monitor panel
---

## What it is

A ring gauge: a circular track, coloured arc segments laid end to end clockwise from the top (each a fraction of the whole ring) and an optional text centred inside, which the app formats. It draws from the painter only (`arc` strokes and text), so it runs unchanged on every target the toolkit builds for. Design choices: [D435].

## Needs

- Custom drawing through the painter (R-data-1).
- Segments from app data with a colour each (R-data-2).

## Existing code

None (new in 0.7.8). The nearest are `data.pie-chart` (a fan with no centre, track or partial coverage) and `widget.progress` (a bar).

## Spec

### Evidence

Pinned: this repo `orch/sysmon-widgets` on main `cce0037`. Read: `crates/lune-ui-core/src/paint.rs` (`arc`, `stroke_pts`, `text_centered`, `elide`), `crates/lune-ui-core/src/widgets/pie_chart.rs` (node, tween), `crates/lune-ui-core/src/ui/animate.rs` (`animate_f32`, `snap_f32`).
Conflicts: none. The look follows the user's reference screenshot of a menu-bar monitor panel (rings with coloured segments and a percentage); no Lunegit or Wane source has a gauge.

### Anatomy

- The ring is the largest circle in the rect, centred; its outer edge touches the square's sides. Default thickness 8 px (`THICKNESS`), at most half the diameter [D435].
- Track: one full circle in `surface_overlay`. Segments: `arc` strokes in the app's colour, from -π/2 clockwise, each sweeping `value * 2π`; butt ends (the dump shows flat ends), so no sweep is trimmed.
- Text: `fg_primary`, size 24 percent of the diameter and at least `font_base`, elided to the inner diameter, centred in the inner square.

### States

| state | look | cite |
|---|---|---|
| segments | the track and one arc per segment with a value above 0 | [D435] |
| none, or all values 0 | the track alone | [D435] |
| values past a whole turn | the segment that crosses it is cut, later ones draw nothing | [D435] |
| value not finite or below 0 | that segment sweeps 0 | [D435] |
| a rect with no size | nothing drawn; the node stays | [D435] |

### Transitions

| change | property | kind | duration | easing | reduced | cite |
|---|---|---|---|---|---|---|
| a segment's value | its sweep | `value` | fast (180 ms) | quad-out | snap | [D435] |

Never animated: the colours, the text, the thickness, a segment appearing or leaving (a new key starts at its value; one the app stops passing leaves at once). `live` snaps every change.

### Input and focus

- Pointer: none. Keys: none. Tab stop: no. Layer: whatever the owner draws it on.
- Overlays: none.

### Retained state

- Transient: one tween per segment key.
- Persistent: none.
- App-owned: the segments, the text.
- Exit retention: no.

### Accessibility

- Role `progress` named by `label` (fallback `luneui.ring-gauge.name`), value `Number` from 0 to 1 (the covered fraction), description the text when not empty; no children.

### Strings

- `luneui.ring-gauge.name`: "Ring gauge" (the fallback name). The text inside is the app's.

### Gallery scenes

- `ring-gauge-single`: one segment (0.36) and "36%", settled.
- `ring-gauge-segments`: three segments (0.55, 0.15, 0.09), settled.
- `ring-gauge-empty`: no segments, "0%", settled.
- `ring-gauge-full`: two segments of 0.6, cut at the turn, settled.
- `ring-gauge-mid`: the value moves 0.2 to 0.8, captured in the middle of the tween (`Moving`).

### Painter primitives

- Existing: `arc` (the track and every segment), `text_centered`, `elide`. New: none.

### Acceptance

- Behaviour: `arcs` for `[]`, one value, several, values not usable and the cap at one turn; the first segment starts at the top and turns clockwise; segments end to end; the ring touches the square and has the default thickness; a thickness not usable and one past half the diameter; the ring in a wide rect.
- Text: centred in the inner square; none when empty; a long text elided.
- Conformance: the `Kind::Value` row on Full, Reduced and Off; a change animates once and ends exactly at the value, mid-tween on the curve; `live`, Off and Reduced snap.
- Node: a `progress` with a name, a covered-fraction value and a description; the fallback name through the table; a disabled scope marks it.
- Theme: `c4::m10` draws it under a theme of distinct colours.
- Tests, bullet by bullet. `crates/lune-ui-core/tests/widget_ring_gauge.rs`: `arcs_start_at_the_top_and_turn_end_to_end`, `a_value_that_is_not_finite_or_below_zero_sweeps_nothing`, `the_running_total_is_capped_at_one_turn`, `the_first_segment_starts_at_the_top_and_turns_clockwise`, `segments_lie_end_to_end_and_the_track_shows_past_the_last`, `butt_ends_do_not_pass_the_segments_angle`, `the_ring_touches_the_squares_sides_and_has_the_default_thickness`, `a_thickness_not_usable_falls_to_the_default_and_is_clamped_to_half_the_diameter`, `the_text_is_centred_in_the_inner_square_and_empty_draws_none`, `a_change_animates_once_and_ends_exactly_at_the_value`, `live_snaps_and_off_and_reduced_snap`, `the_value_row_conforms_under_every_tier`, `the_gauge_is_a_progress_named_by_the_app_whose_value_is_the_covered_fraction`, `a_disabled_scope_marks_the_node`; `crates/lune-ui-core/tests/c4.rs::no_fill_or_text_holds_a_preset_colour_under_a_theme_of_distinct_colours`.

### Build

- Milestone: sysmon-widgets step 1.
- Depends on: core.painter, core.theme, infra.accessibility.
- Done (D47, M11): tests `crates/lune-ui-core/tests/widget_ring_gauge.rs::arcs_start_at_the_top_and_turn_end_to_end, crates/lune-ui-core/tests/widget_ring_gauge.rs::the_running_total_is_capped_at_one_turn, crates/lune-ui-core/tests/widget_ring_gauge.rs::segments_lie_end_to_end_and_the_track_shows_past_the_last, crates/lune-ui-core/tests/widget_ring_gauge.rs::the_value_row_conforms_under_every_tier, crates/lune-ui-core/tests/widget_ring_gauge.rs::the_gauge_is_a_progress_named_by_the_app_whose_value_is_the_covered_fraction`; page `ring-gauge`; unverified here: none.

## Open questions

None.

## Notes for the planner

`Ui::ring_gauge(key, rect, &RingGaugeOptions::new(&segments, label).with_text("36%"))` returns nothing. `ring_gauge::arcs(&[f32])` gives each segment's `(start, sweep)`. The options and `RingSegment` are `#[non_exhaustive]`: build them with `new` and `with_*`. The library knows no unit and no domain: the app names what a ring measures and formats its text. Limits: the stroke has no round cap; a gauge in a window under a loaded theme takes only the track and text colours from it (the segments' are the app's).
