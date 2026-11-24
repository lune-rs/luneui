//! The catalogue check (M11, D236): the gallery and `docs/components` agree, and a `done` entry
//! carries the evidence its status claims. It reads the entries from `docs/components` under the
//! workspace root and the scenes from `PAGES`; nothing here draws.
//!
//! Each rule is a function over an [`Entry`] (or all of them) that returns its complaints, so a
//! test can hand it a small wrong entry and see it fire. The real tests run the rules over the
//! real catalogue.
//!
//! - every hyphenated backticked id under an entry's `### Gallery scenes`, and the first id of
//!   each bullet, is a scene or a page of the gallery; every scene is claimed;
//! - a non-external entry names a scene or says `n/a —`;
//! - the `in-progress` entries are exactly the two Class B ones (D234), each quoting its line;
//! - a `done` entry has no owner, a `source:` that exists, no open "still needs", and a
//!   `- Done (D47, M11):` line in `### Build` whose cited tests exist, unless it says
//!   `tests: n/a — <reason>`;
//! - every requirement id of `docs/requirements.md` is cited by an entry, bar the ones nothing
//!   in this repo can satisfy;
//! - every motion kind and easing is named where `motion-kinds` and `motion-easings` are claimed.
//!
//! The same rules, plus the checklist-table checks of `docs/components/README.md`, run as the
//! component check: `gallery --check-components [<id>...]` (see [`run_cli`]) and, for every `done`
//! entry, the gate test `every_done_component_passes_the_component_check`. The check reads the
//! source tree the gallery was built from (`CARGO_MANIFEST_DIR`).
//!
//! A claim's `@Nx` suffix is dropped: the check reads scene ids, not scales, so an entry claiming
//! `ui-controls@3x` passes while only `ui-controls` exists.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::page::PageKind;
use crate::pages::PAGES;

/// Requirement ids no entry cites, with the reason, and the only ones allowed to go uncited.
#[cfg(test)]
const UNCITED: &[(&str, &str)] = &[
    ("R-ext-3", "an extraction rule, held by the pixel gates"),
    ("R-ext-5", "an extraction rule, held by the pixel gates"),
    ("R-nfr-2", "a non-functional rule about the whole repo"),
    ("R-nfr-4", "a non-functional rule about the whole repo"),
    ("R-nfr-7", "a non-functional rule about the whole repo"),
];

/// Widget modules no catalogue entry claims in its `source:`: routines shared by several widgets
/// (or by one entry's body), each with its reason. A module here that an entry also claims, or
/// that is not in `widgets/`, fails `every_widget_module_is_claimed_by_an_entry`.
#[cfg(test)]
const SHARED_WIDGET_HELPERS: &[(&str, &str)] = &[
    (
        "gesture_track.rs",
        "the motion core of the mobile components: a track that follows a finger and settles, and the pan region (D369)",
    ),
    (
        "input_box.rs",
        "the private chrome shared by the combo box, the stepper and the field",
    ),
    (
        "selectable.rs",
        "the key-value list's selectable value text, named in data.kv-list",
    ),
    (
        "selection.rs",
        "the one selection drawing every text view uses (R18)",
    ),
    ("shine.rs", "the tab strip's moving band (R3)"),
    (
        "text_lines.rs",
        "the shared plumbing of the text views (D6)",
    ),
];

/// One catalogue file, split.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) path: String,
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) owner: String,
    pub(crate) source: String,
    pub(crate) body: String,
}

impl Entry {
    /// Splits `text` at its front matter. `path` is only for messages.
    pub(crate) fn parse(path: &str, text: &str) -> Result<Entry, String> {
        let mut parts = text.splitn(3, "---\n");
        let (Some(""), Some(front), Some(body)) = (parts.next(), parts.next(), parts.next()) else {
            return Err(format!("{path}: no front matter"));
        };
        let key = |k: &str| {
            front
                .lines()
                .find_map(|l| l.strip_prefix(k).and_then(|r| r.strip_prefix(": ")))
                .map(|v| v.trim().to_owned())
                .ok_or_else(|| format!("{path}: no `{k}`"))
        };
        Ok(Entry {
            path: path.to_owned(),
            id: key("id")?,
            status: key("status")?,
            owner: key("owner")?,
            source: key("source")?,
            body: body.to_owned(),
        })
    }

    /// The text of `### <name>` up to the next heading.
    pub(crate) fn section(&self, name: &str) -> Option<&str> {
        let head = format!("### {name}\n");
        let at = self.body.find(&head)? + head.len();
        let rest = &self.body[at..];
        let end = rest
            .find("\n### ")
            .into_iter()
            .chain(rest.find("\n## "))
            .min()
            .unwrap_or(rest.len());
        Some(rest[..end].trim())
    }
}

/// Every backticked span of `text`.
fn spans(text: &str) -> Vec<&str> {
    text.split('`').skip(1).step_by(2).collect()
}

