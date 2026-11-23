//! The stage: the screen the demo draws in every frame. A slowly moving background on the `Base`
//! layer; over it the glass frame, which is a side rail of the six scenes beside a stage card on a
//! wide window, and a title bar, a stage card and a bottom tab bar on a narrow one (a phone); and in
//! the card the current scene, which a click on the rail or the tabs replaces with a push.
//!
//! The background and the push are functions of the app's time (`RawInput::time`), so a frame is
//! deterministic: two dumps of the same state are the same bytes. Every colour comes from the
//! theme: the app only sets alphas and mixes theme colours.

use std::f32::consts::FRAC_PI_2;
use std::f64::consts::TAU;
use std::time::Duration;

use lune_ui::color::{Rgba, clear, mix, premul, with_alpha, with_alpha_byte};
use lune_ui::input::{CursorIcon, FrameOutput, Keymap, NamedKey, RawInput, keymap};
use lune_ui::layout::Insets;
use lune_ui::lunegit_dark;
use lune_ui::motion::{Cadence, Easing, Kind, Motion, MotionTier};
use lune_ui::paint::{Falloff, Rect, Xform};
use lune_ui::scene::{LayerId, Scene};
use lune_ui::shortcuts::Shortcuts;
use lune_ui::strings::{DisplayFormat, StringTable};
use lune_ui::text::{Spec, Style, TextCache, TextSystem};
use lune_ui::theme::Theme;
use lune_ui::ui::{FrameConfig, Response, RoveAxis, Sense, Ui, UiStore, WidgetId};
use lune_ui::widgets::nav_item::{self, NavLook};
use lune_ui::widgets::{nav_bar, page, tab_bar};

use crate::scenes::{SceneId, Scenes};

/// Below this window width, in logical px, the frame is the phone's: a title bar, the stage card and
/// a tab bar.
pub const NARROW_BELOW: f32 = 700.0;
/// The window edge to the content, and the gap between the blocks of a wide frame (the recipes'
/// `MARGIN` and `GAP`).
pub const MARGIN: f32 = 16.0;
/// The gap between the rail and the stage card.
pub const GAP: f32 = 12.0;
/// The rail's width: between the sidebar's 208 and 240.
pub const RAIL_W: f32 = 224.0;
/// The stage card's corner radius (`page::RADIUS`, `panel::RADIUS`).
pub const CARD_RADIUS: f32 = page::RADIUS;
/// The card's padding on a wide window (`page::PAD`).
pub const PAD: f32 = page::PAD;
/// The card's padding, and the distance from the edge to the card, on a narrow window.
pub const NARROW_PAD: f32 = 12.0;
/// The narrow title bar's row under the safe area (`nav_bar::HEIGHT`).
pub const TITLE_BAR: f32 = nav_bar::HEIGHT;
/// The narrow tab bar's row above the safe area (`tab_bar::HEIGHT`).
pub const TAB_BAR: f32 = tab_bar::HEIGHT;
/// The space between the rail's top and its first row: the brand block and a gap.
const RAIL_ROWS_TOP: f32 = 68.0;
/// The rail's footer line.
const RAIL_FOOTER: f32 = 20.0;
/// The tab cell's highlight inset (the tab bar's own).
const TAB_INSET: (f32, f32) = (6.0, 4.0);
/// What the push moves the scene it covers, as a share of the distance the new one travels.
const PARALLAX: f32 = 0.25;
/// The glyph size of a tab.
const TAB_GLYPH: f32 = 24.0;

/// The push between scenes: the `drawer` kind (`moderate`, sine-in-out, a fade under Reduced). The
/// duration is the theme's token, never a number here.
const PUSH: Motion = Motion::of(Kind::Drawer);

/// The app time the background rests at when it does not move (`--still`, and Reduced or Off
/// motion): a moment at which the blobs sit apart.
pub const STILL_AT: Duration = Duration::from_secs(9);

// ---------------------------------------------------------------------------------------------
// The background.

/// A colour of the theme a blob takes.
#[derive(Debug, Clone, Copy)]
enum Hue {
    Data,
    DataHi,
    Link,
    Ok,
    Err,
    /// Halfway between `data` and `err`.
    Violet,
}

