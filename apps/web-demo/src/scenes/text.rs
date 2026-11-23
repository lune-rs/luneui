//! Text: a code view with syntax colours, a field, a Markdown source and the preview that follows
//! it. The autopilot is a person at the keyboard: it focuses the field, selects what is there and
//! types a new phrase at a human pace (40 to 80 ms a character and a longer pause after a word),
//! selects a line of the code, then types a Markdown document into the source while the preview
//! follows. It types through the widgets' own edit commands, so the caret, the selection and the
//! undo are the toolkit's, and a real key works in the same fields (on the web through the text
//! bridge). A person's hand holds it back for [`RESUME`](super::RESUME).
//!
//! The text itself never animates (`docs/motion.md`): only the caret, the typing and the selection
//! change. Under Reduced, Off and `--still` the scene shows its state at rest and types nothing.

use std::time::Duration;

use lune_ui::code_doc::CodeDoc;
use lune_ui::color::{Rgba, with_alpha};
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::text_edit::EditCommand;
use lune_ui::ui::Ui;
use lune_ui::widgets::code_editor::{CodeOptions, LineBand};
use lune_ui::widgets::field::FieldOptions;
use lune_ui::widgets::markdown::{MdBlock, MdInline, MdItem, MdOptions};
use lune_ui::widgets::stat::StatOptions;
use lune_ui::widgets::text_area::AreaOptions;
use lune_ui::widgets::text_lines::{HighlightSpan, Highlighter};

use super::{CARD_PAD, Entry, Env, GAP, Pilot, Targets, caps, scrolled, tile};

/// The pace of typing: a character takes this much at the least, and up to [`TYPE_MAX`].
pub const TYPE_MIN: Duration = Duration::from_millis(40);
/// The slowest a character takes.
pub const TYPE_MAX: Duration = Duration::from_millis(80);
/// The extra pause after a space or a comma.
pub const WORD_PAUSE: Duration = Duration::from_millis(170);

/// The code view's text: a few lines of the toolkit's own API, none longer than 28 characters so
/// every line fits the code view on the narrowest window (320 px) without a sideways scroll.
pub const CODE: &str = "use lune_ui::prelude::*;

/// A glass card.
fn card(ui: &mut Ui) {
    let t = *ui.theme();
    let r = ui.rect();

    // Frost and a hairline.
    let p = ui.painter();
    p.frost(r, 12, t.glass);
    p.ring(r, 12, t.border);

    // One button inside.
    let b = r.inset(16);
    if ui.button(\"Go\", b) {
        ui.toast(\"Opened\");
    }
}";

/// What the field is typed to, round by round.
const PHRASES: [&str; 4] = [
    "Ship the web demo",
    "Glass over a moving light",
    "Hello from the canvas",
    "Type here, a real key works too",
];

/// The Markdown documents the source is typed to, round by round.
const DOCS: [&str; 3] = [
    "# Glass, made of code\nEvery **card** is a frost over the *moving* light.\n\n- A tint and a hairline\n- Drawn by `Painter::frost`\n\n> One canvas, one renderer.",
    "## Release 0.7.7\nThe toolkit now runs in a **browser**.\n\n- WebGPU, or WebGL2\n- One `<canvas>`, no DOM UI\n\n> Fully animated, all of it.",
    "# Fast on purpose\nText is shaped once and **drawn from an atlas**.\n\n- Only the lines in view\n- Selection, caret and *IME*\n\n> Typing never animates.",
];

/// The phrase the field is typed to in round `k`.
pub fn phrase(k: u64) -> &'static str {
    PHRASES[(k as usize) % PHRASES.len()]
}

/// What the field and the source hold at rest.
const FIELD_AT_REST: &str = "Hello from the canvas";

/// One part of the scene the script reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The one-line field.
    Field,
    /// The code view.
    Code,
    /// The Markdown source.
    Source,
}

/// One thing the script does.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Puts the focus on a part.
    Focus(Part),
    /// Selects everything in a part.
    SelectAll(Part),
    /// Selects the line of a part that holds this char offset.
    SelectLine(Part, usize),
    /// Types a character into a part.
    Type(Part, char),
}

/// The time a character takes after the one before it.
fn pace(n: u64, ch: char) -> Duration {
    let x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
    let k = (x % 1000) as f32 / 1000.0;
    let mut d = TYPE_MIN + (TYPE_MAX - TYPE_MIN).mul_f32(k);
    if ch == ' ' || ch == ',' || ch == '\n' {
        d += WORD_PAUSE;
    }
    d
}

