//! Parse parts.search answers into display-friendly result rows.
//!
//! Context: issue #946. The engine's `parts.search` tool returns a JSON object
//! with ranked hits; this module turns each hit into a row the Library tab can
//! show. It is pure parsing code with no file or network access, so it runs the
//! same in the desktop app and in the wasm build.
//!
//! Local rules: never panic or unwrap; skip malformed items and continue. The
//! output is bounded to at most 100 rows regardless of the input size.
//!
//! # Dependencies
//! - crates/engine/src/library.rs (`search_parts` produces the JSON this parses)
//! - crates/library/src/part_search.rs (`PartSearchHit` structure)
//!
//! # Do not
//! - Read files, open sockets, or depend on the system clock.
//! - Change the engine's JSON shape; this module must adapt to it.

use serde_json::Value;

/// One result row ready for display.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct NeedHit {
    /// Manufacturer part number.
    pub(super) mpn: String,
    /// Manufacturer name.
    pub(super) manufacturer: String,
    /// The part's description text from the source.
    pub(super) description: String,
    /// Combined search score; higher is better.
    pub(super) score: f32,
    /// Human-readable reasons why this part matched.
    pub(super) reasons: Vec<String>,
}

/// Turn a parts.search answer into result rows.
///
/// # Rules
/// - Items missing a listing field are skipped.
/// - The result is at most 100 rows.
/// - Never panics; malformed items are skipped.
///
/// # Dependencies
/// - serde_json for value access.
///
/// # Do not
/// - Add side effects such as logging or file access.
pub(super) fn parse_need_hits(answer: &Value) -> Vec<NeedHit> {
    let items = match answer.get("items") {
        Some(Value::Array(items)) => items,
        _ => return Vec::new(),
    };

    let mut hits = Vec::new();
    for item in items {
        let listing = match item.get("listing") {
            Some(Value::Object(_)) => item["listing"].clone(),
            _ => continue,
        };

        let mpn = listing
            .get("mpn")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string();
        let manufacturer = listing
            .get("manufacturer")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string();
        let description = listing
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let score = item
            .get("score")
            .and_then(Value::as_f64)
            .map(|f| f as f32)
            .unwrap_or(0.0);

        let reasons = reason_strings(&item);

        hits.push(NeedHit {
            mpn,
            manufacturer,
            description,
            score,
            reasons,
        });

        if hits.len() >= 100 {
            break;
        }
    }

    hits
}

/// Format a result row as a single display line.
///
/// # Rules
/// - Always shows the MPN and manufacturer.
/// - Adds the first reason after " - " when present.
///
/// # Dependencies
/// - None beyond the NeedHit struct.
///
/// # Do not
/// - Include score or description; those go on separate UI elements.
pub(super) fn need_hit_line(hit: &NeedHit) -> String {
    let base = format!("{} {}", hit.mpn, hit.manufacturer);
    match hit.reasons.first() {
        Some(reason) => format!("{base} - {reason}"),
        None => base,
    }
}

fn reason_strings(item: &Value) -> Vec<String> {
    let reasons = match item.get("reasons") {
        Some(Value::Array(reasons)) => reasons,
        _ => return Vec::new(),
    };

    let mut lines = Vec::new();
    for reason in reasons {
        let line = match reason.get("kind").and_then(Value::as_str) {
            Some("text") => {
                let token = reason.get("token").and_then(Value::as_str).unwrap_or("");
                format!("keyword {token}")
            }
            Some("tag") => {
                let tag = reason.get("tag").and_then(Value::as_str).unwrap_or("");
                format!("tag {tag}")
            }
            Some("filter") => {
                let field = reason.get("field").and_then(Value::as_str).unwrap_or("");
                format!("filter {field}")
            }
            Some("vector") => {
                let similarity = reason
                    .get("similarity")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                format!("vector {similarity:.2}")
            }
            _ => "matched".to_string(),
        };
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_need_hits_reads_score_and_reasons() {
        let answer = json!({
            "indexed": 2,
            "items": [
                {
                    "listing": {
                        "mpn": "EX-R-10K",
                        "manufacturer": "Example Components",
                        "description": "10k resistor"
                    },
                    "score": 2.5,
                    "reasons": [
                        {"kind": "tag", "tag": "resistor"},
                        {"kind": "text", "token": "10k"}
                    ]
                },
                {
                    "listing": {
                        "mpn": "EX-C-100N",
                        "manufacturer": "Example Components",
                        "description": "100nF capacitor"
                    },
                    "score": 1.2,
                    "reasons": [
                        {"kind": "tag", "tag": "capacitor"}
                    ]
                }
            ]
        });

        let hits = parse_need_hits(&answer);
        assert_eq!(hits.len(), 2);

        assert_eq!(hits[0].mpn, "EX-R-10K");
        assert_eq!(hits[0].manufacturer, "Example Components");
        assert_eq!(hits[0].score, 2.5);
        assert_eq!(hits[0].reasons.len(), 2);
        assert_eq!(hits[0].reasons[0], "tag resistor");
        assert_eq!(hits[0].reasons[1], "keyword 10k");

        assert_eq!(hits[1].mpn, "EX-C-100N");
        assert_eq!(hits[1].score, 1.2);
        assert_eq!(hits[1].reasons.len(), 1);
        assert_eq!(hits[1].reasons[0], "tag capacitor");
    }

    #[test]
    fn parse_need_hits_skips_a_broken_item() {
        let answer = json!({
            "indexed": 2,
            "items": [
                {
                    "score": 1.0,
                    "reasons": []
                },
                {
                    "listing": {
                        "mpn": "VALID-1",
                        "manufacturer": "Test Mfg",
                        "description": "A valid part"
                    },
                    "score": 0.5,
                    "reasons": []
                }
            ]
        });

        let hits = parse_need_hits(&answer);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].mpn, "VALID-1");
    }

    #[test]
    fn need_hit_line_shows_the_first_reason() {
        let hit = NeedHit {
            mpn: "EX-R-10K".to_string(),
            manufacturer: "Example Components".to_string(),
            description: "10k resistor".to_string(),
            score: 2.5,
            reasons: vec!["tag resistor".to_string(), "keyword 10k".to_string()],
        };

        let line = need_hit_line(&hit);
        assert_eq!(line, "EX-R-10K Example Components - tag resistor");
    }
}
