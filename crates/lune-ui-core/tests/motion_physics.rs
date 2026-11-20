//! Physics for a finger's motion (D347): `Decay`, `DampedSpring`, `rubber_band`, and the conformance
//! harness at a rate.

use std::time::Duration;

use lune_ui_core::motion::*;

fn secs(s: f64) -> Time {
    Duration::from_secs_f64(s)
}

// ---- (a) Decay ----------------------------------------------------------------------------

#[test]
fn a_decay_covers_the_documented_distance() {
    let d = Decay::NORMAL;
    let k = 1000.0 * f64::from(d.rate).ln();
    for v0 in [1000.0f32, 3000.0, 8000.0, -3000.0] {
        let end = d.duration(v0);
        assert!(end > Time::ZERO, "{v0}");
        // The documented value at `duration`: v0 (e^{k t} - 1) / k.
        let want = f64::from(v0) * ((k * end.as_secs_f64()).exp() - 1.0) / k;
        assert!((f64::from(d.offset(v0, end)) - want).abs() < 1e-3, "{v0}");
        // The total is -v0 / k, less what the last 5 px/s would still cover.
        let total = -f64::from(v0) / k;
        let got = f64::from(d.offset(v0, end));
        assert!(
            (got - total).abs() <= total.abs() * 0.01,
            "{v0}: {got} vs {total}"
        );
        // At rest from `duration` on: exact, and no velocity.
        for extra in [0.0, 0.001, 1.0, 100.0] {
            let t = end + secs(extra);
            assert_eq!(d.offset(v0, t), d.offset(v0, end), "{v0} +{extra}");
            assert_eq!(d.velocity(v0, t), 0.0, "{v0} +{extra}");
        }
        assert_eq!(d.offset(v0, Time::ZERO), 0.0);
        assert_eq!(d.velocity(v0, Time::ZERO), v0);
    }
}

#[test]
fn a_decay_knows_when_it_reaches_a_distance() {
    let d = Decay::NORMAL;
    for v0 in [1000.0f32, 3000.0, 8000.0, -3000.0] {
        let total = d.offset(v0, d.duration(v0));
        for frac in [0.01f32, 0.25, 0.5, 0.9, 0.999] {
            let dist = total * frac;
            let t = d.time_to(v0, dist).unwrap_or_else(|| panic!("{v0} {frac}"));
            assert!((d.offset(v0, t) - dist).abs() < 0.01, "{v0} {frac}");
        }
        // Past the total, the other way, and at zero.
        assert_eq!(d.time_to(v0, total * 1.01), None, "{v0} beyond");
        assert_eq!(d.time_to(v0, -total * 0.5), None, "{v0} other way");
        assert_eq!(d.time_to(v0, 0.0), Some(Time::ZERO));
    }
}

#[test]
fn a_decay_that_cannot_run_ends_at_once() {
    for rate in [0.0, 1.0, -0.5, 1.5, f32::NAN, f32::INFINITY] {
        let d = Decay::new(rate);
        assert_eq!(d.duration(2000.0), Time::ZERO, "{rate}");
        assert_eq!(d.offset(2000.0, secs(0.5)), 0.0, "{rate}");
        assert_eq!(d.velocity(2000.0, secs(0.5)), 0.0, "{rate}");
        assert_eq!(d.time_to(2000.0, 10.0), None, "{rate}");
    }
    // A speed already at rest has no fling, and a faster decay stops sooner.
    assert_eq!(Decay::NORMAL.duration(Decay::REST), Time::ZERO);
    assert_eq!(Decay::NORMAL.offset(3.0, secs(1.0)), 0.0);
    assert!(Decay::FAST.duration(3000.0) < Decay::NORMAL.duration(3000.0));
    assert_eq!(Decay::NORMAL.offset(f32::NAN, secs(1.0)), 0.0);
}

// ---- (b) DampedSpring ---------------------------------------------------------------------

