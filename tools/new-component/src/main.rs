//! `new-component`: scaffolds a component drawn by `Ui` (`scripts/new-component.sh <area> <slug>`).
//!
//! It writes four files (the catalogue entry from `docs/components/_template.md`, the widget
//! module, its test file and a gallery page with one scene) and wires them into the build with four
//! one-line edits (`widgets/mod.rs`, `pages/mod.rs`, `PAGES` and the area's `GROUPS` entry). Every
//! file text and every edit is computed from the files as they are before anything is written, and
//! each anchor must be found exactly once, so a reshaped file stops the tool (exit 2) instead of
//! being edited by guess. Nothing it writes holds a date, so a run is byte-stable.
//!
//! Exit 0 done (or `--dry-run`), 1 a write failed, 2 usage or a refusal (a bad name, a target that
//! exists, an anchor that is not found exactly once, a table it cannot read). Areas outside
//! `widget`, `overlay`, `layout` and `data` are added by hand (`docs/components/adding.md`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// What the usage line says.
const USAGE: &str = "usage: new-component --root <repo> <area> <slug> [--title \"<Title>\"] [--owner <name>] [--dry-run]\n  area: widget, overlay, layout or data (others: see docs/components/adding.md)\n  slug: lowercase letters and digits, single hyphens, starting with a letter";

/// The areas the scaffold covers (R1-D6) with the sidebar group each lands in.
const AREAS: [(&str, &str); 4] = [
    ("widget", "Widgets"),
    ("overlay", "Overlays"),
    ("layout", "Layout"),
    ("data", "Data"),
];

/// Words a module name cannot be: the strict and reserved keywords of edition 2024.
const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// The arguments of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    area: String,
    slug: String,
    title: Option<String>,
    owner: String,
    dry_run: bool,
}

/// Reads the command line.
fn parse(args: &[String]) -> Result<Args, String> {
    let (mut root, mut title, mut owner, mut dry_run) = (None, None, None, false);
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--dry-run" => dry_run = true,
            "--root" | "--title" | "--owner" => {
                let Some(v) = args.get(i + 1).filter(|v| !v.starts_with("--")) else {
                    return Err(format!("{a} needs a value"));
                };
                let slot = match a {
                    "--root" => &mut root,
                    "--title" => &mut title,
                    _ => &mut owner,
                };
                if slot.replace(v.clone()).is_some() {
                    return Err(format!("{a} given more than once"));
                }
                i += 1;
            }
            _ if a.starts_with("--") => return Err(format!("unknown argument `{a}`")),
            _ => positional.push(a.to_owned()),
        }
        i += 1;
    }
    let [area, slug] = &positional[..] else {
        return Err("expected <area> and <slug>".to_owned());
    };
    let root = root.ok_or("--root <repo> is required")?;
    Ok(Args {
        root: PathBuf::from(root),
        area: area.clone(),
        slug: slug.clone(),
        title,
        owner: owner.unwrap_or_else(|| "scaffold".to_owned()),
        dry_run,
    })
}

/// The sidebar group of `area`, or the refusal for an area the scaffold does not cover.
fn group_of(area: &str) -> Result<&'static str, String> {
    AREAS
        .iter()
        .find(|(a, _)| *a == area)
        .map(|(_, g)| *g)
        .ok_or_else(|| {
            format!(
                "area `{area}` is not scaffolded (widget, overlay, layout, data); add a component of another area by hand: docs/components/adding.md"
            )
        })
}

/// Lowercase letters and digits with single hyphens, starting with a letter (so the module name is
/// an identifier), and not a Rust keyword.
fn validate_slug(slug: &str) -> Result<(), String> {
    let words_ok = !slug.is_empty()
        && slug.split('-').all(|w| {
            !w.is_empty()
                && w.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        });
    if !words_ok {
        return Err(format!(
            "slug `{slug}` is not lowercase letters and digits with single hyphens"
        ));
    }
    if !slug.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(format!("slug `{slug}` must start with a letter"));
    }
    let module = module_name(slug);
    if KEYWORDS.contains(&module.as_str()) {
        return Err(format!("slug `{slug}` is a Rust keyword as a module name"));
    }
    Ok(())
}

/// A title or owner goes into front matter and string literals: one line, no quote or backslash.
fn validate_text(what: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.contains(['"', '\\']) || text.chars().any(char::is_control) {
        return Err(format!(
            "{what} must be one non-empty line without a quote or a backslash"
        ));
    }
    Ok(())
}

/// `probe-chip` -> `probe_chip`.
fn module_name(slug: &str) -> String {
    slug.replace('-', "_")
}

/// `probe-chip` -> `ProbeChip`.
fn type_name(slug: &str) -> String {
    slug.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// `probe-chip` -> `Probe chip`.
fn default_title(slug: &str) -> String {
    let spaced = slug.replace('-', " ");
    let mut c = spaced.chars();
    c.next()
        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
        .unwrap_or_default()
}

/// The names a component's files and code carry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Names {
    area: String,
    slug: String,
    module: String,
    ty: String,
    title: String,
    owner: String,
    group: &'static str,
}

impl Names {
    /// Validates `args` into names.
    fn of(args: &Args) -> Result<Names, String> {
        let group = group_of(&args.area)?;
        validate_slug(&args.slug)?;
        let title = args
            .title
            .clone()
            .unwrap_or_else(|| default_title(&args.slug));
        validate_text("--title", &title)?;
        validate_text("--owner", &args.owner)?;
        Ok(Names {
            area: args.area.clone(),
            slug: args.slug.clone(),
            module: module_name(&args.slug),
            ty: type_name(&args.slug),
            title: title.trim().to_owned(),
            owner: args.owner.trim().to_owned(),
            group,
        })
    }

