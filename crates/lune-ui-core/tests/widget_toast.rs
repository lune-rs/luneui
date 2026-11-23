//! Toasts and the notification history (`overlay.toast-notifications` acceptance): the model's
//! rules at injected time, the stack's geometry at scale 2, the four motions in three tiers, the
//! nodes, input over a dialog and a raised popover, the disabled rule and the history panel.

use std::time::Duration;

use lune_ui_core::access::{AccessAction, Action, Live, Role, State, Value};
use lune_ui_core::color::premul;
use lune_ui_core::input::{FrameOutput, Key, Modifiers, NamedKey, PointerButton, Repaint};
use lune_ui_core::motion::{FRAME, MotionTier, Time, conformance_of};
use lune_ui_core::paint::{LayerId, Painter, Rect};
use lune_ui_core::scene::Scene;
use lune_ui_core::shortcuts::{Binding, Chord, Scope};
use lune_ui_core::testing::{FakeText, UiHarness, disabled_violations};
use lune_ui_core::text::{Spec, Style, TextKey};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::{Ui, WidgetId};
use lune_ui_core::widgets::button::ButtonKind;
use lune_ui_core::widgets::dialog::{Cancel, Dialog, DialogGlass};
use lune_ui_core::widgets::field::FieldOptions;
use lune_ui_core::widgets::popover::PopoverOptions;
use lune_ui_core::widgets::toast::{
    APPEAR, DRAIN, EXPAND, GAP, HISTORY_CAP, LEAVE, MAX_VISIBLE, SHIFT, ToastAction, ToastExtras,
    ToastId, ToastKind, ToastResponse, ToastSpec, ToastStackOptions, Toasts, WIDTH, WIDTH_EXPANDED,
    toast_size,
};

const WIN: (f32, f32) = (800.0, 600.0);
const P: PointerButton = PointerButton::Primary;

fn ms(n: u64) -> Time {
    Duration::from_millis(n)
}

fn spec(kind: ToastKind, message: &str) -> ToastSpec {
    ToastSpec::new(kind, message)
}

fn info(message: &str) -> ToastSpec {
    spec(ToastKind::Info, message)
}

fn loading(progress: Option<f32>, cancellable: bool) -> ToastSpec {
    spec(
        ToastKind::Loading {
            progress,
            cancellable,
        },
        "Fetching",
    )
}

/// An info toast with a secondary "Later" (action 1) and a primary "Open" (action 2).
fn prompt(message: &str) -> ToastSpec {
    ToastSpec {
        actions: vec![
            ToastAction {
                label: "Open".into(),
                primary: true,
                id: 2,
            },
            ToastAction {
                label: "Later".into(),
                primary: false,
                id: 1,
            },
        ],
        ..info(message)
    }
}

fn root() -> WidgetId {
    WidgetId::ROOT.with("t")
}

/// The node (and scope) of a toast.
fn node_of(id: ToastId) -> WidgetId {
    root().with(id.0)
}

fn frame(h: &mut UiHarness, toasts: &mut Toasts, at: u64) -> (Scene, FrameOutput, ToastResponse) {
    let mut resp = ToastResponse::default();
    let (scene, out) = h.at_ms(at).frame(|ui| resp = ui.toasts("t", toasts));
    (scene, out, resp)
}

/// Frames at 0, 100, 200 and 400 ms past `from`... a settled start: `at` is the last frame.
fn settle(h: &mut UiHarness, toasts: &mut Toasts, from: u64) -> (Scene, FrameOutput) {
    for dt in [0, 100, 200] {
        frame(h, toasts, from + dt);
    }
    let (s, o, _) = frame(h, toasts, from + 400);
    (s, o)
}

/// The messages of the toasts in the tree, top to bottom.
fn shown(out: &FrameOutput) -> Vec<String> {
    out.tree
        .nodes
        .iter()
        .filter(|n| matches!(n.role, Role::Status | Role::Alert))
        .map(|n| n.name.clone())
        .collect()
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn button_named(out: &FrameOutput, name: &str) -> Rect {
    out.tree
        .nodes
        .iter()
        .find(|n| n.role == Role::Button && n.name == name)
        .unwrap_or_else(|| panic!("no button {name}"))
        .bounds
}

/// Presses and releases at `at`, with a frame between so the regions are current.
fn click(h: &mut UiHarness, toasts: &mut Toasts, at: (f32, f32), t: u64) -> ToastResponse {
    h.input.pointer_to(at.0, at.1);
    frame(h, toasts, t);
    h.input.click(P);
    frame(h, toasts, t + 5).2
}

/// A point on a toast's body: inside its padding, away from every control.
fn body_point(out: &FrameOutput, id: ToastId) -> (f32, f32) {
    let b = out.tree.get(node_of(id)).expect("toast node").bounds;
    (b.x + 100.0, b.bottom() - 5.0)
}

// -- the model's rules --------------------------------------------------------------------------

#[test]
fn timed_toasts_leave_at_their_deadline_and_are_archived_once() {
    for (kind, secs) in [
        (ToastKind::Info, 4),
        (ToastKind::Success, 3),
        (ToastKind::Warning, 6),
    ] {
        let mut h = UiHarness::new(WIN);
        let mut toasts = Toasts::default();
        toasts.push(spec(kind, "Saved"), ms(0));
        frame(&mut h, &mut toasts, 0);
        let (_, out, _) = frame(&mut h, &mut toasts, secs * 1000 - 1);
        assert_eq!(shown(&out), ["Saved"], "{kind:?} before");
        assert_eq!(toasts.history().count(), 0);
        let (_, out, _) = frame(&mut h, &mut toasts, secs * 1000);
        assert!(shown(&out).is_empty(), "{kind:?}: leaving, no node");
        assert_eq!(toasts.history().count(), 1, "{kind:?}");
        // The fade, then gone; never archived twice.
        frame(&mut h, &mut toasts, secs * 1000 + 100);
        frame(&mut h, &mut toasts, secs * 1000 + 400);
        assert!(toasts.is_empty(), "{kind:?}");
        assert_eq!((toasts.history().count(), toasts.unread()), (1, 1));
    }
}

#[test]
fn error_loading_and_prompt_toasts_never_time_out() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    toasts.push(spec(ToastKind::Error, "Failed"), ms(0));
    toasts.push(loading(None, false), ms(0));
    toasts.push(prompt("Update?"), ms(0));
    frame(&mut h, &mut toasts, 0);
    let (_, out, _) = frame(&mut h, &mut toasts, 600_000);
    assert_eq!(shown(&out).len(), 3);
    assert_eq!(toasts.history().count(), 0);
}

#[test]
fn an_expanded_toast_waits_for_the_collapse() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(info("Pulled"), ms(0));
    settle(&mut h, &mut toasts, 0);
    // A press on the body opens the log; the deadline (4 s) passes with it open.
    let (_, out, _) = frame(&mut h, &mut toasts, 600);
    click(&mut h, &mut toasts, body_point(&out, id), 700);
    assert!(toasts.expanded(id));
    let (_, out, _) = frame(&mut h, &mut toasts, 4500);
    assert_eq!(shown(&out), ["Pulled"]);
    assert_eq!(toasts.history().count(), 0);
    // No frame is due: it waits on the user.
    assert_eq!(frame(&mut h, &mut toasts, 5000).1.repaint, Repaint::Idle);
    // Collapsing it lets the expired deadline act at once.
    let (_, out, _) = frame(&mut h, &mut toasts, 5400);
    click(&mut h, &mut toasts, body_point(&out, id), 5500);
    assert!(!toasts.expanded(id));
    let (_, out, _) = frame(&mut h, &mut toasts, 5600);
    assert!(shown(&out).is_empty());
    assert_eq!(toasts.history().count(), 1);
}

#[test]
fn a_fourth_toast_evicts_the_oldest_that_may_go_without_archiving_it() {
    let mut toasts = Toasts::default();
    toasts.push(info("a"), ms(0));
    toasts.push(loading(None, false), ms(0));
    toasts.push(prompt("c"), ms(0));
    toasts.push(info("d"), ms(0));
    let mut h = UiHarness::new(WIN);
    frame(&mut h, &mut toasts, 0);
    let (_, out, _) = frame(&mut h, &mut toasts, 400);
    assert_eq!(shown(&out), ["Fetching", "c", "d"], "a was evicted");
    assert_eq!(toasts.history().count(), 0, "eviction does not archive");
    assert_eq!(toasts.unread(), 0);
}

#[test]
fn with_nothing_to_evict_four_toasts_show() {
    let mut toasts = Toasts::default();
    toasts.push(loading(None, false), ms(0));
    toasts.push(prompt("b"), ms(0));
    toasts.push(loading(Some(0.5), true), ms(0));
    toasts.push(prompt("d"), ms(0));
    let mut h = UiHarness::new(WIN);
    frame(&mut h, &mut toasts, 0);
    let (_, out, _) = frame(&mut h, &mut toasts, 400);
    assert_eq!(shown(&out).len(), MAX_VISIBLE + 1);
}

#[test]
fn the_history_keeps_fifty_newest_first_and_counts_unread() {
    let mut toasts = Toasts::default();
    for n in 0..60 {
        let id = toasts.push(info(&format!("n{n}")), ms(0));
        toasts.dismiss(id, ms(0));
    }
    let messages: Vec<&str> = toasts.history().map(|s| s.message.as_str()).collect();
    assert_eq!(messages.len(), HISTORY_CAP);
    assert_eq!((messages[0], messages[49]), ("n59", "n10"));
    assert_eq!(toasts.unread(), 60);
    toasts.mark_read();
    assert_eq!(toasts.unread(), 0);
    let id = toasts.push(info("later"), ms(0));
    toasts.dismiss(id, ms(0));
    toasts.dismiss(id, ms(0));
    assert_eq!(toasts.unread(), 1, "dismissing twice archives once");
    toasts.clear_history();
    assert_eq!((toasts.history().count(), toasts.unread()), (0, 0));
}

