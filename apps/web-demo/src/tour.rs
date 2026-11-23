//! The attract tour: the demo plays itself through the six scenes with a hand that taps the rail (or
//! the tab bar), and checks on every scene that it came to rest and that its own motion ran. It is a
//! `playback::Script` (D415) built from the stage's layout, so one script fits a desktop window and a
//! phone; the scenes' own autopilots and hands do the rest (the toolkit's gestures are the same
//! events a real finger sends).
//!
//! A real pointer, touch or key event stops the tour at once (a scripted contact still down is
//! cancelled where it was); after [`IDLE`] without one it starts again from the scene the person left
//! the stage on. A window that changes size while it plays starts it again from the current scene,
//! because its points came from the old layout. The marks name each scene's states, so
//! `demo mark glass ok` is "the Glass scene landed" and `demo mark glass-live ok` is "its autopilot
//! ran for a while".
//!
//! Time is app time: the player latches its start at the first frame, a stall (a hidden tab) is not
//! played back, and nothing here reads a clock.

use std::time::Duration;

use lune_ui::input::{Event, Key, Modifiers, NamedKey, TouchEvent, TouchPhase};
use lune_ui::layout::Insets;
use lune_ui::playback::{Advance, Player, Script};

use crate::scenes::text;
use crate::scenes::theme::Look;
use crate::scenes::{SceneId, Scenes, is_composition};
use crate::stage::Layout;

/// How long after the last real event the tour starts again.
pub const IDLE: Duration = Duration::from_secs(10);
/// A gap between two frames above this is a stall (a hidden tab), not played back.
pub const STALL: Duration = Duration::from_secs(2);
/// The first frames of a page load go by before the first gesture.
const LOAD: Duration = Duration::from_millis(1000);
/// After a restart or a resize: the stage settles before the first mark.
const RESUME: Duration = Duration::from_millis(800);
/// After an Escape that closes what a person left open: the scenes' autopilots hold back for
/// [`crate::scenes::RESUME`] after any key, so the tour waits that out and a little more.
const AFTER_ESCAPE: Duration = Duration::from_millis(5300);
/// The pause between a scene's last mark and the next tap, so a recording reads.
const BEAT: Duration = Duration::from_millis(400);
/// From a tap to the mark that says the scene landed: the push is 350 ms.
const LAND: Duration = Duration::from_millis(1200);
/// What the tour holds after its last mark before it wraps.
const END_HOLD: Duration = Duration::from_millis(300);

/// The name of the mark that closes a cycle: the stage is back on the scene it started on.
pub const END: &str = "end";

/// How a run plays the tour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Play {
    /// Play it again when it ends (the attract loop); off, it plays once.
    pub repeat: bool,
    /// Draw a disc under each scripted contact (`?touches`).
    pub touches: bool,
}

/// One mark the tour passed and what its check said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkResult {
    /// The mark's name.
    pub name: String,
    /// `Ok`, or what differed from the state the mark names.
    pub result: Result<(), String>,
}

/// The mark of scene `scene` landing.
pub fn land_mark(scene: SceneId) -> &'static str {
    scene.slug()
}

/// The mark of scene `scene`'s own motion having run.
pub fn live_mark(scene: SceneId) -> &'static str {
    match scene {
        SceneId::Glass => "glass-live",
        SceneId::Controls => "controls-live",
        SceneId::Motion => "motion-live",
        SceneId::Data => "data-live",
        SceneId::Text => "text-live",
        SceneId::Theme => "theme-live",
    }
}

/// How long the tour stays on a scene after it landed, before its `-live` mark: long enough for the
/// autopilot's steps (`STEP` of each scene) to make [`acts_needed`] changes, and for the longer
/// motions (the Text scene types a field, selects, and starts on the Markdown) to show.
pub fn dwell(scene: SceneId) -> Duration {
    Duration::from_millis(match scene {
        SceneId::Glass => 5000,
        SceneId::Controls => 4500,
        SceneId::Motion => 7000,
        SceneId::Data => 9000,
        SceneId::Text => 11000,
        SceneId::Theme => 9000,
    })
}

