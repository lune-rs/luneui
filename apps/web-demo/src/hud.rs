//! The frame-time HUD: frames per second, the median and the 99th percentile of the last 120
//! frame times, a strip of those times and the GPU backend. `H` or `?hud` shows it; it is a reading
//! of the app's own frames (the gaps between the times the shell hands the frame), so it measures
//! what a person sees, not what the GPU spent.
//!
//! The times are recorded whether it is shown or not (the console line of the shell reads them), and
//! a frame that came more than a quarter of a second after the one before is not a frame time: the
//! app was idle or in a hidden tab. Every colour comes from the theme.

use std::collections::VecDeque;
use std::time::Duration;

use lune_ui::color::with_alpha;
use lune_ui::paint::Rect;
use lune_ui::scene::LayerId;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;

use crate::stage::Layout;

/// How many frame times the readings cover.
pub const WINDOW: usize = 120;
/// A gap between two frames above this is idle time, not a frame time.
pub const IDLE_GAP: Duration = Duration::from_millis(250);

/// The panel's size in logical px.
const PANEL: (f32, f32) = (204.0, 78.0);
/// The strip's bar width, and the tallest bar's frame time (a bar is clipped there).
const BAR: f32 = 1.5;
const STRIP_MAX_MS: f32 = 50.0;
/// The strip's height.
const STRIP_H: f32 = 22.0;
/// A frame time at or under this is on time (a 60 Hz frame and a margin), over `LATE` it is a
/// missed frame, in between it is slow.
const ON_TIME_MS: f32 = 20.0;
const LATE_MS: f32 = 34.0;

/// What the last [`WINDOW`] frames read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    /// Frames per second: the mean of the window, inverted.
    pub fps: f32,
    /// The median frame time, ms.
    pub p50: f32,
    /// The 99th percentile frame time, ms (the nearest rank).
    pub p99: f32,
    /// The frames recorded since the start, idle gaps not counted.
    pub frames: u64,
}

/// The ring of frame times and whether the panel is up.
#[derive(Debug, Default)]
pub struct Hud {
    /// The panel is drawn (`H`, `?hud`).
    pub shown: bool,
    times: VecDeque<f32>,
    last: Option<Duration>,
    frames: u64,
}

/// The `p` quantile (0 to 1) of `sorted`, ascending, by the nearest rank.
fn nearest_rank(sorted: &[f32], p: f32) -> f32 {
    let n = sorted.len();
    let rank = ((p * n as f32).ceil() as usize).clamp(1, n);
    sorted[rank - 1]
}

impl Hud {
    /// Notes a frame at app time `now`: the time since the last one joins the window, unless it is an
    /// idle gap or time went back.
    pub fn record(&mut self, now: Duration) {
        if let Some(last) = self.last
            && now > last
            && now - last <= IDLE_GAP
        {
            if self.times.len() == WINDOW {
                self.times.pop_front();
            }
            self.times.push_back((now - last).as_secs_f32() * 1000.0);
            self.frames += 1;
        }
        self.last = Some(now);
    }

    /// Shows the panel if it is hidden, hides it if it is shown.
    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }

    /// The readings, or `None` before the first frame time.
    pub fn stats(&self) -> Option<Stats> {
        if self.times.is_empty() {
            return None;
        }
        let mut sorted: Vec<f32> = self.times.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let mean = sorted.iter().sum::<f32>() / sorted.len() as f32;
        Some(Stats {
            fps: if mean > 0.0 { 1000.0 / mean } else { 0.0 },
            p50: nearest_rank(&sorted, 0.5),
            p99: nearest_rank(&sorted, 0.99),
            frames: self.frames,
        })
    }

    /// The panel's rect in a window laid out as `l`: the top right corner; on a narrow window under
    /// the title bar.
    pub fn rect(l: &Layout) -> Rect {
        let (w, h) = PANEL;
        if l.wide {
            Rect::new(l.size.0 - w - 24.0, 24.0, w, h)
        } else {
            Rect::new(l.size.0 - w - 8.0, l.title_bar.bottom() + 8.0, w, h)
        }
    }

    /// Draws the panel on the toast layer when it is shown. It asks for no frame and takes every
    /// colour from the theme.
    pub fn draw(&self, ui: &mut Ui<'_>, l: &Layout, backend: &str) {
        if !self.shown {
            return;
        }
        let t = *ui.theme();
        let r = Self::rect(l);
        let stats = self.stats();
        let p = ui.painter();
        p.on(LayerId::Toast);
        p.shadow(r, 10.0, 14.0, with_alpha(t.dialog_backdrop, 0.45));
        p.fill(r, 10.0, with_alpha(t.menu_glass, 0.94));
        p.ring(r, 10.0, 1.0, t.menu_popup_border, false);
        let pad = 10.0;
        let headline = match stats {
            Some(s) => format!("{:.0} fps", s.fps),
            None => "- fps".to_owned(),
        };
        p.text(
            &Spec::plain(headline, Style::ui(t.font_base, t.fg_bright).medium()),
            r.x + pad,
            r.y + 7.0,
        );
        let spec = Spec::plain(backend, Style::ui(t.font_s, t.fg_secondary));
        let bw = p.measure(&spec).0;
        p.text(&spec, r.right() - pad - bw, r.y + 9.0);
        let line = match stats {
            Some(s) => format!("p50 {:.1} ms   p99 {:.1} ms", s.p50, s.p99),
            None => "p50 -   p99 -".to_owned(),
        };
        p.text(
            &Spec::plain(
                line,
                Style::mono(t.font_s, (t.font_s * 1.3).round(), t.fg_primary),
            ),
            r.x + pad,
            r.y + 30.0,
        );
        // The strip: one bar per frame in the window, oldest first, on a hairline.
        let strip = Rect::new(
            r.x + pad,
            r.bottom() - pad - STRIP_H,
            r.w - 2.0 * pad,
            STRIP_H,
        );
        p.fill(
            Rect::new(strip.x, strip.bottom(), strip.w, 1.0),
            0.0,
            t.surface_separator,
        );
        // The 20 ms line (a 60 Hz frame and a margin).
        let limit = strip.bottom() - STRIP_H * (ON_TIME_MS / STRIP_MAX_MS);
        p.fill(
            Rect::new(strip.x, limit, strip.w, 1.0),
            0.0,
            with_alpha(t.fg_tertiary, 0.35),
        );
        for (i, ms) in self.times.iter().enumerate() {
            let h = (ms.min(STRIP_MAX_MS) / STRIP_MAX_MS * STRIP_H).max(1.0);
            let colour = if *ms <= ON_TIME_MS {
                t.ok
            } else if *ms <= LATE_MS {
                t.warn
            } else {
                t.err
            };
            p.fill(
                Rect::new(strip.x + i as f32 * BAR, strip.bottom() - h, BAR - 0.5, h),
                0.0,
                colour,
            );
        }
        p.on(LayerId::Page);
    }
}