#[test]
fn update_starts_a_timer_for_a_finished_loading_toast_at_the_next_frame() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(loading(Some(1.0), true), ms(0));
    frame(&mut h, &mut toasts, 0);
    frame(&mut h, &mut toasts, 10_000);
    assert!(toasts.history().next().is_none(), "loading never times out");
    toasts.update(id, spec(ToastKind::Success, "Done"));
    // The timer starts at the frame that sees it: 3 s after 10.5 s (the bar goes, the height
    // settles first).
    frame(&mut h, &mut toasts, 10_500);
    let (_, out, _) = frame(&mut h, &mut toasts, 11_000);
    assert_eq!(out.repaint, Repaint::At(ms(13_500)));
    frame(&mut h, &mut toasts, 13_499);
    assert_eq!(toasts.history().count(), 0);
    frame(&mut h, &mut toasts, 13_500);
    assert_eq!(toasts.history().count(), 1);
    // A timed toast keeps its deadline through a new message; an error loses it.
    let a = toasts.push(info("x"), ms(20_000));
    frame(&mut h, &mut toasts, 20_000);
    frame(&mut h, &mut toasts, 20_400);
    toasts.update(a, info("y"));
    assert_eq!(
        frame(&mut h, &mut toasts, 20_500).1.repaint,
        Repaint::At(ms(24_000))
    );
    toasts.update(a, spec(ToastKind::Error, "z"));
    frame(&mut h, &mut toasts, 20_200);
    let (_, out, _) = frame(&mut h, &mut toasts, 30_000);
    assert_eq!(shown(&out), ["z"]);
}

#[test]
fn a_dismissed_toast_fades_in_one_fade_and_one_archive() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(info("hello"), ms(0));
    settle(&mut h, &mut toasts, 0);
    toasts.dismiss(id, ms(500));
    let mut alphas = Vec::new();
    for t in (500..800).step_by(10) {
        let (scene, out, _) = frame(&mut h, &mut toasts, t);
        if let Some(s) = scene.merged(LayerId::Toast).geometry.under.first() {
            alphas.push(s.color[3]);
        }
        assert!(shown(&out).is_empty(), "no node while leaving");
    }
    assert!(alphas.windows(2).all(|w| w[1] <= w[0]), "{alphas:?}");
    assert!(!alphas.is_empty() && alphas.len() < 30, "{}", alphas.len());
    assert!(toasts.is_empty());
    assert_eq!(toasts.history().count(), 1);
}

#[test]
fn a_toast_that_left_before_it_was_drawn_has_no_fade() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(info("quick"), ms(0));
    toasts.dismiss(id, ms(0));
    let (scene, out, _) = frame(&mut h, &mut toasts, 0);
    assert!(toasts.is_empty());
    assert!(shown(&out).is_empty());
    assert!(scene.merged(LayerId::Toast).geometry.under.is_empty());
    assert_eq!(toasts.history().count(), 1);
}

// -- geometry -----------------------------------------------------------------------------------

#[test]
fn scale_two_probes_width_radius_padding_gap_and_margin() {
    let mut h = UiHarness::new(WIN);
    h.input.set_scale(2.0);
    let mut toasts = Toasts::default();
    toasts.push(info("First"), ms(0));
    toasts.push(info("Second"), ms(0));
    let (scene, _) = settle(&mut h, &mut toasts, 0);
    let layer = &scene.merged(LayerId::Toast);
    // The card fills are the frosts, one per card (the border rings are strokes).
    let fills: Vec<_> = layer
        .geometry
        .under
        .iter()
        .filter(|s| s.params[2] == 2.0 && s.color[3] > 0.8 * premul(DARK.menu_glass)[3])
        .collect();
    assert_eq!(fills.len(), 2);
    let (first, second) = (fills[0], fills[1]);
    // Width 640 (the minimum is 600), radius 16.
    assert!(first.rect[2] >= 600.0, "{:?}", first.rect);
    assert_eq!(first.rect[2], 640.0);
    assert_eq!(first.radii[0], 16.0);
    // 32 from the right and the bottom edge, 16 between.
    assert_eq!(first.rect[0] + first.rect[2], 1600.0 - 32.0);
    assert_eq!(second.rect[1] + second.rect[3], 1200.0 - 32.0);
    assert_eq!(second.rect[1] - (first.rect[1] + first.rect[3]), 16.0);
    // Padding 24 inside a 2 px border, then the 16 px glyph and its 8 px gap: the message starts
    // 26 + 48 in. (Text positions are logical.)
    let message = layer.texts[1].x * 2.0;
    assert_eq!(message, first.rect[0] + 26.0 + 48.0);
}

#[test]
fn width_is_320_collapsed_and_720_expanded_and_the_height_follows_the_text() {
    let mut text = FakeText;
    let win = Rect::new(0.0, 0.0, WIN.0, WIN.1);
    let mut p = Painter::<u8>::new(1.0, win, &mut text, None, DARK);
    let short = info("ok");
    let long = ToastSpec {
        detail: Some("Several words of detail that wrap onto more than one row here".into()),
        ..info("A message long enough to wrap onto a second row of this narrow toast box")
    };
    assert_eq!(toast_size(&mut p, &short, false), (WIDTH, 26.0 + 20.0));
    let (w, h) = toast_size(&mut p, &long, false);
    assert_eq!(w, WIDTH);
    assert!(h > 46.0 + 14.0);
    let (w, he) = toast_size(&mut p, &short, true);
    assert_eq!((w, he), (WIDTH_EXPANDED, 26.0 + 20.0 + 8.0 + 200.0));
    let (_, hp) = toast_size(&mut p, &prompt("x"), false);
    assert_eq!(hp, 26.0 + 20.0 + 8.0 + (13.0f32 * 1.3).round() + 16.0);
    let (_, hl) = toast_size(&mut p, &loading(None, false), false);
    assert_eq!(hl, 26.0 + 20.0 + 8.0 + 3.0);
    let (_, hd) = toast_size(&mut p, &loading(Some(0.4), false), false);
    // 3 px in both states (`MainWindow.axaml:369`).
    assert_eq!(hd, 26.0 + 20.0 + 8.0 + 3.0);
}

// -- motion -------------------------------------------------------------------------------------

/// Samples `f` on the 60 Hz grid from time 0: the harness runs a frame at each sample and `f`
/// reads what it needs from the scene; in flight is `Repaint::Now`.
fn sampler<'a>(
    h: &'a mut UiHarness,
    toasts: &'a mut Toasts,
    f: impl Fn(&Scene) -> f32 + 'a,
) -> impl FnMut(Time) -> (f32, bool) + 'a {
    move |t| {
        h.input.set_time(t);
        let mut resp = ToastResponse::default();
        let (scene, out) = h.frame(|ui| resp = ui.toasts("t", toasts));
        (f(&scene), out.repaint == Repaint::Now)
    }
}

fn card_fill(scene: &Scene) -> [f32; 4] {
    scene.merged(LayerId::Toast).geometry.under[0].rect
}

fn card_alpha(scene: &Scene) -> f32 {
    // A frost keeps its colour unfaded and the fade rides in `params[1]`.
    scene
        .merged(LayerId::Toast)
        .geometry
        .under
        .iter()
        .find(|s| s.params[2] == 2.0)
        .map_or(0.0, |s| s.params[1])
}

const TIERS: [MotionTier; 3] = [MotionTier::Full, MotionTier::Reduced, MotionTier::Off];

fn run_end(m: &lune_ui_core::motion::Motion, tier: MotionTier) -> Time {
    DARK.motion
        .resolve(m, tier)
        .map_or(Time::ZERO, |r| r.duration)
}

#[test]
fn appearing_conforms_in_three_tiers() {
    for tier in TIERS {
        let mut h = UiHarness::new(WIN);
        h.tier = tier;
        let mut toasts = Toasts::default();
        let id = toasts.push(spec(ToastKind::Error, "Failed"), ms(0));
        let _ = id;
        let end = run_end(&APPEAR, tier);
        // Opacity 0 to 1.
        conformance_of(sampler(&mut h, &mut toasts, card_alpha), end, 1.0, 0.35)
            .unwrap_or_else(|e| panic!("{tier:?} opacity: {e}"));
        // The rise: 8 px down to the place, none under Reduced and Off.
        let mut h = UiHarness::new(WIN);
        h.tier = tier;
        let mut toasts = Toasts::default();
        toasts.push(spec(ToastKind::Error, "Failed"), ms(0));
        let rest = WIN.1 - 16.0 - 46.0;
        let bound = if tier == MotionTier::Full {
            0.35 * 8.0
        } else {
            0.0
        };
        conformance_of(
            sampler(&mut h, &mut toasts, |s| card_fill(s)[1]),
            end,
            rest,
            bound,
        )
        .unwrap_or_else(|e| panic!("{tier:?} rise: {e}"));
    }
}

