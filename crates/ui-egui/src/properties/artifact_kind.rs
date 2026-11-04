//! Per-kind facts for an artifact Properties card.
//!
//! Context: a copper plot and a drill file carry different facts that belong
//! on their Properties cards. This file turns an artifact record into a list
//! of labelled facts (side, layer, hole plating, placement side and flavour)
//! and leaves the rest to the generic card rows.
//! Local rules: the facts are plain strings from the text catalog so they
//! are translatable; the caller draws them in the same form rows as the
//! generic rows. The logic is pure: it reads the record and returns facts.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`Artifact`)
//! - crates/text/src/lib.rs (`pcbcraft_text::lookup`)
//!
//! # Do not
//! - Read the board or project here; the record holds everything needed.

use pcbcraft_doc::Artifact;

/// A labelled fact for an artifact Properties card.
pub(crate) struct Fact {
    /// Catalog key of the label, resolved by the caller.
    pub label_key: &'static str,
    /// Text to show. For side and holes this is a value key to resolve.
    pub value: String,
}

/// The per-kind facts of an artifact, empty for kinds with none.
pub(crate) fn kind_facts(a: &Artifact) -> Vec<Fact> {
    match a.kind.as_str() {
        "copper_plot" | "mask_plot" | "silk_plot" | "paste_plot" | "profile_plot" => {
            let first = a.layers.first().map(|s| s.as_str());
            let side = match first {
                None => "top",
                Some(f) if f.starts_with("B.") => "bottom",
                Some(f) if f.starts_with("F.") => "top",
                Some(_) => "inner",
            };
            let side_key = format!("artifact.fact.side.{side}");
            let layer = first.unwrap_or("the usual layers").to_string();
            vec![
                Fact {
                    label_key: "artifact.fact.side",
                    value: side_key,
                },
                Fact {
                    label_key: "artifact.fact.layer",
                    value: layer,
                },
            ]
        }
        "drill_plated" => {
            vec![Fact {
                label_key: "artifact.fact.holes",
                value: "artifact.fact.holes.plated".to_owned(),
            }]
        }
        "drill_unplated" => {
            vec![Fact {
                label_key: "artifact.fact.holes",
                value: "artifact.fact.holes.not_plated".to_owned(),
            }]
        }
        "placement" | "placement_job" | "placement_yy1" => {
            let mut facts = Vec::new();
            let side = match a.settings.get("side").and_then(|v| v.as_str()) {
                Some("bottom") => "bottom",
                _ => "top",
            };
            facts.push(Fact {
                label_key: "artifact.fact.side",
                value: format!("artifact.fact.side.{side}"),
            });
            if let Some(flav) = a.settings.get("flavour").and_then(|v| v.as_str()) {
                facts.push(Fact {
                    label_key: "artifact.fact.flavour",
                    value: flav.to_owned(),
                });
            }
            facts
        }
        "bom" | "drill_map" | "fab_drawing" | "assembly_drawing" | "package" => Vec::new(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make(kind: &str, layers: &[&str], settings: serde_json::Value) -> Artifact {
        Artifact {
            id: 1,
            name: kind.to_string(),
            kind: kind.to_string(),
            layers: layers.iter().map(|s| s.to_string()).collect(),
            settings,
            machine: None,
            path: "make/".to_string(),
            generated_at_revision: None,
            generated_hash: None,
            job: None,
        }
    }

    #[test]
    fn a_bottom_mask_plot_reads_bottom_and_its_layer() {
        let a = make("mask_plot", &["B.Mask"], json!(null));
        let facts = kind_facts(&a);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].value, "artifact.fact.side.bottom");
        assert_eq!(facts[0].label_key, "artifact.fact.side");
        assert_eq!(facts[1].value, "B.Mask");
        assert_eq!(facts[1].label_key, "artifact.fact.layer");
    }

    #[test]
    fn a_default_copper_plot_reads_top() {
        let a = make("copper_plot", &[], json!(null));
        let facts = kind_facts(&a);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].value, "artifact.fact.side.top");
        assert_eq!(facts[1].value, "the usual layers");
    }

    #[test]
    fn a_plated_drill_reads_plated() {
        let a = make("drill_plated", &[], json!(null));
        let facts = kind_facts(&a);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].value, "artifact.fact.holes.plated");
    }

    #[test]
    fn an_unplated_drill_reads_not_plated() {
        let a = make("drill_unplated", &[], json!(null));
        let facts = kind_facts(&a);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].value, "artifact.fact.holes.not_plated");
    }

    #[test]
    fn a_placement_file_reads_its_side_and_flavour() {
        let a = make(
            "placement",
            &[],
            json!({"side": "bottom", "flavour": "classic"}),
        );
        let facts = kind_facts(&a);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].value, "artifact.fact.side.bottom");
        assert_eq!(facts[0].label_key, "artifact.fact.side");
        assert_eq!(facts[1].value, "classic");
        assert_eq!(facts[1].label_key, "artifact.fact.flavour");

        let b = make("placement", &[], json!(null));
        let facts2 = kind_facts(&b);
        assert_eq!(facts2.len(), 1);
        assert_eq!(facts2[0].value, "artifact.fact.side.top");
    }

    #[test]
    fn a_bill_of_materials_has_no_kind_facts() {
        let a = make("bom", &[], json!(null));
        let facts = kind_facts(&a);
        assert!(facts.is_empty());
    }

    #[test]
    fn artifact_facts_use_defined_catalog_keys() {
        let keys = [
            "artifact.fact.side",
            "artifact.fact.layer",
            "artifact.fact.holes",
            "artifact.fact.flavour",
            "artifact.fact.side.top",
            "artifact.fact.side.bottom",
            "artifact.fact.side.inner",
            "artifact.fact.holes.plated",
            "artifact.fact.holes.not_plated",
        ];
        for key in keys {
            let text = pcbcraft_text::lookup(key, &[]);
            assert_ne!(text, key, "key {key} is not defined");
        }
    }
}
