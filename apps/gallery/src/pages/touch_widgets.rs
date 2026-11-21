//! The `touch-widgets` page: the existing widgets under a finger (mobile M5, D361 to D365), laid
//! out for a phone-width window inside `Ui::safe_area`. One scroll area holds a small checkbox
//! and an icon button with a tooltip, a slider, a row split, an overflowing tab strip, a form of
//! two fields, a text area and a selectable paragraph; the app's own edit menu opens on the
//! `secondary_clicked` of any of the text widgets, built from `Ui::edit_actions` and run with
//! `Ui::edit_command` (D292, D364), and a long press on the status strip opens a menu at the top
//! of the window. The `phone-widgets-*` scenes run in the [`PHONE`] window at scale 3 with a
//! `SafeArea` of 62 above and 34 below; the touches are scripted at 60 Hz.
//!
//! The feel of a real finger is not checked here (no finger can be driven): the scenes show the
//! states each rule reaches, which the core's touch tests prove at both rates.

use lune_ui::icon;
use lune_ui::layout::{Axis, Insets};
use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::scene::LayerId;
use lune_ui::text::{Spec, Style};
use lune_ui::text_edit::EditCommand;
use lune_ui::ui::{Sense, Ui, WidgetId};
use lune_ui::widgets::button::IconButton;
use lune_ui::widgets::field::{FieldOptions, FieldResponse};
use lune_ui::widgets::label::LabelRole;
use lune_ui::widgets::menu::MenuRow;
use lune_ui::widgets::scroll_area::{Extent, ScrollOptions};
use lune_ui::widgets::slider::SliderOptions;
use lune_ui::widgets::split_panes::{Pane, SplitOptions, SplitSide};
use lune_ui::widgets::tab_strip::{GAP, HEIGHT, MIN_WIDTH, Tab, TabEvent, TabKey, TabStripOptions};
use lune_ui::widgets::text_area::AreaOptions;

use super::{caption, card, heading};
use crate::page::{Ev, Expect, PHONE, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::scenes::{CARD_MARGIN, H, TITLE_BAND, W};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "touch-widgets",
    title: "Touch widgets",
    kind: PageKind::Ui(UiPage {
        state: live,
        scenes: SCENES,
        bench: BENCH,
    }),
};

fn live() -> Box<dyn PageState> {
    Box::new(Live::new())
}

/// The page with no edit menu: the scenes that show a selection and its handles start from it,
/// because a menu moves focus to its first row, and a field that is not focused draws neither.
fn without_menu() -> Box<dyn PageState> {
    Box::new(Live {
        menus: false,
        ..Live::new()
    })
}

/// The padding inside the card, and the gap between two sections.
const PAD: f32 = 12.0;
/// A caption's height.
const CAP: f32 = 18.0;
/// The space under a section.
const AFTER: f32 = 14.0;
/// The slider's height, and the split's, and the tab body's, and a field's, and the area's.
const SLIDER_H: f32 = 24.0;
const SPLIT_H: f32 = 190.0;
const BODY_H: f32 = 56.0;
const FIELD_H: f32 = 32.0;
const AREA_H: f32 = 110.0;
/// The tooltip anchor, and the checkbox: the least a control is.
const ICON: f32 = 28.0;
const BOX: f32 = 24.0;
/// How many tabs the strip has: more than the card is wide.
const TABS: u64 = 8;
/// Where the slider starts, as a fraction of its track.
const SLIDER_AT: f32 = 0.3;
/// The text the form starts with.
const NAME: &str = "Ada Lovelace";
const EMAIL: &str = "ada@example.org";
const NOTES: &str = "Notes on the analytical engine.";
const PARAGRAPH: &str = "A finger rests where a pointer hovers. Tap to place the caret, press and \
hold to select a word, drag a handle to move an end of the selection.";

/// Where the page puts its parts at scroll offset 0, from the card, the safe area and the
/// window's height. A `const fn`, so the scenes aim their touches at the same places the page
/// draws.
#[derive(Debug, Clone, Copy)]
struct Layout {
    title: (f32, f32),
    /// The status strip a long press opens a menu from.
    strip: Rect,
    /// The scroll area.
    area: Rect,
    check: Rect,
    note: (f32, f32),
    icon: Rect,
    slider: Rect,
    split: Rect,
    tabs: Rect,
    name: Rect,
    email: Rect,
    notes: Rect,
    text: Rect,
    /// The content's height.
    content: f32,
}

