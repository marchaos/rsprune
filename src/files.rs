use std::path::{Path, PathBuf};
use std::sync::mpsc;

use ignore::WalkBuilder;
use rayon::prelude::*;

use crate::{parser::{self, FileAnalysis}, EXTENSIONS};

/// Walk all include directories and parse each file in parallel.
///
/// Strategy: `ignore::WalkParallel` does directory traversal on its own thread
/// pool (separate from rayon), and sends matching paths over a channel.
/// Rayon's `par_bridge` picks up those paths and reads+parses them in parallel.
/// The two pools run concurrently so I/O and CPU overlap.
pub fn walk_and_parse(
    root: &Path,
    include: &[String],
    exclude: &[String],
    ignore_patterns: &[regex::Regex],
) -> Vec<(PathBuf, FileAnalysis, String)> {
    let (tx, rx) = mpsc::sync_channel::<PathBuf>(256);

    // Build parallel walker across all include directories
    let builder = build_walker(root, include, exclude);

    let exclude = exclude.to_vec();
    let ignore_patterns: Vec<_> = ignore_patterns
        .iter()
        .map(|re| re.as_str().to_owned())
        .collect();

    // Walk on ignore's own thread pool
    std::thread::spawn(move || {
        builder.build_parallel().run(|| {
            let tx = tx.clone();
            let exclude = exclude.clone();
            let ignore_patterns = ignore_patterns.clone();
            Box::new(move |result| {
                use ignore::WalkState;
                let Ok(entry) = result else { return WalkState::Continue };

                // Prune node_modules
                if entry.file_name() == "node_modules" {
                    return WalkState::Skip;
                }

                let ft = match entry.file_type() {
                    Some(ft) => ft,
                    None => return WalkState::Continue,
                };
                if !ft.is_file() {
                    return WalkState::Continue;
                }

                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !EXTENSIONS.contains(&ext) {
                    return WalkState::Continue;
                }

                let path_str = path.to_string_lossy();
                if exclude.iter().any(|ex| path_str.contains(ex.trim_end_matches('/'))) {
                    return WalkState::Continue;
                }
                if ignore_patterns.iter().any(|pat| {
                    regex::Regex::new(pat).map(|re| re.is_match(&path_str)).unwrap_or(false)
                }) {
                    return WalkState::Continue;
                }

                let _ = tx.send(path.to_path_buf());
                WalkState::Continue
            })
        });
        // tx dropped here, closing the channel
    });

    // Rayon par_bridge reads from the channel and parses files in parallel
    rx.into_iter()
        .par_bridge()
        .filter_map(|path| {
            let source = std::fs::read_to_string(&path).ok()?;
            let analysis = parser::analyze_file(&path, &source);
            Some((path, analysis, source))
        })
        .collect()
}

fn build_walker(root: &Path, include: &[String], exclude: &[String]) -> WalkBuilder {
    // Start with first include dir, add rest
    let bases: Vec<PathBuf> = include
        .iter()
        .map(|p| include_base(root, p))
        .collect();

    let mut builder = WalkBuilder::new(&bases[0]);
    for base in &bases[1..] {
        builder.add(base);
    }

    builder
        .follow_links(true)
        .hidden(false)          // don't skip hidden files
        .standard_filters(false) // don't read .gitignore etc
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .ignore(false)
        .threads(num_cpus());

    // Add exclude overrides
    if !exclude.is_empty() {
        // We handle exclude in the visitor callback
    }

    builder
}

/// Extract a walkable base directory from a tsconfig include pattern.
///
/// Supports simple directory patterns (`src/`, `src/**`) which covers the
/// vast majority of real-world tsconfigs. Fine-grained glob patterns like
/// `**/*.test.ts` or negation patterns are not supported — the full directory
/// is walked and file-level filtering is left to --ignore-files.
fn include_base(root: &Path, pattern: &str) -> PathBuf {
    let trimmed = pattern.trim_end_matches('/').trim_end_matches("/**");
    let base = root.join(trimmed);
    if base.is_dir() { base } else { base.parent().unwrap_or(root).to_path_buf() }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Collect file paths only (no parsing), used in tests.
pub fn collect_files(root: &Path, include: &[String], exclude: &[String]) -> Vec<PathBuf> {
    let mut results: Vec<PathBuf> = Vec::new();

    let (tx, rx) = mpsc::sync_channel::<PathBuf>(256);
    let builder = build_walker(root, include, exclude);
    let exclude = exclude.to_vec();

    std::thread::spawn(move || {
        builder.build_parallel().run(|| {
            let tx = tx.clone();
            let exclude = exclude.clone();
            Box::new(move |result| {
                use ignore::WalkState;
                let Ok(entry) = result else { return WalkState::Continue };
                if entry.file_name() == "node_modules" { return WalkState::Skip; }
                let Some(ft) = entry.file_type() else { return WalkState::Continue };
                if !ft.is_file() { return WalkState::Continue; }
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !EXTENSIONS.contains(&ext) { return WalkState::Continue; }
                let path_str = path.to_string_lossy();
                if exclude.iter().any(|ex| path_str.contains(ex.trim_end_matches('/'))) {
                    return WalkState::Continue;
                }
                let _ = tx.send(path.to_path_buf());
                WalkState::Continue
            })
        });
    });

    for path in rx { results.push(path); }
    results.sort();
    results
}