impl Hue {
    fn of(self, t: &Theme) -> Rgba {
        match self {
            Hue::Data => t.data,
            Hue::DataHi => t.data_hi,
            Hue::Link => t.link,
            Hue::Ok => t.ok,
            Hue::Err => t.err,
            Hue::Violet => mix(t.data, t.err, 0.5),
        }
    }
}

/// One drifting light. The centre moves on a closed Lissajous path (two incommensurate periods),
/// the radius breathes, the colour leans slowly towards a second hue.
#[derive(Debug, Clone, Copy)]
pub struct Blob {
    /// The path's middle, as shares of the window's width and height.
    pub centre: (f32, f32),
    /// The path's half-extent, as shares of the width and the height.
    pub amp: (f32, f32),
    /// The periods of the two axes, in seconds.
    pub period: (f64, f64),
    /// The phases of the two axes, in turns.
    pub phase: (f64, f64),
    /// The ellipse radius, as shares of a 1440x900 window's width and height (scaled with the
    /// window's diagonal), before breathing.
    pub radius: (f32, f32),
    /// The breathing: the half-extent (a share of the radius) and the period in seconds.
    pub breath: (f32, f64),
    /// The colour at rest, and the one it leans towards over `lean.1` seconds, by at most `lean.0`.
    hue: Hue,
    to: Hue,
    lean: (f32, f64),
    /// The inner colour's alpha, 0 to 1.
    alpha: f32,
}

/// The window the blobs' radii are shares of.
const REFERENCE: (f32, f32) = (1440.0, 900.0);

/// The blobs: blue, rose, teal and violet (the theme's `data`, `err`, `ok` and their mix).
pub const BLOBS: [Blob; 4] = [
    Blob {
        centre: (0.24, 0.30),
        amp: (0.09, 0.10),
        period: (47.0, 58.0),
        phase: (0.00, 0.30),
        radius: (0.42, 0.62),
        breath: (0.10, 31.0),
        hue: Hue::Data,
        to: Hue::Link,
        lean: (0.7, 71.0),
        alpha: 0.66,
    },
    Blob {
        centre: (0.76, 0.34),
        amp: (0.10, 0.12),
        period: (53.0, 41.0),
        phase: (0.35, 0.10),
        radius: (0.38, 0.58),
        breath: (0.12, 37.0),
        hue: Hue::Err,
        to: Hue::Violet,
        lean: (0.6, 83.0),
        alpha: 0.58,
    },
    Blob {
        centre: (0.55, 0.82),
        amp: (0.12, 0.08),
        period: (59.0, 43.0),
        phase: (0.60, 0.70),
        radius: (0.44, 0.52),
        breath: (0.08, 43.0),
        hue: Hue::Ok,
        to: Hue::DataHi,
        lean: (0.8, 79.0),
        alpha: 0.50,
    },
    Blob {
        centre: (0.14, 0.78),
        amp: (0.08, 0.09),
        period: (37.0, 55.0),
        phase: (0.15, 0.85),
        radius: (0.34, 0.46),
        breath: (0.10, 29.0),
        hue: Hue::Violet,
        to: Hue::Data,
        lean: (0.5, 67.0),
        alpha: 0.52,
    },
];

/// The centre of blob `i` at app time `t` (seconds) in a window of `size` logical px.
pub fn blob_centre(i: usize, t: f64, size: (f32, f32)) -> (f32, f32) {
    let b = &BLOBS[i];
    let x = (t / b.period.0 + b.phase.0) * TAU;
    let y = (t / b.period.1 + b.phase.1) * TAU;
    (
        size.0 * (b.centre.0 + b.amp.0 * x.sin() as f32),
        size.1 * (b.centre.1 + b.amp.1 * y.sin() as f32),
    )
}

/// The most blob `i`'s centre can move per second in a window of `size`: the path's speed is at
/// most this on every instant (the axes' own peak speeds, added in quadrature).
pub fn blob_speed_bound(i: usize, size: (f32, f32)) -> f32 {
    let b = &BLOBS[i];
    let vx = f64::from(size.0 * b.amp.0) * TAU / b.period.0;
    let vy = f64::from(size.1 * b.amp.1) * TAU / b.period.1;
    vx.hypot(vy) as f32
}

/// The shortest period of any motion of the background (a path, the breathing or the colour's
/// lean), in seconds.
pub fn shortest_period() -> f64 {
    BLOBS
        .iter()
        .flat_map(|b| [b.period.0, b.period.1, b.breath.1, b.lean.1])
        .fold(f64::INFINITY, f64::min)
}