/// `id` or `id@2x`, lowercase letters, digits and single hyphens: the shape of a scene id.
fn scene_shaped(span: &str) -> Option<&str> {
    let id = span.split('@').next().unwrap_or(span);
    let rest = span.get(id.len()..).unwrap_or("");
    let suffix_ok = rest.is_empty()
        || (rest.starts_with('@')
            && rest.ends_with('x')
            && rest[1..rest.len() - 1]
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.'));
    let words_ok = !id.is_empty()
        && id.split('-').all(|w| {
            !w.is_empty()
                && w.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        });
    (suffix_ok && words_ok).then_some(id)
}

/// What a `### Gallery scenes` section claims: every hyphenated scene-shaped span, and the first
/// span of each bullet (a one-word scene id, `controls`, can only be told by its place).
pub(crate) fn claims(section: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for s in spans(section) {
        if let Some(id) = scene_shaped(s)
            && id.contains('-')
        {
            out.insert(id.to_owned());
        }
    }
    for line in section.lines() {
        if let Some(rest) = line.strip_prefix("- `")
            && let Some(first) = rest.split('`').next()
            && let Some(id) = scene_shaped(first)
        {
            out.insert(id.to_owned());
        }
    }
    out
}

/// What the gallery has: every scene id and every page id.
#[derive(Debug, Default)]
pub(crate) struct Known {
    pub(crate) scenes: BTreeSet<String>,
    pub(crate) pages: BTreeSet<String>,
}

impl Known {
    pub(crate) fn of_gallery() -> Known {
        let mut k = Known::default();
        for page in PAGES {
            k.pages.insert(page.id.to_owned());
            match page.kind {
                PageKind::Legacy(scenes) => {
                    k.scenes.extend(scenes.iter().map(|s| s.id.to_owned()));
                }
                PageKind::Ui(ui) => k.scenes.extend(ui.scenes.iter().map(|s| s.id.to_owned())),
            }
        }
        k
    }

    fn has(&self, id: &str) -> bool {
        self.scenes.contains(id) || self.pages.contains(id)
    }
}

/// The workspace root.
pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The sorted paths under `dir` that pass `keep`.
fn sorted_paths(dir: &Path, keep: impl Fn(&Path) -> bool) -> Result<Vec<PathBuf>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for item in read {
        let path = item.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if keep(&path) {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Every entry under `docs/components`, in path order; an unreadable or malformed file is an
/// error naming it.
pub(crate) fn load_all(root: &Path) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for dir in sorted_paths(&root.join("docs/components"), |p| p.is_dir())? {
        for f in sorted_paths(&dir, |p| p.extension().is_some_and(|x| x == "md"))? {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            // `/` separators on every host: the rules match names by `/` suffix.
            let name = f
                .strip_prefix(root)
                .unwrap_or(&f)
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push(Entry::parse(&name, &text)?);
        }
    }
    Ok(out)
}

/// [`load_all`] for the tests, which unwrap.
#[cfg(test)]
pub(crate) fn load(root: &Path) -> Vec<Entry> {
    load_all(root).unwrap_or_else(|e| panic!("{e}"))
}

// ---- rules ----------------------------------------------------------------------------------

/// Scenes an entry claims that the gallery does not have.
pub(crate) fn unknown_scenes(e: &Entry, known: &Known) -> Vec<String> {
    let Some(sec) = e.section("Gallery scenes") else {
        return Vec::new();
    };
    claims(sec)
        .into_iter()
        .filter(|id| !known.has(id))
        .map(|id| format!("{}: claims `{id}`, which the gallery does not have", e.path))
        .collect()
}

/// A non-external entry names a scene (or a page) or says `n/a —`.
#[cfg(test)]
pub(crate) fn has_a_scene_or_na(e: &Entry, known: &Known) -> Vec<String> {
    if e.status == "external" {
        return Vec::new();
    }
    let Some(sec) = e.section("Gallery scenes") else {
        return vec![format!("{}: no `### Gallery scenes`", e.path)];
    };
    let names_one = claims(sec).iter().any(|id| known.has(id))
        || spans(sec)
            .into_iter()
            .filter_map(scene_shaped)
            .any(|id| known.has(id));
    if names_one || sec.starts_with("n/a —") {
        Vec::new()
    } else {
        vec![format!(
            "{}: Gallery scenes names no scene and is not `n/a —`",
            e.path
        )]
    }
}

/// Scenes and pages of the gallery no entry claims.
#[cfg(test)]
pub(crate) fn unclaimed(entries: &[Entry], known: &Known) -> Vec<String> {
    let mut claimed = BTreeSet::new();
    for e in entries {
        if let Some(sec) = e.section("Gallery scenes") {
            claimed.extend(claims(sec));
            claimed.extend(
                spans(sec)
                    .into_iter()
                    .filter_map(scene_shaped)
                    .map(str::to_owned),
            );
        }
    }
    known
        .scenes
        .iter()
        .filter(|id| !claimed.contains(*id))
        .map(|id| format!("scene `{id}` is claimed by no entry"))
        .collect()
}

/// The `<path>.rs::<fn>` citations in `line`.
fn cited_tests(line: &str) -> Vec<(String, String)> {
    spans(line)
        .into_iter()
        .flat_map(|s| s.split(", ").map(str::trim).collect::<Vec<_>>())
        .filter_map(|s| {
            let (file, name) = s.split_once(".rs::")?;
            Some((format!("{file}.rs"), name.to_owned()))
        })
        .collect()
}

/// What is wrong with one cited test, if anything: its file is absent or has no such function.
fn cited_test_problem(root: &Path, file: &str, name: &str) -> Option<String> {
    match std::fs::read_to_string(root.join(file)) {
        Err(_) => Some(format!("cited test file `{file}` does not exist")),
        Ok(text) => (!text.contains(&format!("fn {name}("))
            && !text.contains(&format!("fn {name}<")))
        .then(|| format!("`{file}` has no `fn {name}`")),
    }
}

/// The `- Done (D47, M11):` line of an entry's Build section.
fn done_line(e: &Entry) -> Option<&str> {
    e.section("Build")?
        .lines()
        .find(|l| l.starts_with("- Done (D47, M11):"))
}

/// What a `done` entry must hold; nothing for any other status.
pub(crate) fn done_complaints(e: &Entry, root: &Path) -> Vec<String> {
    if e.status != "done" {
        return Vec::new();
    }
    let mut out = Vec::new();
    let p = &e.path;
    if e.owner != "—" {
        out.push(format!("{p}: done, but owner is `{}`", e.owner));
    }
    let paths: Vec<&str> = e
        .source
        .split(',')
        .map(|s| s.trim().trim_matches('`'))
        .filter(|s| !s.is_empty())
        .collect();
    if paths.is_empty() || paths.contains(&"—") {
        out.push(format!("{p}: done, but source names no path"));
    }
    for s in paths.iter().filter(|s| **s != "—") {
        if !root.join(s).exists() {
            out.push(format!("{p}: source `{s}` does not exist"));
        }
    }
    for needle in ["still needs", "Open for `done`"] {
        if e.body.contains(needle) {
            out.push(format!("{p}: done, but the entry says \"{needle}\""));
        }
    }
    if e.section("Build").is_none() {
        out.push(format!("{p}: done, but no `### Build`"));
        return out;
    }
    let Some(line) = done_line(e) else {
        out.push(format!(
            "{p}: done, but Build has no `- Done (D47, M11):` line"
        ));
        return out;
    };
    // The page and unverified parts must be there; their content is the entry's to word (real
    // lines say `n/a (...)`, `progress-*`).
    for part in ["; page ", "; unverified here:"] {
        if !line.contains(part) {
            out.push(format!(
                "{p}: the Done line has no `{}` part",
                part.trim_start_matches("; ")
            ));
        }
    }
    if line.contains("tests: n/a —") {
        return out;
    }
    let tests = cited_tests(line);
    if tests.is_empty() {
        out.push(format!("{p}: the Done line cites no `<path>.rs::<fn>`"));
    }
    for (file, name) in tests {
        out.extend(cited_test_problem(root, &file, &name).map(|why| format!("{p}: {why}")));
    }
    out
}

/// The entries that stay `in-progress` by their own text (Class B, D234): path suffix, who runs
/// what the entry names, and the line of the entry that ties its done standard to that run. An
/// audit may add one only by quoting such a line; it may not remove one by rewording it.
pub(crate) const CLASS_B: [(&str, &str, &str); 2] = [
    (
        "platform/material.md",
        "Luneterm M9 to M11 runs",
        "The done standard (D18) for this entry needs those runs",
    ),
    (
        "render/surface.md",
        "consumer's window run",
        "the pixel gate on a presented frame (open: the platform run is not in this milestone)",
    ),
];

/// What is wrong with one `in-progress` entry as a Class B entry; nothing for any other status.
fn class_b_entry_complaints(e: &Entry) -> Vec<String> {
    let mut out = Vec::new();
    if e.status != "in-progress" {
        return out;
    }
    let p = &e.path;
    let Some((_, owner, quote)) = CLASS_B.iter().find(|(suffix, _, _)| p.ends_with(suffix)) else {
        out.push(format!("{p}: in-progress, but it is not a Class B entry"));
        return out;
    };
    if e.owner != *owner {
        out.push(format!("{p}: owner is `{}`, not `{owner}`", e.owner));
    }
    if !e.body.contains(quote) {
        out.push(format!("{p}: no longer quotes \"{quote}\""));
    }
    out
}

/// The `in-progress` entries that are not Class B, and a Class B entry that is not `in-progress`,
/// has another owner, or no longer quotes its line.
#[cfg(test)]
pub(crate) fn class_b_complaints(entries: &[Entry]) -> Vec<String> {
    let mut out: Vec<String> = entries.iter().flat_map(class_b_entry_complaints).collect();
    for (suffix, _, _) in CLASS_B {
        if !entries
            .iter()
            .any(|e| e.path.ends_with(suffix) && e.status == "in-progress")
        {
            out.push(format!("{suffix}: Class B, but not an in-progress entry"));
        }
    }
    out
}

/// `R-<area>-<n>` ids in `text`.
#[cfg(test)]
pub(crate) fn requirement_ids(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while let Some(at) = text[i..].find("R-") {
        let start = i + at;
        i = start + 2;
        if start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        let mut j = start + 2;
        while j < bytes.len() && bytes[j].is_ascii_lowercase() {
            j += 1;
        }
        if j == start + 2 || bytes.get(j) != Some(&b'-') {
            continue;
        }
        let digits = j + 1;
        let mut k = digits;
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k > digits {
            out.insert(text[start..k].to_owned());
        }
    }
    out
}

/// Requirement ids of `requirements` that no entry cites and that are not in [`UNCITED`].
#[cfg(test)]
pub(crate) fn uncited_requirements(entries: &[Entry], requirements: &str) -> Vec<String> {
    let mut cited = BTreeSet::new();
    for e in entries {
        cited.extend(requirement_ids(&e.body));
    }
    requirement_ids(requirements)
        .into_iter()
        .filter(|id| !cited.contains(id) && !UNCITED.iter().any(|(u, _)| u == id))
        .map(|id| format!("{id} is cited by no entry"))
        .collect()
}

/// The words of `names` that the bullet claiming `scene` in the entries does not hold.
#[cfg(test)]
pub(crate) fn missing_from_scene(entries: &[Entry], scene: &str, names: &[&str]) -> Vec<String> {
    let needle = format!("- `{scene}`");
    let bullet = entries
        .iter()
        .filter_map(|e| e.section("Gallery scenes"))
        .flat_map(str::lines)
        .find(|l| l.starts_with(&needle));
    let Some(bullet) = bullet else {
        return vec![format!(
            "no entry's Gallery scenes holds a bullet for `{scene}`"
        )];
    };
    names
        .iter()
        .filter(|n| !bullet.contains(*n))
        .map(|n| format!("`{scene}` does not name `{n}`"))
        .collect()
}

/// Widget files (`name.rs` under `widgets/`, `mod.rs` aside) that no entry's `source:` names and
/// no helper covers, plus every helper that is claimed after all or is not a widget file.
#[cfg(test)]
pub(crate) fn widget_module_complaints(
    files: &[String],
    entries: &[Entry],
    helpers: &[(&str, &str)],
) -> Vec<String> {
    const DIR: &str = "crates/lune-ui-core/src/widgets/";
    let claimed: BTreeSet<&str> = entries
        .iter()
        .flat_map(|e| e.source.split(','))
        .map(|s| s.trim().trim_matches('`'))
        .collect();
    let mut out = Vec::new();
    for f in files.iter().filter(|f| *f != "mod.rs") {
        let listed = helpers.iter().any(|(h, _)| h == f);
        let claimed = claimed.contains(format!("{DIR}{f}").as_str());
        if !listed && !claimed {
            out.push(format!(
                "{DIR}{f}: in no entry's `source:` and not in SHARED_WIDGET_HELPERS"
            ));
        }
        if listed && claimed {
            out.push(format!(
                "{f}: in SHARED_WIDGET_HELPERS but an entry claims it"
            ));
        }
    }
    for (h, why) in helpers {
        if !files.iter().any(|f| f == h) {
            out.push(format!(
                "{h}: in SHARED_WIDGET_HELPERS but not a widget file"
            ));
        }
        if why.trim().is_empty() {
            out.push(format!("{h}: in SHARED_WIDGET_HELPERS with no reason"));
        }
    }
    out
}

// ---- the component check --------------------------------------------------------------------

/// The areas whose entries are drawn by `Ui` and so own a gallery scene (R1-D6).
const SCENE_AREAS: [&str; 4] = ["widget", "overlay", "layout", "data"];

/// How a checklist row is held (the `kind` column of `docs/components/README.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuleKind {
    /// A check function of this id holds it.
    Auto(String),
    /// A test that runs in the gate.
    Test,
    /// A reviewer reading the diff.
    Review,
    /// A rule the mobile program fills in; nothing enforces it yet.
    MobileSlot,
}

/// One row of the checklist table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    pub(crate) id: String,
    pub(crate) what: String,
    pub(crate) enforced_by: String,
    pub(crate) kind: RuleKind,
}

