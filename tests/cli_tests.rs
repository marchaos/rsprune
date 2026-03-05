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
    assert!(stdout.contains("[0,"), "expected [line,col] format, got: {stdout}");
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
