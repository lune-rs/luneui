//! The sparkline (`data.sparkline`, D436): a history graph with no axes, in an inset rounded
//! frame. A series is drawn as a line with a soft fill under it, or as a strip of bars; a second
//! series can be mirrored below a baseline through the middle.
//!
//! Samples are oldest first and the newest sits at the right edge. The graph has `capacity` slots
//! (`0`: as many as the longer series has, at most [`MAX_CAPACITY`]); fewer samples fill the right
//! slots and leave the left empty, more show only the newest ones. Both series share one scale.
//!
//! Nothing here animates: a new sample redraws the graph at once (a history moves by the app's
//! samples). There is no input, no tab stop and no hit region.

use std::hash::Hash;

use crate::access::{Node, Role};
use crate::color::{Rgba, with_alpha};
use crate::paint::{Rect, StrokeStyle};
use crate::ui::Ui;

/// The most slots a graph has. A series' fill is one polygon of at most this many points plus two,
/// and the painter fills no polygon of more than 4096 points (ear clipping is quadratic in them).
pub const MAX_CAPACITY: usize = 1024;

/// The distance from the frame's edge to the graph.
const INSET: f32 = 4.0;
/// The frame's corner radius.
const RADIUS: f32 = 4.0;
/// The line's width.
const LINE: f32 = 1.0;
/// The alpha factor of the fill under a line.
const FILL_ALPHA: f32 = 0.25;
/// The gap between two bars' cells, in logical px.
const BAR_GAP: f32 = 1.0;
/// A bar cell's corner radius, at most half its width.
const BAR_RADIUS: f32 = 2.0;

/// How a series is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SparkStyle {
    /// A 1 px line through the slot centres with a soft fill between it and the baseline.
    #[default]
    Line,
    /// One rounded cell per slot, filled from the baseline up to the sample.
    Bars,
}

/// What [`Ui::sparkline`] is given besides its key and rect.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct SparklineOptions<'a> {
    /// The main series, oldest first, growing up from the baseline.
    pub up: &'a [f32],
    /// Its colour.
    pub up_color: Rgba,
    /// The mirrored series, oldest first, growing down from the middle; empty draws one series.
    pub down: &'a [f32],
    /// Its colour.
    pub down_color: Rgba,
    /// The sample at the top of the graph; not finite or not above 0 uses the largest sample in
    /// view of both series.
    pub max: f32,
    /// The slot count; `0` is the longer series' length. At most [`MAX_CAPACITY`].
    pub capacity: usize,
    /// Line or bars.
    pub style: SparkStyle,
    /// The accessible name; empty falls to `luneui.sparkline.name`.
    pub label: &'a str,
    /// The accessible description (the data as text).
    pub description: &'a str,
}

impl<'a> SparklineOptions<'a> {
    /// A graph of `up` in `up_color` named `label`: one series, scaled to its largest sample, as
    /// many slots as samples, a line.
    #[must_use]
    pub const fn new(up: &'a [f32], up_color: Rgba, label: &'a str) -> Self {
        Self {
            up,
            up_color,
            down: &[],
            down_color: up_color,
            max: 0.0,
            capacity: 0,
            style: SparkStyle::Line,
            label,
            description: "",
        }
    }

    /// These options with `down` mirrored below the baseline in `down_color`.
    #[must_use]
    pub const fn with_mirror(mut self, down: &'a [f32], down_color: Rgba) -> Self {
        self.down = down;
        self.down_color = down_color;
        self
    }

    /// These options scaled so that `max` is the top of the graph.
    #[must_use]
    pub const fn with_max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    /// These options with `capacity` slots.
    #[must_use]
    pub const fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// These options drawn in `style`.
    #[must_use]
    pub const fn with_style(mut self, style: SparkStyle) -> Self {
        self.style = style;
        self
    }

    /// These options with the accessible `description`.
    #[must_use]
    pub const fn with_description(mut self, description: &'a str) -> Self {
        self.description = description;
        self
    }
}

/// Each of `samples` as a fraction of the graph's height, 0 to 1. `max` finite and above 0 is the
/// top; otherwise the largest finite sample is. A sample not finite or below 0 is 0, one above the
/// top is 1, and with no top above 0 (no samples, or all 0) every sample is 0.
pub fn scale(samples: &[f32], max: f32) -> Vec<f32> {
    let usable = |v: f32| v.is_finite() && v > 0.0;
    let top = if usable(max) {
        max
    } else {
        samples
            .iter()
            .copied()
            .filter(|&v| usable(v))
            .fold(0.0, f32::max)
    };
    samples
        .iter()
        .map(|&v| {
            if top > 0.0 && usable(v) {
                (v / top).min(1.0)
            } else {
                0.0
            }
        })
        .collect()
}

