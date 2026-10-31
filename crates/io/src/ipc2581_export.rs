//! Schema-valid IPC-2581C assembly exchange export (issues #660 and #661).
mod ipc2581_assembly;

use crate::board_mesh::{COPPER_THICKNESS, MASK_THICKNESS, SILK_THICKNESS};
use ipc2581_assembly::{collect_enterprises, AssemblyMeta};
use pcbcraft_doc::{Layer, LayerRecord, Project, Track};
use pcbcraft_geom::Point as GeomPoint;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

const NS: &str = "http://webstds.ipc.org/2581";
const MAX_ELEMENTS: usize = 50_000;

pub fn ipc2581_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    let mut w = Writer::new();
    w.write_document(p)?;
    Ok(w.finish())
}

pub(super) struct Writer {
    out: String,
    elements: usize,
}

impl Writer {
    fn new() -> Self {
        Self {
            out: String::new(),
            elements: 0,
        }
    }

    fn finish(self) -> String {
        self.out
    }

    fn count(&mut self, n: usize) -> Result<(), String> {
        self.elements += n;
        if self.elements > MAX_ELEMENTS {
            return Err("IPC-2581 export exceeds the element limit".into());
        }
        Ok(())
    }

    pub(super) fn push(&mut self, s: &str) -> Result<(), String> {
        self.count(s.matches('<').count())?;
        self.out.push_str(s);
        Ok(())
    }

    fn write_document(&mut self, p: &Project) -> Result<(), String> {
        let assembly = AssemblyMeta::plan(p);
        let enterprises = collect_enterprises(p, &assembly);
        self.push("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n")?;
        self.push(&format!(
            "<IPC-2581 revision=\"C\" xmlns=\"{NS}\">\n"
        ))?;
        self.write_content(&assembly)?;
        self.write_logistic_header(&enterprises)?;
        self.write_history_record(p)?;
        self.push("<Ecad>\n")?;
        self.write_cad_header(p)?;
        self.write_cad_data(p, &assembly)?;
        self.push("</Ecad>\n")?;
        self.write_assembly_root(p, &assembly)?;
        self.push("</IPC-2581>\n")?;
        Ok(())
    }

    fn write_content(&mut self, assembly: &AssemblyMeta) -> Result<(), String> {
        self.push("<Content roleRef=\"Owner\">\n")?;
        self.push("<FunctionMode mode=\"FABRICATION\"/>\n")?;
        self.write_content_assembly_refs(assembly)?;
        self.push("<DictionaryColor/>\n")?;
        self.push("<DictionaryLineDesc units=\"MILLIMETER\">\n")?;
        self.push("<EntryLineDesc id=\"trace\">\n")?;
        self.push("<LineDesc lineWidth=\"0.25\" lineEnd=\"ROUND\"/>\n")?;
        self.push("</EntryLineDesc>\n")?;
        self.push("</DictionaryLineDesc>\n")?;
        self.push("<DictionaryFillDesc units=\"MILLIMETER\"/>\n")?;
        self.push("<DictionaryStandard units=\"MILLIMETER\">\n")?;
        self.push("<EntryStandard id=\"round_pad\">\n")?;
        self.push("<Circle diameter=\"1.0\"/>\n")?;
        self.push("</EntryStandard>\n")?;
        self.push("<EntryStandard id=\"round_hole\">\n")?;
        self.push("<Circle diameter=\"0.3\"/>\n")?;
        self.push("</EntryStandard>\n")?;
        self.push("</DictionaryStandard>\n")?;
        self.push("<DictionaryUser units=\"MILLIMETER\"/>\n")?;
        self.push("</Content>\n")?;
        Ok(())
    }

    fn write_logistic_header(&mut self, enterprises: &BTreeMap<String, String>) -> Result<(), String> {
        self.push("<LogisticHeader>\n")?;
        self.push("<Role id=\"Owner\" roleFunction=\"OWNER\"/>\n")?;
        self.push("<Enterprise id=\"PCBCraft\" name=\"PCBCraft\" code=\"PCBCraft\"/>\n")?;
        for (id, name) in enterprises {
            self.push(&format!(
                "<Enterprise id=\"{}\" name=\"{}\" code=\"{}\"/>\n",
                xml_escape(id),
                xml_escape(name),
                xml_escape(id)
            ))?;
        }
        self.push(
            "<Person name=\"Exporter\" enterpriseRef=\"PCBCraft\" roleRef=\"Owner\"/>\n",
        )?;
        self.push("</LogisticHeader>\n")?;
        Ok(())
    }

