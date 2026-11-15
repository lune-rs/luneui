//! The window depth lab: what each surface depth changes on the glass window. The window's
//! surface is 8, 10 or 16 bits per channel (the "Surface depth" setting, or this page's knob; both
//! are one choice), and the samples are the things whose look depends on it: a fade of alpha over
//! the see-through window (the 2-bit alpha of the 10-bit format quantises it to four levels), a
//! soft shadow's halo, a translucent rounded rect, and a dark slow ramp (its bands are the banding
//! of the format, whatever the alpha).
//!
//! The samples sit on the window's own glass, with no card behind them, so what shows through is
//! the desktop. The live line says what the window really is; in a dump there is no window.
//! [`DEPTHS`], [`area`] and [`live_line`] are shared with the banding lab and the settings pane.

use lune_ui::paint::{LayerId, Rect};
use lune_ui::surface::SurfaceDepth;
use lune_ui::ui::Ui;

use super::text;
use crate::page::{Expect, Page, PageKind, PageState, Stage, SurfaceLive, UiPage, UiScene};
use crate::pages::{caption, heading};
use crate::scenes::{CARD_MARGIN, TITLE_BAND};
use crate::showcase::knobs::Knobs;
use lune_ui::motion::MotionTier;

pub(crate) static PAGE: Page = Page {
    id: "lab-depth",
    title: "Window depth",
    kind: PageKind::Ui(UiPage {
        state: live,
        scenes: SCENES,
        bench: &[],
    }),
};

const fn scene(scale: f32) -> UiScene {
    UiScene {
        id: "lab-depth",
        scale,
        tier: MotionTier::Full,
        state: live,
        script: &[],
        capture_ms: 0,
        expect: Expect::Settled,
    }
}

const SCENES: &[UiScene] = &[scene(1.0), scene(2.0)];

fn live() -> Box<dyn PageState> {
    Box::new(DepthLab::default())
}

/// The depths a window can be asked for, in the order the controls list them.
pub(crate) const DEPTHS: [(&str, SurfaceDepth); 5] = [
    ("Auto", SurfaceDepth::Auto),
    ("8-bit", SurfaceDepth::Eight),
    ("10-bit", SurfaceDepth::Ten),
    ("16-bit float", SurfaceDepth::Float16),
    ("16-bit unorm", SurfaceDepth::Unorm16),
];

/// The control's name for `depth`.
pub(crate) fn depth_label(depth: SurfaceDepth) -> &'static str {
    DEPTHS
        .iter()
        .find(|(_, d)| *d == depth)
        .map_or("other", |(label, _)| label)
}

/// The step of the gradient dither of a target of `format` shown on a display of `display_bits`,
/// as the denominator of `1 / n`, or `None` for no dither. It is the renderer's rule (the larger
/// of the format's step and the display's, none for float formats alone and for the sRGB formats),
/// stated again here because the renderer keeps it private.
pub(crate) fn dither_denominator(
    format: wgpu::TextureFormat,
    display_bits: Option<u8>,
) -> Option<u32> {
    use wgpu::TextureFormat as F;
    let format_steps = match format {
        F::Rgba8Unorm | F::Bgra8Unorm => Some(255),
        F::Rgb10a2Unorm => Some(1023),
        F::Rgba16Unorm => Some(65535),
        F::Rgba16Float | F::Rgba32Float => None,
        _ => return None,
    };
    let display_steps = match display_bits {
        Some(b @ 1..=16) => Some((1u32 << b) - 1),
        _ => None,
    };
    match (format_steps, display_steps) {
        (Some(f), Some(d)) => Some(f.min(d)),
        (f, d) => f.or(d),
    }
}

/// The text of the dither: its step, or that there is none.
pub(crate) fn dither_text(live: Option<SurfaceLive>) -> String {
    match live {
        None => "dither: not a window".to_owned(),
        Some(l) => match dither_denominator(l.format, l.display_bits) {
            Some(n) => format!("dither step 1/{n}"),
            None => "no dither (a float surface on a display of unknown depth)".to_owned(),
        },
    }
}

/// The live line: the window's format, depth, alpha mode and display, or that this is a dump.
pub(crate) fn live_line(live: Option<SurfaceLive>) -> String {
    let Some(l) = live else {
        return "Live: not a window".to_owned();
    };
    let display = match l.display_bits {
        Some(b) => format!("display {b}-bit"),
        None => "display unknown".to_owned(),
    };
    format!(
        "Live: {:?} \u{b7} {} \u{b7} alpha {:?} \u{b7} {display}",
        l.format,
        depth_label(l.depth),
        l.alpha
    )
}

