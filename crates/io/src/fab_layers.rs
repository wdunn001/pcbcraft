//! Shared copper-layer naming for fabrication exports (Gerber X2, ODB, IPC-2581).
use pcbcraft_doc::{Layer, Project};

/// True when coordinates for this copper layer should be mirrored for film polarity.
pub fn copper_mirror_for_fabrication(p: &Project, layer: Layer) -> bool {
    layer.index() + 1 == p.copper_layer_count()
}

/// Gerber X2 `%TF.FileFunction,...` value for one copper layer.
pub fn copper_gerber_file_function(p: &Project, layer: Layer) -> String {
    let position = layer.index() as u32 + 1;
    let count = p.copper_layer_count();
    if layer.index() == 0 {
        return format!("Copper,L{position},Top");
    }
    if layer.index() + 1 == count {
        return format!("Copper,L{position},Bot");
    }
    let role = p
        .copper_layer_records()
        .nth(layer.index() as usize)
        .and_then(|r| r.copper_role.as_deref())
        .unwrap_or("signal");
    let tag = match role {
        "plane" => "Plane",
        "mixed" => "Mixed",
        "hatched" => "Hatched",
        _ => "Signal",
    };
    format!("Copper,L{position},Inr,{tag}")
}

/// Zip entry name for a copper Gerber plot (legacy `pcb-F.Cu.gbr` on two-layer boards).
pub fn copper_gerber_zip_name(p: &Project, layer: Layer) -> String {
    format!("pcb-{}.gbr", p.layer_name(layer))
}

/// G04 comment line for a copper Gerber file.
pub fn copper_gerber_comment(p: &Project, layer: Layer) -> String {
    if p.copper_layer_count() <= 2 && layer.index() == 0 {
        "front copper layer".into()
    } else if p.copper_layer_count() <= 2 && layer.index() + 1 == p.copper_layer_count() {
        "back copper layer".into()
    } else {
        format!(
            "{} copper layer",
            copper_fabrication_label(p, layer).to_ascii_lowercase()
        )
    }
}

/// Plain-English copper label for stackup and IPC (Top, Inner 1, Inner 2, Bottom).
pub fn copper_fabrication_label(p: &Project, layer: Layer) -> String {
    let count = p.copper_layer_count();
    if layer.index() == 0 {
        "Top".into()
    } else if layer.index() + 1 == count {
        "Bottom".into()
    } else {
        format!("Inner {}", layer.index())
    }
}

/// Whether a pad geometry should appear on the given copper layer in plots.
pub fn pad_on_copper_layer(p: &Project, pad: &pcbcraft_doc::Pad, layer: Layer) -> bool {
    let Some(g) = pad.geometry.as_ref() else {
        return false;
    };
    if g.pad_type == "unplated" {
        return false;
    }
    let through = g.pad_type == "thru_hole" || g.drill.is_some();
    if through {
        return true;
    }
    let bottom = p.copper_layer_count().saturating_sub(1);
    let on_back = !g.layers.is_empty() && !g.layers.iter().any(|l| l.starts_with("F."));
    if layer.index() == 0 {
        !on_back
    } else if layer.index() == bottom {
        on_back
    } else {
        false
    }
}

/// Excellon attribute line for through-hole plating spanning the whole stack.
pub fn through_plating_line(p: &Project) -> String {
    let top = 1;
    let bottom = p.copper_layer_count();
    format!("Plated,{top},{bottom},PTH")
}

/// Excellon attribute line for unplated holes through the whole stack.
pub fn npth_plating_line(p: &Project) -> String {
    let top = 1;
    let bottom = p.copper_layer_count();
    format!("NonPlated,{top},{bottom},NPTH")
}

/// ODB++ matrix layer suffix: GTL top, GBL bottom, GP inner.
pub fn odb_layer_suffix(p: &Project, layer: Layer) -> &'static str {
    if layer.index() == 0 {
        "GTL"
    } else if layer.index() + 1 == p.copper_layer_count() {
        "GBL"
    } else {
        "GP"
    }
}

pub fn odb_features_key(p: &Project, layer: Layer) -> String {
    if p.copper_layer_count() <= 2 && layer.index() == 0 {
        "top".into()
    } else if p.copper_layer_count() <= 2 && layer.index() == 1 {
        "bottom".into()
    } else {
        p.layer_name(layer).replace('.', "_").to_ascii_lowercase()
    }
}
