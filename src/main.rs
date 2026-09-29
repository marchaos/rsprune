use rsprune::{contexts, files, parser, resolver, tsconfig};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use dashmap::DashMap;
use rayon::prelude::*;

use crate::parser::FileAnalysis;

#[derive(Parser, Debug)]
#[command(name = "rsprune", about = "Find unused TypeScript exports (fast Rust reimplementation)")]
struct Args {
    /// Path to tsconfig.json (optional — walks up from cwd if omitted)
    tsconfig: Option<PathBuf>,

    /// Show line numbers in output (like ts-unused-exports --showLineNumber)
    #[arg(long, default_value_t = true)]
    show_line_number: bool,

    /// Regex patterns of file paths to ignore entirely (like --ignoreFiles)
    #[arg(long)]
    ignore_files: Vec<String>,

    /// Exclude these path segments from the report output (like --excludePathsFromReport)
    #[arg(long)]
    exclude_paths_from_report: Vec<String>,

    /// Print per-phase timing breakdown to stderr
    #[arg(long)]
    timing: bool,

    /// Treat files loaded by `import.meta.webpackContext(...)` or `require.context(...)` as fully used
    #[arg(long)]
    bundler_contexts: bool,

    /// Regex patterns of test file paths. Also reports exports of other files that are only used by
    /// these files and not referenced inside their own module.
    #[arg(long)]
    test_files: Vec<String>,
}

macro_rules! phase {
    ($timing:expr, $label:expr, $block:expr) => {{
        let t = Instant::now();
        let result = $block;
        if $timing {
            eprintln!("[timing] {:30} {:>8.1}ms", $label, t.elapsed().as_secs_f64() * 1000.0);
        }
        result
    }};
}