const fn fmax(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

const fn layout(card: Rect, safe: Insets, window_w: f32, window_h: f32) -> Layout {
    let top = fmax(card.y, safe.top) + PAD;
    let ay = top + 22.0;
    let area = Rect::new(card.x, ay, card.w, window_h - safe.bottom - ay);
    let x = area.x + PAD;
    let w = area.w - 2.0 * PAD;
    let mut y = ay + 6.0;
    let check = Rect::new(x, y, BOX, BOX);
    let icon = Rect::new(x + w - ICON, y - 2.0, ICON, ICON);
    y += ICON + AFTER;
    y += CAP;
    let slider = Rect::new(x, y, w, SLIDER_H);
    y += SLIDER_H + AFTER;
    y += CAP;
    let split = Rect::new(x, y, w, SPLIT_H);
    y += SPLIT_H + AFTER;
    y += CAP;
    let tabs = Rect::new(x, y, w, HEIGHT);
    y += HEIGHT + BODY_H + AFTER;
    y += CAP;
    let name = Rect::new(x, y, w, FIELD_H);
    y += FIELD_H + AFTER;
    y += CAP;
    let email = Rect::new(x, y, w, FIELD_H);
    y += FIELD_H + AFTER;
    y += CAP;
    let notes = Rect::new(x, y, w, AREA_H);
    y += AREA_H + AFTER;
    y += CAP;
    let text = Rect::new(x, y, w, 90.0);
    y += 90.0 + AFTER;
    Layout {
        title: (x, top),
        strip: Rect::new(0.0, 0.0, window_w, top + 14.0),
        area,
        check,
        note: (x + BOX + 8.0, check.y + 3.0),
        icon,
        slider,
        split,
        tabs,
        name,
        email,
        notes,
        text,
        content: y - ay,
    }
}

/// The card of a window `w` by `h` with nothing but the gallery's own band and margin around it.
const fn window_card(w: u32, h: u32) -> Rect {
    Rect::new(
        CARD_MARGIN,
        TITLE_BAND,
        w as f32 - 2.0 * CARD_MARGIN,
        h as f32 - TITLE_BAND - CARD_MARGIN,
    )
}

/// The safe area the phone scenes tell the page.
const SAFE: Insets = Insets::new(0.0, 62.0, 0.0, 34.0);
/// The page in the phone window.
const P: Layout = layout(
    window_card(PHONE.0, PHONE.1),
    SAFE,
    PHONE.0 as f32,
    PHONE.1 as f32,
);
/// The page in the gallery's own window, which has no safe area (the bench aims at it).
const WIN: Layout = layout(window_card(W, H), Insets::ZERO, W as f32, H as f32);

const fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// The text widgets an edit menu can belong to.
#[derive(Debug, Clone, Copy)]
struct EditMenu {
    id: WidgetId,
    at: (f32, f32),
}

/// The live page: what the app would keep.
struct Live {
    checked: bool,
    volume: f32,
    split: f32,
    tabs: Vec<(u64, String)>,
    active: u64,
    name: String,
    email: String,
    notes: String,
    /// The edit menu, when a text widget reported a secondary click.
    edit: Option<EditMenu>,
    /// Where the status strip's menu is open, if it is.
    status: Option<(f32, f32)>,
    /// A secondary click on a text widget opens the edit menu.
    menus: bool,
}

impl Live {
    fn new() -> Self {
        Self {
            checked: false,
            volume: 30.0,
            split: 80.0,
            tabs: (0..TABS).map(|i| (i, format!("Tab {}", i + 1))).collect(),
            active: 0,
            name: NAME.to_owned(),
            email: EMAIL.to_owned(),
            notes: NOTES.to_owned(),
            edit: None,
            status: None,
            menus: true,
        }
    }

    /// The app's edit menu: Cut, Copy, Paste and Select all for the widget, only those it takes.
    fn edit_menu(&mut self, ui: &mut Ui<'_>) {
        let Some(menu) = self.edit else {
            return;
        };
        let Some(actions) = ui.edit_actions(menu.id) else {
            self.edit = None;
            return;
        };
        let all = [
            (actions.cut, "Cut", EditCommand::Cut),
            (actions.copy, "Copy", EditCommand::Copy),
            (actions.paste, "Paste", EditCommand::Paste),
            (actions.select_all, "Select all", EditCommand::SelectAll),
        ];
        let shown: Vec<(&str, EditCommand)> = all
            .into_iter()
            .filter(|(takes, ..)| *takes)
            .map(|(_, text, cmd)| (text, cmd))
            .collect();
        let rows: Vec<MenuRow<'_>> = shown
            .iter()
            .map(|(text, _)| MenuRow::Item {
                glyph: None,
                text,
                shortcut: "",
            })
            .collect();
        let response = ui.menu("edit-menu", &rows, menu.at);
        if let Some(i) = response.picked {
            ui.edit_command(menu.id, shown[i].1.clone());
        }
        if response.picked.is_some() || response.dismissed {
            self.edit = None;
        }
    }

    /// The menu a long press on the status strip opens.
    fn status_menu(&mut self, ui: &mut Ui<'_>) {
        let Some(at) = self.status else {
            return;
        };
        let item = |text| MenuRow::Item {
            glyph: None,
            text,
            shortcut: "",
        };
        let rows = [item("Settings"), item("About"), item("Quit")];
        let response = ui.menu("status-menu", &rows, at);
        if response.picked.is_some() || response.dismissed {
            self.status = None;
        }
    }
}

/// A label that is only text, 18 high.
fn note(ui: &mut Ui<'_>, key: &'static str, at: (f32, f32), text: &str) {
    ui.label(key, at, text, LabelRole::Small);
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let card = card(ui, stage, "Touch widgets");
        let safe = ui.safe_area();
        let window = ui.painter().window;
        let l = layout(card, safe, window.w, window.h);
        heading(ui, "TOUCH WIDGETS", l.title.0, l.title.1);

        // The status strip: a long press is its secondary click, and the menu opens where the
        // finger is, which is above the safe area's top edge (the menu moves inside it).
        let strip = ui.id("strip");
        let hold = ui.interact(strip, l.strip, Sense::CLICK);
        if hold.secondary_clicked {
            self.status = ui.pointer_pos();
        }
        self.status_menu(ui);

        let mut menu = None;
        let opts = ScrollOptions::default();
        let (volume, split, active) = (&mut self.volume, &mut self.split, &mut self.active);
        let (checked, tabs) = (&mut self.checked, &mut self.tabs);
        let (name, email, notes) = (&mut self.name, &mut self.email, &mut self.notes);
        ui.scroll_area("body", l.area, &opts, |ui, _| {
            // A small checkbox and an icon button with a tooltip.
            ui.checkbox("remember", l.check, checked, "");
            note(ui, "remember-note", l.note, "Remember me");
            let share = IconButton {
                glyph: icon::INFO,
                glyph_size: 16.0,
                color: None,
                radius: 4.0,
                name: "Info",
            };
            let r = ui.icon_button("info", l.icon, &share);
            ui.tooltip(&r, "What this page shows");

            caption(ui, "Volume", l.slider.x, l.slider.y - CAP);
            let options = SliderOptions {
                min: 0.0,
                max: 100.0,
                step: 0.0,
                name: "Volume",
            };
            ui.slider("volume", l.slider, volume, &options);

            caption(ui, "Panels", l.split.x, l.split.y - CAP);
            let rows = SplitOptions {
                axis: Axis::Vertical,
                sized: SplitSide::Second,
                default: 80.0,
                min: 48.0,
                max: 150.0,
                min_other: 48.0,
                hidden: false,
                name: "Panel height",
            };
            let r = ui.split_panes("split", l.split, &rows, |ui, which, rect| {
                let t = *ui.theme();
                let (tint, text) = match which {
                    Pane::First => (t.surface_hover, "Content"),
                    Pane::Second => (t.surface_overlay, "Panel below"),
                };
                ui.painter().fill(rect, 0.0, tint);
                let spec = Spec::plain(text, Style::ui(t.font_base, t.fg_primary).medium());
                ui.painter().text(&spec, rect.x + 12.0, rect.y + 12.0);
            });
            *split = r.size;

            caption(ui, "Tabs", l.tabs.x, l.tabs.y - CAP);
            let list: Vec<Tab<'_>> = tabs
                .iter()
                .map(|(key, title)| Tab {
                    key: TabKey(*key),
                    title,
                    closable: true,
                    progress: None,
                })
                .collect();
            let strip_opts = TabStripOptions {
                active: Some(TabKey(*active)),
                panel_focused: true,
                card: Rect::new(l.tabs.x, l.tabs.y, l.tabs.w, HEIGHT + BODY_H),
                name: "Sections",
                ..TabStripOptions::default()
            };
            let r = ui.tab_strip("tabs", l.tabs, &list, &strip_opts);
            for event in r.events {
                match event {
                    TabEvent::Activate(k) => *active = k.0,
                    TabEvent::Close(k) => {
                        tabs.retain(|(key, _)| *key != k.0);
                        if *active == k.0 {
                            *active = tabs.first().map_or(0, |(key, _)| *key);
                        }
                    }
                    TabEvent::New | TabEvent::Reorder { .. } => {}
                }
            }
            let t = *ui.theme();
            let body = format!("The body of tab {}", *active + 1);
            let spec = Spec::plain(body, Style::ui(t.font_base, t.fg_secondary));
            ui.painter().text_centered(
                &spec,
                Rect::new(l.tabs.x, l.tabs.y + HEIGHT, l.tabs.w, BODY_H),
            );

            caption(ui, "Name", l.name.x, l.name.y - CAP);
            let options = FieldOptions {
                label: Some("Name"),
                ..FieldOptions::default()
            };
            let r = ui.text_field_with("name", l.name, name, &options);
            menu = menu.or(edit_menu_of(&r));
            caption(ui, "Email", l.email.x, l.email.y - CAP);
            let options = FieldOptions {
                label: Some("Email"),
                ..FieldOptions::default()
            };
            let r = ui.text_field_with("email", l.email, email, &options);
            menu = menu.or(edit_menu_of(&r));

            caption(ui, "Notes", l.notes.x, l.notes.y - CAP);
            let options = AreaOptions {
                label: Some("Notes"),
                wrap: true,
                ..AreaOptions::default()
            };
            let r = ui.text_area("notes", l.notes, notes, &options);
            menu = menu.or(edit_menu_of(&r));

            caption(ui, "About", l.text.x, l.text.y - CAP);
            let spec =
                Spec::plain(PARAGRAPH, Style::ui(t.font_base, t.fg_secondary)).wrapped(l.text.w);
            let r = ui.selectable_text("about", l.text, &spec, PARAGRAPH);
            if r.secondary_clicked {
                menu = Some(EditMenu {
                    id: r.id,
                    at: (l.text.x, l.text.y - EDIT_MENU_H - 6.0),
                });
            }
            ((), Extent::new(f64::from(l.area.w), f64::from(l.content)))
        });
        if self.menus && menu.is_some() {
            self.edit = menu;
        }
        self.edit_menu(ui);

        // The window has no keyboard of its own: a stand-in fills what the app was told is covered.
        let keys = ui.keyboard_inset().bottom;
        if keys > 0.0 {
            let t = *ui.theme();
            let rect = Rect::new(0.0, window.h - keys, window.w, keys);
            // The theme's overlay tint over a dark base, opaque: the page must not show through.
            let c = t.surface_overlay;
            let a = f32::from(c[3]) / 255.0;
            let mix = |v: u8| (f32::from(v) * a + 34.0 * (1.0 - a)).round() as u8;
            // Its own layer, above the page's text, which draws over a fill of the page's layer.
            ui.painter().on(LayerId::Popup);
            ui.painter()
                .fill(rect, 0.0, [mix(c[0]), mix(c[1]), mix(c[2]), 255]);
            let spec = Spec::plain("Keyboard", Style::ui(t.font_base, t.fg_tertiary));
            ui.painter().text_centered(&spec, rect);
            ui.painter().on(LayerId::Page);
        }
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Edit menu on a long press", &mut self.menus);
    }
}

