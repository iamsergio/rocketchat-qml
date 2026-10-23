// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The check that `slint.txt` and the `Slint` column of TRACEABILITY.md say the same thing: the
//! list of GUI tests that pass against `rocketchat_slint`, and what each feature says about them.
//!
//! Everything is read from the source tree: cargo runs tests from the crate's directory, which is
//! where `slint.txt` and the tests are, and TRACEABILITY.md is two levels up from there.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// One `#[test]` of `tests/*.rs`, with the features its `// <ID>` comment names.
struct GuiTest {
    /// `<file stem>::<function name>`, as `slint.txt` writes it.
    qualified: String,
    /// The features of TRACEABILITY.md, as the `// <ID>` above the test says: one comment can name
    /// several, for a test that covers them together.
    features: Vec<String>,
}

/// A feature row of TRACEABILITY.md: its ID, its `Slint` cell and the line it is on.
#[derive(Debug)]
struct Row {
    id: String,
    slint: String,
    line: usize,
}

// INFRA-005
#[test]
fn slint_txt_and_the_slint_column_agree() {
    let listed = slint_list();
    let gui_tests = gui_tests();
    let rows = traceability_rows();
    let mut broken = Vec::new();

    // Every line names a test that exists, and no line comes twice: `--exact` would otherwise run
    // nothing at all for a typo, and the same function twice for a duplicate.
    let mut seen = BTreeSet::new();
    for (number, entry) in &listed {
        if !gui_tests.iter().any(|test| &test.qualified == entry) {
            broken.push(format!(
                "slint.txt:{number}: no GUI test is called {entry:?}"
            ));
        } else if !seen.insert(entry) {
            broken.push(format!("slint.txt:{number}: {entry} is listed twice"));
        }
    }

    // Every row's `Slint` cell says what the list says about the feature's GUI tests.
    for row in &rows {
        let tests: Vec<&GuiTest> = gui_tests
            .iter()
            .filter(|test| test.features.iter().any(|id| id == &row.id))
            .collect();
        let in_list = tests
            .iter()
            .filter(|test| seen.contains(&test.qualified))
            .count();
        let expected = if tests.is_empty() {
            "n/a"
        } else if in_list == tests.len() {
            "done"
        } else if in_list == 0 {
            "todo"
        } else {
            "partial"
        };
        if row.slint == expected {
            continue;
        }
        let missing: Vec<&str> = tests
            .iter()
            .filter(|test| !seen.contains(&test.qualified))
            .map(|test| test.qualified.as_str())
            .collect();
        let detail = if missing.is_empty() {
            String::new()
        } else if in_list == 0 {
            format!(
                ": none of its GUI tests is in slint.txt ({})",
                missing.join(", ")
            )
        } else {
            format!(": only some are ({})", missing.join(", "))
        };
        broken.push(format!(
            "TRACEABILITY.md:{}: {} says Slint {:?}, but slint.txt says {expected}{detail}",
            row.line, row.id, row.slint
        ));
    }

    assert!(
        broken.is_empty(),
        "slint.txt and TRACEABILITY.md disagree:\n{}",
        broken.join("\n")
    );
}

/// The entries of `slint.txt`, with the line each is on. `#` comments and blank lines are skipped,
/// as ctest reads the file too.
fn slint_list() -> Vec<(usize, String)> {
    let path = crate_dir().join("slint.txt");
    let text = read(&path);
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            Some((index + 1, line.to_owned()))
        })
        .collect()
}

/// Every `#[test]` of `tests/*.rs`, with the feature its `// <ID>` comment names.
fn gui_tests() -> Vec<GuiTest> {
    let tests_dir = crate_dir().join("tests");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&tests_dir)
        .unwrap_or_else(|error| panic!("{}: {error}", tests_dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    files.sort();

    let mut tests = Vec::new();
    for path in files {
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let text = read(&path);
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let Some(name) = test_name(line) else {
                continue;
            };
            if is_test(&lines, index) {
                tests.push(GuiTest {
                    qualified: format!("{stem}::{name}"),
                    features: feature_above(&lines, index),
                });
            }
        }
    }
    tests
}

/// The function name of a line that starts a test at the top level: `fn name() {`. Indented lines
/// are inside a function, where an inner `fn` is not a test of this file.
fn test_name(line: &str) -> Option<String> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    let rest = line.trim().strip_prefix("fn ")?;
    Some(rest.split('(').next()?.trim().to_owned())
}

/// Whether the function starting on `test_line` is one of the file's tests: `#[test]` among the
/// attributes right above it.
fn is_test(lines: &[&str], test_line: usize) -> bool {
    lines[..test_line]
        .iter()
        .rev()
        .map(|line| line.trim())
        .take_while(|line| line.starts_with("#[") || line.is_empty())
        .any(|line| line.starts_with("#[test]"))
}

