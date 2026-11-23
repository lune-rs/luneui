//! The ring gauge (`data.ring-gauge`, D435): a circular track with coloured arc segments laid end
//! to end, clockwise from the top, and an optional text centred inside.
//!
//! The ring is the largest circle that fits the rect the app gives, centred in it; its outer edge
//! touches the square's sides. Each segment's `value` is its fraction of the whole ring, so the
//! segments need not total 1: what is not covered shows the track. A running total past 1 is cut
//! at 1. The app formats the centred text (the toolkit knows no unit).
//!
//! Each segment's value tweens with `Kind::Value`, as a pie slice (D7 of the task), and `live`
//! snaps. The node is a `Progress` whose value is the covered fraction. There is no input, no tab
//! stop and no hit region.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::hash::Hash;

use crate::access::{Node, Role, Value};
use crate::color::Rgba;
use crate::motion::{Kind, Motion};
use crate::paint::{Rect, StrokeStyle};
use crate::text::{Spec, Style};
use crate::ui::Ui;

/// The value change: the `value` kind (`fast`, quad-out; Reduced and Off snap).
pub const VALUE: Motion = Motion::of(Kind::Value);

/// The default ring thickness, in logical px.
pub const THICKNESS: f32 = 8.0;

/// The centred text's size is this share of the ring's diameter, and never below the theme's
/// base size.
const TEXT_SHARE: f32 = 0.24;

/// One segment: the app's key (what the segment's tween is kept under), its share of the ring and
/// its colour.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct RingSegment {
    /// What the segment's tween is kept under.
    pub key: u64,
    /// Its fraction of the whole ring, 0 to 1. Not finite or below 0 draws nothing.
    pub value: f32,
    /// Its colour.
    pub color: Rgba,
}

impl RingSegment {
    /// A segment of `value` (a fraction of the ring) in `color`, tweened under `key`.
    #[must_use]
    pub const fn new(key: u64, value: f32, color: Rgba) -> Self {
        Self { key, value, color }
    }
}

/// What [`Ui::ring_gauge`] is given besides its key and rect.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct RingGaugeOptions<'a> {
    /// The segments, drawn end to end clockwise from the top.
    pub segments: &'a [RingSegment],
    /// The centred text (the app's formatted value); empty draws none.
    pub text: &'a str,
    /// The ring's thickness in logical px; not finite or not above 0 uses [`THICKNESS`]; at most
    /// half the diameter.
    pub thickness: f32,
    /// The accessible name; empty falls to `luneui.ring-gauge.name`.
    pub label: &'a str,
    /// Continuously updating data: a change snaps.
    pub live: bool,
}

impl<'a> RingGaugeOptions<'a> {
    /// Options for `segments` named `label`: no text, the default thickness, tweened.
    #[must_use]
    pub const fn new(segments: &'a [RingSegment], label: &'a str) -> Self {
        Self {
            segments,
            text: "",
            thickness: THICKNESS,
            label,
            live: false,
        }
    }

    /// These options with the centred `text`.
    #[must_use]
    pub const fn with_text(mut self, text: &'a str) -> Self {
        self.text = text;
        self
    }

    /// These options with the ring `thickness` in logical px.
    #[must_use]
    pub const fn with_thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }

    /// These options with changes snapping (`true`) or tweening.
    #[must_use]
    pub const fn with_live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }
}

/// Each of `values` (fractions of the ring) as the `(start, sweep)` of its arc in radians: the
/// first starts at the top (−π/2) and each next starts where the one before ended, turning
/// clockwise. A value not finite or below 0 sweeps 0; the running total is capped at 1, so the
/// segment that crosses it is cut and the ones after it sweep 0.
pub fn arcs(values: &[f32]) -> Vec<(f32, f32)> {
    let mut covered = 0.0_f32;
    values
        .iter()
        .map(|&v| {
            let start = -FRAC_PI_2 + covered * TAU;
            let v = if v.is_finite() && v > 0.0 { v } else { 0.0 };
            let take = v.min(1.0 - covered).max(0.0);
            covered += take;
            (start, take * TAU)
        })
        .collect()
}

impl Ui<'_> {
    /// A ring gauge under `key`: the largest circle in `rect`, centred. The node is a `Progress`
    /// named `opts.label` (or the toolkit's `luneui.ring-gauge.name`), its value the covered
    /// fraction, its description `opts.text` when that is not empty.
    pub fn ring_gauge(&mut self, key: impl Hash, rect: Rect, opts: &RingGaugeOptions<'_>) {
        let id = self.id(key);
        let diameter = rect.w.min(rect.h);
        let drawn = diameter.is_finite() && diameter > 0.0;
        let d = if drawn { diameter } else { 0.0 };
        let square = Rect::new(
            rect.x + (rect.w - d) / 2.0,
            rect.y + (rect.h - d) / 2.0,
            d,
            d,
        );

        let name = if opts.label.is_empty() {
            self.tr("luneui.ring-gauge.name")
        } else {
            opts.label
        };
        let target: f32 = opts
            .segments
            .iter()
            .filter(|s| s.value.is_finite() && s.value > 0.0)
            .map(|s| s.value)
            .sum::<f32>()
            .min(1.0);
        let mut node = Node::new(id, Role::Progress, name, square);
        node.value = Some(Value::Number {
            value: f64::from(target),
            min: 0.0,
            max: 1.0,
            step: 0.0,
        });
        if !opts.text.is_empty() {
            node.description = Some(opts.text.to_owned());
        }
        self.node(node);
        if !drawn {
            return;
        }

        let t = *self.theme();
        let thickness = if opts.thickness.is_finite() && opts.thickness > 0.0 {
            opts.thickness
        } else {
            THICKNESS
        }
        .min(d / 2.0);
        let radius = (d - thickness) / 2.0;
        let centre = [square.x + d / 2.0, square.y + d / 2.0];

        let values: Vec<f32> = opts
            .segments
            .iter()
            .map(|s| {
                let aid = id.with(s.key);
                let v = if s.value.is_finite() { s.value } else { 0.0 };
                if opts.live {
                    self.snap_f32(aid, v)
                } else {
                    self.animate_f32(aid, v, &VALUE)
                }
            })
            .collect();
        let sweeps = arcs(&values);

        let inner = d - 2.0 * thickness;
        let text = (!opts.text.is_empty() && inner > 0.0).then(|| {
            let size = (d * TEXT_SHARE).max(t.font_base);
            let spec = Spec::plain(opts.text, Style::ui(size, t.fg_primary));
            self.painter().elide(&spec, inner)
        });

        let p = self.painter();
        p.arc(
            centre,
            radius,
            -FRAC_PI_2,
            TAU,
            StrokeStyle::solid(t.surface_overlay, thickness),
        );
        for (segment, (start, sweep)) in opts.segments.iter().zip(sweeps) {
            if sweep > 0.0 {
                p.arc(
                    centre,
                    radius,
                    start,
                    sweep,
                    StrokeStyle::solid(segment.color, thickness),
                );
            }
        }
        if let Some(spec) = text {
            let side = Rect::new(
                centre[0] - inner / 2.0,
                centre[1] - inner / 2.0,
                inner,
                inner,
            );
            p.text_centered(&spec, side);
        }
    }
}
