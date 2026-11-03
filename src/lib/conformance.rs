//! Cross-implementation conformance fixtures.
//!
//! `conformance/manifest.tsv` lists `.tekl` files that both this crate and the Termux app's Java
//! parser (`TeklParser` / `TeklCompiler`) must accept or reject identically. The app test reads a
//! copy synced by `scripts/sync-conformance.sh`; this test is the Rust half of the contract.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::keys;

struct Case {
    name: String,
    expect_ok: bool,
    line: usize,
    column: usize,
    rows: Option<usize>,
}

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

fn load_manifest() -> Vec<Case> {
    let path = conformance_dir().join("manifest.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

    let mut cases = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = raw.split('\t').collect();
        if cols.len() < 2 {
            panic!("manifest line {}: expected at least 2 columns", i + 1);
        }
        let name = cols[0].trim().to_string();
        let expect = cols[1].trim();
        let line_no = cols.get(2).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let column = cols.get(3).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let rows = cols.get(4).and_then(|v| v.trim().parse().ok());

        match expect {
            "ok" | "error" => {}
            other => panic!("{name}: unknown expectation '{other}'"),
        }

        cases.push(Case {
            name,
            expect_ok: expect == "ok",
            line: line_no,
            column,
            rows,
        });
    }

    assert!(!cases.is_empty(), "manifest lists no cases");
    cases
}

fn fixture_path(name: &str) -> PathBuf {
    conformance_dir().join("cases").join(format!("{name}.tekl"))
}

#[test]
fn fixtures_follow_the_manifest() {
    let cases = load_manifest();
    let mut listed = HashSet::new();

    for case in &cases {
        assert!(listed.insert(case.name.clone()), "duplicate case '{}'", case.name);

        let path = fixture_path(&case.name);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: cannot read fixture: {e}", case.name));

        match keys::compile_source(&source) {
            Ok(result) => {
                assert!(
                    case.expect_ok,
                    "{}: expected a parse error but the file was accepted",
                    case.name
                );
                if let Some(rows) = case.rows {
                    assert_eq!(result.rows.len(), rows, "{}: wrong row count", case.name);
                }
            }
            Err(err) => {
                assert!(
                    !case.expect_ok,
                    "{}: expected success but got: {err}",
                    case.name
                );
                let message = err.to_string();
                if case.line > 0 {
                    let location = format!("(line {}, column {})", case.line, case.column);
                    assert!(
                        message.contains(&location),
                        "{}: expected {} in: {}",
                        case.name,
                        location,
                        message
                    );
                }
            }
        }
    }

    let mut on_disk = HashSet::new();
    for entry in fs::read_dir(conformance_dir().join("cases")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("tekl") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                on_disk.insert(stem.to_string());
            }
        }
    }
    for name in &on_disk {
        assert!(listed.contains(name), "fixture '{name}.tekl' is not in the manifest");
    }
}

#[test]
fn global_properties_do_not_create_rows() {
    let result = keys::compile_source("height = 40\nreload\nESC;\n").unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.globals.len(), 2);
}