/// What a menu of the edit menu's four rows is high (the app's estimate: it only has to clear the
/// widget).
const EDIT_MENU_H: f32 = 130.0;

/// The edit menu a text widget asks for with a secondary click (a long press, D292): it opens
/// above the widget's box, at the left of it, where a phone's own edit menu is, so the finger
/// and the selection's handles below the text stay clear.
fn edit_menu_of(r: &FieldResponse) -> Option<EditMenu> {
    r.response.secondary_clicked.then_some(EditMenu {
        id: r.response.id,
        at: (
            r.response.rect.x + 12.0,
            r.response.rect.y - EDIT_MENU_H - 6.0,
        ),
    })
}

// ---- scenes -----------------------------------------------------------------------------------

/// The first step of every phone scene: the window's safe area.
const SETUP: Step = Step {
    at_ms: 0,
    events: &[Ev::SafeArea(SAFE.left, SAFE.top, SAFE.right, SAFE.bottom)],
};

/// A phone scene: scale 3, the full motion tier, the page's live state.
const fn phone(
    id: &'static str,
    script: &'static [Step],
    capture_ms: u32,
    expect: Expect,
) -> UiScene {
    UiScene {
        id,
        scale: 3.0,
        tier: MotionTier::Full,
        state: live,
        script,
        capture_ms,
        expect,
    }
}

