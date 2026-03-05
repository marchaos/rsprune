use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{walk, Visit};
use oxc_parser::{ParseOptions, Parser};
use oxc_span::SourceType;

#[derive(Debug, Clone)]
pub struct ExportInfo {
    pub name: String,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The raw module specifier string (e.g. `../foo`, `@rhino/bar`)
    pub specifier: String,
    /// Which export names are used from the target module.
    /// - Named: the exported name (not local alias)
    /// - Default import → `"default"`
    /// - Namespace import (`* as X`) → `"*"`
    /// Empty means bare side-effect import.
    pub names: Vec<String>,
    /// True for `export * as ns from '...'` — all exports of the target are used.
    pub is_namespace: bool,
}

#[derive(Debug, Default)]
pub struct FileAnalysis {
    pub exports: Vec<ExportInfo>,
    /// Static + dynamic imports
    pub imports: Vec<ImportInfo>,
    /// Re-exports (`export { X } from '...'`, `export * from '...'`)
    pub re_exports: Vec<ImportInfo>,
}

pub fn analyze_file(path: &Path, source: &str) -> FileAnalysis {
    let allocator = Allocator::default();
    let source_type = source_type_for(path);
    let parse_opts = ParseOptions {
        parse_regular_expression: false,
        ..Default::default()
    };
    let ret = Parser::new(&allocator, source, source_type)
        .with_options(parse_opts)
        .parse();

    let line_starts = compute_line_starts(source);
    let mut collector = AstCollector {
        analysis: FileAnalysis::default(),
        ambient_module_depth: 0,
        line_starts,
    };
    collector.visit_program(&ret.program);
    collector.analysis
}

/// Precompute the byte offset of each line's start (line_starts[0] = 0 for line 1).
fn compute_line_starts(source: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (i, &b) in source.as_bytes().iter().enumerate() {
        if b == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    starts
}

/// O(log n) line/col lookup using precomputed line starts.
fn line_col_from_starts(line_starts: &[u32], offset: u32) -> (u32, u32) {
    // partition_point returns the number of starts ≤ offset, which equals the 1-indexed line number.
    let line = line_starts.partition_point(|&s| s <= offset) as u32;
    let line_start = line_starts[(line - 1) as usize];
    (line, offset - line_start)
}

fn source_type_for(path: &Path) -> SourceType {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        "ts" | "mts" | "cts" => SourceType::ts(),
        _ => SourceType::mjs(),
    }
}

struct AstCollector {
    analysis: FileAnalysis,
    /// Depth inside `declare module '...'` blocks (ambient module augmentation).
    /// Exports here extend external modules and should not be tracked as our exports.
    ambient_module_depth: u32,
    /// Precomputed byte offset of each line's start; used for O(log n) line/col lookup.
    line_starts: Vec<u32>,
}

impl<'a> Visit<'a> for AstCollector {
    // --- AMBIENT MODULE AUGMENTATION TRACKING ---

    fn visit_ts_module_declaration(&mut self, decl: &TSModuleDeclaration<'a>) {
        // `declare module 'foo' { ... }` augments an external module.
        // `namespace Foo { ... }` / `declare namespace Foo { ... }` members are
        // accessed via `Foo.Member` syntax, not via direct imports.
        // In both cases, nested exports should not be tracked as our file's own exports.
        self.ambient_module_depth += 1;
        walk::walk_ts_module_declaration(self, decl);
        self.ambient_module_depth -= 1;
    }

    // --- EXPORTS ---