/// How long a selection is held before the script goes on.
pub const SELECTION_HOLD: Duration = Duration::from_millis(1800);

/// The char offset of the start of the second line of `text`.
fn second_line(text: &str) -> usize {
    text.chars().position(|c| c == '\n').map_or(0, |n| n + 1)
}

/// Round `k` of the script: what is done and when, and how long the round lasts.
pub fn round(k: u64) -> (Vec<(Duration, Act)>, Duration) {
    let mut acts = Vec::new();
    let mut t = Duration::from_millis(300);
    let ms = Duration::from_millis;
    let mut push = |t: Duration, a: Act| acts.push((t, a));
    // The field: focus it, select what it holds, and type over it.
    push(t, Act::Focus(Part::Field));
    t += ms(500);
    push(t, Act::SelectAll(Part::Field));
    t += ms(900);
    let phrase = phrase(k);
    for (i, ch) in phrase.chars().enumerate() {
        push(t, Act::Type(Part::Field, ch));
        t += pace(k * 97 + i as u64, ch);
    }
    t += ms(1200);
    // The code: the whole text selected (a read-only view takes select all and copy), held.
    push(t, Act::Focus(Part::Code));
    t += ms(250);
    push(t, Act::SelectAll(Part::Code));
    t += SELECTION_HOLD;
    // The source: select it and type a document, then select its first line and hold.
    push(t, Act::Focus(Part::Source));
    t += ms(400);
    push(t, Act::SelectAll(Part::Source));
    t += ms(800);
    let text = DOCS[(k as usize) % DOCS.len()];
    for (i, ch) in text.chars().enumerate() {
        push(t, Act::Type(Part::Source, ch));
        t += pace(k * 131 + i as u64, ch);
    }
    t += ms(700);
    push(t, Act::SelectLine(Part::Source, second_line(text)));
    t += SELECTION_HOLD;
    (acts, t)
}

/// One of the cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Card {
    /// The code view.
    Code,
    /// The field.
    Field,
    /// The counts of the Markdown source.
    Counts,
    /// What to try.
    Hint,
    /// The Markdown source and its preview.
    Markdown,
}

/// Where the cards sit in a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Every card and its rect, in the body's coordinates.
    pub cards: Vec<(Card, Rect)>,
    /// The content's height from the body's top.
    pub height: f32,
}

/// The lines the code view shows on a wide window and on a narrow one.
const CODE_LINES: (f32, f32) = (18.0, 18.0);

/// The height of a card whose code view shows `lines` lines: the padding, the caption and the lines.
fn code_h(lines: f32) -> f32 {
    20.0 * lines + 2.0 * CARD_PAD + 22.0
}

const FIELD_H: f32 = 116.0;
const COUNTS_H: f32 = 104.0;
const HINT_H: f32 = 112.0;

