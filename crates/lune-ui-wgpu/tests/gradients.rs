//! Pixel measurements of the gradient and shadow ops (`Painter::linear`, `radial`, `shadow`)
//! drawn through the real renderer into an offscreen `Rgba8Unorm` target at scale 1 and read back.
//!
//! The ops are dithered by an 8x8 ordered pattern (D326), so a single pixel is off by up to
//! one step but the mean of an aligned 8x8 tile is the exact value: the tests compare tile means
//! with the exact value computed on the CPU in f64 at pixel centres, and run the same check on a
//! CPU-rounded copy of the exact value, which must fail it (the check sees bands).
//!
//! GPU test: with no adapter it prints `skipped: no GPU adapter` and passes, unless
//! `LUNEUI_REQUIRE_GPU=1`, which makes a missing adapter a failure.

use lune_ui_core::paint::{Falloff, Painter, Rect};
use lune_ui_core::scene::Scene;
use lune_ui_core::theme::DARK;
use lune_ui_wgpu::dump;
use lune_ui_wgpu::frame::{FrameParams, FrameRenderer};
use lune_ui_wgpu::gpu::{self, Gpu};
use lune_ui_wgpu::text_cache::TextCache;

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

/// Paints with `f` at scale 1 over a window of `size` and returns the scene.
fn paint(text: &mut TextCache, size: Size, f: impl FnOnce(&mut Painter<'_, u8>)) -> Scene {
    let win = Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32);
    let mut p: Painter<'_, u8> = Painter::new(1.0, win, text, None, DARK);
    f(&mut p);
    p.scene
}

/// The scene drawn into a fresh `Rgba8Unorm` target of `size`, read back (rows unpadded).
fn render(gpu: &mut Gpu, text: &mut TextCache, scene: &Scene, size: Size) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let size = (size.0 as u32, size.1 as u32);
    let mut renderer = FrameRenderer::new(gpu, format);
    let texture = dump::target(&gpu.device, format, size);
    let view = texture.create_view(&Default::default());
    let params = FrameParams {
        size,
        scale: 1.0,
        blur_sigma: 4.0,
        saturation: 1.0,
        ..FrameParams::default()
    };
    renderer
        .draw(gpu, text, scene, &view, params)
        .expect("draw");
    dump::read_rgba(&gpu.device, &gpu.queue, &texture).expect("readback")
}

fn px(img: &[u8], size: Size, x: usize, y: usize) -> [u8; 4] {
    let i = (y * size.0 + x) * 4;
    [img[i], img[i + 1], img[i + 2], img[i + 3]]
}

/// One channel of the image, in LSB (0 to 255).
fn chan(img: &[u8], size: Size, x: usize, y: usize, ch: usize) -> f64 {
    f64::from(px(img, size, x, y)[ch])
}

/// The mean of `f` over each aligned 8x8 tile, in row-major tile order.
fn tile_means(size: Size, f: impl Fn(usize, usize) -> f64) -> Vec<f64> {
    assert!(size.0 % 8 == 0 && size.1 % 8 == 0);
    let mut out = Vec::new();
    for ty in 0..size.1 / 8 {
        for tx in 0..size.0 / 8 {
            let mut sum = 0.0;
            for y in 0..8 {
                for x in 0..8 {
                    sum += f(tx * 8 + x, ty * 8 + y);
                }
            }
            out.push(sum / 64.0);
        }
    }
    out
}

/// Whether every pixel of tile `(tx, ty)` satisfies `pred`.
fn tile_all(tx: usize, ty: usize, pred: impl Fn(usize, usize) -> bool) -> bool {
    (0..8).all(|y| (0..8).all(|x| pred(tx * 8 + x, ty * 8 + y)))
}

/// The largest difference between neighbouring tile means (right and below).
fn max_adjacent_step(size: Size, means: &[f64]) -> f64 {
    let nx = size.0 / 8;
    let ny = size.1 / 8;
    let mut worst: f64 = 0.0;
    for ty in 0..ny {
        for tx in 0..nx {
            let m = means[ty * nx + tx];
            if tx + 1 < nx {
                worst = worst.max((means[ty * nx + tx + 1] - m).abs());
            }
            if ty + 1 < ny {
                worst = worst.max((means[(ty + 1) * nx + tx] - m).abs());
            }
        }
    }
    worst
}

