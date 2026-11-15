---
id: render.gradient-dither
title: Gradient dither
area: render
status: done
consumers: any
source: crates/lune-ui-wgpu/src/gpu.rs, crates/lune-ui-wgpu/src/shader.wgsl
owner: —
summary: Format-aware ordered dither for the per-pixel gradient and shadow kinds (8x8 Bayer, lifted base, ±0.5 LSB of an 8-bit or 10-bit target; off for sRGB and float targets) (D326)
---

## What it is

The gradient and shadow kinds (`linear`, `radial`, `shadow`) are dithered in the fragment shader with
an 8x8 ordered (Bayer) pattern, so a long ramp across a few levels of an 8-bit target shows no
bands. The amplitude is an `override dither_lsb` set per target format when the pipeline is built;
`dither_lsb(format)` in `crates/lune-ui-wgpu/src/gpu.rs` is the only table. Plain fills, rings,
acrylic, frost and strokes are never dithered and stay exact (D2, D4).

The shader cannot read the destination, so the dither lifts the base instead of adding noise to the
alpha (D12): `h = 0.5 · lsb · clamp(a / lsb, 0, 1)` and `rgb_out = clamp(c, h, 1 − h) + (bayer8 − 0.5) · 2h`,
with the same value on r, g and b and alpha untouched (D326).

## Needs

- Gradients over dark surfaces with no visible bands, on the formats the window and the dump use
  (D3, D13).

## Existing code

None before 0.6.2. Built in 0.6.2: `dither_lsb` at `crates/lune-ui-wgpu/src/gpu.rs:835` (with its unit
test), the `override dither_lsb` and the Bayer step in `fs_shape` (`crates/lune-ui-wgpu/src/shader.wgsl`).

## Spec

### Evidence

Pinned: this repo 7f9640b (branch `orch/smooth-gradients`). Read: `crates/lune-ui-wgpu/src/gpu.rs:835-843`,
`crates/lune-ui-wgpu/tests/gradients.rs`. Conflicts: none. The format table is not final (D13): it
is the table the 10-bit follow-up will change.

### Anatomy

- Input: the target's texture format, read when the per-format pipeline is built; the fragment's integer pixel position [R crates/lune-ui-wgpu/src/gpu.rs:835].
- Amplitude (D13): `Rgba8Unorm` and `Bgra8Unorm` 1/255; `Rgb10a2Unorm` 1/1023; `Rgba16Unorm` 1/65535; every other format (the `*Srgb` ones, `Rgba16Float`, float and 8-bit single-channel) 0, no dither.
- Applied last, to kinds 2 to 4 only, after coverage, clips and fade (D12).
- Limits (stated, not hidden): where a channel is darker than `h` it is lifted on average by up to `h` (at most 0.5 LSB); where it is within `h` of 1 it is lowered by up to `h`. The expected mean per pixel is `clamp(c, h, 1 − h) + dst·(1 − a)`. Where `a` is below 1 LSB the amplitude fades with `a`, so the faintest tail may keep its last step partly undithered.
- Not final (D13): the follow-up `docs/tasks/ten-bit-surface.md` needs the display's depth as a second input (`min(format, display)`); `dither_lsb` stays `pub(crate)` until then.

### States

| state | look | cite |
|---|---|---|
| 8-bit or 10-bit or 16-bit unorm target | dithered at ±0.5 LSB | [R crates/lune-ui-wgpu/src/gpu.rs:835] |
| sRGB, float or other target | no dither (amplitude 0) | [R crates/lune-ui-wgpu/src/gpu.rs:835-843] |
| plain fill, ring, acrylic, frost, stroke | exact, never dithered | [R crates/lune-ui-wgpu/tests/gradients.rs::a_plain_fill_stays_exact] |

### Transitions

n/a — a render rule, not a visual state.

### Input and focus

n/a — no pointer, keys, focus or tab stop.

### Retained state

- Transient: none. Persistent: none. App-owned: none (the target's format is the adapter's).
- Exit retention: no.

### Accessibility

n/a — no node.

### Strings

n/a — no text.

### Gallery scenes

n/a — the dither shows in every gradient scene; the scenes are claimed by `core.linear-gradient`, `core.radial-gradient` and `core.soft-shadow`.

### Painter primitives

- Existing: none of its own.
- New: none (the dither is a property of the three gradient and shadow kinds).

### Acceptance

- Table: `crates/lune-ui-wgpu/src/gpu.rs::dither_lsb_table` (every row, including `Rgba16Unorm`, `Rgba16Float`, `Rgba8UnormSrgb`, `Bgra8UnormSrgb`).
- Bands: `crates/lune-ui-wgpu/tests/gradients.rs::a_horizontal_ramp_has_exact_tile_means_and_the_rounded_control_fails` (the CPU-rounded control must fail the same tile check), `a_black_shadow_over_grey_has_exact_tile_means` (the base lift reaches the blended result over a grey background).
- Exactness of plain fills: `crates/lune-ui-wgpu/tests/gradients.rs::a_plain_fill_stays_exact`.

### Build

- Milestone: 0.6.2 (D326; steps 2 and 3 of the smooth-gradients task).
- Depends on: render.gpu, render.surface.
- Done (D47, M11): tests `crates/lune-ui-wgpu/src/gpu.rs::dither_lsb_table, crates/lune-ui-wgpu/tests/gradients.rs::a_horizontal_ramp_has_exact_tile_means_and_the_rounded_control_fails, crates/lune-ui-wgpu/tests/gradients.rs::a_black_shadow_over_grey_has_exact_tile_means, crates/lune-ui-wgpu/tests/gradients.rs::a_plain_fill_stays_exact`; page `gradients`; unverified here: the Windows GPU run, and the 10-bit and 16-bit targets on a real display (no such surface is exercised; D13).

## Open questions

- The format table: the 10-bit surface follow-up (`docs/tasks/ten-bit-surface.md`) decides the display-depth input (D13).

## Notes for the planner

Dither is the only thing that makes a dark wash look smooth on 8-bit; a new target format needs a row in `dither_lsb` and a test in `dither_lsb_table`, not a new branch in the shader.