    fn visit_export_named_declaration(&mut self, decl: &ExportNamedDeclaration<'a>) {
        if self.ambient_module_depth > 0 {
            // Inside `declare module '...'` — skip, these are not our exports
            walk::walk_export_named_declaration(self, decl);
            return;
        }
        if let Some(src) = &decl.source {
            // export { X } from '...'
            let specifier = src.value.to_string();
            let names: Vec<String> = decl
                .specifiers
                .iter()
                .map(|s| match &s.local {
                    ModuleExportName::IdentifierReference(id) => id.name.to_string(),
                    ModuleExportName::IdentifierName(id) => id.name.to_string(),
                    ModuleExportName::StringLiteral(s) => s.value.to_string(),
                })
                .collect();
            self.analysis.re_exports.push(ImportInfo { specifier, names, is_namespace: false });
        } else {
            // export const/function/class/type/enum ...
            if let Some(decl_inner) = &decl.declaration {
                self.collect_declaration_exports(decl_inner);
            }
            // export { X, Y }
            for spec in &decl.specifiers {
                let name = match &spec.exported {
                    ModuleExportName::IdentifierReference(id) => id.name.to_string(),
                    ModuleExportName::IdentifierName(id) => id.name.to_string(),
                    ModuleExportName::StringLiteral(s) => s.value.to_string(),
                };
                let (line, col) = line_col_from_starts(&self.line_starts,spec.span.start);
                self.analysis.exports.push(ExportInfo { name, line, col });
            }
        }
        // Walk child nodes so function bodies / arrow functions get visited
        walk::walk_export_named_declaration(self, decl);
    }

    fn visit_export_default_declaration(&mut self, decl: &ExportDefaultDeclaration<'a>) {
        if self.ambient_module_depth > 0 {
            walk::walk_export_default_declaration(self, decl);
            return;
        }
        let (line, col) = line_col_from_starts(&self.line_starts,decl.span.start);
        self.analysis.exports.push(ExportInfo {
            name: "default".to_string(),
            line,
            col,
        });
        walk::walk_export_default_declaration(self, decl);
    }

    fn visit_export_all_declaration(&mut self, decl: &ExportAllDeclaration<'a>) {
        if self.ambient_module_depth > 0 {
            walk::walk_export_all_declaration(self, decl);
            return;
        }
        let specifier = decl.source.value.to_string();
        let is_namespace = decl.exported.is_some(); // export * as ns from '...'
        let name = decl.exported.as_ref().map(|n| match n {
            ModuleExportName::IdentifierReference(id) => id.name.to_string(),
            ModuleExportName::IdentifierName(id) => id.name.to_string(),
            ModuleExportName::StringLiteral(s) => s.value.to_string(),
        });
        self.analysis.re_exports.push(ImportInfo {
            specifier,
            names: name.into_iter().collect(),
            is_namespace,
        });
        walk::walk_export_all_declaration(self, decl);
    }

    // --- STATIC IMPORTS ---

    fn visit_import_declaration(&mut self, decl: &ImportDeclaration<'a>) {
        let specifier = decl.source.value.to_string();
        let names: Vec<String> = decl
            .specifiers
            .iter()
            .flatten()
            .map(|s| match s {
                ImportDeclarationSpecifier::ImportSpecifier(spec) => {
                    // import { foo as bar } → track the export name "foo"
                    match &spec.imported {
                        ModuleExportName::IdentifierReference(id) => id.name.to_string(),
                        ModuleExportName::IdentifierName(id) => id.name.to_string(),
                        ModuleExportName::StringLiteral(s) => s.value.to_string(),
                    }
                }
                // import Foo from '...' → uses "default" export
                ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => "default".to_string(),
                // import * as Foo → uses all exports
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => "*".to_string(),
            })
            .collect();
        self.analysis.imports.push(ImportInfo { specifier, names, is_namespace: false });
        // Note: import declarations have no child expressions to walk
    }

    // --- DYNAMIC IMPORTS ---

    fn visit_import_expression(&mut self, expr: &ImportExpression<'a>) {
        // import('./foo') or import('./foo').then(m => m.bar)
        if let Expression::StringLiteral(lit) = &expr.source {
            self.analysis.imports.push(ImportInfo {
                specifier: lit.value.to_string(),
                // dynamic import — we don't know which names are used statically,
                // so mark as namespace (all used)
                names: vec!["*".to_string()],
                is_namespace: false,
            });
        }
        // Also handle template literals with no expressions: import(`./foo`)
        if let Expression::TemplateLiteral(tpl) = &expr.source {
            if tpl.expressions.is_empty() {
                if let Some(quasi) = tpl.quasis.first() {
                    self.analysis.imports.push(ImportInfo {
                        specifier: quasi.value.raw.to_string(),
                        names: vec!["*".to_string()],
                        is_namespace: false,
                    });
                }
            }
        }
        // Continue walking into the expression (for chained .then etc.)
        oxc_ast_visit::walk::walk_import_expression(self, expr);
    }
}