/// The largest difference between the steps (right and below) of two lists of tile means.
fn max_step_difference(size: Size, a: &[f64], b: &[f64]) -> f64 {
    let nx = size.0 / 8;
    let ny = size.1 / 8;
    let mut worst: f64 = 0.0;
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
                worst = worst.max(((a[j] - a[i]) - (b[j] - b[i])).abs());
            }
        }
    }
    worst
}

/// The largest `|a - b|` over two lists of tile means.
fn max_tile_error(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}

/// A straight colour as premultiplied channels in LSB (0 to 255) and its alpha in LSB.
fn premul_lsb(c: [u8; 4]) -> ([f64; 3], f64) {
    let a = f64::from(c[3]) / 255.0;
    (
        [
            f64::from(c[0]) * a,
            f64::from(c[1]) * a,
            f64::from(c[2]) * a,
        ],
        f64::from(c[3]),
    )
}

/// The exact result of premultiplied `c` (LSB) with alpha `a` (LSB) over an opaque `dst` (LSB).
fn exact_over(c: f64, a: f64, dst: f64) -> f64 {
    c + dst * (1.0 - a / 255.0)
}

/// The mean of one output channel under the dither (D326): the colour is lifted to `h`
/// (half an LSB, faded in below one LSB of alpha) from both ends, then swings around that.
fn expected_over(c: f64, a: f64, dst: f64) -> f64 {
    let h = 0.5 * a.clamp(0.0, 1.0);
    c.clamp(h, 255.0 - h) + dst * (1.0 - a / 255.0)
}

// ── Linear ──────────────────────────────────────────────────────────────────────────────────────

const FROM: [u8; 4] = [0x0d, 0x0f, 0x1a, 0xff];
const TO: [u8; 4] = [0x14, 0x1a, 0x33, 0xff];

/// The ramp parameter at the centre of pixel `(x, y)` of a rect filling `size` (D323).
fn ramp_t(size: Size, angle: f64, x: usize, y: usize) -> f64 {
    let (w, h) = (size.0 as f64, size.1 as f64);
    let (dx, dy) = (angle.cos(), angle.sin());
    let span = (w / 2.0 * dx).abs() + (h / 2.0 * dy).abs();
    let along = (x as f64 + 0.5 - w / 2.0) * dx + (y as f64 + 0.5 - h / 2.0) * dy;
    (along / (2.0 * span) + 0.5).clamp(0.0, 1.0)
}

fn ramp_exact(size: Size, angle: f64, x: usize, y: usize, ch: usize) -> f64 {
    let t = ramp_t(size, angle, x, y);
    f64::from(FROM[ch]) + (f64::from(TO[ch]) - f64::from(FROM[ch])) * t
}

/// Draws a full-target opaque linear ramp `FROM` to `TO` at `angle` and runs the tile checks.
fn check_ramp(size: Size, angle: f32, per_channel_control: bool) {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let scene = paint(&mut text, size, |p| {
        p.linear(
            Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32),
            0.0,
            FROM,
            TO,
            angle,
        );
    });
    let img = render(&mut gpu, &mut text, &scene, size);
    let angle = f64::from(angle);

    let mut control: f64 = 0.0;
    for ch in 0..3 {
        let exact = tile_means(size, |x, y| ramp_exact(size, angle, x, y, ch));
        let got = tile_means(size, |x, y| chan(&img, size, x, y, ch));
        let err = max_tile_error(&got, &exact);
        assert!(
            err <= 0.2,
            "angle {angle} ch {ch}: tile mean error {err:.4} LSB"
        );
        let step = max_adjacent_step(size, &got);
        assert!(
            step <= 1.0,
            "angle {angle} ch {ch}: adjacent tile step {step:.4} LSB"
        );

        let mut worst: f64 = 0.0;
        for y in 0..size.1 {
            for x in 0..size.0 {
                worst = worst
                    .max((chan(&img, size, x, y, ch) - ramp_exact(size, angle, x, y, ch)).abs());
            }
        }
        assert!(
            worst < 1.0,
            "angle {angle} ch {ch}: pixel error {worst:.4} LSB"
        );

        // The CPU-rounded exact ramp, checked the same way, is the control.
        let rounded = tile_means(size, |x, y| ramp_exact(size, angle, x, y, ch).round());
        let err = max_tile_error(&rounded, &exact);
        if per_channel_control {
            assert!(
                err > 0.4,
                "angle {angle} ch {ch}: the rounded control's tile error is only {err:.4} LSB"
            );
        }
        control = control.max(err);
    }
    // The instrument must see bands: the rounded control fails the check (T1: in every channel;
    // the 2-D angles: in the worst channel, since a channel with many levels over a short ramp
    // has narrow bands that an 8x8 tile mean averages).
    assert!(
        control > 0.4,
        "angle {angle}: the rounded control's tile error is only {control:.4} LSB"
    );
    for y in 0..size.1 {
        for x in 0..size.0 {
            assert_eq!(px(&img, size, x, y)[3], 255, "alpha at ({x}, {y})");
        }
    }
}

