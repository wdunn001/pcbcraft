//! A short, bounded summary of a design for people and assistants (`design.summary`).
//!
//! Context: `Session::inspect()` returns the whole project and a long capabilities paragraph;
//! this returns counts, the board, parts, nets and the number of findings, small enough to send
//! with every chat message.
//! Rules: parts and nets are capped at `LIST_LIMIT` rows with the overflow counted in `more`;
//! reading changes nothing; the output is built from the project only.
//! Do not: add the project itself or any file path to the summary.

use super::Session;
use pcbcraft_doc::Project;
use serde_json::{Map, Value, json};

/// Most part and net rows listed.
pub const LIST_LIMIT: usize = 200;

fn count(v: &Value, key: &str) -> usize {
    v.get(key).and_then(Value::as_array).map_or(0, Vec::len)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The summary of `project` (without its revision; see [`Session::summary`]).
pub(crate) fn build(project: &Project) -> Value {
    let doc = serde_json::to_value(project).unwrap_or(Value::Null);
    let board = doc.get("board").cloned().unwrap_or(Value::Null);
    let components = doc.get("components").and_then(Value::as_array);
    let empty = Vec::new();
    let components = components.unwrap_or(&empty);

    let mut parts = Vec::new();
    for c in components {
        let footprint = c
            .pointer("/library_footprint/name")
            .and_then(Value::as_str)
            .unwrap_or("");
        parts.push(json!({
            "reference": text(c, "reference"),
            "value": text(c, "value"),
            "kind": text(c, "kind"),
            "footprint": footprint,
            "page": c.get("sheet").cloned().unwrap_or(Value::Null),
        }));
    }
    parts.sort_by(|a, b| text(a, "reference").cmp(text(b, "reference")));
    let parts_more = parts.len().saturating_sub(LIST_LIMIT);
    parts.truncate(LIST_LIMIT);

    let mut pins: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for c in components {
        for pin in c
            .get("pins")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(net) = pin.get("net").and_then(Value::as_u64) {
                *pins.entry(net).or_default() += 1;
            }
        }
    }
    let mut nets: Vec<Value> = doc
        .get("nets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|n| {
            let id = n.get("id").and_then(Value::as_u64).unwrap_or(0);
            json!({"name": text(n, "name"), "pins": pins.get(&id).copied().unwrap_or(0)})
        })
        .collect();
    let nets_more = nets.len().saturating_sub(LIST_LIMIT);
    nets.truncate(LIST_LIMIT);

    let mut severity = Map::new();
    let findings = pcbcraft_rules::erc(project)
        .into_iter()
        .chain(pcbcraft_rules::drc(project));
    for f in findings {
        let key = match f.severity.as_str() {
            "error" => "errors",
            "warning" => "warnings",
            _ => "notes",
        };
        let n = severity.get(key).and_then(Value::as_u64).unwrap_or(0);
        severity.insert(key.into(), json!(n + 1));
    }
    let finding_count = |k: &str| severity.get(k).cloned().unwrap_or(json!(0));

    json!({
        "name": text(&doc, "name"),
        "units": text(&doc, "unit"),
        "board": {
            "width": board.get("width").cloned().unwrap_or(Value::Null),
            "height": board.get("height").cloned().unwrap_or(Value::Null),
            "layers": 2,
            "outline_kind": if board.get("outline").is_some_and(|o| !o.is_null()) { "polygon" } else { "rectangle" },
        },
        "counts": {
            "components": components.len(),
            "footprints": count(&board, "footprints"),
            "tracks": count(&board, "tracks"),
            "vias": count(&board, "vias"),
            "zones": count(&doc, "zones") + count(&board, "zones"),
            "nets": count(&doc, "nets"),
            "graphics": count(&doc, "graphics") + count(&board, "graphics"),
            "pages": count(&doc, "sheets") + 1,
        },
        "parts": parts,
        "parts_more": parts_more,
        "nets": nets,
        "nets_more": nets_more,
        "findings": {
            "errors": finding_count("errors"),
            "warnings": finding_count("warnings"),
            "notes": finding_count("notes"),
        },
    })
}

impl Session {
    /// A short summary of the open design: counts, board, parts, nets and finding counts.
    ///
    /// # Rules
    /// Bounded (see `LIST_LIMIT`); reads only.
    pub fn summary(&self) -> Value {
        let mut v = build(&self.project);
        if let Some(map) = v.as_object_mut() {
            map.insert("revision".into(), json!(self.revision));
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    #[test]
    fn design_summary_of_the_sample_design_is_under_eight_kilobytes_and_names_every_part()
    -> Result<(), crate::Error> {
        let s = Session::sample()?;
        let v = s.summary();
        let size = v.to_string().len();
        assert!(size < 8 * 1024, "{size} bytes");
        let refs: Vec<&str> = v["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["reference"].as_str())
            .collect();
        for c in &s.project.components {
            assert!(
                refs.contains(&c.reference.as_str()),
                "{} is listed",
                c.reference
            );
        }
        assert_eq!(v["counts"]["components"], s.project.components.len());
        Ok(())
    }

    #[test]
    fn design_summary_caps_parts_and_nets_at_two_hundred_with_a_more_count()
    -> Result<(), crate::Error> {
        let mut s = Session::default();
        for range in [0..100, 100..200, 200..230] {
            let commands: Vec<Command> = range
                .map(|i| Command {
                    command: "component.add".into(),
                    params: json!({"kind":"resistor","reference":format!("R{i}"),"value":"1k","position":{"x":(i % 20) as f64 * 3.0,"y":(i / 20) as f64 * 3.0}}),
                })
                .collect();
            s.batch(&commands, None)?;
        }
        let v = s.summary();
        assert_eq!(v["parts"].as_array().map(Vec::len), Some(LIST_LIMIT));
        assert_eq!(v["parts_more"], 30);
        Ok(())
    }

    #[test]
    fn design_summary_is_a_registered_command_with_a_description() {
        let spec = crate::commands()
            .into_iter()
            .find(|c| c.id == "design.summary");
        assert!(spec.is_some_and(|c| !c.description.is_empty()));
    }
}
