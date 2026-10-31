//! Fabrication rule minimums and named capability presets.
use crate::Id;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

fn fab_bound(v: f64) -> bool {
    v.is_finite() && (0.001..=100.0).contains(&v)
}

/// Project fabrication minimums and preset metadata. Absent on a board means
/// no fab floor is recorded beyond the default copper clearance.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FabRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_track: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_space: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_drill: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_npth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ring_via: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ring_pth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_hole_via: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_hole_pad: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_copper_via: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_copper_pth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_copper_npth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_routed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_vscore: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silk_line: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silk_text_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silk_to_pad: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_dam: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_aspect_ratio: Option<f64>,
    /// Finished-hole diameter added when measuring annular ring against the
    /// drill tool, when the preset records one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plating_allowance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieved: Option<String>,
    /// When set, fabrication export writes true Gerber arcs by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arcs: Option<bool>,
}

impl FabRules {
    pub fn valid(&self) -> bool {
        let ok = |v: Option<f64>| v.is_none_or(fab_bound);
        ok(self.min_track)
            && ok(self.min_space)
            && ok(self.min_drill)
            && ok(self.min_npth)
            && ok(self.min_ring_via)
            && ok(self.min_ring_pth)
            && ok(self.hole_to_hole_via)
            && ok(self.hole_to_hole_pad)
            && ok(self.hole_to_copper_via)
            && ok(self.hole_to_copper_pth)
            && ok(self.hole_to_copper_npth)
            && ok(self.edge_routed)
            && ok(self.edge_vscore)
            && ok(self.silk_line)
            && ok(self.silk_text_height)
            && ok(self.silk_to_pad)
            && ok(self.mask_dam)
            && self
                .max_aspect_ratio
                .is_none_or(|r| r.is_finite() && (1.0..=50.0).contains(&r))
            && self
                .plating_allowance
                .is_none_or(|p| p.is_finite() && (0.0..=0.5).contains(&p))
            && self
                .preset
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 64)
            && self
                .source_url
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 512)
            && self
                .retrieved
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 32)
    }

    pub fn drill_tool(&self, finished_drill: f64) -> f64 {
        finished_drill + self.plating_allowance.unwrap_or(0.0)
    }
}

/// Clearance between every net in one class and every net in another.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ClassPairRule {
    pub class_a: Id,
    pub class_b: Id,
    pub clearance: f64,
}

impl ClassPairRule {
    pub fn valid(&self) -> bool {
        self.class_a != self.class_b
            && self.class_a != 0
            && self.class_b != 0
            && fab_bound(self.clearance)
    }

    pub fn key(&self) -> (Id, Id) {
        (
            self.class_a.min(self.class_b),
            self.class_a.max(self.class_b),
        )
    }
}

pub const FAB_PRESET_NAMES: &[&str] = &[
    "JLCPCB 1-2L",
    "OSH Park 2L",
    "AISLER 2L HASL 35um",
    "Eurocircuits class 6",
];

#[derive(Clone, Debug)]
pub struct FabPreset {
    pub name: &'static str,
    pub rules: FabRules,
}

pub fn fab_preset(name: &str) -> Option<FabPreset> {
    fab_presets().iter().find(|p| p.name == name).cloned()
}

pub fn fab_preset_list() -> Vec<&'static str> {
    FAB_PRESET_NAMES.to_vec()
}

fn fab_presets() -> &'static [FabPreset] {
    FAB_PRESETS.as_slice()
}

fn preset_rules(
    preset: &'static str,
    source_url: &'static str,
    retrieved: &'static str,
    body: FabRules,
) -> FabRules {
    FabRules {
        preset: Some(preset.into()),
        source_url: Some(source_url.into()),
        retrieved: Some(retrieved.into()),
        ..body
    }
}

