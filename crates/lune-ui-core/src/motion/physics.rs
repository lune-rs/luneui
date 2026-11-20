//! Physics for a finger's motion (D347): a fling's decay, a damped spring for the bounce back from an
//! overscroll, and the rubber band that maps an unbounded pull to a bounded distance. Each is a
//! closed-form function of the app's [`Time`] and stored parameters, like everything in `motion`:
//! nothing integrates per frame, so the same motion sampled at 60 Hz and at 120 Hz agrees at every
//! shared instant, and no function reads a clock. Values are `f32`; the arithmetic inside is `f64`.

use std::time::Duration;

use super::Time;

/// The rubber band's resistance at the start of a pull: the slope of [`rubber_band`] at 0 (iOS's
/// 0.55).
pub const RUBBER_BAND: f32 = 0.55;

/// The largest value [`rubber_band`] and [`rubber_band_inverse`] give for an extent: the float just
/// below it.
fn below(extent: f32) -> f32 {
    f32::from_bits(extent.to_bits() - 1)
}

/// The distance a pull of `overshoot` px past an end shows as, for a viewport `extent` px long:
/// `sign(x) (1 - 1 / (|x| c / d + 1)) d` with `c` = [`RUBBER_BAND`] and `d` = `extent`. It is 0 at 0,
/// odd, monotonic, has slope [`RUBBER_BAND`] at 0, and stays below `extent`. An `extent` that is not
/// positive and finite, or a non-finite `overshoot`, gives 0.
pub fn rubber_band(overshoot: f32, extent: f32) -> f32 {
    if !(extent.is_finite() && extent > 0.0 && overshoot.is_finite()) {
        return 0.0;
    }
    let d = f64::from(extent);
    let x = f64::from(overshoot.abs()) * f64::from(RUBBER_BAND) / d;
    let shown = ((1.0 - 1.0 / (x + 1.0)) * d) as f32;
    shown.min(below(extent)).copysign(overshoot)
}

/// The pull that shows as `shown` px: the inverse of [`rubber_band`] on `(-extent, extent)`, with
/// `shown` held just inside that range. An `extent` that is not positive and finite, or a
/// non-finite `shown`, gives 0.
pub fn rubber_band_inverse(shown: f32, extent: f32) -> f32 {
    if !(extent.is_finite() && extent > 0.0 && shown.is_finite()) {
        return 0.0;
    }
    let d = f64::from(extent);
    let s = f64::from(shown.abs().min(below(extent))) / d;
    let x = s / (1.0 - s) * d / f64::from(RUBBER_BAND);
    (x as f32).copysign(shown)
}

/// `secs` as a [`Time`], zero for anything not a finite non-negative number.
fn secs(secs: f64) -> Time {
    if secs.is_finite() && secs > 0.0 {
        Duration::try_from_secs_f64(secs).unwrap_or(Duration::MAX)
    } else {
        Duration::ZERO
    }
}