#[test]
fn leaving_conforms_in_three_tiers() {
    for tier in TIERS {
        let mut h = UiHarness::new(WIN);
        h.tier = tier;
        let mut toasts = Toasts::default();
        let id = toasts.push(spec(ToastKind::Error, "Failed"), ms(0));
        settle(&mut h, &mut toasts, 0);
        toasts.dismiss(id, ms(1000));
        let base = 1000u64;
        let mut s = sampler(&mut h, &mut toasts, card_alpha);
        // The grid starts at the dismissal.
        let shifted = move |t: Time| s(t + ms(base));
        conformance_of(shifted, run_end(&LEAVE, tier), 0.0, 0.35)
            .unwrap_or_else(|e| panic!("{tier:?}: {e}"));
    }
}

#[test]
fn a_neighbour_moves_in_one_retargeted_transition() {
    for tier in TIERS {
        let mut h = UiHarness::new(WIN);
        h.tier = tier;
        let mut toasts = Toasts::default();
        toasts.push(spec(ToastKind::Error, "Older"), ms(0));
        settle(&mut h, &mut toasts, 0);
        let y0 = card_fill(&frame(&mut h, &mut toasts, 500).0)[1];
        // A newer toast arrives at 1000 ms: the older one moves up by its height and the gap.
        toasts.push(spec(ToastKind::Error, "Newer"), ms(1000));
        let target = y0 - (46.0 + GAP);
        // The older card is the first fill on the layer. The newer one keeps appearing, so "in
        // flight" here is the older card's own position not being there yet.
        let mut s = sampler(&mut h, &mut toasts, |s| card_fill(s)[1]);
        let mut first = true;
        let moved = move |t: Time| {
            let (y, _) = s(t + ms(1000));
            if first {
                // The first frame keeps the card where it was (Full), or snaps (the others).
                let want = if tier == MotionTier::Full { y0 } else { target };
                assert!((y - want).abs() < 0.01, "{tier:?}: {y} against {want}");
                first = false;
            }
            (y, y != target)
        };
        let end = run_end(&SHIFT, tier);
        conformance_of(moved, end, target, 0.35 * (46.0 + GAP))
            .unwrap_or_else(|e| panic!("{tier:?}: {e}"));
    }
}

#[test]
fn expanding_animates_the_height_and_collapsing_retargets_it() {
    let (collapsed, expanded) = {
        let mut text = FakeText;
        let win = Rect::new(0.0, 0.0, WIN.0, WIN.1);
        let mut p = Painter::<u8>::new(1.0, win, &mut text, None, DARK);
        let s = spec(ToastKind::Error, "Failed");
        (
            toast_size(&mut p, &s, false).1,
            toast_size(&mut p, &s, true).1,
        )
    };
    for tier in TIERS {
        let mut h = UiHarness::new(WIN);
        h.tier = tier;
        let mut toasts = Toasts::default();
        let id = toasts.push(spec(ToastKind::Error, "Failed"), ms(0));
        settle(&mut h, &mut toasts, 0);
        toasts.set_expanded(id, true);
        let mut s = sampler(&mut h, &mut toasts, |s| card_fill(s)[3]);
        let moved = move |t: Time| s(t + ms(1000));
        conformance_of(
            moved,
            run_end(&EXPAND, tier),
            expanded,
            0.35 * (expanded - collapsed),
        )
        .unwrap_or_else(|e| panic!("{tier:?}: {e}"));
    }
    // Collapsed again midway: the height turns from where it is, with no jump.
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(spec(ToastKind::Error, "Failed"), ms(0));
    settle(&mut h, &mut toasts, 0);
    toasts.set_expanded(id, true);
    let mut last = collapsed;
    for k in 0..60u64 {
        let t = 1000 + k * 1000 / 60;
        if k == 6 {
            toasts.set_expanded(id, false);
        }
        let (scene, _, _) = frame(&mut h, &mut toasts, t);
        let height = card_fill(&scene)[3];
        assert!(
            (height - last).abs() <= 0.35 * (expanded - collapsed),
            "frame {k}: {height} after {last}"
        );
        last = height;
    }
    assert_eq!(last, collapsed);
}

#[test]
fn no_toast_jumps_while_others_arrive_and_leave() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let mut ids: Vec<ToastId> = Vec::new();
    let mut last: Vec<Option<f32>> = vec![None; 3];
    let mut worst = 0.0f32;
    for k in 0..120u64 {
        let t = k * 1000 / 60;
        if k == 0 || k == 10 || k == 20 {
            ids.push(toasts.push(spec(ToastKind::Error, &format!("t{k}")), ms(t)));
        }
        if k == 40 {
            toasts.dismiss(ids[1], ms(t));
        }
        let (_, out, _) = frame(&mut h, &mut toasts, t);
        for (n, id) in ids.iter().enumerate() {
            let y = out.tree.get(node_of(*id)).map(|node| node.bounds.y);
            if let (Some(a), Some(b)) = (last[n], y) {
                worst = worst.max((a - b).abs());
            }
            last[n] = y;
        }
    }
    // A card is 46 high and the gap 8: one place is 54; no step comes near it.
    assert!(worst > 0.5, "the test moved nothing: {worst}");
    assert!(worst <= 0.35 * 54.0, "a step of {worst}");
}

#[test]
fn a_second_arrival_during_a_shift_retargets_the_toast_from_where_it_is() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let first = toasts.push(spec(ToastKind::Error, "first"), ms(0));
    settle(&mut h, &mut toasts, 0);
    let mut last: Option<f32> = None;
    let mut worst = 0.0f32;
    for k in 0..40u64 {
        let t = 1000 + k * 1000 / 60;
        // The second arrival lands a few frames into the shift the first one started.
        if k == 2 || k == 6 {
            toasts.push(spec(ToastKind::Error, &format!("t{k}")), ms(t));
        }
        let (_, out, _) = frame(&mut h, &mut toasts, t);
        let y = out.tree.get(node_of(first)).expect("first").bounds.y;
        if let Some(a) = last {
            worst = worst.max((a - y).abs());
        }
        last = Some(y);
    }
    assert!(worst > 0.5, "the test moved nothing: {worst}");
    // A place is 54: a shift dropped mid-flight would step by what it had left.
    assert!(worst <= 0.35 * 54.0, "a step of {worst}");
}

// -- nodes and input ----------------------------------------------------------------------------

#[test]
fn the_nodes_are_a_status_an_alert_buttons_and_a_progress() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let plain = ToastSpec {
        detail: Some("Three files".into()),
        ..info("Pulled")
    };
    let a = toasts.push(plain, ms(0));
    let b = toasts.push(spec(ToastKind::Error, "Failed"), ms(0));
    let c = toasts.push(loading(None, true), ms(0));
    let (_, out) = settle(&mut h, &mut toasts, 0);
    let n = out.tree.get(node_of(a)).expect("info");
    assert_eq!((n.role, n.live), (Role::Status, Some(Live::Polite)));
    assert_eq!(n.name, "Pulled");
    assert_eq!(n.description.as_deref(), Some("Three files"));
    let n = out.tree.get(node_of(b)).expect("error");
    assert_eq!((n.role, n.live), (Role::Alert, Some(Live::Assertive)));
    let n = out.tree.get(node_of(c)).expect("loading");
    assert_eq!(n.role, Role::Status);
    // Dismiss on each, Cancel on the cancellable loading toast only.
    let dismissals = out
        .tree
        .nodes
        .iter()
        .filter(|n| n.role == Role::Button && n.name == "Dismiss")
        .count();
    assert_eq!(dismissals, 3);
    let cancels = out
        .tree
        .nodes
        .iter()
        .filter(|n| n.role == Role::Button && n.name == "Cancel")
        .count();
    assert_eq!(cancels, 1);
    // The bar of the loading toast: a progress node with no value while indeterminate.
    let bar = out
        .tree
        .nodes
        .iter()
        .find(|n| {
            n.role == Role::Progress
                && n.parent == Some(node_of(c))
                && n.value.is_none()
                && n.name == "Fetching"
        })
        .expect("indeterminate bar");
    assert!(bar.states.contains(State::Busy) || bar.value.is_none());
    // The default action and Dismiss are on the toast; Expand while collapsed.
    let n = out.tree.get(node_of(a)).expect("info");
    for action in [Action::Default, Action::Dismiss, Action::Expand] {
        assert!(n.actions.contains(action), "{action:?}");
    }
    assert!(!n.actions.contains(Action::Collapse));
}

#[test]
fn a_determinate_bar_has_a_value() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let c = toasts.push(loading(Some(0.4), false), ms(0));
    let (_, out) = settle(&mut h, &mut toasts, 0);
    let bar = out
        .tree
        .nodes
        .iter()
        .find(|n| n.role == Role::Progress && n.parent == Some(node_of(c)) && n.value.is_some())
        .expect("bar");
    match bar.value {
        Some(Value::Number { value, .. }) => assert!((value - 40.0).abs() < 0.01),
        ref v => panic!("{v:?}"),
    }
    assert_eq!(
        bar.bounds.h, 3.0,
        "3 px high while determinate, as the shipped bar"
    );
}

