//! Per-contour outline growth, with the push orientation as a parameter.
//!
//! Copied from swash 0.2.10, `src/scale/outline.rs` (`LayerMut::embolden`, the private `embolden`
//! and `compute_winding`), (c) 2020 Chad Brokaw, MIT OR Apache-2.0 (`licenses/swash-LICENSE-MIT`).
//! swash's `embolden` is a port of FreeType's `FT_Outline_EmboldenXY` (FreeType licence,
//! `licenses/FreeType-FTL.txt`). What changed: swash computes one winding for the whole layer and
//! keeps it private; here the winding is an argument of [`grow_layer`], and [`grow`] takes it from
//! a [`Turn`]. The arithmetic, its order and the contour split are swash's, bit for bit.

use swash::scale::outline::Outline;
use swash::zeno::{Point, Verb};

/// Which way [`grow`] pushes each layer's contours out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Turn {
    /// The layer's own concatenated winding, as swash's `Outline::embolden` takes it.
    AsBefore,
    /// The opposite of [`Turn::AsBefore`].
    #[allow(dead_code)] // the orientation rule builds it; today only the tests do
    Other,
}

/// swash's `compute_winding`: the sign of one shoelace sum over the whole point list (every
/// contour of the layer concatenated), `> 0.0` giving 1, anything else 0.
pub(super) fn concat_winding(points: &[Point]) -> u8 {
    if points.is_empty() {
        return 0;
    }
    let mut area = 0.;
    let last = points.len() - 1;
    let mut prev = points[last];
    for cur in points[0..=last].iter() {
        area += (cur.y - prev.y) * (cur.x + prev.x);
        prev = *cur;
    }
    if area > 0. { 1 } else { 0 }
}

/// swash's `LayerMut::embolden` with the winding passed in: grows every contour of one layer by
/// `dx`, `dy`, the contours split on `MoveTo` and `Close` exactly as swash splits them (an
/// out-of-range slice stops the work, as there).
pub(super) fn grow_layer(points: &mut [Point], verbs: &[Verb], winding: u8, dx: f32, dy: f32) {
    let mut point_start = 0;
    let mut pos = 0;
    for verb in verbs {
        match verb {
            Verb::MoveTo | Verb::Close => {
                if let Some(points) = points.get_mut(point_start..pos) {
                    if !points.is_empty() {
                        embolden(points, winding, dx, dy);
                    }
                    point_start = pos;
                    if *verb == Verb::MoveTo {
                        pos += 1;
                    }
                } else {
                    return;
                }
            }
            Verb::LineTo => pos += 1,
            Verb::QuadTo => pos += 2,
            Verb::CurveTo => pos += 3,
        }
    }
    if pos > point_start {
        if let Some(points) = points.get_mut(point_start..pos) {
            embolden(points, winding, dx, dy);
        }
    }
}

/// swash's `Outline::embolden`: every layer grown by `dx`, `dy`, pushed the way `turn` says.
/// `AsBefore` is bit-identical to swash.
pub(super) fn grow(outline: &mut Outline, turn: Turn, dx: f32, dy: f32) {
    for i in 0..outline.len() {
        if let Some(mut layer) = outline.get_mut(i) {
            let verbs = layer.verbs();
            let points = layer.points_mut();
            let concat = concat_winding(points);
            let winding = match turn {
                Turn::AsBefore => concat,
                Turn::Other => 1 - concat,
            };
            grow_layer(points, verbs, winding, dx, dy);
        }
    }
}

