---
id: render.surface
title: Window surface
area: render
status: in-progress
consumers: any
source: crates/lune-ui-wgpu/src/surface.rs
owner: consumer's window run
summary: One window's swapchain on the shared Gpu: acquire, present, resize, and a depth of 8, 10 or 16 bits chosen live; no winit dependency
---

## What it is

`WindowSurface` wraps a `wgpu::Surface` configured for a `Gpu` (`new`, `resize`, `acquire`, `present`, `format`, `depth`, `size`, `alpha_mode`). `acquire` returns `Acquired::Frame`, `Skip` or `Redraw` instead of calling a windowing function; the app asks its window for a redraw and calls `pre_present_notify` before `present`. The surface comes from any raw-window-handle window (`Instance::create_surface`), so `lune-ui-wgpu` does not depend on winit. Per window: one `WindowSurface` and one `FrameRenderer` (`render.frame`) on the one `Gpu`.

The present mode and the frame latency are `SurfaceOptions` (`with_options`, `options`, `set_options`; `new` keeps the default, `Fifo` with a latency of 2, 0.4.0's). `Fifo` makes `acquire` wait for the display when every drawable is queued (D299). `set_options` and `resize` run between frames, never while a texture from `acquire` is alive.

The depth is an option too (D328): `SurfaceDepth { Eight, Ten, Float16, Unorm16, Auto }` and `SurfaceOptions { depth, display_bits, opaque }`. The surface resolves it to a format from what it lists (with `SRGB` among the colour spaces), the alpha mode and the display's depth, falls back down `Unorm16`, `Float16`, `Ten`, `Eight`, and always configures the sRGB colour space. `Auto` is 8-bit on a glass window on every OS (D330); `Ten` has a 2-bit alpha, so a fallback and `Auto` never choose it while the window is translucent (D329). `set_options` with another depth, display depth or `opaque` re-resolves the format, and the app then calls `FrameRenderer::follow` and rebuilds what it built for the old format (`docs/consuming.md`, "Choosing the depth at run time").

## Needs

- Several windows on one GPU, and headless targets (C2; R-nfr-8).
- A window surface set so the main thread does not wait in acquire when no frame is due (R-nfr-9).

## Existing code

`crates/lune-ui-wgpu/src/surface.rs:29` (`Acquired`), `:59` (`SurfaceDepth`), `:87` (`SurfaceOptions`), `:185` (`resolve_format`, the pure resolver), `:263` (`WindowSurface`), `:313` (`new`), `:325` (`with_options`), `:384` (`options`), `:402` (`set_options`), `:427` (`depth`), `:452` (`resize`), `:464` (`acquire`), `:488` (`present`). The resolver's tables (Metal, DX12 through a Visual, Vulkan, no sRGB, `Rgba8Unorm` only, the alpha mode, `set_options`) are unit tests in `surface.rs`; every format is rendered offscreen and measured in `crates/lune-ui-wgpu/tests/surface_depth.rs`. The headless path is tested in `crates/lune-ui-wgpu/tests/targets.rs`; the window path is run by the platform smoke (`gallery --smoke-platform`, a stage that resizes the hidden window's surface and acquires from it, M11).

## Spec

### Evidence

Pinned: this repo fa2958b. Read: `crates/lune-ui-wgpu/src/surface.rs:1-128` (module doc 1-4; `Acquired` 10, `WindowSurface` 21, `new` 37, `format` 83, `size` 88, `alpha_mode` 93, `resize` 97, `acquire` 106, `present` 125).
Conflicts: none.

### Anatomy

- A window's swapchain: the surface, its configuration, acquire and present [R crates/lune-ui-wgpu/src/surface.rs:1-4].
- Windowing-free: the app creates the `wgpu::Surface` from any window handle and asks for a redraw when `acquire` says so [R crates/lune-ui-wgpu/src/surface.rs:2-4].
- Resize takes a width and height [R crates/lune-ui-wgpu/src/surface.rs:97].
- Present mode and frame latency: `SurfaceOptions { present_mode, frame_latency }`, default `Fifo`, 2; a mode the surface does not list falls back to `Fifo`; `AutoVsync` and `AutoNoVsync` go to wgpu, and `AutoNoVsync` is the first of `Immediate`, `Mailbox`, `Fifo` the surface lists (D299) [R crates/lune-ui-wgpu/src/surface.rs:30-40, 50-60].
- Blocking: under `Fifo`, `acquire` waits for the display when every drawable is queued. Measured on macOS/Metal only (Apple M1 Pro, one 60 Hz display): with `Repaint::paced` and `AutoNoVsync` (`Immediate` there) a run of frames did not wait in `acquire`. DX12, Vulkan and GL are unmeasured; tearing is unchecked on every platform (D299).
- Depth (D328 to D330): `SurfaceOptions::default()` is `{ Fifo, 2, Auto, None, false }`; `Auto` with an unknown display or on a glass window is `Bgra8Unorm` (else `Rgba8Unorm`), configured as in 0.6.2; on an opaque window over a display of 10 bits or more it is `Rgb10a2Unorm`; `Float16` is `Rgba16Float`, `Unorm16` is `Rgba16Unorm` where the device has `TEXTURE_FORMAT_16BIT_NORM` and the adapter's format features. `options()` reports what was asked, `depth()` and `format()` what was configured. `set_options` re-resolves only when `depth`, `display_bits` or `opaque` change.
- No configure while an acquired texture is alive: `resize` and `set_options` say so, and `acquire` drops a suboptimal texture before it reconfigures (D11). At 0.4.0 `acquire` reconfigured with the texture alive, which wgpu rejects with a panic; only Vulkan reports suboptimal, so the fix is checked by reading only.