#[test]
fn access_default_toggles_and_dismiss_dismisses() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(spec(ToastKind::Error, "Failed"), ms(0));
    settle(&mut h, &mut toasts, 0);
    let act = |h: &mut UiHarness, action| {
        h.input.access(AccessAction {
            target: node_of(id),
            action,
            value: None,
        });
    };
    act(&mut h, Action::Default);
    frame(&mut h, &mut toasts, 600);
    assert!(toasts.expanded(id));
    let (_, out, _) = frame(&mut h, &mut toasts, 650);
    assert!(
        out.tree
            .get(node_of(id))
            .expect("node")
            .states
            .contains(State::Expanded)
    );
    act(&mut h, Action::Collapse);
    frame(&mut h, &mut toasts, 700);
    assert!(!toasts.expanded(id));
    // The tree the actions resolve against is the previous frame's: let it catch up.
    frame(&mut h, &mut toasts, 750);
    act(&mut h, Action::Expand);
    frame(&mut h, &mut toasts, 800);
    assert!(toasts.expanded(id));
    act(&mut h, Action::Dismiss);
    frame(&mut h, &mut toasts, 900);
    assert_eq!(toasts.history().count(), 1);
    let (_, out, _) = frame(&mut h, &mut toasts, 1300);
    assert!(shown(&out).is_empty());
    // The fade starts at the first frame that sees it leaving and ends 180 ms later.
    frame(&mut h, &mut toasts, 1500);
    assert!(toasts.is_empty());
}

#[test]
fn dismiss_and_cancel_do_not_toggle_and_a_body_press_does() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let c = toasts.push(loading(Some(0.2), true), ms(0));
    let (_, out) = settle(&mut h, &mut toasts, 0);
    // Cancel: reported, nothing else changes.
    let resp = click(
        &mut h,
        &mut toasts,
        centre(button_named(&out, "Cancel")),
        600,
    );
    assert_eq!(resp.cancelled, Some(c));
    assert!(!toasts.expanded(c));
    assert_eq!(toasts.history().count(), 0);
    // Dismiss: leaves, archived, still collapsed.
    let (_, out, _) = frame(&mut h, &mut toasts, 700);
    click(
        &mut h,
        &mut toasts,
        centre(button_named(&out, "Dismiss")),
        800,
    );
    assert!(!toasts.expanded(c));
    assert_eq!(toasts.history().count(), 1);
    // A second toast: the body toggles.
    let d = toasts.push(info("again"), ms(900));
    settle(&mut h, &mut toasts, 900);
    let (_, out, _) = frame(&mut h, &mut toasts, 1400);
    click(&mut h, &mut toasts, body_point(&out, d), 1500);
    assert!(toasts.expanded(d));
}

#[test]
fn an_action_button_reports_once_and_the_toast_leaves() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(prompt("Update?"), ms(0));
    let (_, out) = settle(&mut h, &mut toasts, 0);
    // The app's order, no sort: `prompt` lists Open (primary) then Later, so Later is rightmost.
    let (later, open) = (button_named(&out, "Later"), button_named(&out, "Open"));
    assert!(open.x < later.x);
    let card = out.tree.get(node_of(id)).expect("node").bounds;
    assert!(later.right() <= card.right() - 13.0 + 0.01);
    let resp = click(&mut h, &mut toasts, centre(open), 600);
    assert_eq!(resp.action, Some((id, 2)));
    assert_eq!(toasts.history().count(), 1);
    assert_eq!(frame(&mut h, &mut toasts, 700).2, ToastResponse::default());
}

#[test]
fn enter_and_space_press_a_focused_action_once() {
    for k in [NamedKey::Enter, NamedKey::Space] {
        let mut h = UiHarness::new(WIN);
        let mut toasts = Toasts::default();
        let id = toasts.push(prompt("Update?"), ms(0));
        let (_, out) = settle(&mut h, &mut toasts, 0);
        let _ = out;
        h.input.set_modifiers(Modifiers {
            shift: true,
            ..Modifiers::default()
        });
        h.input.key_down(Key::Named(NamedKey::Tab), None);
        h.input.set_modifiers(Modifiers::default());
        let (_, out2, _) = frame(&mut h, &mut toasts, 600);
        let at = out2
            .focused
            .and_then(|f| out2.tree.get(f))
            .map(|n| n.name.clone());
        assert_eq!(
            at.as_deref(),
            Some("Later"),
            "Shift+Tab reaches the last button, drawn in the app's order"
        );
        h.input.key_down(Key::Named(k), None);
        let r = frame(&mut h, &mut toasts, 610).2;
        assert!(matches!(r.action, Some((i, _)) if i == id), "{k:?}: {r:?}");
        assert_eq!(frame(&mut h, &mut toasts, 620).2, ToastResponse::default());
    }
}

#[test]
fn a_toast_takes_no_focus_on_appear_and_draws_outside_a_record() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    toasts.push(prompt("Update?"), ms(0));
    let (_, out, _) = frame(&mut h, &mut toasts, 0);
    assert_eq!(out.focused, None);
    // Inside a `record`, the toasts are not part of the recording, and the recording goes on
    // around them.
    let mut ops_len = usize::MAX;
    h.at_ms(100).frame(|ui| {
        let (_, ops) = ui.record(|ui| {
            ui.painter().fill(PAGE, 0.0, [1, 2, 3, 255]);
            ui.toasts("t", &mut toasts);
            ui.painter().fill(PAGE, 0.0, [4, 5, 6, 255]);
        });
        ops_len = ops.len();
    });
    assert_eq!(ops_len, 2, "the two fills, and nothing of the toast");
}

#[test]
fn the_toast_is_called_in_a_disabled_scope_and_every_node_says_so() {
    let mut toasts = Toasts::default();
    let c = toasts.push(loading(Some(0.5), true), ms(0));
    let p = toasts.push(prompt("Update?"), ms(0));
    let mut h = UiHarness::new(WIN);
    let keys = [NamedKey::Enter, NamedKey::Space];
    for k in keys {
        h.shortcuts
            .add(Binding::new(Scope::Global, Chord::key(Key::Named(k)), "go"));
    }
    // Focus a button while enabled (Tab, which presses nothing), then disable the whole stack.
    settle(&mut h, &mut toasts, 0);
    h.input.key_down(Key::Named(NamedKey::Tab), None);
    let (_, _, _) = frame(&mut h, &mut toasts, 600);
    frame(&mut h, &mut toasts, 620);
    let run = |h: &mut UiHarness, toasts: &mut Toasts, at: u64| {
        let mut resp = ToastResponse::default();
        let (_, out) = h
            .at_ms(at)
            .frame(|ui| ui.with_enabled(false, |ui| resp = ui.toasts("t", toasts)));
        (out, resp)
    };
    run(&mut h, &mut toasts, 700);
    for (i, k) in keys.iter().enumerate() {
        h.input.key_down(Key::Named(*k), None);
        let (out, resp) = run(&mut h, &mut toasts, 710 + 10 * i as u64);
        assert!(out.fired.is_empty(), "{k:?} reached the binding");
        assert_eq!(resp, ToastResponse::default(), "{k:?}");
    }
    // A body press toggles nothing, a button press reports nothing.
    let (out, _) = run(&mut h, &mut toasts, 800);
    for id in [c, p] {
        let b = out.tree.get(node_of(id)).expect("node").bounds;
        h.input.pointer_to(b.x + 100.0, b.bottom() - 5.0);
        run(&mut h, &mut toasts, 810);
        h.input.click(P);
        run(&mut h, &mut toasts, 820);
        assert!(!toasts.expanded(id));
        assert_eq!(disabled_violations(&out.tree, node_of(id)), [], "{id:?}");
    }
    let open = centre(button_named(&out, "Open"));
    h.input.pointer_to(open.0, open.1);
    run(&mut h, &mut toasts, 830);
    h.input.click(P);
    let (_, resp) = run(&mut h, &mut toasts, 840);
    assert_eq!(resp, ToastResponse::default());
    assert_eq!(toasts.history().count(), 0);
}

#[test]
fn toasts_waiting_on_a_timer_report_at_the_next_deadline_never_now() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    toasts.push(info("a"), ms(0));
    toasts.push(spec(ToastKind::Success, "b"), ms(100));
    settle(&mut h, &mut toasts, 0);
    let (_, out, _) = frame(&mut h, &mut toasts, 800);
    // Success at 100 ms + 3 s is before info at 4 s.
    assert_eq!(out.repaint, Repaint::At(ms(3100)));
    let (_, out, _) = frame(&mut h, &mut toasts, 1500);
    assert_eq!(out.repaint, Repaint::At(ms(3100)));
    // Nothing timed: idle.
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    toasts.push(spec(ToastKind::Error, "e"), ms(0));
    let (_, out) = settle(&mut h, &mut toasts, 0);
    assert_eq!(out.repaint, Repaint::Idle);
}

// -- over a dialog and a raised popover ----------------------------------------------------------

const GLASS: DialogGlass = DialogGlass {
    fill: [0.2, 0.2, 0.2, 0.62],
    noise: false,
};
const DIALOG: Dialog<'static> = Dialog {
    title: "Title",
    message: "Message",
    extra_h: 80.0,
    confirm: "Yes",
    cancel: Cancel::Default,
};
const PAGE: Rect = Rect::new(20.0, 20.0, 100.0, 30.0);

/// A page button, a dialog (open until cancelled) with a popover opened by its anchor button, and
/// the toasts.
#[derive(Debug, Default)]
struct Bed {
    dialog: bool,
    popover: bool,
    cancelled: u32,
    confirmed: u32,
}