/// [`phone`] on the page without its edit menu.
const fn phone_bare(
    id: &'static str,
    script: &'static [Step],
    capture_ms: u32,
    expect: Expect,
) -> UiScene {
    UiScene {
        id,
        scale: 3.0,
        tier: MotionTier::Full,
        state: without_menu,
        script,
        capture_ms,
        expect,
    }
}

/// A step that moves touch 1 to (x, y) at `ms`.
macro_rules! drag {
    ($ms:expr, $x:expr, $y:expr) => {
        Step {
            at_ms: $ms,
            events: &[Ev::TouchMove(1, $x, $y)],
        }
    };
}

/// A step that puts touch 1 down at (x, y) at `ms`.
macro_rules! down {
    ($ms:expr, $x:expr, $y:expr) => {
        Step {
            at_ms: $ms,
            events: &[Ev::TouchStart(1, $x, $y)],
        }
    };
}

/// A step that lifts touch 1 at (x, y) at `ms`.
macro_rules! up {
    ($ms:expr, $x:expr, $y:expr) => {
        Step {
            at_ms: $ms,
            events: &[Ev::TouchEnd(1, $x, $y)],
        }
    };
}

/// An empty step: a frame at `ms` with no events (what a real loop runs when the core asked for
/// one).
const fn frame(ms: u32) -> Step {
    Step {
        at_ms: ms,
        events: &[],
    }
}

