//! Touch reduced to the single-pointer path (C1(c), C1(g)). A pre-pass over a frame's events turns
//! the primary touch into pointer events the existing arms already handle, keeps the other
//! contacts out of the pointer, and drops the mouse copy some platforms make of a touch.

use std::time::Duration;

use crate::input::{Event, Modifiers, PointerButton, PointerKind, TouchEvent, TouchId, TouchPhase};
use crate::motion::Time;

use super::WidgetId;

/// How long, after the last touch ends, the pointer events a platform copies from a touch are
/// dropped (app time, D5).
pub(crate) const MOUSE_AFTER_TOUCH: Time = Duration::from_millis(500);

/// One step the frame's event loop takes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Step<'e> {
    /// The raw event, as it arrived.
    Pass(&'e Event),
    /// A pointer event made from a touch.
    Synth(Event),
    /// The press in flight is dropped: no click, no drag stop, no secondary click.
    Cancel,
}

/// Two touches moving apart or together over the widget the first one pressed (D348), read through
/// [`Ui::pinch`](super::Ui::pinch).
///
/// Nothing is cancelled by a second touch: the first keeps driving the pointer, so a widget that
/// also senses drag sees both.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Pinch {
    /// The midpoint of the two touches now, in logical pixels.
    pub center: (f32, f32),
    /// The midpoint when the pinch began.
    pub start_center: (f32, f32),
    /// The distance between the touches now over the distance when the pinch began (a start
    /// distance under 1 px counts as 1 px): above 1 spread, below 1 pinched.
    pub scale: f32,
    /// This is the pinch's first frame.
    pub started: bool,
    /// This is the pinch's last frame: a touch of the two ended, and the values are the last ones.
    pub ended: bool,
}

/// A pinch in progress: the second touch it follows and the reading it started from.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PinchRun {
    second: TouchId,
    /// The widget the primary touch held when the pinch began; `None` when it held nothing.
    target: Option<WidgetId>,
    d0: f32,
    last: Pinch,
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn mid(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

/// The touches that are down and the mouse guard, held in the store between frames.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TouchState {
    /// The contacts that are down with their last position, in the order they started.
    down: Vec<(TouchId, (f32, f32))>,
    /// The contact that drives the pointer; `None` while none does.
    primary: Option<TouchId>,
    /// The mouse guard holds until this time passes.
    quiet_until: Option<Time>,
    /// The kind of the current or last press.
    pub(crate) kind: PointerKind,
    /// The pinch in progress, if any.
    pinch: Option<PinchRun>,
    /// A pinch began since the last time every touch was up: no other begins before then.
    pinched: bool,
}

impl TouchState {
    /// Whether the pointer events of `time` are a platform's copy of a touch.
    fn guarded(&self, time: Time) -> bool {
        !self.down.is_empty() || self.quiet_until.is_some_and(|until| time < until)
    }

    fn lifted(&mut self, time: Time) {
        if self.down.is_empty() {
            self.quiet_until = Some(time + MOUSE_AFTER_TOUCH);
            self.pinched = false;
        }
    }

    fn pos_of(&self, id: TouchId) -> Option<(f32, f32)> {
        self.down.iter().find(|d| d.0 == id).map(|d| d.1)
    }

    /// The pinch reading after a frame's events, as `(target, reading)` when it has a widget to
    /// go to; `held` is the widget the pointer's press holds now. A pinch begins when a second
    /// touch is down beside the primary and ends when either of the two is up (D348).
    pub(crate) fn pinch_frame(&mut self, held: Option<WidgetId>) -> Option<(WidgetId, Pinch)> {
        let primary = self.primary.and_then(|id| self.pos_of(id).map(|p| (id, p)));
        let run = match self.pinch {
            None => {
                let (pid, p) = primary.filter(|_| !self.pinched)?;
                let (second, q) = *self.down.iter().find(|d| d.0 != pid)?;
                self.pinched = true;
                let d0 = dist(p, q).max(1.0);
                let c = mid(p, q);
                PinchRun {
                    second,
                    target: held,
                    d0,
                    last: Pinch {
                        center: c,
                        start_center: c,
                        scale: dist(p, q) / d0,
                        started: true,
                        ended: false,
                    },
                }
            }
            Some(mut run) => {
                match (primary, self.pos_of(run.second)) {
                    (Some((_, p)), Some(q)) => {
                        run.last.center = mid(p, q);
                        run.last.scale = dist(p, q) / run.d0;
                        run.last.started = false;
                    }
                    _ => {
                        run.last.started = false;
                        run.last.ended = true;
                    }
                }
                run
            }
        };
        self.pinch = (!run.last.ended).then_some(run);
        run.target.map(|id| (id, run.last))
    }

