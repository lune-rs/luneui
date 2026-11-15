//! A window's depth, measured offscreen: the same renderer drawing into targets of every surface
//! format (`Rgba8Unorm`, `Bgra8Unorm`, `Rgb10a2Unorm`, `Rgba16Float`, `Rgba16Unorm`), read back at
//! full precision (`dump::read_rgba_f32`).
//!
//! - The dithered gradient is exact to the format's step: aligned 8x8 tile means within a quarter
//!   step of the exact ramp, the step between adjacent tile means within a step of the exact
//!   ramp's own step, no pixel more than a step off. A
//!   float target with no display is not dithered and is within one f16 ulp; on a 10-bit display
//!   it is judged at 10 bits. The 8-bit render judged at 10-bit tolerance must fail (the control).
//! - A scene of fills, rounded rects, a stroke, text, a picture and glass reads within 1 per
//!   channel of the 8-bit render outside the blurred regions and within 3 inside them.
//! - `Rgb10a2Unorm` keeps 4 alpha levels, `Rgba16Float` keeps the alpha it is given.
//!
//! Measured on this Mac (Apple GPU, Metal), in steps of the judging format; `tile`, `step`,
//! `pixel` as `RampStats`:
//!
//! | ramp | tile | step | pixel |
//! | --- | --- | --- | --- |
//! | `Rgba8Unorm`, `Bgra8Unorm` | 0.016 | 0.031 | 0.992 |
//! | `Rgb10a2Unorm` | 0.100 | 0.139 | 0.975 |
//! | `Rgba16Unorm` | 0.016 | 0.032 | 0.993 |
//! | `Rgba16Float`, no display (ulps) | | | 0.996 ulp |
//! | `Rgba16Float`, 10-bit display | 0.069 | 0.018 | 0.563 |
//! | control: 8-bit render judged at 10 bits | 0.063 | 0.125 | 3.98 |
//!
//! The control fails on the pixel clause only: an ordered dither makes a tile mean exact far
//! beyond the format's step, so tile means and steps cannot tell 8 bits from 10; the pixels can.
//! The 8-bit match reads at most 1 outside the glass and 2 inside it, for `Rgb10a2Unorm`,
//! `Rgba16Float` and `Rgba16Unorm` alike. A 50 % fill reads alpha 0.6667 on `Rgb10a2Unorm`
//! (2/3 of 3 levels), 0.5020 on `Rgba16Float` and `Rgba8Unorm`.
//!
//! Every pass runs with wgpu's default error handler, which panics on a validation error, so a
//! pass that completes made and sampled every target (the blur's included) in that format.
//!
//! GPU test: with no adapter it prints `skipped: no GPU adapter` and passes, unless
//! `LUNEUI_REQUIRE_GPU=1`, which makes a missing adapter a failure. `Rgba16Unorm` needs a device
//! that offers it (printed `skipped: ...` where it does not).

use lune_ui_core::image::{Fit, ImageAlign, ImageData, ImageId, ImageStore};
use lune_ui_core::paint::{LayerId, Painter, Rect, StrokeStyle, Xform};
use lune_ui_core::scene::Scene;
use lune_ui_core::text::{Spec, Style};
use lune_ui_core::theme::DARK;
use lune_ui_wgpu::dump;
use lune_ui_wgpu::frame::{FrameParams, FrameRenderer};
use lune_ui_wgpu::gpu::{self, Gpu};
use lune_ui_wgpu::text_cache::TextCache;
use wgpu::TextureFormat as F;

type Size = (usize, usize);

fn fonts() -> [&'static [u8]; 3] {
    [
        include_bytes!("../../../apps/gallery/assets/RobotoMonoNerdFontMono-Regular.ttf"),
        include_bytes!("../../../apps/gallery/assets/RznSans-Regular.ttf"),
        include_bytes!("../../../apps/gallery/assets/RznSans-Medium.ttf"),
    ]
}

fn text_cache() -> TextCache {
    TextCache::new(&fonts(), "RznSans", "RobotoMono Nerd Font Mono")
}

fn headless_gpu() -> Option<Gpu> {
    match pollster::block_on(Gpu::new(gpu::instance(None), None)) {
        Ok(g) => Some(g),
        Err(e) => {
            let required = std::env::var("LUNEUI_REQUIRE_GPU").is_ok_and(|v| v == "1");
            assert!(!required, "no GPU adapter: {e}");
            println!("skipped: no GPU adapter");
            None
        }
    }
}

