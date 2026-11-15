//! The gradient banding lab: large, dark, slow gradients, the kind that band at 8 bits, with the
//! window's live format, depth and dither step on screen. Switch the depth (the "Surface depth"
//! setting, or this page's knob) and the bands change with it.
//!
//! The first ramp is the real gradient, dithered by the renderer for the window's format and
//! display. Under it, the same ramp as steps of 8-bit rounding drawn as plain fills, which is
//! what a surface with no dither at 8 bits shows (a fill is an 8-bit colour, so the reference
//! cannot be finer; the renderer has no switch to turn its own dither off). Then a radial light
//! and a vertical ramp, dithered, full size.

use lune_ui::paint::{Falloff, Rect};
use lune_ui::surface::SurfaceDepth;
use lune_ui::ui::Ui;

use super::depth::{DepthChoice, area, dither_text};
use super::text;
use crate::page::{Expect, Page, PageKind, PageState, Stage, SurfaceLive, UiPage, UiScene};
use crate::pages::{caption, heading};
use crate::showcase::knobs::Knobs;
use lune_ui::motion::MotionTier;

pub(crate) static PAGE: Page = Page {
    id: "lab-banding",
    title: "Gradient banding",
    kind: PageKind::Ui(UiPage {
        state: live,
        scenes: SCENES,
        bench: &[],
    }),
};

const SCENES: &[UiScene] = &[UiScene {
    id: "lab-banding",
    scale: 1.0,
    tier: MotionTier::Full,
    state: live,
    script: &[],
    capture_ms: 0,
    expect: Expect::Settled,
}];

fn live() -> Box<dyn PageState> {
    Box::new(Banding::default())
}

/// The dark ramp: eight 8-bit levels per channel from one end to the other.
const FROM: [u8; 4] = [0x0d, 0x0f, 0x1a, 0xff];
const TO: [u8; 4] = [0x15, 0x17, 0x22, 0xff];
/// The radial light's glow: fifteen levels in its brightest channel.
const GLOW: [u8; 4] = [0x1c, 0x22, 0x33, 0xff];

/// The ramp as steps of 8-bit rounding across `width` px: runs of equal colour as (start, length,
/// colour), in px, that cover `0..width`. A pixel column takes the colour its centre has.
fn steps(width: u32) -> Vec<(u32, u32, [u8; 4])> {
    let level = |x: u32| {
        let t = (f64::from(x) + 0.5) / f64::from(width);
        let mix = |ch: usize| {
            let (a, b) = (f64::from(FROM[ch]), f64::from(TO[ch]));
            (a + (b - a) * t).round() as u8
        };
        [mix(0), mix(1), mix(2), 255]
    };
    let mut runs: Vec<(u32, u32, [u8; 4])> = Vec::new();
    for x in 0..width {
        let c = level(x);
        match runs.last_mut() {
            Some((_, len, last)) if *last == c => *len += 1,
            _ => runs.push((x, 1, c)),
        }
    }
    runs
}

#[derive(Debug, Default)]
struct Banding {
    choice: DepthChoice,
}

impl PageState for Banding {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = area(ui, stage);
        let t = *ui.theme();
        let (x, w) = (area.x + 24.0, area.w - 48.0);
        let y = area.y + 20.0;
        self.choice.lines(ui, x, y);
        text(
            ui,
            &dither_text(self.choice.live),
            x + 420.0,
            y + 20.0,
            t.font_s,
            t.fg_secondary,
        );

        let a = y + 52.0;
        heading(ui, "DARK RAMP, DITHERED FOR THIS WINDOW", x, a);
        ui.painter()
            .linear(Rect::new(x, a + 18.0, w, 72.0), 0.0, FROM, TO, 0.0);
        let b = a + 100.0;
        heading(ui, "THE SAME RAMP IN 8-BIT STEPS, NO DITHER", x, b);
        {
            let p = ui.painter();
            for (start, len, colour) in steps(w as u32) {
                p.fill(
                    Rect::new(x + start as f32, b + 18.0, len as f32, 72.0),
                    0.0,
                    colour,
                );
            }
        }
        caption(
            ui,
            "Eight wide bands at 8 bits. A deeper surface has more levels between them, dithered.",
            x,
            b + 98.0,
        );

        let c = b + 130.0;
        let half = w / 2.0 - 12.0;
        heading(ui, "RADIAL LIGHT", x, c);
        heading(ui, "VERTICAL RAMP", x + half + 24.0, c);
        let bg = FROM;
        {
            let p = ui.painter();
            let glow = Rect::new(x, c + 18.0, half, 200.0);
            p.fill(glow, 0.0, bg);
            p.radial(
                [glow.x + half / 2.0, glow.y + 100.0],
                [half / 2.0, 100.0],
                GLOW,
                [GLOW[0], GLOW[1], GLOW[2], 0],
                Falloff::Smooth,
            );
            p.linear(
                Rect::new(x + half + 24.0, c + 18.0, half, 200.0),
                0.0,
                FROM,
                TO,
                std::f32::consts::FRAC_PI_2,
            );
        }
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

    /// The steps cover the width exactly once, rise from one end's colour to the other's, and
    /// there are as many as the ramp has levels (eight per channel, so at most 24 changes).
    #[test]
    fn the_staircase_covers_the_ramp_in_its_8_bit_levels() {
        let runs = steps(972);
        let mut at = 0;
        for (start, len, _) in &runs {
            assert_eq!(*start, at);
            at += len;
        }
        assert_eq!(at, 972);
        assert_eq!(runs.first().expect("runs").2, FROM);
        assert_eq!(runs.last().expect("runs").2, TO);
        assert!(runs.len() > 8 && runs.len() <= 25, "{} runs", runs.len());
        for pair in runs.windows(2) {
            for ch in 0..3 {
                assert!(pair[1].2[ch] >= pair[0].2[ch], "never darker going right");
            }
        }
    }

    #[test]
    fn the_page_draws_and_follows_the_depth_it_is_given() {
        let mut p = player("lab-banding", live());
        p.frame(&mut FakeText, 0, &[]).expect("frame");
        p.frame(&mut FakeText, 16, &[]).expect("frame");
        let mut page = Banding::default();
        assert_eq!(page.surface_depth(), Some(SurfaceDepth::Auto));
        page.set_surface_depth(SurfaceDepth::Float16);
        assert_eq!(page.surface_depth(), Some(SurfaceDepth::Float16));
    }
}
