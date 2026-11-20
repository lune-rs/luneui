//! Generates `docs/DECISIONS-INDEX.md` from `docs/DECISIONS.md` (D400).
//!
//! `decisions-index <DECISIONS.md> <out.md>` writes the index; `decisions-index --check
//! <DECISIONS.md> <out.md>` exits 1 with a line diff when `out.md` is stale. Exit 2 on a usage or
//! validation error, each printed as `DECISIONS.md:<line>: <reason>`. The output is byte-stable: no
//! dates of generation, LF line ends, one final newline.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

/// The words that make a later row change an earlier one.
const KEYWORDS: [&str; 5] = ["supersedes", "supersede", "amends", "refines", "refining"];
/// A title shorter than this (in chars) is extended by the next sentence.
const SHORT_TITLE: usize = 40;
/// A title longer than this (in chars) is cut on a word boundary.
const MAX_TITLE: usize = 160;

/// Errors as `(line, reason)`.
type Errs = Vec<(usize, String)>;
/// For each row id, the later rows that change it.
type ChangeMap = BTreeMap<u32, Vec<Change>>;

/// One `| D<n> | <date> | <decision> | ...` row.
#[derive(Debug, Clone)]
struct Row {
    id: u32,
    line: usize,
    date: String,
    text: String,
}

/// A later row that changes an earlier one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Change {
    by: u32,
    keyword: String,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (check, src, out) = match args.as_slice() {
        [src, out] => (false, src, out),
        [flag, src, out] if flag == "--check" => (true, src, out),
        _ => {
            eprintln!("usage: decisions-index [--check] <DECISIONS.md> <out.md>");
            return ExitCode::from(2);
        }
    };
    let text = match std::fs::read_to_string(src) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("decisions-index: cannot read {src}: {e}");
            return ExitCode::from(2);
        }
    };
    let name = Path::new(src)
        .file_name()
        .map_or_else(|| src.clone(), |n| n.to_string_lossy().into_owned());
    let generated = match generate(&text) {
        Ok(g) => g,
        Err(errs) => {
            for (line, reason) in errs {
                eprintln!("{name}:{line}: {reason}");
            }
            return ExitCode::from(2);
        }
    };
    if check {
        let current = std::fs::read_to_string(out).unwrap_or_default();
        if current == generated {
            return ExitCode::SUCCESS;
        }
        print_diff(&current, &generated);
        return ExitCode::from(1);
    }
    match std::fs::write(out, generated) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("decisions-index: cannot write {out}: {e}");
            ExitCode::from(2)
        }
    }
}

/// Prints the lines only in `current` (`-`) and only in `wanted` (`+`), at most 40 of them.
fn print_diff(current: &str, wanted: &str) {
    let cur: BTreeSet<&str> = current.lines().collect();
    let want: BTreeSet<&str> = wanted.lines().collect();
    let mut shown = 0;
    for l in current.lines().filter(|l| !want.contains(l)) {
        println!("-{l}");
        shown += 1;
        if shown >= 40 {
            break;
        }
    }
    for l in wanted.lines().filter(|l| !cur.contains(l)) {
        println!("+{l}");
        shown += 1;
        if shown >= 80 {
            break;
        }
    }
    println!("decisions-index: the index is stale");
}

/// Parses the rows and renders the index; errors are `(line, reason)`.
fn generate(text: &str) -> Result<String, Errs> {
    let rows = parse(text)?;
    let changes = changes(&rows)?;
    Ok(render(&rows, &changes))
}