/// Whether the device can render to `Rgba16Unorm` the way a window surface needs it: the 16-bit
/// norm and adapter-specific format features, and the format a blendable, filterable render
/// attachment.
fn unorm16_usable(gpu: &Gpu) -> bool {
    let need = wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
        | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    if !gpu.device.features().contains(need) {
        return false;
    }
    let f = gpu.adapter.get_texture_format_features(F::Rgba16Unorm);
    f.allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        && f.flags.contains(
            wgpu::TextureFormatFeatureFlags::BLENDABLE
                | wgpu::TextureFormatFeatureFlags::FILTERABLE,
        )
}

/// The formats this device renders to, `Rgba16Unorm` only where usable (printing why not).
fn formats(gpu: &Gpu) -> Vec<F> {
    let mut v = vec![
        F::Rgba8Unorm,
        F::Bgra8Unorm,
        F::Rgb10a2Unorm,
        F::Rgba16Float,
    ];
    if unorm16_usable(gpu) {
        v.push(F::Rgba16Unorm);
    } else {
        println!("skipped: Rgba16Unorm is not usable on this device");
    }
    v
}

/// The size of one step of `format` in the unit range.
fn lsb(format: F) -> f64 {
    match format {
        F::Rgba8Unorm | F::Bgra8Unorm => 1.0 / 255.0,
        F::Rgb10a2Unorm => 1.0 / 1023.0,
        F::Rgba16Unorm => 1.0 / 65535.0,
        other => panic!("no step for {other:?}"),
    }
}

