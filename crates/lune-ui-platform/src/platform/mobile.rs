//! The OS code of the phone functions, one file per platform; the safe API is `crate::mobile`.

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod other;

#[cfg(target_os = "android")]
pub(crate) use android::{KeyboardInset, Pasteboard, attach_to_scene, safe_area};
#[cfg(target_os = "ios")]
pub(crate) use ios::{KeyboardInset, Pasteboard, attach_to_scene, safe_area};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use other::{KeyboardInset, Pasteboard, attach_to_scene, safe_area};

/// The safe-area insets, in logical pixels, from a window's geometry in physical pixels: the offset
/// of the inner rectangle inside the outer one on the left and top, the rest on the right and
/// bottom, each clamped to at least 0. A scale that is not a positive finite number gives zero.
#[cfg(any(target_os = "ios", test))]
pub(crate) fn insets_from_geometry(
    inner_pos: (i32, i32),
    outer_pos: (i32, i32),
    inner_size: (u32, u32),
    outer_size: (u32, u32),
    scale: f64,
) -> lune_ui_core::layout::Insets {
    use lune_ui_core::layout::Insets;
    if !(scale.is_finite() && scale > 0.0) {
        return Insets::ZERO;
    }
    let left = f64::from(inner_pos.0) - f64::from(outer_pos.0);
    let top = f64::from(inner_pos.1) - f64::from(outer_pos.1);
    let right = f64::from(outer_size.0) - f64::from(inner_size.0) - left;
    let bottom = f64::from(outer_size.1) - f64::from(inner_size.1) - top;
    let logical = |v: f64| {
        let l = (v / scale).max(0.0);
        if l.is_finite() { l as f32 } else { 0.0 }
    };
    Insets::new(logical(left), logical(top), logical(right), logical(bottom))
}

/// The safe-area insets, in logical pixels, from an Android activity's content rect (`rect`: left,
/// top, right, bottom, in physical pixels of the native window) and the native window's `size`
/// (D7): left = `l`, top = `t`, right = `w - r`, bottom = `h - b`, each clamped to at least 0. A
/// scale that is not a positive finite number, an empty window or a rect that is reversed
/// (`l > r` or `t > b`) gives zero; a rect that reaches outside the window clamps to zero on that
/// side.
#[cfg(any(target_os = "android", test))]
pub(crate) fn insets_from_content_rect(
    rect: (i32, i32, i32, i32),
    size: (u32, u32),
    scale: f64,
) -> lune_ui_core::layout::Insets {
    use lune_ui_core::layout::Insets;
    let (l, t, r, b) = rect;
    if !(scale.is_finite() && scale > 0.0) || size.0 == 0 || size.1 == 0 || l > r || t > b {
        return Insets::ZERO;
    }
    let logical = |v: i64| {
        let l = (v as f64 / scale).max(0.0);
        if l.is_finite() { l as f32 } else { 0.0 }
    };
    let (w, h) = (i64::from(size.0), i64::from(size.1));
    Insets::new(
        logical(i64::from(l)),
        logical(i64::from(t)),
        logical(w - i64::from(r)),
        logical(h - i64::from(b)),
    )
}

/// What the Android keyboard split remembers (D8): the native window's size and the smallest
/// bottom inset seen at that size, the bottom the system bars alone cover.
#[cfg(any(target_os = "android", test))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BottomRecord {
    pub(crate) size: (u32, u32),
    pub(crate) baseline: f32,
}

/// Splits the content rect's bottom inset `bottom` (logical pixels, at window size `size`) into the
/// safe area's bottom and the soft keyboard's: the record's baseline (the smallest bottom inset
/// seen at this size; a new size starts a new record at `bottom`) and, when `bottom` exceeds the
/// baseline by more than 1 logical pixel, `bottom` itself as the keyboard's overlap from the
/// window's bottom edge, else 0. Returns the new record, the safe bottom and the keyboard bottom.
/// A keyboard that is already up when the size changes makes the baseline too big until it hides,
/// then the smaller reading heals it.
#[cfg(any(target_os = "android", test))]
pub(crate) fn split_bottom(
    prev: Option<BottomRecord>,
    size: (u32, u32),
    bottom: f32,
) -> (BottomRecord, f32, f32) {
    let mut record = match prev {
        Some(p) if p.size == size => p,
        _ => BottomRecord {
            size,
            baseline: bottom,
        },
    };
    record.baseline = record.baseline.min(bottom);
    let keyboard = if bottom > record.baseline + 1.0 {
        bottom
    } else {
        0.0
    };
    (record, record.baseline, keyboard)
}