    fn entry_path(&self) -> String {
        format!("docs/components/{}/{}.md", self.area, self.slug)
    }
    fn widget_path(&self) -> String {
        format!("crates/lune-ui-core/src/widgets/{}.rs", self.module)
    }
    fn test_path(&self) -> String {
        format!("crates/lune-ui-core/tests/widget_{}.rs", self.module)
    }
    fn page_path(&self) -> String {
        format!("apps/gallery/src/pages/{}.rs", self.module)
    }
    fn scene(&self) -> String {
        format!("{}-default", self.slug)
    }
}

// ---- the edits ------------------------------------------------------------------------------

/// `text` with `line` inserted at line index `at`.
fn insert_line(text: &str, at: usize, line: &str) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    lines.insert(at, line);
    lines.join("\n")
}

/// `name` of a `mod`/`pub mod`/`pub(crate) mod` line `line` (up to its `;`), if it is one.
fn mod_name<'a>(line: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    let rest = prefixes.iter().find_map(|p| line.strip_prefix(p))?;
    let name = rest.strip_suffix(';')?;
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
    .then_some(name)
}

/// Inserts `new_line` (a module line of `module`) in sorted position into the one run of lines
/// that [`mod_name`] reads with `prefixes`. Fails when the run is not found exactly once, or when
/// `module` is already in it.
fn insert_mod(
    text: &str,
    prefixes: &[&str],
    module: &str,
    new_line: &str,
    anchor: &str,
) -> Result<String, String> {
    let lines: Vec<&str> = text.split('\n').collect();
    // The runs of consecutive module lines: (first index, names).
    let mut runs: Vec<(usize, Vec<&str>)> = Vec::new();
    let mut open = false;
    for (i, l) in lines.iter().enumerate() {
        match mod_name(l, prefixes) {
            Some(n) if open => runs.last_mut().map(|r| r.1.push(n)).unwrap_or_default(),
            Some(n) => {
                runs.push((i, vec![n]));
                open = true;
            }
            None => open = false,
        }
    }
    let [(first, names)] = &runs[..] else {
        return Err(format!(
            "{anchor}: expected one run of module lines, found {}",
            runs.len()
        ));
    };
    if names.contains(&module) {
        return Err(format!("{anchor}: `{module}` is already declared"));
    }
    let at = names
        .iter()
        .position(|n| *n > module)
        .unwrap_or(names.len());
    Ok(insert_line(text, first + at, new_line))
}

/// `pub mod <module>;` into `widgets/mod.rs`.
fn edit_widgets_mod(text: &str, module: &str) -> Result<String, String> {
    insert_mod(
        text,
        &["pub mod "],
        module,
        &format!("pub mod {module};"),
        "crates/lune-ui-core/src/widgets/mod.rs `pub mod` run",
    )
}

/// `mod <module>;` into `pages/mod.rs`, among its `mod` and `pub(crate) mod` lines.
fn edit_pages_mod(text: &str, module: &str) -> Result<String, String> {
    insert_mod(
        text,
        &["mod ", "pub(crate) mod "],
        module,
        &format!("mod {module};"),
        "apps/gallery/src/pages/mod.rs `mod` run",
    )
}