/// Paints with `f` at `scale` over a window of `logical` and returns the scene.
fn paint(
    text: &mut TextCache,
    scale: f32,
    logical: (f32, f32),
    f: impl FnOnce(&mut Painter<'_, u8>),
) -> Scene {
    let win = Rect::new(0.0, 0.0, logical.0, logical.1);
    let mut p: Painter<'_, u8> = Painter::new(scale, win, text, None, DARK);
    f(&mut p);
    p.scene
}

/// A fresh renderer for `format` shown on a display of `display_bits`, the scene drawn into a
/// fresh target of `size` device px, and the target as made.
fn draw(
    gpu: &mut Gpu,
    text: &mut TextCache,
    scene: &Scene,
    size: Size,
    scale: f32,
    format: F,
    display_bits: Option<u8>,
) -> wgpu::Texture {
    let size = (size.0 as u32, size.1 as u32);
    let mut renderer = FrameRenderer::with_display(gpu, format, display_bits);
    let texture = dump::target(&gpu.device, format, size);
    let view = texture.create_view(&Default::default());
    let params = FrameParams {
        size,
        scale,
        blur_sigma: 4.0,
        saturation: 1.0,
        ..FrameParams::default()
    };
    renderer
        .draw(gpu, text, scene, &view, params)
        .expect("draw");
    texture
}

/// The scene read back at full precision: four `f32` per pixel, rows unpadded.
fn render_f32(
    gpu: &mut Gpu,
    text: &mut TextCache,
    scene: &Scene,
    size: Size,
    format: F,
    display_bits: Option<u8>,
) -> Vec<f32> {
    let t = draw(gpu, text, scene, size, 1.0, format, display_bits);
    dump::read_rgba_f32(&gpu.device, &gpu.queue, &t).expect("readback")
}

// ── Gradient steps ──────────────────────────────────────────────────────────────────────────────

/// A dark ramp of 8 steps of 8-bit across 512 px.
const FROM: [u8; 4] = [0x0d, 0x0f, 0x1a, 0xff];
const TO: [u8; 4] = [0x15, 0x17, 0x22, 0xff];
const RAMP: Size = (512, 64);

/// The exact ramp at pixel column `x`, channel `ch`, in the unit range (the shader's ramp at the
/// pixel centre, angle 0).
fn ramp_exact(x: usize, ch: usize) -> f64 {
    let t = (x as f64 + 0.5) / RAMP.0 as f64;
    (f64::from(FROM[ch]) + (f64::from(TO[ch]) - f64::from(FROM[ch])) * t) / 255.0
}

fn ramp_scene(text: &mut TextCache) -> Scene {
    paint(text, 1.0, (RAMP.0 as f32, RAMP.1 as f32), |p| {
        p.linear(
            Rect::new(0.0, 0.0, RAMP.0 as f32, RAMP.1 as f32),
            0.0,
            FROM,
            TO,
            0.0,
        );
    })
}

/// What a ramp render measures, in steps of the judging format.
#[derive(Debug, Clone, Copy)]
struct RampStats {
    /// The largest `|tile mean - exact tile mean|` over the aligned 8x8 tiles of every channel.
    tile: f64,
    /// The largest difference between a step of neighbouring tile means (right and below) and
    /// the exact ramp's step there.
    step: f64,
    /// The largest `|pixel - exact|`.
    pixel: f64,
    /// The largest `|pixel - exact| / ulp(exact)` of an f16 value (a float target, no dither).
    ulps: f64,
}

/// The f16 spacing at `v` (for `v` in the normal range).
fn f16_ulp(v: f64) -> f64 {
    2f64.powi(v.log2().floor() as i32 - 10)
}

fn ramp_stats(img: &[f32], judge: f64) -> RampStats {
    let (w, h) = RAMP;
    let at = |x: usize, y: usize, ch: usize| f64::from(img[(y * w + x) * 4 + ch]);
    let (nx, ny) = (w / 8, h / 8);
    let (mut tile, mut step, mut pixel, mut ulps): (f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.0);
    for ch in 0..3 {
        let mut means = vec![0.0; nx * ny];
        let mut exact = vec![0.0; nx * ny];
        for ty in 0..ny {
            for tx in 0..nx {
                let (mut got, mut want) = (0.0, 0.0);
                for y in 0..8 {
                    for x in 0..8 {
                        got += at(tx * 8 + x, ty * 8 + y, ch);
                        want += ramp_exact(tx * 8 + x, ch);
                    }
                }
                means[ty * nx + tx] = got / 64.0;
                exact[ty * nx + tx] = want / 64.0;
                tile = tile.max((got - want).abs() / 64.0 / judge);
            }
        }
        // The step between neighbouring tiles, against the exact ramp's own: a band is a jump
        // the exact ramp does not have (a 16-bit target's ramp itself steps by tens of its
        // steps from one tile to the next).
        for ty in 0..ny {
            for tx in 0..nx {
                let i = ty * nx + tx;
                for j in [
                    (tx + 1 < nx).then_some(i + 1),
                    (ty + 1 < ny).then_some(i + nx),
                ]
                .into_iter()
                .flatten()
                {
                    let got = means[j] - means[i];
                    let want = exact[j] - exact[i];
                    step = step.max((got - want).abs() / judge);
                }
            }
        }
        for y in 0..h {
            for x in 0..w {
                let err = (at(x, y, ch) - ramp_exact(x, ch)).abs();
                pixel = pixel.max(err / judge);
                ulps = ulps.max(err / f16_ulp(ramp_exact(x, ch)));
            }
        }
    }
    RampStats {
        tile,
        step,
        pixel,
        ulps,
    }
}

/// The tolerances of a dithered target, in steps: a tile mean within a quarter step of the exact
/// ramp, each step between neighbouring tile means within a step of the exact ramp's, no pixel
/// more than a step off.
fn dithered_ok(s: &RampStats) -> bool {
    s.tile <= 0.25 && s.step <= 1.0 && s.pixel <= 1.0
}

#[test]
fn a_dithered_ramp_is_exact_to_the_step_of_every_format() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let scene = ramp_scene(&mut text);
    // The float format has no step of its own: its tests are below.
    for format in formats(&gpu).into_iter().filter(|f| *f != F::Rgba16Float) {
        let img = render_f32(&mut gpu, &mut text, &scene, RAMP, format, None);
        let s = ramp_stats(&img, lsb(format));
        println!("ramp {format:?} (no display): {s:?}");
        assert!(dithered_ok(&s), "{format:?}: {s:?}");
        for y in 0..RAMP.1 {
            for x in 0..RAMP.0 {
                assert_eq!(img[(y * RAMP.0 + x) * 4 + 3], 1.0, "{format:?} alpha");
            }
        }
    }
}

#[test]
fn a_float_ramp_with_no_display_is_within_an_f16_ulp() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let scene = ramp_scene(&mut text);
    let img = render_f32(&mut gpu, &mut text, &scene, RAMP, F::Rgba16Float, None);
    let s = ramp_stats(&img, 1.0 / 255.0);
    println!("ramp Rgba16Float (no display): {s:?}");
    assert!(s.ulps <= 1.0, "{s:?}");
    // It is not dithered: the pixels of one column are all the same.
    for x in [0, 100, 255, 511] {
        let first = img[x * 4];
        for y in 0..RAMP.1 {
            assert_eq!(img[(y * RAMP.0 + x) * 4], first, "column {x} row {y}");
        }
    }
}