fn bed(
    h: &mut UiHarness,
    b: &mut Bed,
    toasts: &mut Toasts,
    at: u64,
) -> (FrameOutput, ToastResponse) {
    let mut resp = ToastResponse::default();
    let (_, out) = h.at_ms(at).frame(|ui| {
        ui.painter().on(LayerId::Page);
        ui.button("page", PAGE, "page", ButtonKind::Secondary);
        if b.dialog {
            let r = ui.dialog("dialog", &DIALOG, &GLASS, |ui: &mut Ui<'_>, x, y, _| {
                let open = ui.button(
                    "anchor",
                    Rect::new(x, y, 120.0, 24.0),
                    "open",
                    ButtonKind::Secondary,
                );
                b.popover |= open.clicked;
                if b.popover {
                    let options = PopoverOptions {
                        anchor_id: Some(open.id),
                        ..PopoverOptions::new(open.rect, (200.0, 60.0))
                    };
                    let (frame, _, _) = ui.popover("pop", &options, |ui, rect| {
                        ui.button(
                            "inside",
                            Rect::new(rect.x + 8.0, rect.y + 8.0, 100.0, 24.0),
                            "inside",
                            ButtonKind::Secondary,
                        );
                    });
                    if frame.dismissed.is_some() {
                        b.popover = false;
                    }
                }
            });
            if r.cancelled {
                b.cancelled += 1;
                b.dialog = false;
            }
            if r.confirmed {
                b.confirmed += 1;
                b.dialog = false;
            }
        }
        resp = ui.toasts("t", toasts);
    });
    (out, resp)
}

fn open_bed(with_popover: bool) -> (UiHarness, Bed, Toasts) {
    let mut h = UiHarness::new(WIN);
    let mut b = Bed {
        dialog: true,
        ..Bed::default()
    };
    let mut toasts = Toasts::default();
    toasts.push(prompt("Update?"), ms(0));
    for t in [0, 300, 600, 900] {
        bed(&mut h, &mut b, &mut toasts, t);
    }
    if with_popover {
        let (out, _) = bed(&mut h, &mut b, &mut toasts, 1000);
        let c = centre(button_named(&out, "open"));
        h.input.pointer_to(c.0, c.1);
        bed(&mut h, &mut b, &mut toasts, 1010);
        h.input.click(P);
        for t in [1020, 1300, 1600] {
            bed(&mut h, &mut b, &mut toasts, t);
        }
        assert!(b.popover);
    }
    (h, b, toasts)
}

#[test]
fn a_toast_button_works_over_a_dialog_and_over_a_raised_popover() {
    for with_popover in [false, true] {
        let (mut h, mut b, mut toasts) = open_bed(with_popover);
        let (out, _) = bed(&mut h, &mut b, &mut toasts, 2000);
        let c = centre(button_named(&out, "Open"));
        h.input.pointer_to(c.0, c.1);
        bed(&mut h, &mut b, &mut toasts, 2010);
        h.input.click(P);
        let (_, resp) = bed(&mut h, &mut b, &mut toasts, 2020);
        assert!(
            matches!(resp.action, Some((_, 2))),
            "popover {with_popover}: {resp:?}"
        );
        // The press that did it dismissed nothing it should not have: the dialog is open.
        assert!(b.dialog, "popover {with_popover}");
        assert_eq!(b.cancelled, 0);
    }
}

#[test]
fn a_body_press_over_a_dialog_toggles_the_toast_and_not_the_dialog() {
    let (mut h, mut b, mut toasts) = open_bed(false);
    let (out, _) = bed(&mut h, &mut b, &mut toasts, 2000);
    let id = ToastId(1);
    let at = body_point(&out, id);
    h.input.pointer_to(at.0, at.1);
    bed(&mut h, &mut b, &mut toasts, 2010);
    h.input.click(P);
    bed(&mut h, &mut b, &mut toasts, 2020);
    assert!(toasts.expanded(id));
    assert!(b.dialog && b.cancelled == 0);
}

#[test]
fn an_accessibility_dismiss_on_a_toast_dismisses_the_toast_and_no_overlay() {
    for with_popover in [false, true] {
        let (mut h, mut b, mut toasts) = open_bed(with_popover);
        let id = ToastId(1);
        h.input.access(AccessAction {
            target: node_of(id),
            action: Action::Dismiss,
            value: None,
        });
        bed(&mut h, &mut b, &mut toasts, 2000);
        for t in [2100, 2400, 2800] {
            bed(&mut h, &mut b, &mut toasts, t);
        }
        assert_eq!(toasts.history().count(), 1, "popover {with_popover}");
        assert!(b.dialog && b.cancelled == 0, "popover {with_popover}");
        assert_eq!(b.popover, with_popover, "the popover stays as it was");
    }
}

#[test]
fn a_dismiss_on_any_node_of_the_dialog_still_cancels_it_with_a_toast_up() {
    let (mut h, mut b, mut toasts) = open_bed(false);
    let (out, _) = bed(&mut h, &mut b, &mut toasts, 2000);
    let card = out
        .tree
        .nodes
        .iter()
        .find(|n| n.role == Role::Dialog)
        .expect("dialog")
        .id;
    h.input.access(AccessAction {
        target: card,
        action: Action::Dismiss,
        value: None,
    });
    bed(&mut h, &mut b, &mut toasts, 2010);
    assert_eq!(b.cancelled, 1);
}

#[test]
fn a_press_on_a_toast_button_leaves_the_dialogs_focus_and_enter_still_confirms() {
    let (mut h, mut b, mut toasts) = open_bed(false);
    let (out, _) = bed(&mut h, &mut b, &mut toasts, 2000);
    let held = out.focused;
    assert!(held.is_some(), "the dialog took focus");
    let c = centre(button_named(&out, "Open"));
    h.input.pointer_to(c.0, c.1);
    bed(&mut h, &mut b, &mut toasts, 2010);
    h.input.click(P);
    let (out, resp) = bed(&mut h, &mut b, &mut toasts, 2020);
    assert!(matches!(resp.action, Some((_, 2))), "{resp:?}");
    assert_eq!(out.focused, held);
    // The toast has gone; the dialog still has the keys.
    for t in [2300, 2600, 2900] {
        let (out, _) = bed(&mut h, &mut b, &mut toasts, t);
        assert_eq!(out.focused, held, "at {t}");
    }
    h.input.key_down(Key::Named(NamedKey::Enter), None);
    bed(&mut h, &mut b, &mut toasts, 3000);
    assert_eq!(b.confirmed, 1);
}

#[test]
fn an_accessibility_focus_on_a_toast_button_stays_out_of_a_dialog() {
    let (mut h, mut b, mut toasts) = open_bed(false);
    let (out, _) = bed(&mut h, &mut b, &mut toasts, 2000);
    let held = out.focused;
    let button = out
        .tree
        .nodes
        .iter()
        .find(|n| n.role == Role::Button && n.name == "Open")
        .expect("Open")
        .id;
    h.input.access(AccessAction {
        target: button,
        action: Action::Focus,
        value: None,
    });
    let (out, _) = bed(&mut h, &mut b, &mut toasts, 2010);
    assert_eq!(out.focused, held);

    // With no modal a toast button can hold focus, as Tab can put it there.
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    toasts.push(prompt("Update?"), ms(0));
    settle(&mut h, &mut toasts, 0);
    h.input.access(AccessAction {
        target: button,
        action: Action::Focus,
        value: None,
    });
    let (_, out, _) = frame(&mut h, &mut toasts, 500);
    assert_eq!(out.focused, Some(button));
}

#[test]
fn a_press_on_a_toast_leaves_a_text_field_its_focus() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let mut text = String::new();
    let field = Rect::new(20.0, 20.0, 200.0, 28.0);
    toasts.push(prompt("Update?"), ms(0));
    let run = |h: &mut UiHarness, toasts: &mut Toasts, text: &mut String, at: u64| {
        h.at_ms(at)
            .frame(|ui| {
                ui.text_field_with("field", field, text, &FieldOptions::default());
                ui.toasts("t", toasts);
            })
            .1
    };
    for t in [0, 300, 600] {
        run(&mut h, &mut toasts, &mut text, t);
    }
    h.input.pointer_to(30.0, 30.0);
    run(&mut h, &mut toasts, &mut text, 700);
    h.input.click(P);
    let out = run(&mut h, &mut toasts, &mut text, 710);
    let held = out.focused;
    assert!(held.is_some(), "the field took focus");
    let c = centre(button_named(&out, "Later"));
    h.input.pointer_to(c.0, c.1);
    run(&mut h, &mut toasts, &mut text, 720);
    h.input.click(P);
    run(&mut h, &mut toasts, &mut text, 730);
    for t in [1000, 1300] {
        assert_eq!(run(&mut h, &mut toasts, &mut text, t).focused, held);
    }
    h.input.key_down(Key::Character("a".to_owned()), Some("a"));
    run(&mut h, &mut toasts, &mut text, 1400);
    assert_eq!(text, "a", "typing goes on");
}

#[test]
fn a_chord_on_a_focused_toast_button_stays_a_shortcut() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    h.shortcuts.add(Binding::new(
        Scope::Global,
        Chord::key(Key::Named(NamedKey::Enter)).ctrl(),
        "go",
    ));
    toasts.push(prompt("Update?"), ms(0));
    settle(&mut h, &mut toasts, 0);
    h.input.key_down(Key::Named(NamedKey::Tab), None);
    let (_, out, _) = frame(&mut h, &mut toasts, 500);
    assert!(out.focused.is_some());
    h.input.set_modifiers(Modifiers {
        control: true,
        ..Modifiers::default()
    });
    h.input.key_down(Key::Named(NamedKey::Enter), None);
    let (_, out, r) = frame(&mut h, &mut toasts, 510);
    assert_eq!(r, ToastResponse::default());
    assert_eq!(out.fired.len(), 1);
}

