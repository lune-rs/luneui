---
id: core.linear-gradient
title: Linear gradient
area: core
status: done
consumers: any
source: crates/lune-ui-core/src/paint.rs, crates/lune-ui-wgpu/src/shader.wgsl
owner: —
summary: Painter::linear, a per-pixel two-colour ramp over a rounded rect at any angle, interpolated premultiplied and dithered to the target's format (D323)
---

## What it is

`Painter::linear(r, radius, from, to, angle)` fills the rounded rect `r` with a ramp from `from` to
`to`. The angle is in radians, clockwise on screen: 0 runs left to right, π/2 top to bottom. The ramp
spans the rect's extent along that direction, so the corners lie on the two ends (D323). It is recorded
as one shape of kind 2 and evaluated per pixel in the fragment shader; the colours interpolate
premultiplied (D323). The dither that hides banding on dark ramps is in `render.gradient-dither`.

## Needs

- A gradient fill with no visible bands on large dark washes (D326).
- A painter primitive recorded as plain data, drawn by the wgpu adapter (D47, D5).

## Existing code

None before 0.6.2. Built in 0.6.2: `Painter::linear` at `crates/lune-ui-core/src/paint.rs:778`, the
`Op::Linear` variant beside it, and kind 2 in `fs_shape` (`crates/lune-ui-wgpu/src/shader.wgsl`).

## Spec

### Evidence

Pinned: this repo 7f9640b (branch `orch/smooth-gradients`). Read: `crates/lune-ui-core/src/paint.rs:778-790`,
`crates/lune-ui-wgpu/tests/gradients.rs`. Conflicts: none (D323 is the source).

### Anatomy

- Input: the rect, one corner radius, two colours (straight alpha, premultiplied when recorded), an angle in radians. A non-finite angle counts as 0 [R crates/lune-ui-core/src/paint.rs:778].
- Shape: rect `r`, radii `[radius; 4]`, colour `from` premultiplied, `under` = `to` premultiplied and faded by the painter's alpha, `params = [2, angle, 0, 0]` [R crates/lune-ui-core/src/paint.rs:778].
- The fade of a transform applies to both colours; the clip, rounded clip and layer work as they do for `fill`.

### States

| state | look | cite |
|---|---|---|
| rest | the ramp over the rect | [R crates/lune-ui-core/src/paint.rs:778] |
| non-finite angle | drawn as angle 0 | [R crates/lune-ui-core/src/paint.rs:778] |
| zero-size rect | nothing drawn | [R crates/lune-ui-core/src/paint.rs:778] |

### Transitions

n/a — a paint primitive; an app animates by changing its colours or angle.

### Input and focus

n/a — a paint primitive: no pointer, keys, focus or tab stop. It draws on the layer it is recorded on.

### Retained state

- Transient: none. Persistent: none. App-owned: the rect, the colours and the angle each frame.
- Exit retention: no.

### Accessibility

n/a — no node of its own; the widget that draws it owns the semantics.

### Strings

n/a — no text.

### Gallery scenes

- `gradient-wash`: a full-area vertical ramp under two lights and a card, scale 2, settled.
- `gradient-wash-1x`: the same page at scale 1, settled.
- `gradient-linear-fade`: the opaque vertical ramp alone over the whole area, scale 2, settled (step 5 measures its pixels).

### Painter primitives

- Existing: fill.
- New: linear (built in 0.6.2, D323).

### Acceptance

- Recording: `crates/lune-ui-core/tests/gradient.rs::each_op_lands_on_under_with_its_kind`, `the_transform_scales_geometry_and_fades_colours`, `the_clip_and_rounded_clip_match_a_fill`, `recorded_and_replayed_equal_the_direct_scene`, `a_non_finite_angle_counts_as_zero`, `degenerate_inputs`.
- Pixels (`Rgba8Unorm`, scale 1, tile means against the exact ramp): `crates/lune-ui-wgpu/tests/gradients.rs::a_horizontal_ramp_has_exact_tile_means_and_the_rounded_control_fails`, `a_vertical_ramp_runs_top_to_bottom`, `a_diagonal_ramp_has_exact_tile_means`.
- Clip: `crates/lune-ui-wgpu/tests/gradients.rs::a_linear_inside_a_clip_leaves_the_outside_untouched`.

### Build

- Milestone: 0.6.2 (D323; steps 1 to 3 of the smooth-gradients task).
- Depends on: core.painter, render.gpu, render.gradient-dither.
- Done (D47, M11): tests `crates/lune-ui-core/tests/gradient.rs::each_op_lands_on_under_with_its_kind, crates/lune-ui-core/tests/gradient.rs::the_transform_scales_geometry_and_fades_colours, crates/lune-ui-core/tests/gradient.rs::the_clip_and_rounded_clip_match_a_fill, crates/lune-ui-core/tests/gradient.rs::recorded_and_replayed_equal_the_direct_scene, crates/lune-ui-core/tests/gradient.rs::a_non_finite_angle_counts_as_zero, crates/lune-ui-core/tests/gradient.rs::degenerate_inputs, crates/lune-ui-wgpu/tests/gradients.rs::a_horizontal_ramp_has_exact_tile_means_and_the_rounded_control_fails, crates/lune-ui-wgpu/tests/gradients.rs::a_vertical_ramp_runs_top_to_bottom, crates/lune-ui-wgpu/tests/gradients.rs::a_diagonal_ramp_has_exact_tile_means, crates/lune-ui-wgpu/tests/gradients.rs::a_linear_inside_a_clip_leaves_the_outside_untouched`; page `gradients`; unverified here: the Windows GPU run (the GPU tests skip without an adapter).

## Open questions

- None.

## Notes for the planner

The dither amplitude depends on the target format (D326); see `render.gradient-dither`, which states the limits.