/// A tap 12 px past the checkbox's drawn box (16 wide), 4 px outside its 24 px hit.
const BOX_TAP: (f32, f32) = (P.check.x + 16.0 + 12.0, P.check.y + BOX / 2.0);

const HIT_AREA: &[Step] = &[
    SETUP,
    down!(16, BOX_TAP.0, BOX_TAP.1),
    up!(80, BOX_TAP.0, BOX_TAP.1),
];

/// The finger on the slider's track at the value, and 60 px up: the area scrolls by 50.
const SLIDER_X: f32 = P.slider.x + P.slider.w * SLIDER_AT;
const SLIDER_Y: f32 = P.slider.y + SLIDER_H / 2.0;

const SLIDER_PAN: &[Step] = &[
    SETUP,
    down!(16, SLIDER_X, SLIDER_Y),
    drag!(33, SLIDER_X, SLIDER_Y - 15.0),
    drag!(50, SLIDER_X, SLIDER_Y - 30.0),
    drag!(67, SLIDER_X, SLIDER_Y - 45.0),
    drag!(83, SLIDER_X, SLIDER_Y - 60.0),
];

/// The same finger moved along the track, 40 px right: the value follows.
const SLIDER_DRAG: &[Step] = &[
    SETUP,
    down!(16, SLIDER_X, SLIDER_Y),
    drag!(33, SLIDER_X + 12.0, SLIDER_Y - 2.0),
    drag!(50, SLIDER_X + 60.0, SLIDER_Y - 3.0),
    drag!(67, SLIDER_X + 120.0, SLIDER_Y - 3.0),
    drag!(83, SLIDER_X + 160.0, SLIDER_Y - 3.0),
];

/// The finger on the second tab, moved 150 px left: the strip scrolls, nothing activates.
const TAB_Y: f32 = P.tabs.y + HEIGHT / 2.0;
const TAB_X: f32 = P.tabs.x + MIN_WIDTH + GAP + 30.0;

const TABS_PAN: &[Step] = &[
    SETUP,
    down!(16, TAB_X, TAB_Y),
    drag!(33, TAB_X - 30.0, TAB_Y),
    drag!(50, TAB_X - 80.0, TAB_Y),
    drag!(67, TAB_X - 130.0, TAB_Y),
    drag!(83, TAB_X - 170.0, TAB_Y),
];

/// A finger resting 600 ms on the icon button: the tooltip shows under it.
const TOOLTIP: &[Step] = &[
    SETUP,
    Step {
        at_ms: 16,
        events: &[Ev::TouchStart(
            1,
            P.icon.x + ICON / 2.0,
            P.icon.y + ICON / 2.0,
        )],
    },
    frame(516),
    frame(616),
];

/// A tap on the name field, 4 characters in: the caret is there and the field has focus.
const FIELD_TAP_AT: (f32, f32) = (P.name.x + 10.0 + 33.0, P.name.y + FIELD_H / 2.0);

const FIELD_TAP: &[Step] = &[
    SETUP,
    down!(16, FIELD_TAP_AT.0, FIELD_TAP_AT.1),
    up!(80, FIELD_TAP_AT.0, FIELD_TAP_AT.1),
    frame(200),
];

/// A press held on the second word of the name and lifted: the word is selected, the handles
/// show and the edit menu is open above the field.
const WORD_AT: (f32, f32) = (P.name.x + 10.0 + 75.0, P.name.y + FIELD_H / 2.0);

const FIELD_LONG_PRESS: &[Step] = &[
    SETUP,
    down!(16, WORD_AT.0, WORD_AT.1),
    frame(516),
    up!(700, WORD_AT.0, WORD_AT.1),
];