/// `&<module>::PAGE,` as the last element of `PAGES`.
fn edit_pages_array(text: &str, module: &str) -> Result<String, String> {
    const OPEN: &str = "pub(crate) static PAGES: &[&Page] = &[";
    let anchor = "apps/gallery/src/pages/mod.rs `PAGES`";
    if text.matches(OPEN).count() != 1 {
        return Err(format!("{anchor}: `{OPEN}` not found exactly once"));
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let open = lines
        .iter()
        .position(|l| *l == OPEN)
        .ok_or_else(|| format!("{anchor}: `{OPEN}` is not alone on its line"))?;
    let close = lines[open..]
        .iter()
        .position(|l| *l == "];")
        .map(|i| open + i)
        .ok_or_else(|| format!("{anchor}: no closing `];`"))?;
    let entry = format!("    &{module}::PAGE,");
    if lines[open..close].contains(&entry.as_str()) {
        return Err(format!("{anchor}: `{}` is already listed", entry.trim()));
    }
    Ok(insert_line(text, close, &entry))
}

/// `"<slug>"` as the last page of the group called `group` in `GROUPS`.
fn edit_groups(text: &str, group: &str, slug: &str) -> Result<String, String> {
    let name_line = format!("        name: \"{group}\",");
    let anchor = format!("apps/gallery/src/showcase/nav.rs group `{group}`");
    let lines: Vec<&str> = text.split('\n').collect();
    let named: Vec<usize> = (0..lines.len())
        .filter(|i| lines[*i] == name_line)
        .collect();
    let [name_at] = named[..] else {
        return Err(format!(
            "{anchor}: `{}` found {} times, not once",
            name_line.trim(),
            named.len()
        ));
    };
    let close = lines[name_at..]
        .iter()
        .position(|l| l.trim() == "],")
        .map(|i| name_at + i)
        .ok_or_else(|| format!("{anchor}: no closing `],` of its pages"))?;
    let entry = format!("            \"{slug}\",");
    if lines[name_at..close].contains(&entry.as_str()) {
        return Err(format!("{anchor}: `{}` is already in it", entry.trim()));
    }
    Ok(insert_line(text, close, &entry))
}

// ---- the new files --------------------------------------------------------------------------

/// The catalogue entry: the template with the names filled in. Fails when the template has no
/// longer the lines it is filled by, or a `<...>` placeholder is left in the front matter.
fn fill_template(template: &str, n: &Names) -> Result<String, String> {
    let source = format!("{}, {}, {}", n.widget_path(), n.test_path(), n.page_path());
    let fills: [(&str, String); 6] = [
        ("id: <area>.<slug>", format!("id: {}.{}", n.area, n.slug)),
        ("title: <Human title>", format!("title: {}", n.title)),
        ("area: <area>", format!("area: {}", n.area)),
        ("source: —", format!("source: {source}")),
        ("owner: —", format!("owner: {}", n.owner)),
        (
            "summary: <one line, what it is>",
            format!(
                "summary: {} (scaffolded by scripts/new-component.sh; replace with one line)",
                n.title
            ),
        ),
    ];
    let scene_line = "- `<slug>-<what>`: <state setup>, t = <ms or settled>";
    let mut out = template.to_owned();
    for (from, to) in fills.iter().map(|(f, t)| (*f, t.as_str())).chain([(
        scene_line,
        &*format!(
            "- `{}`: the widget at rest in a 160 by 32 box, settled",
            n.scene()
        ),
    )]) {
        let found = out.lines().filter(|l| *l == from).count();
        if found != 1 {
            return Err(format!(
                "docs/components/_template.md: the line `{from}` is found {found} times, not once"
            ));
        }
        out = out
            .lines()
            .map(|l| if l == from { to } else { l })
            .collect::<Vec<_>>()
            .join("\n")
            + if template.ends_with('\n') { "\n" } else { "" };
    }
    let front = out
        .split("---\n")
        .nth(1)
        .ok_or("docs/components/_template.md has no front matter")?;
    if let Some(left) = front.lines().find(|l| l.contains('<')) {
        return Err(format!(
            "docs/components/_template.md: the front matter still holds a placeholder: `{left}`"
        ));
    }
    Ok(out)
}

/// The widget module stub.
fn widget_source(n: &Names) -> String {
    let (m, t) = (&n.module, &n.ty);
    format!(
        r#"//! {title} (`{area}.{slug}`): scaffolded by `scripts/new-component.sh`. Replace this module doc
//! with what the component is and the body with its real drawing; the checklist in
//! `docs/components/README.md` says what it must hold.

use std::hash::Hash;

use crate::access::{{Node, Role}};
use crate::paint::Rect;
use crate::ui::{{Response, Sense, Ui}};

/// What [`Ui::{m}`] is given besides its key and rect.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct {t}Options<'a> {{
    /// The accessible name of the node.
    pub label: &'a str,
}}

impl<'a> {t}Options<'a> {{
    /// These options with the accessible name `label`.
    #[must_use]
    pub fn label(mut self, label: &'a str) -> Self {{
        self.label = label;
        self
    }}
}}

impl Ui<'_> {{
    /// A themed box in `rect` under `key`: a rounded fill from `ui.theme()` and a `Group` node
    /// named by `opts.label`. Returns the box's response. A scaffold: replace the drawing.
    pub fn {m}(&mut self, key: impl Hash, rect: Rect, opts: &{t}Options<'_>) -> Response {{
        let id = self.id(key);
        let response = self.interact(id, rect, Sense::HOVER);
        let fill = self.theme().surface_hover;
        self.painter().fill(rect, 6.0, fill);
        self.node(Node::new(id, Role::Group, opts.label, rect));
        response
    }}
}}
"#,
        title = n.title,
        area = n.area,
        slug = n.slug,
    )
}

/// The test file stub.
fn test_source(n: &Names) -> String {
    let (m, t) = (&n.module, &n.ty);
    format!(
        r#"//! The `{slug}` widget (`{area}.{slug}`), scaffolded by `scripts/new-component.sh`: it registers
//! its accessibility node. Add the cases of its acceptance list here.

use lune_ui_core::access::Role;
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::UiHarness;
use lune_ui_core::widgets::{m}::{t}Options;

#[test]
fn the_{m}_registers_its_node() {{
    let mut h = UiHarness::new((200.0, 100.0));
    let rect = Rect::new(10.0, 10.0, 80.0, 32.0);
    let (_, out) = h.frame(|ui| {{
        ui.{m}("k", rect, &{t}Options::default().label("probe"));
    }});
    let node = out.tree.nodes.iter().find(|n| n.name == "probe");
    let node = node.expect("the widget registers a node named by its label");
    assert_eq!(node.role, Role::Group);
    assert_eq!(node.bounds, rect);
}}
"#,
        slug = n.slug,
        area = n.area,
    )
}

