//! Scripted input in app time: a [`Script`] of timed touch gestures, marks and raw events, and a
//! [`Player`] that hands the due events to the frame through [`RawInput::events`](crate::input::RawInput::events), for demos,
//! recordings and tests that must not depend on the frame rate (D415).
//!
//! A script is built with a cursor: each gesture starts where the previous one ended and moves the
//! cursor to its own end. A gesture expands at build time into [`Event::Touch`] samples every
//! [`SAMPLE`], so the events are the same whatever rate the app draws at; a 60 Hz app sees the last
//! sample of each frame, as it does for a finger. Nothing here reads a clock: the app passes its
//! time to [`Player::advance`], which appends the events due by then and returns the marks passed
//! and the time the player needs the next frame at (give it to
//! [`Ui::request_repaint_at`](crate::ui::Ui::request_repaint_at)).
//!
//! The marks are returned in the same call as the events up to them. An app calls
//! [`Player::advance`] before [`Ui::begin`](crate::ui::Ui::begin), so the frame that follows reads
//! those events and the state after that frame's draw reflects them: check what a mark names
//! there, on the frame it passes.
//!
//! [`draw_touches`] draws a disc under each scripted contact, for a recording; it is opt-in and is
//! written with public painter calls only, so an app could draw its own.
//!
//! ```
//! use std::time::Duration;
//! use lune_ui_core::playback::{Player, Script};
//!
//! let mut script = Script::new();
//! script.tap((20.0, 30.0)).wait(Duration::from_millis(200)).mark("after the tap");
//! let mut player = Player::new(script);
//! let mut events = Vec::new();
//! let adv = player.advance(Duration::from_secs(1), &mut events);
//! assert_eq!(events.len(), 1); // the press; the lift is 80 ms later
//! assert!(adv.next.is_some());
//! ```

use std::time::Duration;

use crate::color::with_alpha;
use crate::input::{Event, TouchEvent, TouchId, TouchPhase};
use crate::motion::MotionTier;
use crate::paint::Rect;
use crate::scene::LayerId;
use crate::ui::Ui;

/// The spacing of the samples a gesture expands into: 8 ms, 120 Hz.
pub const SAMPLE: Duration = Duration::from_millis(8);

/// How long a [`Script::tap`] holds the finger down: well under the long press.
pub const TAP_HOLD: Duration = Duration::from_millis(80);

/// How long a [`Script::drag`] holds still at its end point before the lift, so the release
/// velocity reads zero and nothing flings.
pub const DRAG_SETTLE: Duration = Duration::from_millis(80);

/// How long a lifted contact fades out in [`draw_touches`] at [`MotionTier::Full`].
pub const FADE: Duration = Duration::from_millis(250);

/// The raw identifier of a script's first gesture's contact; the n-th is this plus n.
pub const TOUCH_ID_BASE: u64 = 1 << 48;

/// What a script holds after its last cue unless [`Script::end_hold`] says otherwise.
const END_HOLD: Duration = Duration::from_millis(1500);

/// The disc a contact draws, in logical points across.
const DISC: f32 = 44.0;

#[derive(Debug, Clone)]
enum CueKind {
    Event(Event),
    Mark(String),
}

#[derive(Debug, Clone)]
struct Cue {
    at: Duration,
    kind: CueKind,
}

/// A list of timed cues built by a cursor: touch gestures, raw events and named marks. Each
/// builder method starts at the cursor and returns `&mut Self`, so calls chain.
#[derive(Debug, Clone, Default)]
pub struct Script {
    /// In builder order, which is time order: the cursor never moves back.
    cues: Vec<Cue>,
    cursor: Duration,
    end_hold: Option<Duration>,
    touches: u64,
}

impl Script {
    /// An empty script with its cursor at 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the cursor on by `d` without a cue.
    pub fn wait(&mut self, d: Duration) -> &mut Self {
        self.cursor += d;
        self
    }

    /// A raw event at the cursor. The cursor does not move.
    pub fn event(&mut self, event: Event) -> &mut Self {
        self.cue(self.cursor, event);
        self
    }

    /// A named mark at the cursor, returned by [`Player::advance`] in [`Advance::marks`]. The
    /// cursor does not move; a mark after a gesture comes after its last event.
    pub fn mark(&mut self, name: impl Into<String>) -> &mut Self {
        self.cues.push(Cue {
            at: self.cursor,
            kind: CueKind::Mark(name.into()),
        });
        self
    }

    /// A tap at `pos`: the contact goes down at the cursor and lifts [`TAP_HOLD`] later.
    pub fn tap(&mut self, pos: (f32, f32)) -> &mut Self {
        self.press(pos, TAP_HOLD)
    }