// swash's per-contour `embolden`, copied as is: a rewrite that reorders float operations breaks
// the bit-exact test below (`grow_as_before_is_swashs_embolden`).
fn embolden(points: &mut [Point], winding: u8, x_strength: f32, y_strength: f32) {
    if points.is_empty() {
        return;
    }
    let last = points.len() - 1;
    let mut i = last;
    let mut j = 0;
    let mut k = !0;
    let mut out_len;
    let mut in_len = 0.;
    let mut anchor_len = 0.;
    let mut anchor = Point::ZERO;
    let mut out;
    let mut in_ = Point::ZERO;
    while j != i && i != k {
        if j != k {
            out = points[j] - points[i];
            out_len = out.length();
            if out_len == 0. {
                j = if j < last { j + 1 } else { 0 };
                continue;
            } else {
                let s = 1. / out_len;
                out.x *= s;
                out.y *= s;
            }
        } else {
            out = anchor;
            out_len = anchor_len;
        }
        if in_len != 0. {
            if k == !0 {
                k = i;
                anchor = in_;
                anchor_len = in_len;
            }
            let mut d = (in_.x * out.x) + (in_.y * out.y);
            let shift = if d > -0.9396 {
                d += 1.;
                let mut sx = in_.y + out.y;
                let mut sy = in_.x + out.x;
                if winding == 0 {
                    sx = -sx;
                } else {
                    sy = -sy;
                }
                let mut q = (out.x * in_.y) - (out.y * in_.x);
                if winding == 0 {
                    q = -q;
                }
                let l = in_len.min(out_len);
                if x_strength * q <= l * d {
                    sx = sx * x_strength / d;
                } else {
                    sx = sx * l / q;
                }
                if y_strength * q <= l * d {
                    sy = sy * y_strength / d;
                } else {
                    sy = sy * l / q;
                }
                Point::new(sx, sy)
            } else {
                Point::ZERO
            };

            while i != j {
                points[i].x += x_strength + shift.x;
                points[i].y += y_strength + shift.y;
                i = if i < last { i + 1 } else { 0 };
            }
        } else {
            i = j;
        }
        in_ = out;
        in_len = out_len;
        j = if j < last { j + 1 } else { 0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lune_ui_core::text::TextRendering;
    use swash::FontRef;
    use swash::scale::ScaleContext;

    const SANS: &[u8] = include_bytes!("../../../../apps/gallery/assets/RznSans-Regular.ttf");
    const MONO: &[u8] =
        include_bytes!("../../../../apps/gallery/assets/RobotoMonoNerdFontMono-Regular.ttf");

    fn bits(outline: &Outline) -> Vec<(u32, u32)> {
        outline
            .points()
            .iter()
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .collect()
    }

    /// Every glyph of `data` at `size` (hinting as given), grown by SMOOTH's dilation at `size`
    /// through [`grow`] at `AsBefore` and through swash's `Outline::embolden`: the points must
    /// match bit for bit. Returns how many glyphs with an outline were compared. `stride` takes
    /// every `stride`-th glyph id.
    fn oracle(data: &[u8], size: f32, hint: bool, stride: usize) -> usize {
        let font = FontRef::from_index(data, 0).expect("the font parses");
        let [dx, dy] = TextRendering::SMOOTH.glyph_params(size).dilation();
        assert!(dx > 0.0 && dy > 0.0, "SMOOTH dilates at {size}");
        let mut ctx = ScaleContext::new();
        let mut scaler = ctx.builder(font).size(size).hint(hint).build();
        let mut compared = 0;
        for id in (0..font.glyph_metrics(&[]).glyph_count()).step_by(stride) {
            let Some(outline) = scaler.scale_outline(id) else {
                continue;
            };
            if outline.points().is_empty() {
                continue;
            }
            let mut want = outline.clone();
            want.embolden(dx, dy);
            let mut got = outline;
            grow(&mut got, Turn::AsBefore, dx, dy);
            assert_eq!(bits(&got), bits(&want), "glyph {id} at {size} px");
            compared += 1;
        }
        compared
    }

    /// E1: the copy matches swash bit for bit on every glyph of both gallery fonts.
    #[test]
    fn grow_as_before_is_swashs_embolden() {
        let mut total = 0;
        for (name, data, stride, min) in [("RznSans", SANS, 1, 400), ("RobotoMono", MONO, 1, 5000)]
        {
            for (size, hint) in [(13.0, false), (26.0, false), (13.0, true)] {
                let n = oracle(data, size, hint, stride);
                println!("{name} {size} px hinted={hint}: {n} glyphs compared");
                assert!(n >= min, "{name} {size} px: only {n} glyphs compared");
                total += n;
            }
        }
        println!("E1: {total} glyph comparisons");
    }

    const VERBS: [Verb; 10] = [
        Verb::MoveTo,
        Verb::LineTo,
        Verb::LineTo,
        Verb::LineTo,
        Verb::Close,
        Verb::MoveTo,
        Verb::LineTo,
        Verb::LineTo,
        Verb::LineTo,
        Verb::Close,
    ];

    fn pts(raw: [(f32, f32); 8]) -> Vec<Point> {
        raw.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    /// Two counter-clockwise squares: swash's concatenated sign is 0 (it thins), though the
    /// contours' own areas sum to a positive number.
    fn trigger() -> Vec<Point> {
        pts([
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (110.0, 10.0),
            (100.0, 10.0),
            (100.0, 0.0),
            (110.0, 0.0),
        ])
    }

    /// The trigger mirrored about x = 60, each contour kept counter-clockwise.
    fn mirrored() -> Vec<Point> {
        pts([
            (120.0, 10.0),
            (110.0, 10.0),
            (110.0, 0.0),
            (120.0, 0.0),
            (20.0, 10.0),
            (10.0, 10.0),
            (10.0, 0.0),
            (20.0, 0.0),
        ])
    }

    /// Standard signed area (positive counter-clockwise, y up) of each of the two squares.
    fn areas(points: &[Point]) -> [f64; 2] {
        let one = |sq: &[Point]| {
            let mut s = 0.0;
            for (i, a) in sq.iter().enumerate() {
                let b = sq[(i + 1) % sq.len()];
                s += f64::from(a.x) * f64::from(b.y) - f64::from(b.x) * f64::from(a.y);
            }
            s / 2.0
        };
        [one(&points[..4]), one(&points[4..])]
    }

    /// The side lengths (width, height) of each square's bounding box.
    fn boxes(points: &[Point]) -> [(f32, f32); 2] {
        let one = |sq: &[Point]| {
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in sq {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            (x1 - x0, y1 - y0)
        };
        [one(&points[..4]), one(&points[4..])]
    }

    fn grown(points: &[Point], winding: u8) -> Vec<Point> {
        let mut p = points.to_vec();
        grow_layer(&mut p, &VERBS, winding, 0.3, 0.3);
        p
    }

    /// E2: on the trigger the concatenated sign shrinks both squares and the other orientation
    /// grows each to 10.6 x 10.6.
    #[test]
    fn synthetic_trigger_shrinks_at_the_concatenated_sign() {
        let t = trigger();
        assert_eq!(concat_winding(&t), 0);
        let [a, b] = areas(&t);
        assert!(
            a + b > 0.0,
            "the contours' own areas sum positive: {}",
            a + b
        );
        let before = grown(&t, concat_winding(&t));
        let [a1, b1] = areas(&before);
        assert!(
            a1 + b1 < a + b,
            "AsBefore shrinks: {} -> {}",
            a + b,
            a1 + b1
        );
        let other = grown(&t, 1 - concat_winding(&t));
        let [a2, b2] = areas(&other);
        assert!(a2 + b2 > a + b, "Other grows: {} -> {}", a + b, a2 + b2);
        for (w, h) in boxes(&other) {
            assert!(
                (w - 10.6).abs() < 1e-3 && (h - 10.6).abs() < 1e-3,
                "{w} x {h}"
            );
        }
    }

    /// E3: the mirrored pair has concatenated sign 1; grown at that sign it matches the trigger
    /// grown at `Other`.
    #[test]
    fn mirrored_pair_at_the_concatenated_sign_matches_the_trigger_at_other() {
        let m = mirrored();
        assert_eq!(concat_winding(&m), 1);
        let [ma, mb] = areas(&grown(&m, concat_winding(&m)));
        let t = trigger();
        let [ta, tb] = areas(&grown(&t, 1 - concat_winding(&t)));
        let (got, want) = (ma + mb, ta + tb);
        assert!(((got - want) / want).abs() < 1e-5, "{got} vs {want}");
    }
}