/// How many changes the scene's autopilot must have made in the dwell, with room for the frames'
/// jitter: each is at most the dwell over the scene's step, less one.
pub fn acts_needed(scene: SceneId) -> u64 {
    match scene {
        // 5.0 s over 1.6 s.
        SceneId::Glass => 2,
        // 4.5 s over 1.2 s.
        SceneId::Controls => 2,
        // 7 s over 1 s.
        SceneId::Motion => 5,
        // 9 s over 1.5 s.
        SceneId::Data => 4,
        // The field (19 acts), the code view (2) and the Markdown source (2) before its typing.
        SceneId::Text => 24,
        // 9 s over 2.8 s.
        SceneId::Theme => 2,
    }
}

/// The scenes in the order a tour that starts on `from` visits them.
pub fn order(from: SceneId) -> [SceneId; 6] {
    std::array::from_fn(|k| SceneId::from_index((from.index() + k) % SceneId::ALL.len()))
}

/// The names of the marks of a cycle that starts on `from`, in order.
pub fn marks_from(from: SceneId) -> Vec<&'static str> {
    let mut marks = Vec::new();
    for scene in order(from) {
        marks.push(land_mark(scene));
        marks.push(live_mark(scene));
    }
    marks.push(END);
    marks
}

/// How the cycle begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opening {
    /// The page just loaded.
    Load,
    /// The tour starts again over a stage a person used: the stage settles first.
    Resume,
    /// As `Resume`, over a dialog or a sheet the person left open: Escape closes it first.
    ResumeOver,
    /// The layout changed under it: the stage is as the tour left it.
    Relayout,
}

/// The centre of the rail row or the tab of `scene`.
pub fn nav_point(l: &Layout, scene: SceneId) -> (f32, f32) {
    let r = if l.wide {
        l.rail_rows()[scene.index()]
    } else {
        l.tab_cells()[scene.index()]
    };
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn key(key: NamedKey, pressed: bool) -> Event {
    Event::Key {
        key: Key::Named(key),
        pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    }
}

/// One cycle of the tour for a window laid out as `l`, starting on `from`: the stage is on `from`
/// and at rest; the hand taps each next scene's rail row or tab, the scene lands, the tour stays
/// through its dwell, and the cycle ends back on `from`.
fn script(l: &Layout, from: SceneId, opening: Opening) -> Script {
    let mut s = Script::new();
    match opening {
        Opening::Load => {
            s.wait(LOAD);
        }
        Opening::ResumeOver => {
            s.event(key(NamedKey::Escape, true));
            s.event(key(NamedKey::Escape, false));
            s.wait(AFTER_ESCAPE);
        }
        Opening::Resume | Opening::Relayout => {
            s.wait(RESUME);
        }
    }
    for (k, scene) in order(from).into_iter().enumerate() {
        if k > 0 {
            s.wait(BEAT).tap(nav_point(l, scene)).wait(LAND);
        }
        s.mark(land_mark(scene));
        s.wait(dwell(scene)).mark(live_mark(scene));
    }
    s.wait(BEAT).tap(nav_point(l, from)).wait(LAND).mark(END);
    s.end_hold(END_HOLD);
    s
}

/// Whether `e` is something a person did: a pointer move, a button, a wheel turn, a touch, a key,
/// typed or composed text. An input method switching on or off is the app's own focus, not a person.
fn is_real(e: &Event) -> bool {
    matches!(
        e,
        Event::PointerMoved { .. }
            | Event::PointerButton { .. }
            | Event::Scroll { .. }
            | Event::Touch(_)
            | Event::Key { .. }
            | Event::Text(_)
    ) || matches!(e, Event::Ime(i) if is_composition(i))
}

/// The name of the kind of event `e`, for the tour's notes.
fn kind_of(e: &Event) -> &'static str {
    match e {
        Event::PointerMoved { .. } => "a pointer move",
        Event::PointerButton { .. } => "a pointer button",
        Event::Scroll { .. } => "a wheel turn",
        Event::Touch(_) => "a touch",
        Event::Key { .. } => "a key",
        Event::Text(_) => "typed text",
        Event::Ime(_) => "a composition event",
        _ => "an event",
    }
}

/// Whether the stage at `position` rests on scene `index`.
fn at_rest(position: f32, index: usize) -> bool {
    (position - index as f32).abs() < 1e-3
}

/// What the checks compare against, taken when a scene landed.
#[derive(Debug, Clone, Default)]
struct Baseline {
    acted: [u64; 6],
    look: Option<Look>,
}

