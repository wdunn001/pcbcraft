//! Helpers shared by tests in several crates. Only dev-dependencies may use it.
//!
//! # Rules
//! - Golden files are rewritten through `blessing()` and nothing else: one
//!   variable, `PCBCRAFT_BLESS=1`, and never on CI.
//! - Pure helpers only; no clocks, network or writes outside the file asked for.
//!
//! # Dependencies
//! - docs/testing-standard.md
//!
//! # Do not
//! - Add a second bless or update variable; `cargo xtask test-standards` fails it.

/// True when the person asked to rewrite golden files (`PCBCRAFT_BLESS` is set)
/// and the run is not on CI. A test that blesses writes its fixture first and
/// then compares, so the diff of the fixture is what review reads.
pub fn blessing() -> bool {
    blessing_from(
        std::env::var_os("PCBCRAFT_BLESS").is_some(),
        std::env::var_os("CI").is_some(),
    )
}

/// The decision behind `blessing()`, separated so it is testable.
pub fn blessing_from(bless_set: bool, on_ci: bool) -> bool {
    bless_set && !on_ci
}

/// Compares `actual` with the golden file at `path`, rewriting the file first
/// when `blessing()` is true. The error names the file.
///
/// # Rules
/// - A missing golden file is an error, never an implicit bless.
pub fn check_golden(path: &std::path::Path, actual: &str) -> Result<(), String> {
    if blessing() {
        std::fs::write(path, actual).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let want = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if want == actual {
        Ok(())
    } else {
        Err(format!(
            "output differs from golden file {} (rerun with PCBCRAFT_BLESS=1 to rewrite it)",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blessing_needs_the_variable_and_no_ci() {
        assert!(blessing_from(true, false));
        assert!(!blessing_from(true, true));
        assert!(!blessing_from(false, false));
    }

    #[test]
    fn a_missing_or_different_golden_file_is_an_error() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("pcbcraft-testkit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join("g.txt");
        assert!(check_golden(&file, "x").is_err());
        std::fs::write(&file, "x").map_err(|e| e.to_string())?;
        if !blessing() {
            assert!(check_golden(&file, "x").is_ok());
            assert!(check_golden(&file, "y").is_err());
        }
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
    }
}