/// The newest `slots` samples of `series`.
fn newest(series: &[f32], slots: usize) -> &[f32] {
    &series[series.len().saturating_sub(slots)..]
}

impl Ui<'_> {
    /// A sparkline under `key` filling `rect`. The node is an `Image` named `opts.label` (or the
    /// toolkit's `luneui.sparkline.name`) with the description.
    pub fn sparkline(&mut self, key: impl Hash, rect: Rect, opts: &SparklineOptions<'_>) {
        let id = self.id(key);
        let name = if opts.label.is_empty() {
            self.tr("luneui.sparkline.name")
        } else {
            opts.label
        };
        let mut node = Node::new(id, Role::Image, name, rect);
        if !opts.description.is_empty() {
            node.description = Some(opts.description.to_owned());
        }
        self.node(node);

        let t = *self.theme();
        let graph = Rect::new(
            rect.x + INSET,
            rect.y + INSET,
            rect.w - 2.0 * INSET,
            rect.h - 2.0 * INSET,
        );
        let sized = rect.w.is_finite() && rect.h.is_finite() && graph.w > 0.0 && graph.h > 0.0;
        if !sized {
            return;
        }

        let longer = opts.up.len().max(opts.down.len());
        let slots = if opts.capacity == 0 {
            longer
        } else {
            opts.capacity
        }
        .min(MAX_CAPACITY);
        let (up, down) = (newest(opts.up, slots), newest(opts.down, slots));

        // One top for both series: the given one, else the largest sample in view of either.
        let top = if opts.max.is_finite() && opts.max > 0.0 {
            opts.max
        } else {
            let usable = |v: &f32| v.is_finite() && *v > 0.0;
            up.iter()
                .chain(down)
                .copied()
                .filter(usable)
                .fold(0.0, f32::max)
        };
        let (up, down) = (scale(up, top), scale(down, top));

        let mirrored = !opts.down.is_empty();
        let base = if mirrored {
            graph.y + graph.h / 2.0
        } else {
            graph.y + graph.h
        };
        let reach = if mirrored { graph.h / 2.0 } else { graph.h };
        let pitch = if slots > 0 {
            graph.w / slots as f32
        } else {
            0.0
        };
        let centre = |slot: usize| graph.x + (slot as f32 + 0.5) * pitch;

        let p = self.painter();
        p.fill(rect, RADIUS, t.surface_input);
        if opts.style == SparkStyle::Bars && slots > 0 {
            let w = (pitch - BAR_GAP).max(1.0);
            let radius = BAR_RADIUS.min(w / 2.0);
            for slot in 0..slots {
                let x = graph.x + slot as f32 * pitch + (pitch - w) / 2.0;
                p.fill(Rect::new(x, graph.y, w, graph.h), radius, t.surface_overlay);
            }
            for (series, color, sign) in [(&up, opts.up_color, -1.0), (&down, opts.down_color, 1.0)]
            {
                let first = slots - series.len();
                for (i, &v) in series.iter().enumerate() {
                    let h = v * reach;
                    if h <= 0.0 {
                        continue;
                    }
                    let x = graph.x + (first + i) as f32 * pitch + (pitch - w) / 2.0;
                    let y = if sign < 0.0 { base - h } else { base };
                    p.fill(Rect::new(x, y, w, h), radius.min(h / 2.0), color);
                }
            }
        }
        p.fill(
            Rect::new(graph.x, base - LINE / 2.0, graph.w, LINE),
            0.0,
            t.border,
        );
        if opts.style == SparkStyle::Line && slots > 0 {
            for (series, color, sign) in [(&up, opts.up_color, -1.0), (&down, opts.down_color, 1.0)]
            {
                let first = slots - series.len();
                let line: Vec<[f32; 2]> = series
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| [centre(first + i), base + sign * v * reach])
                    .collect();
                if line.len() < 2 || series.iter().all(|&v| v <= 0.0) {
                    continue;
                }
                let mut area = line.clone();
                area.push([line[line.len() - 1][0], base]);
                area.push([line[0][0], base]);
                p.fill_path(&area, with_alpha(color, FILL_ALPHA));
                p.stroke(&line, false, StrokeStyle::solid(color, LINE));
            }
        }
    }
}
