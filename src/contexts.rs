use std::path::{Path, PathBuf};

use regex::RegexBuilder;

use crate::parser::ContextInfo;

/// Returns the files a bundler context in `from_path` loads, chosen from `candidates` (the files being analysed).
/// Paths are tested the way webpack and rspack test them: relative to the context directory, prefixed with `./`.
pub fn matching_files<'a>(
    context: &ContextInfo,
    from_path: &Path,
    candidates: impl Iterator<Item = &'a Path>,
) -> anyhow::Result<Vec<PathBuf>> {
    let reg_exp = RegexBuilder::new(context.reg_exp.as_deref().unwrap_or(""))
        .case_insensitive(context.ignore_case)
        .build()?;
    let from_dir = from_path.parent().unwrap_or(Path::new("/"));
    let directory = from_dir.join(&context.directory);
    let directory = directory.canonicalize().unwrap_or(directory);

    Ok(candidates
        .filter(|path| {
            let Ok(relative) = path.strip_prefix(&directory) else {
                return false;
            };
            if !context.recursive && relative.components().count() > 1 {
                return false;
            }
            let request = format!("./{}", relative.to_string_lossy().replace('\\', "/"));
            reg_exp.is_match(&request)
        })
        .map(Path::to_path_buf)
        .collect())
}