/// The area a lab draws in. A page in the showcase is given a rect of the window; alone in a
/// window (a dump) it gets the window's own glass and the area a card would have, with no card.
pub(crate) fn area(ui: &mut Ui<'_>, stage: &Stage) -> Rect {
    if let Some(rect) = stage.rect() {
        return rect;
    }
    let glass = ui.theme().glass.panel.premultiplied();
    let p = ui.painter();
    p.on(LayerId::Base);
    let win = p.window;
    p.acrylic(win, 0.0, glass, [0.0; 4], true);
    Rect::new(
        CARD_MARGIN,
        TITLE_BAND,
        win.w - 2.0 * CARD_MARGIN,
        win.h - TITLE_BAND - CARD_MARGIN,
    )
}

/// The depth choice and what the window last reported, as the depth labs hold them.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DepthChoice {
    pub(crate) depth: SurfaceDepth,
    pub(crate) live: Option<SurfaceLive>,
}

impl DepthChoice {
    /// The knob: the same choice as the settings pane's "Surface depth".
    pub(crate) fn knob(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        let labels: Vec<&str> = DEPTHS.iter().map(|(label, _)| *label).collect();
        let mut selected = DEPTHS
            .iter()
            .position(|(_, d)| *d == self.depth)
            .unwrap_or(0);
        if panel.choice(ui, "Depth", &labels, &mut selected) {
            self.depth = DEPTHS[selected].1;
        }
    }

    /// The live line and the asked depth under it, at (`x`, `y`).
    pub(crate) fn lines(&self, ui: &mut Ui<'_>, x: f32, y: f32) {
        let t = *ui.theme();
        text(ui, &live_line(self.live), x, y, t.font_base, t.fg_primary);
        let asked = format!("Asked: {}", depth_label(self.depth));
        text(ui, &asked, x, y + 20.0, t.font_s, t.fg_secondary);
    }
}

/// The samples of the depth lab; the knob is the depth.
#[derive(Debug, Default)]
struct DepthLab {
    choice: DepthChoice,
}

/// A dark ramp of eight 8-bit levels per channel: where an 8-bit surface bands.
const FROM: [u8; 4] = [0x0d, 0x0f, 0x1a, 0xff];
const TO: [u8; 4] = [0x15, 0x17, 0x22, 0xff];

impl PageState for DepthLab {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = area(ui, stage);
        let t = *ui.theme();
        let (x, w) = (area.x + 24.0, area.w - 48.0);
        let y = area.y + 20.0;
        self.choice.lines(ui, x, y);

        // A fade of alpha, from nothing to the text colour, over the see-through window.
        let fade_y = y + 56.0;
        heading(
            ui,
            "TRANSLUCENT FADE: ALPHA 0 TO 1 OVER THE GLASS",
            x,
            fade_y,
        );
        let clear = [t.fg_primary[0], t.fg_primary[1], t.fg_primary[2], 0];
        ui.painter().linear(
            Rect::new(x, fade_y + 20.0, w, 56.0),
            0.0,
            clear,
            t.fg_primary,
            0.0,
        );
        caption(
            ui,
            "8-bit and 16-bit: smooth. 10-bit: four steps, the 2-bit alpha of its format.",
            x,
            fade_y + 84.0,
        );

        // A soft shadow and a translucent rounded rect, side by side.
        let row_y = fade_y + 120.0;
        let half = w / 2.0;
        heading(ui, "SOFT SHADOW: SIGMA 24", x, row_y);
        heading(ui, "TRANSLUCENT FILL: ALPHA 0.35", x + half, row_y);
        let top = row_y + 20.0 + 48.0;
        let accent = [t.accent[0], t.accent[1], t.accent[2], 89];
        {
            let p = ui.painter();
            p.shadow(
                Rect::new(x + 48.0, top, 200.0, 56.0),
                16.0,
                24.0,
                [0, 0, 0, 200],
            );
            p.fill(Rect::new(x + half + 48.0, top, 220.0, 56.0), 24.0, accent);
        }
        caption(
            ui,
            "The halo's alpha ramps to nothing; 10-bit shows it in steps.",
            x,
            top + 56.0 + 56.0,
        );
        caption(
            ui,
            "A translucent fill with rounded, antialiased corners.",
            x + half,
            top + 56.0 + 56.0,
        );