    fn write_history_record(&mut self, _p: &Project) -> Result<(), String> {
        let stamp = "2020-01-01T00:00:00";
        self.push(&format!(
            "<HistoryRecord number=\"1\" origination=\"{stamp}\" software=\"PCBCraft\" lastChange=\"{stamp}\">\n"
        ))?;
        self.push("<FileRevision fileRevisionId=\"1\" comment=\"export\">\n")?;
        self.push("<SoftwarePackage name=\"PCBCraft\" vendor=\"PCBCraft\" revision=\"0.1.0\">\n")?;
        self.push(
            "<Certification certificationStatus=\"SELFTEST\" certificationCategory=\"BOARDFABRICATION\"/>\n",
        )?;
        self.push("</SoftwarePackage>\n")?;
        self.push("</FileRevision>\n")?;
        self.push("</HistoryRecord>\n")?;
        Ok(())
    }

    fn write_cad_header(&mut self, p: &Project) -> Result<(), String> {
        self.push("<CadHeader units=\"MILLIMETER\">\n")?;
        for row in stackup_rows(p) {
            if let Some(spec) = spec_for_layer(&row) {
                self.push(&spec)?;
            }
        }
        self.push("</CadHeader>\n")?;
        Ok(())
    }

    fn write_cad_data(&mut self, p: &Project, assembly: &AssemblyMeta) -> Result<(), String> {
        let rows = stackup_rows(p);
        let drill_layers = drill_span_layers(p)?;
        self.push("<CadData>\n")?;
        for row in &rows {
            self.write_layer_row(p, row)?;
        }
        for span in &drill_layers {
            self.write_drill_layer(span)?;
        }
        self.write_stackup(p, &rows)?;
        self.write_step(p, &rows, &drill_layers, assembly)?;
        self.push("</CadData>\n")?;
        Ok(())
    }

    fn write_layer_row(&mut self, p: &Project, row: &LayerRecord) -> Result<(), String> {
        let (function, side) = ipc_layer_attrs(p, row);
        let polarity = row.polarity().to_uppercase();
        self.push(&format!(
            "<Layer name=\"{}\" layerFunction=\"{function}\" side=\"{side}\" polarity=\"{polarity}\">\n",
            xml_escape(&row.name)
        ))?;
        if let Some(spec_id) = spec_id_for_layer(row) {
            self.push(&format!("<SpecRef id=\"{spec_id}\"/>\n"));
        }
        self.push("</Layer>\n")?;
        Ok(())
    }

    fn write_drill_layer(&mut self, span: &DrillSpanLayer) -> Result<(), String> {
        self.push(&format!(
            "<Layer name=\"{}\" layerFunction=\"DRILL\" side=\"ALL\" polarity=\"POSITIVE\">\n",
            xml_escape(&span.layer_name)
        ))?;
        self.push(&format!(
            "<Span fromLayer=\"{}\" toLayer=\"{}\"/>\n",
            xml_escape(&span.from_name),
            xml_escape(&span.to_name)
        ))?;
        self.push("</Layer>\n")?;
        Ok(())
    }

    fn write_stackup(&mut self, p: &Project, rows: &[LayerRecord]) -> Result<(), String> {
        let total: f64 = rows.iter().map(|r| r.thickness).sum();
        self.push(&format!(
            "<Stackup name=\"primary\" overallThickness=\"{total:.6}\" tolPlus=\"0\" tolMinus=\"0\" whereMeasured=\"METAL\" stackupStatus=\"PROPOSED\">\n"
        ))?;
        for (seq, row) in rows.iter().enumerate() {
            let spec = spec_id_for_layer(row);
            self.push(&format!(
                "<StackupLayer layerOrGroupRef=\"{}\" thickness=\"{:.6}\" tolPlus=\"0\" tolMinus=\"0\" sequence=\"{}\">\n",
                xml_escape(&row.name),
                row.thickness,
                seq + 1
            ))?;
            if let Some(id) = spec {
                self.push(&format!("<SpecRef id=\"{id}\"/>\n"));
            }
            self.push("</StackupLayer>\n")?;
        }
        self.push("</Stackup>\n")?;
        let _ = p;
        Ok(())
    }

