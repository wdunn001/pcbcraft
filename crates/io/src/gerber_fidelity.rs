//! Gerber X2 aperture and file functions for fabrication fidelity (issue #655).
use pcbcraft_doc::{CopperZone, Footprint, Layer, Pad, Project, ViaCovering, ZoneConnect};
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

/// MD5 hex digest of `body`, lowercase, for `%TF.MD5`.
pub(crate) fn gerber_md5_hex(body: &str) -> String {
    format!("{:x}", md5::compute(body.as_bytes()))
}

/// Insert `%TF.MD5` before `M02*`; the hash covers the file without that attribute.
pub(crate) fn attach_gerber_md5(text: String) -> String {
    let marker = "M02*\n";
    let Some(prefix) = text.strip_suffix(marker) else {
        return text;
    };
    let hash = gerber_md5_hex(prefix);
    format!("{prefix}%TF.MD5,{hash}*%\n{marker}")
}

/// Check `%TF.MD5` against the file body (reader acceptance for issue #655).
pub fn verify_gerber_md5(text: &str) -> Result<(), String> {
    let marker = "M02*\n";
    let Some(full) = text.strip_suffix(marker) else {
        return Err("gerber file does not end with M02".into());
    };
    let md5_line = full
        .lines()
        .rev()
        .find(|line| line.contains("TF.MD5,"))
        .ok_or("gerber file has no MD5 attribute")?;
    let stated = md5_line
        .trim()
        .trim_start_matches('%')
        .strip_prefix("TF.MD5,")
        .and_then(|rest| rest.strip_suffix("*%"))
        .ok_or("gerber MD5 attribute is malformed")?;
    let body = full
        .lines()
        .filter(|line| !line.contains("TF.MD5,"))
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!("{body}\n");
    let expected = gerber_md5_hex(&body);
    if stated != expected {
        return Err("gerber MD5 does not match the file body".into());
    }
    Ok(())
}

/// True when the pad's mask layer defines copper (mask present, no explicit copper layer).
pub(crate) fn pad_mask_defined(fp: &Footprint, pad: &Pad) -> bool {
    let Some(g) = pad.geometry.as_ref() else {
        return false;
    };
    if g.pad_type != "smd" {
        return false;
    }
    let layers = fp.pad_geometry_layers(pad);
    let mask = layers
        .iter()
        .any(|l| l.ends_with(".Mask") || l == "*.Mask");
    let copper = layers
        .iter()
        .any(|l| l.ends_with(".Cu") || l == "*.Cu" || l == "F&B.Cu");
    mask && !copper
}

pub(crate) fn smd_aperture_function(fp: &Footprint, pad: &Pad) -> &'static str {
    if pad_mask_defined(fp, pad) {
        "SMDPad,SMDef"
    } else {
        "SMDPad,CuDef"
    }
}

pub(crate) fn pad_is_through(pad: &Pad) -> bool {
    pad.geometry.as_ref().is_some_and(|g| {
        g.pad_type == "thru_hole" || g.drill.is_some() || g.layers.iter().any(|l| l == "*.Cu")
    })
}

pub(crate) fn copper_layer_is_back(p: &Project, layer: Layer) -> bool {
    layer.index() + 1 == p.copper_layer_count()
}

pub(crate) fn copper_file_function(p: &Project, layer: Layer) -> String {
    let index = layer.index() + 1;
    let side = if layer.index() == 0 {
        "Top"
    } else if copper_layer_is_back(p, layer) {
        "Bot"
    } else {
        "Inr"
    };
    format!("Copper,L{index},{side}")
}

pub(crate) fn copper_gerber_basename(p: &Project, layer: Layer) -> String {
    let name = p.layer_name(layer);
    format!("pcb-{}.gbr", name.replace('.', "-"))
}

pub(crate) fn pad_copper_aperture_function(
    p: &Project,
    layer: Layer,
    fp: &Footprint,
    pad: &Pad,
) -> Option<&'static str> {
    if !fp.pad_on_copper(pad, layer) {
        return None;
    }
    let outer = layer.index() == 0 || copper_layer_is_back(p, layer);
    if pad_mask_defined(fp, pad) {
        return Some(smd_aperture_function(fp, pad));
    }
    if pad_is_through(pad) {
        if outer {
            return None;
        }
        return Some("ComponentPad");
    }
    if pad.geometry.as_ref().is_some_and(|g| g.pad_type == "smd" || g.drill.is_none()) {
        return Some(smd_aperture_function(fp, pad));
    }
    None
}

pub(crate) fn layer_copper_role(p: &Project, layer: Layer) -> Option<&str> {
    let name = p.layer_name(layer);
    p.board
        .layer_records
        .iter()
        .find(|r| r.name == name)
        .and_then(|r| r.copper_role.as_deref())
}

pub(crate) fn layer_is_plane(p: &Project, layer: Layer) -> bool {
    layer_copper_role(p, layer) == Some("plane")
}

