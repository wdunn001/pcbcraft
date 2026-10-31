//! Bundle file type rules.
//!
//! # Rules
//! - Executable and script extensions listed in data are refused.
//!
//! # Dependencies
//! - crates/publication/data/refused_bundle_extensions.toml
//!
//! # Do not
//! - Allow script files to ride along inside a design bundle.

use serde::Deserialize;
use std::sync::OnceLock;

const REFUSED_TOML: &str = include_str!("../data/refused_bundle_extensions.toml");

fn refused_list() -> &'static [String] {
    static LIST: OnceLock<Vec<String>> = OnceLock::new();
    LIST.get_or_init(|| {
        #[derive(Deserialize)]
        struct Root {
            extensions: Vec<String>,
        }
        toml::from_str(REFUSED_TOML)
            .map(|r: Root| r.extensions)
            .unwrap_or_default()
    })
}

/// True when the path ends with a refused extension.
pub fn refused_path(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let Some(ext) = name.rsplit('.').next() else {
        return false;
    };
    if ext == name {
        return false;
    }
    let lower = ext.to_ascii_lowercase();
    refused_list().iter().any(|e| *e == lower)
}