/// `c` made opaque.
fn opaque(c: Rgba) -> Rgba {
    with_alpha_byte(c, 255)
}

/// Draws the background into `win` on the `Base` layer at app time `t` (seconds): the theme's window
/// tint with a faint wash of its `data` and `err` colours, and the blobs as radial lights. It leaves
/// the painter on the `Page` layer.
fn draw_background(ui: &mut Ui<'_>, win: Rect, t: f64) {
    let th = *ui.theme();
    let size = (win.w, win.h);
    // The radii are shares of the reference window (1440x900), scaled by the window's diagonal: a
    // phone's tall window gets round lights, not bands.
    let reach = win.w.hypot(win.h) / REFERENCE.0.hypot(REFERENCE.1);
    let p = ui.painter();
    p.on(LayerId::Base);
    let base = opaque(th.glass.window.tint);
    p.fill(win, 0.0, base);
    p.linear(
        win,
        0.0,
        mix(base, th.data, 0.14),
        mix(base, th.err, 0.10),
        1.0,
    );
    for (i, b) in BLOBS.iter().enumerate() {
        let (cx, cy) = blob_centre(i, t, size);
        let breathe = 1.0 + b.breath.0 * (t / b.breath.1 * TAU).sin() as f32;
        let lean = b.lean.0 * (0.5 + 0.5 * (t / b.lean.1 * TAU + b.phase.0).sin() as f32);
        let colour = mix(b.hue.of(&th), b.to.of(&th), lean);
        p.radial(
            [win.x + cx, win.y + cy],
            [
                REFERENCE.0 * b.radius.0 * breathe * reach,
                REFERENCE.1 * b.radius.1 * breathe * reach,
            ],
            with_alpha(colour, b.alpha),
            clear(colour),
            Falloff::Gaussian,
        );
    }
    p.on(LayerId::Page);
}

// ---------------------------------------------------------------------------------------------
// The layout.

/// Where the frame's parts are in a window of `size` logical px with a `safe` area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// The window's size.
    pub size: (f32, f32),
    /// The safe area the layout kept clear.
    pub safe: Insets,
    /// The wide frame (a rail), or the narrow one (title bar and tab bar).
    pub wide: bool,
    /// The glass rail (wide); an empty rect when narrow.
    pub rail: Rect,
    /// The glass title bar from the window's top edge to the row's bottom (narrow); empty when wide.
    pub title_bar: Rect,
    /// The glass tab bar from the row's top to the window's bottom edge (narrow); empty when wide.
    pub tab_bar: Rect,
    /// The glass stage card the scenes draw in.
    pub card: Rect,
}

const NONE: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 0.0,
    h: 0.0,
};

impl Layout {
    /// The layout of a window of `size` with `safe` insets.
    pub fn new(size: (f32, f32), safe: Insets) -> Self {
        let (w, h) = size;
        if w >= NARROW_BELOW {
            let (l, t) = (MARGIN.max(safe.left), MARGIN.max(safe.top));
            let (r, b) = (MARGIN.max(safe.right), MARGIN.max(safe.bottom));
            let rail = Rect::new(l, t, RAIL_W, (h - t - b).max(0.0));
            let x = rail.right() + GAP;
            let card = Rect::new(x, t, (w - r - x).max(0.0), rail.h);
            Layout {
                size,
                safe,
                wide: true,
                rail,
                title_bar: NONE,
                tab_bar: NONE,
                card,
            }
        } else {
            let title_bar = Rect::new(0.0, 0.0, w, safe.top + TITLE_BAR);
            let tab_h = TAB_BAR + safe.bottom;
            let tab_bar = Rect::new(0.0, (h - tab_h).max(0.0), w, tab_h);
            let top = title_bar.bottom() + NARROW_PAD;
            let x = NARROW_PAD + safe.left;
            let card = Rect::new(
                x,
                top,
                (w - x - NARROW_PAD - safe.right).max(0.0),
                (tab_bar.y - NARROW_PAD - top).max(0.0),
            );
            Layout {
                size,
                safe,
                wide: false,
                rail: NONE,
                title_bar,
                tab_bar,
                card,
            }
        }
    }

    /// The card's padding.
    pub fn pad(&self) -> f32 {
        if self.wide { PAD } else { NARROW_PAD }
    }