#[test]
fn a_damped_spring_starts_where_told_and_ends_exactly() {
    for damping in [0.5f32, 0.99995, 1.0, 1.00005, 2.0, 8.0] {
        let s = DampedSpring::new(Duration::from_millis(400), damping);
        for (from, to, v0) in [
            (80.0f32, 0.0f32, -1500.0f32),
            (0.0, 120.0, 0.0),
            (-30.0, 50.0, 900.0),
        ] {
            let (x, v) = s.sample(from, to, v0, Time::ZERO);
            assert!(
                (x - from).abs() < 1e-3,
                "{damping}: x(0) = {x}, want {from}"
            );
            assert!(
                (v - v0).abs() < 1e-2 * v0.abs().max(1.0),
                "{damping}: v(0) = {v}, want {v0}"
            );
            let end = s.duration(from, to, v0);
            assert!(end > Time::ZERO, "{damping}");
            assert!(end <= secs(60.0), "{damping}");
            assert_eq!(
                s.sample(from, to, v0, end),
                (to, 0.0),
                "{damping} at the end"
            );
            assert_eq!(
                s.sample(from, to, v0, end + secs(3.0)),
                (to, 0.0),
                "{damping} after"
            );
            // Just before the end the motion is within the rest band: no visible jump.
            let (x, v) = s.sample(from, to, v0, end.saturating_sub(Duration::from_micros(100)));
            assert!(
                (x - to).abs() <= DampedSpring::REST * 1.01,
                "{damping}: x = {x}"
            );
            assert!(
                v.abs() <= DampedSpring::REST_VELOCITY * 1.5 + 1.0,
                "{damping}: v = {v}"
            );
        }
    }
}

#[test]
fn a_damped_spring_velocity_is_the_derivative_of_its_value() {
    let h = 1e-3f64;
    for damping in [0.5f32, 1.0, 2.0] {
        let s = DampedSpring::new(Duration::from_millis(400), damping);
        let (from, to, v0) = (80.0f32, 0.0f32, -1500.0f32);
        let end = s.duration(from, to, v0).as_secs_f64();
        let mut checked = 0;
        let mut t = 0.01;
        while t < end - 0.05 {
            let (_, v) = s.sample(from, to, v0, secs(t));
            let (a, _) = s.sample(from, to, v0, secs(t - h));
            let (b, _) = s.sample(from, to, v0, secs(t + h));
            let numeric = f64::from(b - a) / (2.0 * h);
            // 1% of the speed, or of 100 px/s where the speed passes through zero (f32 values).
            let tol = 0.01 * f64::from(v).abs().max(100.0);
            assert!(
                (numeric - f64::from(v)).abs() <= tol,
                "{damping} t={t}: {numeric} vs {v}"
            );
            checked += 1;
            t += 0.013;
        }
        assert!(checked > 10, "{damping}: {checked} points");
    }
}

#[test]
fn an_underdamped_spring_overshoots_and_the_others_do_not() {
    let sample_all = |damping: f32| {
        let s = DampedSpring::new(Duration::from_millis(400), damping);
        let end = s.duration(100.0, 0.0, 0.0).as_secs_f64();
        (0..=400)
            .map(|i| {
                s.sample(100.0, 0.0, 0.0, secs(end * f64::from(i) / 400.0))
                    .0
            })
            .collect::<Vec<_>>()
    };
    assert!(sample_all(0.5).iter().any(|&x| x < -0.5), "0.5 overshoots");
    for damping in [1.0, 2.0] {
        assert!(
            sample_all(damping).iter().all(|&x| x >= -1e-3),
            "{damping} stays on its side"
        );
    }
}

#[test]
fn a_damped_spring_that_cannot_run_holds_its_target() {
    let ok = Duration::from_millis(400);
    for s in [
        DampedSpring::new(Time::ZERO, 1.0),
        DampedSpring::new(ok, 0.0),
        DampedSpring::new(ok, -1.0),
        DampedSpring::new(ok, f32::NAN),
        DampedSpring::new(ok, f32::INFINITY),
    ] {
        assert_eq!(s.sample(80.0, 5.0, 100.0, secs(0.1)), (5.0, 0.0), "{s:?}");
        assert_eq!(s.sample(80.0, 5.0, 100.0, Time::ZERO), (5.0, 0.0), "{s:?}");
        assert_eq!(s.duration(80.0, 5.0, 100.0), Time::ZERO, "{s:?}");
    }
    // Already at rest on its target: nothing to run.
    assert_eq!(DampedSpring::BOUNCE.duration(3.0, 3.0, 0.0), Time::ZERO);
    assert_eq!(
        DampedSpring::BOUNCE.sample(f32::NAN, 3.0, 0.0, secs(0.1)),
        (3.0, 0.0)
    );
}