#[test]
fn a_float_ramp_on_a_10_bit_display_is_judged_at_10_bits() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let scene = ramp_scene(&mut text);
    let img = render_f32(&mut gpu, &mut text, &scene, RAMP, F::Rgba16Float, Some(10));
    let s = ramp_stats(&img, 1.0 / 1023.0);
    println!("ramp Rgba16Float (10-bit display): {s:?}");
    assert!(dithered_ok(&s), "{s:?}");
    // And it is dithered: a column varies (an undithered float ramp would not).
    let varies = (0..RAMP.1).any(|y| img[(y * RAMP.0 + 100) * 4] != img[100 * 4]);
    assert!(varies, "the float ramp on a 10-bit display is not dithered");
}

#[test]
fn the_8_bit_render_judged_at_10_bit_tolerance_fails() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let scene = ramp_scene(&mut text);
    let img = render_f32(&mut gpu, &mut text, &scene, RAMP, F::Rgba8Unorm, None);
    let s8 = ramp_stats(&img, 1.0 / 255.0);
    let s10 = ramp_stats(&img, 1.0 / 1023.0);
    println!("control: 8-bit render judged at 8 bits {s8:?}, at 10 bits {s10:?}");
    assert!(dithered_ok(&s8), "the control's own format: {s8:?}");
    assert!(
        !dithered_ok(&s10),
        "the instrument cannot tell 8 bits from 10: {s10:?}"
    );
}

// ── 8-bit match ─────────────────────────────────────────────────────────────────────────────────

const SCENE_LOGICAL: (f32, f32) = (320.0, 200.0);
const SCENE_SCALE: f32 = 2.0;
const SCENE_SIZE: Size = (640, 400);

/// A 16 by 16 picture of smooth colour (no dithering anywhere in the scene).
fn picture() -> ImageData {
    let mut rgba = Vec::new();
    for y in 0..16u32 {
        for x in 0..16u32 {
            rgba.extend_from_slice(&[(x * 16) as u8, (y * 16) as u8, 200 - (x * 8) as u8, 255]);
        }
    }
    ImageData {
        width: 16,
        height: 16,
        rgba,
    }
}

/// The rects of the glass: the acrylic card and the popup's backdrop, logical px.
fn glass_rects() -> [Rect; 2] {
    let (w, h) = SCENE_LOGICAL;
    [
        Rect::new(24.0, 60.0, w * 0.6, h * 0.5),
        Rect::new(w * 0.4, h * 0.3, w * 0.5, h * 0.5),
    ]
}

/// Fills, rounded rects, a ring, a stroke, text, a picture, an acrylic card and a popup with a
/// backdrop under a transform. Opaque everywhere (the Base fill), so alpha is not in question.
fn match_scene(text: &mut TextCache) -> Scene {
    let [card, menu] = glass_rects();
    paint(text, SCENE_SCALE, SCENE_LOGICAL, |p| {
        let (w, h) = SCENE_LOGICAL;
        let win = Rect::new(0.0, 0.0, w, h);
        p.on(LayerId::Base);
        p.fill(win, 0.0, [0x1c, 0x1e, 0x28, 0xff]);
        p.fill(
            Rect::new(8.0, 8.0, 90.0, 40.0),
            8.0,
            [0x24, 0x6b, 0xc4, 0xff],
        );
        p.fill(
            Rect::new(110.0, 8.0, 90.0, 40.0),
            20.0,
            [0xc4, 0x4a, 0x24, 0xe0],
        );
        p.fill(
            Rect::new(212.0, 8.0, 100.0, 40.0),
            3.0,
            [0x30, 0x90, 0x50, 0x80],
        );
        p.ring(
            Rect::new(8.0, 8.0, w - 16.0, 44.0),
            6.0,
            1.0,
            DARK.border,
            true,
        );
        p.stroke(
            &[[12.0, h - 20.0], [w * 0.5, h - 60.0], [w - 12.0, h - 24.0]],
            false,
            StrokeStyle::solid(DARK.accent, 2.0),
        );
        p.text(
            &Spec::plain("Base layer text", Style::ui(14.0, DARK.fg_primary)),
            16.0,
            16.0,
        );
        p.image(
            Rect::new(w - 80.0, h - 80.0, 64.0, 64.0),
            ImageId(1),
            Fit::Contain,
            ImageAlign::CENTER,
            1.0,
        );
        p.on(LayerId::Page);
        p.acrylic(
            card,
            8.0,
            lune_ui_core::color::premul(DARK.menu_glass),
            [0.0; 4],
            true,
        );
        p.on(LayerId::Popup);
        p.set_xform(Some(Xform {
            origin: (menu.x, menu.y),
            scale: 0.95,
            dx: 0.0,
            dy: 3.0,
            alpha: 0.6,
        }));
        p.backdrop(menu, 8.0);
        p.fill(menu, 8.0, DARK.menu_glass);
        p.ring(menu, 8.0, 1.0, DARK.menu_popup_border, false);
        p.text(
            &Spec::plain("Popup item", Style::ui(13.0, DARK.menu_text_primary)),
            menu.x + 12.0,
            menu.y + 10.0,
        );
        p.set_xform(None);
    })
}