/// The focused node names after `tabs` presses of Tab.
fn tab_walk(with_toast: bool, tabs: usize) -> Vec<Option<String>> {
    let mut h = UiHarness::new(WIN);
    let mut b = Bed {
        dialog: true,
        ..Bed::default()
    };
    let mut toasts = Toasts::default();
    if with_toast {
        toasts.push(prompt("Update?"), ms(0));
    }
    for t in [0, 300, 600, 900] {
        bed(&mut h, &mut b, &mut toasts, t);
    }
    let mut seen = Vec::new();
    for i in 0..tabs {
        h.input.key_down(Key::Named(NamedKey::Tab), None);
        let (out, _) = bed(&mut h, &mut b, &mut toasts, 1000 + 10 * i as u64);
        seen.push(
            out.focused
                .and_then(|f| out.tree.get(f))
                .map(|n| n.name.clone()),
        );
    }
    seen
}

#[test]
fn tab_does_not_reach_a_toast_under_a_dialog() {
    let without = tab_walk(false, 6);
    let with = tab_walk(true, 6);
    assert_eq!(without, with);
    assert!(without.iter().all(|n| n.as_deref() != Some("Open")));
    // And with no dialog, Tab does reach it.
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    toasts.push(prompt("Update?"), ms(0));
    settle(&mut h, &mut toasts, 0);
    h.input.key_down(Key::Named(NamedKey::Tab), None);
    let (_, out, _) = frame(&mut h, &mut toasts, 600);
    let f = out
        .focused
        .and_then(|f| out.tree.get(f))
        .map(|n| n.name.clone());
    assert!(
        matches!(f.as_deref(), Some("Dismiss" | "Later" | "Open")),
        "{f:?}"
    );
}

#[test]
fn escape_closes_the_dialog_with_or_without_a_prompt_toast_up() {
    for with_toast in [false, true] {
        let mut h = UiHarness::new(WIN);
        let mut b = Bed {
            dialog: true,
            ..Bed::default()
        };
        let mut toasts = Toasts::default();
        if with_toast {
            toasts.push(prompt("Update?"), ms(0));
        }
        for t in [0, 300, 600, 900] {
            bed(&mut h, &mut b, &mut toasts, t);
        }
        h.input.key_down(Key::Named(NamedKey::Escape), None);
        bed(&mut h, &mut b, &mut toasts, 1000);
        assert_eq!(b.cancelled, 1, "toast {with_toast}");
        assert_eq!(toasts.history().count(), 0, "the toast stays");
        assert!(!toasts.is_empty() || !with_toast);
    }
}

// -- the history ---------------------------------------------------------------------------------

fn history_frame(h: &mut UiHarness, toasts: &mut Toasts, at: u64) -> (Scene, FrameOutput) {
    h.at_ms(at).frame(|ui| {
        ui.toast_history("hist", Rect::new(0.0, 0.0, 320.0, 400.0), toasts);
    })
}

fn history_toasts() -> Toasts {
    let mut toasts = Toasts::default();
    for (kind, m) in [
        (ToastKind::Info, "first"),
        (ToastKind::Error, "second"),
        (ToastKind::Success, "third"),
    ] {
        let id = toasts.push(spec(kind, m), ms(0));
        toasts.dismiss(id, ms(0));
    }
    toasts
}

#[test]
fn the_history_lists_newest_first_and_clear_all_empties_it() {
    let mut toasts = history_toasts();
    let mut h = UiHarness::new((320.0, 400.0));
    history_frame(&mut h, &mut toasts, 0);
    let (_, out) = history_frame(&mut h, &mut toasts, 100);
    let list = out
        .tree
        .nodes
        .iter()
        .find(|n| n.role == Role::List)
        .expect("list");
    assert_eq!(list.name, "NOTIFICATIONS");
    let rows: Vec<&str> = out
        .tree
        .nodes
        .iter()
        .filter(|n| n.role == Role::ListItem)
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(rows, ["third", "second", "first"]);
    // The clear-all button is 24 square, at the header's right.
    let clear = button_named(&out, "Clear all");
    assert_eq!((clear.w, clear.h), (24.0, 24.0));
    assert_eq!(clear.right(), 320.0);
    let c = centre(clear);
    h.input.pointer_to(c.0, c.1);
    history_frame(&mut h, &mut toasts, 110);
    h.input.click(P);
    history_frame(&mut h, &mut toasts, 120);
    assert_eq!((toasts.history().count(), toasts.unread()), (0, 0));
    let (_, out) = history_frame(&mut h, &mut toasts, 130);
    assert!(out.tree.nodes.iter().all(|n| n.role != Role::ListItem));
}

#[test]
fn the_history_title_is_white() {
    // `NotificationHistoryPanelView.axaml:22`: `Foreground="White"`, not `fg_bright`.
    let mut toasts = history_toasts();
    let mut h = UiHarness::new((320.0, 400.0));
    let (scene, _) = history_frame(&mut h, &mut toasts, 0);
    let title = Spec::plain(
        "NOTIFICATIONS",
        Style::ui(DARK.font_m, DARK.on_accent).bold(),
    );
    let key = TextKey(title.cache_key(1.0));
    assert!(
        scene
            .in_order()
            .into_iter()
            .any(|l| l.texts.iter().any(|t| t.key == key))
    );
}

#[test]
fn the_history_rows_carry_the_detail_and_a_disabled_history_clears_nothing() {
    let mut toasts = Toasts::default();
    let id = toasts.push(
        ToastSpec {
            detail: Some("because".into()),
            ..spec(ToastKind::Error, "Failed")
        },
        ms(0),
    );
    toasts.dismiss(id, ms(0));
    let mut h = UiHarness::new((320.0, 400.0));
    history_frame(&mut h, &mut toasts, 0);
    let (_, out) = history_frame(&mut h, &mut toasts, 100);
    let row = out
        .tree
        .nodes
        .iter()
        .find(|n| n.role == Role::ListItem)
        .expect("row");
    assert_eq!(row.description.as_deref(), Some("because"));
    let c = centre(button_named(&out, "Clear all"));
    h.input.pointer_to(c.0, c.1);
    h.at_ms(110).frame(|ui| {
        ui.with_enabled(false, |ui| {
            ui.toast_history("hist", Rect::new(0.0, 0.0, 320.0, 400.0), &mut toasts);
        });
    });
    h.input.click(P);
    h.at_ms(120).frame(|ui| {
        ui.with_enabled(false, |ui| {
            ui.toast_history("hist", Rect::new(0.0, 0.0, 320.0, 400.0), &mut toasts);
        });
    });
    assert_eq!(toasts.history().count(), 1);
}

#[test]
fn a_long_log_shapes_only_the_lines_in_view() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let log: Vec<String> = (0..5000).map(|n| format!("line {n}")).collect();
    let id = toasts.push(
        ToastSpec {
            log,
            ..spec(ToastKind::Error, "Failed")
        },
        ms(0),
    );
    toasts.set_expanded(id, true);
    for t in [0, 100, 200, 300, 400, 500] {
        frame(&mut h, &mut toasts, t);
    }
    let (scene, _, _) = frame(&mut h, &mut toasts, 600);
    let texts = scene.merged(LayerId::Toast).texts.len();
    // The tail follows: about 13 lines fit 184 px at 14 px a line, plus the margin lines.
    assert!(texts < 40, "{texts} texts");
    let _ = FRAME;
}

// -- notices (`ToastExtras`), the hold, the drain line, the body action and the live rects ------

fn secs(n: u64) -> u64 {
    n * 1000
}

/// A frame with the pointer at `at` (or off the window), at `t` ms.
fn frame_at(
    h: &mut UiHarness,
    toasts: &mut Toasts,
    at: Option<(f32, f32)>,
    t: u64,
) -> (Scene, FrameOutput, ToastResponse) {
    match at {
        Some((x, y)) => h.input.pointer_to(x, y),
        None => h.input.pointer_to(2.0, 2.0),
    };
    frame(h, toasts, t)
}

/// A notice: an info toast with a 6 s timeout of its own.
fn timed(extras: ToastExtras) -> ToastExtras {
    extras.with_timeout(Duration::from_secs(6))
}

/// A bench with one settled notice, its id and its card's centre.
fn notice_bench(spec: ToastSpec, extras: ToastExtras) -> (UiHarness, Toasts, ToastId, (f32, f32)) {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push_with(spec, extras, ms(0));
    let (_, out) = settle(&mut h, &mut toasts, 0);
    let card = out.tree.get(node_of(id)).expect("toast node").bounds;
    (h, toasts, id, centre(card))
}

#[test]
fn a_notice_with_actions_times_out_on_its_own_timeout_and_a_plain_one_stays() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let plain = toasts.push(prompt("Plain"), ms(0));
    let notice = toasts.push_with(prompt("Notice"), timed(ToastExtras::new()), ms(0));
    frame(&mut h, &mut toasts, 0);
    frame(&mut h, &mut toasts, secs(6) - 1);
    assert!(toasts.is_live(notice));
    frame(&mut h, &mut toasts, secs(6));
    assert!(!toasts.is_live(notice), "gone at its timeout");
    assert!(toasts.is_live(plain));
    assert_eq!(toasts.history().count(), 1);
    frame(&mut h, &mut toasts, secs(6) + 100);
    let (_, out, _) = frame(&mut h, &mut toasts, secs(6) + 400);
    assert_eq!(shown(&out), ["Plain"], "the fade ended");
    let (_, out, _) = frame(&mut h, &mut toasts, 600_000);
    assert_eq!(shown(&out), ["Plain"], "a plain toast with actions stays");
}

