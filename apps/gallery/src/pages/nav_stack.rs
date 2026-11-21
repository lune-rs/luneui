//! The `nav-stack` page: the navigation stack of a phone app (`layout.nav-stack`, D372) over two
//! levels, each with its own navigation bar: a list of albums and one album. The stack fills the
//! window; each level fills its rect with the window's colour, as the component asks. The
//! `phone-nav-stack-*` scenes run in the [`PHONE`] window at scale 3 with a `SafeArea` of 62
//! above and 34 below: the root, the pushed level at rest, a frame in the middle of a push and a
//! frame with a finger half way through the edge swipe. The knob pushes and pops by cutting.

use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::nav_bar::{HEIGHT, NavBar};

use super::nav_bar::{SETUP, backdrop, phone, plain_rows};
use crate::page::{Ev, Expect, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "nav-stack",
    title: "Navigation stack",
    kind: PageKind::Ui(UiPage {
        state: root,
        scenes: SCENES,
        bench: &[],
    }),
};

/// What the app keeps: how many levels there are.
#[derive(Debug, Clone, Copy)]
struct Live {
    depth: usize,
}

fn root() -> Box<dyn PageState> {
    Box::new(Live { depth: 1 })
}

fn pushed() -> Box<dyn PageState> {
    Box::new(Live { depth: 2 })
}

fn text(ui: &mut Ui<'_>, s: &str, x: f32, y: f32, bold: bool) {
    let t = *ui.theme();
    let style = if bold {
        Style::ui(t.font_title, t.fg_primary).bold()
    } else {
        Style::ui(t.font_base, t.fg_secondary)
    };
    ui.painter().text(&Spec::plain(s, style), x, y);
}

/// What a level asked of the app this frame.
#[derive(Debug, Default, Clone, Copy)]
struct Asked {
    push: bool,
    back: bool,
}

/// Level `level` in `r`: the window's colour behind it, its bar, and its content.
fn level_page(ui: &mut Ui<'_>, level: usize, r: Rect, asked: &mut Asked) {
    let tint = ui.theme().glass.window.tint;
    ui.painter().fill(r, 0.0, [tint[0], tint[1], tint[2], 255]);
    let safe = ui.safe_area();
    let bar = Rect::new(r.x, r.y, r.w, safe.top + HEIGHT);
    if level == 0 {
        plain_rows(ui, r, bar.bottom() + 52.0, r.bottom() - safe.bottom);
        if ui
            .button(
                "open",
                Rect::new(r.x + 16.0, bar.bottom() + 8.0, 160.0, 36.0),
                "Open Holiday",
                ButtonKind::Primary,
            )
            .clicked
        {
            asked.push = true;
        }
        ui.nav_bar("bar", bar, &NavBar::new("Albums"));
    } else {
        text(ui, "Holiday", r.x + 16.0, bar.bottom() + 16.0, true);
        for i in 0..5 {
            text(
                ui,
                "Photos from the trip, newest first",
                r.x + 16.0,
                bar.bottom() + 56.0 + 36.0 * i as f32,
                false,
            );
        }
        let t = *ui.theme();
        for i in 0..6 {
            let (col, row) = ((i % 3) as f32, (i / 3) as f32);
            ui.painter().fill(
                Rect::new(
                    r.x + 16.0 + col * 120.0,
                    bar.bottom() + 260.0 + row * 120.0,
                    112.0,
                    112.0,
                ),
                8.0,
                t.surface_hover,
            );
        }
        let nav = NavBar::new("Holiday").with_back("Albums");
        if ui.nav_bar("bar", bar, &nav).back {
            asked.back = true;
        }
    }
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = backdrop(ui, stage);
        let mut asked = Asked::default();
        let resp = ui.nav_stack("stack", area, self.depth, |ui, level, r| {
            level_page(ui, level, r, &mut asked);
        });
        if asked.back {
            ui.nav_stack_pop("stack");
        }
        if asked.push {
            self.depth += 1;
        }
        if resp.popped {
            self.depth -= 1;
        }
    }

    fn cue(&mut self, name: &str) {
        if name == "push" {
            self.depth += 1;
        }
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        let mut open = self.depth >= 2;
        panel.toggle(ui, "Album open", &mut open);
        self.depth = if open { 2 } else { 1 };
    }
}

/// A tap on "Open Holiday" at 1.0 s; 100 ms later the new level is part way in.
const PUSH: &[Step] = &[
    SETUP,
    Step {
        at_ms: 500,
        events: &[],
    },
    Step {
        at_ms: 1000,
        events: &[Ev::Cue("push")],
    },
];

/// A finger down in the edge band at 1.0 s, 211 px right by 1.05 s and held: half way across.
const SWIPE: &[Step] = &[
    SETUP,
    Step {
        at_ms: 500,
        events: &[],
    },
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 10.0, 420.0)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 80.0, 420.0)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 150.0, 420.0)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 221.0, 420.0)],
    },
];

const SCENES: &[UiScene] = &[
    phone("phone-nav-stack-root", root, &[SETUP], 300),
    phone("phone-nav-stack-pushed", pushed, &[SETUP], 300),
    UiScene {
        expect: Expect::Moving,
        ..phone("phone-nav-stack-push-mid", root, PUSH, 1100)
    },
    phone("phone-nav-stack-swipe-mid", pushed, SWIPE, 1100),
];