/// What the checks read besides the entry: the gallery's scenes, the decision ids of the
/// generated index and the workspace root.
#[derive(Debug)]
pub(crate) struct Ctx {
    pub(crate) known: Known,
    pub(crate) decisions: BTreeSet<u32>,
    pub(crate) root: PathBuf,
}

/// One check: its id (the `auto:` ids of the table) and its function.
type Check = (&'static str, fn(&Entry, &Ctx) -> Vec<String>);

/// The checks the table's `auto:` rows name, in the order they run.
const CHECKS: [Check; 4] = [
    ("catalogue-file", check_catalogue_file),
    ("gallery-scene", check_gallery_scene),
    ("decision-row", check_decision_row),
    ("test-cited", check_test_cited),
];

/// Existing entries that fail a check, listed so the gate holds the rest: id, check id, reason.
/// The list only shrinks: `every_done_component_passes_the_component_check` fails when it differs
/// from the measured failures in either direction.
pub(crate) const KNOWN_GAPS: &[(&str, &str, &str)] = &[];

/// A failed check of one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) id: String,
    pub(crate) check: String,
    pub(crate) reason: String,
}

impl Failure {
    fn line(&self) -> String {
        format!("{}: {}: {}", self.id, self.check, self.reason)
    }
}

/// `<area>` and `<slug>` of an entry's path (`docs/components/<area>/<slug>.md`).
fn area_and_slug(path: &str) -> Option<(&str, &str)> {
    let mut parts = path.rsplit('/');
    let slug = parts.next()?.strip_suffix(".md")?;
    Some((parts.next()?, slug))
}

/// The entry's id is `<area>.<slug>` of its path.
fn check_catalogue_file(e: &Entry, _: &Ctx) -> Vec<String> {
    match area_and_slug(&e.path) {
        Some((area, slug)) if e.id == format!("{area}.{slug}") => Vec::new(),
        Some((area, slug)) => vec![format!(
            "id `{}` is not `{area}.{slug}`, the id of its path",
            e.id
        )],
        None => vec![format!("path `{}` is not <area>/<slug>.md", e.path)],
    }
}

/// No entry claims a scene the gallery lacks, and an entry of a `Ui` area claims at least one it
/// has; n/a does not do.
fn check_gallery_scene(e: &Entry, ctx: &Ctx) -> Vec<String> {
    let mut out = reasons(e, unknown_scenes(e, &ctx.known));
    let in_scene_area = area_and_slug(&e.path).is_some_and(|(a, _)| SCENE_AREAS.contains(&a));
    if !in_scene_area {
        return out;
    }
    let claimed = e
        .section("Gallery scenes")
        .map(claims)
        .unwrap_or_default()
        .into_iter()
        .any(|id| ctx.known.scenes.contains(&id));
    if !claimed {
        out.push("`### Gallery scenes` claims no scene the gallery has".to_owned());
    }
    out
}

/// The `[D<n>]` cites in `text`, as numbers.
fn decision_cites(text: &str) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("[D") {
        rest = &rest[at + 2..];
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0
            && rest[digits..].starts_with(']')
            && let Ok(n) = rest[..digits].parse()
        {
            out.insert(n);
        }
    }
    out
}

/// An entry of a `Ui` area cites at least one `[D<n>]` and each cited row is in the generated
/// index. The other areas (core, render, text, infra, platform) hold machinery, not components a
/// new widget joins, and cite their rows in prose.
fn check_decision_row(e: &Entry, ctx: &Ctx) -> Vec<String> {
    let in_scene_area = area_and_slug(&e.path).is_some_and(|(a, _)| SCENE_AREAS.contains(&a));
    if !in_scene_area {
        return Vec::new();
    }
    let cites = decision_cites(&e.body);
    if cites.is_empty() {
        return vec!["cites no decision row as `[D<n>]`".to_owned()];
    }
    cites
        .iter()
        .filter(|n| !ctx.decisions.contains(n))
        .map(|n| {
            format!("cites [D{n}], which is no row of docs/DECISIONS-INDEX.md (run scripts/gen-decisions-index.sh)")
        })
        .collect()
}

