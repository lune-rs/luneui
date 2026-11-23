//! The battery (`data.battery`, D437): a rounded outline with a small nub on its right end, a fill
//! inside it to the level and, while charging, a bolt over the fill.
//!
//! The body keeps the rect's height and the nub takes a share of its width on the right, centred
//! vertically. The fill is `level` of the inner width, in `ok`, `warn` or `err` by the level (the
//! app can name a colour instead). The level is a reading, as a progress bar's: it never animates.
//! The node is a `Progress` from 0 to 100; there is no input and no tab stop.

use std::hash::Hash;

use crate::access::{Node, Role, Value};
use crate::color::Rgba;
use crate::paint::Rect;
use crate::theme::Theme;
use crate::ui::Ui;

/// The nub's share of the rect's width.
const NUB_SHARE: f32 = 0.08;
/// The nub's most and least width, in logical px.
const NUB_MAX_W: f32 = 4.0;
const NUB_MIN_W: f32 = 2.0;
/// The nub's share of the rect's height.
const NUB_HEIGHT_SHARE: f32 = 0.4;
/// The body's corner radius as a share of its height.
const BODY_RADIUS_SHARE: f32 = 0.28;
/// The outline's width.
const OUTLINE: f32 = 1.0;
/// The gap between the outline and the fill.
const FILL_INSET: f32 = 2.0;
/// The bolt's height as a share of the inner height, and its width as a share of its height.
const BOLT_HEIGHT_SHARE: f32 = 0.72;
const BOLT_ASPECT: f32 = 0.6;
/// The bolt's corners in a unit box (x across, y down): the top, the left of the waist, the
/// waist's inner corner, the bottom, the right of the waist and its inner corner.
const BOLT: [[f32; 2]; 6] = [
    [0.6, 0.0],
    [0.15, 0.56],
    [0.45, 0.56],
    [0.38, 1.0],
    [0.85, 0.4],
    [0.55, 0.4],
];
/// The level at or below which the fill is `warn`, and at or below which it is `err`.
const WARN_AT: f32 = 0.2;
const ERR_AT: f32 = 0.1;

/// What [`Ui::battery`] is given besides its key and rect.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct BatteryOptions<'a> {
    /// The charge, 0 to 1; not finite is 0 and past either end is cut to it.
    pub level: f32,
    /// Whether it is charging: a bolt over the fill and the `ok` colour.
    pub charging: bool,
    /// The fill's colour; `None` takes [`level_color`].
    pub color: Option<Rgba>,
    /// The accessible name; empty falls to `luneui.battery.name`.
    pub label: &'a str,
}

impl<'a> BatteryOptions<'a> {
    /// A battery at `level` named `label`: not charging, coloured by the level.
    #[must_use]
    pub const fn new(level: f32, label: &'a str) -> Self {
        Self {
            level,
            charging: false,
            color: None,
            label,
        }
    }

    /// These options charging or not.
    #[must_use]
    pub const fn with_charging(mut self, charging: bool) -> Self {
        self.charging = charging;
        self
    }

    /// These options with the fill in `color`.
    #[must_use]
    pub const fn with_color(mut self, color: Rgba) -> Self {
        self.color = Some(color);
        self
    }
}

/// `level` as a 0 to 1 value: not finite is 0.
fn clamp(level: f32) -> f32 {
    if level.is_finite() {
        level.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The fill colour for `level`: `ok` while charging or above 0.2, `warn` from 0.2 down to above
/// 0.1, `err` at 0.1 or below.
pub fn level_color(theme: &Theme, level: f32, charging: bool) -> Rgba {
    let level = clamp(level);
    if charging || level > WARN_AT {
        theme.ok
    } else if level > ERR_AT {
        theme.warn
    } else {
        theme.err
    }
}

impl Ui<'_> {
    /// A battery under `key` in `rect`. The node is a `Progress` named `opts.label` (or the
    /// toolkit's `luneui.battery.name`) with the level as a percentage, described as charging
    /// while it charges.
    pub fn battery(&mut self, key: impl Hash, rect: Rect, opts: &BatteryOptions<'_>) {
        let id = self.id(key);
        let level = clamp(opts.level);
        let name = if opts.label.is_empty() {
            self.tr("luneui.battery.name")
        } else {
            opts.label
        };
        let mut node = Node::new(id, Role::Progress, name, rect);
        node.value = Some(Value::Number {
            value: f64::from(level * 100.0),
            min: 0.0,
            max: 100.0,
            step: 1.0,
        });
        if opts.charging {
            node.description = Some(self.tr("luneui.battery.charging").to_owned());
        }
        self.node(node);

        let sized = rect.w.is_finite() && rect.h.is_finite() && rect.w > 0.0 && rect.h > 0.0;
        if !sized {
            return;
        }
        let t = *self.theme();
        let nub_w = (rect.w * NUB_SHARE)
            .clamp(NUB_MIN_W, NUB_MAX_W)
            .min(rect.w / 2.0);
        let body = Rect::new(rect.x, rect.y, rect.w - nub_w, rect.h);
        let nub_h = rect.h * NUB_HEIGHT_SHARE;
        let nub = Rect::new(
            body.x + body.w,
            rect.y + (rect.h - nub_h) / 2.0,
            nub_w,
            nub_h,
        );
        let radius = (body.h * BODY_RADIUS_SHARE).min(body.w / 2.0);
        let inner = Rect::new(
            body.x + FILL_INSET,
            body.y + FILL_INSET,
            body.w - 2.0 * FILL_INSET,
            body.h - 2.0 * FILL_INSET,
        );
        let color = opts
            .color
            .unwrap_or_else(|| level_color(&t, level, opts.charging));

        let p = self.painter();
        p.ring(body, radius, OUTLINE, t.fg_tertiary, false);
        p.fill(nub, (nub_w / 2.0).min(nub_h / 2.0), t.fg_tertiary);
        if inner.w <= 0.0 || inner.h <= 0.0 {
            return;
        }
        let fill = Rect::new(inner.x, inner.y, inner.w * level, inner.h);
        if level > 0.0 {
            let r = (radius - FILL_INSET)
                .max(0.0)
                .min(fill.w / 2.0)
                .min(fill.h / 2.0);
            p.fill(fill, r, color);
        }
        if opts.charging {
            let h = inner.h * BOLT_HEIGHT_SHARE;
            let w = h * BOLT_ASPECT;
            let (x, y) = (inner.x + (inner.w - w) / 2.0, inner.y + (inner.h - h) / 2.0);
            let pts: Vec<[f32; 2]> = BOLT.iter().map(|c| [x + c[0] * w, y + c[1] * h]).collect();
            p.fill_path(&pts, t.on_accent);
        }
    }
}