static FAB_PRESETS: LazyLock<Vec<FabPreset>> = LazyLock::new(|| {
    vec![
        FabPreset {
            name: "JLCPCB 1-2L",
            rules: preset_rules(
                "JLCPCB 1-2L",
                "https://jlcpcb.com/capabilities/Capabilities",
                "2026-03-28",
                FabRules {
                    arcs: Some(true),
                    min_track: Some(0.127),
                    min_space: Some(0.127),
                    min_drill: Some(0.3),
                    min_npth: Some(0.3),
                    min_ring_via: Some(0.075),
                    min_ring_pth: Some(0.075),
                    hole_to_hole_via: Some(0.3),
                    hole_to_hole_pad: Some(0.3),
                    hole_to_copper_via: Some(0.2),
                    hole_to_copper_pth: Some(0.2),
                    hole_to_copper_npth: Some(0.2),
                    edge_routed: Some(0.3),
                    silk_line: Some(0.15),
                    silk_text_height: Some(1.0),
                    silk_to_pad: Some(0.15),
                    max_aspect_ratio: Some(20.0),
                    plating_allowance: Some(0.025),
                    ..FabRules::default()
                },
            ),
        },
        FabPreset {
            name: "OSH Park 2L",
            rules: preset_rules(
                "OSH Park 2L",
                "https://docs.oshpark.com/design-rules/2-layer-design-rules/",
                "2026-03-28",
                FabRules {
                    min_track: Some(0.1524),
                    min_space: Some(0.1524),
                    min_drill: Some(0.254),
                    min_npth: Some(0.254),
                    min_ring_via: Some(0.1524),
                    min_ring_pth: Some(0.1524),
                    hole_to_hole_via: Some(0.254),
                    hole_to_hole_pad: Some(0.254),
                    hole_to_copper_via: Some(0.254),
                    hole_to_copper_pth: Some(0.254),
                    hole_to_copper_npth: Some(0.254),
                    edge_routed: Some(0.254),
                    silk_line: Some(0.1524),
                    silk_text_height: Some(1.016),
                    silk_to_pad: Some(0.1524),
                    max_aspect_ratio: Some(16.0),
                    plating_allowance: Some(0.025),
                    ..FabRules::default()
                },
            ),
        },
        FabPreset {
            name: "AISLER 2L HASL 35um",
            rules: preset_rules(
                "AISLER 2L HASL 35um",
                "https://aisler.net/pcb-specs",
                "2026-03-28",
                FabRules {
                    min_track: Some(0.15),
                    min_space: Some(0.15),
                    min_drill: Some(0.3),
                    min_npth: Some(0.3),
                    min_ring_via: Some(0.075),
                    min_ring_pth: Some(0.075),
                    hole_to_hole_via: Some(0.3),
                    hole_to_hole_pad: Some(0.3),
                    hole_to_copper_via: Some(0.2),
                    hole_to_copper_pth: Some(0.2),
                    hole_to_copper_npth: Some(0.2),
                    edge_routed: Some(0.3),
                    silk_line: Some(0.15),
                    silk_text_height: Some(1.0),
                    silk_to_pad: Some(0.15),
                    max_aspect_ratio: Some(20.0),
                    plating_allowance: Some(0.035),
                    ..FabRules::default()
                },
            ),
        },
        FabPreset {
            name: "Eurocircuits class 6",
            rules: preset_rules(
                "Eurocircuits class 6",
                "https://www.eurocircuits.com/pcb-design-guidelines/",
                "2026-03-28",
                FabRules {
                    min_track: Some(0.1),
                    min_space: Some(0.1),
                    min_drill: Some(0.25),
                    min_npth: Some(0.25),
                    min_ring_via: Some(0.05),
                    min_ring_pth: Some(0.075),
                    hole_to_hole_via: Some(0.25),
                    hole_to_hole_pad: Some(0.25),
                    hole_to_copper_via: Some(0.15),
                    hole_to_copper_pth: Some(0.15),
                    hole_to_copper_npth: Some(0.15),
                    edge_routed: Some(0.25),
                    silk_line: Some(0.125),
                    silk_text_height: Some(0.8),
                    silk_to_pad: Some(0.125),
                    max_aspect_ratio: Some(25.0),
                    plating_allowance: Some(0.025),
                    ..FabRules::default()
                },
            ),
        },
    ]
});