#[test]
fn a_horizontal_ramp_has_exact_tile_means_and_the_rounded_control_fails() {
    check_ramp((1024, 64), 0.0, true);
}

#[test]
fn a_vertical_ramp_runs_top_to_bottom() {
    check_ramp((512, 512), std::f32::consts::FRAC_PI_2, false);
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (512, 512);
    let scene = paint(&mut text, size, |p| {
        p.linear(
            Rect::new(0.0, 0.0, 512.0, 512.0),
            0.0,
            FROM,
            TO,
            std::f32::consts::FRAC_PI_2,
        );
    });
    let img = render(&mut gpu, &mut text, &scene, size);
    for ch in 0..3 {
        assert!(
            (chan(&img, size, 256, 0, ch) - f64::from(FROM[ch])).abs() <= 1.0,
            "top, ch {ch}"
        );
        assert!(
            (chan(&img, size, 256, 511, ch) - f64::from(TO[ch])).abs() <= 1.0,
            "bottom, ch {ch}"
        );
    }
}

#[test]
fn a_diagonal_ramp_has_exact_tile_means() {
    check_ramp((512, 512), std::f32::consts::FRAC_PI_4, false);
}

#[test]
fn a_plain_fill_stays_exact() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (64, 64);
    let scene = paint(&mut text, size, |p| {
        p.fill(Rect::new(0.0, 0.0, 64.0, 64.0), 0.0, TO);
    });
    let img = render(&mut gpu, &mut text, &scene, size);
    for y in 0..64 {
        for x in 0..64 {
            assert_eq!(px(&img, size, x, y), [0x14, 0x1a, 0x33, 0xff], "({x}, {y})");
        }
    }
}

#[test]
fn a_linear_inside_a_clip_leaves_the_outside_untouched() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (512, 64);
    let scene = paint(&mut text, size, |p| {
        p.fill(Rect::new(0.0, 0.0, 512.0, 64.0), 0.0, FROM);
        p.push_clip(Rect::new(100.0, 10.0, 200.0, 30.0));
        p.linear(Rect::new(0.0, 0.0, 512.0, 64.0), 0.0, TO, FROM, 0.0);
        p.pop_clip();
    });
    let img = render(&mut gpu, &mut text, &scene, size);
    let mut changed = 0;
    for y in 0..64 {
        for x in 0..512 {
            let inside = (100..300).contains(&x) && (10..40).contains(&y);
            if inside {
                changed += usize::from(px(&img, size, x, y) != FROM);
            } else {
                assert_eq!(px(&img, size, x, y), FROM, "outside the clip at ({x}, {y})");
            }
        }
    }
    assert!(
        changed > 1000,
        "the ramp must show inside the clip ({changed} px)"
    );
}