/// The gallery page stub.
fn page_source(n: &Names) -> String {
    let (m, t) = (&n.module, &n.ty);
    format!(
        r#"//! The `{slug}` page: the {title} component (`{area}.{slug}`), scaffolded by
//! `scripts/new-component.sh`. Draw every state the entry lists, one scene each.

use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::ui::Ui;
use lune_ui::widgets::{m}::{t}Options;

use crate::page::Stage;
use crate::page::{{Expect, Page, PageKind, PageState, UiPage, UiScene}};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {{
    id: "{slug}",
    title: "{title}",
    kind: PageKind::Ui(UiPage {{
        state: sheet,
        scenes: SCENES,
        bench: &[],
    }}),
}};

fn sheet() -> Box<dyn PageState> {{
    Box::new(Sheet {{ wide: false }})
}}

/// The widget at rest, laid out from the stage's rect; the knob widens it.
struct Sheet {{
    wide: bool,
}}

impl PageState for Sheet {{
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {{
        let window = stage.area(ui);
        let background = ui.theme().glass.window.tint;
        ui.painter().fill(window, 0.0, background);
        let width = if self.wide {{ 320.0 }} else {{ 160.0 }};
        let at = Rect::new(window.x + 16.0, window.y + 16.0, width, 32.0);
        ui.{m}("{slug}", at, &{t}Options::default().label("{title}"));
    }}

    /// Every page of the sidebar has a knob (a test holds it): replace this with the component's
    /// own options.
    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {{
        panel.toggle(ui, "Wide", &mut self.wide);
    }}
}}

const SCENES: &[UiScene] = &[UiScene {{
    id: "{scene}",
    scale: 2.0,
    tier: MotionTier::Full,
    state: sheet,
    script: &[],
    capture_ms: 0,
    expect: Expect::Settled,
}}];
"#,
        slug = n.slug,
        title = n.title,
        area = n.area,
        scene = n.scene(),
    )
}

// ---- the checklist ----------------------------------------------------------------------------

/// A checklist row that is left to a person: its id, what it asks and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    id: String,
    what: String,
    kind: String,
}

/// The rows of the table between the markers of `docs/components/README.md` whose kind is not
/// `auto:...`. A table it cannot read is an error.
fn read_by_hand_rules(readme: &str) -> Result<Vec<Rule>, String> {
    const BEGIN: &str = "<!-- checklist:begin -->";
    const END: &str = "<!-- checklist:end -->";
    let begin = readme
        .find(BEGIN)
        .ok_or_else(|| format!("docs/components/README.md has no `{BEGIN}`"))?;
    let end = readme
        .find(END)
        .ok_or_else(|| format!("docs/components/README.md has no `{END}`"))?;
    let table = readme
        .get(begin + BEGIN.len()..end)
        .ok_or("the checklist markers are in the wrong order")?;
    let mut out = Vec::new();
    for line in table.lines().map(str::trim).filter(|l| l.starts_with('|')) {
        let cells: Vec<&str> = line
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split(" | ")
            .map(str::trim)
            .collect();
        if cells.first() == Some(&"rule") || cells.iter().all(|c| c.chars().all(|ch| ch == '-')) {
            continue;
        }
        let [id, what, _, _, kind] = cells[..] else {
            return Err(format!(
                "checklist row `{line}` has {} cells, not 5",
                cells.len()
            ));
        };
        if !kind.starts_with("auto:") {
            out.push(Rule {
                id: id.to_owned(),
                what: what.to_owned(),
                kind: kind.to_owned(),
            });
        }
    }
    if out.is_empty() {
        return Err("the checklist table holds no rule left to a person".to_owned());
    }
    Ok(out)
}

/// The commands to run next and the rules still to hold by hand, as text.
fn next_steps(n: &Names, rules: &[Rule]) -> Vec<String> {
    let mut out = vec![
        "next, in order:".to_owned(),
        "  scripts/gen-component-index.sh".to_owned(),
        "  cargo test -p gallery showcase".to_owned(),
        format!(
            "  cargo run -p gallery -- --dump <dir> --only {}",
            n.scene()
        ),
        "  scripts/gen-api-docs.sh".to_owned(),
        format!("  scripts/check-component.sh {}.{}", n.area, n.slug),
    ];
    for (heading, kind) in [
        ("review", "review"),
        ("test", "test"),
        ("touch and phone [mobile slot]", "[mobile slot]"),
    ] {
        let mine: Vec<&Rule> = rules.iter().filter(|r| r.kind == kind).collect();
        if mine.is_empty() {
            continue;
        }
        out.push(format!("still to do by hand, {heading}:"));
        out.extend(mine.iter().map(|r| format!("  {}: {}", r.id, r.what)));
    }
    out
}

// ---- the plan ---------------------------------------------------------------------------------

/// Everything a run does, computed before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    /// New files: path from the root, text.
    files: Vec<(String, String)>,
    /// Edited files: path from the root, the new whole text, what was inserted.
    edits: Vec<(String, String, String)>,
    /// What to print after.
    next: Vec<String>,
}

/// Reads `rel` under `root`.
fn read(root: &Path, rel: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}: {e}"))
}

/// Every `.rs` file under `dir`, recursively (paths from `root`), for the id collision scan.
fn rust_files(root: &Path, dir: &str, out: &mut Vec<String>) -> Result<(), String> {
    let read_dir = std::fs::read_dir(root.join(dir)).map_err(|e| format!("{dir}: {e}"))?;
    let mut entries: Vec<_> = read_dir
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{dir}: {e}"))?;
    entries.sort();
    for name in entries {
        let rel = format!("{dir}/{name}");
        if root.join(&rel).is_dir() {
            rust_files(root, &rel, out)?;
        } else if name.ends_with(".rs") {
            out.push(rel);
        }
    }
    Ok(())
}

