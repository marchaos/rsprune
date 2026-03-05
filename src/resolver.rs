use std::path::{Path, PathBuf};

use oxc_resolver::{ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions, TsconfigReferences};

/// Build an oxc_resolver that understands the tsconfig paths/baseUrl.
pub fn build_resolver(tsconfig_path: &Path) -> Resolver {
    let opts = ResolveOptions {
        extensions: vec![
            ".ts".into(),
            ".tsx".into(),
            ".js".into(),
            ".jsx".into(),
            ".mts".into(),
            ".cts".into(),
            ".mjs".into(),
            ".cjs".into(),
            ".d.ts".into(),
            ".json".into(),
        ],
        tsconfig: Some(TsconfigDiscovery::Manual(TsconfigOptions {
            config_file: tsconfig_path.to_path_buf(),
            references: TsconfigReferences::Auto,
        })),
        ..ResolveOptions::default()
    };
    Resolver::new(opts)
}

/// Returns true if this specifier can possibly resolve to a project-local file.
/// Bare node_modules imports (e.g. "react", "lodash") are filtered out early
/// without hitting the filesystem, saving the majority of resolver calls.
#[inline]
pub fn is_project_local(specifier: &str) -> bool {
    // Relative imports always start with . or ..
    if specifier.starts_with('.') || specifier.starts_with('/') {
        return true;
    }
    // tsconfig path aliases always start with a known prefix in this project.
    // We detect them by checking for @ (scoped packages that might be aliases)
    // and any specifier that the resolver would map via `paths`.
    // We let the resolver handle @-prefixed ones since some are aliases.
    // Pure bare module names (no @) are definitely node_modules.
    specifier.starts_with('@')
}

/// Resolve a module specifier from a given file's directory.
/// Returns the absolute path if resolvable within the project (not node_modules).
pub fn resolve_specifier(resolver: &Resolver, from_dir: &Path, specifier: &str) -> Option<PathBuf> {
    match resolver.resolve(from_dir, specifier) {
        Ok(resolved) => {
            let path = resolved.full_path();
            // Ignore node_modules resolutions
            if path.components().any(|c| c.as_os_str() == "node_modules") {
                None
            } else {
                // Avoid canonicalize() syscall — path from resolver is already absolute.
                // Only canonicalize to resolve symlinks if needed.
                Some(path.to_path_buf())
            }
        }
        Err(_) => None,
    }
}
