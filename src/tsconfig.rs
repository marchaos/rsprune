use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jsonc_parser::parse_to_serde_value;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
    pub base_url: Option<String>,
    pub paths: Option<HashMap<String, Vec<String>>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
    pub compiler_options: Option<CompilerOptions>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

impl TsConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let value = parse_to_serde_value(&content, &Default::default())
            .map_err(|e| anyhow::anyhow!("Failed to parse tsconfig JSONC: {e}"))?
            .unwrap_or(serde_json::Value::Object(Default::default()));
        let config: TsConfig = serde_json::from_value(value)
            .context("Failed to deserialize tsconfig")?;
        Ok(config)
    }

    pub fn root_dir(&self, tsconfig_path: &Path) -> PathBuf {
        tsconfig_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }
}