    /// A press at `pos` held for `hold`: a long press when `hold` is past the long-press time.
    pub fn press(&mut self, pos: (f32, f32), hold: Duration) -> &mut Self {
        let id = self.fresh();
        let start = self.cursor;
        self.touch(id, TouchPhase::Started, start, pos);
        self.cursor = start + hold;
        self.touch(id, TouchPhase::Ended, self.cursor, pos);
        self
    }

    /// A swipe from `from` to `to` over `d`: a straight line sampled every [`SAMPLE`], lifted at
    /// once at `to`, so it leaves the speed it travelled at (a fling).
    pub fn swipe(&mut self, from: (f32, f32), to: (f32, f32), d: Duration) -> &mut Self {
        let id = self.stroke(from, to, d, false);
        self.touch(id, TouchPhase::Ended, self.cursor, to);
        self
    }

    /// A drag from `from` to `to` over `d`, easing in and out, then held still at `to` for
    /// [`DRAG_SETTLE`] before the lift, so the lift leaves no speed and nothing flings.
    pub fn drag(&mut self, from: (f32, f32), to: (f32, f32), d: Duration) -> &mut Self {
        let id = self.stroke(from, to, d, true);
        let end = self.cursor;
        let mut t = SAMPLE;
        while t < DRAG_SETTLE {
            self.touch(id, TouchPhase::Moved, end + t, to);
            t += SAMPLE;
        }
        self.touch(id, TouchPhase::Moved, end + DRAG_SETTLE, to);
        self.cursor = end + DRAG_SETTLE;
        self.touch(id, TouchPhase::Ended, self.cursor, to);
        self
    }

    /// How long the script holds after its last cue before it is over (1.5 s unless set).
    pub fn end_hold(&mut self, d: Duration) -> &mut Self {
        self.end_hold = Some(d);
        self
    }

    /// The time of the last cue plus [`Self::end_hold`]: one cycle of a repeating player.
    pub fn length(&self) -> Duration {
        self.cues.last().map_or(Duration::ZERO, |c| c.at) + self.end_hold.unwrap_or(END_HOLD)
    }

    /// Where the next gesture starts.
    pub fn cursor(&self) -> Duration {
        self.cursor
    }

    fn fresh(&mut self) -> TouchId {
        let id = TouchId::new(TOUCH_ID_BASE + self.touches);
        self.touches += 1;
        id
    }

    fn cue(&mut self, at: Duration, event: Event) {
        self.cues.push(Cue {
            at,
            kind: CueKind::Event(event),
        });
    }

    fn touch(&mut self, id: TouchId, phase: TouchPhase, at: Duration, pos: (f32, f32)) {
        self.cue(at, Event::Touch(TouchEvent::new(id, phase, pos)));
    }

    /// The press and the moves of a swipe or a drag; the cursor ends at `to`'s time and the
    /// lift is the caller's. The last move is exactly at `to`.
    fn stroke(&mut self, from: (f32, f32), to: (f32, f32), d: Duration, eased: bool) -> TouchId {
        let id = self.fresh();
        let start = self.cursor;
        self.touch(id, TouchPhase::Started, start, from);
        let n = d.as_nanos().div_ceil(SAMPLE.as_nanos()).max(1);
        for i in 1..=n {
            let off = Duration::from_nanos((SAMPLE.as_nanos() * i).min(d.as_nanos()) as u64);
            let pos = if i == n {
                to
            } else {
                let f = off.as_secs_f64() / d.as_secs_f64();
                let f = if eased { f * f * (3.0 - 2.0 * f) } else { f };
                (lerp(from.0, to.0, f), lerp(from.1, to.1, f))
            };
            self.touch(id, TouchPhase::Moved, start + off, pos);
        }
        self.cursor = start + d;
        id
    }
}

fn lerp(a: f32, b: f32, f: f64) -> f32 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * f) as f32
}

/// What one [`Player::advance`] call found.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct Advance {
    /// The names of the marks passed, in script order.
    pub marks: Vec<String>,
    /// The app time the player needs a frame at next: the next cue, the wrap of a repeating
    /// script, or the end of a running fade, whichever is first. `None` once the script is
    /// finished with nothing fading, while paused, and before the first call.
    pub next: Option<Duration>,
    /// A repeating script wrapped in this call.
    pub restarted: bool,
    /// The script is over (it never is while it repeats).
    pub finished: bool,
}

/// A scripted contact as [`draw_touches`] sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Contact {
    /// The contact's identifier.
    pub id: TouchId,
    /// Its last position, logical pixels.
    pub pos: (f32, f32),
    /// The contact is down; a lifted one is fading.
    pub down: bool,
    /// 1 while down, then falling linearly to 0 over [`FADE`] after the lift.
    pub alpha: f32,
}

