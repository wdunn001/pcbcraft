//! Signal integrity check codes and classification.
//!
//! Lists the published DRC codes that belong to signal integrity checks,
//! and provides a lookup to test whether any code is an SI check.
//!
//! # Rules
//! - Every code in SIGNAL_INTEGRITY_CODES must appear in CHECK_CODES.
//! - New SI checks must add their code to both lists.
//!
//! # Dependencies
//! - crates/rules/src/lib.rs (CHECK_CODES)
//!
//! # Do not
//! Do not add codes here that are not yet in CHECK_CODES.

/// Codes for signal integrity checks that verify high-speed routing quality.
///
/// These checks flag impedance mismatches, return path gaps, crosstalk
/// risk and stub length violations that degrade signal quality on fast
/// nets. Each code appears in CHECK_CODES with its default severity.
pub const SIGNAL_INTEGRITY_CODES: &[&str] = &[
    "impedance_target",
    "diffpair_impedance",
    "return_path",
    "parallel_crosstalk",
    "net_electrically_long",
    "stub_length",
    "testpoint_coverage",
];

/// Returns true when the given finding code is a signal integrity check.
///
/// Call this to group, filter or style SI findings separately from
/// electrical or geometric checks. Unknown or empty codes return false.
pub fn is_signal_integrity(code: &str) -> bool {
    SIGNAL_INTEGRITY_CODES.contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHECK_CODES;

    #[test]
    fn every_signal_integrity_code_is_a_published_check_code() -> Result<(), String> {
        for code in SIGNAL_INTEGRITY_CODES {
            let found = CHECK_CODES.iter().any(|(c, _)| *c == *code);
            if !found {
                return Err(format!("SI code {} not in CHECK_CODES", code));
            }
        }
        Ok(())
    }

    #[test]
    fn is_signal_integrity_identifies_si_codes() -> Result<(), String> {
        if !is_signal_integrity("return_path") {
            return Err("return_path should be SI".into());
        }
        if is_signal_integrity("clearance") {
            return Err("clearance should not be SI".into());
        }
        if is_signal_integrity("") {
            return Err("empty string should not be SI".into());
        }
        Ok(())
    }
}
