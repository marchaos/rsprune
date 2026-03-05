use rsprune::files::collect_files;
use std::fs;
use std::path::Path;

fn to_strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn filenames(paths: &[std::path::PathBuf]) -> Vec<String> {
    let mut names: Vec<String> = paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// ─── EXTENSION FILTERING ─────────────────────────────────────────────────────

#[test]
fn only_collects_supported_extensions() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const x = 1;").unwrap();
    fs::write(src.join("b.tsx"), "export const y = 1;").unwrap();
    fs::write(src.join("c.js"), "export const z = 1;").unwrap();
    fs::write(src.join("d.css"), ".foo {}").unwrap();
    fs::write(src.join("e.json"), "{}").unwrap();
    fs::write(src.join("f.md"), "# hello").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(names.contains(&"b.tsx".to_string()));
    assert!(names.contains(&"c.js".to_string()));
    assert!(!names.contains(&"d.css".to_string()), "css should be excluded");
    assert!(!names.contains(&"e.json".to_string()), "json should be excluded");
    assert!(!names.contains(&"f.md".to_string()), "md should be excluded");
}

// ─── NODE_MODULES SKIPPING ────────────────────────────────────────────────────

#[test]
fn skips_node_modules() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let nm = dir.path().join("src/node_modules/some-pkg");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&nm).unwrap();

    fs::write(src.join("a.ts"), "export const x = 1;").unwrap();
    fs::write(nm.join("index.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"index.ts".to_string()), "node_modules file should be skipped");
}

// ─── INCLUDE GLOB SEMANTICS ───────────────────────────────────────────────────

#[test]
fn include_bare_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("other")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("other/b.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"b.ts".to_string()), "other/ should not be included");
}

#[test]
fn include_glob_pattern() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("src/a.spec.ts"), "export const y = 1;").unwrap();

    // Include only non-spec files
    let files = collect_files(
        dir.path(),
        &to_strings(&["src/**/*.ts"]),
        &to_strings(&["**/*.spec.ts"]),
    );
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"a.spec.ts".to_string()), "spec file should be excluded");
}

#[test]
fn include_single_star_non_recursive() {
    // `src/*.ts` should match only direct children of src, not nested files
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/nested")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("src/nested/b.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src/*.ts"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()), "direct child should be included");
    assert!(!names.contains(&"b.ts".to_string()), "nested file should not match src/*.ts");
}

// ─── EXCLUDE GLOB SEMANTICS ───────────────────────────────────────────────────

#[test]
fn exclude_directory_glob() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/features")).unwrap();
    fs::create_dir_all(dir.path().join("src/test")).unwrap();

    fs::write(dir.path().join("src/features/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("src/test/b.ts"), "export const y = 1;").unwrap();

    let files = collect_files(
        dir.path(),
        &to_strings(&["src"]),
        &to_strings(&["src/test"]),
    );
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"b.ts".to_string()), "test dir should be excluded");
}

#[test]
fn exclude_specific_extension_pattern() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("src/a.test.ts"), "export const y = 1;").unwrap();
    fs::write(dir.path().join("src/a.spec.ts"), "export const z = 1;").unwrap();

    let files = collect_files(
        dir.path(),
        &to_strings(&["src"]),
        &to_strings(&["**/*.test.ts", "**/*.spec.ts"]),
    );
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"a.test.ts".to_string()));
    assert!(!names.contains(&"a.spec.ts".to_string()));
}

// ─── MULTIPLE INCLUDE DIRECTORIES ────────────────────────────────────────────

#[test]
fn multiple_include_directories() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("lib")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("lib/b.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src", "lib"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(names.contains(&"b.ts".to_string()));
}

// ─── ALL SUPPORTED EXTENSIONS ────────────────────────────────────────────────

#[test]
fn collects_all_supported_extensions() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    for ext in &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"] {
        fs::write(src.join(format!("file.{ext}")), "export const x = 1;").unwrap();
    }

    let files = collect_files(dir.path(), &to_strings(&["src"]), &[]);
    assert_eq!(files.len(), 8, "all 8 supported extensions should be collected");
}

// ─── GLOB NORMALISATION EDGE CASES ───────────────────────────────────────────

#[test]
fn include_dir_double_star_pattern() {
    // `src/**` should behave the same as `src` — include all files under src
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/nested")).unwrap();
    fs::create_dir_all(dir.path().join("other")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("src/nested/b.ts"), "export const y = 1;").unwrap();
    fs::write(dir.path().join("other/c.ts"), "export const z = 1;").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src/**"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()), "top-level file should be included");
    assert!(names.contains(&"b.ts".to_string()), "nested file should be included");
    assert!(!names.contains(&"c.ts".to_string()), "other/ should not be included");
}

#[test]
fn include_dir_with_trailing_slash() {
    // `src/` should behave the same as `src`
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("other")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("other/b.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src/"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"b.ts".to_string()));
}

#[test]
fn exclude_dir_double_star_pattern() {
    // `src/test/**` should exclude all files under src/test
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/test")).unwrap();
    fs::create_dir_all(dir.path().join("src/main")).unwrap();

    fs::write(dir.path().join("src/test/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("src/main/b.ts"), "export const y = 1;").unwrap();

    let files = collect_files(
        dir.path(),
        &to_strings(&["src"]),
        &to_strings(&["src/test/**"]),
    );
    let names = filenames(&files);

    assert!(names.contains(&"b.ts".to_string()));
    assert!(!names.contains(&"a.ts".to_string()), "test dir should be excluded via src/test/**");
}

// ─── DEFAULT TSCONFIG EXCLUSIONS ─────────────────────────────────────────────

#[test]
fn skips_bower_components() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let bower = dir.path().join("src/bower_components/pkg");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&bower).unwrap();

    fs::write(src.join("a.ts"), "export const x = 1;").unwrap();
    fs::write(bower.join("index.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"index.ts".to_string()), "bower_components should be skipped");
}

#[test]
fn skips_jspm_packages() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let jspm = dir.path().join("src/jspm_packages/pkg");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&jspm).unwrap();

    fs::write(src.join("a.ts"), "export const x = 1;").unwrap();
    fs::write(jspm.join("index.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &to_strings(&["src"]), &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"index.ts".to_string()), "jspm_packages should be skipped");
}

// ─── EMPTY INCLUDE (tsc: walk from project root) ──────────────────────────────

#[test]
fn empty_include_walks_from_root() {
    // When include is empty, collect_files should walk from root and find all files.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("lib")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("lib/b.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &[], &[]);
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()), "src/a.ts should be found with empty include");
    assert!(names.contains(&"b.ts".to_string()), "lib/b.ts should be found with empty include");
}

#[test]
fn empty_include_respects_exclude() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("dist")).unwrap();

    fs::write(dir.path().join("src/a.ts"), "export const x = 1;").unwrap();
    fs::write(dir.path().join("dist/b.ts"), "export const y = 1;").unwrap();

    let files = collect_files(dir.path(), &[], &to_strings(&["dist"]));
    let names = filenames(&files);

    assert!(names.contains(&"a.ts".to_string()));
    assert!(!names.contains(&"b.ts".to_string()), "dist should be excluded");
}