/// Computes the plan, or the refusal. Reads only.
fn plan(root: &Path, args: &Args) -> Result<Plan, String> {
    let n = Names::of(args)?;
    // Nothing that is already there is touched.
    for target in [
        n.entry_path(),
        n.widget_path(),
        n.test_path(),
        n.page_path(),
    ] {
        if root.join(&target).exists() {
            return Err(format!("{target} exists; nothing written"));
        }
    }
    let mut sources = Vec::new();
    rust_files(root, "apps/gallery/src", &mut sources)?;
    for needle in [
        format!("id: \"{}\"", n.slug),
        format!("id: \"{}\"", n.scene()),
    ] {
        for f in &sources {
            if read(root, f)?.contains(&needle) {
                return Err(format!("{f} already holds `{needle}`; nothing written"));
            }
        }
    }
    let template = read(root, "docs/components/_template.md")?;
    let readme = read(root, "docs/components/README.md")?;
    let rules = read_by_hand_rules(&readme)?;
    let files = vec![
        (n.entry_path(), fill_template(&template, &n)?),
        (n.widget_path(), widget_source(&n)),
        (n.test_path(), test_source(&n)),
        (n.page_path(), page_source(&n)),
    ];
    let widgets = "crates/lune-ui-core/src/widgets/mod.rs";
    let pages = "apps/gallery/src/pages/mod.rs";
    let nav = "apps/gallery/src/showcase/nav.rs";
    let pages_text = read(root, pages)?;
    let with_mod = edit_pages_mod(&pages_text, &n.module)?;
    let edits = vec![
        (
            widgets.to_owned(),
            edit_widgets_mod(&read(root, widgets)?, &n.module)?,
            format!("pub mod {};", n.module),
        ),
        (
            pages.to_owned(),
            edit_pages_array(&with_mod, &n.module)?,
            format!("mod {}; and &{}::PAGE, in PAGES", n.module, n.module),
        ),
        (
            nav.to_owned(),
            edit_groups(&read(root, nav)?, n.group, &n.slug)?,
            format!("\"{}\" at the end of group {}", n.slug, n.group),
        ),
    ];
    Ok(Plan {
        files,
        edits,
        next: next_steps(&n, &rules),
    })
}

/// Writes a plan: the new files, then the edits.
fn apply(root: &Path, plan: &Plan) -> Result<(), String> {
    for (rel, text) in &plan.files {
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{rel}: {e}"))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{rel}: {e}"))?;
    }
    for (rel, text, _) in &plan.edits {
        std::fs::write(root.join(rel), text).map_err(|e| format!("{rel}: {e}"))?;
    }
    Ok(())
}

