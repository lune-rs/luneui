//! The motion core the touch-first components share (mobile M6, D369). A [`Track`] is one
//! position on one axis that follows a finger 1:1 (with a rubber band past its ends), releases into
//! a critically damped spring that carries the finger's velocity to a picked stop, and moves under
//! the frame's motion tier when the app moves it. [`Ui::pan_region`] registers the region a
//! component reads its touch pan through, arbitrated per axis as a scroll area is (D349).
//!
//! Crate-private: the components of M6 use it, no app does. Every motion here is a closed form of
//! the app's time, so a frame at 60 Hz and one at 120 Hz agree at every shared instant (D347).

// Steps 2 to 7 of M6 are the consumers; until each lands, part of this module has none.
#![allow(dead_code)]

use std::time::Duration;

use crate::motion::{
    DampedSpring, Decay, Resolved, Time, Transition, rubber_band, rubber_band_inverse,
};
use crate::paint::Rect;
use crate::ui::{PanReading, Sense, Ui, WidgetId};

/// The spring every release settles with: critically damped (no bounce), natural period 0.3 s, so a
/// 300 px settle from rest takes about 0.4 s. The finger's velocity is its start speed.
pub(crate) const SETTLE: DampedSpring = DampedSpring::new(Duration::from_millis(300), 1.0);

/// The speed, in px/s, above which a release is a fling that moves at least one stop.
pub(crate) const FLING: f32 = 300.0;

/// The range a [`Track`] moves in: `lo` to `hi` is where the content may rest; past it the finger's
/// pull shows through [`rubber_band`] with the viewport `extent` (px) as its limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bounds {
    pub(crate) lo: f32,
    pub(crate) hi: f32,
    pub(crate) extent: f32,
}

impl Bounds {
    /// The range `lo..=hi` (swapped when given the wrong way round) and its viewport `extent`.
    pub(crate) fn new(lo: f32, hi: f32, extent: f32) -> Self {
        Bounds {
            lo: lo.min(hi),
            hi: lo.max(hi),
            extent,
        }
    }

    /// What a raw (unbounded) position shows as: itself inside the range, the rubber band past it.
    fn shown(&self, raw: f32) -> f32 {
        if raw < self.lo {
            self.lo + rubber_band(raw - self.lo, self.extent)
        } else if raw > self.hi {
            self.hi + rubber_band(raw - self.hi, self.extent)
        } else {
            raw
        }
    }

    /// The raw position that shows as `shown`: the inverse of [`Bounds::shown`].
    fn raw(&self, shown: f32) -> f32 {
        if shown < self.lo {
            self.lo + rubber_band_inverse(shown - self.lo, self.extent)
        } else if shown > self.hi {
            self.hi + rubber_band_inverse(shown - self.hi, self.extent)
        } else {
            shown
        }
    }
}

/// What a [`Track`] is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// At rest on a position.
    Rest(f32),
    /// Held by a finger at this raw position (shown through the bounds' rubber band).
    Follow(f32),
    /// A release: a [`SETTLE`] spring from `from` to `to` with the speed `v0` px/s, started at `start`.
    Spring {
        from: f32,
        to: f32,
        v0: f32,
        start: Time,
    },
    /// A programmatic move under a resolved motion.
    Tween(Transition<f32>),
}

/// One position on one axis, in logical px: follows a finger, releases to a stop, moves by the app.
/// The shown position is continuous across every method that changes how it moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Track {
    state: State,
}

impl Track {
    /// At rest on `at`.
    pub(crate) fn new(at: f32) -> Self {
        Track {
            state: State::Rest(at),
        }
    }

    /// The shown position at `now` and whether the track is still moving by itself (a release or a
    /// programmatic move in flight; a held finger is not, the finger's events repaint).
    pub(crate) fn at(&self, now: Time, lo: f32, hi: f32, extent: f32) -> (f32, bool) {
        self.shown(now, Bounds::new(lo, hi, extent))
    }

    fn shown(&self, now: Time, b: Bounds) -> (f32, bool) {
        match self.state {
            State::Rest(p) => (p, false),
            State::Follow(raw) => (b.shown(raw), false),
            State::Spring {
                from,
                to,
                v0,
                start,
            } => {
                let t = now.saturating_sub(start);
                let (p, _) = SETTLE.sample(from, to, v0, t);
                (p, t < SETTLE.duration(from, to, v0))
            }
            State::Tween(tr) => tr.sample(now),
        }
    }