    /// The card's content area: inside its padding.
    pub fn page(&self) -> Rect {
        self.card.inset(self.pad(), self.pad())
    }

    /// The header's height: a title and a subtitle (wide), or the subtitle alone (narrow: the title
    /// bar carries the title).
    pub fn header_h(&self, t: &Theme) -> f32 {
        if self.wide {
            t.line_title + 4.0 + (t.font_base * 1.3).round()
        } else {
            (t.font_base * 1.3).round()
        }
    }

    /// The scene's area: below the header.
    pub fn body(&self, t: &Theme) -> Rect {
        let page = self.page();
        let top = self.header_h(t) + self.pad();
        Rect::new(page.x, page.y + top, page.w, (page.h - top).max(0.0))
    }

    /// The rail's six rows, at `nav_item`'s height and pitch.
    pub fn rail_rows(&self) -> [Rect; 6] {
        let x = self.rail.x + nav_item::PAD;
        let w = (self.rail.w - 2.0 * nav_item::PAD).max(0.0);
        std::array::from_fn(|i| {
            Rect::new(
                x,
                self.rail.y + RAIL_ROWS_TOP + i as f32 * nav_item::PITCH,
                w,
                nav_item::ROW_HEIGHT,
            )
        })
    }

    /// The rail's brand block: the mark and the two lines beside it.
    pub fn brand(&self) -> Rect {
        Rect::new(
            self.rail.x + 16.0,
            self.rail.y + 16.0,
            (self.rail.w - 32.0).max(0.0),
            32.0,
        )
    }

    /// The rail's footer line (the backend's name; the frame-time readout later).
    pub fn footer(&self) -> Rect {
        Rect::new(
            self.rail.x + 16.0,
            self.rail.bottom() - 16.0 - RAIL_FOOTER,
            (self.rail.w - 32.0).max(0.0),
            RAIL_FOOTER,
        )
    }

    /// The tab bar's six cells (the row, above the safe area's bottom inset).
    pub fn tab_cells(&self) -> [Rect; 6] {
        let w = self.tab_bar.w / 6.0;
        std::array::from_fn(|i| {
            Rect::new(self.tab_bar.x + i as f32 * w, self.tab_bar.y, w, TAB_BAR)
        })
    }

    /// The title bar's row: under the safe area's top inset.
    pub fn title_row(&self) -> Rect {
        Rect::new(0.0, self.safe.top, self.size.0, TITLE_BAR)
    }
}

// ---------------------------------------------------------------------------------------------
// Glass.

/// The alpha of the panels' tint (the theme's panel glass tint): low, so the background glows
/// through.
const GLASS_ALPHA: f32 = 0.36;

/// A glass panel over `r`: the theme's glass tint frosted over the background with the noise, a faint
/// sheen from the top and the theme's popup hairline. `radius` 0 is a bar at the window's edge. It
/// asks for no backdrop blur: the background is made of soft gradients, so a blur of it changes
/// nothing a person sees, and a frame without a backdrop request never takes the renderer's
/// offscreen path (a request on a layer under `Popup` blurs the text of those layers too).
fn glass(ui: &mut Ui<'_>, r: Rect, radius: f32) {
    let t = *ui.theme();
    let p = ui.painter();
    p.frost(
        r,
        radius,
        premul(with_alpha(t.glass.panel.tint, GLASS_ALPHA)),
        true,
    );
    p.linear(
        r,
        radius,
        t.surface_hover,
        clear(t.surface_hover),
        FRAC_PI_2,
    );
    p.ring(r, radius, 1.0, t.menu_popup_border, false);
}

// ---------------------------------------------------------------------------------------------
// The navigation: one roving group of six cells, shared by the rail and the tab bar.