#[test]
fn the_bounce_settles_in_a_believable_time() {
    // From a full pull with a flick the other way: well under 2 s, and not instant.
    for (from, v0) in [(80.0f32, -1500.0f32), (80.0, 0.0), (300.0, 800.0)] {
        let d = DampedSpring::BOUNCE.duration(from, 0.0, v0).as_secs_f64();
        assert!((0.2..2.0).contains(&d), "{from} {v0}: {d} s");
    }
    // A slower, springier one runs longer than a firm one.
    let long = DampedSpring::new(Duration::from_millis(400), 0.3).duration(80.0, 0.0, 0.0);
    assert!(long > DampedSpring::BOUNCE.duration(80.0, 0.0, 0.0));
}

// ---- (c) rubber_band ----------------------------------------------------------------------

#[test]
fn the_rubber_band_is_odd_monotonic_bounded_and_starts_at_its_slope() {
    let d = 800.0f32;
    assert_eq!(rubber_band(0.0, d), 0.0);
    let mut prev = 0.0f32;
    let mut x = 0.5f32;
    while x < 4.0 * d {
        let y = rubber_band(x, d);
        assert!(y > prev, "monotonic at {x}");
        assert!(y < d, "below the extent at {x}");
        assert_eq!(rubber_band(-x, d), -y, "odd at {x}");
        prev = y;
        x += 3.7;
    }
    assert!(rubber_band(1.0e9, d) < d);
    // The slope at 0 is RUBBER_BAND.
    let slope = rubber_band(0.01, d) / 0.01;
    assert!((slope - RUBBER_BAND).abs() < 1e-3, "{slope}");
    assert_eq!(RUBBER_BAND, 0.55);
    // Bad inputs show nothing.
    for (x, e) in [
        (10.0, 0.0),
        (10.0, -5.0),
        (f32::NAN, d),
        (f32::INFINITY, d),
        (10.0, f32::NAN),
    ] {
        assert_eq!(rubber_band(x, e), 0.0, "{x} {e}");
        assert_eq!(rubber_band_inverse(x, e), 0.0, "{x} {e}");
    }
}

#[test]
fn the_rubber_band_inverse_undoes_it() {
    for d in [100.0f32, 402.0, 874.0] {
        let mut x = -2.0 * d;
        while x <= 2.0 * d {
            let back = rubber_band_inverse(rubber_band(x, d), d);
            assert!((back - x).abs() < 0.01, "{d}: {x} -> {back}");
            x += d / 37.0;
        }
        // Held just inside the extent, finite and the same side.
        let near = rubber_band_inverse(d, d);
        assert!(near.is_finite() && near > 0.0);
        assert_eq!(rubber_band_inverse(-d, d), -near);
        assert_eq!(rubber_band_inverse(5.0 * d, d), near);
    }
}

// ---- (d) the 120 Hz sweep -----------------------------------------------------------------

#[test]
fn every_kind_and_tier_conforms_at_120_hz_as_at_60() {
    for kind in Kind::ALL {
        let m = Motion::of(kind);
        for tier in [MotionTier::Full, MotionTier::Reduced, MotionTier::Off] {
            let at60 = conformance_at(60, &m, &MOTION, tier, 0.0f32, 100.0)
                .unwrap_or_else(|e| panic!("{kind:?} {tier:?} at 60: {e}"));
            let at120 = conformance_at(120, &m, &MOTION, tier, 0.0f32, 100.0)
                .unwrap_or_else(|e| panic!("{kind:?} {tier:?} at 120: {e}"));
            let plain = conformance(&m, &MOTION, tier, 0.0f32, 100.0)
                .unwrap_or_else(|e| panic!("{kind:?} {tier:?}: {e}"));
            assert_eq!(
                at60, plain,
                "{kind:?} {tier:?}: the 60 Hz pair is the old harness"
            );
            // A finer grid takes smaller steps and settles within one frame of the same end.
            assert!(at120.max_step <= at60.max_step + 1e-3, "{kind:?} {tier:?}");
            assert!(
                at120.settled_at <= at60.settled_at + Duration::from_nanos(1_000_000_000 / 120),
                "{kind:?} {tier:?}"
            );
        }
    }
}