impl AstCollector {
    fn collect_declaration_exports(&mut self, decl: &Declaration) {
        match decl {
            Declaration::VariableDeclaration(var) => {
                for declarator in &var.declarations {
                    self.collect_binding_pattern_exports(&declarator.id);
                }
            }
            Declaration::FunctionDeclaration(func) => {
                if let Some(id) = &func.id {
                    let (line, col) = line_col_from_starts(&self.line_starts,func.span.start);
                    self.analysis.exports.push(ExportInfo {
                        name: id.name.to_string(),
                        line,
                        col,
                    });
                }
            }
            Declaration::ClassDeclaration(cls) => {
                if let Some(id) = &cls.id {
                    let (line, col) = line_col_from_starts(&self.line_starts,cls.span.start);
                    self.analysis.exports.push(ExportInfo {
                        name: id.name.to_string(),
                        line,
                        col,
                    });
                }
            }
            Declaration::TSTypeAliasDeclaration(ts) => {
                let (line, col) = line_col_from_starts(&self.line_starts,ts.span.start);
                self.analysis.exports.push(ExportInfo {
                    name: ts.id.name.to_string(),
                    line,
                    col,
                });
            }
            Declaration::TSInterfaceDeclaration(ts) => {
                let (line, col) = line_col_from_starts(&self.line_starts,ts.span.start);
                self.analysis.exports.push(ExportInfo {
                    name: ts.id.name.to_string(),
                    line,
                    col,
                });
            }
            Declaration::TSEnumDeclaration(ts) => {
                let (line, col) = line_col_from_starts(&self.line_starts,ts.span.start);
                self.analysis.exports.push(ExportInfo {
                    name: ts.id.name.to_string(),
                    line,
                    col,
                });
            }
            Declaration::TSModuleDeclaration(ts) => {
                let (line, col) = line_col_from_starts(&self.line_starts,ts.span.start);
                let name = match &ts.id {
                    TSModuleDeclarationName::Identifier(id) => id.name.to_string(),
                    TSModuleDeclarationName::StringLiteral(s) => s.value.to_string(),
                };
                self.analysis.exports.push(ExportInfo { name, line, col });
            }
            _ => {}
        }
    }

    fn collect_binding_pattern_exports(&mut self, pat: &BindingPattern) {
        match pat {
            BindingPattern::BindingIdentifier(id) => {
                let (line, col) = line_col_from_starts(&self.line_starts,id.span.start);
                self.analysis.exports.push(ExportInfo {
                    name: id.name.to_string(),
                    line,
                    col,
                });
            }
            BindingPattern::ObjectPattern(obj) => {
                for prop in &obj.properties {
                    self.collect_binding_pattern_exports(&prop.value);
                }
            }
            BindingPattern::ArrayPattern(arr) => {
                for elem in arr.elements.iter().flatten() {
                    self.collect_binding_pattern_exports(elem);
                }
            }
            BindingPattern::AssignmentPattern(assign) => {
                self.collect_binding_pattern_exports(&assign.left);
            }
        }
    }
}

/// Public helper used in tests. For file-level analysis the precomputed path in
/// `analyze_file` is used instead.
pub fn offset_to_line_col(source: &str, offset: usize) -> (u32, u32) {
    let starts = compute_line_starts(source);
    let offset = (offset.min(source.len())) as u32;
    line_col_from_starts(&starts, offset)
}

/// Returns true if the line immediately preceding `line_number` (1-indexed) contains
/// a suppression comment. Uses an iterator to avoid allocating a Vec<&str>.
pub fn is_suppressed(source: &str, line_number: u32) -> bool {
    if line_number <= 1 {
        return false;
    }
    let prev_line_idx = (line_number - 2) as usize;
    if let Some(prev) = source.split('\n').nth(prev_line_idx) {
        let trimmed = prev.trim();
        trimmed.contains("ts-unused-exports:disable-next-line")
            || trimmed.contains("rsprune:disable-next-line")
    } else {
        false
    }
}