/// Registers `cells` as one roving group under `key` (`active` is the current stop) and returns the
/// index a click, Enter or Space chose and each cell's response.
fn nav_group(
    ui: &mut Ui<'_>,
    key: &str,
    cells: &[Rect],
    active: usize,
    axis: RoveAxis,
) -> (Option<usize>, Vec<Response>) {
    let id = ui.id(key);
    let items: Vec<(WidgetId, bool)> = (0..cells.len()).map(|i| (id.with(i), true)).collect();
    let stop = ui.rove_stop(&items, Some(active));
    let mut chose = None;
    let mut responses = Vec::with_capacity(cells.len());
    for (i, cell) in cells.iter().enumerate() {
        let sense = if stop == Some(i) {
            Sense::CLICK_FOCUS
        } else {
            Sense::CLICK_ROVE
        };
        let kid = items[i].0;
        let r = ui.interact(kid, *cell, sense);
        let enter = ui.consume_key(kid, NamedKey::Enter);
        let space = ui.consume_key(kid, NamedKey::Space);
        if r.clicked || enter || space {
            chose = Some(i);
        }
        if r.hovered {
            ui.set_cursor(CursorIcon::Pointer);
        }
        responses.push(r);
    }
    ui.rove_keys(&items, axis, false);
    (chose, responses)
}

/// How much cell `i` is the current one while the position is `pos`: 1 at it, 0 a cell away.
fn nearness(pos: f32, i: usize) -> f32 {
    (1.0 - (pos - i as f32).abs()).clamp(0.0, 1.0)
}

/// The wide frame's rail: glass, the brand, six rows with a highlight that moves to the current one,
/// and the footer. Returns the scene a row chose.
fn draw_rail(ui: &mut Ui<'_>, l: &Layout, pos: f32, backend: &str) -> Option<usize> {
    let t = *ui.theme();
    glass(ui, l.rail, CARD_RADIUS);
    let rows = l.rail_rows();
    let (chose, responses) = nav_group(ui, "rail", &rows, pos.round() as usize, RoveAxis::Vertical);
    let reduced = ui.tier() != MotionTier::Full;
    let shown = if reduced { pos.round() } else { pos };
    // The highlight: one rect that moves between the rows (kind `move`), under the labels.
    let top = rows[0].y + shown * nav_item::PITCH;
    ui.painter().fill(
        Rect::new(rows[0].x, top, rows[0].w, nav_item::ROW_HEIGHT),
        nav_item::RADIUS,
        t.surface_selected,
    );
    for (i, scene) in SceneId::ALL.iter().enumerate() {
        let kid = responses[i].id;
        let hov = ui.animate_f32(
            kid.with("hov"),
            level(responses[i].hovered),
            &nav_item::HOVER,
        );
        let press = ui.animate_f32(
            kid.with("press"),
            level(responses[i].pressed),
            &nav_item::HOVER,
        );
        let focus = ui.animate_f32(
            kid.with("focus"),
            level(responses[i].focus_visible),
            &nav_item::FOCUS,
        );
        let here = if reduced {
            level(i == shown as usize)
        } else {
            nearness(pos, i)
        };
        let mut look = NavLook::mixed(&t, 0.0, hov, press, focus);
        // The fill is the moving highlight's; the label and its weight follow how current the row is.
        look.label = mix(look.label, t.fg_bright, here);
        look.medium = here >= 0.5;
        nav_item::paint(
            ui.painter(),
            rows[i],
            scene.name(),
            Some(scene.glyph()),
            &look,
        );
    }
    // The brand.
    let b = l.brand();
    let mark = Rect::new(b.x, b.y, 32.0, 32.0);
    let p = ui.painter();
    p.linear(mark, 8.0, t.data, t.err, 0.9);
    let g = lune_ui::icon::glyph(SceneId::Glass.glyph(), 18.0, t.on_accent);
    p.text_centered(&g, mark);
    p.text(
        &Spec::plain("luneui", Style::ui(t.font_title, t.fg_bright).medium()),
        b.x + 44.0,
        b.y - 3.0,
    );
    p.text(
        &Spec::plain("web demo", Style::ui(t.font_s, t.fg_secondary)),
        b.x + 44.0,
        b.y + 21.0,
    );
    // The footer: a status dot and the backend.
    let f = l.footer();
    p.fill(
        Rect::new(f.x, f.y - 8.0, f.w, 1.0),
        0.0,
        t.surface_separator,
    );
    p.fill(Rect::new(f.x, f.y + 6.0, 8.0, 8.0), 4.0, t.ok);
    p.text_in(
        &Spec::plain(backend, Style::ui(t.font_s, t.fg_tertiary)),
        Rect::new(f.x + 16.0, f.y, (f.w - 16.0).max(0.0), f.h),
        0.0,
    );
    chose
}