/// Lays the cards out: with room, the code on the left beside the field, the counts and a hint, and
/// under them the Markdown pair across the width; without, one stacked column (which scrolls).
pub fn plan(body: Rect, wide: bool) -> Plan {
    if wide && body.w >= 900.0 {
        let lw = ((body.w - GAP) * 0.56).round();
        let rw = body.w - GAP - lw;
        let top = code_h(CODE_LINES.0);
        let md_h = (body.h - top - GAP).max(300.0);
        let x2 = body.x + lw + GAP;
        let hint_h = (top - FIELD_H - COUNTS_H - 2.0 * GAP).max(HINT_H);
        Plan {
            cards: vec![
                (Card::Code, Rect::new(body.x, body.y, lw, top)),
                (Card::Field, Rect::new(x2, body.y, rw, FIELD_H)),
                (
                    Card::Counts,
                    Rect::new(x2, body.y + FIELD_H + GAP, rw, COUNTS_H),
                ),
                (
                    Card::Hint,
                    Rect::new(x2, body.y + FIELD_H + COUNTS_H + 2.0 * GAP, rw, hint_h),
                ),
                (
                    Card::Markdown,
                    Rect::new(body.x, body.y + top + GAP, body.w, md_h),
                ),
            ],
            height: top + GAP + md_h,
        }
    } else {
        let mut y = body.y;
        let mut cards = Vec::new();
        for (card, h) in [
            (Card::Field, FIELD_H),
            (Card::Code, code_h(CODE_LINES.1)),
            (Card::Markdown, 440.0),
            (Card::Counts, COUNTS_H),
            (Card::Hint, HINT_H),
        ] {
            cards.push((card, Rect::new(body.x, y, body.w, h)));
            y += h + GAP;
        }
        Plan {
            cards,
            height: y - GAP - body.y,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The highlighter and the Markdown reader.

/// The colours of the code view's tokens, all the theme's own.
#[derive(Debug, Clone, Copy)]
struct Palette {
    keyword: Rgba,
    string: Rgba,
    comment: Rgba,
    number: Rgba,
    kind: Rgba,
    function: Rgba,
}

impl Palette {
    fn of(t: &lune_ui::theme::Theme) -> Self {
        Self {
            keyword: t.link,
            string: t.ok,
            comment: t.fg_tertiary,
            number: t.err,
            kind: t.data_hi,
            function: t.warn,
        }
    }
}

const KEYWORDS: [&str; 11] = [
    "use", "fn", "let", "if", "mut", "for", "in", "pub", "struct", "impl", "match",
];

/// A small Rust tokenizer, by line: comments, strings, numbers, keywords, types, calls.
struct Rust(Palette);

impl Highlighter for Rust {
    fn spans(&self, _line: usize, text: &str) -> Vec<HighlightSpan> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut i = 0;
        let span = |start: usize, end: usize, color: Rgba| HighlightSpan { start, end, color };
        while i < chars.len() {
            let c = chars[i];
            if c == '/' && chars.get(i + 1) == Some(&'/') {
                out.push(span(i, chars.len(), self.0.comment));
                break;
            }
            if c == '"' {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '"' {
                    j += 1;
                }
                out.push(span(i, (j + 1).min(chars.len()), self.0.string));
                i = j + 1;
                continue;
            }
            if c.is_ascii_digit() {
                let mut j = i;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                    j += 1;
                }
                out.push(span(i, j, self.0.number));
                i = j;
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let mut j = i;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let word: String = chars[i..j].iter().collect();
                let call = chars.get(j) == Some(&'(');
                let colour = if KEYWORDS.contains(&word.as_str()) {
                    Some(self.0.keyword)
                } else if c.is_uppercase() {
                    Some(self.0.kind)
                } else if call {
                    Some(self.0.function)
                } else {
                    None
                };
                if let Some(colour) = colour {
                    out.push(span(i, j, colour));
                }
                i = j;
                continue;
            }
            i += 1;
        }
        out
    }
}

/// One inline run, owned: its text and how it is drawn.
#[derive(Debug, Clone, PartialEq)]
struct Run {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
}

/// One block, owned.
#[derive(Debug, Clone, PartialEq)]
enum Block {
    Heading(u8, Vec<Run>),
    Paragraph(Vec<Run>),
    Quote(Vec<Run>),
    List(Vec<Vec<Run>>),
}

/// The inline runs of `s`: `**bold**`, `*italic*` and `` `code` `` where they close, else the
/// marks stay as typed (a half-typed mark is plain text).
fn runs(s: &str) -> Vec<Run> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<Run> = Vec::new();
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut Vec<Run>| {
        if !plain.is_empty() {
            out.push(Run {
                text: std::mem::take(plain),
                bold: false,
                italic: false,
                code: false,
            });
        }
    };
    let find = |from: usize, pat: &[char]| -> Option<usize> {
        (from..chars.len().saturating_sub(pat.len() - 1)).find(|&k| chars[k..k + pat.len()] == *pat)
    };
    let mut i = 0;
    while i < chars.len() {
        let rest = &chars[i..];
        let (pat, bold, italic, code): (&[char], bool, bool, bool) =
            if rest.starts_with(&['*', '*']) {
                (&['*', '*'], true, false, false)
            } else if rest.starts_with(&['*']) {
                (&['*'], false, true, false)
            } else if rest.starts_with(&['`']) {
                (&['`'], false, false, true)
            } else {
                plain.push(chars[i]);
                i += 1;
                continue;
            };
        match find(i + pat.len(), pat).filter(|&e| e > i + pat.len()) {
            Some(end) => {
                flush(&mut plain, &mut out);
                out.push(Run {
                    text: chars[i + pat.len()..end].iter().collect(),
                    bold,
                    italic,
                    code,
                });
                i = end + pat.len();
            }
            None => {
                plain.extend(pat);
                i += pat.len();
            }
        }
    }
    flush(&mut plain, &mut out);
    out
}

