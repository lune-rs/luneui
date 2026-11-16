---
id: render.gradient-dither
title: Gradient dither
area: render
status: done
consumers: any
source: crates/lune-ui-wgpu/src/gpu.rs, crates/lune-ui-wgpu/src/shader.wgsl
owner: —
summary: Format- and display-aware ordered dither for the per-pixel gradient and shadow kinds (8x8 Bayer, lifted base, ±0.5 LSB of the coarser of the target's format and the display; off for sRGB targets and for float targets on a display of unknown depth) (D326, D333)
---

## What it is

The gradient and shadow kinds (`linear`, `radial`, `shadow`) are dithered in the fragment shader with
an 8x8 ordered (Bayer) pattern, so a long ramp across a few levels of an 8-bit target shows no
bands. The amplitude is an `override dither_lsb` set per target format and display when the pipeline is built;
`dither_lsb(format, display_bits)` in `crates/lune-ui-wgpu/src/gpu.rs` is the only table. Plain fills, rings,
acrylic, frost and strokes are never dithered and stay exact (D326).

The shader cannot read the destination, so the dither lifts the base instead of adding noise to the
alpha (D326): `h = 0.5 · lsb · clamp(a / lsb, 0, 1)` and `rgb_out = clamp(c, h, 1 − h) + (bayer8 − 0.5) · 2h`,
with the same value on r, g and b and alpha untouched (D326).

## Needs

- Gradients over dark surfaces with no visible bands, on the formats the window and the dump use
  (D326).

## Existing code

None before 0.6.2. Built in 0.6.2, extended in 0.7.0 (D333): `dither_lsb(format, display_bits)` at `crates/lune-ui-wgpu/src/gpu.rs:866` (with its unit
test `dither_lsb_table`), the `override dither_lsb` and the Bayer step in `fs_shape` (`crates/lune-ui-wgpu/src/shader.wgsl`).

## Spec

### Evidence

Pinned: this repo 7f9640b (branch `orch/smooth-gradients`), extended on `orch/ten-bit-surface`. Read: `crates/lune-ui-wgpu/src/gpu.rs:855-880`,
`crates/lune-ui-wgpu/tests/gradients.rs`, `crates/lune-ui-wgpu/tests/surface_depth.rs`. Conflicts: none. The display input D326 left open is closed (D333); the `*Srgb` note stays.

### Anatomy

- Input: the target's texture format and the display's bits per channel (`display_bits`, `None` when unknown), read when the per-format pipeline is built; the fragment's integer pixel position [R crates/lune-ui-wgpu/src/gpu.rs:866]. The renderers are keyed by (format, amplitude), so two displays that need the same amplitude share one set of pipelines (D333).
- Amplitude (D326, D333): the larger of the format's step and the display's. The format's step: `Rgba8Unorm` and `Bgra8Unorm` 1/255; `Rgb10a2Unorm` 1/1023; `Rgba16Unorm` 1/65535; the float formats 0 (no fixed step). The display's step: `1 / (2^bits - 1)` for 1 to 16 bits, 0 for `None`, 0 or above 16. So `Rgba16Float` on a 10-bit display is 1/1023, `Rgb10a2Unorm` on an 8-bit display 1/255, and `display_bits: None` is exactly D326's table. Every other format (the `*Srgb` ones, 8-bit single-channel) is 0 whatever the display, no dither.
- Applied last, to kinds 2 to 4 only, after coverage, clips and fade (D326).
- Limits (stated, not hidden): where a channel is darker than `h` it is lifted on average by up to `h` (at most 0.5 LSB); where it is within `h` of 1 it is lowered by up to `h`. The expected mean per pixel is `clamp(c, h, 1 − h) + dst·(1 − a)`. Where `a` is below 1 LSB the amplitude fades with `a`, so the faintest tail may keep its last step partly undithered.
- Honest gap that stays (D326): the `*Srgb` formats are undithered, because the shader writes linear values there and a step is not a fixed size; a window picks one only when the surface lists no other (D328). `dither_lsb` stays `pub(crate)`.

### States

| state | look | cite |
|---|---|---|
| 8-bit or 10-bit or 16-bit unorm target | dithered at ±0.5 LSB of the coarser of format and display | [R crates/lune-ui-wgpu/src/gpu.rs:866] |
| float target on a display of 1 to 16 bits | dithered at ±0.5 LSB of the display | [R crates/lune-ui-wgpu/src/gpu.rs:866] |
| float target, unknown display; sRGB or other target | no dither (amplitude 0) | [R crates/lune-ui-wgpu/src/gpu.rs:866-880] |
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
- Per format and display, measured offscreen on this Mac (D333; `crates/lune-ui-wgpu/tests/surface_depth.rs`): `a_dithered_ramp_is_exact_to_the_step_of_every_format` (`Rgba8Unorm`, `Bgra8Unorm`, `Rgb10a2Unorm`, `Rgba16Unorm`), `a_float_ramp_with_no_display_is_within_an_f16_ulp`, `a_float_ramp_on_a_10_bit_display_is_judged_at_10_bits`, and the control `the_8_bit_render_judged_at_10_bit_tolerance_fails` (an 8-bit render must fail the 10-bit check).
- Exactness of plain fills: `crates/lune-ui-wgpu/tests/gradients.rs::a_plain_fill_stays_exact`.

### Build

- Milestone: 0.6.2 (D326; steps 2 and 3 of the smooth-gradients task), 0.7.0 (D333: the display input).
- Depends on: render.gpu, render.surface.
- Done (D47, M11): tests `crates/lune-ui-wgpu/src/gpu.rs::dither_lsb_table, crates/lune-ui-wgpu/tests/gradients.rs::a_horizontal_ramp_has_exact_tile_means_and_the_rounded_control_fails, crates/lune-ui-wgpu/tests/gradients.rs::a_black_shadow_over_grey_has_exact_tile_means, crates/lune-ui-wgpu/tests/gradients.rs::a_plain_fill_stays_exact`; page `gradients`; unverified here: the Windows GPU run, and the dither as presented on a real 10-bit or 16-bit display (the targets are measured offscreen; no presented frame is read, D333).

## Open questions

- The `*Srgb` formats stay undithered (D326, D333); nothing here picks one unless the surface offers no other format.

## Notes for the planner

Dither is the only thing that makes a dark wash look smooth on 8-bit; a new target format needs a row in `dither_lsb` and a test in `dither_lsb_table`, not a new branch in the shader.