    /// A finger lands: any glide stops where it shows (no jump), and the track is held.
    pub(crate) fn grab(&mut self, now: Time, b: Bounds) {
        if matches!(self.state, State::Follow(_)) {
            return;
        }
        let (p, _) = self.shown(now, b);
        self.state = State::Follow(b.raw(p));
    }

    /// A finger lands on a glide and does not move yet: it stops where it shows and rests there
    /// (no jump). A held track stays held.
    pub(crate) fn halt(&mut self, now: Time, b: Bounds) {
        if matches!(self.state, State::Follow(_)) {
            return;
        }
        let (p, _) = self.shown(now, b);
        self.state = State::Rest(p);
    }

    /// The finger moves by `d` px: the track follows 1:1, showing the rubber band past the ends.
    pub(crate) fn follow(&mut self, now: Time, d: f32, b: Bounds) {
        self.grab(now, b);
        if let State::Follow(raw) = &mut self.state {
            *raw += d;
        }
    }

    /// The finger lifts at speed `v` px/s: picks one of `stops` (positions) and springs to it from
    /// the shown position with `v` as the start speed. Returns the index picked.
    ///
    /// The pick is the stop nearest to where a fling of `v` would come to rest
    /// ([`Decay::NORMAL`]), among those at most `max_step` indices from `current` (all of them
    /// when `None`). A release faster than [`FLING`] that would stay on `current` moves to the
    /// nearest stop beyond it in `v`'s direction, when the step allows one. With no stops it does
    /// nothing and returns 0.
    pub(crate) fn release(
        &mut self,
        now: Time,
        v: f32,
        stops: &[f32],
        current: usize,
        max_step: Option<usize>,
        b: Bounds,
    ) -> usize {
        if stops.is_empty() {
            return 0;
        }
        let (p, _) = self.shown(now, b);
        let v = if v.is_finite() { v } else { 0.0 };
        let current = current.min(stops.len() - 1);
        let reach = Decay::NORMAL.offset(v, Decay::NORMAL.duration(v));
        let target = p + reach;
        let window = |i: &usize| max_step.is_none_or(|m| i.abs_diff(current) <= m);
        let nearest = |to: f32, only: &dyn Fn(usize) -> bool| {
            (0..stops.len())
                .filter(window)
                .filter(|&i| only(i))
                .min_by(|&a, &c| (stops[a] - to).abs().total_cmp(&(stops[c] - to).abs()))
        };
        let mut pick = nearest(target, &|_| true).unwrap_or(current);
        if v.abs() > FLING && pick == current {
            let here = stops[current];
            let beyond = |i: usize| {
                if v > 0.0 {
                    stops[i] > here
                } else {
                    stops[i] < here
                }
            };
            if let Some(i) = nearest(here, &beyond) {
                pick = i;
            }
        }
        self.state = State::Spring {
            from: p,
            to: stops[pick],
            v0: v,
            start: now,
        };
        pick
    }

    /// The app moves the track to `target`: a tween under `resolved` from where it shows, or a snap
    /// when the motion resolves to nothing (`None`, the tier says so).
    pub(crate) fn go_to(&mut self, now: Time, target: f32, resolved: Option<Resolved>, b: Bounds) {
        let (p, _) = self.shown(now, b);
        self.state = match resolved {
            Some(r) if p != target => {
                let mut t = Transition::settled(p);
                t.retarget(now, target, r.duration, r.easing);
                State::Tween(t)
            }
            _ => State::Rest(target),
        };
    }

    /// Whether the track is at rest on `stop` at `now`: not held, not moving, and its target.
    pub(crate) fn settled_at(&self, now: Time, stop: f32) -> bool {
        match self.state {
            State::Rest(p) => p == stop,
            State::Follow(_) => false,
            State::Spring {
                from,
                to,
                v0,
                start,
            } => to == stop && now.saturating_sub(start) >= SETTLE.duration(from, to, v0),
            State::Tween(t) => t.target() == stop && !t.in_flight(now),
        }
    }
}

