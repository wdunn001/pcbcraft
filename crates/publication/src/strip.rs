//! Remove private lines from text before publication.
//!
//! # Rules
//! - Drop whole lines that contain privacy violations.
//! - Drop entire PEM private key blocks.
//! - Record one note per removed line.
//!
//! # Dependencies
//! - crates/publication/src/privacy.rs provides the scan functions.
//!
//! # Do not
//! - Edit line contents; only remove complete lines.

use crate::ScanOutcome;
use crate::privacy::{PrivacyOptions, scan_privacy_text};

/// Record of one line that was removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripNote {
    /// The file the line was removed from.
    pub path: String,
    /// The 1-based number of the removed line in the original text.
    pub line: u32,
    /// The privacy rule that matched, such as `private_key`.
    pub rule: String,
}

/// Remove private lines and return the clean text with a note list.
pub fn strip_text(path: &str, text: &str, options: &PrivacyOptions) -> (String, Vec<StripNote>) {
    let outcomes = scan_privacy_text(path, text, options);
    let lines: Vec<&str> = text.lines().collect();

    // Collect lines to drop
    let mut to_drop: Vec<u32> = Vec::new();
    let mut line_rules: Vec<(u32, String)> = Vec::new();

    for o in &outcomes {
        if let ScanOutcome::HardFail { line, rule, .. } = o
            && let Some(l) = line
            && !to_drop.contains(l)
        {
            to_drop.push(*l);
            line_rules.push((*l, rule.to_string()));
        }
    }

    // Handle PEM blocks
    let mut i = 0;
    while i < lines.len() {
        if lines[i].contains("-----BEGIN") {
            let end_idx = find_pem_end(&lines, i);
            if end_idx > i {
                for j in i..=end_idx {
                    let l = (j + 1) as u32;
                    if !to_drop.contains(&l) {
                        to_drop.push(l);
                        line_rules.push((l, "private_key".to_string()));
                    }
                }
            }
        }
        i += 1;
    }

    to_drop.sort();

    let mut result_lines: Vec<&str> = Vec::new();
    let mut notes = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let l = (idx + 1) as u32;
        if to_drop.contains(&l) {
            let rule = line_rules
                .iter()
                .find(|(r, _)| *r == l)
                .map(|(_, name)| name.clone())
                .unwrap_or("unknown".to_string());
            notes.push(StripNote {
                path: path.to_string(),
                line: l,
                rule,
            });
        } else {
            result_lines.push(line);
        }
    }

    // Preserve final newline if input had one
    let has_trailing = text.ends_with('\n');
    let result = if result_lines.is_empty() {
        String::new()
    } else {
        result_lines.join("\n") + if has_trailing { "\n" } else { "" }
    };

    if notes.len() > 10000 {
        notes.truncate(10000);
    }

    (result, notes)
}

fn find_pem_end(lines: &[&str], start: usize) -> usize {
    let rest = &lines[start + 1..];
    for (offset, line) in rest.iter().enumerate() {
        if line.contains("-----END") {
            return start + 1 + offset;
        }
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripped_text_scans_clean() {
        let home_prefix = if cfg!(target_os = "linux") {
            "/home/"
        } else {
            "/Users/"
        };
        let text = format!(
            "name = \"board\"\npath = \"{}alice/pcb/board.pcb\"\ntoken = \"ghp_0123456789abcdefghijklmnopqrstuvwxyz\"\nkeep = 1\n",
            home_prefix
        );
        let opts = PrivacyOptions::default();
        let (clean, notes) = strip_text("test.txt", &text, &opts);
        assert_eq!(notes.len(), 2, "expected 2 notes, got {}", notes.len());
        assert_eq!(clean, "name = \"board\"\nkeep = 1\n");
        assert!(
            scan_privacy_text("test.txt", &clean, &opts).is_empty(),
            "cleaned text still has privacy findings"
        );
    }

    #[test]
    fn clean_text_is_unchanged() {
        let text = "name = \"board\"\nkeep = 1\n";
        let opts = PrivacyOptions::default();
        let (clean, notes) = strip_text("test.txt", text, &opts);
        assert_eq!(notes.len(), 0);
        assert_eq!(clean, text);
    }
}
