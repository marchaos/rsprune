/// CLI tests that run the rsprune binary and verify exit codes and output.
///
/// Uses `env!("CARGO_BIN_EXE_rsprune")` to locate the compiled binary,
/// so these tests require the binary to be built first (cargo test does this).

use std::fs;
use std::process::Command;

/// Path to the compiled rsprune binary, provided by Cargo at test compile time.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_rsprune")
}

/// Create a minimal tsconfig.json in `dir` that includes the given subdirectory.
fn write_tsconfig(dir: &std::path::Path, include: &str) {
    fs::write(
        dir.join("tsconfig.json"),
        format!(r#"{{"include": ["{include}"]}}"#),
    )
    .unwrap();
}

// ─── EXIT CODE: CLEAN PROJECT ────────────────────────────────────────────────

#[test]
fn exit_code_zero_when_no_unused_exports() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    fs::write(src.join("b.ts"), "import { foo } from './a';").unwrap();

    write_tsconfig(dir.path(), "src");

    let status = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .status()
        .unwrap();

    assert!(status.success(), "expected exit 0 when no unused exports");
}

// ─── EXIT CODE: UNUSED EXPORTS FOUND ─────────────────────────────────────────

#[test]
fn exit_code_one_when_unused_exports_found() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1; export const bar = 2;").unwrap();
    fs::write(src.join("b.ts"), "import { foo } from './a';").unwrap();

    write_tsconfig(dir.path(), "src");

    let status = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .status()
        .unwrap();

    assert_eq!(status.code(), Some(1), "expected exit 1 when unused exports found");
}

// ─── OUTPUT FORMAT ────────────────────────────────────────────────────────────

#[test]
fn output_summary_line_when_clean() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    fs::write(src.join("b.ts"), "import { foo } from './a';").unwrap();
    write_tsconfig(dir.path(), "src");

    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("0 modules with unused exports"), "got: {stdout}");
}

#[test]
fn output_summary_line_when_unused_found() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1; export const bar = 2;").unwrap();
    fs::write(src.join("b.ts"), "import { foo } from './a';").unwrap();
    write_tsconfig(dir.path(), "src");

    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("modules with unused exports"), "got: {stdout}");
    assert!(stdout.contains("bar"), "expected 'bar' in output, got: {stdout}");
}

#[test]
fn output_includes_line_number_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    // b.ts doesn't import a at all
    fs::write(src.join("b.ts"), "const x = 2;").unwrap();
    write_tsconfig(dir.path(), "src");

    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should include [line,col] format
    assert!(stdout.contains("[1,"), "expected [line,col] format, got: {stdout}");
}

// ─── SUPPRESSION COMMENT ──────────────────────────────────────────────────────

#[test]
fn suppressed_export_not_reported() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(
        src.join("a.ts"),
        "// ts-unused-exports:disable-next-line\nexport const foo = 1;",
    )
    .unwrap();
    fs::write(src.join("b.ts"), "const x = 2;").unwrap();
    write_tsconfig(dir.path(), "src");

    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("foo"), "suppressed export should not appear: {stdout}");
    assert!(out.status.success(), "expected exit 0 when only suppressed exports");
}

#[test]
fn rsprune_disable_next_line_suppresses_export() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "// rsprune:disable-next-line\nexport const foo = 1;").unwrap();
    fs::write(src.join("b.ts"), "const x = 2;").unwrap();
    write_tsconfig(dir.path(), "src");

    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("foo"), "rsprune-suppressed export should not appear: {stdout}");
    assert!(out.status.success());
}

// ─── --ignore-files ───────────────────────────────────────────────────────────

#[test]
fn ignore_files_skips_matching_files() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    fs::write(src.join("a.spec.ts"), "export const bar = 2;").unwrap();
    write_tsconfig(dir.path(), "src");

    // Without --ignore-files both are flagged
    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("foo") && stdout.contains("bar"), "both should appear without filter");

    // With --ignore-files the spec file is skipped entirely
    let out = Command::new(bin())
        .args(["tsconfig.json", "--ignore-files", r"\.spec\."])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("foo"), "a.ts export should still appear");
    assert!(!stdout.contains("bar"), "spec file export should be ignored: {stdout}");
}

#[test]
fn ignore_files_all_results_in_clean() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    write_tsconfig(dir.path(), "src");

    let out = Command::new(bin())
        .args(["tsconfig.json", "--ignore-files", "a\\.ts"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(out.status.success(), "expected exit 0 when all files ignored");
}

// ─── --exclude-paths-from-report ─────────────────────────────────────────────

#[test]
fn exclude_paths_from_report_hides_output() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let internal = src.join("internal");
    fs::create_dir_all(&internal).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    fs::write(internal.join("b.ts"), "export const bar = 2;").unwrap();
    write_tsconfig(dir.path(), "src");

    let out = Command::new(bin())
        .args(["tsconfig.json", "--exclude-paths-from-report", "internal"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("foo"), "a.ts export should appear");
    assert!(!stdout.contains("bar"), "internal export should be excluded from report: {stdout}");
}

#[test]
fn exclude_paths_from_report_affects_module_count() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    fs::write(src.join("b.ts"), "export const bar = 2;").unwrap();
    write_tsconfig(dir.path(), "src");

    // Without exclude: 2 modules
    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("2 modules"), "got: {stdout}");

    // Exclude b.ts: 1 module
    let out = Command::new(bin())
        .args(["tsconfig.json", "--exclude-paths-from-report", "b.ts"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1 modules"), "got: {stdout}");
}

// ─── outDir AUTO-EXCLUSION ────────────────────────────────────────────────────

#[test]
fn outdir_is_excluded_automatically() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dist = dir.path().join("dist");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&dist).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    // Simulated compiled output — should not be analysed
    fs::write(dist.join("a.ts"), "export const foo = 1;").unwrap();

    // tsconfig with outDir set
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"include":["src"],"compilerOptions":{"outDir":"dist"}}"#,
    )
    .unwrap();

    // foo is unused (no other file imports it), but dist/a.ts should not be
    // picked up and cause a false "foo is used" result.
    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    // Only src/a.ts is analysed; dist/ is excluded by outDir
    assert!(stdout.contains("foo"), "foo in src should be reported");
    assert_eq!(out.status.code(), Some(1));
}

// ─── EMPTY INCLUDE (walk from root) ──────────────────────────────────────────

#[test]
fn no_include_walks_from_root() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let lib = dir.path().join("lib");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&lib).unwrap();

    fs::write(src.join("a.ts"), "export const foo = 1;").unwrap();
    fs::write(lib.join("b.ts"), "import { foo } from '../src/a';").unwrap();

    // tsconfig with no "include" key — should walk from root and find both files
    fs::write(dir.path().join("tsconfig.json"), r#"{}"#).unwrap();

    let out = Command::new(bin())
        .arg("tsconfig.json")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    // lib/b.ts imports foo, so foo should be considered used → clean run
    assert!(out.status.success(), "expected exit 0, got: {stdout}");
}