    /// The steps `events` take, in arrival order, at app time `time`.
    pub(crate) fn reduce<'e>(&mut self, events: &'e [Event], time: Time) -> Vec<Step<'e>> {
        let mut steps = Vec::with_capacity(events.len());
        for ev in events {
            match ev {
                Event::Touch(t) => self.touch(*t, time, &mut steps),
                Event::Focus(false) => {
                    if self.primary.take().is_some() {
                        steps.push(Step::Cancel);
                        steps.push(Step::Synth(Event::PointerLeft));
                    }
                    if !self.down.is_empty() {
                        self.down.clear();
                        self.lifted(time);
                    }
                    steps.push(Step::Pass(ev));
                }
                Event::PointerMoved { .. } | Event::PointerLeft if self.guarded(time) => {}
                Event::PointerButton { pressed, .. } => {
                    if self.guarded(time) {
                        continue;
                    }
                    if *pressed {
                        self.kind = PointerKind::Mouse;
                    }
                    steps.push(Step::Pass(ev));
                }
                _ => steps.push(Step::Pass(ev)),
            }
        }
        steps
    }

    fn touch(&mut self, t: TouchEvent, time: Time, steps: &mut Vec<Step<'_>>) {
        let known = self.down.iter().any(|d| d.0 == t.id);
        let phase = match t.phase {
            TouchPhase::Started if known => TouchPhase::Moved,
            p => p,
        };
        match phase {
            TouchPhase::Started => {
                let first = self.down.is_empty();
                self.down.push((t.id, t.pos));
                if first {
                    self.primary = Some(t.id);
                    self.kind = PointerKind::Touch;
                    // A mouse press still held lets go first: one press at a time.
                    steps.push(Step::Cancel);
                    steps.push(Step::Synth(Event::PointerMoved { pos: t.pos }));
                    steps.push(Step::Synth(Event::PointerButton {
                        pos: t.pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::default(),
                    }));
                }
            }
            TouchPhase::Moved => {
                if let Some(d) = self.down.iter_mut().find(|d| d.0 == t.id) {
                    d.1 = t.pos;
                }
                if known && self.primary == Some(t.id) {
                    steps.push(Step::Synth(Event::PointerMoved { pos: t.pos }));
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if !known {
                    return;
                }
                self.down.retain(|d| d.0 != t.id);
                if self.primary == Some(t.id) {
                    self.primary = None;
                    if phase == TouchPhase::Ended {
                        steps.push(Step::Synth(Event::PointerButton {
                            pos: t.pos,
                            button: PointerButton::Primary,
                            pressed: false,
                            modifiers: Modifiers::default(),
                        }));
                    } else {
                        steps.push(Step::Cancel);
                    }
                    steps.push(Step::Synth(Event::PointerLeft));
                }
                self.lifted(time);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mouse_only_list_passes_every_event_in_order() {
        let events = [
            Event::PointerMoved { pos: (1.0, 2.0) },
            Event::PointerButton {
                pos: (1.0, 2.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
            Event::PointerButton {
                pos: (1.0, 2.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
            Event::Focus(false),
            Event::PointerLeft,
        ];
        let mut state = TouchState::default();
        let steps = state.reduce(&events, Time::from_secs(9));
        let want: Vec<Step<'_>> = events.iter().map(Step::Pass).collect();
        assert_eq!(steps, want);
        assert_eq!(state, TouchState::default());
    }
}