/// Prints what a plan does (`dry_run`) or did.
fn report(plan: &Plan, dry_run: bool) {
    let verb = if dry_run { "would write" } else { "wrote" };
    for (rel, text) in &plan.files {
        println!("{verb} {rel} ({} lines)", text.lines().count());
    }
    let verb = if dry_run { "would edit" } else { "edited" };
    for (rel, _, what) in &plan.edits {
        println!("{verb} {rel}: {what}");
    }
    for line in &plan.next {
        println!("{line}");
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("new-component: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let plan = match plan(&args.root, &args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("new-component: {e}");
            return ExitCode::from(2);
        }
    };
    if !args.dry_run
        && let Err(e) = apply(&args.root, &plan)
    {
        eprintln!("new-component: write failed: {e} (some files may be written; see git status)");
        return ExitCode::from(1);
    }
    report(&plan, args.dry_run);
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn real_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn names(area: &str, slug: &str) -> Names {
        Names::of(&Args {
            root: PathBuf::from("."),
            area: area.to_owned(),
            slug: slug.to_owned(),
            title: None,
            owner: "scaffold".to_owned(),
            dry_run: false,
        })
        .expect("valid names")
    }

    // -- the command line and the names ----------------------------------------------------

    #[test]
    fn the_command_line_reads_its_flags_in_any_order() {
        let a = parse(&args(&[
            "--root",
            "/r",
            "widget",
            "probe-chip",
            "--title",
            "A chip",
            "--dry-run",
            "--owner",
            "me",
        ]))
        .expect("parses");
        assert_eq!(a.area, "widget");
        assert_eq!(a.slug, "probe-chip");
        assert_eq!(
            (a.title.as_deref(), a.owner.as_str(), a.dry_run),
            (Some("A chip"), "me", true)
        );
        let d = parse(&args(&["widget", "x", "--root", "/r"])).expect("parses");
        assert_eq!(
            (d.owner.as_str(), d.dry_run, d.title),
            ("scaffold", false, None)
        );
    }

    #[test]
    fn a_bad_command_line_is_an_error() {
        for bad in [
            args(&["widget", "x"]),
            args(&["--root", "/r", "widget"]),
            args(&["--root", "/r", "widget", "x", "y"]),
            args(&["--root", "/r", "widget", "x", "--nope"]),
            args(&["--root", "/r", "--root", "/s", "widget", "x"]),
            args(&["--root", "/r", "widget", "x", "--title"]),
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_four_areas_are_scaffolded_and_another_points_at_the_guide() {
        for (a, g) in AREAS {
            assert_eq!(group_of(a), Ok(g));
        }
        let e = group_of("render").expect_err("render is by hand");
        assert!(e.contains("docs/components/adding.md"), "{e}");
    }

    #[test]
    fn a_slug_is_lowercase_words_with_single_hyphens_starting_with_a_letter() {
        for ok in ["probe-chip", "a", "x1", "tab-strip-2"] {
            assert_eq!(validate_slug(ok), Ok(()), "{ok}");
        }
        for bad in [
            "", "Probe", "a--b", "-a", "a-", "1a", "a_b", "a b", "type", "loop", "self", "gen",
            "try", "yield", "priv", "typeof", "unsized", "virtual",
        ] {
            assert!(validate_slug(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn titles_and_owners_are_one_line_without_quotes() {
        assert!(validate_text("--title", "A chip").is_ok());
        for bad in ["", "  ", "a\"b", "a\\b", "a\nb"] {
            assert!(validate_text("--title", bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn names_follow_the_slug() {
        assert_eq!(module_name("probe-chip"), "probe_chip");
        assert_eq!(type_name("probe-chip"), "ProbeChip");
        assert_eq!(type_name("a"), "A");
        assert_eq!(default_title("probe-chip"), "Probe chip");
        let n = names("overlay", "probe-chip");
        assert_eq!(n.group, "Overlays");
        assert_eq!(n.entry_path(), "docs/components/overlay/probe-chip.md");
        assert_eq!(
            n.widget_path(),
            "crates/lune-ui-core/src/widgets/probe_chip.rs"
        );
        assert_eq!(
            n.test_path(),
            "crates/lune-ui-core/tests/widget_probe_chip.rs"
        );
        assert_eq!(n.page_path(), "apps/gallery/src/pages/probe_chip.rs");
        assert_eq!(n.scene(), "probe-chip-default");
    }

    // -- the edits -------------------------------------------------------------------------

    const WIDGETS: &str = "//! doc\n\npub mod badge;\npub mod button;\npub mod tone;\n\nmod input_box;\n\npub use button::button;\n";

    #[test]
    fn a_module_goes_in_sorted_position_at_the_start_the_middle_and_the_end() {
        let at_start = edit_widgets_mod(WIDGETS, "alert").expect("inserts");
        assert!(
            at_start.contains("pub mod alert;\npub mod badge;"),
            "{at_start}"
        );
        let middle = edit_widgets_mod(WIDGETS, "chip").expect("inserts");
        assert!(
            middle.contains("pub mod button;\npub mod chip;\npub mod tone;"),
            "{middle}"
        );
        let end = edit_widgets_mod(WIDGETS, "zoom").expect("inserts");
        assert!(
            end.contains("pub mod tone;\npub mod zoom;\n\nmod input_box;"),
            "{end}"
        );
        // Only the one line is new; the rest is as it was.
        assert_eq!(middle.replace("pub mod chip;\n", ""), WIDGETS);
    }

    #[test]
    fn a_module_that_is_already_declared_or_a_run_that_is_not_found_once_is_refused() {
        assert!(
            edit_widgets_mod(WIDGETS, "button")
                .expect_err("declared")
                .contains("already")
        );
        assert!(
            edit_widgets_mod("//! doc\nmod x;\n", "chip")
                .expect_err("no run")
                .contains("found 0")
        );
        let two = "pub mod a;\n\nfn f() {}\npub mod b;\n";
        assert!(
            edit_widgets_mod(two, "c")
                .expect_err("two runs")
                .contains("found 2")
        );
        // A private `mod` line is not a `pub mod` line.
        assert!(edit_widgets_mod("mod input_box;\n", "chip").is_err());
    }

    const PAGES_MOD: &str = "use x;\n\nmod a11y;\nmod button;\npub(crate) mod effects;\nmod window;\n\npub(crate) static PAGES: &[&Page] = &[\n    &a11y::PAGE,\n    &window::PAGE,\n];\n\nfn card() {}\n";

    #[test]
    fn the_pages_module_line_reads_mod_and_pub_crate_mod_lines_as_one_run() {
        let t = edit_pages_mod(PAGES_MOD, "d").expect("inserts");
        assert!(
            t.contains("mod button;\nmod d;\npub(crate) mod effects;"),
            "{t}"
        );
        let t = edit_pages_mod(PAGES_MOD, "eg").expect("inserts");
        assert!(
            t.contains("pub(crate) mod effects;\nmod eg;\nmod window;"),
            "{t}"
        );
        assert!(edit_pages_mod(PAGES_MOD, "window").is_err());
    }

    #[test]
    fn the_page_goes_last_in_pages_and_the_anchor_must_be_there_once() {
        let t = edit_pages_array(PAGES_MOD, "chip").expect("inserts");
        assert!(
            t.contains("    &window::PAGE,\n    &chip::PAGE,\n];"),
            "{t}"
        );
        assert!(
            edit_pages_array("fn f() {}\n", "chip")
                .expect_err("no anchor")
                .contains("not found exactly once")
        );
        let twice = format!("{PAGES_MOD}{PAGES_MOD}");
        assert!(edit_pages_array(&twice, "chip").is_err());
        let unclosed = "pub(crate) static PAGES: &[&Page] = &[\n    &a::PAGE,\n";
        assert!(
            edit_pages_array(unclosed, "chip")
                .expect_err("no close")
                .contains("closing")
        );
        assert!(
            edit_pages_array(PAGES_MOD, "window")
                .expect_err("listed")
                .contains("already")
        );
    }

    const GROUPS: &str = "pub(crate) const GROUPS: &[Group] = &[\n    Group {\n        name: \"Widgets\",\n        pages: &[\n            \"button\",\n            \"toggle\",\n        ],\n    },\n    Group {\n        name: \"Layout\",\n        pages: &[\n            \"stack\",\n        ],\n    },\n];\n";

    #[test]
    fn a_page_goes_last_in_its_group_and_the_group_must_be_found_once() {
        let t = edit_groups(GROUPS, "Widgets", "chip").expect("inserts");
        assert!(t.contains("            \"toggle\",\n            \"chip\",\n        ],\n    },\n    Group {\n        name: \"Layout\""), "{t}");
        let t = edit_groups(GROUPS, "Layout", "chip").expect("inserts");
        assert!(
            t.contains("\"stack\",\n            \"chip\",\n        ],"),
            "{t}"
        );
        assert!(
            edit_groups(GROUPS, "Data", "chip")
                .expect_err("no group")
                .contains("0 times")
        );
        let twice = format!("{GROUPS}{GROUPS}");
        assert!(
            edit_groups(&twice, "Widgets", "chip")
                .expect_err("twice")
                .contains("2 times")
        );
        assert!(
            edit_groups(GROUPS, "Widgets", "toggle")
                .expect_err("listed")
                .contains("already")
        );
    }

    // -- the new files ---------------------------------------------------------------------

    const TEMPLATE: &str = include_str!("../../../docs/components/_template.md");

    #[test]
    fn the_template_fill_leaves_no_placeholder_in_the_front_matter() {
        let n = names("widget", "probe-chip");
        let out = fill_template(TEMPLATE, &n).expect("fills");
        let front = out.split("---\n").nth(1).expect("front matter");
        assert!(!front.contains('<'), "{front}");
        for line in [
            "id: widget.probe-chip",
            "title: Probe chip",
            "area: widget",
            "status: planned",
            "consumers: any",
            "owner: scaffold",
        ] {
            assert!(front.lines().any(|l| l == line), "{line}\n{front}");
        }
        assert!(front.contains("source: crates/lune-ui-core/src/widgets/probe_chip.rs, crates/lune-ui-core/tests/widget_probe_chip.rs, apps/gallery/src/pages/probe_chip.rs"));
        assert!(out.contains("- `probe-chip-default`: "));
        assert!(!out.contains("<slug>-<what>"));
        assert!(
            out.contains("- Milestone: <") && out.contains("- Depends on: <"),
            "the Build placeholders stay"
        );
        assert!(out.ends_with('\n') == TEMPLATE.ends_with('\n'));
    }

    #[test]
    fn a_template_that_lost_a_line_stops_the_fill() {
        let n = names("widget", "probe-chip");
        let cut = TEMPLATE.replace("area: <area>\n", "area: x\n");
        assert!(
            fill_template(&cut, &n)
                .expect_err("changed")
                .contains("area: <area>")
        );
        let twice = TEMPLATE.replace("owner: —\n", "owner: —\nowner: —\n");
        assert!(fill_template(&twice, &n).is_err());
    }

    #[test]
    fn the_stubs_hold_what_the_checklist_asks_of_a_new_widget() {
        let n = names("widget", "probe-chip");
        let w = widget_source(&n);
        assert!(w.contains("#[non_exhaustive]") && w.contains("#[derive(Debug, Clone, Default)]"));
        assert!(w.contains("pub struct ProbeChipOptions<'a>") && w.contains("pub fn probe_chip("));
        assert!(
            w.contains("self.theme()") && w.contains("self.node("),
            "theme colour and a node"
        );
        assert!(!w.contains("unwrap"));
        assert!(test_source(&n).contains("fn the_probe_chip_registers_its_node()"));
        let p = page_source(&n);
        assert!(p.contains("id: \"probe-chip\"") && p.contains("id: \"probe-chip-default\""));
        assert!(p.contains("stage.area(ui)") && p.contains("scale: 2.0"));
        // Every sidebar page needs a knob (showcase::knobs test): the stub has one.
        assert!(p.contains("fn knobs(&mut self") && p.contains("panel.toggle("));
    }

    // -- the checklist ---------------------------------------------------------------------

    const TABLE: &str = "<!-- checklist:begin -->\n| rule | what | D-rows | enforced by | kind |\n| --- | --- | --- | --- | --- |\n| a | does a | D1 | `x.rs::t` | test |\n| b | does b | D2 | by review | review |\n| c | does c | D3 | by review | auto:test-cited |\n| m | does m | pending | by review | [mobile slot] |\n<!-- checklist:end -->\n";

    #[test]
    fn the_checklist_reader_keeps_the_rows_left_to_a_person() {
        let rules = read_by_hand_rules(TABLE).expect("reads");
        let got: Vec<(&str, &str)> = rules
            .iter()
            .map(|r| (r.id.as_str(), r.kind.as_str()))
            .collect();
        assert_eq!(
            got,
            [("a", "test"), ("b", "review"), ("m", "[mobile slot]")]
        );
        assert!(read_by_hand_rules("no markers").is_err());
        assert!(read_by_hand_rules(&TABLE.replace("| test |", "")).is_err());
        let only_auto =
            "<!-- checklist:begin -->\n| c | does c | D3 | x | auto:y |\n<!-- checklist:end -->";
        assert!(read_by_hand_rules(only_auto).is_err());
    }

    #[test]
    fn a_new_rule_row_shows_up_in_the_next_steps_with_no_change_to_the_tool() {
        let n = names("widget", "probe-chip");
        let with_new = TABLE.replace(
            "<!-- checklist:end -->",
            "| z-new | does z | D9 | by review | review |\n<!-- checklist:end -->",
        );
        let text = next_steps(&n, &read_by_hand_rules(&with_new).expect("reads")).join("\n");
        assert!(text.contains("z-new: does z"), "{text}");
        let order: Vec<usize> = [
            "scripts/gen-component-index.sh",
            "cargo test -p gallery showcase",
            "--only probe-chip-default",
            "scripts/gen-api-docs.sh",
            "scripts/check-component.sh widget.probe-chip",
            "by hand, review",
            "by hand, test",
            "by hand, touch and phone [mobile slot]",
        ]
        .iter()
        .map(|needle| {
            text.find(needle)
                .unwrap_or_else(|| panic!("{needle}\n{text}"))
        })
        .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}\n{text}");
    }

    // -- the plan on the real files --------------------------------------------------------

    #[test]
    fn the_plan_applies_to_the_real_files_and_changes_one_line_in_each() {
        let root = real_root();
        let a = Args {
            root: root.clone(),
            area: "widget".to_owned(),
            slug: "scaffold-probe".to_owned(),
            title: None,
            owner: "scaffold".to_owned(),
            dry_run: true,
        };
        let p = plan(&root, &a).expect("the real anchors are all found once");
        assert_eq!(p.files.len(), 4);
        assert_eq!(p.edits.len(), 3);
        for (rel, new, _) in &p.edits {
            let old = std::fs::read_to_string(root.join(rel)).expect("reads");
            assert_ne!(&old, new, "{rel} changed");
            let added = new.lines().count() - old.lines().count();
            assert!((1..=2).contains(&added), "{rel}: {added} lines added");
        }
        // The group the area names is there; `Widgets` ends with the new page.
        let nav = &p.edits[2].1;
        assert!(
            nav.contains("\"scaffold-probe\",\n        ],"),
            "last in its group"
        );
        assert!(
            !root
                .join("crates/lune-ui-core/src/widgets/scaffold_probe.rs")
                .exists(),
            "a plan writes nothing"
        );
    }

    #[test]
    fn a_taken_name_is_refused_with_nothing_written() {
        let root = real_root();
        let base = Args {
            root: root.clone(),
            area: "widget".to_owned(),
            slug: "button".to_owned(),
            title: None,
            owner: "scaffold".to_owned(),
            dry_run: true,
        };
        let e = plan(&root, &base).expect_err("button exists");
        assert!(e.contains("exists"), "{e}");
        // A page id of its own, with no module of that name: the id scan finds it.
        let by_id = Args {
            slug: "status-dot".to_owned(),
            ..base.clone()
        };
        assert!(plan(&root, &by_id).is_err());
        let scene = Args {
            slug: "status-dot-tones".to_owned(),
            ..base
        };
        assert!(
            plan(&root, &scene).is_err(),
            "status-dot-tones is a scene id"
        );
    }

    /// A temp tree with the files the plan reads, copied from the repo.
    fn copy_tree(into: &Path) {
        let root = real_root();
        let mut files = vec![
            "docs/components/_template.md".to_owned(),
            "docs/components/README.md".to_owned(),
            "crates/lune-ui-core/src/widgets/mod.rs".to_owned(),
            "apps/gallery/src/pages/mod.rs".to_owned(),
            "apps/gallery/src/showcase/nav.rs".to_owned(),
        ];
        files.extend(
            ["widget", "overlay", "layout", "data"].map(|a| format!("docs/components/{a}/.keep")),
        );
        for rel in files {
            let to = into.join(&rel);
            std::fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
            let from = root.join(&rel);
            if from.exists() {
                std::fs::copy(from, to).expect("copy");
            } else {
                std::fs::write(to, "").expect("keep");
            }
        }
        std::fs::create_dir_all(into.join("crates/lune-ui-core/tests")).expect("mkdir");
    }

    #[test]
    fn a_run_writes_the_files_and_a_second_run_is_refused_and_changes_nothing() {
        let dir = std::env::temp_dir().join(format!("new-component-test-{}", std::process::id()));
        assert!(!dir.exists(), "the scratch dir is new");
        copy_tree(&dir);
        let a = Args {
            root: dir.clone(),
            area: "layout".to_owned(),
            slug: "probe-chip".to_owned(),
            title: Some("Probe chip".to_owned()),
            owner: "me".to_owned(),
            dry_run: false,
        };
        let p = plan(&dir, &a).expect("plans");
        apply(&dir, &p).expect("writes");
        for (rel, text) in &p.files {
            assert_eq!(
                &std::fs::read_to_string(dir.join(rel)).expect("written"),
                text
            );
        }
        let nav =
            std::fs::read_to_string(dir.join("apps/gallery/src/showcase/nav.rs")).expect("nav");
        assert!(nav.contains("\"probe-chip\","));
        let snapshot = |rels: &[&str]| -> Vec<String> {
            rels.iter()
                .map(|r| std::fs::read_to_string(dir.join(r)).expect("read"))
                .collect()
        };
        let rels = [
            "docs/components/layout/probe-chip.md",
            "crates/lune-ui-core/src/widgets/mod.rs",
            "apps/gallery/src/pages/mod.rs",
            "apps/gallery/src/showcase/nav.rs",
        ];
        let before = snapshot(&rels);
        assert!(plan(&dir, &a).expect_err("second run").contains("exists"));
        assert_eq!(snapshot(&rels), before);
        std::fs::remove_dir_all(&dir).expect("cleanup of the dir this test made");
    }
}
