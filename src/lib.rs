pub mod files;
pub mod parser;
pub mod resolver;
pub mod tsconfig;

/// File extensions rsprune walks and parses.
pub static EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"];