/// The tour's player, and what it needs between frames.
#[derive(Debug)]
pub struct Tour {
    play: Play,
    /// The scene the current cycle started on.
    from: SceneId,
    opening: Opening,
    player: Option<Player>,
    /// The window and safe area the script was built for.
    built_for: Option<((f32, f32), Insets)>,
    /// A person took the stage: the player is not advanced.
    stopped: bool,
    /// App time of the last real event while stopped.
    idle_since: Duration,
    last_now: Option<Duration>,
    baseline: Baseline,
    /// The marks of the current cycle that were ok, and all that passed.
    ok: u32,
    total: u32,
    /// What the tour did on its own account since the last [`Self::take_notes`] (a stop, a restart).
    notes: Vec<String>,
}

impl Tour {
    /// A tour that starts on `from` at the first frame.
    pub fn new(play: Play, from: SceneId) -> Self {
        Self {
            play,
            from,
            opening: Opening::Load,
            player: None,
            built_for: None,
            stopped: false,
            idle_since: Duration::ZERO,
            last_now: None,
            baseline: Baseline::default(),
            ok: 0,
            total: 0,
            notes: Vec::new(),
        }
    }

    /// The lines the tour has to say (`tour stopped by ...`, `tour restarted on ...`), oldest first;
    /// the shell prints them.
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// How the run plays it.
    pub fn play(&self) -> Play {
        self.play
    }

    /// The player, for the touch discs.
    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    /// Whether a person has taken the stage and the tour waits for them to leave it.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Looks at the frame's own input, before anything is added to it. A real event stops the tour
    /// (and cancels a scripted contact still down, appended to `events`) or, while it is stopped,
    /// starts the idle count again. Returns whether there was one.
    pub fn observe(&mut self, now: Duration, events: &mut Vec<Event>) -> bool {
        let Some(first) = events.iter().find(|e| is_real(e)) else {
            return false;
        };
        if !self.stopped {
            self.notes
                .push(format!("tour stopped by {}", kind_of(first)));
            self.stopped = true;
            if let Some(player) = &self.player {
                for c in player.contacts(now).iter().filter(|c| c.down) {
                    events.push(Event::Touch(TouchEvent::new(
                        c.id,
                        TouchPhase::Cancelled,
                        c.pos,
                    )));
                }
            }
        }
        self.idle_since = now;
        true
    }

    /// Stops the tour's clock at `now` (the app went to the background).
    pub fn pause(&mut self, now: Duration) {
        if let Some(player) = &mut self.player {
            player.pause(now);
        }
    }

    /// Starts the tour's clock again at `now`: the time spent paused is not played.
    pub fn resume(&mut self, now: Duration) {
        if let Some(player) = &mut self.player {
            player.resume(now);
        }
    }

    /// Appends the events due at `now` to `events` and returns the marks passed. `want` is the scene
    /// the stage is on, `size` and `safe` its window: a stopped tour that has been left alone for
    /// [`IDLE`] starts again from `want` (with an Escape first when `modal`: a dialog or a sheet is
    /// open), and so does one whose window changed.
    pub fn advance(
        &mut self,
        now: Duration,
        size: (f32, f32),
        safe: Insets,
        want: SceneId,
        modal: bool,
        events: &mut Vec<Event>,
    ) -> Advance {
        let last = self.last_now.replace(now);
        if self.stopped {
            if now.saturating_sub(self.idle_since) < IDLE {
                return Advance::default();
            }
            self.stopped = false;
            self.notes
                .push(format!("tour restarted on {} (idle)", want.slug()));
            self.from = want;
            self.opening = if modal {
                Opening::ResumeOver
            } else {
                Opening::Resume
            };
            self.player = None;
        }
        if self.player.is_some() && self.built_for != Some((size, safe)) {
            // The points came from the old layout.
            self.notes.push(format!(
                "tour restarted on {} (window changed)",
                want.slug()
            ));
            self.from = want;
            self.opening = Opening::Relayout;
            self.player = None;
        }
        if self.player.is_none() {
            let tour = script(&Layout::new(size, safe), self.from, self.opening);
            self.player = Some(Player::new(tour).with_repeat(self.play.repeat));
            self.built_for = Some((size, safe));
            self.ok = 0;
            self.total = 0;
            self.baseline = Baseline::default();
        }
        let Some(player) = &mut self.player else {
            return Advance::default();
        };
        if let Some(last) = last
            && now.saturating_sub(last) > STALL
        {
            // A hidden tab: the script's clock did not run.
            player.pause(last);
            player.resume(now);
        }
        let adv = player.advance(now, events);
        if adv.restarted {
            // Each cycle of the loop begins in the same look.
            self.ok = 0;
            self.total = 0;
        }
        adv
    }

