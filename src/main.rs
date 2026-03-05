use rsprune::{files, parser, resolver, tsconfig};

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
    /// Path to tsconfig.json
    #[arg(default_value = "tsconfig.json")]
    tsconfig: PathBuf,

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

    let tsconfig_path = args
        .tsconfig
        .canonicalize()
        .unwrap_or_else(|_| args.tsconfig.clone());

    let config = phase!(args.timing, "tsconfig parse", {
        tsconfig::TsConfig::load(&tsconfig_path)?
    });
    let root = config.root_dir(&tsconfig_path);

    let include = config
        .include
        .as_deref()
        .unwrap_or(&["src".to_string()][..])
        .to_vec();
    let exclude = config.exclude.as_deref().unwrap_or_default().to_vec();

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

    let record = |resolved: PathBuf, names: &[String]| {
        let mut entry = used_exports.entry(resolved).or_default();
        if names.is_empty() {
            entry.insert("__sideeffect__".to_string());
        } else {
            for name in names {
                entry.insert(name.clone());
            }
        }
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
                record(resolved, &import.names);
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
                let mut entry = used_exports.entry(resolved).or_default();
                if re_export.names.is_empty() {
                    entry.insert("*".to_string());
                } else {
                    for name in &re_export.names {
                        entry.insert(name.clone());
                    }
                }
            }
        });
    });

    // Find unused exports
    let mut unused: Vec<(PathBuf, Vec<parser::ExportInfo>)> = phase!(args.timing, "find unused", {
        let mut unused = Vec::new();
        for (path, analysis, source) in &analyses {
            if analysis.exports.is_empty() {
                continue;
            }
            let used = used_exports.get(path.as_path());
            let mut unused_in_file: Vec<parser::ExportInfo> = Vec::new();

            for export in &analysis.exports {
                let is_used = match &used {
                    None => false,
                    Some(set) => {
                        set.contains("*")
                            || set.contains(&export.name)
                            || (export.name == "default" && set.contains("default"))
                    }
                };
                if !is_used && !parser::is_suppressed(source, export.line) {
                    unused_in_file.push(export.clone());
                }
            }

            if !unused_in_file.is_empty() {
                unused.push((path.clone(), unused_in_file));
            }
        }
        unused
    });

    // Sort by path for deterministic output
    unused.sort_by(|a, b| a.0.cmp(&b.0));

    if args.timing {
        eprintln!("[timing] {:30} {:>8.1}ms  (TOTAL)", "wall time", t_total.elapsed().as_secs_f64() * 1000.0);
    }

    let module_count = unused.len();

    if module_count == 0 {
        println!("0 modules with unused exports");
        return Ok(());
    }

    println!("{module_count} modules with unused exports");

    for (path, exports) in &unused {
        let path_str = path.to_string_lossy();

        if args
            .exclude_paths_from_report
            .iter()
            .any(|ex| path_str.contains(ex.as_str()))
        {
            continue;
        }

        for export in exports {
            if args.show_line_number {
                println!(
                    "{path_str}[{},{}]: {}",
                    export.line, export.col, export.name
                );
            } else {
                println!("{path_str}: {}", export.name);
            }
        }
    }

    std::process::exit(1);
}
