/// Integration tests that verify unused export detection across a small
/// synthetic project with known expected results.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rsprune::parser::analyze_file;

fn path(name: &str) -> PathBuf {
    PathBuf::from(format!("/fake/{name}"))
}

/// Analyse a synthetic "project" and return unused exports as (file, name) pairs.
fn find_unused(files: &[(&str, &str)]) -> Vec<(String, String)> {
    use std::collections::HashSet;

    // Parse all files
    let analyses: Vec<(PathBuf, rsprune::parser::FileAnalysis)> = files
        .iter()
        .map(|(name, src)| (path(name), analyze_file(&path(name), src)))
        .collect();

    // Build used-exports map using a simple same-directory resolver
    let mut used_exports: HashMap<PathBuf, HashSet<String>> = HashMap::new();

    for (from_path, analysis) in &analyses {
        let from_dir = from_path.parent().unwrap_or(Path::new("/fake"));

        for import in &analysis.imports {
            // Resolve relative specifiers against our fake paths
            if let Some(resolved) = resolve_fake(files, from_dir, &import.specifier) {
                let entry = used_exports.entry(resolved).or_default();
                if import.names.is_empty() {
                    entry.insert("__sideeffect__".to_string());
                } else {
                    for name in &import.names {
                        entry.insert(name.clone());
                    }
                }
            }
        }

        for re_export in &analysis.re_exports {
            if let Some(resolved) = resolve_fake(files, from_dir, &re_export.specifier) {
                let entry = used_exports.entry(resolved).or_default();
                if re_export.names.is_empty() {
                    entry.insert("*".to_string());
                } else {
                    for name in &re_export.names {
                        entry.insert(name.clone());
                    }
                }
            }
        }
    }

    // Find unused exports
    let mut unused = Vec::new();
    for (file_path, analysis) in &analyses {
        let used = used_exports.get(file_path);
        for export in &analysis.exports {
            let is_used = match used {
                None => false,
                Some(set) => set.contains("*") || set.contains(&export.name),
            };
            if !is_used {
                let fname = file_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                unused.push((fname, export.name.clone()));
            }
        }
    }

    unused.sort();
    unused
}

/// Resolve a specifier like "./foo" from a directory into one of the fake file paths.
fn resolve_fake(files: &[(&str, &str)], _from_dir: &Path, specifier: &str) -> Option<PathBuf> {
    let stem = specifier.trim_start_matches("./").trim_start_matches("../");
    for (name, _) in files {
        let file_stem = Path::new(name)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if file_stem == stem || *name == stem {
            return Some(path(name));
        }
    }
    None
}

// ─── TESTS ───────────────────────────────────────────────────────────────────

#[test]
fn detects_unused_named_export() {
    let files = vec![
        ("a.ts", "export const foo = 1; export const bar = 2;"),
        ("b.ts", "import { foo } from './a';"), // uses foo but not bar
    ];
    let unused = find_unused(&files);
    assert_eq!(unused, vec![("a.ts".to_string(), "bar".to_string())]);
}

#[test]
fn no_unused_when_all_exports_imported() {
    let files = vec![
        ("a.ts", "export const foo = 1;"),
        ("b.ts", "import { foo } from './a';"),
    ];
    assert!(find_unused(&files).is_empty());
}

#[test]
fn detects_unused_default_export() {
    let files = vec![
        ("a.ts", "export default function() {}"),
        ("b.ts", "export const x = 1;"), // doesn't import a
    ];
    let unused = find_unused(&files);
    assert!(unused.iter().any(|(f, n)| f == "a.ts" && n == "default"));
}

#[test]
fn namespace_import_marks_all_exports_used() {
    let files = vec![
        ("a.ts", "export const foo = 1; export const bar = 2;"),
        ("b.ts", "import * as A from './a';"),
    ];
    assert!(find_unused(&files).is_empty());
}

#[test]
fn re_export_star_marks_all_used() {
    let files = vec![
        ("a.ts", "export const foo = 1; export const bar = 2;"),
        ("b.ts", "export * from './a';"),
    ];
    assert!(find_unused(&files).is_empty());
}

#[test]
fn re_export_named_marks_specific_used() {
    let files = vec![
        ("a.ts", "export const foo = 1; export const bar = 2;"),
        ("b.ts", "export { foo } from './a';"), // only re-exports foo, bar unused
    ];
    let unused = find_unused(&files);
    assert!(unused.iter().any(|(f, n)| f == "a.ts" && n == "bar"));
    assert!(!unused.iter().any(|(f, n)| f == "a.ts" && n == "foo"));
}

#[test]
fn dynamic_import_marks_all_used() {
    let files = vec![
        ("a.ts", "export const foo = 1;"),
        ("b.ts", "async function load() { const m = await import('./a'); return m.foo; }"),
    ];
    assert!(find_unused(&files).is_empty());
}

#[test]
fn type_alias_unused_when_not_imported() {
    let files = vec![
        ("a.ts", "export type Foo = string; export const bar = 1;"),
        ("b.ts", "import { bar } from './a';"),
    ];
    let unused = find_unused(&files);
    assert!(unused.iter().any(|(f, n)| f == "a.ts" && n == "Foo"));
}

#[test]
fn aliased_import_tracks_original_name() {
    let files = vec![
        ("a.ts", "export const original = 1; export const other = 2;"),
        ("b.ts", "import { original as alias } from './a';"),
    ];
    let unused = find_unused(&files);
    // "original" is used (imported as alias), "other" is not
    assert!(unused.iter().any(|(f, n)| f == "a.ts" && n == "other"));
    assert!(!unused.iter().any(|(f, n)| f == "a.ts" && n == "original"));
}