    /// Checks `mark` against the state the frame drew. `want` is the scene the stage was asked for,
    /// `position` where the push is (the index of the scene at rest, or between two).
    pub fn check(
        &mut self,
        mark: &str,
        scenes: &Scenes,
        want: SceneId,
        position: f32,
    ) -> MarkResult {
        let result = self.check_inner(mark, scenes, want, position);
        self.total += 1;
        if result.is_ok() {
            self.ok += 1;
        }
        MarkResult {
            name: mark.to_owned(),
            result,
        }
    }

    /// How many marks of the current cycle were ok, and how many passed: `demo end <ok>/<total>`.
    pub fn tally(&self) -> (u32, u32) {
        (self.ok, self.total)
    }

    fn check_inner(
        &mut self,
        mark: &str,
        scenes: &Scenes,
        want: SceneId,
        position: f32,
    ) -> Result<(), String> {
        let mut bad: Vec<String> = Vec::new();
        macro_rules! want {
            ($ok:expr, $($arg:tt)*) => {
                let held: bool = $ok;
                if !held {
                    bad.push(format!($($arg)*));
                }
            };
        }
        if mark == END {
            want!(
                want == self.from,
                "on {} not {}",
                want.slug(),
                self.from.slug()
            );
            want!(
                at_rest(position, self.from.index()),
                "the push is at {position:.3}, not at rest on {}",
                self.from.slug()
            );
        } else if let Some(scene) = SceneId::from_slug(mark) {
            let i = scene.index();
            want!(want == scene, "on {} not {}", want.slug(), scene.slug());
            want!(
                at_rest(position, i),
                "the push is at {position:.3}, not at rest on {}",
                scene.slug()
            );
            want!(
                scenes.targets(scene).is_some_and(|t| !t.all().is_empty()),
                "{} drew no control",
                scene.slug()
            );
            self.baseline.acted[i] = scenes.pilot(scene).map_or(0, |p| p.acted);
            self.baseline.look = Some(scenes.theme.look);
        } else if let Some(scene) = mark.strip_suffix("-live").and_then(SceneId::from_slug) {
            let i = scene.index();
            want!(want == scene, "on {} not {}", want.slug(), scene.slug());
            want!(
                at_rest(position, i),
                "the push is at {position:.3}, not at rest on {}",
                scene.slug()
            );
            let acted = scenes.pilot(scene).map_or(0, |p| p.acted);
            let made = acted.saturating_sub(self.baseline.acted[i]);
            want!(
                made >= acts_needed(scene),
                "the autopilot made {made} changes in {} s, {} needed",
                dwell(scene).as_secs_f32(),
                acts_needed(scene)
            );
            match scene {
                SceneId::Text => {
                    want!(
                        scenes.text.field == text::phrase(0),
                        "the field holds {:?}, not {:?}",
                        scenes.text.field,
                        text::phrase(0)
                    );
                }
                SceneId::Theme => {
                    want!(
                        self.baseline.look != Some(scenes.theme.look),
                        "the look did not change"
                    );
                }
                _ => {}
            }
        } else {
            bad.push(format!("no mark named {mark:?}"));
        }
        if bad.is_empty() {
            Ok(())
        } else {
            Err(bad.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cycle_has_thirteen_marks_and_ends_with_the_end_mark() {
        let marks = marks_from(SceneId::Glass);
        assert_eq!(marks.len(), 13);
        assert_eq!(marks.first(), Some(&"glass"));
        assert_eq!(marks.last(), Some(&END));
        let from_text = marks_from(SceneId::Text);
        assert_eq!(from_text.first(), Some(&"text"));
        assert_eq!(from_text[9], "motion-live");
    }

    #[test]
    fn the_order_wraps_around_the_six_scenes() {
        let o = order(SceneId::Data);
        assert_eq!(
            o.map(SceneId::slug),
            ["data", "text", "theme", "glass", "controls", "motion"]
        );
    }
}