impl Ui<'_> {
    /// Registers `rect` as a region a touch pan moves on `axes` (x, y) and returns this frame's
    /// reading of the pan (D349). It senses scroll only, and the wheel never routes to it
    /// (D178), so a mouse press, a wheel delta and a key pass it by to the widgets and areas
    /// around it; `moving` is whether the component's own motion is in flight, so a touch press
    /// catches it. Register it before the content it sits under, as a scroll area does: a pan
    /// that starts on a button inside it goes to the region along an axis the button does not
    /// claim.
    pub(crate) fn pan_region(
        &mut self,
        id: WidgetId,
        rect: Rect,
        axes: (bool, bool),
        moving: bool,
    ) -> PanReading {
        let mark = self.registration_mark();
        self.interact(id, rect, Sense::SCROLL);
        self.set_scroll_axes((false, false));
        self.set_pan_axes(axes);
        self.set_scroll_moving(mark, moving);
        self.pan(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::PointerButton;
    use crate::motion::{Easing, conformance_of_at};
    use crate::testing::UiHarness;

    fn ms(n: u64) -> Time {
        Duration::from_millis(n)
    }

    /// 0 to 600 px with a 400 px viewport.
    const B: Bounds = Bounds {
        lo: 0.0,
        hi: 600.0,
        extent: 400.0,
    };

    fn at(t: &Track, now: Time) -> f32 {
        t.at(now, B.lo, B.hi, B.extent).0
    }

    #[test]
    fn following_inside_the_range_is_one_to_one_and_past_it_is_the_rubber_band() {
        let mut t = Track::new(100.0);
        t.follow(ms(0), 50.0, B);
        assert_eq!(at(&t, ms(0)), 150.0);
        t.follow(ms(16), -300.0, B);
        // 100 + 50 - 300 = -150 raw: 150 past the low end.
        assert_eq!(at(&t, ms(16)), rubber_band(-150.0, 400.0));
        assert!(at(&t, ms(16)) < 0.0 && at(&t, ms(16)) > -150.0);
        t.follow(ms(32), 900.0, B);
        // raw 750: 150 past the high end.
        assert_eq!(at(&t, ms(32)), 600.0 + rubber_band(150.0, 400.0));
        let (_, moving) = t.at(ms(32), B.lo, B.hi, B.extent);
        assert!(!moving, "a held finger is not the track's own motion");
    }

    #[test]
    fn a_release_at_rest_picks_the_nearest_stop() {
        let stops = [0.0, 200.0, 400.0];
        for (from, want) in [(10.0, 0), (130.0, 1), (170.0, 1), (390.0, 2)] {
            let mut t = Track::new(from);
            t.grab(ms(0), B);
            let got = t.release(ms(0), 0.0, &stops, 0, None, B);
            assert_eq!(got, want, "from {from}");
            let end = ms(2000);
            assert_eq!(at(&t, end), stops[want]);
            assert!(t.settled_at(end, stops[want]));
        }
    }

    #[test]
    fn a_fast_fling_with_a_step_of_one_moves_exactly_one_stop() {
        let stops = [0.0, 200.0, 400.0, 600.0];
        let mut t = Track::new(200.0);
        t.follow(ms(0), 10.0, B);
        // A hard flick left: its coast would pass every stop, but the step is one.
        assert_eq!(t.release(ms(1), -6000.0, &stops, 1, Some(1), B), 0);
        let mut t = Track::new(200.0);
        t.follow(ms(0), 10.0, B);
        assert_eq!(t.release(ms(1), 6000.0, &stops, 1, Some(1), B), 2);
        // A short flick that would stay on the current stop still moves one in its direction.
        let mut t = Track::new(200.0);
        t.follow(ms(0), 10.0, B);
        assert_eq!(t.release(ms(1), 400.0, &stops, 1, Some(1), B), 2);
        // No stop beyond: it stays.
        let mut t = Track::new(0.0);
        t.follow(ms(0), 5.0, B);
        assert_eq!(t.release(ms(1), -3000.0, &stops, 0, Some(1), B), 0);
    }

    #[test]
    fn a_slow_release_near_the_middle_picks_the_nearer_stop() {
        let stops = [0.0, 200.0];
        // Under 300 px/s the projection decides: a coast of v / 2 px (rate 0.998 per ms).
        for (along, v, want) in [(90.0, 10.0, 0), (90.0, 100.0, 1), (110.0, -10.0, 1)] {
            let mut t = Track::new(0.0);
            t.follow(ms(0), along, B);
            assert_eq!(
                t.release(ms(1), v, &stops, 0, Some(1), B),
                want,
                "{along} px along at {v} px/s"
            );
        }
        // Past the middle the nearer stop is the far one, even moving back a little.
        let mut t = Track::new(0.0);
        t.follow(ms(0), 120.0, B);
        assert_eq!(t.release(ms(1), -20.0, &stops, 0, Some(1), B), 1);
    }

    #[test]
    fn a_grab_in_the_middle_of_a_spring_holds_the_shown_position() {
        let stops = [0.0, 300.0];
        let mut t = Track::new(0.0);
        t.follow(ms(0), 250.0, B);
        t.release(ms(10), 900.0, &stops, 0, None, B);
        for at_ms in [10u64, 60, 150, 260] {
            let mut g = t;
            let before = g.at(ms(at_ms), B.lo, B.hi, B.extent).0;
            g.grab(ms(at_ms), B);
            let after = g.at(ms(at_ms), B.lo, B.hi, B.extent).0;
            assert!((before - after).abs() < 1e-3, "{at_ms}: {before} {after}");
            // And it stays there while held, with the clock moving on.
            assert!((g.at(ms(at_ms + 500), B.lo, B.hi, B.extent).0 - after).abs() < 1e-3);
        }
        // Grabbed while rubber-banded past an end: the same position, the same raw pull.
        let mut r = Track::new(0.0);
        r.follow(ms(0), -120.0, B);
        let shown = at(&r, ms(0));
        r.release(ms(1), 0.0, &stops, 0, None, B);
        let mid = r.at(ms(80), B.lo, B.hi, B.extent).0;
        r.grab(ms(80), B);
        assert!((r.at(ms(80), B.lo, B.hi, B.extent).0 - mid).abs() < 1e-3);
        assert!(shown < mid || mid <= 0.0);
    }

    #[test]
    fn the_shown_position_is_continuous_across_every_change() {
        let stops = [0.0, 300.0];
        let mut t = Track::new(40.0);
        let hold = |t: &Track, now| t.at(now, B.lo, B.hi, B.extent).0;
        let p0 = hold(&t, ms(0));
        t.grab(ms(0), B);
        assert!((hold(&t, ms(0)) - p0).abs() < 1e-3);
        t.follow(ms(5), 0.0, B);
        assert!((hold(&t, ms(5)) - p0).abs() < 1e-3);
        t.follow(ms(10), 60.0, B);
        let p1 = hold(&t, ms(10));
        t.release(ms(10), 500.0, &stops, 0, None, B);
        assert!((hold(&t, ms(10)) - p1).abs() < 1e-3, "release");
        let p2 = hold(&t, ms(70));
        t.go_to(ms(70), 0.0, Some(resolved(200)), B);
        assert!((hold(&t, ms(70)) - p2).abs() < 1e-3, "go_to");
    }

    #[test]
    fn a_halt_holds_the_shown_position_with_no_follow() {
        let stops = [0.0, 300.0];
        let mut t = Track::new(0.0);
        t.release(ms(0), 900.0, &stops, 0, None, B);
        let mid = at(&t, ms(80));
        t.halt(ms(80), B);
        assert!((at(&t, ms(80)) - mid).abs() < 1e-3);
        assert!((at(&t, ms(900)) - mid).abs() < 1e-3, "it stays");
        assert!(!t.at(ms(900), B.lo, B.hi, B.extent).1);
    }

    fn resolved(duration_ms: u64) -> Resolved {
        Resolved {
            duration: ms(duration_ms),
            easing: Easing::CUBIC_OUT,
            transforms: true,
        }
    }

    #[test]
    fn go_to_with_nothing_resolved_snaps() {
        let mut t = Track::new(0.0);
        t.go_to(ms(7), 300.0, None, B);
        assert_eq!(t.at(ms(7), B.lo, B.hi, B.extent), (300.0, false));
        assert!(t.settled_at(ms(7), 300.0));
    }

    #[test]
    fn go_to_with_a_resolved_motion_ends_exactly_on_the_target() {
        let mut t = Track::new(0.0);
        t.go_to(ms(100), 300.0, Some(resolved(250)), B);
        let (mid, moving) = t.at(ms(200), B.lo, B.hi, B.extent);
        assert!(moving && mid > 0.0 && mid < 300.0, "{mid}");
        assert!(!t.settled_at(ms(200), 300.0));
        assert_eq!(t.at(ms(350), B.lo, B.hi, B.extent), (300.0, false));
        assert!(t.settled_at(ms(350), 300.0));
    }

    #[test]
    fn a_300_px_settle_takes_between_0_3_and_0_5_s() {
        let d = SETTLE.duration(300.0, 0.0, 0.0).as_secs_f64();
        assert!((0.3..=0.5).contains(&d), "{d}");
        // It never overshoots: critically damped.
        let (mut worst, mut i) = (f32::MAX, 0);
        while i <= 500 {
            worst = worst.min(SETTLE.sample(300.0, 0.0, 0.0, ms(i)).0);
            i += 5;
        }
        assert!(worst >= 0.0, "{worst}");
    }

    /// A release from 120 px with a flick toward stop 0 passes the 60 and 120 Hz conformance
    /// harness (the same row `tests/mobile_motion.rs` runs on the harness's own known case).
    #[test]
    fn a_release_conforms_at_60_and_120() {
        let from = 120.0f32;
        let v = -1500.0f32;
        let stops = [0.0, 300.0];
        let end = SETTLE.duration(from, 0.0, v);
        let vmax = (0..=4000)
            .map(|i| {
                SETTLE
                    .sample(
                        from,
                        0.0,
                        v,
                        Duration::from_secs_f64(end.as_secs_f64() * f64::from(i) / 4000.0),
                    )
                    .1
                    .abs()
            })
            .fold(0.0f32, f32::max);
        assert!(vmax >= v.abs() - 1.0, "the instrument: {vmax}");
        for rate in [60u32, 120] {
            let mut t = Track::new(from);
            t.grab(Time::ZERO, B);
            assert_eq!(t.release(Time::ZERO, v, &stops, 1, None, B), 0);
            conformance_of_at(
                rate,
                |now| t.at(now, B.lo, B.hi, B.extent),
                end,
                0.0f32,
                vmax / rate as f32 + 0.5,
            )
            .unwrap_or_else(|e| panic!("release at {rate}: {e}"));
        }
    }

    // ---- the pan region ------------------------------------------------------------------

    const REGION: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);
    const BUTTON: Rect = Rect::new(10.0, 10.0, 100.0, 30.0);

    /// One frame: the region, then a button over it. Returns the pan reading and whether the
    /// button clicked.
    fn frame(h: &mut UiHarness) -> (PanReading, bool) {
        let mut out = None;
        h.frame(|ui| {
            let (r, b) = (ui.id("region"), ui.id("button"));
            let pan = ui.pan_region(r, REGION, (true, false), false);
            let clicked = ui.interact(b, BUTTON, Sense::CLICK_FOCUS).clicked;
            out = Some((pan, clicked));
        });
        out.expect("the frame ran")
    }

    #[test]
    fn a_mouse_press_never_lands_on_a_pan_region() {
        let mut h = UiHarness::new((400.0, 300.0));
        frame(&mut h);
        h.input.pointer_to(20.0, 20.0).press(PointerButton::Primary);
        frame(&mut h);
        h.input.release(PointerButton::Primary);
        let (pan, clicked) = frame(&mut h);
        assert!(clicked, "the button under the mouse clicks");
        assert_eq!(pan, PanReading::default());
        // A press where only the region is: nothing pans, with or without travel.
        let mut h = UiHarness::new((400.0, 300.0));
        frame(&mut h);
        h.input
            .pointer_to(300.0, 200.0)
            .press(PointerButton::Primary);
        let (pan, clicked) = frame(&mut h);
        h.input.pointer_to(340.0, 200.0);
        let (moved, _) = frame(&mut h);
        assert!(!clicked && pan == PanReading::default() && moved == PanReading::default());
    }

    #[test]
    fn a_touch_pan_along_an_axis_the_region_takes_reads_through_it() {
        let mut h = UiHarness::new((400.0, 300.0));
        frame(&mut h);
        h.at_ms(0).input.touch_start(1, 20.0, 20.0);
        frame(&mut h);
        h.at_ms(16).input.touch_move(1, 50.0, 20.0);
        let (pan, _) = frame(&mut h);
        assert!(pan.started && pan.active && !pan.ended);
        assert!(
            (pan.delta.0 - 20.0).abs() < 1e-3 && pan.delta.1 == 0.0,
            "30 px of travel less the 10 px slop: {:?}",
            pan.delta
        );
        h.at_ms(32).input.touch_end(1, 50.0, 20.0);
        let (pan, clicked) = frame(&mut h);
        assert!(pan.ended && !pan.active && !clicked);
        // A vertical drag is not the region's axis: it does not pan.
        let mut h = UiHarness::new((400.0, 300.0));
        frame(&mut h);
        h.at_ms(0).input.touch_start(1, 20.0, 100.0);
        frame(&mut h);
        h.at_ms(16).input.touch_move(1, 20.0, 160.0);
        assert_eq!(frame(&mut h).0, PanReading::default());
    }
}
