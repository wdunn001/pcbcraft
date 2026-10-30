//! Minimum track width from net current and stackup, using the IPC-2221 curve.

use pcbcraft_calc::copper::{self, CopperLayer, TraceMethod, TraceWidth};
use pcbcraft_doc::{Layer, LayerRecord, Project};

/// Width advice for one net on one copper layer.
#[derive(Clone, Debug, PartialEq)]
pub struct WidthAdvice {
    /// Minimum width in millimetres.
    pub width_mm: f64,
    /// Which rule produced the width: `ipc2221`.
    pub method: &'static str,
    /// Set when a plane layer sits next to this copper row.
    pub plane_note: Option<&'static str>,
}

const PLANE_NOTE: &str =
    "A nearby plane layer can lower the temperature rise; this width assumes none.";

fn copper_weight_oz(project: &Project, layer: Layer) -> f64 {
    let idx = layer.index() as usize;
    project
        .copper_layer_records()
        .nth(idx)
        .and_then(|r| r.copper_weight_oz)
        .filter(|w| w.is_finite() && *w > 0.0)
        .unwrap_or(1.0)
}

fn layer_position(project: &Project, layer: Layer) -> CopperLayer {
    let name = project.layer_name(layer);
    match project.layer_side(&name).as_deref() {
        Some("top") | Some("bottom") => CopperLayer::External,
        _ => CopperLayer::Internal,
    }
}

fn record_is_plane(rec: &LayerRecord) -> bool {
    rec.copper_role.as_deref() == Some("plane")
        || rec.function.as_deref() == Some("plane")
        || rec.function.as_deref() == Some("mixed")
}

fn plane_beside_layer(project: &Project, layer: Layer) -> bool {
    let copper: Vec<_> = project.copper_layer_records().collect();
    let idx = layer.index() as usize;
    if idx > 0 {
        if let Some(rec) = copper.get(idx - 1) {
            if record_is_plane(rec) {
                return true;
            }
        }
    }
    if let Some(rec) = copper.get(idx + 1) {
        if record_is_plane(rec) {
            return true;
        }
    }
    false
}

/// Minimum width for a net on one copper layer from its current, rise and the
/// layer copper weight. Returns none when the net has no current.
pub fn width_advice_on_layer(
    project: &Project,
    net: pcbcraft_doc::Id,
    layer: Layer,
) -> Option<WidthAdvice> {
    let current = project.effective_current(net)?;
    let temperature_rise = project.effective_temperature_rise(net)?;
    let out = copper::trace_width(TraceWidth {
        current,
        temperature_rise,
        copper_weight: copper_weight_oz(project, layer),
        layer: layer_position(project, layer),
        method: TraceMethod::Ipc2221,
    })
    .ok()?;
    Some(WidthAdvice {
        width_mm: out.width_mm,
        method: out.method_used,
        plane_note: plane_beside_layer(project, layer).then_some(PLANE_NOTE),
    })
}

/// Minimum width for a net from the layers its tracks use. When tracks sit on
/// several layers, the widest requirement wins.
pub fn width_advice(project: &Project, net: pcbcraft_doc::Id) -> Option<WidthAdvice> {
    let mut layers = project
        .board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(|t| t.layer)
        .collect::<Vec<_>>();
    if layers.is_empty() {
        layers.push(Layer::Front);
    }
    layers.sort_by_key(|l| l.index());
    layers.dedup();
    let mut best: Option<WidthAdvice> = None;
    for layer in layers {
        let Some(advice) = width_advice_on_layer(project, net, layer) else {
            continue;
        };
        best = Some(match best {
            None => advice,
            Some(prev) if advice.width_mm > prev.width_mm => advice,
            Some(prev) if advice.width_mm < prev.width_mm => prev,
            Some(prev) => WidthAdvice {
                plane_note: prev.plane_note.or(advice.plane_note),
                ..prev
            },
        });
    }
    best
}
