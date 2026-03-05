use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
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

    let builder = build_walker(root, include);
    let include_set = build_glob_set(root, include, true);
    let exclude_set = build_glob_set(root, exclude, false);

    // Compile regexes once; share across walker threads via Arc.
    let ignore_patterns: Arc<Vec<regex::Regex>> = Arc::new(ignore_patterns.to_vec());

    // Walk on ignore's own thread pool
    std::thread::spawn(move || {
        builder.build_parallel().run(|| {
            let tx = tx.clone();
            let include_set = include_set.clone();
            let exclude_set = exclude_set.clone();
            let ignore_patterns = Arc::clone(&ignore_patterns);
            Box::new(move |result| {
                use ignore::WalkState;
                let Ok(entry) = result else { return WalkState::Continue };

                // Prune default tsconfig-excluded directories
                let fname = entry.file_name();
                if fname == "node_modules"
                    || fname == "bower_components"
                    || fname == "jspm_packages"
                {
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

                if !include_set.is_match(path) {
                    return WalkState::Continue;
                }
                if exclude_set.is_match(path) {
                    return WalkState::Continue;
                }

                let path_str = path.to_string_lossy();
                if ignore_patterns.iter().any(|re| re.is_match(&path_str)) {
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

/// Build a GlobSet from tsconfig include/exclude patterns rooted at `root`.
///
/// Normalises patterns to match tsconfig semantics (see `normalise_tsconfig_glob`):
/// - Bare directory name or `dir/` → `dir/**/*`
/// - `dir/**` (no file segment) → `dir/**/*`
/// - `**/*.test.ts` or `src/**/*.ts` → unchanged (already has a file segment)
fn build_glob_set(root: &Path, patterns: &[String], is_include: bool) -> GlobSet {
    let mut builder = GlobSetBuilder::new();

    if patterns.is_empty() {
        if is_include {
            // No include patterns = match everything
            builder.add(GlobBuilder::new("**/*").literal_separator(true).build().unwrap());
        }
        return builder.build().unwrap();
    }

    for pattern in patterns {
        let normalised = normalise_tsconfig_glob(pattern);
        // Make absolute by joining with root
        let abs = root.join(&normalised);
        let abs_str = abs.to_string_lossy();
        // literal_separator: `*` stays within one path segment (tsc semantics),
        // while `**` still crosses directory boundaries.
        if let Ok(glob) = GlobBuilder::new(&abs_str).literal_separator(true).build() {
            builder.add(glob);
        }
    }

    builder.build().unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap())
}

/// Normalise a tsconfig include/exclude pattern to a proper glob.
///
/// tsconfig semantics:
/// - `src` or `src/`      → `src/**/*`
/// - `src/**`             → `src/**/*`
/// - `**/*.test.ts`       → unchanged
/// - `src/**/*.ts`        → unchanged
fn normalise_tsconfig_glob(pattern: &str) -> String {
    let p = pattern.trim_end_matches('/');
    if p.contains('*') || p.contains('?') {
        // Ends with `/**` (no trailing file segment) → append `/*`
        if p.ends_with("/**") {
            return format!("{p}/*");
        }
        // Already a full glob — use as-is
        return p.to_string();
    }
    // Bare path — treat as directory, recurse into all files
    format!("{p}/**/*")
}

fn build_walker(root: &Path, include: &[String]) -> WalkBuilder {
    // When include is empty tsc walks from the project root.
    let bases: Vec<PathBuf> = if include.is_empty() {
        vec![root.to_path_buf()]
    } else {
        include.iter().map(|p| include_base(root, p)).collect()
    };

    let mut builder = WalkBuilder::new(&bases[0]);
    for base in &bases[1..] {
        builder.add(base);
    }

    builder
        .follow_links(true)
        .hidden(false)
        .standard_filters(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .ignore(false)
        .threads(num_cpus());

    builder
}

/// Extract the deepest walkable base directory from a tsconfig include pattern.
/// Walking starts here; glob filtering handles the rest.
fn include_base(root: &Path, pattern: &str) -> PathBuf {
    // Strip trailing wildcards to find the literal prefix
    let trimmed = pattern
        .trim_end_matches('/')
        .trim_end_matches("/**/*")
        .trim_end_matches("/**")
        .trim_end_matches("/*");

    // If there's still a wildcard, walk from root
    if trimmed.contains('*') || trimmed.contains('?') {
        return root.to_path_buf();
    }

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
    let (tx, rx) = mpsc::sync_channel::<PathBuf>(256);
    let builder = build_walker(root, include);
    let include_set = build_glob_set(root, include, true);
    let exclude_set = build_glob_set(root, exclude, false);

    std::thread::spawn(move || {
        builder.build_parallel().run(|| {
            let tx = tx.clone();
            let include_set = include_set.clone();
            let exclude_set = exclude_set.clone();
            Box::new(move |result| {
                use ignore::WalkState;
                let Ok(entry) = result else { return WalkState::Continue };
                let fname = entry.file_name();
                if fname == "node_modules" || fname == "bower_components" || fname == "jspm_packages" {
                    return WalkState::Skip;
                }
                let Some(ft) = entry.file_type() else { return WalkState::Continue };
                if !ft.is_file() { return WalkState::Continue; }
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !EXTENSIONS.contains(&ext) { return WalkState::Continue; }
                if !include_set.is_match(path) { return WalkState::Continue; }
                if exclude_set.is_match(path) { return WalkState::Continue; }
                let _ = tx.send(path.to_path_buf());
                WalkState::Continue
            })
        });
    });

    let mut results: Vec<PathBuf> = rx.into_iter().collect();
    results.sort();
    results
}