fn main() -> Result<()> {
    let t_total = Instant::now();
    let args = Args::parse();

    let tsconfig_path = match args.tsconfig {
        Some(p) => p.canonicalize().unwrap_or(p),
        None => find_tsconfig()
            .ok_or_else(|| anyhow::anyhow!("No tsconfig.json found in current directory or any parent directory"))?,
    };

    let config = phase!(args.timing, "tsconfig parse", {
        tsconfig::TsConfig::load(&tsconfig_path)?
    });
    let root = config.root_dir(&tsconfig_path);

    // When include is absent tsc walks the entire project root.
    let include = config.include.as_deref().unwrap_or(&[]).to_vec();

    // Start with explicit excludes, then append tsc's automatic outDir exclusion.
    let mut exclude = config.exclude.as_deref().unwrap_or_default().to_vec();
    if let Some(opts) = &config.compiler_options {
        if let Some(out_dir) = &opts.out_dir {
            exclude.push(out_dir.clone());
        }
    }

    let test_patterns: Vec<regex::Regex> = args
        .test_files
        .iter()
        .filter_map(|p| regex::Regex::new(p).ok())
        .collect();
    let is_test_file = |path: &Path| {
        let path_str = path.to_string_lossy();
        test_patterns.iter().any(|pattern| pattern.is_match(&path_str))
    };

    let ignore_patterns: Vec<regex::Regex> = args
        .ignore_files
        .iter()
        .filter_map(|p| regex::Regex::new(p).ok())
        .collect();

    // Walk + read + parse in a single parallel streaming pass
    let analyses: Vec<(PathBuf, FileAnalysis, String)> =
        phase!(args.timing, "walk+parse (parallel)", {
            files::walk_and_parse(&root, &include, &exclude, &ignore_patterns)
        });

    if args.timing {
        let total_imports: usize = analyses.iter().map(|(_, a, _)| a.imports.len() + a.re_exports.len()).sum();
        let total_exports: usize = analyses.iter().map(|(_, a, _)| a.exports.len()).sum();
        eprintln!("[timing] {:30} {:>8} files, {} imports, {} exports",
            "totals", analyses.len(), total_imports, total_exports);
    }

    // Build resolver
    let resolver = phase!(args.timing, "build resolver", {
        resolver::build_resolver(&tsconfig_path)
    });

    // Map: path -> set of exported names used by other files.
    // DashMap allows parallel writes from rayon threads.
    let used_exports: DashMap<PathBuf, HashSet<String>> = DashMap::new();
    // The same, counting only usages from files that don't match --test-files.
    let used_outside_tests: DashMap<PathBuf, HashSet<String>> = DashMap::new();

    let record = |resolved: PathBuf, names: &[String], from_path: &Path| {
        let names = if names.is_empty() { vec!["__sideeffect__".to_string()] } else { names.to_vec() };
        if !is_test_file(from_path) {
            used_outside_tests.entry(resolved.clone()).or_default().extend(names.iter().cloned());
        }
        used_exports.entry(resolved).or_default().extend(names);
    };

    // Resolve imports in parallel — Resolver is Sync, DashMap allows concurrent inserts
    phase!(args.timing, "resolve imports (parallel)", {
        analyses.par_iter().for_each(|(from_path, analysis, _source)| {
            let from_dir = from_path.parent().unwrap_or(Path::new("/"));

            for import in &analysis.imports {
                // Skip bare node_module imports early (no filesystem call needed)
                if !resolver::is_project_local(&import.specifier) {
                    continue;
                }
                let Some(resolved) =
                    resolver::resolve_specifier(&resolver, from_dir, &import.specifier)
                else {
                    continue;
                };
                record(resolved, &import.names, from_path);
            }

            for re_export in &analysis.re_exports {
                if !resolver::is_project_local(&re_export.specifier) {
                    continue;
                }
                let Some(resolved) =
                    resolver::resolve_specifier(&resolver, from_dir, &re_export.specifier)
                else {
                    continue;
                };
                if re_export.names.is_empty() || re_export.is_namespace {
                    // export * from '...' or export * as ns from '...' — all exports used
                    record(resolved, &["*".to_string()], from_path);
                } else {
                    record(resolved, &re_export.names, from_path);
                }
            }
        });
    });

    if args.bundler_contexts {
        phase!(args.timing, "resolve bundler contexts", {
            let all_names = ["*".to_string()];
            for (from_path, analysis, _source) in &analyses {
                for context in &analysis.contexts {
                    let candidates = analyses.iter().map(|(path, _, _)| path.as_path());
                    match contexts::matching_files(context, from_path, candidates) {
                        Ok(matched) => matched.into_iter().for_each(|path| record(path, &all_names, from_path)),
                        Err(error) => eprintln!(
                            "rsprune: skipping bundler context in {}: {error}",
                            from_path.display()
                        ),
                    }
                }
            }
        });
    }

    let is_used = |usages: &DashMap<PathBuf, HashSet<String>>, path: &Path, export: &parser::ExportInfo| {
        usages
            .get(path)
            .is_some_and(|set| set.contains("*") || set.contains(&export.name))
    };

    // Find unused exports, and (with --test-files) exports only used by tests
    let (mut unused, mut test_only) = phase!(args.timing, "find unused", {
        let mut unused: Vec<(PathBuf, Vec<parser::ExportInfo>)> = Vec::new();
        let mut test_only: Vec<(PathBuf, Vec<parser::ExportInfo>)> = Vec::new();
        for (path, analysis, source) in &analyses {
            if analysis.exports.is_empty() {
                continue;
            }
            let check_test_only = !test_patterns.is_empty() && !is_test_file(path);
            let mut unused_in_file: Vec<parser::ExportInfo> = Vec::new();
            let mut test_only_in_file: Vec<parser::ExportInfo> = Vec::new();

            for export in &analysis.exports {
                if parser::is_suppressed(source, export.line) {
                    continue;
                }
                if !is_used(&used_exports, path, export) {
                    unused_in_file.push(export.clone());
                } else if check_test_only
                    && !analysis.is_used_in_module(export)
                    // Exports of the same binding (e.g. `export const Foo` and `export default Foo`) are aliases.
                    && !analysis.exports.iter().any(|alias| {
                        analysis.local_name(alias) == analysis.local_name(export)
                            && is_used(&used_outside_tests, path, alias)
                    })
                {
                    test_only_in_file.push(export.clone());
                }
            }

            if !unused_in_file.is_empty() {
                unused.push((path.clone(), unused_in_file));
            }
            if !test_only_in_file.is_empty() {
                test_only.push((path.clone(), test_only_in_file));
            }
        }
        (unused, test_only)
    });

    // Sort by path for deterministic output
    unused.sort_by(|a, b| a.0.cmp(&b.0));
    test_only.sort_by(|a, b| a.0.cmp(&b.0));

    if args.timing {
        eprintln!("[timing] {:30} {:>8.1}ms  (TOTAL)", "wall time", t_total.elapsed().as_secs_f64() * 1000.0);
    }

    let reported = |modules: Vec<(PathBuf, Vec<parser::ExportInfo>)>| -> Vec<(PathBuf, Vec<parser::ExportInfo>)> {
        modules
            .into_iter()
            .filter(|(path, _)| {
                let path_str = path.to_string_lossy();
                !args
                    .exclude_paths_from_report
                    .iter()
                    .any(|ex| path_str.contains(ex.as_str()))
            })
            .collect()
    };
    let print_exports = |modules: &[(PathBuf, Vec<parser::ExportInfo>)]| {
        for (path, exports) in modules {
            let path_str = path.to_string_lossy();
            for export in exports.iter() {
                if args.show_line_number {
                    println!("{path_str}[{},{}]: {}", export.line, export.col, export.name);
                } else {
                    println!("{path_str}: {}", export.name);
                }
            }
        }
    };

    let unused = reported(unused);
    let test_only = reported(test_only);

    println!("{} modules with unused exports", unused.len());
    print_exports(&unused);

    if !test_patterns.is_empty() {
        println!("{} modules with exports only used by tests", test_only.len());
        print_exports(&test_only);
    }

    if !unused.is_empty() || !test_only.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

/// Walk up from cwd looking for tsconfig.json, mirroring tsc's behaviour.
fn find_tsconfig() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("tsconfig.json");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}
