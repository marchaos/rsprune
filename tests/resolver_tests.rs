use rsprune::resolver::{is_project_local, resolve_specifier, build_resolver};
use std::fs;

// ─── is_project_local ─────────────────────────────────────────────────────────

#[test]
fn relative_dot_is_local() {
    assert!(is_project_local("./foo"));
    assert!(is_project_local("../bar"));
    assert!(is_project_local("."));
}

#[test]
fn absolute_path_is_local() {
    assert!(is_project_local("/some/absolute/path"));
}

#[test]
fn scoped_package_is_considered_local() {
    // @-prefixed may be a tsconfig path alias
    assert!(is_project_local("@myorg/utils"));
    assert!(is_project_local("@scope/pkg"));
}

#[test]
fn bare_module_is_not_local() {
    assert!(!is_project_local("react"));
    assert!(!is_project_local("lodash"));
    assert!(!is_project_local("fs"));
    assert!(!is_project_local("some-package"));
}

// ─── resolve_specifier ────────────────────────────────────────────────────────

#[test]
fn resolves_relative_ts_file() {
    let dir = tempfile::tempdir().unwrap();
    let tsconfig = dir.path().join("tsconfig.json");
    fs::write(&tsconfig, r#"{"include":["src"]}"#).unwrap();

    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.ts"), "export const x = 1;").unwrap();
    fs::write(src.join("b.ts"), "import { x } from './a';").unwrap();

    let resolver = build_resolver(&tsconfig);
    let result = resolve_specifier(&resolver, &src, "./a");
    assert!(result.is_some(), "should resolve ./a to a.ts");
    assert!(result.unwrap().ends_with("a.ts"));
}

#[test]
fn does_not_resolve_node_modules() {
    let dir = tempfile::tempdir().unwrap();
    let tsconfig = dir.path().join("tsconfig.json");
    fs::write(&tsconfig, r#"{"include":["src"]}"#).unwrap();

    let resolver = build_resolver(&tsconfig);
    // "react" resolves to node_modules — should be filtered out
    let result = resolve_specifier(&resolver, dir.path(), "react");
    assert!(result.is_none(), "node_modules resolution should return None");
}

#[test]
fn returns_none_for_unresolvable_specifier() {
    let dir = tempfile::tempdir().unwrap();
    let tsconfig = dir.path().join("tsconfig.json");
    fs::write(&tsconfig, r#"{"include":["src"]}"#).unwrap();

    let resolver = build_resolver(&tsconfig);
    let result = resolve_specifier(&resolver, dir.path(), "./does-not-exist");
    assert!(result.is_none());
}
