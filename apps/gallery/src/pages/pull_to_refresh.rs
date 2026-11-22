//! The `pull-to-refresh` page: pull to refresh of a phone list (`layout.pull-to-refresh`, D375).
//! A navigation bar over a list of rows in a `Ui::pull_to_refresh` area. The app's refresh takes
//! a second and a half from the pull's release (or from the bar's Refresh action, the mouse and
//! keyboard way to the same thing). The `phone-refresh-*` scenes run in the
//! [`PHONE`](crate::page::PHONE) window at scale 3 with a `SafeArea` of 62 above and 34 below: a
//! finger pulling with the content 40 px down, one past the threshold, the hold after its
//! release, and the list at rest. The knob holds the refresh on.

use std::time::Duration;

use lune_ui::icon;
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::nav_bar::{HEIGHT, NavAction, NavBar};
use lune_ui::widgets::scroll_area::{Extent, ScrollOptions};

use super::nav_bar::{SETUP, backdrop, phone};
use crate::page::{Ev, Expect, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "pull-to-refresh",
    title: "Pull to refresh",
    kind: PageKind::Ui(UiPage {
        state: live,
        scenes: SCENES,
        bench: &[],
    }),
};

/// A row's height.
const ROW: f32 = 64.0;
/// How long the app's refresh takes.
const WORK: Duration = Duration::from_millis(1500);

/// The rows: a title and a line under it.
const NEWS: [(&str, &str); 8] = [
    ("Tides and the moon", "A short history of the almanac"),
    ("Harvest report", "Wheat is up, barley is steady"),
    ("Ferry timetable", "The winter service starts on Monday"),
    ("Library hours", "Open late on Thursdays"),
    ("Market day", "Forty stalls, one new baker"),
    ("Road works", "The north bridge reopens in March"),
    ("Choir practice", "Moved to the old school hall"),
    ("Weather", "Clear, then a little rain by evening"),
];

/// What the app keeps: when its refresh started, and the knob that holds it on.
#[derive(Debug, Clone, Copy)]
struct Live {
    since: Option<Duration>,
    hold: bool,
}

fn live() -> Box<dyn PageState> {
    Box::new(Live {
        since: None,
        hold: false,
    })
}

/// A row in `r`: a disc, the title, the line under it, a separator.
fn row(ui: &mut Ui<'_>, index: usize, r: Rect) {
    let t = *ui.theme();
    let (title, line) = NEWS[index % NEWS.len()];
    let p = ui.painter();
    let disc = Rect::new(r.x + 16.0, r.y + 12.0, 40.0, 40.0);
    p.fill(disc, 20.0, t.surface_active);
    let initial = Spec::plain(&title[..1], Style::ui(t.font_m, t.fg_primary).bold());
    p.text_centered(&initial, disc);
    let name = Spec::plain(title, Style::ui(t.font_m, t.fg_primary));
    let name = p.elide(&name, r.w - 68.0 - 16.0);
    p.text_in(
        &name,
        Rect::new(r.x + 68.0, r.y + 10.0, r.w - 84.0, 22.0),
        0.0,
    );
    let sub = Spec::plain(line, Style::ui(t.font_s, t.fg_secondary));
    let sub = p.elide(&sub, r.w - 68.0 - 16.0);
    p.text_in(
        &sub,
        Rect::new(r.x + 68.0, r.y + 34.0, r.w - 84.0, 20.0),
        0.0,
    );
    p.fill(
        Rect::new(r.x + 68.0, r.bottom() - 1.0, r.w - 68.0, 1.0),
        0.0,
        t.surface_separator,
    );
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = backdrop(ui, stage);
        let safe = ui.safe_area();
        let now = ui.time();
        if self.since.is_some_and(|t| now.saturating_sub(t) >= WORK) {
            self.since = None;
        }
        let bar = Rect::new(area.x, area.y, area.w, safe.top + HEIGHT);
        let list = Rect::new(
            area.x,
            bar.bottom(),
            area.w,
            (area.bottom() - safe.bottom - bar.bottom()).max(0.0),
        );
        let refreshing = self.hold || self.since.is_some();
        let res = ui.pull_to_refresh(
            "list",
            list,
            refreshing,
            &ScrollOptions::default(),
            |ui, view| {
                let rows = 20;
                for i in 0..rows {
                    let r = Rect::new(
                        view.viewport.x,
                        view.viewport.y + i as f32 * ROW,
                        view.viewport.w,
                        ROW,
                    );
                    row(ui, i, r);
                }
                (
                    (),
                    Extent::new(f64::from(view.viewport.w), f64::from(ROW) * rows as f64),
                )
            },
        );
        let actions = [NavAction::new(icon::SYNC, "Refresh")];
        let bar_res = ui.nav_bar("bar", bar, &NavBar::new("News").with_actions(&actions));
        if res.refresh || bar_res.action.is_some() {
            self.since = Some(now);
        }
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Hold the refresh", &mut self.hold);
    }
}