    fn write_step(
        &mut self,
        p: &Project,
        rows: &[LayerRecord],
        drill_layers: &[DrillSpanLayer],
        assembly: &AssemblyMeta,
    ) -> Result<(), String> {
        self.push("<Step name=\"Board\">\n")?;
        self.write_padstacks(p)?;
        self.push("<Datum x=\"0\" y=\"0\"/>\n")?;
        self.write_profile(p)?;
        self.write_step_assembly(p, assembly)?;
        self.write_logical_nets(p)?;
        self.write_phy_nets(p)?;
        for row in rows {
            if row.kind == "copper" {
                self.write_copper_features(p, row)?;
            }
        }
        for span in drill_layers {
            self.write_drill_features(p, span)?;
        }
        self.push("</Step>\n")?;
        Ok(())
    }

    fn write_padstacks(&mut self, p: &Project) -> Result<(), String> {
        self.push("<PadStackDef name=\"via\">\n")?;
        self.push(
            "<PadstackHoleDef name=\"via_hole\" diameter=\"0.3\" platingStatus=\"VIA\" plusTol=\"0\" minusTol=\"0\" x=\"0\" y=\"0\"/>\n",
        )?;
        for layer in p.copper_layers() {
            let name = p.layer_name(layer);
            self.push(&format!(
                "<PadstackPadDef layerRef=\"{}\" padUse=\"REGULAR\">\n",
                xml_escape(&name)
            ))?;
            self.push("<Location x=\"0\" y=\"0\"/>\n")?;
            self.push("<StandardPrimitiveRef id=\"round_pad\"/>\n")?;
            self.push("</PadstackPadDef>\n")?;
        }
        self.push("</PadStackDef>\n")?;
        self.push("<PadStackDef name=\"via_antipad\">\n")?;
        self.push(
            "<PadstackHoleDef name=\"via_hole\" diameter=\"0.3\" platingStatus=\"VIA\" plusTol=\"0\" minusTol=\"0\" x=\"0\" y=\"0\"/>\n",
        )?;
        for layer in p.copper_layers() {
            let name = p.layer_name(layer);
            self.push(&format!(
                "<PadstackPadDef layerRef=\"{}\" padUse=\"ANTIPAD\">\n",
                xml_escape(&name)
            ))?;
            self.push("<Location x=\"0\" y=\"0\"/>\n")?;
            self.push("<StandardPrimitiveRef id=\"round_pad\"/>\n")?;
            self.push("</PadstackPadDef>\n")?;
        }
        self.push("</PadStackDef>\n")?;
        Ok(())
    }

    fn write_profile(&mut self, p: &Project) -> Result<(), String> {
        let outline: Vec<GeomPoint> = match &p.board.outline {
            Some(o) if o.len() >= 3 => o.clone(),
            _ => vec![
                GeomPoint::new(0.0, 0.0),
                GeomPoint::new(p.board.width, 0.0),
                GeomPoint::new(p.board.width, p.board.height),
                GeomPoint::new(0.0, p.board.height),
                GeomPoint::new(0.0, 0.0),
            ],
        };
        self.push("<Profile>\n<Contour>\n<Polygon>\n")?;
        for (i, pt) in outline.iter().enumerate() {
            if i == 0 {
                self.push(&format!(
                    "<PolyBegin x=\"{:.6}\" y=\"{:.6}\"/>\n",
                    pt.x, pt.y
                ))?;
            } else {
                self.push(&format!(
                    "<PolyStepSegment x=\"{:.6}\" y=\"{:.6}\"/>\n",
                    pt.x, pt.y
                ))?;
            }
        }
        self.push("</Polygon>\n</Contour>\n</Profile>\n")?;
        Ok(())
    }

