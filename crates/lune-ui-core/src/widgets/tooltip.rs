//! Tooltips (`overlay.tooltip`): a small panel under the control the pointer rests on, shown
//! after `MotionTokens::tooltip_delay` and hidden the moment the pointer leaves. It takes no
//! input and no focus. Also [`Ui::hover_dwell`], the delay's timer.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::popover::{GLASS_RADIUS, glass_surface};
use super::popover::{PlaceAlign, PlaceRequest, Side, keep_in_bounds, place};
use crate::access::{Node, Role};
use crate::input::PointerKind;
use crate::motion::{Easing, Kind, Motion, Reduce, Time, Token};
use crate::paint::{LayerId, Ops, Painter, Rect, Xform};
use crate::text::{Spec, Style};
use crate::ui::LONG_PRESS;
use crate::ui::{OverlayOptions, OverlayPhase, Response, Ui, WidgetId};

/// The minimum continuous hover before lazily built hover content is built (C5's 150 ms). Not the tooltip delay, which is `MotionTokens::tooltip_delay`.
pub const LAZY_HOVER_DWELL: Duration = Duration::from_millis(150);

/// The widest a tooltip's panel gets; longer text wraps.
const MAX_WIDTH: f32 = 320.0;
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 8.0;
const GAP: f32 = 4.0;
const MARGIN: f32 = 8.0;

/// Appear: opacity only, fast, quad-out; Reduced keeps the fade.
const APPEAR: Motion = Motion {
    kind: Kind::Appear,
    token: Token::Fast,
    easing: Easing::QuadOut,
    reduced: Reduce::Keep,
    reduced_token: None,
};
/// Dismiss: opacity only, instant, quad-in; Reduced keeps the fade.
const DISMISS: Motion = Motion {
    kind: Kind::Dismiss,
    token: Token::Instant,
    easing: Easing::QuadIn,
    reduced: Reduce::Keep,
    reduced_token: None,
};

/// When the pointer (or focus) started resting on an id.
#[derive(Debug, Default)]
struct Dwell {
    since: Option<Time>,
}

/// A tooltip's hiding state: a press or Escape hides it until the pointer (and focus) leave.
#[derive(Debug, Default)]
struct Hidden(bool);

/// How long app time a touch tooltip stays after the finger lifts (D363): long enough to read it.
pub(crate) const TOUCH_LINGER: Duration = Duration::from_millis(1500);

/// A touch tooltip's state under its anchor (D363).
#[derive(Debug, Default)]
struct TouchTip {
    /// The `t0` of the press that owns the tooltip: the finger rests on the anchor.
    press: Option<Time>,
    /// When the linger after the lift ends.
    until: Option<Time>,
    /// The press whose tooltip Escape dismissed.
    dismissed: Option<Time>,
}

/// The tooltip's panel opacity at one moment.
fn opacity(phase: OverlayPhase) -> f32 {
    match phase {
        OverlayPhase::Opening(p) => APPEAR.easing.eval(p),
        OverlayPhase::Open => 1.0,
    }
}

/// Opacity-only transform, or none at full opacity.
fn fade(alpha: f32) -> Option<Xform> {
    (alpha < 1.0).then_some(Xform {
        origin: (0.0, 0.0),
        scale: 1.0,
        dx: 0.0,
        dy: 0.0,
        alpha,
    })
}

impl<'a> Ui<'a> {
    /// Whether `active` has held for `delay` (app time), counting from the frame it turned true.
    /// Transient state under `id`: it resets on a frame with `active` false and is dropped when
    /// not called. While pending it asks for a frame at the due time (`Repaint::At`).
    pub fn hover_dwell(&mut self, id: WidgetId, active: bool, delay: Duration) -> bool {
        let now = self.time();
        let dwell = self.transient::<Dwell>(id);
        if !active {
            dwell.since = None;
            return false;
        }
        let due = *dwell.since.get_or_insert(now) + delay;
        if now >= due {
            return true;
        }
        self.request_repaint_at(due);
        false
    }

    /// A tooltip with `text` for `anchor`, below it (flipped and shifted to stay in the window).
    /// Call it after the anchor's own widget, every frame: it shows after the theme's tooltip
    /// delay of the pointer over the anchor (disabled ones too) or of keyboard focus on it, and
    /// is hidden the frame the pointer leaves; a press on the anchor or Escape hides it until the
    /// pointer leaves. Sets the anchor node's description to `text` (and its name when empty).
    pub fn tooltip(&mut self, anchor: &Response, text: &str) {
        let t = *self.theme();
        let style = Style::ui(t.font_s, t.fg_primary);
        self.tip(
            anchor,
            Some(text),
            |ui| {
                let spec = Spec::plain(text, style);
                let size = ui.painter().measure(&spec);
                if size.0 > MAX_WIDTH - 2.0 * PAD_X {
                    let spec = spec.wrapped(MAX_WIDTH - 2.0 * PAD_X);
                    (ui.painter().measure(&spec), spec)
                } else {
                    (size, spec)
                }
            },
            |ui, content, spec| {
                ui.painter().text(&spec, content.x, content.y);
            },
        );
    }