#[test]
fn the_pointer_over_a_notice_holds_its_timer_and_leaving_resumes_it() {
    let (mut h, mut toasts, id, over) = notice_bench(info("Held"), timed(ToastExtras::new()));
    frame_at(&mut h, &mut toasts, None, 1000);
    frame_at(&mut h, &mut toasts, Some(over), secs(2));
    frame_at(&mut h, &mut toasts, Some(over), 3500);
    let (_, out, _) = frame_at(&mut h, &mut toasts, Some(over), 4900);
    assert_eq!(shown(&out), ["Held"]);
    // No frame is due while it is held.
    assert_eq!(
        frame_at(&mut h, &mut toasts, Some(over), 4950).1.repaint,
        Repaint::Idle
    );
    // It leaves at 5 s with 4 s left, so the deadline is 9 s, not a restart.
    frame_at(&mut h, &mut toasts, None, secs(5));
    let (_, out, _) = frame_at(&mut h, &mut toasts, None, 8900);
    assert_eq!(shown(&out), ["Held"]);
    assert!(toasts.is_live(id));
    let (_, out, _) = frame_at(&mut h, &mut toasts, None, secs(9));
    assert!(shown(&out).is_empty());
    assert!(!toasts.is_live(id));
    frame_at(&mut h, &mut toasts, None, secs(9) + 100);
    frame_at(&mut h, &mut toasts, None, secs(9) + 400);
    assert!(toasts.is_empty());
}

#[test]
fn the_pointer_over_a_button_of_a_notice_holds_it_too() {
    let (mut h, mut toasts, id, _) = notice_bench(prompt("Held"), timed(ToastExtras::new()));
    let (_, out, _) = frame_at(&mut h, &mut toasts, None, 1000);
    let open = centre(button_named(&out, "Open"));
    frame_at(&mut h, &mut toasts, Some(open), secs(2));
    let (_, out, _) = frame_at(&mut h, &mut toasts, Some(open), secs(8));
    assert_eq!(shown(&out), ["Held"], "held past its 6 s");
    assert!(toasts.is_live(id));
}

#[test]
fn a_finger_on_a_notice_does_not_hold_it() {
    let (mut h, mut toasts, id, over) = notice_bench(info("Touched"), timed(ToastExtras::new()));
    frame(&mut h, &mut toasts, 1000);
    h.input.touch_start(1, over.0, over.1);
    frame(&mut h, &mut toasts, secs(2));
    h.input.touch_end(1, over.0, over.1);
    frame(&mut h, &mut toasts, 2100);
    assert!(toasts.is_live(id), "a press on an inert body does nothing");
    frame(&mut h, &mut toasts, secs(6));
    assert!(!toasts.is_live(id), "the timer ran at its own pace");
}

/// The 2 px fills of `color` on the toast layer: their rects.
fn lines(scene: &Scene, color: [u8; 4]) -> Vec<[f32; 4]> {
    let want = premul(color);
    scene
        .merged(LayerId::Toast)
        .geometry
        .under
        .iter()
        .filter(|s| s.color == want && s.rect[3] == DRAIN)
        .map(|s| s.rect)
        .collect()
}

#[test]
fn the_drain_line_is_the_fraction_left_under_full_and_absent_otherwise() {
    for (tier, shown_line) in [
        (MotionTier::Full, true),
        (MotionTier::Reduced, false),
        (MotionTier::Off, false),
    ] {
        let mut h = UiHarness::new(WIN);
        h.tier = tier;
        let mut toasts = Toasts::default();
        let id = toasts.push_with(
            info("Draining"),
            timed(ToastExtras::new().with_drain(true)),
            ms(0),
        );
        settle(&mut h, &mut toasts, 0);
        let (scene, out, _) = frame(&mut h, &mut toasts, secs(3));
        let card = out.tree.get(node_of(id)).expect("node").bounds;
        let l = lines(&scene, DARK.toast_info);
        if shown_line {
            assert_eq!(l.len(), 1, "{tier:?}: {l:?}");
            assert!(
                (l[0][2] - card.w / 2.0).abs() <= 1.0,
                "{tier:?}: {}",
                l[0][2]
            );
            assert_eq!(l[0][0], card.x, "left-anchored");
            assert_eq!(l[0][1] + l[0][3], card.bottom(), "on the bottom edge");
        } else {
            assert!(l.is_empty(), "{tier:?}: {l:?}");
        }
    }
    // No line without the flag, none for an untimed toast; the timeout still runs under Reduced.
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    toasts.push_with(info("No line"), timed(ToastExtras::new()), ms(0));
    toasts.push_with(
        spec(ToastKind::Error, "Untimed"),
        ToastExtras::new().with_drain(true),
        ms(0),
    );
    settle(&mut h, &mut toasts, 0);
    let (scene, _, _) = frame(&mut h, &mut toasts, secs(3));
    assert!(lines(&scene, DARK.toast_info).is_empty());
    assert!(lines(&scene, DARK.toast_error).is_empty());
}

#[test]
fn a_running_drain_line_asks_for_frames_and_a_held_one_does_not() {
    let (mut h, mut toasts, _, over) =
        notice_bench(info("Draining"), timed(ToastExtras::new().with_drain(true)));
    let r = frame_at(&mut h, &mut toasts, None, 1000).1.repaint;
    assert!(
        matches!(r, Repaint::At(t) if t < ms(1100)),
        "the line moves, so the next busy tick is due soon: {r:?}"
    );
    frame_at(&mut h, &mut toasts, Some(over), secs(2));
    assert_eq!(
        frame_at(&mut h, &mut toasts, Some(over), 2500).1.repaint,
        Repaint::Idle,
        "held: still line, no deadline"
    );
}

#[test]
fn a_body_action_reports_and_the_toast_leaves_without_toggling_the_log() {
    let mut spec = info("Open me");
    spec.log = vec!["a log line".into()];
    let (mut h, mut toasts, id, _) = notice_bench(spec, ToastExtras::new().with_body_action(7));
    let (_, out, _) = frame(&mut h, &mut toasts, 600);
    let r = click(&mut h, &mut toasts, body_point(&out, id), 700);
    assert_eq!(r.action, Some((id, 7)));
    assert!(!toasts.is_live(id));
    assert!(!toasts.expanded(id));
    assert_eq!(toasts.history().count(), 1);
}

#[test]
fn toasts_with_places_the_stack_in_an_area_inset_by_the_margin() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    toasts.push(info("older"), ms(0));
    let newest = toasts.push(info("newest"), ms(0));
    let opts = ToastStackOptions::new()
        .with_area(Rect::new(100.0, 100.0, 600.0, 400.0))
        .with_margin(22.0);
    let mut out = None;
    for at in [0, 100, 200, 400] {
        out = Some(h.at_ms(at).frame(|ui| {
            ui.toasts_with("t", &mut toasts, &opts);
        }));
    }
    let tree = out.expect("frames").1.tree;
    let b = tree.get(node_of(newest)).expect("node").bounds;
    assert!((b.right() - 678.0).abs() < 0.01, "{b:?}");
    assert!((b.bottom() - 478.0).abs() < 0.01, "{b:?}");
    // The default options are the window's bottom right with the margin of 16.
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(info("one"), ms(0));
    let (_, out) = settle(&mut h, &mut toasts, 0);
    let b = out.tree.get(node_of(id)).expect("node").bounds;
    assert_eq!((b.right(), b.bottom()), (WIN.0 - 16.0, WIN.1 - 16.0));
}

fn texts_of(scene: &Scene) -> Vec<(u64, f32, f32)> {
    scene
        .merged(LayerId::Toast)
        .texts
        .iter()
        .map(|t| (t.key.0, t.x, t.y))
        .collect()
}

fn key_of(text: &str, style: Style) -> u64 {
    Spec::plain(text, style).cache_key(1.0)
}

#[test]
fn the_header_shows_the_source_and_the_age_and_its_dismiss_button_dismisses() {
    let extras = ToastExtras::new()
        .with_source("build")
        .with_age("now")
        .with_title("Done")
        .with_timeout(Duration::from_secs(60));
    let (mut h, mut toasts, id, _) = notice_bench(info("It compiled"), extras);
    let (scene, out, _) = frame(&mut h, &mut toasts, 600);
    let t = texts_of(&scene);
    let at = |style: Style, text: &str| {
        let k = key_of(text, style);
        t.iter()
            .find(|i| i.0 == k)
            .unwrap_or_else(|| panic!("no {text}"))
    };
    let source = at(Style::ui(DARK.font_base, DARK.fg_primary).medium(), "build");
    let age = at(Style::ui(DARK.font_s, DARK.fg_muted), "now");
    let title = at(Style::ui(DARK.font_base, DARK.fg_bright).bold(), "Done");
    let message_key = Spec::plain("It compiled", Style::ui(DARK.font_base, DARK.fg_primary))
        .wrapped(294.0)
        .cache_key(1.0);
    let message = t
        .iter()
        .find(|i| i.0 == message_key)
        .expect("the message, wrapped at the inner width");
    assert!(age.1 > source.1, "the age is to the right of the source");
    assert!(
        source.2 < title.2 && title.2 < message.2,
        "header, title, message"
    );
    let card = out.tree.get(node_of(id)).expect("node").bounds;
    assert!(
        (title.1 - message.1).abs() < 0.01 && title.1 < card.x + 14.0,
        "the title and message start at the inner left edge"
    );
    // The header's dismiss button dismisses.
    let r = click(
        &mut h,
        &mut toasts,
        centre(button_named(&out, "Dismiss")),
        700,
    );
    assert_eq!(r, ToastResponse::default());
    assert!(!toasts.is_live(id));
    assert_eq!(toasts.history().count(), 1);
}