// ── Radial ──────────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_radial_light_has_exact_tile_means_and_stays_in_its_box() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (512, 512);
    let bg = [0x0d, 0x0f, 0x1a, 0xff];
    let inner = [150, 190, 255, 160];
    let outer = [150, 190, 255, 0];
    let (cx, cy, rad) = (256.0, 256.0, 200.0);
    let scene = paint(&mut text, size, |p| {
        p.fill(Rect::new(0.0, 0.0, 512.0, 512.0), 0.0, bg);
        p.radial(
            [256.0, 256.0],
            [200.0, 200.0],
            inner,
            outer,
            Falloff::Smooth,
        );
    });
    let img = render(&mut gpu, &mut text, &scene, size);

    let (inner_c, inner_a) = premul_lsb(inner);
    // The source (premultiplied colour, alpha) at pixel (x, y): D324's smooth falloff and rim.
    let source = |x: usize, y: usize| -> ([f64; 3], f64) {
        let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
        let t = (dx * dx + dy * dy).sqrt() / rad;
        let u = t.min(1.0);
        let k = 1.0 - u * u * (3.0 - 2.0 * u);
        let edge = ((1.0 - t) * rad + 0.5).clamp(0.0, 1.0);
        let w = k * edge;
        (
            [inner_c[0] * w, inner_c[1] * w, inner_c[2] * w],
            inner_a * w,
        )
    };

    let nx = size.0 / 8;
    let (mut worst_solid, mut worst_other): (f64, f64) = (0.0, 0.0);
    for ch in 0..3 {
        let dst = f64::from(bg[ch]);
        let model = |x: usize, y: usize| {
            let (c, a) = source(x, y);
            expected_over(c[ch], a, dst)
        };
        let want = tile_means(size, model);
        let got = tile_means(size, |x, y| chan(&img, size, x, y, ch));
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            let (tx, ty) = (i % nx, i / nx);
            let solid = tile_all(tx, ty, |x, y| source(x, y).1 >= 1.0);
            let err = (g - w).abs();
            if solid {
                worst_solid = worst_solid.max(err);
            } else {
                worst_other = worst_other.max(err);
            }
        }
    }
    println!("radial: worst tile error: solid {worst_solid:.4}, other {worst_other:.4} LSB");
    assert!(
        worst_solid <= 0.3,
        "solid tiles: worst error {worst_solid:.4} LSB"
    );
    assert!(
        worst_other <= 0.5,
        "other tiles: worst error {worst_other:.4} LSB"
    );
    for y in 0..size.1 {
        for x in 0..size.0 {
            let outside = !(56..456).contains(&x) || !(56..456).contains(&y);
            if outside {
                assert_eq!(px(&img, size, x, y), bg, "outside the box at ({x}, {y})");
            }
        }
    }
}

// ── Shadow ──────────────────────────────────────────────────────────────────────────────────────

const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// Whether `(px, py)` is inside the rounded rect of centre `(cx, cy)`, half extents `(hx, hy)`.
fn inside_rounded(px: f64, py: f64, c: (f64, f64), half: (f64, f64), r: f64) -> bool {
    let (qx, qy) = (
        (px - c.0).abs() - (half.0 - r),
        (py - c.1).abs() - (half.1 - r),
    );
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    qx.max(qy).min(0.0) + outside - r <= 0.0
}

/// The rounded rect blurred by a Gaussian of `sigma`, at `(px, py)`: a brute-force sum of the
/// hard indicator over a 0.25 px grid out to 5 sigma, weighted by a normalised Gaussian. Shares
/// nothing with the shader (no erf, no slices).
fn shadow_reference(px: f64, py: f64, c: (f64, f64), half: (f64, f64), r: f64, sigma: f64) -> f64 {
    let step = 0.25;
    let n = (5.0 * sigma / step).ceil() as i64;
    let offsets: Vec<(f64, f64)> = (-n..n)
        .map(|i| {
            let u = (i as f64 + 0.5) * step;
            (u, (-0.5 * (u / sigma).powi(2)).exp())
        })
        .collect();
    let norm: f64 = offsets.iter().map(|o| o.1).sum();
    let mut sum = 0.0;
    for &(u, wu) in &offsets {
        for &(v, wv) in &offsets {
            if inside_rounded(px + u, py + v, c, half, r) {
                sum += wu * wv;
            }
        }
    }
    sum / (norm * norm)
}

