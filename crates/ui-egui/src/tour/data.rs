//! Tour file format, validation and embedded loader.
//!
//! Files live in `crates/text/tours/` beside `strings/`. Step words are catalog
//! keys; the TOML carries ids, anchors, placement and optional wait rules only.

use pcbcraft_text::{ENGLISH, valid_key};
use serde::Deserialize;
use std::collections::{BTreeSet, HashSet};

/// English tour structure embedded in every build.
pub const EN_TOML: &str = include_str!("../../../text/tours/en.toml");

const MIN_STEPS: usize = 3;
const MAX_STEPS: usize = 6;

const WORKSPACES: &[&str] = &["files", "schematic", "layout", "manufacture"];

const TRIGGERS: &[&str] = &["first_launch", "first_visit", "manual", "hint"];

const PLACEMENTS: &[&str] = &["top", "bottom", "left", "right", "auto"];

/// Named step conditions the runtime may evaluate (#773).
const CONDITIONS: &[&str] = &["project.open", "project.empty"];

const PANEL_TABS: &[&str] = &[
    "findings", "output", "console", "layers", "library", "outputs", "machines",
];

/// One tour definition from TOML.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tour {
    pub id: String,
    pub version: u32,
    pub workspace: String,
    pub trigger: String,
    pub steps: Vec<Step>,
}

/// One step within a tour.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Step {
    pub id: String,
    pub target: String,
    pub title: String,
    pub body: String,
    pub placement: String,
    #[serde(default, rename = "if")]
    pub condition: Option<String>,
    #[serde(default)]
    pub wait_for: Option<String>,
}

/// Root document: `[[tour]]` rows in TOML.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Doc {
    #[serde(default)]
    pub tour: Vec<Tour>,
}

impl Tour {
    /// Catalog key for the Help > Tours menu label.
    pub fn list_key(&self) -> String {
        format!("tour.{}.list", self.id)
    }
}

/// Parse TOML text into a document without validation.
pub fn parse_toml(text: &str) -> Result<Doc, String> {
    toml::from_str(text).map_err(|e| format!("tour TOML: {e}"))
}

/// Load and validate the embedded English tour file.
pub fn embedded() -> Result<Doc, String> {
    let doc = parse_toml(EN_TOML)?;
    validate(&doc)?;
    Ok(doc)
}

/// Validate every rule on a parsed document against the English catalog.
pub fn validate(doc: &Doc) -> Result<(), String> {
    validate_with_catalog(doc, &catalog_keys())
}

fn validate_with_catalog(doc: &Doc, keys: &BTreeSet<String>) -> Result<(), String> {
    let command_ids = command_ids();
    let mut tour_ids = HashSet::new();
    for tour in &doc.tour {
        if !valid_key(&tour.id) {
            return Err(format!("tour id `{}` is not a valid key", tour.id));
        }
        if !tour_ids.insert(tour.id.clone()) {
            return Err(format!("duplicate tour id `{}`", tour.id));
        }
        if !WORKSPACES.contains(&tour.workspace.as_str()) {
            return Err(format!(
                "tour `{}`: unknown workspace `{}`",
                tour.id, tour.workspace
            ));
        }
        if !TRIGGERS.contains(&tour.trigger.as_str()) {
            return Err(format!(
                "tour `{}`: unknown trigger `{}`",
                tour.id, tour.trigger
            ));
        }
        let step_count = tour.steps.len();
        if !(MIN_STEPS..=MAX_STEPS).contains(&step_count) {
            return Err(format!(
                "tour `{}`: expected {MIN_STEPS} to {MAX_STEPS} steps, found {step_count}"
            ));
        }
        require_catalog_key(keys, &tour.list_key(), &format!("tour `{}` list", tour.id))?;
        let mut step_ids = HashSet::new();
        for step in &tour.steps {
            if !valid_key(&step.id) {
                return Err(format!(
                    "tour `{}`: step id `{}` is not a valid key",
                    tour.id, step.id
                ));
            }
            if !step_ids.insert(step.id.clone()) {
                return Err(format!(
                    "tour `{}`: duplicate step id `{}`",
                    tour.id, step.id
                ));
            }
            if step.target != "none" && step.target.is_empty() {
                return Err(format!(
                    "tour `{}` step `{}`: target is empty",
                    tour.id, step.id
                ));
            }
            if !PLACEMENTS.contains(&step.placement.as_str()) {
                return Err(format!(
                    "tour `{}` step `{}`: unknown placement `{}`",
                    tour.id, step.id, step.placement
                ));
            }
            let ctx = format!("tour `{}` step `{}`", tour.id, step.id);
            require_catalog_key(keys, &step.title, &format!("{ctx} title"))?;
            require_catalog_key(keys, &step.body, &format!("{ctx} body"))?;
            if let Some(cond) = &step.condition {
                if !CONDITIONS.contains(&cond.as_str()) {
                    return Err(format!("{ctx}: unknown condition `{cond}`"));
                }
            }
            if let Some(wait) = &step.wait_for {
                validate_wait_for(wait, &command_ids)
                    .map_err(|reason| format!("{ctx}: {reason}"))?;
            }
        }
    }
    Ok(())
}