/// The rows of the file, sorted by id.
fn parse(text: &str) -> Result<Vec<Row>, Errs> {
    let mut rows: Vec<Row> = Vec::new();
    let mut errs: Vec<(usize, String)> = Vec::new();
    let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let l = raw.strip_suffix('\r').unwrap_or(raw);
        let Some(rest) = l.strip_prefix("| D") else {
            continue;
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            continue;
        }
        let Some(rest) = rest[digits.len()..].strip_prefix(" | ") else {
            continue;
        };
        let Ok(id) = digits.parse::<u32>() else {
            errs.push((line, format!("id D{digits} is not a number")));
            continue;
        };
        let Some((date, rest)) = rest.split_once(" | ") else {
            errs.push((line, format!("D{id} has no decision cell")));
            continue;
        };
        if !is_date(date) {
            errs.push((line, format!("D{id} date `{date}` is not YYYY-MM-DD")));
            continue;
        }
        let cell = rest.split_once(" | ").map_or(rest, |(c, _)| c);
        let cell = cell.strip_suffix(" |").unwrap_or(cell).trim();
        if let Some(first) = seen.insert(id, line) {
            errs.push((line, format!("duplicate id D{id} (first on line {first})")));
            continue;
        }
        rows.push(Row {
            id,
            line,
            date: date.to_string(),
            text: cell.to_string(),
        });
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    rows.sort_by_key(|r| r.id);
    Ok(rows)
}