#[test]
fn a_shadow_matches_a_brute_force_blur() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (240, 200);
    let (rect, radius, sigma) = (Rect::new(60.0, 60.0, 120.0, 80.0), 12.0_f32, 10.0_f32);
    let scene = paint(&mut text, size, |p| {
        p.fill(Rect::new(0.0, 0.0, 240.0, 200.0), 0.0, BLACK);
        p.shadow(rect, radius, sigma, WHITE);
    });
    let img = render(&mut gpu, &mut text, &scene, size);
    let (centre, half) = ((120.0, 100.0), (60.0, 40.0));

    // Offsets in px from the rect centre: deep inside, the edge midpoints, the corner and its
    // neighbours, 1, 2 and 3 sigma outside an edge and outside the corner diagonal.
    let points: [(i32, i32); 20] = [
        (0, 0),
        (30, 20),
        (-30, -20),
        (59, 0),
        (-60, 0),
        (0, 39),
        (0, -40),
        (60, 40),
        (55, 35),
        (-60, -40),
        (70, 0),
        (80, 0),
        (90, 0),
        (0, 50),
        (0, 60),
        (0, 70),
        (70, 50),
        (80, 60),
        (-80, 0),
        (45, 30),
    ];
    let mut worst: f64 = 0.0;
    for (dx, dy) in points {
        let (x, y) = ((120 + dx) as usize, (100 + dy) as usize);
        let reference = 255.0
            * shadow_reference(
                f64::from(x as i32) + 0.5,
                f64::from(y as i32) + 0.5,
                centre,
                half,
                f64::from(radius),
                f64::from(sigma),
            );
        for ch in 0..3 {
            let got = chan(&img, size, x, y, ch);
            worst = worst.max((got - reference).abs());
            assert!(
                (got - reference).abs() <= 1.5,
                "offset ({dx}, {dy}) ch {ch}: got {got}, reference {reference:.3}"
            );
        }
        assert_eq!(px(&img, size, x, y)[3], 255);
    }
    println!(
        "shadow: worst error over {} points: {worst:.3} LSB",
        points.len()
    );

    // The pad: 2 sigma outside the edge is not background; beyond 4 sigma + 2 px it is.
    assert!(px(&img, size, 200, 100)[0] > 0, "2 sigma outside the rect");
    for y in 0..size.1 {
        for x in 0..size.0 {
            if !(18..222).contains(&x) || !(18..182).contains(&y) {
                assert_eq!(px(&img, size, x, y), BLACK, "beyond the pad at ({x}, {y})");
            }
        }
    }
}

#[test]
fn a_shadow_of_a_square_rect_is_half_the_colour_at_its_edge() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (240, 200);
    // The edges run through the pixel centres 59.5 (left) and 179.5 (right).
    let scene = paint(&mut text, size, |p| {
        p.fill(Rect::new(0.0, 0.0, 240.0, 200.0), 0.0, BLACK);
        p.shadow(Rect::new(59.5, 59.5, 120.0, 80.0), 0.0, 10.0, WHITE);
    });
    let img = render(&mut gpu, &mut text, &scene, size);
    for (x, y) in [(179, 99), (59, 99), (119, 59), (119, 139)] {
        for ch in 0..3 {
            let got = chan(&img, size, x, y, ch);
            assert!(
                (got - 127.5).abs() <= 1.0,
                "edge pixel ({x}, {y}) ch {ch}: {got}"
            );
        }
    }
}

#[test]
fn a_negative_shadow_radius_draws_the_square_shadow() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (240, 200);
    let draw = |gpu: &mut Gpu, text: &mut TextCache, radius: f32| {
        let scene = paint(text, size, |p| {
            p.fill(Rect::new(0.0, 0.0, 240.0, 200.0), 0.0, BLACK);
            p.shadow(Rect::new(59.5, 59.5, 120.0, 80.0), radius, 10.0, WHITE);
        });
        render(gpu, text, &scene, size)
    };
    let square = draw(&mut gpu, &mut text, 0.0);
    let negative = draw(&mut gpu, &mut text, -5.0);
    assert_eq!(negative, square, "a negative radius is a radius of 0");
}