    fn write_logical_nets(&mut self, p: &Project) -> Result<(), String> {
        let names: BTreeMap<u64, String> = p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
        let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
            p.components.iter().map(|c| (c.id, c)).collect();
        let mut pins: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for f in &p.board.footprints {
            let Some(c) = refs.get(&f.component) else {
                continue;
            };
            for pad in &f.pads {
                let Some(net) = pad.net else { continue };
                let Some(net_name) = names.get(&net) else {
                    return Err("pad references an unknown net".into());
                };
                pins.entry(net_name.clone())
                    .or_default()
                    .push((c.reference.clone(), pad.number.clone()));
            }
        }
        for c in &p.components {
            for pin in &c.pins {
                let Some(net) = pin.net else { continue };
                let Some(net_name) = names.get(&net) else {
                    return Err("pin references an unknown net".into());
                };
                let entry = (c.reference.clone(), pin.number.clone());
                pins.entry(net_name.clone())
                    .or_default()
                    .push(entry);
            }
        }
        for (name, mut list) in pins {
            list.sort();
            list.dedup();
            self.push(&format!(
                "<LogicalNet name=\"{}\">\n",
                xml_escape(&name)
            ))?;
            for (reference, pin) in list {
                self.push(&format!(
                    "<PinRef componentRef=\"{}\" pin=\"{}\"/>\n",
                    xml_escape(&reference),
                    xml_escape(&pin)
                ))?;
            }
            self.push("</LogicalNet>\n")?;
        }
        Ok(())
    }

    fn write_phy_nets(&mut self, p: &Project) -> Result<(), String> {
        let names: BTreeMap<u64, String> = p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
        let mut by_net: BTreeMap<String, Vec<&Track>> = BTreeMap::new();
        for track in &p.board.tracks {
            let Some(name) = names.get(&track.net) else {
                return Err("track references an unknown net".into());
            };
            by_net.entry(name.clone()).or_default().push(track);
        }
        if by_net.is_empty() {
            return Ok(());
        }
        self.push("<PhyNetGroup name=\"copper\">\n")?;
        for (name, tracks) in by_net {
            self.push(&format!("<PhyNet name=\"{}\">\n", xml_escape(&name)))?;
            for track in tracks {
                let layer_name = p.layer_name(track.layer);
                for (i, (x, y, node)) in track_phy_points(track)
                    .into_iter()
                    .enumerate()
                {
                    let _ = i;
                    self.push(&format!(
                        "<PhyNetPoint x=\"{x:.6}\" y=\"{y:.6}\" layerRef=\"{}\" netNode=\"{node}\" exposure=\"EXPOSED\">\n",
                        xml_escape(&layer_name)
                    ))?;
                    self.push("<StandardPrimitiveRef id=\"round_pad\"/>\n")?;
                    self.push("</PhyNetPoint>\n")?;
                }
            }
            self.push("</PhyNet>\n")?;
        }
        self.push("</PhyNetGroup>\n")?;
        Ok(())
    }

    fn write_copper_features(&mut self, p: &Project, row: &LayerRecord) -> Result<(), String> {
        let layer = p.resolve_copper_layer(&row.name)?;
        let names: BTreeMap<u64, String> = p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
        let tracks: Vec<&Track> = p
            .board
            .tracks
            .iter()
            .filter(|t| t.layer == layer)
            .collect();
        if tracks.is_empty() {
            return Ok(());
        }
        self.push(&format!(
            "<LayerFeature layerRef=\"{}\">\n",
            xml_escape(&row.name)
        ))?;
        let mut by_net: BTreeMap<String, Vec<&Track>> = BTreeMap::new();
        for t in tracks {
            let name = names
                .get(&t.net)
                .ok_or("track references an unknown net")?
                .clone();
            by_net.entry(name).or_default().push(t);
        }
        for (net, list) in by_net {
            self.push(&format!("<Set net=\"{}\">\n", xml_escape(&net)))?;
            for track in list {
                self.write_track_polyline(track)?;
            }
            self.push("</Set>\n")?;
        }
        self.push("</LayerFeature>\n")?;
        Ok(())
    }

    fn write_track_polyline(&mut self, track: &Track) -> Result<(), String> {
        self.push("<Polyline>\n")?;
        self.push(&format!(
            "<PolyBegin x=\"{:.6}\" y=\"{:.6}\"/>\n",
            track.start.x, track.start.y
        ))?;
        if let Some(arc) = track.curve() {
            self.push(&format!(
                "<PolyStepCurve x=\"{:.6}\" y=\"{:.6}\" centerX=\"{:.6}\" centerY=\"{:.6}\" clockwise=\"{}\"/>\n",
                track.end.x,
                track.end.y,
                arc.center.x,
                arc.center.y,
                if arc.sweep < 0.0 { "true" } else { "false" }
            ))?;
        } else {
            self.push(&format!(
                "<PolyStepSegment x=\"{:.6}\" y=\"{:.6}\"/>\n",
                track.end.x,
                track.end.y
            ))?;
        }
        self.push("<LineDescRef id=\"trace\"/>\n")?;
        self.push("</Polyline>\n")?;
        Ok(())
    }