#[test]
fn a_notice_with_a_title_names_its_node_by_the_title() {
    let mut spec = info("The message");
    spec.detail = Some("The detail".into());
    let (mut h, mut toasts, id, _) = notice_bench(spec, ToastExtras::new().with_title("The title"));
    let (_, out, _) = frame(&mut h, &mut toasts, 600);
    let n = out.tree.get(node_of(id)).expect("node");
    assert_eq!(n.name, "The title");
    assert_eq!(n.description.as_deref(), Some("The message\nThe detail"));
    // A plain toast is named by its message as before.
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let id = toasts.push(info("Plain"), ms(0));
    let (_, out) = settle(&mut h, &mut toasts, 0);
    assert_eq!(out.tree.get(node_of(id)).expect("node").name, "Plain");
}

#[test]
fn a_title_and_no_header_keeps_the_glyph_row_and_grows_it() {
    let (mut h, mut toasts, _, _) = notice_bench(info("Msg"), ToastExtras::new().with_title("T"));
    let mut p = Toasts::default();
    p.push(info("Msg"), ms(0));
    let mut hp = UiHarness::new(WIN);
    settle(&mut hp, &mut p, 0);
    let (_, with, _) = frame(&mut h, &mut toasts, 600);
    let (_, plain, _) = frame(&mut hp, &mut p, 600);
    let height = |out: &FrameOutput| {
        out.tree
            .nodes
            .iter()
            .find(|n| matches!(n.role, Role::Status))
            .expect("toast")
            .bounds
            .h
    };
    assert!(height(&with) > height(&plain), "the title line adds height");
    // The Dismiss button stays in the first row, at the right.
    assert!(button_named(&with, "Dismiss").y < button_named(&with, "Dismiss").bottom());
}

#[test]
fn update_with_keeps_a_held_toast_held_with_its_remaining_time() {
    let (mut h, mut toasts, id, over) = notice_bench(info("Held"), timed(ToastExtras::new()));
    frame_at(&mut h, &mut toasts, Some(over), secs(2));
    toasts.update_with(
        id,
        info("Held, new age"),
        timed(ToastExtras::new().with_age("1m")),
    );
    let (_, out, _) = frame_at(&mut h, &mut toasts, Some(over), secs(8));
    assert_eq!(shown(&out), ["Held, new age"], "still held at 8 s");
    // It leaves at 8 s with the 4 s it had left.
    frame_at(&mut h, &mut toasts, None, secs(8));
    let (_, out, _) = frame_at(&mut h, &mut toasts, None, 11_900);
    assert_eq!(shown(&out).len(), 1);
    frame_at(&mut h, &mut toasts, None, secs(12));
    assert!(!toasts.is_live(id));
}

#[test]
fn update_with_keeps_the_deadline_of_a_timed_notice() {
    let (mut h, mut toasts, id, _) = notice_bench(info("Timed"), timed(ToastExtras::new()));
    frame_at(&mut h, &mut toasts, None, secs(3));
    toasts.update_with(
        id,
        info("Timed again"),
        timed(ToastExtras::new().with_age("3s")),
    );
    let (_, out, _) = frame_at(&mut h, &mut toasts, None, secs(6) - 1);
    assert_eq!(shown(&out), ["Timed again"]);
    frame_at(&mut h, &mut toasts, None, secs(6));
    assert!(!toasts.is_live(id), "gone at 6 s, not 9 s");
}

#[test]
fn update_keeps_a_notice_s_extras() {
    let (mut h, mut toasts, id, _) =
        notice_bench(info("One"), ToastExtras::new().with_title("Kept"));
    toasts.update(id, info("Two"));
    let (_, out, _) = frame(&mut h, &mut toasts, 600);
    let n = out.tree.get(node_of(id)).expect("node");
    assert_eq!(
        (n.name.as_str(), n.description.as_deref()),
        ("Kept", Some("Two"))
    );
}

#[test]
fn live_holds_each_drawn_toast_with_the_rect_its_body_takes_input_in() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let a = toasts.push(info("first"), ms(0));
    let b = toasts.push_with(info("second"), timed(ToastExtras::new()), ms(0));
    // Pushed, not yet drawn.
    assert!(toasts.is_live(a) && toasts.is_live(b));
    assert!(toasts.live().is_empty());
    let (_, out) = settle(&mut h, &mut toasts, 0);
    let live = toasts.live().to_vec();
    assert_eq!(
        live.iter().map(|l| l.0).collect::<Vec<_>>(),
        [a, b],
        "top to bottom"
    );
    for (id, rect) in &live {
        let card = out.tree.get(node_of(*id)).expect("node").bounds;
        for (x, y) in [
            (rect.x, card.x),
            (rect.y, card.y),
            (rect.w, card.w),
            (rect.h, card.h),
        ] {
            assert!((x - y).abs() < 0.01, "{id:?}: {rect:?} vs {card:?}");
        }
    }
    let r = live[0].1;
    assert!(toasts.contains(centre(r)));
    assert!(!toasts.contains((r.x - 1.0, r.y + r.h / 2.0)));
    assert!(!toasts.contains((r.x + r.w / 2.0, r.y - 1.0)));
    assert!(
        toasts.contains((r.x + 2.0, r.bottom() - 2.0)),
        "a corner of the card"
    );
    // The model starts empty.
    let none = Toasts::default();
    assert!(none.live().is_empty() && !none.contains((1.0, 1.0)) && !none.is_live(ToastId(1)));
}

#[test]
fn live_and_is_live_follow_a_toast_out_by_a_timeout_its_x_or_an_action() {
    let mut h = UiHarness::new(WIN);
    let mut toasts = Toasts::default();
    let timed_out = toasts.push(info("times out"), ms(0));
    let closed = toasts.push(spec(ToastKind::Error, "closed"), ms(0));
    let acted = toasts.push(prompt("acted"), ms(0));
    settle(&mut h, &mut toasts, 0);
    assert_eq!(toasts.live().len(), 3);
    // The timeout: not live from the frame it starts leaving.
    frame(&mut h, &mut toasts, secs(4));
    assert!(!toasts.is_live(timed_out));
    assert!(toasts.live().iter().all(|l| l.0 != timed_out));
    frame(&mut h, &mut toasts, secs(4) + 400);
    // The x.
    let (_, out, _) = frame(&mut h, &mut toasts, secs(5));
    let x = out
        .tree
        .nodes
        .iter()
        .filter(|n| n.role == Role::Button && n.name == "Dismiss")
        .map(|n| centre(n.bounds))
        .next()
        .expect("a dismiss button");
    click(&mut h, &mut toasts, x, secs(5) + 50);
    assert!(!toasts.is_live(closed));
    assert!(toasts.live().iter().all(|l| l.0 != closed));
    frame(&mut h, &mut toasts, secs(5) + 500);
    // An action.
    let (_, out, _) = frame(&mut h, &mut toasts, secs(6));
    let r = click(
        &mut h,
        &mut toasts,
        centre(button_named(&out, "Open")),
        secs(6) + 50,
    );
    assert_eq!(r.action, Some((acted, 2)));
    assert!(!toasts.is_live(acted));
    assert!(toasts.live().is_empty());
    // Once the last fade ends the model is empty too.
    frame(&mut h, &mut toasts, secs(6) + 100);
    frame(&mut h, &mut toasts, secs(6) + 600);
    assert!(toasts.is_empty() && toasts.live().is_empty());
}

#[test]
fn a_notice_body_with_no_action_and_no_log_is_inert_and_a_plain_one_toggles() {
    let (mut h, mut toasts, id, _) = notice_bench(info("Inert"), ToastExtras::new());
    let (_, out, _) = frame(&mut h, &mut toasts, 600);
    let r = click(&mut h, &mut toasts, body_point(&out, id), 700);
    assert_eq!(r, ToastResponse::default());
    assert!(!toasts.expanded(id));
    // The timer ran on (the pointer held it from 700 ms to 1 s: 300 ms late): info's 4 s.
    frame_at(&mut h, &mut toasts, None, 1000);
    frame_at(&mut h, &mut toasts, None, secs(4) + 300 - 1);
    assert!(toasts.is_live(id));
    frame_at(&mut h, &mut toasts, None, secs(4) + 300);
    assert!(
        !toasts.is_live(id),
        "gone on schedule, no expand waited for"
    );
    // The node offers no log toggle, and no default action.
    let mut h = UiHarness::new(WIN);
    let mut t2 = Toasts::default();
    let id2 = t2.push_with(info("Inert"), ToastExtras::new(), ms(0));
    let (_, out) = settle(&mut h, &mut t2, 0);
    let n = out.tree.get(node_of(id2)).expect("node");
    assert!(!n.actions.contains(Action::Default) && !n.actions.contains(Action::Expand));
    assert!(n.actions.contains(Action::Dismiss));
    // A notice with a log toggles it, as a plain toast does.
    let mut with_log = info("Logged");
    with_log.log = vec!["line".into()];
    let (mut h, mut toasts, id, _) = notice_bench(with_log, ToastExtras::new());
    let (_, out, _) = frame(&mut h, &mut toasts, 600);
    click(&mut h, &mut toasts, body_point(&out, id), 700);
    assert!(toasts.expanded(id));
    let mut h = UiHarness::new(WIN);
    let mut plain = Toasts::default();
    let id = plain.push(info("Plain"), ms(0));
    let (_, out) = settle(&mut h, &mut plain, 0);
    click(&mut h, &mut plain, body_point(&out, id), 700);
    assert!(plain.expanded(id));
}