/// How far the keyboard's top edge reaches into a view of height `view_height`, in points: the
/// view's height less the keyboard's top (`keyboard_top`, in the view's own space), clamped to
/// `0..=view_height`. A value that is not finite gives 0.
#[cfg(any(target_os = "ios", test))]
pub(crate) fn keyboard_overlap(view_height: f64, keyboard_top: f64) -> f32 {
    let overlap = view_height - keyboard_top;
    if !overlap.is_finite() || !view_height.is_finite() {
        return 0.0;
    }
    overlap.clamp(0.0, view_height.max(0.0)) as f32
}

#[cfg(test)]
mod tests {
    use lune_ui_core::layout::Insets;

    use super::{
        BottomRecord, insets_from_content_rect, insets_from_geometry, keyboard_overlap,
        split_bottom,
    };

    #[test]
    fn a_keyboard_in_the_lower_part_of_the_view_overlaps_it_by_its_height() {
        // 874 points high view, keyboard top at 538 (the iPhone 16 Pro's portrait keyboard).
        assert_eq!(keyboard_overlap(874.0, 538.0), 336.0);
    }

    #[test]
    fn a_keyboard_below_the_view_or_above_its_top_is_clamped() {
        assert_eq!(keyboard_overlap(874.0, 874.0), 0.0);
        assert_eq!(keyboard_overlap(874.0, 900.0), 0.0);
        assert_eq!(keyboard_overlap(874.0, -50.0), 874.0);
    }

    #[test]
    fn a_keyboard_frame_that_is_not_finite_gives_zero() {
        assert_eq!(keyboard_overlap(874.0, f64::NAN), 0.0);
        assert_eq!(keyboard_overlap(874.0, f64::INFINITY), 0.0);
        assert_eq!(keyboard_overlap(874.0, f64::NEG_INFINITY), 0.0);
        assert_eq!(keyboard_overlap(f64::NAN, 538.0), 0.0);
    }

    #[test]
    fn the_ios_portrait_reading_gives_zero_62_zero_34() {
        // iOS 18.0, iPhone 16 Pro (docs/mobile.md, M1 reading): inner 1206x2334 at 0,186, outer
        // 1206x2622 at 0,0, scale 3.
        let got = insets_from_geometry((0, 186), (0, 0), (1206, 2334), (1206, 2622), 3.0);
        assert_eq!(got, Insets::new(0.0, 62.0, 0.0, 34.0));
    }

    #[test]
    fn a_landscape_reading_has_left_and_right_and_no_top() {
        // A made-up landscape geometry (59 points each side), not a reading.
        let got = insets_from_geometry((177, 0), (0, 0), (2268, 1206), (2622, 1206), 3.0);
        assert_eq!(got, Insets::new(59.0, 0.0, 59.0, 0.0));
    }

    #[test]
    fn a_negative_value_is_clamped_to_zero() {
        let got = insets_from_geometry((-3, -3), (0, 0), (110, 110), (100, 100), 1.0);
        assert_eq!(got, Insets::ZERO);
    }