/// A press held on the first word and lifted, then the end handle taken (the knob is 5 px below
/// the line, under the word's end) and moved right: the end of the selection follows it, past the
/// second word's fifth letter.
const FIRST_WORD_AT: (f32, f32) = (P.name.x + 10.0 + 15.0, P.name.y + FIELD_H / 2.0);
const END_HANDLE: (f32, f32) = (P.name.x + 33.5, P.name.y + 30.0);

const HANDLE_DRAG: &[Step] = &[
    SETUP,
    down!(16, FIRST_WORD_AT.0, FIRST_WORD_AT.1),
    frame(516),
    up!(560, FIRST_WORD_AT.0, FIRST_WORD_AT.1),
    down!(700, END_HANDLE.0, END_HANDLE.1),
    drag!(716, END_HANDLE.0 + 12.0, END_HANDLE.1),
    drag!(733, END_HANDLE.0 + 26.0, END_HANDLE.1),
    drag!(750, END_HANDLE.0 + 40.0, END_HANDLE.1),
    frame(900),
];

/// A tap in the notes box, low in the page, and then the keyboard 300 px up: the page scrolls the
/// box above the keyboard.
const NOTES_AT: (f32, f32) = (P.notes.x + 150.0, P.notes.y + 60.0);

const KEYBOARD: &[Step] = &[
    SETUP,
    down!(16, NOTES_AT.0, NOTES_AT.1),
    up!(80, NOTES_AT.0, NOTES_AT.1),
    Step {
        at_ms: 300,
        events: &[Ev::KeyboardInset(300.0)],
    },
    frame(316),
    frame(333),
];

/// A long press at the very top of the window, in the status strip: the menu opens below the
/// safe area's top edge.
const STRIP_AT: (f32, f32) = (P.title.0 + 60.0, 20.0);

const SAFE_AREA_MENU: &[Step] = &[
    SETUP,
    down!(16, STRIP_AT.0, STRIP_AT.1),
    frame(516),
    frame(616),
];

pub(super) const SCENES: &[UiScene] = &[
    // A tap 12 px beside the small checkbox's box toggles it.
    phone("phone-widgets-hit-area", HIT_AREA, 400, Expect::Settled),
    // A pan that starts on the slider scrolls the page and leaves the value.
    phone("phone-widgets-slider-pan", SLIDER_PAN, 400, Expect::Settled),
    // A drag along the track moves the value from the slop on.
    phone(
        "phone-widgets-slider-drag",
        SLIDER_DRAG,
        400,
        Expect::Settled,
    ),
    // A pan that starts on a tab scrolls the strip and activates nothing.
    phone("phone-widgets-tabs-pan", TABS_PAN, 400, Expect::Settled),
    // A finger resting 600 ms on the icon button shows its tooltip.
    phone("phone-widgets-tooltip", TOOLTIP, 700, Expect::Settled),
    // A tap in the name field puts the caret there and focuses it.
    phone("phone-widgets-field-tap", FIELD_TAP, 400, Expect::Settled),
    // A long press selects the word and shows the handles.
    phone_bare(
        "phone-widgets-field-long-press",
        FIELD_LONG_PRESS,
        1200,
        Expect::Settled,
    ),
    // The end handle dragged right: the selection grows with it.
    phone_bare(
        "phone-widgets-handle-drag",
        HANDLE_DRAG,
        1000,
        Expect::Settled,
    ),
    // The same long press with the app's edit menu: the rows are the ones the field takes.
    phone(
        "phone-widgets-edit-menu",
        FIELD_LONG_PRESS,
        1200,
        Expect::Settled,
    ),
    // The notes field tapped low in the page and the keyboard 300 px up: the field is revealed.
    phone("phone-widgets-keyboard", KEYBOARD, 1200, Expect::Settled),
    // A long press at the top of the window opens its menu below the notch.
    phone(
        "phone-widgets-safe-area-menu",
        SAFE_AREA_MENU,
        800,
        Expect::Settled,
    ),
];