fn level(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

/// The narrow frame's title bar and tab bar. Returns the scene a tab chose.
fn draw_bars(ui: &mut Ui<'_>, l: &Layout, pos: f32, current: SceneId) -> Option<usize> {
    let t = *ui.theme();
    glass(ui, l.title_bar, 0.0);
    glass(ui, l.tab_bar, 0.0);
    let title = Spec::plain(
        current.name(),
        Style::ui(t.font_title, t.fg_bright).medium(),
    );
    ui.painter().text_centered(&title, l.title_row());
    let cells = l.tab_cells();
    let (chose, responses) = nav_group(
        ui,
        "tabs",
        &cells,
        pos.round() as usize,
        RoveAxis::Horizontal,
    );
    let reduced = ui.tier() != MotionTier::Full;
    let shown = if reduced { pos.round() } else { pos };
    let w = cells[0].w;
    let marker =
        Rect::new(l.tab_bar.x + shown * w, l.tab_bar.y, w, TAB_BAR).inset(TAB_INSET.0, TAB_INSET.1);
    ui.painter().fill(marker, t.item_radius, t.surface_selected);
    for (i, scene) in SceneId::ALL.iter().enumerate() {
        let cell = cells[i];
        let kid = responses[i].id;
        let press = ui.animate_f32(
            kid.with("press"),
            level(responses[i].pressed),
            &nav_item::HOVER,
        );
        let here = if reduced {
            level(i == shown as usize)
        } else {
            nearness(pos, i)
        };
        let tint = mix(mix(t.fg_tertiary, t.fg_primary, here), t.fg_bright, press);
        let p = ui.painter();
        let g = lune_ui::icon::glyph(scene.glyph(), TAB_GLYPH, tint);
        let gw = p.measure(&g).0;
        p.text_centered(
            &g,
            Rect::new(
                cell.x + (cell.w - gw) / 2.0,
                cell.y + 2.0,
                gw,
                TAB_GLYPH * 1.25,
            ),
        );
        let label = p.elide(
            &Spec::plain(scene.name(), Style::ui(t.font_s, tint)),
            (cell.w - 4.0).max(0.0),
        );
        let lw = p.measure(&label).0;
        p.text(
            &label,
            cell.x + (cell.w - lw) / 2.0,
            cell.bottom() - 5.0 - 14.0 + 1.0,
        );
    }
    chose
}

// ---------------------------------------------------------------------------------------------
// The pages: the header and the scene, pushed in and out of the card.

/// How a page is drawn this frame.
#[derive(Debug, Clone, Copy)]
struct Show {
    layer: LayerId,
    /// The page's offset and opacity (the push).
    dx: f32,
    dy: f32,
    alpha: f32,
    /// The scrim over what is below, and the opacity of the plate the page slides on (0 for none).
    scrim: f32,
    plate: f32,
}

/// Draws `scene`'s page (its header and body) into the card, clipped to it.
fn draw_page(ui: &mut Ui<'_>, scenes: &mut Scenes, l: &Layout, scene: SceneId, show: Show) {
    let t = *ui.theme();
    let moved = show.dx != 0.0 || show.dy != 0.0 || show.alpha < 1.0;
    {
        let p = ui.painter();
        p.on(show.layer);
        p.push_clip_rounded(l.card, CARD_RADIUS);
        if show.scrim > 0.0 {
            p.fill(l.card, 0.0, with_alpha(t.dialog_backdrop, show.scrim));
        }
        if moved {
            p.push_xform(Xform {
                origin: (0.0, 0.0),
                scale: 1.0,
                dx: show.dx,
                dy: show.dy,
                alpha: show.alpha,
            });
        }
        if show.plate > 0.0 {
            let a = (show.plate.clamp(0.0, 1.0) * 255.0).round() as u8;
            p.fill(l.card, 0.0, with_alpha_byte(t.menu_glass, a));
        }
        // The header.
        let page = l.page();
        if l.wide {
            p.text(
                &Spec::plain(scene.name(), Style::ui(t.font_title, t.fg_bright).medium()),
                page.x,
                page.y,
            );
            let sub = p.elide(
                &Spec::plain(scene.subtitle(), Style::ui(t.font_base, t.fg_secondary)),
                page.w,
            );
            p.text(&sub, page.x, page.y + t.line_title + 4.0);
        } else {
            let sub = p.elide(
                &Spec::plain(scene.subtitle(), Style::ui(t.font_base, t.fg_secondary)),
                page.w,
            );
            p.text(&sub, page.x, page.y);
        }
    }
    let body = l.body(&t);
    ui.scope(scene.slug(), |ui| scenes.draw(scene, ui, body, l.wide));
    let p = ui.painter();
    if moved {
        p.pop_xform();
    }
    p.pop_clip();
    p.on(LayerId::Page);
}

// ---------------------------------------------------------------------------------------------
// The stage.

/// What a frame hands back.
#[derive(Debug)]
pub struct Rendered {
    /// The display list to draw.
    pub scene: Scene,
    /// What the frame asks of the app: repaint, cursor, clipboard.
    pub out: FrameOutput,
    /// Where the frame laid everything out.
    pub layout: Layout,
    /// The scene position: the index of the current scene, between two indices during a push.
    pub position: f32,
}

/// The demo's state and what one frame needs besides the GPU.
#[derive(Debug)]
pub struct Stage {
    /// The theme every widget draws with (the Theme scene changes it).
    pub theme: Theme,
    /// The motion tier: Full, Reduced (the page's `prefers-reduced-motion`) or Off.
    pub tier: MotionTier,
    /// The background does not move and asks for no frames (`--still`).
    pub still: bool,
    /// The scene the user chose: the push runs towards it.
    pub want: SceneId,
    /// The line the rail's footer shows: the GPU backend.
    pub backend: String,
    store: UiStore,
    strings: StringTable,
    shortcuts: Shortcuts,
    keymap: Option<Keymap>,
    scenes: Scenes,
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage {
    /// A stage on the Glass scene with the Lunegit dark theme and full motion.
    pub fn new() -> Self {
        Self {
            theme: lunegit_dark::THEME,
            tier: MotionTier::Full,
            still: false,
            want: SceneId::Glass,
            backend: String::new(),
            store: UiStore::default(),
            strings: StringTable::default(),
            shortcuts: Shortcuts::default(),
            keymap: None,
            scenes: Scenes::new(),
        }
    }

    /// Uses `keymap` for the shortcuts from now on (a page asks the browser which machine it is).
    pub fn set_keymap(&mut self, keymap: Keymap) {
        self.keymap = Some(keymap);
    }

    /// Chooses `scene`: the next frame starts the push towards it.
    pub fn go(&mut self, scene: SceneId) {
        self.want = scene;
    }

    /// Whether the background moves: full motion and not `--still`.
    pub fn animating(&self) -> bool {
        self.tier == MotionTier::Full && !self.still
    }

    /// The push's duration under the current tier: `moderate` at Full, the Reduced map's token
    /// under Reduced, none (a cut) under Off.
    pub fn push_duration(&self) -> Option<Duration> {
        self.theme
            .motion
            .resolve(&PUSH, self.tier)
            .map(|r| r.duration)
    }

    /// Runs one frame over `input` and returns what it drew and asked for.
    pub fn frame(&mut self, input: RawInput, text: &mut dyn TextSystem) -> Rendered {
        let animating = self.animating();
        let config = FrameConfig {
            theme: self.theme,
            tier: self.tier,
            // The background moves on every display frame; a still one asks for none.
            busy_cadence: Cadence::EveryFrame,
            strings: &self.strings,
            arg_format: &DisplayFormat,
            keymap: self.keymap.unwrap_or_else(keymap),
            shortcuts: &self.shortcuts,
        };
        let input = &input;
        let mut ui = Ui::begin(&mut self.store, input, config, text);
        let layout = Layout::new(input.size, ui.safe_area());
        let win = Rect::new(0.0, 0.0, input.size.0, input.size.1);
        let t = *ui.theme();

        // Where the push is: the scene position follows the chosen scene.
        let pos_id = ui.id("stage.position");
        let pos = ui
            .animate_f32(pos_id, self.want.index() as f32, &PUSH)
            .clamp(0.0, (SceneId::ALL.len() - 1) as f32);

        let clock = if animating { input.time } else { STILL_AT };
        draw_background(&mut ui, win, clock.as_secs_f64());
        if animating {
            ui.request_busy_frame(win);
        }

        // The glass and the navigation.
        glass(&mut ui, layout.card, CARD_RADIUS);
        let chose = if layout.wide {
            draw_rail(&mut ui, &layout, pos, &self.backend)
        } else {
            draw_bars(
                &mut ui,
                &layout,
                pos,
                SceneId::from_index(pos.round() as usize),
            )
        };
        if let Some(i) = chose {
            self.want = SceneId::from_index(i);
        }

        // The scenes: one at rest, two while a push runs.
        let near = pos.round();
        if (pos - near).abs() < 1e-3 {
            let at = SceneId::from_index(near as usize);
            let rest = Show {
                layer: LayerId::Page,
                dx: 0.0,
                dy: 0.0,
                alpha: 1.0,
                scrim: 0.0,
                plate: 0.0,
            };
            draw_page(&mut ui, &mut self.scenes, &layout, at, rest);
        } else {
            let lo = pos.floor();
            let f = pos - lo;
            let under = SceneId::from_index(lo as usize);
            let over = SceneId::from_index(lo as usize + 1);
            let (reach_x, reach_y) = if layout.wide {
                (0.0, layout.card.h)
            } else {
                (layout.card.w, 0.0)
            };
            let full = ui.tier() == MotionTier::Full;
            let (under_show, over_show) = if full {
                (
                    Show {
                        layer: LayerId::Page,
                        dx: -f * PARALLAX * reach_x,
                        dy: -f * PARALLAX * reach_y,
                        alpha: 1.0,
                        scrim: 0.0,
                        plate: 0.0,
                    },
                    Show {
                        layer: LayerId::Popup,
                        dx: (1.0 - f) * reach_x,
                        dy: (1.0 - f) * reach_y,
                        alpha: 1.0,
                        scrim: Easing::QuadOut.eval(f),
                        plate: (1.0 - f).sqrt(),
                    },
                )
            } else {
                // Reduced: no movement, the new page fades in over the old one.
                (
                    Show {
                        layer: LayerId::Page,
                        dx: 0.0,
                        dy: 0.0,
                        alpha: 1.0,
                        scrim: 0.0,
                        plate: 0.0,
                    },
                    Show {
                        layer: LayerId::Popup,
                        dx: 0.0,
                        dy: 0.0,
                        alpha: f,
                        scrim: 0.0,
                        plate: 1.0,
                    },
                )
            };
            draw_page(&mut ui, &mut self.scenes, &layout, under, under_show);
            draw_page(&mut ui, &mut self.scenes, &layout, over, over_show);
        }
        let _ = t;
        let (scene, out) = ui.finish();
        Rendered {
            scene,
            out,
            layout,
            position: pos,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Headless.

/// Drives a [`Stage`] in app time with no window and no GPU: the dump and the tests go through it.
#[derive(Debug)]
pub struct Headless {
    /// The app.
    pub stage: Stage,
    /// The fonts.
    pub text: TextCache,
    /// The window's size in logical points.
    pub size: (f32, f32),
    /// Device pixels per logical point.
    pub scale: f32,
    /// The safe area pushed with every frame.
    pub safe: Insets,
    /// App time: the time of the last frame.
    pub time: Duration,
}

impl Headless {
    /// A stage at time zero in a window of `size` at `scale` with `safe` insets.
    pub fn new(size: (f32, f32), scale: f32, safe: Insets) -> Self {
        Self {
            stage: Stage::new(),
            text: crate::text_cache(),
            size,
            scale,
            safe,
            time: Duration::ZERO,
        }
    }

    /// The layout of this window.
    pub fn layout(&self) -> Layout {
        Layout::new(self.size, self.safe)
    }

    /// One frame `dt` after the last, with `events` after the safe area.
    pub fn step(&mut self, dt: Duration, events: Vec<lune_ui::input::Event>) -> Rendered {
        self.step_to(self.time + dt, events)
    }

    /// One frame at app time `now` (no earlier than the last), with `events` after the safe area.
    pub fn step_to(&mut self, now: Duration, mut events: Vec<lune_ui::input::Event>) -> Rendered {
        self.time = self.time.max(now);
        events.insert(0, lune_ui::input::Event::SafeArea(self.safe));
        let input = RawInput {
            size: self.size,
            scale: self.scale,
            time: self.time,
            focused: true,
            events,
        };
        self.stage.frame(input, &mut self.text)
    }

    /// `frames` frames `dt` apart with no events; returns the last.
    pub fn run(&mut self, frames: u32, dt: Duration) -> Option<Rendered> {
        let mut last = None;
        for _ in 0..frames {
            last = Some(self.step(dt, Vec::new()));
        }
        last
    }
}