/// The Build section's Done line cites at least one existing test (or says `tests: n/a —`).
fn check_test_cited(e: &Entry, ctx: &Ctx) -> Vec<String> {
    let Some(line) = done_line(e) else {
        return vec!["Build has no `- Done (D47, M11):` line".to_owned()];
    };
    if line.contains("tests: n/a —") {
        return Vec::new();
    }
    let tests = cited_tests(line);
    if tests.is_empty() {
        return vec!["the Done line cites no `<path>.rs::<fn>`".to_owned()];
    }
    if tests
        .iter()
        .any(|(file, name)| cited_test_problem(&ctx.root, file, name).is_none())
    {
        Vec::new()
    } else {
        vec!["the Done line cites no test that exists".to_owned()]
    }
}

/// `complaints` without the `<path>: ` the older rules put in front of each.
fn reasons(e: &Entry, complaints: Vec<String>) -> Vec<String> {
    let prefix = format!("{}: ", e.path);
    complaints
        .into_iter()
        .map(|c| c.strip_prefix(&prefix).map_or(c.clone(), str::to_owned))
        .collect()
}

/// Every check on `e`, whatever its status.
pub(crate) fn check_entry(e: &Entry, ctx: &Ctx) -> Vec<Failure> {
    let mut out = Vec::new();
    let mut add = |check: &str, reasons: Vec<String>| {
        out.extend(reasons.into_iter().map(|reason| Failure {
            id: e.id.clone(),
            check: check.to_owned(),
            reason,
        }));
    };
    for (id, check) in CHECKS {
        add(id, check(e, ctx));
    }
    add("done-evidence", reasons(e, done_complaints(e, &ctx.root)));
    add("class-b", reasons(e, class_b_entry_complaints(e)));
    out
}

/// The text between the checklist markers, as rules; an error names what is wrong with the table.
pub(crate) fn read_checklist(readme: &str) -> Result<Vec<Rule>, String> {
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
        .ok_or_else(|| format!("`{END}` comes before `{BEGIN}`"))?;
    let mut rules: Vec<Rule> = Vec::new();
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
        let [rule, what, _d_rows, enforced_by, kind] = cells[..] else {
            return Err(format!(
                "checklist row `{line}` has {} cells, not 5",
                cells.len()
            ));
        };
        if rules.iter().any(|r| r.id == rule) {
            return Err(format!("checklist rule `{rule}` appears twice"));
        }
        let kind = match kind {
            "test" => RuleKind::Test,
            "review" => RuleKind::Review,
            "[mobile slot]" => RuleKind::MobileSlot,
            other => match other.strip_prefix("auto:") {
                Some(id) if CHECKS.iter().any(|(c, _)| *c == id) => RuleKind::Auto(id.to_owned()),
                Some(id) => return Err(format!("checklist rule `{rule}`: no check `{id}`")),
                None => return Err(format!("checklist rule `{rule}`: unknown kind `{other}`")),
            },
        };
        rules.push(Rule {
            id: rule.to_owned(),
            what: what.to_owned(),
            enforced_by: enforced_by.to_owned(),
            kind,
        });
    }
    if rules.is_empty() {
        return Err("the checklist table holds no rule".to_owned());
    }
    Ok(rules)
}

/// The decision numbers of the generated index: every line that starts `| D<n> |`.
fn index_decisions(index: &str) -> BTreeSet<u32> {
    index
        .lines()
        .filter_map(|l| l.strip_prefix("| D")?.split(' ').next()?.parse().ok())
        .collect()
}

impl Ctx {
    /// What the checks read, from the workspace at `root`.
    pub(crate) fn load(root: &Path) -> Result<Ctx, String> {
        let path = root.join("docs/DECISIONS-INDEX.md");
        let index =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let decisions = index_decisions(&index);
        if decisions.is_empty() {
            return Err(format!("{} holds no decision row", path.display()));
        }
        Ok(Ctx {
            known: Known::of_gallery(),
            decisions,
            root: root.to_owned(),
        })
    }
}

/// What a run found, for the CLI to print and the gate test to compare.
#[derive(Debug, Default)]
pub(crate) struct Measured {
    /// Failures of `done` entries, in entry order.
    pub(crate) failures: Vec<Failure>,
    /// `done` entries checked.
    pub(crate) checked: usize,
    /// Entries that are neither `done` nor `external`.
    pub(crate) open: Vec<String>,
}

/// Every `done` entry through every check; the other entries are listed as open.
pub(crate) fn measure(entries: &[Entry], ctx: &Ctx) -> Measured {
    let mut m = Measured::default();
    for e in entries {
        if e.status == "done" {
            m.checked += 1;
            m.failures.extend(check_entry(e, ctx));
        } else if e.status != "external" {
            m.open.push(format!("{} ({})", e.id, e.status));
        }
    }
    m
}

/// The failures not in `gaps`, and the `gaps` that no longer fail.
fn against_known_gaps<'a>(
    m: &'a Measured,
    gaps: &[(&str, &str, &str)],
) -> (Vec<&'a Failure>, Vec<String>) {
    let known = |f: &Failure| gaps.iter().any(|(id, c, _)| *id == f.id && *c == f.check);
    let new = m.failures.iter().filter(|f| !known(f)).collect();
    let stale = gaps
        .iter()
        .filter(|(id, c, _)| !m.failures.iter().any(|f| f.id == *id && f.check == *c))
        .map(|(id, c, _)| format!("{id}: {c}: listed in KNOWN_GAPS but passes; drop it"))
        .collect();
    (new, stale)
}

/// The text of one run and its exit code: 0 all pass, 1 a failure.
#[derive(Debug)]
pub(crate) struct Outcome {
    pub(crate) lines: Vec<String>,
    pub(crate) code: u8,
}

/// The check of every `done` entry (the gate's path).
pub(crate) fn run_all(entries: &[Entry], ctx: &Ctx) -> Outcome {
    let m = measure(entries, ctx);
    let (new, stale) = against_known_gaps(&m, KNOWN_GAPS);
    let mut lines = Vec::new();
    for f in &m.failures {
        if let Some((_, _, why)) = KNOWN_GAPS
            .iter()
            .find(|(id, c, _)| *id == f.id && *c == f.check)
        {
            lines.push(format!("{} (known gap: {why})", f.line()));
        }
    }
    lines.extend(new.iter().map(|f| f.line()));
    lines.extend(stale.iter().cloned());
    for open in &m.open {
        lines.push(format!("open: {open}"));
    }
    let failed = new.len() + stale.len();
    lines.push(format!(
        "checked {} done entries: {failed} failing, {} known gaps, {} open",
        m.checked,
        m.failures.len() - new.len(),
        m.open.len()
    ));
    Outcome {
        lines,
        code: u8::from(failed > 0),
    }
}

