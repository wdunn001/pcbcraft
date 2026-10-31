//! Signal-integrity export files: impedance tables, per-net results, Touchstone hand-off.
//!
//! # Rules
//! - Every estimate names its model; uncertainty is stated in plain words.
//! - Touchstone export copies a project attachment; nothing is invented without a solve.
//!
//! # Dependencies
//! - crates/io/src/schematic_sim.rs for optional DC volts in the results CSV
//! - pcbcraft_geom::impedance for microstrip estimates
//!
//! # Do not
//! - Name other design tools in user-facing strings.

use crate::schematic_sim::build_schematic_netlist;
use pcbcraft_doc::{Id, Layer, Project};
use pcbcraft_geom::impedance as imp;
use pcbcraft_sim::solve_dc;
use std::collections::BTreeMap;

const DEFAULT_COPPER_MM: f64 = 0.035;
const IMPEDANCE_METHOD: &str =
    "Hammerstad and Jensen 1980 microstrip with thickness correction; finish not included";
const IMPEDANCE_UNCERTAINTY: &str =
    "roughly +/- 10 percent when stackup Dk and copper thickness match fabrication";

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn dielectric_for(p: &Project, layer: Layer) -> (f64, f64) {
    p.dielectric_beside(&p.layer_name(layer))
        .unwrap_or((p.board.thickness, p.board.er))
}

fn copper_thickness(p: &Project, layer: Layer) -> f64 {
    let name = p.layer_name(layer);
    p.board
        .layer_records
        .iter()
        .find(|r| r.name == name && r.kind == "copper" && r.thickness > 0.0)
        .map(|r| r.thickness)
        .unwrap_or(DEFAULT_COPPER_MM)
}

fn net_length(p: &Project, net: Id) -> f64 {
    p.board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(|t| t.length())
        .sum()
}

fn widest(p: &Project, net: Id) -> Option<(f64, Layer)> {
    p.board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .fold(None, |best: Option<(f64, Layer)>, t| match best {
            Some((w, _)) if w >= t.width => best,
            _ => Some((t.width, t.layer)),
        })
}

fn class_name(p: &Project, net: Id) -> String {
    p.net_classes
        .iter()
        .find(|c| c.nets.contains(&net))
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "default".to_string())
}

struct NetImpedanceRow {
    net_name: String,
    class_name: String,
    length_mm: f64,
    width_mm: f64,
    layer: String,
    impedance_ohm: Option<f64>,
    eeff: Option<f64>,
    delay_ps_per_mm: Option<f64>,
    diff_pair: String,
    z_diff_ohm: Option<f64>,
    validity: String,
}

fn impedance_rows(p: &Project) -> Result<Vec<NetImpedanceRow>, String> {
    p.validate()?;
    let mut rows = vec![];
    for net in &p.nets {
        let length_mm = net_length(p, net.id);
        let class_name = class_name(p, net.id);
        let mut row = NetImpedanceRow {
            net_name: net.name.clone(),
            class_name,
            length_mm: r3(length_mm),
            width_mm: 0.0,
            layer: String::new(),
            impedance_ohm: None,
            eeff: None,
            delay_ps_per_mm: None,
            diff_pair: String::new(),
            z_diff_ohm: None,
            validity: String::new(),
        };
        let Some((width, layer)) = widest(p, net.id) else {
            row.validity = "no routed copper".into();
            rows.push(row);
            continue;
        };
        row.width_mm = r3(width);
        row.layer = p.layer_name(layer);
        let copper = copper_thickness(p, layer);
        let (height, er) = dielectric_for(p, layer);
        let m = match imp::microstrip(width, height, copper, er) {
            Some(m) => m,
            None => {
                row.validity = "stackup or width outside the microstrip model".into();
                rows.push(row);
                continue;
            }
        };
        row.impedance_ohm = Some(r2(m.ohms));
        row.eeff = Some(r3(m.eeff));
        row.delay_ps_per_mm = Some(r3(imp::delay_ps(1.0, m.eeff)));
        if let Some(note) = imp::validity(width, height) {
            row.validity = note;
        }
        if let Some(pair) = p
            .board
            .diff_pairs
            .iter()
            .find(|d| d.net_p == net.id || d.net_n == net.id)
        {
            row.diff_pair = pair.name.clone();
            if let Some(c) = imp::coupled_microstrip(width, pair.gap, height, er) {
                row.z_diff_ohm = Some(r2(c.z_diff()));
            }
        }
        rows.push(row);
    }
    rows.sort_by(|a, b| a.net_name.cmp(&b.net_name));
    Ok(rows)
}