        // A dark, slow, opaque ramp: its bands do not depend on the alpha.
        let ramp_y = top + 56.0 + 56.0 + 36.0;
        heading(
            ui,
            "DARK RAMP: EIGHT 8-BIT LEVELS ACROSS THE WIDTH",
            x,
            ramp_y,
        );
        ui.painter()
            .linear(Rect::new(x, ramp_y + 20.0, w, 48.0), 0.0, FROM, TO, 0.0);
        caption(
            ui,
            "8-bit: eight wide bands. 10-bit and 16-bit: finer, and dithered between the levels.",
            x,
            ramp_y + 76.0,
        );
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        self.choice.knob(ui, panel);
    }

    fn surface_depth(&self) -> Option<SurfaceDepth> {
        Some(self.choice.depth)
    }

    fn set_surface_depth(&mut self, depth: SurfaceDepth) {
        self.choice.depth = depth;
    }

    fn surface_applied(&mut self, live: SurfaceLive) {
        self.choice.live = Some(live);
    }
}

#[cfg(test)]
mod tests {
    use super::super::rig::player;
    use super::*;
    use lune_ui::testing::FakeText;
    use wgpu::TextureFormat as F;

    fn live_of(format: F, depth: SurfaceDepth, bits: Option<u8>) -> SurfaceLive {
        SurfaceLive {
            format,
            depth,
            alpha: wgpu::CompositeAlphaMode::PostMultiplied,
            display_bits: bits,
        }
    }

    #[test]
    fn the_live_line_names_the_format_the_depth_the_alpha_and_the_display() {
        assert_eq!(live_line(None), "Live: not a window");
        let l = live_of(F::Rgb10a2Unorm, SurfaceDepth::Ten, None);
        assert_eq!(
            live_line(Some(l)),
            "Live: Rgb10a2Unorm \u{b7} 10-bit \u{b7} alpha PostMultiplied \u{b7} display unknown"
        );
        let l = live_of(F::Rgba16Float, SurfaceDepth::Float16, Some(10));
        assert!(live_line(Some(l)).ends_with("display 10-bit"));
    }

    /// The renderer's rule (`dither_lsb`): the larger step of the format and the display.
    #[test]
    fn the_dither_step_is_the_larger_of_the_formats_and_the_displays() {
        assert_eq!(dither_denominator(F::Bgra8Unorm, None), Some(255));
        assert_eq!(dither_denominator(F::Bgra8Unorm, Some(10)), Some(255));
        assert_eq!(dither_denominator(F::Rgb10a2Unorm, None), Some(1023));
        assert_eq!(dither_denominator(F::Rgb10a2Unorm, Some(8)), Some(255));
        assert_eq!(dither_denominator(F::Rgba16Float, None), None);
        assert_eq!(dither_denominator(F::Rgba16Float, Some(10)), Some(1023));
        assert_eq!(dither_denominator(F::Rgba16Unorm, None), Some(65535));
        assert_eq!(dither_denominator(F::Bgra8UnormSrgb, Some(8)), None);
        assert_eq!(dither_denominator(F::Rgba16Float, Some(17)), None);
        assert_eq!(dither_text(None), "dither: not a window");
    }

    #[test]
    fn every_depth_has_a_label_and_the_labels_differ() {
        let mut labels: Vec<&str> = DEPTHS.iter().map(|(l, _)| *l).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), DEPTHS.len());
        assert_eq!(depth_label(SurfaceDepth::Ten), "10-bit");
    }

    #[test]
    fn the_page_draws_its_samples_and_follows_the_depth_it_is_given() {
        let mut p = player("lab-depth", live());
        p.frame(&mut FakeText, 0, &[]).expect("frame");
        let (scene, _) = p.frame(&mut FakeText, 16, &[]).expect("frame");
        let base = scene.merged(LayerId::Base);
        // The fade, the ramp, the shadow and the fill: more than a bare page.
        assert!(base.geometry.under.len() >= 4, "the samples are drawn");
        let mut lab = DepthLab::default();
        assert_eq!(lab.surface_depth(), Some(SurfaceDepth::Auto));
        lab.set_surface_depth(SurfaceDepth::Ten);
        assert_eq!(lab.surface_depth(), Some(SurfaceDepth::Ten));
    }
}