    /// [`Self::tooltip`] with content of `size` (before padding) drawn by `content` into the rect
    /// it is given; the panel adds the padding and is at most 320 wide.
    pub fn tooltip_with(
        &mut self,
        anchor: &Response,
        size: (f32, f32),
        content: impl FnOnce(&mut Ui<'a>, Rect),
    ) {
        self.tip(
            anchor,
            None,
            |_| (size, ()),
            |ui, rect, ()| content(ui, rect),
        );
    }

    /// The shared tooltip: `layout` sizes the content and hands data to `draw`, both run only
    /// while the tooltip is shown.
    fn tip<D>(
        &mut self,
        anchor: &Response,
        text: Option<&str>,
        layout: impl FnOnce(&mut Ui<'a>) -> ((f32, f32), D),
        draw: impl FnOnce(&mut Ui<'a>, Rect, D),
    ) {
        if let Some(text) = text {
            self.patch_node(anchor.id, |n| {
                n.description = Some(text.to_owned());
                if n.name.is_empty() {
                    n.name = text.to_owned();
                }
            });
        }
        // `anchor.rect` is a `Response`'s: already in window coordinates. Under a finger there is
        // no hover: a rest of the finger shows the tooltip instead (D363).
        let touch = self.pointer_kind() == PointerKind::Touch;
        let hover = !touch && self.pointer_over(anchor.rect);
        let active = hover || anchor.focus_visible;
        let hidden_id = anchor.id.with("tooltip-hidden");
        let hidden = {
            let h = &mut self.transient::<Hidden>(hidden_id).0;
            if !active {
                *h = false;
            } else if anchor.press_started || anchor.pressed {
                *h = true;
            }
            *h
        };
        let delay = self.theme().motion.tooltip_delay;
        let due = self.hover_dwell(anchor.id.with("tooltip-dwell"), active && !hidden, delay);
        let touched = touch && self.touch_tooltip(anchor);
        if !(due || touched) {
            return;
        }

        let ((cw, ch), data) = layout(self);
        let size = ((cw + 2.0 * PAD_X).min(MAX_WIDTH), ch + 2.0 * PAD_Y);
        let window = self.overlay_bounds();
        let rect = place(&PlaceRequest {
            anchor: anchor.rect,
            size,
            side: Side::Bottom,
            align: PlaceAlign::Center,
            gap: GAP,
            margin: MARGIN,
            shift: true,
            window,
        })
        .rect;
        let rect = keep_in_bounds(rect, self.painter().window, window, MARGIN);
        let inner = Rect::new(
            rect.x + PAD_X,
            rect.y + PAD_Y,
            rect.w - 2.0 * PAD_X,
            rect.h - 2.0 * PAD_Y,
        );

        let options = OverlayOptions {
            layer: LayerId::Popup,
            modal: false,
            dismiss_on_escape: true,
            dismiss_on_outside_press: false,
            dismiss_on_focus_loss: false,
            anchor: None,
            initial_focus: None,
            open: APPEAR,
            close: DISMISS,
        };
        let drawn: Rc<RefCell<Ops>> = Rc::default();
        let exit_drawn = Rc::clone(&drawn);
        let exit = move |p: &mut Painter<'_, WidgetId>, progress: f32, _transforms: bool| {
            let alpha = 1.0 - DISMISS.easing.eval(progress);
            if let Some(x) = fade(alpha) {
                p.push_xform(x);
                p.replay(&exit_drawn.borrow());
                p.pop_xform();
            } else {
                p.replay(&exit_drawn.borrow());
            }
        };
        let name = text.unwrap_or_default();
        let (frame, ()) = self.overlay(anchor.id.with("tooltip"), &options, exit, |ui, frame| {
            let faded = fade(opacity(frame.phase));
            if let Some(x) = faded {
                ui.painter().push_xform(x);
            }
            let ((), ops) = ui.record(|ui| {
                glass_surface(ui.painter(), rect, GLASS_RADIUS);
                draw(ui, inner, data);
            });
            if faded.is_some() {
                ui.painter().pop_xform();
            }
            *drawn.borrow_mut() = ops;
            ui.node(Node::new(frame.id, Role::Tooltip, name, rect));
        });
        if frame.dismissed.is_some() {
            self.transient::<Hidden>(hidden_id).0 = true;
            let held = self.touch_rest().map(|p| p.t0);
            self.transient::<TouchTip>(anchor.id.with("tooltip-touch"))
                .dismissed = held;
        }
    }

    /// Whether a touch shows `anchor`'s tooltip now (D363): the finger rested on the anchor for
    /// `LONG_PRESS`, and it stays while the finger is down and for `TOUCH_LINGER` after a lift
    /// that came after the tooltip showed. A new press elsewhere, a pan, a drag past the slop, a
    /// cancel or Escape hides it. Asks for the frame each deadline falls on.
    fn touch_tooltip(&mut self, anchor: &Response) -> bool {
        let now = self.time();
        let rest = self.touch_rest();
        let end = self.touch_end();
        let id = anchor.id.with("tooltip-touch");
        let st = self.transient::<TouchTip>(id);
        match rest {
            Some(p) if p.resting && anchor.rect.contains(p.at) => {
                if st.press != Some(p.t0) {
                    st.press = Some(p.t0);
                    st.until = None;
                }
                if st.dismissed == Some(p.t0) {
                    return false;
                }
                let due = p.t0 + LONG_PRESS;
                if now >= due {
                    return true;
                }
                self.request_repaint_at(due);
                false
            }
            Some(_) => {
                st.press = None;
                st.until = None;
                false
            }
            None => {
                let Some(t0) = st.press else {
                    return false;
                };
                let shown = end.filter(|e| {
                    e.t0 == t0
                        && e.rested_lift
                        && e.at >= t0 + LONG_PRESS
                        && st.dismissed != Some(t0)
                });
                let Some(e) = shown else {
                    st.press = None;
                    st.until = None;
                    return false;
                };
                let until = *st.until.get_or_insert(e.at + TOUCH_LINGER);
                if now < until {
                    self.request_repaint_at(until);
                    true
                } else {
                    st.press = None;
                    st.until = None;
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lazy_dwell_is_not_the_tooltip_delay() {
        assert_eq!(LAZY_HOVER_DWELL, Duration::from_millis(150));
        assert_eq!(
            crate::motion::MOTION.tooltip_delay,
            Duration::from_millis(800)
        );
    }
}