/// The check of the named entries, whatever their status (what a new component runs): a status
/// that is not `done` is itself a failure, and the rules a person has to hold follow.
pub(crate) fn run_ids(
    ids: &[String],
    entries: &[Entry],
    ctx: &Ctx,
    rules: &[Rule],
) -> Result<Outcome, String> {
    let mut chosen = Vec::new();
    for id in ids {
        let Some(e) = entries.iter().find(|e| e.id == *id) else {
            return Err(format!(
                "no entry with id `{id}` (the id is `<area>.<slug>`, a file under docs/components)"
            ));
        };
        chosen.push(e);
    }
    let mut lines = Vec::new();
    let mut failing = 0;
    for e in &chosen {
        let mut mine: Vec<Failure> = Vec::new();
        if e.status != "done" {
            mine.push(Failure {
                id: e.id.clone(),
                check: "status".to_owned(),
                reason: format!("{}, not done", e.status),
            });
        }
        mine.extend(check_entry(e, ctx));
        failing += mine.len();
        lines.extend(mine.iter().map(Failure::line));
        let by_hand: Vec<String> = rules
            .iter()
            .filter(|r| !matches!(r.kind, RuleKind::Auto(_)))
            .map(|r| {
                let kind = match r.kind {
                    RuleKind::Test => "test",
                    RuleKind::Review => "review",
                    _ => "[mobile slot]",
                };
                format!("  {} ({kind}): {} [{}]", r.id, r.what, r.enforced_by)
            })
            .collect();
        lines.push(format!("{}: still to do by hand:", e.id));
        lines.extend(by_hand);
    }
    lines.push(format!(
        "checked {} entries: {failing} failing",
        chosen.len()
    ));
    Ok(Outcome {
        lines,
        code: u8::from(failing > 0),
    })
}

/// `gallery --check-components [<id>...]`: prints the result; exit 0 all pass, 1 a failure, 2 an
/// unknown id or a table or file the check cannot read.
pub(crate) fn run_cli(ids: &[String]) -> std::process::ExitCode {
    let root = root();
    let result = (|| {
        let readme_path = root.join("docs/components/README.md");
        let readme = std::fs::read_to_string(&readme_path)
            .map_err(|e| format!("{}: {e}", readme_path.display()))?;
        let rules = read_checklist(&readme)?;
        let entries = load_all(&root)?;
        let ctx = Ctx::load(&root)?;
        if ids.is_empty() {
            Ok(run_all(&entries, &ctx))
        } else {
            run_ids(ids, &entries, &ctx, &rules)
        }
    })();
    match result {
        Ok(outcome) => {
            for line in &outcome.lines {
                println!("{line}");
            }
            std::process::ExitCode::from(outcome.code)
        }
        Err(message) => {
            eprintln!("check-components: {message}");
            std::process::ExitCode::from(2)
        }
    }
}