#[derive(Debug, Clone)]
struct Track {
    id: TouchId,
    pos: (f32, f32),
    down: bool,
    /// The app time of the lift; meaningful only when not down.
    up_at: Duration,
}

/// Plays a [`Script`] against the app's time. It latches its start at the first
/// [`Self::advance`], not at construction, so a first frame that comes late starts the script
/// when it comes.
#[derive(Debug, Clone)]
pub struct Player {
    script: Script,
    repeat: bool,
    start: Option<Duration>,
    paused_at: Option<Duration>,
    /// The total time spent paused: script time is app time less the start less this.
    shift: Duration,
    idx: usize,
    cycle: u32,
    finished: bool,
    last_now: Duration,
    tracks: Vec<Track>,
}

impl Player {
    /// A player of `script`, not yet started.
    pub fn new(script: Script) -> Self {
        Self {
            script,
            repeat: false,
            start: None,
            paused_at: None,
            shift: Duration::ZERO,
            idx: 0,
            cycle: 0,
            finished: false,
            last_now: Duration::ZERO,
            tracks: Vec::new(),
        }
    }

    /// Whether the script wraps after [`Script::length`] and plays again. The app decides what a
    /// restart resets ([`Advance::restarted`]).
    pub fn with_repeat(mut self, repeat: bool) -> Self {
        self.repeat = repeat;
        self
    }

    /// Appends the events due at app time `now` to `events`, in script order, and returns the
    /// marks passed. The first call starts the script at `now`. While paused, or when `now` is
    /// earlier than the last call's, nothing is appended. A repeating script that wraps first
    /// cancels any contact still down, then goes on with the next cycle's cues due by `now`; a
    /// call that finds whole cycles past skips them rather than replay them in one frame.
    pub fn advance(&mut self, now: Duration, events: &mut Vec<Event>) -> Advance {
        let mut adv = Advance::default();
        if self.paused_at.is_some() {
            adv.finished = self.finished;
            return adv;
        }
        let start = *self.start.get_or_insert(now);
        if now < self.last_now {
            adv.next = self.next_time(now);
            adv.finished = self.finished;
            return adv;
        }
        self.last_now = now;
        let origin = start + self.shift;
        let s = now.saturating_sub(origin);
        let len = self.cycle_len();
        loop {
            let base = len.saturating_mul(self.cycle);
            while let Some(cue) = self.script.cues.get(self.idx) {
                let at = base + cue.at;
                if at > s {
                    break;
                }
                match cue.kind.clone() {
                    CueKind::Event(e) => self.emit(e, origin + at, events),
                    CueKind::Mark(name) => adv.marks.push(name),
                }
                self.idx += 1;
            }
            if self.idx < self.script.cues.len() {
                break;
            }
            if !self.repeat {
                if s >= self.script.length() {
                    self.finished = true;
                }
                break;
            }
            if s < base + len {
                break;
            }
            let lift = origin + base + len;
            self.cancel_down(lift, events);
            let skipped = u32::try_from(s.as_nanos() / len.as_nanos()).unwrap_or(u32::MAX);
            self.cycle = self.cycle.saturating_add(1).max(skipped);
            self.idx = 0;
            adv.restarted = true;
        }
        self.tracks.retain(|c| c.down || now < c.up_at + FADE);
        adv.next = self.next_time(now);
        adv.finished = self.finished;
        adv
    }

    /// Stops the script's clock at app time `now`: [`Self::advance`] appends nothing until
    /// [`Self::resume`], and the time spent paused is not played back.
    pub fn pause(&mut self, now: Duration) {
        if self.paused_at.is_none() {
            self.paused_at = Some(now);
        }
    }

    /// Starts the clock again at app time `now`: every later cue comes as late as the pause was
    /// long, so a background of any length replays no burst.
    pub fn resume(&mut self, now: Duration) {
        if let Some(at) = self.paused_at.take() {
            if self.start.is_some() {
                self.shift += now.saturating_sub(at);
            }
        }
    }

    /// Whether a script that does not repeat has played to its end.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// The contacts the player emitted that are down, or lifted less than [`FADE`] before `now`,
    /// each at its last position.
    pub fn contacts(&self, now: Duration) -> Vec<Contact> {
        self.tracks
            .iter()
            .filter_map(|c| {
                let alpha = if c.down {
                    1.0
                } else {
                    let gone = now.saturating_sub(c.up_at);
                    if gone >= FADE {
                        return None;
                    }
                    1.0 - gone.as_secs_f32() / FADE.as_secs_f32()
                };
                Some(Contact {
                    id: c.id,
                    pos: c.pos,
                    down: c.down,
                    alpha,
                })
            })
            .collect()
    }

    /// One cycle's length as the wrap uses it: never zero, so a wrap cannot loop in place.
    fn cycle_len(&self) -> Duration {
        self.script.length().max(SAMPLE)
    }

