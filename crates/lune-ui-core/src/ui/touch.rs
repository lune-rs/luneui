//! Touch reduced to the single-pointer path (C1(c), C1(g)). A pre-pass over a frame's events turns
//! the primary touch into pointer events the existing arms already handle, keeps the other
//! contacts out of the pointer, and drops the mouse copy some platforms make of a touch.

use std::time::Duration;

use crate::input::{Event, Modifiers, PointerButton, PointerKind, TouchEvent, TouchId, TouchPhase};
use crate::motion::Time;

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

/// The touches that are down and the mouse guard, held in the store between frames.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TouchState {
    /// The contacts that are down, in the order they started.
    down: Vec<TouchId>,
    /// The contact that drives the pointer; `None` while none does.
    primary: Option<TouchId>,
    /// The mouse guard holds until this time passes.
    quiet_until: Option<Time>,
    /// The kind of the current or last press.
    pub(crate) kind: PointerKind,
}

impl TouchState {
    /// Whether the pointer events of `time` are a platform's copy of a touch.
    fn guarded(&self, time: Time) -> bool {
        !self.down.is_empty() || self.quiet_until.is_some_and(|until| time < until)
    }

    fn lifted(&mut self, time: Time) {
        if self.down.is_empty() {
            self.quiet_until = Some(time + MOUSE_AFTER_TOUCH);
        }
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
        let known = self.down.contains(&t.id);
        let phase = match t.phase {
            TouchPhase::Started if known => TouchPhase::Moved,
            p => p,
        };
        match phase {
            TouchPhase::Started => {
                let first = self.down.is_empty();
                self.down.push(t.id);
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
                if known && self.primary == Some(t.id) {
                    steps.push(Step::Synth(Event::PointerMoved { pos: t.pos }));
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if !known {
                    return;
                }
                self.down.retain(|d| *d != t.id);
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