/// Reads the Markdown subset the demo types: headings, paragraphs, bullet lists and quotes.
fn parse(source: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut list: Vec<Vec<Run>> = Vec::new();
    let end_para = |para: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            blocks.push(Block::Paragraph(runs(&para.join(" "))));
            para.clear();
        }
    };
    let end_list = |list: &mut Vec<Vec<Run>>, blocks: &mut Vec<Block>| {
        if !list.is_empty() {
            blocks.push(Block::List(std::mem::take(list)));
        }
    };
    for line in source.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            end_para(&mut para, &mut blocks);
            end_list(&mut list, &mut blocks);
        } else if let Some(rest) = t.strip_prefix("- ") {
            end_para(&mut para, &mut blocks);
            list.push(runs(rest));
        } else if let Some(rest) = t.strip_prefix("> ") {
            end_para(&mut para, &mut blocks);
            end_list(&mut list, &mut blocks);
            blocks.push(Block::Quote(runs(rest)));
        } else if t.starts_with('#') {
            end_para(&mut para, &mut blocks);
            end_list(&mut list, &mut blocks);
            let level = t.chars().take_while(|c| *c == '#').count().min(6) as u8;
            let rest = t.trim_start_matches('#');
            if rest.starts_with(' ') {
                blocks.push(Block::Heading(level, runs(rest.trim_start())));
            } else {
                para.push(t);
            }
        } else {
            end_list(&mut list, &mut blocks);
            para.push(t);
        }
    }
    end_para(&mut para, &mut blocks);
    end_list(&mut list, &mut blocks);
    blocks
}

/// The inline runs as the widget's.
fn inlines(rs: &[Run]) -> Vec<MdInline<'_>> {
    rs.iter()
        .map(|r| {
            if r.code {
                MdInline::Code(&r.text)
            } else {
                MdInline::Text {
                    text: &r.text,
                    bold: r.bold,
                    italic: r.italic,
                    strike: false,
                }
            }
        })
        .collect()
}