/// Foreign-net clearance discs and same-net thermal pads to tag on plane pours.
pub(crate) fn plane_pour_aperture_flashes(
    p: &Project,
    zone: &CopperZone,
) -> Result<Vec<(Point, f64, &'static str)>, String> {
    if !layer_is_plane(p, zone.layer) {
        return Ok(Vec::new());
    }
    let settings = zone.pour_settings();
    let mut out = Vec::new();
    for v in &p.board.vias {
        if !p.via_span_layers(v).contains(&zone.layer) {
            continue;
        }
        if v.net == zone.net {
            if settings.via_connect == ZoneConnect::Relief {
                let r = v.diameter / 2.0 + settings.relief_gap;
                out.push((v.position, r * 2.0, "ThermalReliefPad"));
            }
            continue;
        }
        let clearance =
            p.required_clearance_at(zone.net, v.net, v.position, v.position, zone.layer, zone.layer);
        let d = (v.diameter + 2.0 * clearance).max(0.001);
        out.push((v.position, d, "AntiPad"));
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            if !f.pad_on_copper(pad, zone.layer) {
                continue;
            }
            let Some(at) = f.pad_position(&pad.number) else {
                continue;
            };
            let net = pad.net.unwrap_or(0);
            if net == zone.net {
                if settings.pad_connect == ZoneConnect::Relief {
                    let outline = pad.outline_points();
                    let mut max_r = 0.0f64;
                    for pt in &outline {
                        let d = at.distance(f.position.offset(pt.rotate(-f.rotation)));
                        max_r = max_r.max(d);
                    }
                    let d = (max_r * 2.0 + 2.0 * settings.relief_gap).max(0.001);
                    out.push((at, d, "ThermalReliefPad"));
                }
                continue;
            }
            let clearance =
                p.required_clearance_at(zone.net, net, at, at, zone.layer, zone.layer);
            let outline = pad.outline_points();
            let mut max_r = 0.0f64;
            for pt in &outline {
                let d = at.distance(f.position.offset(pt.rotate(-f.rotation)));
                max_r = max_r.max(d);
            }
            let d = (max_r * 2.0 + 2.0 * clearance).max(0.001);
            out.push((at, d, "AntiPad"));
        }
    }
    Ok(out)
}

/// Gerber file function for a mask-family stack row and its outward index (1 is outermost).
pub(crate) fn mask_row_file_function(kind: &str, side: &str, outward_index: u8) -> Option<String> {
    let face = match side {
        "bottom" => "Bot",
        "top" | _ => "Top",
    };
    let base = match kind {
        "soldermask" => "Soldermask",
        "peelable_mask" => "Peelablemask",
        "carbon_ink" => "Carbonmask",
        "gold_mask" => "Goldmask",
        "heatsink_mask" => "Heatsinkmask",
        "silver_mask" => "Silvermask",
        "tin_mask" => "Tinmask",
        _ => return None,
    };
    Some(format!("{base},{face},{outward_index}"))
}

fn mask_side(rec: &pcbcraft_doc::LayerRecord) -> &'static str {
    match rec.side.as_deref() {
        Some("bottom") => "bottom",
        Some("internal") => "top",
        _ => "top",
    }
}

fn mask_kind(rec: &pcbcraft_doc::LayerRecord) -> &str {
    match rec.kind.as_str() {
        "soldermask" => "soldermask",
        "custom" => rec.function.as_deref().unwrap_or("other"),
        other => other,
    }
}

/// Extra mask-layer Gerber files from the recorded stack (variants and outward index).
pub fn mask_stack_gerber_files(p: &Project) -> Result<BTreeMap<String, String>, String> {
    use crate::gerber::{gerber_mask_for_stack_row, DEFAULT_MASK_MARGIN};
    p.validate()?;
    let mut rows: Vec<(usize, &pcbcraft_doc::LayerRecord)> = p
        .board
        .layer_records
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            matches!(
                mask_kind(r),
                "soldermask"
                    | "peelable_mask"
                    | "carbon_ink"
                    | "gold_mask"
                    | "heatsink_mask"
                    | "silver_mask"
                    | "tin_mask"
            )
        })
        .collect();
    if rows.is_empty() {
        return Ok(BTreeMap::new());
    }
    rows.sort_by_key(|(i, r)| {
        let side = mask_side(r);
        (side, *i)
    });
    let mut top_count = 0u8;
    let mut bot_count = 0u8;
    let mut files = BTreeMap::new();
    for (_, rec) in rows {
        let side = mask_side(rec);
        let outward = if side == "bottom" {
            bot_count = bot_count.saturating_add(1);
            bot_count
        } else {
            top_count = top_count.saturating_add(1);
            top_count
        };
        let Some(function) = mask_row_file_function(mask_kind(rec), side, outward) else {
            continue;
        };
        let layer = if side == "bottom" {
            Layer::from_index(p.copper_layer_count().saturating_sub(1))?
        } else {
            Layer::Front
        };
        let stem = rec.name.replace('.', "-");
        let name = format!("pcb-{stem}.gbr");
        let text = gerber_mask_for_stack_row(p, layer, &function, DEFAULT_MASK_MARGIN)?;
        files.insert(name, text);
    }
    Ok(files)
}

/// True when any via is filled and needs a Viafill Gerber file.
pub fn board_needs_viafill(p: &Project) -> bool {
    p.board.vias.iter().any(|v| {
        matches!(
            v.effective_covering(&p.board),
            ViaCovering::Filled | ViaCovering::FilledCovered | ViaCovering::FilledCapped
        )
    })
}

/// Gerber Viafill layer: one flash per filled via.
pub fn gerber_viafill(p: &Project) -> Result<String, String> {
    use crate::gerber::gerber_viafill_layer;
    gerber_viafill_layer(p)
}

#[cfg(test)]
mod md5_tests {
    use super::gerber_md5_hex;

    #[test]
    fn md5_empty_string_matches_rfc_vector() {
        assert_eq!(
            gerber_md5_hex(""),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
    }
}