### States

| state | look | cite |
|---|---|---|
| acquired | a frame to draw | [R crates/lune-ui-wgpu/src/surface.rs:10,106] |
| needs redraw | `acquire` says so | [R crates/lune-ui-wgpu/src/surface.rs:2-4] |

### Transitions

n/a — no motion.

### Input and focus

n/a — no input.

### Retained state

- Persistent: the surface and its configuration [R crates/lune-ui-wgpu/src/surface.rs:21].
- App-owned: the window handle.

### Accessibility

n/a — no UI node.

### Strings

n/a — no text.

### Gallery scenes

n/a — the gallery's scenes render offscreen through dumps; the window swapchain has no scene of its own.

### Painter primitives

n/a — no drawing.

### Acceptance

- Done standard still needs: a resize-and-acquire test on a hidden window, and the pixel gate on a presented frame (open: the platform run is not in this milestone).
- Resize and acquire on a hidden window: `apps/gallery/src/smoke.rs` `resize_stage` (`gallery --smoke-platform`, needs a logged-in GUI session): resizes the surface to a size that is not its own, checks `size()`, acquires (a frame, a skip or a redraw are answers; an error or a panic fails the smoke), resizes back and checks. Its last run in this environment (debug, logged-in macOS session): exit 0, surface resized (2080, 1320) -> (2144, 1352) and back, acquire answered skip (the window is hidden). It is not a `cargo test`, so no test cites it.
- Frame pacing on the real renderer (D299; macOS/Metal only; Apple M1 Pro, one 60 Hz display; `cargo run --release -p gallery -- --pace-probe`, a logged-in session, the window left visible and untouched). Reading at 9250138: exit 0, first run, `load1=5.20`. Phase 1 (0.4.0, `Now` every frame, `Fifo`): scheduled 60.0/s (180 frames), acquire p50 16.21 p95 16.38 p99 16.87 max 16.88 ms, busy 1000 ms/s. Phase 2 (busy cadence, `Fifo`): scheduled 30.0/s (90 frames), acquire p50 0.04 p95 0.06 p99 0.07 max 0.07 ms, busy 14 ms/s. Phase 3 (paced, `Fifo`): scheduled 60.0/s (180 frames), acquire p50 0.03 p95 0.05 p99 0.07 max 0.08 ms, busy 24 ms/s. Phase 4 (paced, `AutoNoVsync`): scheduled 60.0/s (180 frames), acquire p50 0.03 p95 0.04 p99 0.05 max 0.06 ms, busy 28 ms/s. Verdict: held. Phase 4's `AutoNoVsync` resolved to `Immediate` on Metal (the planner's listing; the probe does not print it, as `options()` was not read back). Frames were all scheduled (no `os` frames, no `Skip` or `Redraw`).
- Window depth (D328 to D335), all measured on this Mac (Apple GPU, Metal): the resolver tables; every format offscreen (`tests/surface_depth.rs`: a dithered ramp exact to the step of each format, the 8-bit match within 1 outside the glass and 2 inside, the alpha of each format, no validation error); and the live switch on the gallery's real window, which cycled `Eight`, `Ten`, `Float16`, `Unorm16`, `Auto` for 14 s (1240 frames, 31 switches) with no wgpu error (`Unorm16` falls back, the Metal surface does not list it). The live line on the glass main window of this XDR Mac reads `Bgra8Unorm · 8-bit · alpha PostMultiplied · display 10-bit`.
- The headless target path, which shares the format and size handling: `crates/lune-ui-wgpu/tests/targets.rs::one_renderer_across_two_sizes_matches_a_fresh_renderer_at_each`.
- Not done here: the pixel gate on a presented frame; the presented colour of a 10-bit or float surface (below).

### Build

- Milestone: M11 (done standard; code exists).
- Depends on: render.gpu, platform.window.

## Open questions

- Window-mode presentation is not exercised by an automated test (no visible windows in the gates).
- Frame pacing (D298, D299): `paced` with `AutoNoVsync` is measured on macOS/Metal only; DX12, Vulkan and GL are unmeasured; tearing under `AutoNoVsync` is unchecked on every platform; `acquire`'s suboptimal path (D11) is fixed by reading and never run (Metal never reports suboptimal).
- Class B (D234): this entry's own Acceptance says "the pixel gate on a presented frame (open: the platform run is not in this milestone)". A presented frame is what the OS composites to the screen; nothing here can read it back (reading it would need a `COPY_SRC` surface configuration for every app, D234). So the entry stays `in-progress` and `owner:` names the run that closes it: a consumer's window run that presents a frame and compares it.

- Window depth (D328 to D335), unverified: the presented colour and alpha of `Rgb10a2Unorm` and `Rgba16Float` windows over the desktop on any OS (nothing here reads a presented frame; the float surface is configured in the sRGB colour space, which Metal maps to the layer's default and sets no EDR, so its colour is assumed); `Rgb10a2Unorm`, `Rgba16Float` and `Rgba16Unorm` windows on DX12 and Vulkan (nothing ran on Windows or Linux: the DX12 listing, a glass window `PreMultiplied` and `Rgba16Float` without sRGB, is read from wgpu-hal 30.0.1); `Rgba16Unorm` as a window at all (this Mac's surface does not list it); on Windows `Auto` stays 8-bit on a glass window and `display_depth` has no read there (D330, D331: a follow-up for an app that passes `opaque: true`).

## Notes for the planner

Added in M2 step 3.