    fn write_drill_features(&mut self, p: &Project, span: &DrillSpanLayer) -> Result<(), String> {
        let names: BTreeMap<u64, String> = p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
        self.push(&format!(
            "<LayerFeature layerRef=\"{}\">\n",
            xml_escape(&span.layer_name)
        ))?;
        self.push("<Set>\n")?;
        let mut hole_id = 0usize;
        for via in &p.board.vias {
            let (from, to) = p.via_span(via);
            let from_name = p.layer_name(from);
            let to_name = p.layer_name(to);
            if from_name != span.from_name || to_name != span.to_name {
                continue;
            }
            hole_id += 1;
            let padstack = if p.via_kind(via) == pcbcraft_doc::ViaKind::Buried
                || p.via_kind(via) == pcbcraft_doc::ViaKind::Blind
            {
                "via_antipad"
            } else {
                "via"
            };
            let _net = names.get(&via.net).map(String::as_str).unwrap_or("");
            self.push(&format!(
                "<Hole name=\"V{hole_id}\" diameter=\"{:.6}\" platingStatus=\"VIA\" plusTol=\"0\" minusTol=\"0\" x=\"{:.6}\" y=\"{:.6}\" padstackDefRef=\"{padstack}\"/>\n",
                via.drill.max(0.0),
                via.position.x,
                via.position.y
            ))?;
        }
        for hole in &p.board.holes {
            hole_id += 1;
            self.push(&format!(
                "<Hole name=\"M{hole_id}\" diameter=\"{:.6}\" platingStatus=\"NONPLATED\" plusTol=\"0\" minusTol=\"0\" x=\"{:.6}\" y=\"{:.6}\"/>\n",
                hole.diameter,
                hole.position.x,
                hole.position.y
            ))?;
        }
        self.push("</Set>\n")?;
        self.push("</LayerFeature>\n")?;
        Ok(())
    }
}

struct DrillSpanLayer {
    layer_name: String,
    from_name: String,
    to_name: String,
}

fn drill_span_layers(p: &Project) -> Result<Vec<DrillSpanLayer>, String> {
    let mut spans: BTreeSet<(String, String)> = BTreeSet::new();
    for via in &p.board.vias {
        let (from, to) = p.via_span(via);
        spans.insert((p.layer_name(from), p.layer_name(to)));
    }
    if spans.is_empty() && (!p.board.holes.is_empty() || p.board.vias.is_empty()) {
        let top = p.layer_name(Layer::Front);
        let bottom = p.layer_name(
            Layer::from_index(p.copper_layer_count().saturating_sub(1))
                .unwrap_or(Layer::Back),
        );
        spans.insert((top, bottom));
    }
    let mut out = Vec::new();
    for (from_name, to_name) in spans {
        let layer_name = drill_layer_name(&from_name, &to_name);
        out.push(DrillSpanLayer {
            layer_name,
            from_name,
            to_name,
        });
    }
    Ok(out)
}

fn drill_layer_name(from: &str, to: &str) -> String {
    format!(
        "DRILL_{}_{}",
        from.replace('.', "_"),
        to.replace('.', "_")
    )
}

fn stackup_rows(p: &Project) -> Vec<LayerRecord> {
    if !p.board.layer_records.is_empty() {
        return p.board.layer_records.clone();
    }
    synthesized_stackup(p)
}