    #[test]
    fn a_scale_that_is_not_positive_and_finite_gives_zero() {
        for scale in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            let got = insets_from_geometry((0, 186), (0, 0), (1206, 2334), (1206, 2622), scale);
            assert_eq!(got, Insets::ZERO, "scale {scale}");
        }
    }

    #[test]
    fn the_android_emulator_reading_gives_the_insets_computed_by_hand() {
        // M8 step 0, reading 4: content rect 0,283,1080,2337 in a 1080x2400 window at scale 2.625.
        let got = insets_from_content_rect((0, 283, 1080, 2337), (1080, 2400), 2.625);
        assert_eq!(got, Insets::new(0.0, 283.0 / 2.625, 0.0, 63.0 / 2.625));
    }

    #[test]
    fn a_landscape_shaped_content_rect_has_left_and_right() {
        // A made-up landscape rect (a cutout of 100 px on the left, 50 on the right), not a reading.
        let got = insets_from_content_rect((100, 0, 2350, 1080), (2400, 1080), 2.0);
        assert_eq!(got, Insets::new(50.0, 0.0, 25.0, 0.0));
    }

    #[test]
    fn a_degenerate_content_rect_gives_zero() {
        let rect = (0, 283, 1080, 2337);
        for scale in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                insets_from_content_rect(rect, (1080, 2400), scale),
                Insets::ZERO
            );
        }
        assert_eq!(insets_from_content_rect(rect, (0, 2400), 2.0), Insets::ZERO);
        assert_eq!(insets_from_content_rect(rect, (1080, 0), 2.0), Insets::ZERO);
        // Reversed: left past right, top past bottom.
        assert_eq!(
            insets_from_content_rect((600, 0, 500, 2400), (1080, 2400), 2.0),
            Insets::ZERO
        );
        assert_eq!(
            insets_from_content_rect((0, 900, 1080, 800), (1080, 2400), 2.0),
            Insets::ZERO
        );
    }

    #[test]
    fn a_content_rect_outside_the_window_clamps_each_side_to_zero() {
        let got = insets_from_content_rect((-10, -20, 1100, 2500), (1080, 2400), 1.0);
        assert_eq!(got, Insets::ZERO);
    }

    const PORTRAIT: (u32, u32) = (1080, 2400);

    #[test]
    fn at_rest_the_bottom_is_the_baseline_and_the_keyboard_is_zero() {
        let (rec, safe, kb) = split_bottom(None, PORTRAIT, 24.0);
        assert_eq!((safe, kb), (24.0, 0.0));
        assert_eq!(rec.baseline, 24.0);
    }

    #[test]
    fn a_keyboard_coming_up_reports_the_current_bottom_and_keeps_the_baseline() {
        let (rec, _, _) = split_bottom(None, PORTRAIT, 24.0);
        // The step-0 reading: the bottom went 63 px to 883 px (24 to 336.4 logical).
        let (rec, safe, kb) = split_bottom(Some(rec), PORTRAIT, 883.0 / 2.625);
        assert_eq!(safe, 24.0);
        assert_eq!(kb, 883.0 / 2.625);
        assert_eq!(rec.baseline, 24.0);
    }

    #[test]
    fn a_keyboard_going_down_gives_zero_again() {
        let (rec, _, _) = split_bottom(None, PORTRAIT, 24.0);
        let (rec, _, _) = split_bottom(Some(rec), PORTRAIT, 336.0);
        let (_, safe, kb) = split_bottom(Some(rec), PORTRAIT, 24.0);
        assert_eq!((safe, kb), (24.0, 0.0));
    }

    #[test]
    fn a_change_of_one_pixel_or_less_over_the_baseline_is_not_a_keyboard() {
        let (rec, _, _) = split_bottom(None, PORTRAIT, 24.0);
        let (_, _, kb) = split_bottom(Some(rec), PORTRAIT, 25.0);
        assert_eq!(kb, 0.0);
        let (_, _, kb) = split_bottom(Some(rec), PORTRAIT, 25.5);
        assert_eq!(kb, 25.5);
    }

    #[test]
    fn a_new_window_size_starts_a_new_baseline() {
        let (rec, _, _) = split_bottom(None, PORTRAIT, 24.0);
        // Landscape: the bottom bar is thinner, and it is the baseline there, not a lower reading
        // of the portrait one.
        let landscape = (2400, 1080);
        let (rec, safe, kb) = split_bottom(Some(rec), landscape, 12.0);
        assert_eq!((safe, kb), (12.0, 0.0));
        assert_eq!(
            rec,
            BottomRecord {
                size: landscape,
                baseline: 12.0
            }
        );
        // Back to portrait: the baseline starts again from the first reading.
        let (_, safe, _) = split_bottom(Some(rec), PORTRAIT, 24.0);
        assert_eq!(safe, 24.0);
    }

    #[test]
    fn a_keyboard_already_up_at_a_new_size_makes_the_baseline_too_big_until_it_hides() {
        // The known limit (D8): the first reading at the new size is the keyboard's, so the
        // baseline is the keyboard's bottom and no keyboard is reported; the first smaller reading
        // (the keyboard hiding) heals it.
        let (rec, _, _) = split_bottom(None, PORTRAIT, 24.0);
        let landscape = (2400, 1080);
        let (rec, safe, kb) = split_bottom(Some(rec), landscape, 200.0);
        assert_eq!((safe, kb), (200.0, 0.0));
        let (_, safe, kb) = split_bottom(Some(rec), landscape, 12.0);
        assert_eq!((safe, kb), (12.0, 0.0));
    }
}