#[test]
fn the_harness_at_a_rate_still_catches_what_it_should() {
    // A jump of 50 on a 120 Hz grid, a rate of 0, and a late settle by one 120 Hz frame.
    let err = conformance_of_at(
        120,
        |t| (if t.is_zero() { 0.0f32 } else { 50.0 }, t.is_zero()),
        Duration::from_millis(1),
        50.0,
        10.0,
    )
    .expect_err("must fail");
    assert!(
        matches!(err, ConformanceError::Jump { frame: 1, .. }),
        "{err:?}"
    );
    let err =
        conformance_of_at(0, |_| (0.0f32, false), Time::ZERO, 0.0, 1.0).expect_err("must fail");
    assert!(
        matches!(err, ConformanceError::NotFinite { frame: 0, .. }),
        "{err:?}"
    );
    // The first sample at or after `end` must be settled: a 120 Hz grid settles within one 120 Hz
    // frame of `end` (not one 60 Hz frame), and a sampler still moving there is `End`.
    let end = Duration::from_millis(100);
    let frame = Duration::from_nanos(1_000_000_000 / 120);
    let run = |lag: Duration| {
        conformance_of_at(
            120,
            move |t| (if t < end + lag { 0.0f32 } else { 1.0 }, t < end + lag),
            end,
            1.0,
            2.0,
        )
    };
    let ok = run(Time::ZERO).expect("settles at its end");
    assert!(ok.settled_at >= end && ok.settled_at <= end + frame);
    assert!(matches!(run(frame * 2), Err(ConformanceError::End { .. })));
}

// ---- (e) physics under the harness --------------------------------------------------------

#[test]
fn a_fling_and_a_bounce_conform_at_60_and_120() {
    let decay = Decay::NORMAL;
    let v0 = 2000.0f32;
    let end = decay.duration(v0);
    let target = decay.offset(v0, end);
    for rate in [60u32, 120] {
        conformance_of_at(
            rate,
            |t| (decay.offset(v0, t), t < end),
            end,
            target,
            v0.abs() / rate as f32 + 0.5,
        )
        .unwrap_or_else(|e| panic!("fling at {rate}: {e}"));
    }

    let spring = DampedSpring::BOUNCE;
    let (from, to, sv0) = (80.0f32, 0.0f32, -1500.0f32);
    let end = spring.duration(from, to, sv0);
    // The fastest the spring moves, from a fine scan (the value is the instrument, checked: the
    // speed at 0 is `sv0`).
    let vmax = (0..=4000)
        .map(|i| {
            spring
                .sample(
                    from,
                    to,
                    sv0,
                    secs(end.as_secs_f64() * f64::from(i) / 4000.0),
                )
                .1
                .abs()
        })
        .fold(0.0f32, f32::max);
    assert!(vmax >= sv0.abs() - 1.0, "{vmax}");
    for rate in [60u32, 120] {
        conformance_of_at(
            rate,
            |t| (spring.sample(from, to, sv0, t).0, t < end),
            end,
            to,
            vmax / rate as f32 + 0.5,
        )
        .unwrap_or_else(|e| panic!("bounce at {rate}: {e}"));
    }
}

// ---- (f) rate independence ----------------------------------------------------------------

#[test]
fn physics_is_a_pure_function_of_time() {
    let decay = Decay::NORMAL;
    let spring = DampedSpring::BOUNCE;
    // Shared instants of the 60 and 120 Hz grids: every even frame of the faster one.
    for k in 0..600u64 {
        let t60 = Duration::from_nanos(k * 1_000_000_000 / 60);
        let t120 = Duration::from_nanos(2 * k * 1_000_000_000 / 120);
        assert_eq!(t60, t120, "the grids share frame {k}");
        assert_eq!(
            decay.offset(2000.0, t60).to_bits(),
            decay.offset(2000.0, t120).to_bits()
        );
        assert_eq!(
            decay.velocity(2000.0, t60).to_bits(),
            decay.velocity(2000.0, t120).to_bits()
        );
        let (a, av) = spring.sample(80.0, 0.0, -1500.0, t60);
        let (b, bv) = spring.sample(80.0, 0.0, -1500.0, t120);
        assert_eq!((a.to_bits(), av.to_bits()), (b.to_bits(), bv.to_bits()));
    }
}