fn synthesized_stackup(p: &Project) -> Vec<LayerRecord> {
    let copper = p.copper_layer_count().max(2);
    let gaps = (copper - 1).max(1) as f64;
    let dielectric =
        ((p.board.thickness - COPPER_THICKNESS * copper as f64) / gaps).max(0.0);
    let mut rows = Vec::new();
    rows.push(LayerRecord {
        name: "F.SilkS".into(),
        kind: "silkscreen".into(),
        thickness: SILK_THICKNESS,
        ..LayerRecord::default()
    });
    rows.push(LayerRecord {
        name: "F.Mask".into(),
        kind: "soldermask".into(),
        thickness: MASK_THICKNESS,
        ..LayerRecord::default()
    });
    for layer in 0..copper {
        let name = p.layer_name(Layer::from_index(layer as u8).unwrap_or(Layer::Front));
        rows.push(LayerRecord {
            name,
            kind: "copper".into(),
            thickness: COPPER_THICKNESS,
            copper_role: Some("signal".into()),
            ..LayerRecord::default()
        });
        if layer + 1 < copper {
            rows.push(LayerRecord {
                name: format!("Prepreg_{layer}"),
                kind: "prepreg".into(),
                thickness: dielectric,
                material: Some("FR4".into()),
                dk: Some(p.board.er),
                ..LayerRecord::default()
            });
        }
    }
    rows.push(LayerRecord {
        name: "B.Mask".into(),
        kind: "soldermask".into(),
        thickness: MASK_THICKNESS,
        ..LayerRecord::default()
    });
    rows.push(LayerRecord {
        name: "B.SilkS".into(),
        kind: "silkscreen".into(),
        thickness: SILK_THICKNESS,
        ..LayerRecord::default()
    });
    rows
}

fn ipc_layer_attrs(p: &Project, row: &LayerRecord) -> (&'static str, &'static str) {
    let function = match row.kind.as_str() {
        "copper" => match row.function().as_str() {
            "plane" => "PLANE",
            "mixed" => "MIXED",
            _ => "SIGNAL",
        },
        "core" => "DIELCORE",
        "prepreg" => "DIELPREG",
        "soldermask" => "SOLDERMASK",
        "silkscreen" => "LEGEND",
        "paste" => "SOLDERPASTE",
        _ => "OTHER",
    };
    let side = if let Some(s) = row.side.as_deref() {
        match s {
            "top" => "TOP",
            "bottom" => "BOTTOM",
            "internal" => "INTERNAL",
            "both" => "BOTH",
            "none" => "NONE",
            _ => "INTERNAL",
        }
    } else {
        match row.kind.as_str() {
            "copper" => {
                if row.name.starts_with('F') || row.name == "F.Cu" {
                    "TOP"
                } else if row.name.starts_with('B') || row.name == "B.Cu" {
                    "BOTTOM"
                } else {
                    "INTERNAL"
                }
            }
            "soldermask" | "silkscreen" | "paste" => {
                if row.name.starts_with('F') {
                    "TOP"
                } else {
                    "BOTTOM"
                }
            }
            _ => "INTERNAL",
        }
    };
    let _ = p;
    (function, side)
}

fn spec_id_for_layer(row: &LayerRecord) -> Option<String> {
    if row.is_dielectric() || row.material.is_some() || row.dk.is_some() || row.df.is_some() {
        Some(format!("spec_{}", sanitize_id(&row.name)))
    } else if row.kind == "copper" && row.copper_weight_oz.is_some() {
        Some(format!("spec_{}", sanitize_id(&row.name)))
    } else {
        None
    }
}

fn spec_for_layer(row: &LayerRecord) -> Option<String> {
    let id = spec_id_for_layer(row)?;
    let mut block = format!("<Spec name=\"{id}\">\n");
    if row.is_dielectric() {
        if let Some(dk) = row.dk() {
            let _ = write!(
                block,
                "<Dielectric type=\"DIELECTRIC_CONSTANT\"><Property value=\"{dk:.6}\"/></Dielectric>\n"
            );
        }
        if let Some(df) = row.df() {
            let _ = write!(
                block,
                "<Loss type=\"LOSS_TANGENT\"><Property value=\"{df:.6}\"/></Loss>\n"
            );
        }
    }
    if row.kind == "copper" {
        if let Some(oz) = row.copper_weight_oz {
            let _ = write!(
                block,
                "<Conductor type=\"WEIGHT\"><Property value=\"{oz:.6}\"/></Conductor>\n"
            );
        }
    }
    block.push_str("</Spec>\n");
    Some(block)
}

pub(super) fn sanitize_id(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn track_phy_points(track: &Track) -> Vec<(f64, f64, &'static str)> {
    vec![
        (track.start.x, track.start.y, "END"),
        (track.end.x, track.end.y, "END"),
    ]
}

pub(super) fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}
