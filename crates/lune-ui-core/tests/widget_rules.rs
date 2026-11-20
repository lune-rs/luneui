//! Source rules for the widgets (D403): two things no widget may do, checked by reading the files
//! under `src/widgets/`.
//!
//! - no widget names the `Raised` or `Outside` layer: `Ui::overlay` picks the layer of an overlay,
//!   `Raised` only while an older modal is on the stack, and `Ui::outside_modals` owns `Outside`
//!   (D137, D255, D259, D263);
//! - no widget builds or runs a context menu, so none but `widgets/menu.rs` calls `menu`,
//!   `menu_with`, `menu_window` or `menu_window_in`: a widget reports the secondary click and the
//!   show-menu action as `context` and the app opens the menu (D292).
//!
//! Each rule is a function over a file's path and text that returns the offending `path:line`
//! strings, so a unit test can hand it a bad line and a commented-out one. A `//` comment (a doc
//! comment too) is not code; a `/* */` block is not skipped, and no widget has one.

use std::path::{Path, PathBuf};

/// The line without its `//` comment.
fn code(line: &str) -> &str {
    line.split_once("//").map_or(line, |(code, _)| code)
}

/// Every line of `text` whose code holds one of `needles`, as `path:line: <needle>`.
fn offences(path: &str, text: &str, needles: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        for needle in needles {
            if code(line).contains(needle) {
                out.push(format!("{path}:{}: `{needle}`", i + 1));
            }
        }
    }
    out
}

/// The layer rule over one widget file.
fn layer_offences(path: &str, text: &str) -> Vec<String> {
    offences(path, text, &["LayerId::Raised", "LayerId::Outside"])
}

/// The menu rule over one widget file; `widgets/menu.rs` is the one that may.
fn menu_offences(path: &str, text: &str) -> Vec<String> {
    if path.ends_with("widgets/menu.rs") {
        return Vec::new();
    }
    offences(
        path,
        text,
        &[".menu(", ".menu_with(", "menu_window(", "menu_window_in("],
    )
}

/// The `.rs` files under `dir`, as `(path, text)`, `/` separators in the path.
fn rust_files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for entry in std::fs::read_dir(&d).expect("the widgets directory is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&path).expect("a widget file is readable");
                out.push((path.to_string_lossy().replace('\\', "/"), text));
            }
        }
    }
    out.sort();
    out
}

/// The widget files. A directory with no `.rs` file is an error, so a wrong path cannot pass.
fn widgets() -> Vec<(String, String)> {
    let files = rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/widgets"));
    assert!(
        !files.is_empty(),
        "no .rs file under src/widgets: wrong path"
    );
    files
}

fn all(rule: fn(&str, &str) -> Vec<String>) -> Vec<String> {
    widgets().iter().flat_map(|(p, t)| rule(p, t)).collect()
}

#[test]
fn no_widget_names_the_raised_or_outside_layer() {
    let found = all(layer_offences);
    assert!(
        found.is_empty(),
        "a widget names a layer `Ui::overlay` owns:\n{}",
        found.join("\n")
    );
}

#[test]
fn no_widget_opens_a_menu() {
    let found = all(menu_offences);
    assert!(
        found.is_empty(),
        "a widget opens a menu; report `context` instead:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_layer_rule_sees_a_bad_line_and_skips_a_comment() {
    let bad = "let l = LayerId::Raised;\nui.on(LayerId::Outside);\n";
    assert_eq!(
        layer_offences("w.rs", bad),
        ["w.rs:1: `LayerId::Raised`", "w.rs:2: `LayerId::Outside`"]
    );
    let commented = "// LayerId::Raised is the overlay's\n/// uses LayerId::Outside\nlet l = LayerId::Popup; // not LayerId::Raised\n";
    assert!(layer_offences("w.rs", commented).is_empty());
}

#[test]
fn the_menu_rule_sees_a_bad_line_and_skips_a_comment_and_menu_rs() {
    let bad = "ui.menu(\"k\", &rows, at);\nui.menu_with(k);\nlet r = ui.menu_window(k, &rows);\nui.menu_window_in(k);\n";
    assert_eq!(menu_offences("widgets/tab_strip.rs", bad).len(), 4);
    let commented = "// ui.menu(k, &rows, at) is the app's\nlet r = row.menu_label; // .menu(\n";
    assert!(menu_offences("widgets/tab_strip.rs", commented).is_empty());
    assert!(menu_offences("src/widgets/menu.rs", bad).is_empty());
}

#[test]
fn a_directory_without_rust_files_is_an_error_not_a_pass() {
    let dir = std::env::temp_dir().join(format!("widget-rules-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("creates its scratch dir");
    assert!(rust_files(&dir).is_empty());
    std::fs::remove_dir_all(&dir).expect("removes its scratch dir");
}