#[test]
fn a_black_shadow_over_grey_has_exact_tile_means() {
    let Some(mut gpu) = headless_gpu() else {
        return;
    };
    let mut text = text_cache();
    let size = (768, 64);
    let grey = [0x20, 0x20, 0x20, 0xff];
    let sigma = 48.0_f64;
    let scene = paint(&mut text, size, |p| {
        p.fill(Rect::new(0.0, 0.0, 768.0, 64.0), 0.0, grey);
        p.shadow(
            Rect::new(-256.0, -512.0, 512.0, 1088.0),
            0.0,
            sigma as f32,
            [0, 0, 0, 153],
        );
    });
    let img = render(&mut gpu, &mut text, &scene, size);

    // The shadow's alpha in LSB at column x: 153 times the Gaussian mass over [-256, 256],
    // integrated numerically (midpoint rule, 0.05 px), not by erf.
    let alpha = |x: usize| -> f64 {
        let xc = x as f64 + 0.5;
        let (lo, hi) = (
            (-256.0_f64).max(xc - 8.0 * sigma),
            256.0_f64.min(xc + 8.0 * sigma),
        );
        let norm = sigma * (2.0 * std::f64::consts::PI).sqrt();
        let step = 0.05;
        let n = ((hi - lo) / step).max(0.0).round() as i64;
        let mass: f64 = (0..n)
            .map(|i| {
                let u = lo + (i as f64 + 0.5) * step;
                (-0.5 * ((u - xc) / sigma).powi(2)).exp()
            })
            .sum::<f64>()
            * step
            / norm;
        153.0 * mass
    };
    let dst = 32.0;
    let a_at: Vec<f64> = (0..size.0).map(alpha).collect();

    let nx = size.0 / 8;
    let exact_fn = |x: usize, _y: usize| exact_over(0.0, a_at[x], dst);
    let expected_fn = |x: usize, _y: usize| expected_over(0.0, a_at[x], dst);
    let exact = tile_means(size, exact_fn);
    let expected = tile_means(size, expected_fn);
    println!(
        "shadow over grey: the exact strip's own adjacent tile step is {:.4} LSB",
        max_adjacent_step(size, &exact)
    );
    for ch in 0..3 {
        let got = tile_means(size, |x, y| chan(&img, size, x, y, ch));
        for (i, ((g, e), x)) in got.iter().zip(&expected).zip(&exact).enumerate() {
            let (tx, ty) = (i % nx, i / nx);
            let solid = tile_all(tx, ty, |px, _| a_at[px] >= 1.0);
            if solid {
                assert!(
                    (g - e).abs() <= 0.2,
                    "ch {ch} tile ({tx}, {ty}): got {g:.4}, expected {e:.4}"
                );
            }
            assert!(
                (g - x).abs() <= 0.7,
                "ch {ch} tile ({tx}, {ty}): got {g:.4}, exact {x:.4}"
            );
        }
        // No extra band: each adjacent tile-mean step equals the exact strip's own step.
        let step_err = max_step_difference(size, &got, &exact);
        println!("shadow over grey: ch {ch}: adjacent tile step error {step_err:.4} LSB");
        assert!(
            step_err <= 0.25,
            "ch {ch}: adjacent tile step differs from the exact strip's by {step_err:.4} LSB"
        );
    }
    // Negative control: the CPU-rounded exact strip fails this test's own tile-mean check
    // (limit 0.2 LSB against the expected means).
    let rounded = tile_means(size, |x, y| exact_fn(x, y).round());
    let control = max_tile_error(&rounded, &exact);
    let step_control = max_step_difference(size, &rounded, &exact);
    println!(
        "shadow over grey: rounded control: tile error {control:.4}, step error {step_control:.4} LSB"
    );
    assert!(
        control > 0.2,
        "the rounded control's tile error is only {control:.4} LSB"
    );
    assert!(
        step_control > 0.25,
        "the rounded control's step error is only {step_control:.4} LSB"
    );

    for y in 0..size.1 {
        for x in (256 + 192 + 2 + 1)..size.0 {
            assert_eq!(px(&img, size, x, y), grey, "beyond the pad at ({x}, {y})");
        }
    }
}