/// The features an `// <ID>` comment names, looking up from a test function: the comment right
/// above the test's attributes is the one that names its features, one `// <ID, ID>` list of them.
fn feature_above(lines: &[&str], test_line: usize) -> Vec<String> {
    for line in lines[..test_line].iter().rev() {
        let line = line.trim();
        if line.starts_with("#[") || line.is_empty() || line.starts_with("///") {
            continue;
        }
        let Some(comment) = line.strip_prefix("//") else {
            return Vec::new();
        };
        return comment
            .split(',')
            .filter_map(|id| feature_id(id.trim()))
            .collect();
    }
    Vec::new()
}

/// `AUTH-001`: upper case letters, a dash, digits, and nothing else.
fn feature_id(id: &str) -> Option<String> {
    let mut parts = id.split('-');
    let (letters, number) = (parts.next()?, parts.next()?);
    if parts.next().is_none()
        && !letters.is_empty()
        && letters.chars().all(|what| what.is_ascii_uppercase())
        && !number.is_empty()
        && number.chars().all(|what| what.is_ascii_digit())
    {
        return Some(id.to_owned());
    }
    None
}

/// The feature rows of TRACEABILITY.md.
fn traceability_rows() -> Vec<Row> {
    let path = repo_root().join("TRACEABILITY.md");
    rows_of(&read(&path))
}

/// The feature rows of a TRACEABILITY.md: each one under its `| ID | Feature | Slint | Tests |`.
fn rows_of(text: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for (index, line) in text.lines().enumerate() {
        let cells = table_cells(line);
        match cells.first().map(String::as_str) {
            // A table header: the tables of features are the ones with an ID and a Feature column,
            // and every one of them has to say where its Slint column is.
            Some("ID") => in_table = cells.iter().any(|cell| cell == "Feature"),
            // The dashes under the header, which is a row too.
            Some(cell) if cell.chars().all(|what| what == '-') => {}
            Some(id) if in_table => rows.push(Row {
                id: id.to_owned(),
                slint: cells
                    .get(2)
                    .cloned()
                    .unwrap_or_else(|| "no Slint column".to_owned()),
                line: index + 1,
            }),
            _ => {}
        }
    }
    rows
}

/// The cells of a line that is one row of a Markdown table, or nothing for any other line.
fn table_cells(line: &str) -> Vec<String> {
    let line = line.trim();
    if !line.starts_with('|') {
        return Vec::new();
    }
    line.trim_start_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The crate's own directory: cargo runs tests from it, and it holds `slint.txt` and `tests/`.
fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The repository root, where TRACEABILITY.md is.
fn repo_root() -> PathBuf {
    crate_dir().join("..").join("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-005
    #[test]
    fn a_test_function_is_named_with_the_feature_above_it() {
        let lines = [
            "// AUTH-001",
            "#[test]",
            "#[ignore = \"needs the app: run through ctest\"]",
            "fn logs_in() {",
            "    assert!(true);",
            "}",
        ];
        assert_eq!(feature_above(&lines, 3), vec!["AUTH-001".to_owned()]);
        assert!(feature_above(&lines, 5).is_empty());
        assert!(is_test(&lines, 3));
        assert!(!is_test(&lines, 5));
        assert_eq!(test_name(lines[3]), Some("logs_in".to_owned()));
        assert_eq!(test_name(lines[5]), None);
    }

    // INFRA-005
    #[test]
    fn a_feature_id_is_upper_case_letters_and_digits() {
        assert_eq!(feature_id("AUTH-001"), Some("AUTH-001".to_owned()));
        assert_eq!(feature_id("AUTH-001 something"), None);
        assert_eq!(feature_id("no id here"), None);

        // One comment can name the features a test covers together.
        let lines = ["// ROOM-006, ROOM-007", "#[test]", "fn favorites() {"];
        assert_eq!(
            feature_above(&lines, 2),
            vec!["ROOM-006".to_owned(), "ROOM-007".to_owned()]
        );
    }

    // INFRA-005
    #[test]
    fn a_row_of_a_feature_table_has_its_id_and_its_slint_cell() {
        let text = "\
| ID | Feature | Slint | Tests |
|---|---|---|---|
| AUTH-001 | Log in | done | GUI: `auth.rs` |
| INFRA-001 | No GUI test | n/a | unit |
not a table | row";
        let rows = rows_of(text);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].id, "AUTH-001");
        assert_eq!(rows[0].slint, "done");
        assert_eq!(rows[1].id, "INFRA-001");
        assert_eq!(rows[1].slint, "n/a");
    }
}