    fn emit(&mut self, event: Event, at: Duration, events: &mut Vec<Event>) {
        if let Event::Touch(t) = &event {
            self.track(t, at);
        }
        events.push(event);
    }

    fn track(&mut self, t: &TouchEvent, at: Duration) {
        match t.phase {
            TouchPhase::Started => {
                self.tracks.retain(|c| c.id != t.id);
                self.tracks.push(Track {
                    id: t.id,
                    pos: t.pos,
                    down: true,
                    up_at: Duration::ZERO,
                });
            }
            TouchPhase::Moved => {
                if let Some(c) = self.tracks.iter_mut().find(|c| c.id == t.id) {
                    c.pos = t.pos;
                }
            }
            _ => {
                if let Some(c) = self.tracks.iter_mut().find(|c| c.id == t.id) {
                    c.pos = t.pos;
                    c.down = false;
                    c.up_at = at;
                }
            }
        }
    }

    /// A `Cancelled` for every contact still down, at its last position.
    fn cancel_down(&mut self, at: Duration, events: &mut Vec<Event>) {
        let down: Vec<(TouchId, (f32, f32))> = self
            .tracks
            .iter()
            .filter(|c| c.down)
            .map(|c| (c.id, c.pos))
            .collect();
        for (id, pos) in down {
            let e = Event::Touch(TouchEvent::new(id, TouchPhase::Cancelled, pos));
            self.emit(e, at, events);
        }
    }

    /// The app time of the next cue (or the wrap), or the end of a running fade if sooner.
    fn next_time(&self, now: Duration) -> Option<Duration> {
        if self.paused_at.is_some() {
            return None;
        }
        let start = self.start?;
        let fade = self
            .tracks
            .iter()
            .filter(|c| !c.down)
            .map(|c| c.up_at + FADE)
            .filter(|t| *t > now)
            .min();
        let cue = if self.finished {
            None
        } else {
            let len = self.cycle_len();
            let base = len.saturating_mul(self.cycle);
            let at = match self.script.cues.get(self.idx) {
                Some(c) => base + c.at,
                None if self.repeat => base + len,
                None => self.script.length(),
            };
            Some(start + self.shift + at)
        };
        match (cue, fade) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

/// Draws a disc under each of the player's contacts, 44 points across: the theme's `fg_primary`
/// at 22 % with a 1.5 point ring at 55 %, times the contact's alpha. A lifted contact fades over
/// [`FADE`] at [`MotionTier::Full`] and is gone at once under any lower tier. It shows scripted
/// contacts only; a live finger is the OS's own setting. It draws on [`LayerId::Toast`] with
/// [`Painter::on`](crate::paint::Painter::on), which keeps the clip stack, so call it last in a
/// frame at the top level, outside any clip. It asks for no frame: [`Advance::next`] carries the
/// end of the fade.
pub fn draw_touches(ui: &mut Ui<'_>, player: &Player) {
    let contacts = player.contacts(ui.time());
    let full = ui.tier() == MotionTier::Full;
    let colour = ui.theme().fg_primary;
    let p = ui.painter();
    p.on(LayerId::Toast);
    for c in contacts.iter().filter(|c| c.down || full) {
        let r = Rect::new(c.pos.0 - DISC / 2.0, c.pos.1 - DISC / 2.0, DISC, DISC);
        p.fill(r, DISC / 2.0, with_alpha(colour, 0.22 * c.alpha));
        p.ring(
            r,
            DISC / 2.0,
            1.5,
            with_alpha(colour, 0.55 * c.alpha),
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_script_with_no_cue_is_its_end_hold_long() {
        assert_eq!(Script::new().length(), END_HOLD);
        assert_eq!(Script::new().end_hold(ms(10)).length(), ms(10));
    }

    #[test]
    fn a_repeating_player_of_an_empty_script_with_no_hold_never_loops_in_a_call() {
        let mut s = Script::new();
        s.end_hold(Duration::ZERO);
        let mut p = Player::new(s).with_repeat(true);
        let mut out = Vec::new();
        p.advance(ms(0), &mut out);
        let adv = p.advance(ms(100), &mut out);
        assert!(adv.restarted && out.is_empty());
    }

    #[test]
    fn the_cursor_moves_by_each_gesture() {
        let mut s = Script::new();
        s.tap((0.0, 0.0));
        assert_eq!(s.cursor(), TAP_HOLD);
        s.swipe((0.0, 0.0), (1.0, 0.0), ms(100));
        assert_eq!(s.cursor(), TAP_HOLD + ms(100));
        s.drag((0.0, 0.0), (1.0, 0.0), ms(100));
        assert_eq!(s.cursor(), TAP_HOLD + ms(200) + DRAG_SETTLE);
    }
}