/// What the bench replays, in the gallery's own window: a pan over the slider, a tap in a field.
const BENCH: &[Step] = &[
    down!(0, centre(WIN.slider).0, centre(WIN.slider).1),
    drag!(16, centre(WIN.slider).0, centre(WIN.slider).1 - 60.0),
    drag!(33, centre(WIN.slider).0, centre(WIN.slider).1 - 120.0),
    up!(50, centre(WIN.slider).0, centre(WIN.slider).1 - 120.0),
    down!(1500, centre(WIN.name).0, centre(WIN.name).1),
    up!(1560, centre(WIN.name).0, centre(WIN.name).1),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Player, run_scene};
    use lune_ui::access::{Node, State, Value};
    use lune_ui::input::FrameOutput;
    use lune_ui::testing::FakeText;

    fn scene(id: &str) -> &'static UiScene {
        SCENES.iter().find(|s| s.id == id).expect("a scene")
    }

    fn capture(id: &str) -> FrameOutput {
        run_scene("touch-widgets", scene(id), &mut FakeText)
            .expect("the scene runs")
            .1
    }

    fn named<'a>(out: &'a FrameOutput, name: &str) -> Option<&'a Node> {
        out.tree.nodes.iter().find(|n| n.name == name)
    }

    /// The slider's value and the scroll area's offset, from their nodes.
    fn number(out: &FrameOutput, name: &str) -> f64 {
        let node = out
            .tree
            .nodes
            .iter()
            .find(|n| n.name == name && matches!(n.value, Some(Value::Number { .. })))
            .unwrap_or_else(|| panic!("no number node named {name}"));
        match node.value {
            Some(Value::Number { value, .. }) => value,
            _ => unreachable!(),
        }
    }

    /// Where the slider's node is: the page's scroll offset moves it.
    fn slider_y(out: &FrameOutput) -> Option<f32> {
        out.tree
            .nodes
            .iter()
            .find(|n| n.name == "Volume" && matches!(n.value, Some(Value::Number { .. })))
            .map(|n| n.bounds.y)
    }

    fn selection(out: &FrameOutput, name: &str) -> (usize, usize) {
        let n = named(out, name).expect("the field");
        let (a, b) = n.text.as_ref().expect("a text node").selection;
        (a.min(b), a.max(b))
    }

    fn focused(out: &FrameOutput, name: &str) -> bool {
        named(out, name).is_some_and(|n| n.states.contains(State::Focused))
    }

    #[test]
    fn a_tap_beside_the_small_checkbox_toggles_it() {
        let out = capture("phone-widgets-hit-area");
        let boxed = out
            .tree
            .nodes
            .iter()
            .find(|n| n.states.contains(State::Checked))
            .expect("a checked node");
        assert!(boxed.bounds.contains((BOX_TAP.0 - 12.0, BOX_TAP.1)));
        // The instrument: the tap is outside the checkbox's own 24 px hit, so only the target
        // around it can have taken it.
        assert!(
            !boxed.bounds.contains(BOX_TAP),
            "{:?} {BOX_TAP:?}",
            boxed.bounds
        );
        let rest = capture("phone-widgets-tooltip");
        assert!(
            rest.tree
                .nodes
                .iter()
                .all(|n| !n.states.contains(State::Checked))
        );
    }

    #[test]
    fn a_pan_from_the_slider_scrolls_the_page_and_a_drag_along_it_moves_the_value() {
        let rest = capture("phone-widgets-tooltip");
        let pan = capture("phone-widgets-slider-pan");
        assert_eq!(number(&rest, "Volume"), 30.0);
        assert_eq!(number(&pan, "Volume"), 30.0, "the pan leaves the value");
        let offset = slider_y;
        assert!(
            offset(&rest)
                .zip(offset(&pan))
                .is_some_and(|(a, b)| (a - b - 50.0).abs() < 1.0),
            "the page moved {:?} {:?}",
            offset(&rest),
            offset(&pan)
        );
        let drag = capture("phone-widgets-slider-drag");
        assert!(
            number(&drag, "Volume") > 60.0,
            "{}",
            number(&drag, "Volume")
        );
        assert_eq!(
            offset(&rest),
            offset(&drag),
            "a drag along the track scrolls nothing"
        );
    }

    #[test]
    fn a_pan_from_a_tab_scrolls_the_strip_and_activates_nothing() {
        let rest = capture("phone-widgets-tooltip");
        let pan = capture("phone-widgets-tabs-pan");
        let tab =
            |o: &FrameOutput, t: &str| named(o, t).map(|n| n.states.contains(State::Selected));
        assert_eq!(
            tab(&pan, "Tab 1"),
            Some(true),
            "the first tab stays the active one"
        );
        assert_eq!(
            tab(&pan, "Tab 2"),
            Some(false),
            "the tab under the finger was not activated"
        );
        let x = |o: &FrameOutput, t: &str| named(o, t).map(|n| n.bounds.x);
        assert!(
            x(&rest, "Tab 3")
                .zip(x(&pan, "Tab 3"))
                .is_some_and(|(a, b)| a - b > 100.0),
            "the strip moved {:?} {:?}",
            x(&rest, "Tab 3"),
            x(&pan, "Tab 3")
        );
        assert_eq!(slider_y(&rest), slider_y(&pan), "the page did not move");
    }

    #[test]
    fn a_finger_resting_on_the_icon_button_shows_its_tooltip() {
        let out = capture("phone-widgets-tooltip");
        assert!(
            out.tree
                .nodes
                .iter()
                .any(|n| n.description.as_deref() == Some("What this page shows")),
            "the anchor carries the tip"
        );
        let tip = |o: &FrameOutput| {
            o.tree
                .nodes
                .iter()
                .filter(|n| n.name == "What this page shows")
                .count()
        };
        let rest = capture("phone-widgets-hit-area");
        assert!(
            tip(&out) > tip(&rest),
            "the tooltip is drawn only under the finger"
        );
    }

    #[test]
    fn a_tap_places_the_caret_and_a_long_press_selects_the_word_with_handles() {
        let tap = capture("phone-widgets-field-tap");
        let (a, b) = selection(&tap, "Name");
        assert!(a == b && a > 0 && a < 12, "a caret inside the text: {a}");
        assert!(focused(&tap, "Name"));
        let long = capture("phone-widgets-field-long-press");
        assert_eq!(selection(&long, "Name"), (4, 12), "`Lovelace`");
        assert!(focused(&long, "Name"));
        assert!(named(&long, "Cut").is_none(), "no menu in this scene");
    }

    #[test]
    fn dragging_the_end_handle_extends_the_selection() {
        let out = capture("phone-widgets-handle-drag");
        let (a, b) = selection(&out, "Name");
        assert_eq!(a, 0);
        assert!(
            (4..12).contains(&b),
            "grown past `Ada` and short of the end: {b}"
        );
    }

    #[test]
    fn the_edit_menu_offers_what_the_field_takes_and_runs_the_pick() {
        let out = capture("phone-widgets-edit-menu");
        for row in ["Cut", "Copy", "Paste", "Select all"] {
            assert!(named(&out, row).is_some(), "{row}");
        }
        // The pick: a tap on `Select all` selects all of the field's text.
        let mut player = Player::for_scene("touch-widgets", scene("phone-widgets-edit-menu"));
        player.frame(&mut FakeText, 0, &[]).expect("frame");
        let mut last = None;
        for step in scene("phone-widgets-edit-menu").script {
            last = Some(
                player
                    .frame(&mut FakeText, step.at_ms, step.events)
                    .expect("frame")
                    .1,
            );
        }
        let at = centre(
            named(&last.expect("a frame"), "Select all")
                .expect("the row")
                .bounds,
        );
        let touch = [Ev::TouchStart(1, at.0, at.1)];
        player.frame(&mut FakeText, 800, &touch).expect("frame");
        let lift = [Ev::TouchEnd(1, at.0, at.1)];
        player.frame(&mut FakeText, 850, &lift).expect("frame");
        player.frame(&mut FakeText, 900, &[]).expect("frame");
        let (_, after) = player.frame(&mut FakeText, 1200, &[]).expect("frame");
        assert_eq!(selection(&after, "Name"), (0, 12), "select all ran");
        assert!(named(&after, "Cut").is_none(), "the menu closed");
    }

    #[test]
    fn the_keyboard_leaves_the_tapped_box_above_it() {
        let out = capture("phone-widgets-keyboard");
        let notes = named(&out, "Notes").expect("the notes box");
        assert!(
            notes.bounds.bottom() <= PHONE.1 as f32 - 300.0,
            "{:?}",
            notes.bounds
        );
        assert!(notes.states.contains(State::Focused));
    }

    #[test]
    fn a_menu_opened_at_the_top_is_placed_below_the_safe_area() {
        let out = capture("phone-widgets-safe-area-menu");
        let row = named(&out, "Settings").expect("the menu");
        assert!(row.bounds.y >= SAFE.top, "{:?}", row.bounds);
        assert!(STRIP_AT.1 < SAFE.top, "the press is inside the notch");
    }

    #[test]
    fn every_phone_scene_runs_in_the_phone_window_at_scale_3() {
        for s in SCENES {
            assert!(s.id.starts_with("phone-widgets-"), "{}", s.id);
            assert_eq!((s.window(), s.scale), (PHONE, 3.0), "{}", s.id);
        }
    }

    /// The bench's touches run through the page in the gallery's window without an error.
    #[test]
    fn the_bench_runs() {
        let mut player = Player::new(
            "touch-widgets",
            live(),
            (W as f32, H as f32),
            1.0,
            MotionTier::Full,
        );
        player.frame(&mut FakeText, 0, &[]).expect("frame");
        for step in BENCH {
            player
                .frame(&mut FakeText, step.at_ms, step.events)
                .expect("a bench frame");
        }
    }
}