// ---- the real catalogue ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use lune_ui::motion::Kind;

    use super::*;
    use crate::pages::motion::{EASINGS, easing_name, kind_name};

    fn entry(status: &str, owner: &str, source: &str, body: &str) -> Entry {
        let text = format!(
            "---\nid: x.y\nstatus: {status}\nowner: {owner}\nsource: {source}\n---\n{body}"
        );
        Entry::parse("x/y.md", &text).expect("parses")
    }

    fn known(scenes: &[&str]) -> Known {
        Known {
            scenes: scenes.iter().map(|s| (*s).to_owned()).collect(),
            pages: BTreeSet::new(),
        }
    }

    // -- the rules fire on a wrong entry --------------------------------------------------

    #[test]
    fn a_claimed_scene_the_gallery_lacks_is_reported() {
        let e = entry(
            "in-progress",
            "o",
            "—",
            "### Gallery scenes\n\n- `real-scene`: a\n- `no-such-scene`: b\n\n### Build\n",
        );
        let bad = unknown_scenes(&e, &known(&["real-scene"]));
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("no-such-scene"));
    }

    #[test]
    fn a_one_word_scene_id_at_a_bullets_start_is_checked() {
        let e = entry(
            "in-progress",
            "o",
            "—",
            "### Gallery scenes\n\n- `controls`: a\n",
        );
        assert_eq!(unknown_scenes(&e, &known(&["chrome"])).len(), 1);
        assert!(unknown_scenes(&e, &known(&["controls"])).is_empty());
    }

    #[test]
    fn a_scene_no_entry_claims_is_reported() {
        let e = entry(
            "in-progress",
            "o",
            "—",
            "### Gallery scenes\n\n- `a-b`: x\n",
        );
        let bad = unclaimed(&[e], &known(&["a-b", "c-d"]));
        assert_eq!(bad, ["scene `c-d` is claimed by no entry"]);
    }

    #[test]
    fn an_entry_with_neither_a_scene_nor_n_a_is_reported() {
        let none = entry(
            "in-progress",
            "o",
            "—",
            "### Gallery scenes\n\nSomething else.\n",
        );
        assert_eq!(has_a_scene_or_na(&none, &known(&["a-b"])).len(), 1);
        let na = entry(
            "in-progress",
            "o",
            "—",
            "### Gallery scenes\n\nn/a — no UI.\n",
        );
        assert!(has_a_scene_or_na(&na, &known(&["a-b"])).is_empty());
        let ext = entry(
            "external",
            "o",
            "—",
            "### Gallery scenes\n\nSomething else.\n",
        );
        assert!(has_a_scene_or_na(&ext, &known(&["a-b"])).is_empty());
    }

    const BUILD_OK: &str = "### Build\n\n- Milestone: M11\n- Done (D47, M11): tests `crates/lune-ui-core/tests/color.rs::premul_at_alpha_zero_is_all_zero`; page n/a; unverified here: none.\n";

    #[test]
    fn a_done_entry_that_holds_its_evidence_passes() {
        let e = entry("done", "—", "crates/lune-ui-core/src/color.rs", BUILD_OK);
        assert_eq!(done_complaints(&e, &root()), Vec::<String>::new());
    }

    #[test]
    fn a_done_entry_citing_an_absent_fn_is_reported() {
        let body = BUILD_OK.replace("premul_at_alpha_zero_is_all_zero", "no_such_fn");
        let e = entry("done", "—", "crates/lune-ui-core/src/color.rs", &body);
        let bad = done_complaints(&e, &root());
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("no `fn no_such_fn`"));
    }

    #[test]
    fn a_done_entry_citing_an_absent_file_is_reported() {
        let body = BUILD_OK.replace("tests/color.rs", "tests/nope.rs");
        let e = entry("done", "—", "crates/lune-ui-core/src/color.rs", &body);
        assert!(done_complaints(&e, &root())[0].contains("does not exist"));
    }

    #[test]
    fn a_done_entry_without_the_line_an_owner_or_a_source_is_reported() {
        let e = entry(
            "done",
            "o",
            "nope/none.rs",
            "### Build\n\n- Milestone: M11\n",
        );
        let bad = done_complaints(&e, &root());
        assert!(bad.iter().any(|b| b.contains("owner")), "{bad:?}");
        assert!(bad.iter().any(|b| b.contains("does not exist")), "{bad:?}");
        assert!(bad.iter().any(|b| b.contains("no `- Done")), "{bad:?}");
    }

    #[test]
    fn a_done_line_without_a_page_part_or_an_unverified_part_is_reported() {
        let no_page = BUILD_OK.replace("; page n/a", "");
        let e = entry("done", "—", "crates/lune-ui-core/src/color.rs", &no_page);
        let bad = done_complaints(&e, &root());
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("no `page ` part"), "{bad:?}");
        let no_unverified = BUILD_OK.replace("; unverified here: none", "");
        let e = entry(
            "done",
            "—",
            "crates/lune-ui-core/src/color.rs",
            &no_unverified,
        );
        let bad = done_complaints(&e, &root());
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("no `unverified here:` part"), "{bad:?}");
        // A docs-only line is held to the same two parts.
        let docs = "### Build\n\n- Done (D47, M11): tests: n/a — documentation only.\n";
        let e = entry("done", "—", "docs/architecture.md", docs);
        assert_eq!(done_complaints(&e, &root()).len(), 2);
    }

    #[test]
    fn a_done_entry_that_still_needs_something_is_reported() {
        let body = format!("- Done standard still needs: a test.\n\n{BUILD_OK}");
        let e = entry("done", "—", "crates/lune-ui-core/src/color.rs", &body);
        assert!(done_complaints(&e, &root())[0].contains("still needs"));
    }

    #[test]
    fn a_docs_only_done_entry_may_say_its_tests_are_n_a() {
        let body = "### Build\n\n- Done (D47, M11): tests: n/a — documentation only; page n/a; unverified here: none.\n";
        let e = entry("done", "—", "docs/architecture.md", body);
        assert_eq!(done_complaints(&e, &root()), Vec::<String>::new());
    }

    #[test]
    fn an_entry_that_is_not_done_is_not_held_to_the_done_rules() {
        let e = entry("in-progress", "o", "nope", "");
        assert!(done_complaints(&e, &root()).is_empty());
    }

    #[test]
    fn an_in_progress_entry_outside_class_b_is_reported() {
        let e = entry("in-progress", "o", "—", "");
        let bad = class_b_complaints(&[e]);
        assert!(
            bad.iter().any(|b| b.contains("not a Class B entry")),
            "{bad:?}"
        );
    }

    #[test]
    fn a_class_b_entry_that_lost_its_quote_its_owner_or_its_status_is_reported() {
        let make = |status: &str, owner: &str, body: &str| {
            let text = format!(
                "---\nid: platform.material\nstatus: {status}\nowner: {owner}\nsource: —\n---\n{body}"
            );
            Entry::parse("docs/components/platform/material.md", &text).expect("parses")
        };
        let surface = |status: &str| {
            let text = format!(
                "---\nid: render.surface\nstatus: {status}\nowner: consumer's window run\nsource: —\n---\n{}",
                CLASS_B[1].2
            );
            Entry::parse("docs/components/render/surface.md", &text).expect("parses")
        };
        let ok = make("in-progress", CLASS_B[0].1, CLASS_B[0].2);
        assert_eq!(
            class_b_complaints(&[ok, surface("in-progress")]),
            Vec::<String>::new()
        );
        let no_quote = make("in-progress", CLASS_B[0].1, "reworded");
        assert!(class_b_complaints(&[no_quote, surface("in-progress")])[0].contains("quotes"));
        let other_owner = make("in-progress", "someone", CLASS_B[0].2);
        assert!(class_b_complaints(&[other_owner, surface("in-progress")])[0].contains("owner"));
        let flipped = make("done", "—", CLASS_B[0].2);
        let bad = class_b_complaints(&[flipped, surface("in-progress")]);
        assert!(
            bad.iter().any(|b| b.contains("Class B, but not")),
            "{bad:?}"
        );
    }

    #[test]
    fn requirement_ids_are_read_and_an_uncited_one_is_reported() {
        let ids = requirement_ids("see R-core-4, (R-nfr-6) and R-<area>-<n>, not XR-core-9");
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), ["R-core-4", "R-nfr-6"]);
        let e = entry("in-progress", "o", "—", "Cites R-core-4.");
        let bad = uncited_requirements(&[e], "R-core-4 and R-core-5 and R-nfr-2");
        assert_eq!(bad, ["R-core-5 is cited by no entry"]);
    }

    #[test]
    fn a_motion_name_missing_from_its_scene_bullet_is_reported() {
        let e = entry(
            "in-progress",
            "o",
            "—",
            "### Gallery scenes\n\n- `motion-kinds`: hover and press\n",
        );
        let bad = missing_from_scene(
            std::slice::from_ref(&e),
            "motion-kinds",
            &["hover", "focus"],
        );
        assert_eq!(bad, ["`motion-kinds` does not name `focus`"]);
        assert_eq!(
            missing_from_scene(&[e], "motion-nothing", &["hover"]).len(),
            1
        );
    }

    // -- the real catalogue -----------------------------------------------------------------

    fn all() -> (Vec<Entry>, Known) {
        (load(&root()), Known::of_gallery())
    }

    fn none(complaints: Vec<String>) {
        assert_eq!(complaints, Vec::<String>::new());
    }

    #[test]
    fn the_catalogue_loads_at_least_108_entries_of_which_four_are_external() {
        // At least: a new component adds an entry (scripts/new-component.sh), and that must not
        // need an edit here. The floor still catches a directory the loader stopped reading.
        let (entries, _) = all();
        assert!(entries.len() >= 108, "{}", entries.len());
        assert_eq!(entries.iter().filter(|e| e.status == "external").count(), 4);
    }

    #[test]
    fn every_scene_an_entry_claims_is_in_the_gallery() {
        let (entries, known) = all();
        none(
            entries
                .iter()
                .flat_map(|e| unknown_scenes(e, &known))
                .collect(),
        );
        // The instrument: the gallery list is not empty and holds a known id.
        assert!(known.scenes.contains("controls") && known.pages.contains("window"));
    }

    #[test]
    fn every_scene_in_the_gallery_is_claimed_by_an_entry() {
        let (entries, known) = all();
        none(unclaimed(&entries, &known));
    }

    #[test]
    fn every_entry_names_a_scene_or_says_n_a() {
        let (entries, known) = all();
        none(
            entries
                .iter()
                .flat_map(|e| has_a_scene_or_na(e, &known))
                .collect(),
        );
    }

    #[test]
    fn every_done_entry_holds_its_evidence() {
        let (entries, _) = all();
        let root = root();
        none(
            entries
                .iter()
                .flat_map(|e| done_complaints(e, &root))
                .collect(),
        );
    }

    #[test]
    fn every_widget_module_is_claimed_by_an_entry() {
        let (entries, _) = all();
        let dir = root().join("crates/lune-ui-core/src/widgets");
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .expect("widgets dir")
            .map(|e| {
                e.expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|n| n.ends_with(".rs"))
            .collect();
        files.sort();
        // The instrument: the directory read found the widgets and a known one.
        assert!(
            files.len() > 50 && files.iter().any(|f| f == "menu.rs"),
            "{files:?}"
        );
        none(widget_module_complaints(
            &files,
            &entries,
            SHARED_WIDGET_HELPERS,
        ));
    }

    #[test]
    fn an_unclaimed_unlisted_widget_a_double_claim_and_a_stale_helper_are_reported() {
        let e = entry(
            "done",
            "—",
            "crates/lune-ui-core/src/widgets/button.rs, crates/lune-ui-core/src/widgets/both.rs",
            "",
        );
        let files: Vec<String> = ["mod.rs", "button.rs", "orphan.rs", "both.rs", "helper.rs"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let helpers = [
            ("helper.rs", "shared"),
            ("both.rs", "shared"),
            ("gone.rs", "x"),
        ];
        let bad = widget_module_complaints(&files, &[e], &helpers);
        assert_eq!(bad.len(), 3, "{bad:?}");
        assert!(bad.iter().any(|b| b.contains("orphan.rs")));
        assert!(
            bad.iter()
                .any(|b| b.contains("both.rs") && b.contains("claims"))
        );
        assert!(bad.iter().any(|b| b.contains("gone.rs")));
    }

    #[test]
    fn the_in_progress_entries_are_exactly_the_two_class_b_ones() {
        let (entries, _) = all();
        none(class_b_complaints(&entries));
        let open: Vec<&str> = entries
            .iter()
            .filter(|e| e.status == "in-progress")
            .map(|e| e.path.as_str())
            .collect();
        assert_eq!(open.len(), 2, "{open:?}");
    }

    #[test]
    fn every_requirement_is_cited_by_an_entry_or_listed() {
        let (entries, _) = all();
        let req =
            std::fs::read_to_string(root().join("docs/requirements.md")).expect("requirements");
        assert!(
            requirement_ids(&req).len() >= 60,
            "the requirements were read"
        );
        none(uncited_requirements(&entries, &req));
        // The list cannot go stale: each listed id is still in the requirements and still cited
        // by no entry (cite one, and it leaves the list).
        let ids = requirement_ids(&req);
        let cited: BTreeSet<String> = entries
            .iter()
            .flat_map(|e| requirement_ids(&e.body))
            .collect();
        for (id, why) in UNCITED {
            assert!(ids.contains(*id) && !why.is_empty(), "{id}");
            assert!(
                !cited.contains(*id),
                "{id} is cited now: drop it from UNCITED"
            );
        }
    }

    #[test]
    fn every_motion_kind_and_easing_is_named_where_its_scene_is_claimed() {
        let (entries, _) = all();
        let kinds: Vec<&str> = Kind::ALL.into_iter().map(kind_name).collect();
        assert_eq!(kinds.len(), 15);
        none(missing_from_scene(&entries, "motion-kinds", &kinds));
        let easings: Vec<&str> = EASINGS.into_iter().map(easing_name).collect();
        assert_eq!(easings.len(), 7);
        none(missing_from_scene(&entries, "motion-easings", &easings));
    }

    // -- the component check ---------------------------------------------------------------

    /// An entry at `docs/components/<area>/<slug>.md`.
    fn entry_at(area: &str, slug: &str, status: &str, body: &str) -> Entry {
        let text = format!(
            "---\nid: {area}.{slug}\nstatus: {status}\nowner: —\nsource: docs/design-recipes.md\n---\n{body}"
        );
        Entry::parse(&format!("docs/components/{area}/{slug}.md"), &text).expect("parses")
    }

    /// What the checks read, with a fixture gallery and index and the real root.
    fn ctx(scenes: &[&str], decisions: &[u32]) -> Ctx {
        Ctx {
            known: known(scenes),
            decisions: decisions.iter().copied().collect(),
            root: root(),
        }
    }

    const GOOD_BODY: &str = "Built ([D12]).\n\n### Gallery scenes\n\n- `real-scene`: a\n\n### Build\n\n- Done (D47, M11): tests `apps/gallery/src/catalogue.rs::every_done_entry_holds_its_evidence`; page `x`; unverified here: none.\n";

    #[test]
    fn an_entry_that_holds_every_check_passes_them() {
        let e = entry_at("widget", "thing", "done", GOOD_BODY);
        assert_eq!(check_entry(&e, &ctx(&["real-scene"], &[12])), []);
    }

    #[test]
    fn catalogue_file_fails_when_the_id_is_not_the_path() {
        let e = Entry::parse(
            "docs/components/widget/thing.md",
            "---\nid: widget.other\nstatus: planned\nowner: o\nsource: —\n---\n",
        )
        .expect("parses");
        let c = ctx(&[], &[]);
        assert_eq!(check_catalogue_file(&e, &c).len(), 1);
        let ok = entry_at("widget", "thing", "planned", "");
        assert!(check_catalogue_file(&ok, &c).is_empty());
    }

    #[test]
    fn gallery_scene_needs_a_scene_the_gallery_has_in_a_ui_area_and_n_a_does_not_do() {
        let c = ctx(&["real-scene"], &[]);
        let ok = entry_at("layout", "x", "done", GOOD_BODY);
        assert!(check_gallery_scene(&ok, &c).is_empty());
        let na = entry_at(
            "layout",
            "x",
            "done",
            "### Gallery scenes\n\nn/a — nothing\n",
        );
        assert_eq!(check_gallery_scene(&na, &c).len(), 1);
        let unknown = entry_at(
            "data",
            "x",
            "done",
            "### Gallery scenes\n\n- `real-scene`: a\n- `no-such-scene`: b\n",
        );
        assert_eq!(check_gallery_scene(&unknown, &c).len(), 1);
        // A machinery area owes no scene.
        let core = entry_at("core", "x", "done", "### Gallery scenes\n\nn/a — nothing\n");
        assert!(check_gallery_scene(&core, &c).is_empty());
    }

    #[test]
    fn decision_row_needs_a_cite_and_a_row_of_the_index_for_each() {
        let ok = entry_at("widget", "x", "done", GOOD_BODY);
        assert!(check_decision_row(&ok, &ctx(&[], &[12])).is_empty());
        // The same entry against an index that lacks D12, as when a row was added and the index
        // not regenerated.
        let stale = check_decision_row(&ok, &ctx(&[], &[13]));
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(stale[0].contains("D12"));
        let bare = entry_at("widget", "x", "done", "Built in M5 (D108).\n");
        assert_eq!(check_decision_row(&bare, &ctx(&[], &[108])).len(), 1);
        // A machinery area cites rows in prose.
        let render = entry_at("render", "x", "done", "Built in M5 (D108).\n");
        assert!(check_decision_row(&render, &ctx(&[], &[108])).is_empty());
    }

    #[test]
    fn a_cite_is_a_bracketed_d_and_digits() {
        let cites = decision_cites("see [D40], [D116] and (D5), [D], [Dx], [D7 and [D9]");
        assert_eq!(cites.into_iter().collect::<Vec<_>>(), [9, 40, 116]);
    }

    #[test]
    fn test_cited_needs_an_existing_test_or_an_n_a_reason() {
        let c = ctx(&[], &[]);
        let ok = entry_at("widget", "x", "done", GOOD_BODY);
        assert!(check_test_cited(&ok, &c).is_empty());
        let na = entry_at(
            "widget",
            "x",
            "done",
            "### Build\n\n- Done (D47, M11): tests: n/a — docs only; page n/a; unverified here: none.\n",
        );
        assert!(check_test_cited(&na, &c).is_empty());
        let absent = entry_at(
            "widget",
            "x",
            "done",
            "### Build\n\n- Done (D47, M11): tests `apps/gallery/src/catalogue.rs::no_such_test_anywhere`; page `x`; unverified here: none.\n",
        );
        assert_eq!(check_test_cited(&absent, &c).len(), 1);
        let none_cited = entry_at(
            "widget",
            "x",
            "done",
            "### Build\n\n- Done (D47, M11): tests nothing; page `x`; unverified here: none.\n",
        );
        assert_eq!(check_test_cited(&none_cited, &c).len(), 1);
        let no_line = entry_at("widget", "x", "done", "### Build\n\n- Milestone: M1\n");
        assert_eq!(check_test_cited(&no_line, &c).len(), 1);
    }

    #[test]
    fn the_older_rules_are_reported_as_done_evidence_and_class_b_without_the_path() {
        let c = ctx(&["real-scene"], &[12]);
        let mut e = entry_at("widget", "thing", "done", GOOD_BODY);
        e.owner = "someone".to_owned();
        let bad = check_entry(&e, &c);
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert_eq!(
            (bad[0].check.as_str(), bad[0].id.as_str()),
            ("done-evidence", "widget.thing")
        );
        assert!(
            !bad[0].reason.contains("docs/components"),
            "{}",
            bad[0].reason
        );
        let open = entry_at("widget", "thing", "in-progress", GOOD_BODY);
        let bad = check_entry(&open, &c);
        assert!(bad.iter().any(|f| f.check == "class-b"), "{bad:?}");
    }

    const TABLE: &str = "intro\n<!-- checklist:begin -->\n| rule | what | D-rows | enforced by | kind |\n| --- | --- | --- | --- | --- |\n| a-rule | does a | D1 | `x.rs::t` | test |\n| b-rule | does b | D2 | by review | review |\n| c-rule | does c | D3 | by review | auto:test-cited |\n| m-rule | does m | pending (x) | by review | [mobile slot] |\n<!-- checklist:end -->\nouter\n";

    #[test]
    fn the_checklist_is_read_from_between_the_markers() {
        let rules = read_checklist(TABLE).expect("reads");
        let ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["a-rule", "b-rule", "c-rule", "m-rule"]);
        assert_eq!(rules[0].kind, RuleKind::Test);
        assert_eq!(rules[2].kind, RuleKind::Auto("test-cited".to_owned()));
        assert_eq!(rules[3].kind, RuleKind::MobileSlot);
        assert_eq!(rules[0].enforced_by, "`x.rs::t`");
    }

    #[test]
    fn a_bad_checklist_is_an_error_not_a_pass() {
        let no_begin = TABLE.replace("<!-- checklist:begin -->", "");
        assert!(
            read_checklist(&no_begin)
                .expect_err("an error")
                .contains("checklist:begin")
        );
        let no_end = TABLE.replace("<!-- checklist:end -->", "");
        assert!(
            read_checklist(&no_end)
                .expect_err("an error")
                .contains("checklist:end")
        );
        let twice = TABLE.replace("| b-rule |", "| a-rule |");
        assert!(
            read_checklist(&twice)
                .expect_err("an error")
                .contains("twice")
        );
        let kind = TABLE.replace("| review |", "| nonsense |");
        assert!(
            read_checklist(&kind)
                .expect_err("an error")
                .contains("unknown kind")
        );
        let auto = TABLE.replace("auto:test-cited", "auto:no-such-check");
        assert!(
            read_checklist(&auto)
                .expect_err("an error")
                .contains("no check")
        );
        let cells = TABLE.replace("| does b | D2 |", "| does b |");
        assert!(
            read_checklist(&cells)
                .expect_err("an error")
                .contains("cells")
        );
        let empty = "<!-- checklist:begin -->\n<!-- checklist:end -->";
        assert!(read_checklist(empty).is_err());
    }

    #[test]
    fn named_entries_are_checked_whatever_their_status_and_the_by_hand_rules_follow() {
        let rules = read_checklist(TABLE).expect("reads");
        let entries = vec![entry_at("widget", "thing", "planned", "")];
        let c = ctx(&[], &[]);
        let out = run_ids(&["widget.thing".to_owned()], &entries, &c, &rules).expect("runs");
        assert_eq!(out.code, 1);
        let text = out.lines.join("\n");
        assert!(
            text.contains("widget.thing: status: planned, not done"),
            "{text}"
        );
        assert!(text.contains("widget.thing: decision-row:"), "{text}");
        assert!(text.contains("still to do by hand"), "{text}");
        assert!(
            text.contains("a-rule (test)") && text.contains("b-rule (review)"),
            "{text}"
        );
        assert!(text.contains("m-rule ([mobile slot])"), "{text}");
        assert!(
            !text.contains("c-rule"),
            "an auto rule is not left to hand: {text}"
        );
        let unknown = run_ids(&["nope.nope".to_owned()], &entries, &c, &rules);
        assert!(unknown.expect_err("an error").contains("nope.nope"));
    }

    #[test]
    fn measure_counts_done_failures_skips_external_and_lists_open_entries() {
        let c = ctx(&["real-scene"], &[12]);
        let good = entry_at("widget", "thing", "done", GOOD_BODY);
        let m = measure(std::slice::from_ref(&good), &c);
        assert_eq!((m.checked, m.failures.len()), (1, 0));
        // Instrument: a failing entry is counted, an external one is skipped, an open one listed.
        let bad = entry_at("widget", "other", "done", "### Build\n\n- Milestone: M1\n");
        let ext = entry_at("lunegit", "ext", "external", "");
        let open = entry_at("widget", "open", "planned", "");
        let m = measure(&[bad, ext, open], &c);
        assert_eq!(m.checked, 1);
        assert!(!m.failures.is_empty());
        assert_eq!(m.open, ["widget.open (planned)"]);
        let (new, stale) = against_known_gaps(&m, &[]);
        assert_eq!(new.len(), m.failures.len());
        assert!(stale.is_empty());
    }

    #[test]
    fn a_listed_gap_is_known_and_one_that_no_longer_fails_is_stale() {
        let c = ctx(&["real-scene"], &[12]);
        let bad = entry_at("widget", "other", "done", "### Build\n\n- Milestone: M1\n");
        let m = measure(&[bad], &c);
        let checks: Vec<String> = m.failures.iter().map(|f| f.check.clone()).collect();
        assert!(checks.contains(&"decision-row".to_owned()), "{checks:?}");
        let gaps = [
            ("widget.other", "decision-row", "D1: x"),
            ("widget.gone", "decision-row", "D1: x"),
        ];
        let (new, stale) = against_known_gaps(&m, &gaps);
        assert!(new.iter().all(|f| f.check != "decision-row"));
        assert_eq!(
            stale,
            ["widget.gone: decision-row: listed in KNOWN_GAPS but passes; drop it"]
        );
    }

    // -- the gate: the real catalogue ----------------------------------------------------

    #[test]
    fn every_done_component_passes_the_component_check() {
        let root = root();
        let entries = load_all(&root).expect("entries");
        let c = Ctx::load(&root).expect("ctx");
        let m = measure(&entries, &c);
        // The instrument: the run saw the entries and a known done one.
        assert!(m.checked > 50 && entries.iter().any(|e| e.id == "widget.button"));
        let got: BTreeSet<(String, String)> = m
            .failures
            .iter()
            .map(|f| (f.id.clone(), f.check.clone()))
            .collect();
        let listed: BTreeSet<(String, String)> = KNOWN_GAPS
            .iter()
            .map(|(id, c, why)| {
                assert!(
                    !why.is_empty() && why.contains('D'),
                    "{id}: {c} needs a D-row reason"
                );
                ((*id).to_owned(), (*c).to_owned())
            })
            .collect();
        let lines: Vec<String> = m.failures.iter().map(Failure::line).collect();
        assert_eq!(
            got,
            listed,
            "measured failures differ from KNOWN_GAPS:\n{}",
            lines.join("\n")
        );
    }

    #[test]
    fn the_checklist_table_and_the_checks_agree() {
        let root = root();
        let readme =
            std::fs::read_to_string(root.join("docs/components/README.md")).expect("README");
        let rules = read_checklist(&readme).expect("the real table reads");
        // Every `auto:` id has a check (read_checklist fails otherwise) and every check has a row.
        for (id, _) in CHECKS {
            assert!(
                rules
                    .iter()
                    .any(|r| r.kind == RuleKind::Auto((*id).to_owned())),
                "check `{id}` has no `auto:{id}` row in the table"
            );
        }
        // Every test row names tests that exist, found as the Done line's citations are.
        let mut cited = 0;
        for r in rules.iter().filter(|r| r.kind == RuleKind::Test) {
            let tests = cited_tests(&r.enforced_by);
            assert!(
                !tests.is_empty(),
                "{}: a test row cites no `<file>.rs::<fn>`",
                r.id
            );
            for (file, name) in tests {
                cited += 1;
                assert_eq!(cited_test_problem(&root, &file, &name), None, "{}", r.id);
            }
        }
        assert!(cited >= 8, "the table cites {cited} tests");
        // The mobile program has decided every mobile rule (M3 filled four of the seven, M5 the last
        // three), so no row is left as an open slot.
        assert_eq!(
            rules
                .iter()
                .filter(|r| r.kind == RuleKind::MobileSlot)
                .count(),
            0
        );
    }
}