/// A finger down at (200, 300) at 1.0 s, then `Ev::TouchMove`s down by 15 px every 16 ms.
const PULLING: &[Step] = &[
    SETUP,
    Step {
        at_ms: 500,
        events: &[],
    },
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 200.0, 300.0)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 200.0, 315.0)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 200.0, 330.0)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 200.0, 345.0)],
    },
    Step {
        at_ms: 1066,
        events: &[Ev::TouchMove(1, 200.0, 360.0)],
    },
    Step {
        at_ms: 1083,
        events: &[Ev::TouchMove(1, 200.0, 375.0)],
    },
    Step {
        at_ms: 1100,
        events: &[Ev::TouchMove(1, 200.0, 390.0)],
    },
];

/// The pull on to past the threshold, held.
const ARMED: &[Step] = &[
    SETUP,
    Step {
        at_ms: 500,
        events: &[],
    },
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 200.0, 300.0)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 200.0, 330.0)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 200.0, 360.0)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 200.0, 390.0)],
    },
    Step {
        at_ms: 1066,
        events: &[Ev::TouchMove(1, 200.0, 420.0)],
    },
    Step {
        at_ms: 1083,
        events: &[Ev::TouchMove(1, 200.0, 450.0)],
    },
    Step {
        at_ms: 1100,
        events: &[Ev::TouchMove(1, 200.0, 480.0)],
    },
    Step {
        at_ms: 1116,
        events: &[],
    },
];

/// The same pull, released at 1.3 s: the app's refresh runs for 1.5 s, and the capture is 0.7 s in.
const HOLDING: &[Step] = &[
    SETUP,
    Step {
        at_ms: 500,
        events: &[],
    },
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 200.0, 300.0)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 200.0, 330.0)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 200.0, 360.0)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 200.0, 390.0)],
    },
    Step {
        at_ms: 1066,
        events: &[Ev::TouchMove(1, 200.0, 420.0)],
    },
    Step {
        at_ms: 1083,
        events: &[Ev::TouchMove(1, 200.0, 450.0)],
    },
    Step {
        at_ms: 1100,
        events: &[Ev::TouchMove(1, 200.0, 480.0)],
    },
    Step {
        at_ms: 1300,
        events: &[Ev::TouchEnd(1, 200.0, 480.0)],
    },
    Step {
        at_ms: 1700,
        events: &[],
    },
];

const SCENES: &[UiScene] = &[
    phone("phone-refresh-pulling", live, PULLING, 1300),
    phone("phone-refresh-armed", live, ARMED, 1300),
    // The spinner turns: a frame is always due at the busy cadence.
    UiScene {
        expect: Expect::Waiting,
        ..phone("phone-refresh-holding", live, HOLDING, 2000)
    },
    phone("phone-refresh-idle", live, &[SETUP], 300),
];