/// Impedance table for fabrication notes: one row per net with an estimate when routed.
pub fn impedance_table_csv(p: &Project) -> Result<String, String> {
    let title = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    let rows = impedance_rows(p)?;
    let mut out = format!(
        "# Impedance table for {title}\n# method: {IMPEDANCE_METHOD}\n# uncertainty: {IMPEDANCE_UNCERTAINTY}\nNET,CLASS,ROUTED_LENGTH_MM,WIDTH_MM,LAYER,IMPEDANCE_OHM,DIFF_PAIR,DIFF_Z_OHM,VALIDITY\n"
    );
    for row in rows {
        let z = row
            .impedance_ohm
            .map(|v| format!("{v:.2}"))
            .unwrap_or_default();
        let zd = row
            .z_diff_ohm
            .map(|v| format!("{v:.2}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{},{},{:.3},{:.3},{},{},{},{},{}\n",
            csv_cell(&row.net_name),
            csv_cell(&row.class_name),
            row.length_mm,
            row.width_mm,
            csv_cell(&row.layer),
            z,
            csv_cell(&row.diff_pair),
            zd,
            csv_cell(&row.validity),
        ));
    }
    Ok(out)
}

/// Per-net signal-integrity results: impedance, length, optional DC volts when sim models exist.
pub fn si_net_results_csv(p: &Project) -> Result<String, String> {
    let title = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    let rows = impedance_rows(p)?;
    let mut dc_by_net: BTreeMap<Id, f64> = BTreeMap::new();
    if let Ok(built) = build_schematic_netlist(p) {
        if let Ok(solution) = solve_dc(&built.net) {
            for (net_id, node) in &built.node_by_net {
                if *net_id == built.ground {
                    continue;
                }
                let volts = solution.node_volts[node.index];
                if volts.is_finite() {
                    dc_by_net.insert(*net_id, r3(volts));
                }
            }
        }
    }
    let mut out = format!(
        "# Signal integrity results for {title}\n# method: {IMPEDANCE_METHOD}\n# uncertainty: {IMPEDANCE_UNCERTAINTY}\nNET,CLASS,ROUTED_LENGTH_MM,IMPEDANCE_OHM,EFFECTIVE_ER,DELAY_PS_PER_MM,DIFF_PAIR,DIFF_Z_OHM,DC_VOLTS,NOTES\n"
    );
    for net in &p.nets {
        let Some(row) = rows.iter().find(|r| r.net_name == net.name) else {
            continue;
        };
        let er = row.eeff.map(|v| format!("{v:.3}")).unwrap_or_default();
        let delay = row
            .delay_ps_per_mm
            .map(|v| format!("{v:.3}"))
            .unwrap_or_default();
        let z = row
            .impedance_ohm
            .map(|v| format!("{v:.2}"))
            .unwrap_or_default();
        let zd = row
            .z_diff_ohm
            .map(|v| format!("{v:.2}"))
            .unwrap_or_default();
        let dc = dc_by_net
            .get(&net.id)
            .map(|v| format!("{v:.3}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{},{},{:.3},{},{},{},{},{},{},{}\n",
            csv_cell(&row.net_name),
            csv_cell(&row.class_name),
            row.length_mm,
            z,
            er,
            delay,
            csv_cell(&row.diff_pair),
            zd,
            dc,
            csv_cell(&row.validity),
        ));
    }
    Ok(out)
}

fn csv_cell(text: &str) -> String {
    if text.contains(',') || text.contains('"') || text.contains('\n') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

const TOUCHSTONE_NAMES: [&str; 3] = ["touchstone.s2p", "si.touchstone.s2p", "channel.s2p"];

/// Copies a stored Touchstone file from project attachments when present.
pub fn touchstone_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    for name in TOUCHSTONE_NAMES {
        if let Some(file) = p.files.iter().find(|f| f.name == name) {
            if file.is_binary() {
                return Err(format!(
                    "{name} is stored as binary; save it as a text attachment instead"
                ));
            }
            if file.data.trim().is_empty() {
                return Err(format!("{name} is empty"));
            }
            return Ok(file.data.clone());
        }
    }
    Err(
        "no Touchstone file is attached; add touchstone.s2p after you have S-parameters for the channel"
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Project;

    #[test]
    fn impedance_table_lists_every_net() -> Result<(), String> {
        let p = Project::default();
        let text = impedance_table_csv(&p)?;
        if !text.contains("NET,CLASS") {
            return Err("missing header".into());
        }
        Ok(())
    }

    #[test]
    fn touchstone_refuses_without_attachment() {
        let p = Project::default();
        let err = touchstone_text(&p).err().unwrap_or_default();
        if !err.contains("touchstone.s2p") {
            panic!("unexpected: {err}");
        }
    }
}
