---
id: core.soft-shadow
title: Soft shadow
area: core
status: done
consumers: any
source: crates/lune-ui-core/src/paint.rs, crates/lune-ui-wgpu/src/shader.wgsl
owner: —
summary: Painter::shadow, an analytic Gaussian blur of a rounded rect (erf in x, 16 slices in y), coloured freely, dithered to the target's format; its cost is stated in its doc (D325)
---

## What it is

`Painter::shadow(r, radius, sigma, color)` draws the rounded rect `r` blurred by a Gaussian of `sigma`
logical px (a CSS blur radius is about 2σ). It is the whole blurred shape, not cut out under `r`; an
offset or a spread is the app's, made by moving or growing `r` (D325). It is evaluated per pixel: x by
the error function, y by 16 slices, each with its exact Gaussian mass (D325). It is recorded as one
shape of kind 4 with no second colour. A `sigma` below half a physical px draws the plain
antialiased fill.

## Needs

- A soft shadow and glow without a blur pass or a texture (D325).
- A painter primitive recorded as plain data, drawn by the wgpu adapter (D47, D325).

## Existing code

None before 0.6.2. Built in 0.6.2: `Painter::shadow` at `crates/lune-ui-core/src/paint.rs:824`, the
`Op::Shadow` variant, and kind 4 in `fs_shape` (`crates/lune-ui-wgpu/src/shader.wgsl`), with the quad
padded by 4σ + 1 physical px.

## Spec

### Evidence

Pinned: this repo 7f9640b (branch `orch/smooth-gradients`). Read: `crates/lune-ui-core/src/paint.rs:824-840`,
`crates/lune-ui-wgpu/tests/gradients.rs`. Conflicts: none (D325 is the source).

### Anatomy

- Input: rect `r`, one corner radius, `sigma` in logical px, colour (straight alpha) [R crates/lune-ui-core/src/paint.rs:824].
- Shape: rect `r`, radii `[radius; 4]`, colour premultiplied, `under` zero, `params = [4, sigma · scale · s, 0, 0]`; a negative or non-finite sigma becomes 0 [R crates/lune-ui-core/src/paint.rs:824].
- Cost (stated in the doc): the quad pads 4σ + 1 physical px on each side, so σ = 40 logical px at scale 2 pads 321 px; every covered pixel evaluates 16 slices of the error function. There is no cap, because a cap would clip the shadow.

### States

| state | look | cite |
|---|---|---|
| rest | the blurred rounded rect in the colour | [R crates/lune-ui-core/src/paint.rs:824] |
| sigma below half a physical px | the plain antialiased fill | [R crates/lune-ui-core/src/paint.rs:824] |
| radius 0 | the edge midpoint at half the colour (test `crates/lune-ui-wgpu/tests/gradients.rs::a_shadow_of_a_square_rect_is_half_the_colour_at_its_edge`) | [R crates/lune-ui-core/src/paint.rs:824] |

### Transitions

n/a — a paint primitive; an app animates by moving, growing or recolouring `r`.

### Input and focus

n/a — a paint primitive: no pointer, keys, focus or tab stop. It draws on the layer it is recorded on, below that layer's text.

### Retained state

- Transient: none. Persistent: none. App-owned: the rect, radius, sigma and colour each frame.
- Exit retention: no.

### Accessibility

n/a — no node of its own.

### Strings

n/a — no text.

### Gallery scenes

- `gradient-shadow`: a card with a blue shadow offset down and a light glow, over a dark fill, scale 2, settled.
- `gradient-wash`: the same card over the wash, scale 2, settled.

### Painter primitives

- Existing: fill.
- New: shadow (built in 0.6.2, D325).

### Acceptance

- Recording: `crates/lune-ui-core/tests/gradient.rs::each_op_lands_on_under_with_its_kind`, `the_transform_scales_geometry_and_fades_colours`, `degenerate_inputs` (a NaN sigma gives `params[1] == 0`).
- Pixels (`Rgba8Unorm`): `crates/lune-ui-wgpu/tests/gradients.rs::a_shadow_matches_a_brute_force_blur` (against a supersampled reference, never the shader's erf), `a_shadow_of_a_square_rect_is_half_the_colour_at_its_edge`, and `a_black_shadow_over_grey_has_exact_tile_means` (the coloured-over-grey case in `render.gradient-dither`).

### Build

- Milestone: 0.6.2 (D325; steps 1 to 3 of the smooth-gradients task).
- Depends on: core.painter, render.gpu, render.gradient-dither.
- Done (D47, M11): tests `crates/lune-ui-core/tests/gradient.rs::each_op_lands_on_under_with_its_kind, crates/lune-ui-core/tests/gradient.rs::the_transform_scales_geometry_and_fades_colours, crates/lune-ui-core/tests/gradient.rs::degenerate_inputs, crates/lune-ui-wgpu/tests/gradients.rs::a_shadow_matches_a_brute_force_blur, crates/lune-ui-wgpu/tests/gradients.rs::a_shadow_of_a_square_rect_is_half_the_colour_at_its_edge, crates/lune-ui-wgpu/tests/gradients.rs::a_black_shadow_over_grey_has_exact_tile_means`; page `gradients`; unverified here: the Windows GPU run (the GPU tests skip without an adapter).

## Open questions

- None.

## Notes for the planner

The shadow has no second colour and no cut-out under `r`: a box-shadow look needs the app to move or grow `r`.