/// The first time from which `(alpha + beta t) e^{-lambda t}` stays at or under `limit`, in seconds
/// (0 when it never exceeds it; infinite when `lambda` is not positive and it does). The bound is
/// unimodal: it grows to its peak, then only falls, so the crossing is found by bisection after it.
fn fall(alpha: f64, beta: f64, lambda: f64, limit: f64) -> f64 {
    let f = |t: f64| (alpha + beta * t) * (-lambda * t).exp();
    if !(lambda > 0.0 && lambda.is_finite()) {
        return if alpha > limit { f64::INFINITY } else { 0.0 };
    }
    let peak = if beta > 0.0 {
        (1.0 / lambda - alpha / beta).max(0.0)
    } else {
        0.0
    };
    if f(peak) <= limit {
        return 0.0;
    }
    if beta == 0.0 {
        return (alpha / limit).ln() / lambda;
    }
    let (mut lo, mut hi) = (peak, peak + 1.0 / lambda);
    while f(hi) > limit && hi < 1e6 {
        hi *= 2.0;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if f(mid) > limit {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// A fling's exponential decay: the velocity falls by the same fraction every millisecond, so the
/// distance covered is a closed form of the time since the release. `rate` is the fraction of the
/// velocity kept per millisecond. A `rate` outside (0, 1), or not finite, ends at once (duration 0,
/// offset 0, velocity 0).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Decay {
    /// The fraction of the velocity kept per millisecond.
    pub rate: f32,
}

impl Decay {
    /// A fling that coasts: 0.998 per millisecond (a list that runs a few screens from a hard flick).
    pub const NORMAL: Decay = Decay { rate: 0.998 };
    /// A fling that stops sooner: 0.99 per millisecond.
    pub const FAST: Decay = Decay { rate: 0.99 };
    /// The speed, in px/s, under which a fling is at rest.
    pub const REST: f32 = 5.0;

    /// A decay of `rate` per millisecond.
    pub const fn new(rate: f32) -> Self {
        Decay { rate }
    }

    /// `k`, the decay constant in 1/s (`1000 ln rate`, negative), or `None` when the rate ends the
    /// fling at once.
    fn k(&self) -> Option<f64> {
        let r = f64::from(self.rate);
        (r.is_finite() && r > 0.0 && r < 1.0).then(|| 1000.0 * r.ln())
    }

    /// The time until the speed is [`Decay::REST`]: zero when `v0` (px/s) is already that slow.
    pub fn duration(&self, v0: f32) -> Time {
        let (Some(k), true) = (self.k(), v0.is_finite()) else {
            return Time::ZERO;
        };
        let v = f64::from(v0.abs());
        if v <= f64::from(Self::REST) {
            return Time::ZERO;
        }
        secs((f64::from(Self::REST) / v).ln() / k)
    }

    /// The distance covered `t` after a release at `v0` px/s: `v0 (e^{k t} - 1) / k`. From
    /// [`Decay::duration`] on it is the distance at that time exactly.
    pub fn offset(&self, v0: f32, t: Time) -> f32 {
        let Some(k) = self.k() else {
            return 0.0;
        };
        if !v0.is_finite() {
            return 0.0;
        }
        let t = t.min(self.duration(v0)).as_secs_f64();
        (f64::from(v0) * (k * t).exp_m1() / k) as f32
    }

    /// The velocity `t` after a release at `v0`: `v0 e^{k t}`, and 0 from [`Decay::duration`] on.
    pub fn velocity(&self, v0: f32, t: Time) -> f32 {
        let Some(k) = self.k() else {
            return 0.0;
        };
        if !v0.is_finite() || t >= self.duration(v0) {
            return 0.0;
        }
        (f64::from(v0) * (k * t.as_secs_f64()).exp()) as f32
    }

    /// When the offset first reaches `distance` (the sign of `v0`), or `None` when the fling
    /// ends before it, `distance` has the other sign, or the decay is invalid. A `distance` of 0
    /// is reached at once.
    pub fn time_to(&self, v0: f32, distance: f32) -> Option<Time> {
        let k = self.k()?;
        if !(v0.is_finite() && distance.is_finite()) {
            return None;
        }
        if distance == 0.0 {
            return Some(Time::ZERO);
        }
        if v0 == 0.0 || (v0 < 0.0) != (distance < 0.0) {
            return None;
        }
        // e^{k t} = 1 + d k / v0, which has a solution while the argument stays positive.
        let arg = 1.0 + f64::from(distance) * k / f64::from(v0);
        if arg <= 0.0 {
            return None;
        }
        let t = secs(arg.ln() / k);
        (t <= self.duration(v0)).then_some(t)
    }
}

/// A damped spring from one value to another, for the bounce back from an overscroll: a
/// second-order system of natural period `response` and damping ratio `damping`, evaluated in
/// closed form (under, critically and over damped). Unlike [`Spring`](super::Spring), it takes the
/// damping and so can overshoot. `response` zero or `damping` not positive and finite makes it hold
/// its target: [`DampedSpring::sample`] gives `(to, 0.0)` at every time.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct DampedSpring {
    /// The undamped period: the natural angular rate is `2 pi / response`.
    pub response: Time,
    /// The damping ratio: under 1 overshoots, 1 is critical, over 1 creeps.
    pub damping: f32,
}

/// The shape of one run, per damping regime: `d(t) = to - value` and its derivative in closed form.
enum Run {
    /// `e^{-a t} (A cos w t + B sin w t)`.
    Under { a: f64, w: f64, ca: f64, cb: f64 },
    /// `e^{-w t} (d0 + c t)`.
    Critical { w: f64, d0: f64, c: f64 },
    /// `C1 e^{r1 t} + C2 e^{r2 t}`, `r1 > r2`.
    Over { r1: f64, r2: f64, c1: f64, c2: f64 },
}

impl DampedSpring {
    /// The scroll area's bounce back from an overscroll: 0.4 s, critically damped.
    pub const BOUNCE: DampedSpring = DampedSpring {
        response: Duration::from_millis(400),
        damping: 1.0,
    };
    /// The distance, in px, within which a spring is at rest.
    pub const REST: f32 = 0.5;
    /// The speed, in px/s, under which a spring is at rest.
    pub const REST_VELOCITY: f32 = 5.0;
    /// The longest [`DampedSpring::duration`] gives, in seconds (a damping of 1000 would otherwise
    /// take minutes).
    const LONGEST: f64 = 60.0;

    /// A spring of `response` and `damping`.
    pub const fn new(response: Time, damping: f32) -> Self {
        DampedSpring { response, damping }
    }

    /// The natural angular rate and the ratio, or `None` for a spring that holds its target.
    fn params(&self) -> Option<(f64, f64)> {
        let r = self.response.as_secs_f64();
        let z = f64::from(self.damping);
        (r > 0.0 && z.is_finite() && z > 0.0).then(|| (std::f64::consts::TAU / r, z))
    }

    fn run(&self, from: f32, to: f32, v0: f32) -> Option<Run> {
        let (w0, z) = self.params()?;
        let (d0, v0) = (f64::from(from) - f64::from(to), f64::from(v0));
        Some(if (z - 1.0).abs() <= 1e-4 {
            Run::Critical {
                w: w0,
                d0,
                c: v0 + w0 * d0,
            }
        } else if z < 1.0 {
            let a = z * w0;
            let w = w0 * (1.0 - z * z).sqrt();
            Run::Under {
                a,
                w,
                ca: d0,
                cb: (v0 + a * d0) / w,
            }
        } else {
            let s = w0 * (z * z - 1.0).sqrt();
            let (r1, r2) = (-z * w0 + s, -z * w0 - s);
            let c1 = (v0 - r2 * d0) / (r1 - r2);
            Run::Over {
                r1,
                r2,
                c1,
                c2: d0 - c1,
            }
        })
    }

    /// `(d, d')` of a run at `t` seconds.
    fn at(run: &Run, t: f64) -> (f64, f64) {
        match *run {
            Run::Under { a, w, ca, cb } => {
                let e = (-a * t).exp();
                let (s, c) = (w * t).sin_cos();
                // d' = e^{-at} (v0 cos - ((a v0 + w0^2 d0) / w) sin), with v0 = w cb - a ca.
                let v0 = w * cb - a * ca;
                (
                    e * (ca * c + cb * s),
                    e * (v0 * c - ((a * v0 + (a * a + w * w) * ca) / w) * s),
                )
            }
            Run::Critical { w, d0, c } => {
                let e = (-w * t).exp();
                ((d0 + c * t) * e, (c - w * (d0 + c * t)) * e)
            }
            Run::Over { r1, r2, c1, c2 } => {
                let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
                (c1 * e1 + c2 * e2, r1 * c1 * e1 + r2 * c2 * e2)
            }
        }
    }

    /// The value and the velocity `t` after the start, moving from `from` to `to` with the speed
    /// `v0` (px/s) at the start. Both are exactly `(to, 0.0)` from [`DampedSpring::duration`] on,
    /// and at every time for a spring that holds its target.
    pub fn sample(&self, from: f32, to: f32, v0: f32, t: Time) -> (f32, f32) {
        if !(from.is_finite() && v0.is_finite()) {
            return (to, 0.0);
        }
        let Some(run) = self.run(from, to, v0) else {
            return (to, 0.0);
        };
        if t >= self.duration(from, to, v0) {
            return (to, 0.0);
        }
        let (d, v) = Self::at(&run, t.as_secs_f64());
        ((f64::from(to) + d) as f32, v as f32)
    }

    /// The time after which the motion stays within [`DampedSpring::REST`] of `to` and slower than
    /// [`DampedSpring::REST_VELOCITY`], from the parameters alone: the first time an upper bound
    /// of the motion's envelope falls under both limits (so it is never early, and late by a small
    /// fraction), at most 60 s.
    pub fn duration(&self, from: f32, to: f32, v0: f32) -> Time {
        if !(from.is_finite() && v0.is_finite()) {
            return Time::ZERO;
        }
        let Some(run) = self.run(from, to, v0) else {
            return Time::ZERO;
        };
        let (pos, vel) = (f64::from(Self::REST), f64::from(Self::REST_VELOCITY));
        // Each bound is `(alpha + beta t) e^{-lambda t}`: the time it falls under its limit for good.
        let secs_ = match run {
            Run::Under { a, w, ca, cb } => {
                let v0 = w * cb - a * ca;
                let rd = ca.hypot(cb);
                let rv = v0.hypot((a * v0 + (a * a + w * w) * ca) / w);
                fall(rd, 0.0, a, pos).max(fall(rv, 0.0, a, vel))
            }
            Run::Over { r1, r2, c1, c2 } => {
                let rd = c1.abs() + c2.abs();
                let rv = (r1 * c1).abs() + (r2 * c2).abs();
                fall(rd, 0.0, -r1, pos).max(fall(rv, 0.0, -r1, vel))
            }
            Run::Critical { w, d0, c } => {
                let v0 = c - w * d0;
                fall(d0.abs(), c.abs(), w, pos).max(fall(v0.abs(), w * c.abs(), w, vel))
            }
        };
        secs(secs_.clamp(0.0, Self::LONGEST))
    }
}