/// Whether device pixel `(x, y)` is in a glass rect, grown by 4 logical px for the popup's
/// transform and the blur's edge.
fn in_glass(x: usize, y: usize) -> bool {
    glass_rects().iter().any(|r| {
        let (px, py) = (x as f32 / SCENE_SCALE, y as f32 / SCENE_SCALE);
        px >= r.x - 4.0 && px < r.x + r.w + 4.0 && py >= r.y - 4.0 && py < r.y + r.h + 4.0
    })
}

#[test]
fn every_format_matches_the_8_bit_render_to_within_a_step() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    gpu.register(ImageId(1), picture()).expect("registers");
    let mut text = text_cache();
    let scene = match_scene(&mut text);
    let read8 = |gpu: &mut Gpu, text: &mut TextCache, format: F| {
        let t = draw(gpu, text, &scene, SCENE_SIZE, SCENE_SCALE, format, None);
        dump::read_rgba(&gpu.device, &gpu.queue, &t).expect("readback")
    };
    let reference = read8(&mut gpu, &mut text, F::Rgba8Unorm);
    let covered = reference
        .chunks_exact(4)
        .filter(|p| p[0] > 0 || p[1] > 0)
        .count();
    assert!(
        covered > 10_000,
        "the reference is nearly empty ({covered})"
    );
    for format in formats(&gpu) {
        let img = read8(&mut gpu, &mut text, format);
        let (mut outside, mut inside) = (0u8, 0u8);
        for y in 0..SCENE_SIZE.1 {
            for x in 0..SCENE_SIZE.0 {
                let i = (y * SCENE_SIZE.0 + x) * 4;
                for ch in 0..4 {
                    let d = img[i + ch].abs_diff(reference[i + ch]);
                    if in_glass(x, y) {
                        inside = inside.max(d);
                    } else {
                        outside = outside.max(d);
                    }
                }
            }
        }
        println!("match {format:?}: outside the glass {outside}, inside {inside}");
        assert!(outside <= 1, "{format:?}: {outside} outside the glass");
        assert!(inside <= 3, "{format:?}: {inside} inside the glass");
    }
}

// ── Alpha ───────────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_alpha_of_each_format_is_what_the_format_can_hold() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (64, 64);
    // A 50 % white fill over a transparent clear.
    let scene = paint(&mut text, 1.0, (64.0, 64.0), |p| {
        p.fill(Rect::new(0.0, 0.0, 64.0, 64.0), 0.0, [255, 255, 255, 128]);
    });
    let alpha_at_centre = |gpu: &mut Gpu, text: &mut TextCache, format: F| {
        let img = render_f32(gpu, text, &scene, size, format, None);
        f64::from(img[(32 * size.0 + 32) * 4 + 3])
    };
    let a10 = alpha_at_centre(&mut gpu, &mut text, F::Rgb10a2Unorm);
    let a16 = alpha_at_centre(&mut gpu, &mut text, F::Rgba16Float);
    let a8 = alpha_at_centre(&mut gpu, &mut text, F::Rgba8Unorm);
    println!(
        "alpha of a 50 % fill: Rgb10a2Unorm {a10:.4}, Rgba16Float {a16:.4}, Rgba8Unorm {a8:.4}"
    );
    assert!(
        [1.0 / 3.0, 2.0 / 3.0]
            .iter()
            .any(|level| (a10 - level).abs() < 1e-6),
        "Rgb10a2Unorm alpha {a10}"
    );
    assert!((a16 - 0.5).abs() <= 0.002, "Rgba16Float alpha {a16}");
    assert!((a8 - 0.5).abs() <= 0.002, "Rgba8Unorm alpha {a8}");
}