fn catalog_keys() -> BTreeSet<String> {
    pcbcraft_text::parse(ENGLISH)
        .0
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

fn command_ids() -> BTreeSet<String> {
    pcbcraft_engine::commands()
        .into_iter()
        .map(|spec| spec.id.to_string())
        .collect()
}

fn require_catalog_key(keys: &BTreeSet<String>, key: &str, label: &str) -> Result<(), String> {
    if !valid_key(key) {
        return Err(format!("{label}: `{key}` is not a valid catalog key"));
    }
    if !keys.contains(key) {
        return Err(format!("{label}: missing catalog key `{key}`"));
    }
    let tip = format!("{key}.tip");
    if !keys.contains(&tip) {
        return Err(format!("{label}: missing catalog key `{tip}`"));
    }
    Ok(())
}

fn validate_wait_for(raw: &str, command_ids: &BTreeSet<String>) -> Result<(), String> {
    match raw {
        "document_changed" | "selection:part" | "selection:any" => Ok(()),
        _ if raw.starts_with("click:") => {
            let anchor = raw.strip_prefix("click:").unwrap_or("");
            if anchor.is_empty() {
                Err("wait_for click: needs an anchor id".into())
            } else {
                Ok(())
            }
        }
        _ if raw.starts_with("tool:") => {
            let name = raw.strip_prefix("tool:").unwrap_or("");
            if name.is_empty() {
                Err("wait_for tool: needs a tool name".into())
            } else {
                Ok(())
            }
        }
        _ if raw.starts_with("tab:") => {
            let tab = raw.strip_prefix("tab:").unwrap_or("");
            if WORKSPACES.contains(&tab) {
                Ok(())
            } else {
                Err(format!("wait_for tab: unknown workspace `{tab}`"))
            }
        }
        _ if raw.starts_with("panel_open:") => {
            let panel = raw.strip_prefix("panel_open:").unwrap_or("");
            if panel.is_empty() {
                Err("wait_for panel_open: needs a region tab id".into())
            } else if PANEL_TABS.contains(&panel) {
                Ok(())
            } else {
                Err(format!("wait_for panel_open: unknown region tab `{panel}`"))
            }
        }
        _ if raw.starts_with("command:") => {
            let id = raw.strip_prefix("command:").unwrap_or("");
            if id.is_empty() {
                Err("wait_for command: needs a command id".into())
            } else if command_ids.contains(id) {
                Ok(())
            } else {
                Err(format!("wait_for command: unknown command `{id}`"))
            }
        }
        _ => Err(format!("unknown wait_for `{raw}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_catalog_keys() -> BTreeSet<String> {
        let mut keys = catalog_keys();
        keys.insert("tour.validator-fixture.list".into());
        keys.insert("tour.validator-fixture.list.tip".into());
        keys
    }

    fn validate_fixture(doc: &Doc) -> Result<(), String> {
        validate_with_catalog(doc, &fixture_catalog_keys())
    }

    fn good_tour_toml() -> String {
        r#"
[[tour]]
id = "validator-fixture"
version = 1
workspace = "files"
trigger = "manual"

[[tour.steps]]
id = "one"
target = "none"
title = "menu.help.shortcuts"
body = "menu.help.commands"
placement = "auto"

[[tour.steps]]
id = "two"
target = "none"
title = "menu.help.commands"
body = "menu.help.shortcuts"
placement = "bottom"

[[tour.steps]]
id = "three"
target = "none"
title = "menu.help.shortcuts"
body = "menu.help.commands"
placement = "top"
wait_for = "document_changed"
"#
        .to_string()
    }

    fn seven_step_toml() -> String {
        let mut text = good_tour_toml();
        for n in 4..=7 {
            text.push_str(&format!(
                "\n[[tour.steps]]\nid = \"step{n}\"\ntarget = \"none\"\ntitle = \"menu.help.shortcuts\"\nbody = \"menu.help.commands\"\nplacement = \"auto\"\n"
            ));
        }
        text
    }

    #[test]
    fn a_good_file_loads() -> Result<(), String> {
        let doc = parse_toml(&good_tour_toml())?;
        validate_fixture(&doc)
    }

    #[test]
    fn two_steps_is_rejected() -> Result<(), String> {
        let text = r#"
[[tour]]
id = "short"
version = 1
workspace = "files"
trigger = "manual"

[[tour.steps]]
id = "a"
target = "none"
title = "menu.help.shortcuts"
body = "menu.help.commands"
placement = "auto"

[[tour.steps]]
id = "b"
target = "none"
title = "menu.help.commands"
body = "menu.help.shortcuts"
placement = "auto"
"#;
        let doc = parse_toml(text)?;
        let err = validate(&doc).expect_err("two steps");
        assert!(
            err.contains("3 to 6"),
            "expected step count error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn seven_steps_is_rejected() -> Result<(), String> {
        let doc = parse_toml(&seven_step_toml())?;
        let err = validate(&doc).expect_err("seven steps");
        assert!(
            err.contains("3 to 6"),
            "expected step count error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn unknown_wait_for_is_rejected() -> Result<(), String> {
        let text = good_tour_toml().replace(
            "wait_for = \"document_changed\"",
            "wait_for = \"not-a-real-wait\"",
        );
        let doc = parse_toml(&text)?;
        let err = validate_fixture(&doc).expect_err("bad wait_for");
        assert!(
            err.contains("unknown wait_for"),
            "expected wait_for error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn duplicate_step_ids_are_rejected() -> Result<(), String> {
        let text = good_tour_toml().replace("id = \"three\"", "id = \"two\"");
        let doc = parse_toml(&text)?;
        let err = validate_fixture(&doc).expect_err("duplicate step");
        assert!(
            err.contains("duplicate step id"),
            "expected duplicate step error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn missing_string_keys_are_rejected() -> Result<(), String> {
        let text = good_tour_toml().replace("menu.help.shortcuts", "tour.no.such.title");
        let doc = parse_toml(&text)?;
        let err = validate_fixture(&doc).expect_err("missing key");
        assert!(
            err.contains("missing catalog key"),
            "expected missing key error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn missing_tip_keys_are_rejected() -> Result<(), String> {
        let text = good_tour_toml().replace("menu.help.shortcuts", "menu.help");
        let doc = parse_toml(&text)?;
        let err = validate_fixture(&doc).expect_err("missing tip");
        assert!(
            err.contains(".tip"),
            "expected missing tip error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn unknown_condition_is_rejected() -> Result<(), String> {
        let text = good_tour_toml().replace(
            "placement = \"top\"",
            "placement = \"top\"\nif = \"never.registered\"",
        );
        let doc = parse_toml(&text)?;
        let err = validate_fixture(&doc).expect_err("bad condition");
        assert!(
            err.contains("unknown condition"),
            "expected condition error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn every_shipped_tour_passes_validation() -> Result<(), String> {
        embedded()?;
        Ok(())
    }
}
