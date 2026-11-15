---
id: core.radial-gradient
title: Radial gradient
area: core
status: done
consumers: any
source: crates/lune-ui-core/src/paint.rs, crates/lune-ui-wgpu/src/shader.wgsl
owner: —
summary: Painter::radial, a per-pixel ellipse gradient from an inner to an outer colour with linear, smooth or Gaussian falloff, dithered to the target's format (D324)
---

## What it is

`Painter::radial(center, radius, inner, outer, falloff)` fills the ellipse inscribed in
`center ± radius` with a ramp from `inner` at the centre to `outer` at the rim, nothing drawn outside
the ellipse (the rim is antialiased). The falloff curves are on `u = min(distance / radius, 1)`: Linear
`1 − u`, Smooth `1 − u²(3 − 2u)`, Gaussian `(e^(−4.5u²) − e^(−4.5)) / (1 − e^(−4.5))` (D324). It is
recorded as one shape of kind 3 and evaluated per pixel; the colours interpolate premultiplied (D323).
An outer of transparent colour makes a light that fades out.

## Needs

- A soft light on a surface with no bands (D326).
- A painter primitive recorded as plain data, drawn by the wgpu adapter (D47, D5).

## Existing code

None before 0.6.2. Built in 0.6.2: `Painter::radial` at `crates/lune-ui-core/src/paint.rs:793`, the
`Op::Radial` variant, and kind 3 in `fs_shape` (`crates/lune-ui-wgpu/src/shader.wgsl`).

## Spec

### Evidence

Pinned: this repo 7f9640b (branch `orch/smooth-gradients`). Read: `crates/lune-ui-core/src/paint.rs:793-805`,
`crates/lune-ui-wgpu/tests/gradients.rs`. Conflicts: none (D324 is the source of the curves).

### Anatomy

- Input: centre `[x, y]`, radius `[rx, ry]`, inner and outer colours (straight alpha), a `Falloff` (`Linear`, `Smooth`, `Gaussian`; `#[non_exhaustive]`) [R crates/lune-ui-core/src/paint.rs:153, 793].
- Shape: rect `Rect::new(cx − rx, cy − ry, 2rx, 2ry)`, radii `[0; 4]`, colour `inner` premultiplied, `under` = `outer` premultiplied and faded, `params = [3, code, 0, 0]` with the code 0, 1 or 2 for the three falloffs [R crates/lune-ui-core/src/paint.rs:793].
- A radius that is not finite, or not above zero on both axes, records the op and draws nothing [R crates/lune-ui-core/src/paint.rs:793].

### States

| state | look | cite |
|---|---|---|
| rest | the falloff from inner at the centre to outer at the rim | [R crates/lune-ui-core/src/paint.rs:793] |
| outer transparent | a light that fades to nothing at the rim | [R crates/lune-ui-core/src/paint.rs:793] |
| radius not finite or ≤ 0 | nothing drawn | [R crates/lune-ui-core/src/paint.rs:793] |

### Transitions

n/a — a paint primitive; an app animates by changing its centre, radius or colours.

### Input and focus

n/a — a paint primitive: no pointer, keys, focus or tab stop. It draws on the layer it is recorded on.

### Retained state

- Transient: none. Persistent: none. App-owned: the centre, radius, colours and falloff each frame.
- Exit retention: no.

### Accessibility

n/a — no node of its own.

### Strings

n/a — no text.

### Gallery scenes

- `gradient-wash`: two large lights (Gaussian and Smooth falloff, transparent outers) over the wash, scale 2, settled.
- `gradient-radial-light`: the two lights over a plain dark fill, scale 2, settled.

### Painter primitives

- Existing: fill.
- New: radial (built in 0.6.2, D324).

### Acceptance

- Recording: `crates/lune-ui-core/tests/gradient.rs::each_op_lands_on_under_with_its_kind`, `the_transform_scales_geometry_and_fades_colours`, `recorded_and_replayed_equal_the_direct_scene`, `the_falloff_codes`, `degenerate_inputs`.
- Pixels (`Rgba8Unorm`, a 512 by 512 target, radius 200, Smooth): `crates/lune-ui-wgpu/tests/gradients.rs::a_radial_light_has_exact_tile_means_and_stays_in_its_box` (tile means against the expected mean; pixels outside the ellipse's box equal the background).

### Build

- Milestone: 0.6.2 (D324; steps 1 to 3 of the smooth-gradients task).
- Depends on: core.painter, render.gpu, render.gradient-dither.
- Done (D47, M11): tests `crates/lune-ui-core/tests/gradient.rs::each_op_lands_on_under_with_its_kind, crates/lune-ui-core/tests/gradient.rs::the_transform_scales_geometry_and_fades_colours, crates/lune-ui-core/tests/gradient.rs::recorded_and_replayed_equal_the_direct_scene, crates/lune-ui-core/tests/gradient.rs::the_falloff_codes, crates/lune-ui-core/tests/gradient.rs::degenerate_inputs, crates/lune-ui-wgpu/tests/gradients.rs::a_radial_light_has_exact_tile_means_and_stays_in_its_box`; page `gradients`; unverified here: the Windows GPU run (the GPU tests skip without an adapter).

## Open questions

- None.

## Notes for the planner

The falloff curves are D324 and the shape of the light (inscribed ellipse, nothing outside it) is part of the contract, not a choice.