/// Draws `blocks` as the Markdown widget under `key` in `rect`.
fn preview(ui: &mut Ui<'_>, key: &str, rect: Rect, blocks: &[Block]) {
    // The widget's blocks borrow the inline lists, the list items' paragraphs and the items, so
    // each layer is built whole before the next borrows it.
    fn runs_of(b: &Block) -> Vec<Vec<MdInline<'_>>> {
        match b {
            Block::Heading(_, r) | Block::Paragraph(r) | Block::Quote(r) => vec![inlines(r)],
            Block::List(items) => items.iter().map(|r| inlines(r)).collect(),
        }
    }
    let inl: Vec<Vec<Vec<MdInline<'_>>>> = blocks.iter().map(runs_of).collect();
    let paras: Vec<Vec<[MdBlock<'_>; 1]>> = inl
        .iter()
        .zip(blocks)
        .map(|(lists, b)| match b {
            Block::List(_) => lists.iter().map(|l| [MdBlock::Paragraph(l)]).collect(),
            Block::Quote(_) => lists.iter().map(|l| [MdBlock::Paragraph(l)]).collect(),
            _ => Vec::new(),
        })
        .collect();
    let items: Vec<Vec<MdItem<'_>>> = paras
        .iter()
        .zip(blocks)
        .map(|(ps, b)| match b {
            Block::List(_) => ps
                .iter()
                .map(|p| MdItem {
                    marker: "\u{2022}",
                    blocks: p,
                    task: None,
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    let md: Vec<MdBlock<'_>> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| match b {
            Block::Heading(level, _) => MdBlock::Heading {
                level: *level,
                inlines: &inl[i][0],
            },
            Block::Paragraph(_) => MdBlock::Paragraph(&inl[i][0]),
            Block::Quote(_) => MdBlock::Quote(&paras[i][0]),
            Block::List(_) => MdBlock::List { items: &items[i] },
        })
        .collect();
    let opts = MdOptions {
        label: "Preview",
        tasks_live: false,
        identity: None,
    };
    ui.markdown(key, rect, &md, &opts);
}

// ---------------------------------------------------------------------------------------------
// The scene.

/// The Text scene's state.
#[derive(Debug)]
pub struct TextScene {
    /// The autopilot.
    pub pilot: Pilot,
    /// When the page was entered.
    pub entry: Entry,
    /// The field's text.
    pub field: String,
    /// The Markdown source.
    pub source: String,
    /// The code the view shows.
    pub doc: CodeDoc,
    /// The round of the script and the time into it.
    pub round: u64,
    clock: Duration,
    last: Option<Duration>,
    next: usize,
    seen_entries: u32,
    script: Vec<(Duration, Act)>,
    length: Duration,
    /// The controls of the last frame.
    pub targets: Targets,
}

impl Default for TextScene {
    fn default() -> Self {
        let doc = CodeDoc::new(CODE);
        let (script, length) = round(0);
        Self {
            pilot: Pilot::default(),
            entry: Entry::default(),
            field: FIELD_AT_REST.to_owned(),
            source: DOCS[0].to_owned(),
            doc,
            round: 0,
            clock: Duration::ZERO,
            last: None,
            next: 0,
            seen_entries: 0,
            script,
            length,
            targets: Targets::default(),
        }
    }
}

impl TextScene {
    /// Starts the script again from its first round.
    pub fn restart(&mut self) {
        self.round = 0;
        self.clock = Duration::ZERO;
        self.next = 0;
        let (script, length) = round(0);
        self.script = script;
        self.length = length;
    }

    /// Advances the script to the app time `env.now` and runs what has come due through the
    /// widgets' edit commands. Called with the widgets' scope current.
    fn play(&mut self, ui: &mut Ui<'_>, env: &Env) {
        let dt = self
            .last
            .map_or(Duration::ZERO, |l| env.now.saturating_sub(l))
            .min(Duration::from_millis(250));
        self.last = Some(env.now);
        if !self.pilot.live(env) {
            return;
        }
        self.clock += dt;
        while self.clock >= self.length {
            self.clock -= self.length;
            self.round += 1;
            let (script, length) = round(self.round);
            self.script = script;
            self.length = length;
            self.next = 0;
        }
        let ids = [
            (Part::Field, ui.id("field")),
            (Part::Code, ui.id("code")),
            (Part::Source, ui.id("source")),
        ];
        let id_of = |p: Part| ids.iter().find(|(q, _)| *q == p).map(|(_, i)| *i);
        while let Some((at, act)) = self.script.get(self.next).cloned() {
            if at > self.clock {
                break;
            }
            self.next += 1;
            self.pilot.acted += 1;
            // A phone's keyboard would rise with the focus: the script keeps its hands off it
            // there and types into the fields all the same.
            match act {
                Act::Focus(p) => {
                    if env.wide
                        && let Some(id) = id_of(p)
                    {
                        ui.request_focus(id);
                    }
                }
                Act::SelectAll(p) => {
                    if let Some(id) = id_of(p) {
                        ui.edit_command(id, EditCommand::SelectAll);
                    }
                }
                Act::SelectLine(p, at) => {
                    if let Some(id) = id_of(p) {
                        ui.edit_command(id, EditCommand::SelectLineAt(at));
                    }
                }
                Act::Type(p, ch) => {
                    if let Some(id) = id_of(p) {
                        ui.edit_command(id, EditCommand::Insert(ch.to_string()));
                    }
                }
            }
        }
    }

    /// Draws the scene into `body`.
    pub fn draw(&mut self, ui: &mut Ui<'_>, body: Rect, env: &Env) {
        self.pilot.observe(ui, body.inset(-env.pad(), 0.0), env);
        self.entry.elapsed(env);
        if self.entry.entries != self.seen_entries {
            self.seen_entries = self.entry.entries;
            self.restart();
        }
        if env.animating {
            ui.request_busy_frame(body);
        }
        let plan = plan(body, env.wide);
        self.targets.clear();
        scrolled(ui, body, env.pad(), plan.height, |ui| {
            self.play(ui, env);
            for (_, r) in &plan.cards {
                tile(ui, *r, 0.0);
            }
            for (card, r) in &plan.cards {
                let inner = r.inset(CARD_PAD, CARD_PAD);
                match card {
                    Card::Code => self.code_card(ui, inner),
                    Card::Field => self.field_card(ui, inner),
                    Card::Counts => self.counts_card(ui, inner),
                    Card::Hint => self.hint_card(ui, inner),
                    Card::Markdown => self.markdown_card(ui, inner),
                }
            }
        });
    }

    fn code_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let t = *ui.theme();
        caps(ui, "card.rs", inner.x, inner.y);
        let view = Rect::new(
            inner.x - 6.0,
            inner.y + 22.0,
            inner.w + 12.0,
            inner.h - 22.0,
        );
        let highlight = Rust(Palette::of(&t));
        let bands: Vec<LineBand> = Vec::new();
        let opts = CodeOptions {
            read_only: true,
            wrap: false,
            gutter: true,
            label: "Code",
            highlighter: &highlight,
            bands: &bands,
            ..CodeOptions::default()
        };
        ui.code_editor("code", view, &mut self.doc, &opts);
        self.targets.note("code", view);
    }

    fn field_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        caps(ui, "Field", inner.x, inner.y);
        let r = Rect::new(inner.x, inner.y + 28.0, inner.w, 36.0);
        let opts = FieldOptions {
            placeholder: "Type something",
            label: Some("Field"),
            ..FieldOptions::default()
        };
        ui.text_field_with("field", r, &mut self.field, &opts);
        self.targets.note("field", r);
        let t = *ui.theme();
        let hint = format!("{} characters", self.field.chars().count());
        super::line(
            ui,
            &hint,
            inner.x,
            r.bottom() + 16.0,
            inner.w,
            t.font_s,
            t.fg_tertiary,
        );
    }

    fn counts_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let words = self.source.split_whitespace().count();
        let chars = self.source.chars().count();
        let lines = self.source.lines().count().max(1);
        let n = |v: f64| format!("{}", v as u64);
        let texts = [n(words as f64), n(chars as f64), n(lines as f64)];
        let cell = |i: usize, label: &'static str, v: usize| StatOptions {
            label,
            value: &texts[i],
            number: Some((v as f64, &n)),
            live: true,
            ..StatOptions::default()
        };
        let cells = [
            cell(0, "Words", words),
            cell(1, "Characters", chars),
            cell(2, "Lines", lines),
        ];
        ui.stat_strip(
            "counts",
            Rect::new(
                inner.x - CARD_PAD,
                inner.y - CARD_PAD + 8.0,
                inner.w + 2.0 * CARD_PAD,
                0.0,
            ),
            &cells,
        );
    }

    fn hint_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let t = *ui.theme();
        caps(ui, "Try it", inner.x, inner.y);
        let lines = [
            "Click a field and type: a real keyboard works.",
            "Drag to select, double-click a word, triple-click a line.",
            "Cmd or Ctrl and A selects all; Tab moves to the next field.",
            "On a phone the field raises the keyboard.",
            "The preview follows the source as you type.",
        ];
        let mut y = inner.y + 28.0;
        for l in lines {
            let spec = Spec::plain(l, Style::ui(t.font_base, t.fg_secondary)).wrapped(inner.w);
            let h = ui.painter().text(&spec, inner.x, y).1;
            y += h + 9.0;
        }
    }

    fn markdown_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let t = *ui.theme();
        let side = inner.w >= 560.0;
        caps(ui, "Markdown", inner.x, inner.y);
        let area = Rect::new(inner.x, inner.y + 26.0, inner.w, inner.h - 26.0);
        let (src, prev) = if side {
            let w = (area.w - GAP) / 2.0;
            (
                Rect::new(area.x, area.y, w, area.h),
                Rect::new(area.x + w + GAP, area.y, w, area.h),
            )
        } else {
            let h = ((area.h - GAP) * 0.42).round();
            (
                Rect::new(area.x, area.y, area.w, h),
                Rect::new(area.x, area.y + h + GAP, area.w, area.h - h - GAP),
            )
        };
        let opts = AreaOptions {
            placeholder: "Write some Markdown",
            wrap: true,
            label: Some("Markdown source"),
            ..AreaOptions::default()
        };
        ui.text_area("source", src, &mut self.source, &opts);
        self.targets.note("source", src);
        // The preview sits on a quiet inset of its own.
        let p = ui.painter();
        p.fill(prev, 8.0, with_alpha(t.surface_overlay, 0.8));
        let blocks = parse(&self.source);
        preview(ui, "preview", prev, &blocks);
        self.targets.note("preview", prev);
    }
}
