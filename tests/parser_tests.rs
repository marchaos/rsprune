use std::path::Path;

use rsprune::parser::{analyze_file, is_suppressed, ExportInfo};

fn ts(src: &str) -> Vec<ExportInfo> {
    let path = Path::new("test.ts");
    analyze_file(path, src).exports
}

fn tsx(src: &str) -> Vec<ExportInfo> {
    let path = Path::new("test.tsx");
    analyze_file(path, src).exports
}

fn imports(src: &str) -> Vec<(String, Vec<String>)> {
    let path = Path::new("test.ts");
    let analysis = analyze_file(path, src);
    analysis
        .imports
        .into_iter()
        .map(|i| (i.specifier, i.names))
        .collect()
}

fn re_exports(src: &str) -> Vec<(String, Vec<String>)> {
    let path = Path::new("test.ts");
    let analysis = analyze_file(path, src);
    analysis
        .re_exports
        .into_iter()
        .map(|i| (i.specifier, i.names))
        .collect()
}

// ─── EXPORT DETECTION ────────────────────────────────────────────────────────

#[test]
fn detects_named_function_export() {
    let exports = ts("export function foo() {}");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "foo");
}

#[test]
fn detects_named_const_export() {
    let exports = ts("export const bar = 42;");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "bar");
}

#[test]
fn detects_default_export() {
    let exports = ts("export default function() {}");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "default");
}

#[test]
fn detects_default_class_export() {
    let exports = tsx("export default class Foo {}");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "default");
}

#[test]
fn detects_multiple_named_exports() {
    let exports = ts("export const a = 1;\nexport const b = 2;");
    assert_eq!(exports.len(), 2);
    let names: Vec<_> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
}

#[test]
fn detects_export_list() {
    let exports = ts("const x = 1; const y = 2; export { x, y };");
    assert_eq!(exports.len(), 2);
    let names: Vec<_> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"x"));
    assert!(names.contains(&"y"));
}

#[test]
fn detects_type_alias_export() {
    let exports = ts("export type Foo = string;");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "Foo");
}

#[test]
fn detects_interface_export() {
    let exports = ts("export interface Bar { x: number; }");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "Bar");
}

#[test]
fn detects_enum_export() {
    let exports = ts("export enum Color { Red, Green, Blue }");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "Color");
}

#[test]
fn detects_class_export() {
    let exports = ts("export class MyClass {}");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "MyClass");
}

#[test]
fn detects_export_renamed() {
    let exports = ts("const x = 1; export { x as renamed };");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "renamed");
}

// ─── IMPORT DETECTION ────────────────────────────────────────────────────────

#[test]
fn detects_named_import() {
    let imps = imports("import { foo } from './bar';");
    assert_eq!(imps.len(), 1);
    assert_eq!(imps[0].0, "./bar");
    assert_eq!(imps[0].1, vec!["foo"]);
}

#[test]
fn detects_default_import() {
    let imps = imports("import Foo from './foo';");
    assert_eq!(imps.len(), 1);
    assert_eq!(imps[0].1, vec!["default"]);
}

#[test]
fn detects_namespace_import() {
    let imps = imports("import * as Foo from './foo';");
    assert_eq!(imps.len(), 1);
    assert_eq!(imps[0].1, vec!["*"]);
}

#[test]
fn detects_side_effect_import() {
    let imps = imports("import './side-effect';");
    assert_eq!(imps.len(), 1);
    assert_eq!(imps[0].0, "./side-effect");
    assert!(imps[0].1.is_empty());
}

#[test]
fn detects_aliased_named_import() {
    // import { foo as bar } → should track the export name "foo", not "bar"
    let imps = imports("import { foo as bar } from './mod';");
    assert_eq!(imps[0].1, vec!["foo"]);
}

#[test]
fn detects_dynamic_import() {
    let imps = imports("const x = await import('./dynamic');");
    assert_eq!(imps.len(), 1);
    assert_eq!(imps[0].0, "./dynamic");
    assert_eq!(imps[0].1, vec!["*"]);
}

#[test]
fn detects_dynamic_import_inside_function() {
    let imps = imports("export async function load() { return (await import('./mod')).default; }");
    assert_eq!(imps.len(), 1);
    assert_eq!(imps[0].0, "./mod");
    assert_eq!(imps[0].1, vec!["*"]);
}

// ─── RE-EXPORT DETECTION ─────────────────────────────────────────────────────

#[test]
fn detects_named_re_export() {
    let re = re_exports("export { foo } from './other';");
    assert_eq!(re.len(), 1);
    assert_eq!(re[0].0, "./other");
    assert_eq!(re[0].1, vec!["foo"]);
}

#[test]
fn detects_star_re_export() {
    let re = re_exports("export * from './other';");
    assert_eq!(re.len(), 1);
    assert_eq!(re[0].0, "./other");
    assert!(re[0].1.is_empty(), "star re-export should have empty names (means all)");
}

#[test]
fn detects_star_as_re_export() {
    let re = re_exports("export * as ns from './other';");
    assert_eq!(re.len(), 1);
    assert_eq!(re[0].0, "./other");
    assert_eq!(re[0].1, vec!["ns"]);
}

// ─── LINE/COL NUMBERS ────────────────────────────────────────────────────────

#[test]
fn line_number_is_one_indexed() {
    let exports = ts("export const x = 1;");
    assert_eq!(exports[0].line, 1);
}

#[test]
fn line_number_second_line() {
    let exports = ts("\nexport const x = 1;");
    assert_eq!(exports[0].line, 2);
}

#[test]
fn no_false_positives_for_unexported_const() {
    let exports = ts("const x = 1;");
    assert!(exports.is_empty());
}

#[test]
fn handles_tsx_jsx_export() {
    let exports = tsx("export const MyComp = () => <div />;");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "MyComp");
}

// ─── SUPPRESSION COMMENTS ────────────────────────────────────────────────────

#[test]
fn suppression_comment_detected_on_preceding_line() {
    let src = "const x = 1;\n// ts-unused-exports:disable-next-line\nexport const foo = 1;";
    assert!(is_suppressed(src, 3)); // line 3 (1-indexed) has the export
}

#[test]
fn no_suppression_without_comment() {
    let src = "export const foo = 1;";
    assert!(!is_suppressed(src, 1));
}

#[test]
fn suppression_on_wrong_line_does_not_match() {
    let src = "// ts-unused-exports:disable-next-line\nconst x = 1;\nexport const foo = 1;";
    // comment is on line 1, export is on line 3 — not adjacent
    assert!(!is_suppressed(src, 3));
}