/// `YYYY-MM-DD`, digits only.
fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// For every row, the later rows that supersede, amend or refine it. Keywords are looked up in the
/// row's decision text (the cell after the date; the reason and rejected cells are not read, so a
/// change said only there reads as in force); the ids that follow a keyword up to the first `. `,
/// `;`, `)` or the end are its targets, except an id that follows `plan ` or `roadmap ` (a
/// plan-local id).
fn changes(rows: &[Row]) -> Result<ChangeMap, Errs> {
    let known: BTreeSet<u32> = rows.iter().map(|r| r.id).collect();
    let mut out: BTreeMap<u32, BTreeSet<Change>> = BTreeMap::new();
    let mut errs = Vec::new();
    for row in rows {
        for (keyword, target) in targets(&row.text) {
            if !known.contains(&target) {
                errs.push((
                    row.line,
                    format!("D{} {keyword} D{target}, which is not a row", row.id),
                ));
            } else if target >= row.id {
                errs.push((
                    row.line,
                    format!(
                        "D{} {keyword} D{target}, which is not an earlier row",
                        row.id
                    ),
                ));
            } else {
                out.entry(target).or_default().insert(Change {
                    by: row.id,
                    keyword: keyword.to_string(),
                });
            }
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    Ok(out
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect())
}

/// `(keyword, target id)` pairs named in one row's text.
fn targets(text: &str) -> Vec<(&'static str, u32)> {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    // Lowercasing must not move positions, or the keyword offsets would drift from `chars`.
    let lower = if lower.len() == chars.len() {
        lower
    } else {
        chars.clone()
    };
    let mut found = Vec::new();
    let mut i = 0;
    while i < lower.len() {
        let before_ok = i == 0 || !is_word_char(lower[i - 1]);
        let mut hit = None;
        if before_ok {
            for kw in KEYWORDS {
                let k: Vec<char> = kw.chars().collect();
                let end = i + k.len();
                if end <= lower.len()
                    && lower[i..end] == k[..]
                    && (end == lower.len() || !is_word_char(lower[end]))
                    && hit.is_none_or(|(_, e)| end > e)
                {
                    hit = Some((kw, end));
                }
            }
        }
        let Some((kw, end)) = hit else {
            i += 1;
            continue;
        };
        let name = if kw == "refining" { "refines" } else { kw };
        let stop = segment_end(&chars, end);
        for id in ids(&chars[end..stop]) {
            found.push((name, id));
        }
        i = end;
    }
    found
}

/// The end of the id segment that starts at `from`.
fn segment_end(chars: &[char], from: usize) -> usize {
    let mut j = from;
    while j < chars.len() {
        let c = chars[j];
        let dot_space = c == '.' && chars.get(j + 1) == Some(&' ');
        let bar = c == ' ' && chars.get(j + 1) == Some(&'|') && chars.get(j + 2) == Some(&' ');
        if c == ';' || c == ')' || dot_space || bar {
            break;
        }
        j += 1;
    }
    j
}

/// The `D<digits>` ids of a segment, skipping the ones after `plan ` or `roadmap `.
fn ids(seg: &[char]) -> Vec<u32> {
    let s: String = seg.iter().collect();
    let mut out = Vec::new();
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == 'D' && (i == 0 || !is_word_char(cs[i - 1])) {
            let digits: String = cs[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                let head: String = cs[..i].iter().collect();
                let local = head.ends_with("plan ") || head.ends_with("roadmap ");
                if !local && let Ok(n) = digits.parse::<u32>() {
                    out.push(n);
                }
                i += 1 + digits.len();
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Where the sentence that starts at `from` ends: just after a `.` followed by a space (or the
/// end) outside a backtick span, else the end of the text.
fn sentence_end(chars: &[char], from: usize) -> usize {
    let mut tick = false;
    for j in from..chars.len() {
        match chars[j] {
            '`' => tick = !tick,
            '.' if !tick && chars.get(j + 1).is_none_or(|c| *c == ' ') => return j + 1,
            _ => {}
        }
    }
    chars.len()
}

/// The index title of a decision cell (D400): the first sentence, extended by the next when it
/// is under 40 chars, cut to 160 chars on a word boundary outside backticks, with ` …`.
fn title(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut end = sentence_end(&chars, 0);
    if end < chars.len()
        && chars[..end]
            .iter()
            .collect::<String>()
            .trim()
            .chars()
            .count()
            < SHORT_TITLE
    {
        let mut start = end;
        while start < chars.len() && chars[start] == ' ' {
            start += 1;
        }
        end = sentence_end(&chars, start);
    }
    let chars = &chars[..end];
    let mut s: String = if chars.len() > MAX_TITLE {
        cap(chars)
    } else {
        chars.iter().collect()
    };
    s = s.trim().to_string();
    escape_bars(&s)
}

fn ticks(chars: &[char]) -> usize {
    chars.iter().filter(|c| **c == '`').count()
}

/// Cuts `chars` (longer than the cap) at a space at or before the cap, with an even number of
/// backticks before it, and adds ` …`.
fn cap(chars: &[char]) -> String {
    let mut cut = (1..=MAX_TITLE)
        .rev()
        .find(|&k| chars[k] == ' ' && ticks(&chars[..k]) % 2 == 0);
    if cut.is_none() {
        // No usable space: cut at the cap, before an open backtick span.
        let k = MAX_TITLE;
        cut = Some(if ticks(&chars[..k]) % 2 == 0 {
            k
        } else {
            chars[..k].iter().rposition(|c| *c == '`').unwrap_or(k)
        });
    }
    let k = cut.unwrap_or(MAX_TITLE);
    let head: String = chars[..k].iter().collect();
    format!("{} …", head.trim_end())
}

/// Escapes each `|` not already escaped, for a table cell.
fn escape_bars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev = ' ';
    for c in s.chars() {
        if c == '|' && prev != '\\' {
            out.push('\\');
        }
        out.push(c);
        prev = c;
    }
    out
}

/// The index file.
fn render(rows: &[Row], changes: &ChangeMap) -> String {
    let mut o = String::new();
    o.push_str("<!-- Generated by scripts/gen-decisions-index.sh from docs/DECISIONS.md. Do not edit: regenerate. -->\n");
    o.push_str("# Decisions index\n\n");
    o.push_str(&format!(
        "One line per row of `docs/DECISIONS.md` ({} rows), oldest first. The file is generated by\n`scripts/gen-decisions-index.sh` (the gate checks it is current); do not edit it.\n\n",
        rows.len()
    ));
    o.push_str("- **decision** is the row's first sentence (the next one too when the first is short), cut at 160 characters. Open the row in `docs/DECISIONS.md` (search `| D<n> |`) for the full text, the reason and what was rejected.\n");
    o.push_str("- **status** is `in force`, or `changed by D<n> (<keyword>)` when a later row says it supersedes, amends or refines this one, in whole or in part. The index does not judge whole versus part: read the later row before relying on a row that is changed. A later row that changes an earlier one in other words, or says so only in its reason or rejected cell, is not caught, so the full row has the last word.\n");
    o.push_str("- Numbering has gaps (a range may be reserved by another branch); ids are never reused.\n\n");
    o.push_str("| id | date | decision | status |\n|---|---|---|---|\n");
    for r in rows {
        let status = match changes.get(&r.id) {
            None => "in force".to_string(),
            Some(cs) => {
                let list: Vec<String> = cs
                    .iter()
                    .map(|c| format!("D{} ({})", c.by, c.keyword))
                    .collect();
                format!("changed by {}", list.join(", "))
            }
        };
        o.push_str(&format!(
            "| D{} | {} | {} | {} |\n",
            r.id,
            r.date,
            title(&r.text),
            status
        ));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str) -> Vec<Row> {
        parse(text).expect("fixture parses")
    }

    fn change_map(text: &str) -> BTreeMap<u32, Vec<(u32, String)>> {
        let r = rows(text);
        changes(&r)
            .expect("fixture changes")
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|c| (c.by, c.keyword)).collect()))
            .collect()
    }

    #[test]
    fn rows_sort_numerically_with_gaps() {
        let t = "| D10 | 2026-01-02 | ten | w | r |\n| D2 | 2026-01-01 | two | w | r |\n| D337 | 2026-01-03 | gap | w | r |\n";
        let ids: Vec<u32> = rows(t).iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![2, 10, 337]);
    }

    #[test]
    fn an_escaped_bar_stays_in_the_title() {
        let t = "| D5 | 2026-01-01 | A \\| B. Rest | why | rej |\n";
        let r = rows(t);
        assert_eq!(r[0].text, "A \\| B. Rest");
        assert_eq!(title(&r[0].text), "A \\| B. Rest");
        let out = render(&r, &BTreeMap::new());
        assert!(out.contains("| D5 | 2026-01-01 | A \\| B. Rest | in force |"));
    }

    #[test]
    fn a_row_with_two_separators_parses() {
        let r = rows("| D88 | 2026-11-04 | Only a decision. |\n| D89 | 2026-11-04 | Two cells\n");
        assert_eq!(r[0].text, "Only a decision.");
        assert_eq!(r[1].text, "Two cells");
    }

    #[test]
    fn keywords_name_their_targets() {
        let t = "| D1 | 2026-01-01 | a | w | r |\n| D2 | 2026-01-01 | b | w | r |\n| D3 | 2026-01-01 | c | w | r |\n| D4 | 2026-01-01 | d | w | r |\n\
| D5 | 2026-01-01 | This supersedes D1. Done | w | r |\n\
| D6 | 2026-01-01 | Amends D2 | w | r |\n\
| D7 | 2026-01-01 | Snapshot, refining D3 and D4 (plan D3; supersedes D4's limit) | w | r |\n\
| D8 | 2026-01-01 | Supersedes the widget lists in D1 and D2 | w | r |\n";
        let m = change_map(t);
        let s = |v: &[(u32, &str)]| -> Vec<(u32, String)> {
            v.iter().map(|(a, b)| (*a, b.to_string())).collect()
        };
        assert_eq!(m[&1], s(&[(5, "supersedes"), (8, "supersedes")]));
        assert_eq!(m[&2], s(&[(6, "amends"), (8, "supersedes")]));
        assert_eq!(m[&3], s(&[(7, "refines")]));
        assert_eq!(m[&4], s(&[(7, "refines"), (7, "supersedes")]));
    }

    #[test]
    fn plan_local_ids_are_skipped() {
        let m = change_map(
            "| D1 | 2026-01-01 | a | w | r |\n| D2 | 2026-01-01 | Amends D1 (roadmap D1, plan D2) | w | r |\n",
        );
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn a_keyword_inside_a_word_does_not_count() {
        let m = change_map(
            "| D1 | 2026-01-01 | a | w | r |\n| D2 | 2026-01-01 | unamends D1 and refinesD1 | w | r |\n",
        );
        assert!(m.is_empty());
    }

    #[test]
    fn bad_targets_and_duplicates_are_errors() {
        let fwd =
            rows("| D1 | 2026-01-01 | Supersedes D2 | w | r |\n| D2 | 2026-01-01 | b | w | r |\n");
        assert!(changes(&fwd).is_err());
        let unknown = rows("| D5 | 2026-01-01 | Amends D3 | w | r |\n");
        assert!(changes(&unknown).is_err());
        let dup = parse("| D1 | 2026-01-01 | a | w | r |\n| D1 | 2026-01-02 | b | w | r |\n");
        let errs = dup.expect_err("duplicate id");
        assert_eq!(errs[0].0, 2);
        assert!(errs[0].1.contains("line 1"));
        assert!(parse("| D1 | 01-01-2026 | a | w | r |\n").is_err());
    }

    #[test]
    fn titles_follow_the_rules() {
        // A short first sentence takes the next one.
        assert_eq!(
            title("Short one. The second sentence follows here. Third."),
            "Short one. The second sentence follows here."
        );
        // A long first sentence stands alone.
        let long_first =
            "A first sentence that is comfortably longer than forty characters. Second.";
        assert_eq!(
            title(long_first),
            "A first sentence that is comfortably longer than forty characters."
        );
        // The cap is on a word boundary.
        let words = "word ".repeat(60);
        let t = title(&words);
        assert!(t.ends_with(" …"), "{t}");
        assert!(t.chars().count() <= MAX_TITLE + 2, "{}", t.chars().count());
        assert!(t.trim_end_matches(" …").ends_with("word"));
        // Never inside a backtick span: the span starts before the cap and ends after it.
        let pad = "x".repeat(150);
        let spanned = format!("{pad} `Ui::some_call with spaces inside it` and more words after");
        let t = title(&spanned);
        assert_eq!(ticks(&t.chars().collect::<Vec<_>>()) % 2, 0, "{t}");
        assert!(!t.contains('`'), "{t}");
        // A `. ` inside a backtick span is not a sentence end.
        assert_eq!(
            title("A call such as `a. b` is one sentence that is long enough. Next."),
            "A call such as `a. b` is one sentence that is long enough."
        );
        // A bar in a title is escaped; an escaped one is not doubled.
        assert_eq!(
            title("a | b and a \\| c are long enough to be one sentence here."),
            "a \\| b and a \\| c are long enough to be one sentence here."
        );
    }

    #[test]
    fn check_diff_and_output_are_stable() {
        let t = "| D1 | 2026-01-01 | one | w | r |\n| D2 | 2026-01-02 | Supersedes D1 | w | r |\n";
        let a = generate(t).expect("generates");
        assert_eq!(a, generate(t).expect("generates"));
        assert!(a.ends_with("|\n") && !a.ends_with("\n\n") && !a.contains('\r'));
        assert!(a.contains("| D1 | 2026-01-01 | one | changed by D2 (supersedes) |"));
        assert!(a.contains("| D2 | 2026-01-02 | Supersedes D1 | in force |"));
    }

    /// Instrument check on the real file: the rows that change an earlier one and what they name.
    #[test]
    fn the_real_decisions_file_has_the_known_changing_rows() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/DECISIONS.md");
        let text = std::fs::read_to_string(path).expect("docs/DECISIONS.md is readable");
        // Only the rows up to D336 (this task's own rows may add more after).
        let rs: Vec<Row> = rows(&text).into_iter().filter(|r| r.id <= 336).collect();
        let mut got: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for r in &rs {
            for (_, t) in targets(&r.text) {
                got.entry(r.id).or_default().insert(t);
            }
        }
        let known: [(u32, &[u32]); 23] = [
            (12, &[5]),
            (39, &[34]),
            (58, &[56]),
            (85, &[41]),
            (115, &[109]),
            (132, &[129]),
            (159, &[157, 158]),
            (172, &[170]),
            (215, &[107]),
            (226, &[218]),
            (235, &[47]),
            (239, &[13, 16]),
            (255, &[137]),
            (256, &[137]),
            (257, &[138]),
            (262, &[128]),
            (263, &[137, 259]),
            (268, &[246]),
            (283, &[271]),
            (284, &[206]),
            (288, &[285]),
            (298, &[117]),
            (320, &[295]),
        ];
        let want: BTreeMap<u32, BTreeSet<u32>> = known
            .iter()
            .map(|(k, v)| (*k, v.iter().copied().collect()))
            .collect();
        assert_eq!(got, want);
        // The whole file, new rows included, still validates and renders.
        generate(&text).expect("the real file validates");
    }
}
