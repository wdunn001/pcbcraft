//! Read a bracketed board interchange file into a native project.
use pcbcraft_doc::{
    Component, CopperZone, Cutout, Footprint, Id, Layer, Net, Pad, Pin, PreservedSource, Project,
    Track, Via,
};
use pcbcraft_geom::Point;
use pcbcraft_library::{Error as LibraryError, Node, line_number_at_offset, parse, read_footprint};
use std::collections::BTreeMap;

const MAX_NETS: usize = 512;
const MAX_COMPONENTS: usize = 512;

pub fn read_board_interchange(text: &str) -> Result<Project, String> {
    let root = parse(text).map_err(|e| format_parse_error(text, e))?;
    if root.name() != "kicad_pcb" {
        return Err("expected a board interchange file".into());
    }
    let version = root.field("version");
    if !version.is_empty() && version.parse::<u32>().is_err() {
        return Err(format!("unsupported board file version {version:?}"));
    }
    let mut ctx = ImportCtx::new(text);
    let mut board = Project::default().board;
    let mut net_names: BTreeMap<u32, String> = BTreeMap::new();
    net_names.insert(0, String::new());

    for child in &root.children {
        match child.name() {
            "general" => {
                let t = child.field("thickness");
                if !t.is_empty() {
                    board.thickness = parse_f64(t, text, child.span.start)?;
                }
            }
            "layers" => ctx.check_layers(child)?,
            "setup" => ctx.read_setup(child, &mut board)?,
            "aux_axis_origin" => {
                board.drill_origin = parse_at(child, text)?;
            }
            "net" => {
                let ord = parse_u32(child.value(1), text, child.span.start)?;
                let name = child.value(2).to_owned();
                if ord > MAX_NETS as u32 {
                    return Err("net ordinal exceeds limit".into());
                }
                net_names.insert(ord, name);
            }
            "footprint" | "module" => {
                ctx.import_footprint(child, &mut board)?;
            }
            "segment" => {
                ctx.import_segment(child, &mut board, false)?;
            }
            "arc" => {
                ctx.import_segment(child, &mut board, true)?;
            }
            "via" => {
                ctx.import_via(child, &mut board)?;
            }
            "zone" => {
                ctx.import_zone(child, &mut board)?;
            }
            "gr_rect" if child.field("layer") == "Edge.Cuts" => {
                ctx.import_rect_outline(child, &mut board)?;
            }
            "gr_poly" if child.field("layer") == "Edge.Cuts" => {
                ctx.import_poly_outline(child, &mut board)?;
            }
            "gr_circle" if child.field("layer") == "Edge.Cuts" => {
                ctx.import_circle_cutout(child, &mut board)?;
            }
            "paper" | "title_block" | "plot" | "embedded_fonts" | "generator"
            | "generator_version" | "version" => {
                board.preserved.push_span(text, child.span.clone())?;
            }
            _ => {
                if child.atom.is_none() {
                    board.preserved.push_span(text, child.span.clone())?;
                }
            }
        }
    }

    ctx.finish_nets(net_names, &mut board)?;
    sync_footprint_pin_nets(&mut ctx.components, &board.footprints);
    let project = Project {
        name: "Imported board".into(),
        unit: "mm".into(),
        components: ctx.components,
        nets: ctx.nets,
        board,
        ..Project::default()
    };
    project.validate()?;
    Ok(project)
}

struct ImportCtx {
    source: String,
    components: Vec<Component>,
    nets: Vec<Net>,
    next_component: Id,
    refs: BTreeMap<String, Id>,
}

impl ImportCtx {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            components: Vec::new(),
            nets: Vec::new(),
            next_component: 1,
            refs: BTreeMap::new(),
        }
    }

    fn finish_nets(
        &mut self,
        names: BTreeMap<u32, String>,
        board: &mut pcbcraft_doc::Board,
    ) -> Result<(), String> {
        const NET_ID_BASE: Id = 10_000;
        self.nets.clear();
        let mut ord_to_id: BTreeMap<u32, Id> = BTreeMap::new();
        for (ord, name) in names {
            if ord == 0 {
                continue;
            }
            if name.is_empty() {
                return Err(format!("net {ord} needs a name"));
            }
            let id = NET_ID_BASE.saturating_add(ord as Id);
            if id > u32::MAX as u64 {
                return Err("net id out of range".into());
            }
            ord_to_id.insert(ord, id);
            self.nets.push(Net {
                id,
                name,
                target_length: None,
                length_tolerance: None,
            });
        }
        let map_net = |raw: Id| -> Id { ord_to_id.get(&(raw as u32)).copied().unwrap_or(0) };
        for track in &mut board.tracks {
            track.net = map_net(track.net);
        }
        for via in &mut board.vias {
            via.net = map_net(via.net);
        }
        for zone in &mut board.zones {
            zone.net = map_net(zone.net);
        }
        for footprint in &mut board.footprints {
            for pad in &mut footprint.pads {
                if let Some(net) = pad.net {
                    pad.net = Some(map_net(net));
                }
            }
        }
        Ok(())
    }

    fn check_layers(&self, node: &Node) -> Result<(), String> {
        for entry in node.children.iter().filter(|c| c.atom.is_none()) {
            let layer_name = entry.value(2).trim_matches('"');
            if layer_name == "F.Cu" || layer_name == "B.Cu" {
                continue;
            }
            if layer_name.ends_with(".Cu") {
                return Err(
                    "inner copper layers are not supported yet; use a two-layer board".into(),
                );
            }
        }
        Ok(())
    }

    fn read_setup(&self, node: &Node, board: &mut pcbcraft_doc::Board) -> Result<(), String> {
        for child in &node.children {
            if child.name() == "stackup" {
                for layer in &child.children {
                    if layer.name() == "layer" {
                        let finish = layer.field("finish");
                        if !finish.is_empty() {
                            board.finish = finish.to_owned();
                        }
                    }
                }
            } else if child.name() != "pad_to_mask_clearance" && child.atom.is_none() {
                board
                    .preserved
                    .push_span(&self.source, child.span.clone())?;
            }
        }
        Ok(())
    }

    fn component_id(&mut self, reference: &str) -> Result<Id, String> {
        if let Some(id) = self.refs.get(reference) {
            return Ok(*id);
        }
        if self.next_component > MAX_COMPONENTS as Id {
            return Err("component limit reached".into());
        }
        let id = self.next_component;
        self.next_component += 1;
        self.refs.insert(reference.to_owned(), id);
        Ok(id)
    }

    fn import_footprint(
        &mut self,
        node: &Node,
        board: &mut pcbcraft_doc::Board,
    ) -> Result<(), String> {
        let slice = &self.source[node.span.clone()];
        let definition = read_footprint(slice).map_err(|e| library_error(&self.source, e))?;
        let reference = property(node, "Reference").unwrap_or_else(|| "REF".into());
        let value = property(node, "Value").unwrap_or_default();
        let comp_id = self.component_id(&reference)?;
        let fp_id = uuid_id(node).unwrap_or(comp_id.saturating_mul(1000));
        let (position, rotation) = footprint_at(node, &self.source)?;
        let kind = footprint_kind(&definition.name);

        let mut pads = Vec::new();
        for pad_node in node.children.iter().filter(|c| c.name() == "pad") {
            let number = pad_node.value(1).to_owned();
            let def_pad = definition
                .pads
                .iter()
                .find(|p| p.number == number)
                .ok_or_else(|| {
                    line_error(
                        &self.source,
                        pad_node.span.start,
                        &format!("pad {number} missing from footprint definition"),
                    )
                })?;
            let ord =
                parse_u32(pad_node.field("net"), &self.source, pad_node.span.start).unwrap_or(0);
            let net = if ord == 0 { None } else { Some(ord as Id) };
            pads.push(Pad {
                number,
                offset: def_pad.position,
                diameter: def_pad.size.x.min(def_pad.size.y),
                net,
                castellated: None,
                geometry: Some(def_pad.clone()),
            });
        }

        let mut preserved = PreservedSource::default();
        for child in &node.children {
            if child.atom.is_none()
                && !matches!(
                    child.name(),
                    "pad"
                        | "property"
                        | "at"
                        | "uuid"
                        | "layer"
                        | "fp_text"
                        | "attr"
                        | "model"
                        | "clearance"
                        | "solder_mask_margin"
                        | "solder_paste_margin"
                )
                && !child.name().starts_with("fp_")
                && !is_graphic(child.name())
            {
                preserved.push_span(&self.source, child.span.clone())?;
            }
        }

        if self.components.iter().all(|c| c.id != comp_id) {
            let pins = template_pins(&kind);
            self.components.push(Component {
                id: comp_id,
                reference,
                value,
                kind,
                position: Point::default(),
                rotation: 0.0,
                pins,
                units: Vec::new(),
                library_symbol: None,
                library_footprint: None,
                fields: Vec::new(),
                sheet: 0,
            });
        }

        board.footprints.push(Footprint {
            id: fp_id,
            component: comp_id,
            position,
            rotation,
            locked: None,
            pads,
            preserved,
        });
        Ok(())
    }

    fn import_segment(
        &mut self,
        node: &Node,
        board: &mut pcbcraft_doc::Board,
        arc: bool,
    ) -> Result<(), String> {
        let start = parse_xy(node, "start", &self.source)?;
        let end = parse_xy(node, "end", &self.source)?;
        let width = parse_f64(node.field("width"), &self.source, node.span.start)?;
        let layer = Layer::from_file_str(node.field("layer").trim_matches('"'))?;
        let ord = parse_u32(node.field("net"), &self.source, node.span.start)?;
        let mid = if arc {
            Some(parse_xy(node, "mid", &self.source)?)
        } else {
            None
        };
        let id = uuid_id(node).unwrap_or(board.tracks.len() as Id + 100);
        let mut preserved = PreservedSource::default();
        for child in &node.children {
            if child.atom.is_none()
                && !matches!(
                    child.name(),
                    "start" | "end" | "mid" | "width" | "layer" | "net" | "uuid"
                )
            {
                preserved.push_span(&self.source, child.span.clone())?;
            }
        }
        board.tracks.push(Track {
            id,
            net: ord as Id,
            start,
            end,
            width,
            layer,
            arc: mid,
            preserved,
        });
        Ok(())
    }

    fn import_via(&mut self, node: &Node, board: &mut pcbcraft_doc::Board) -> Result<(), String> {
        let layers: Vec<_> = node
            .child("layers")
            .map(|l| {
                l.children
                    .iter()
                    .skip(1)
                    .filter_map(|c| c.atom.as_deref())
                    .map(|s| s.trim_matches('"'))
                    .collect()
            })
            .unwrap_or_else(|| vec!["F.Cu", "B.Cu"]);
        if layers != ["F.Cu", "B.Cu"] && layers != ["B.Cu", "F.Cu"] {
            return Err(
                "only through-hole vias between the outer copper layers are supported".into(),
            );
        }
        let position = parse_at(node, &self.source)?;
        let diameter = parse_f64(node.field("size"), &self.source, node.span.start)?;
        let drill = parse_f64(node.field("drill"), &self.source, node.span.start)?;
        let ord = parse_u32(node.field("net"), &self.source, node.span.start)?;
        let id = uuid_id(node).unwrap_or(board.vias.len() as Id + 200);
        board.vias.push(Via {
            id,
            net: ord as Id,
            position,
            diameter,
            drill,
            preserved: PreservedSource::default(),
        });
        Ok(())
    }

    fn import_zone(&mut self, node: &Node, board: &mut pcbcraft_doc::Board) -> Result<(), String> {
        if node.child("keepout").is_some() {
            return Err("zone keepouts are not imported as copper yet".into());
        }
        let ord = parse_u32(node.field("net"), &self.source, node.span.start)?;
        let layer = Layer::from_file_str(node.field("layer").trim_matches('"'))?;
        let polygon = node
            .child("polygon")
            .or_else(|| node.child("filled_polygon"))
            .ok_or_else(|| line_error(&self.source, node.span.start, "zone missing polygon"))?;
        let outline = parse_polygon_pts(polygon, &self.source)?;
        let id = uuid_id(node).unwrap_or(board.zones.len() as Id + 300);
        let filled = node.field("filled") != "no";
        let mut preserved = PreservedSource::default();
        for child in &node.children {
            if child.atom.is_none()
                && !matches!(
                    child.name(),
                    "net"
                        | "net_name"
                        | "layer"
                        | "uuid"
                        | "hatch"
                        | "connect_pads"
                        | "min_thickness"
                        | "filled"
                        | "fill"
                        | "polygon"
                        | "filled_polygon"
                        | "keepout"
                )
            {
                preserved.push_span(&self.source, child.span.clone())?;
            }
        }
        board.zones.push(CopperZone {
            id,
            net: ord as Id,
            layer,
            outline,
            cutouts: Vec::new(),
            filled,
            kind: None,
            preserved,
        });
        Ok(())
    }

    fn import_rect_outline(
        &self,
        node: &Node,
        board: &mut pcbcraft_doc::Board,
    ) -> Result<(), String> {
        let start = parse_xy(node, "start", &self.source)?;
        let end = parse_xy(node, "end", &self.source)?;
        board.width = start.x.max(end.x);
        board.height = start.y.max(end.y);
        board.outline = None;
        Ok(())
    }

    fn import_poly_outline(
        &self,
        node: &Node,
        board: &mut pcbcraft_doc::Board,
    ) -> Result<(), String> {
        let pts = node
            .child("pts")
            .ok_or_else(|| line_error(&self.source, node.span.start, "outline missing points"))?;
        let mut outline = parse_pts(pts, &self.source)?;
        if outline.first() != outline.last()
            && let Some(first) = outline.first().copied()
        {
            outline.push(first);
        }
        let max_x = outline.iter().map(|p| p.x).fold(0.0, f64::max);
        let max_y = outline.iter().map(|p| p.y).fold(0.0, f64::max);
        board.width = max_x.max(board.width);
        board.height = max_y.max(board.height);
        board.outline = Some(outline);
        Ok(())
    }

    fn import_circle_cutout(
        &self,
        node: &Node,
        board: &mut pcbcraft_doc::Board,
    ) -> Result<(), String> {
        let center = parse_xy(node, "center", &self.source)?;
        let end = parse_xy(node, "end", &self.source)?;
        let radius = center.distance(end);
        let id = uuid_id(node).unwrap_or(board.cutouts.len() as Id + 400);
        board.cutouts.push(Cutout { id, center, radius });
        Ok(())
    }
}

fn is_graphic(name: &str) -> bool {
    matches!(
        name,
        "fp_line"
            | "fp_rect"
            | "fp_circle"
            | "fp_arc"
            | "fp_poly"
            | "fp_text"
            | "gr_line"
            | "gr_rect"
            | "gr_circle"
            | "gr_arc"
            | "gr_poly"
    )
}

fn sync_footprint_pin_nets(components: &mut [Component], footprints: &[Footprint]) {
    for footprint in footprints {
        let Some(component) = components.iter_mut().find(|c| c.id == footprint.component) else {
            continue;
        };
        for pad in &footprint.pads {
            if let Some(pin) = component
                .pins
                .iter_mut()
                .find(|pin| pin.number == pad.number)
            {
                pin.net = pad.net;
            }
        }
    }
}

fn template_pins(kind: &str) -> Vec<Pin> {
    let count = match kind {
        "connector" => 2,
        "resistor" | "led" | "capacitor" => 2,
        _ => 2,
    };
    (1..=count)
        .map(|n| Pin {
            number: n.to_string(),
            name: n.to_string(),
            offset: Point::default(),
            net: None,
            electrical_type: "passive".into(),
            direction: None,
            unit: 1,
        })
        .collect()
}

fn footprint_kind(name: &str) -> String {
    name.strip_prefix("pcbcraft_")
        .or_else(|| name.split(':').next_back())
        .unwrap_or(name)
        .to_owned()
}

fn property(node: &Node, key: &str) -> Option<String> {
    node.children
        .iter()
        .find(|c| c.name() == "property" && c.value(1) == key)
        .map(|c| c.value(2).to_owned())
}

fn footprint_at(node: &Node, source: &str) -> Result<(Point, f64), String> {
    let at = node
        .child("at")
        .ok_or_else(|| line_error(source, node.span.start, "footprint missing placement"))?;
    let x = parse_f64(at.value(1), source, at.span.start)?;
    let y = parse_f64(at.value(2), source, at.span.start)?;
    let rot = if at.value(3).is_empty() {
        0.0
    } else {
        parse_f64(at.value(3), source, at.span.start)?
    };
    Ok((Point::new(x, y), rot))
}

fn parse_at(node: &Node, source: &str) -> Result<Point, String> {
    let at = node
        .child("at")
        .ok_or_else(|| line_error(source, node.span.start, "missing at"))?;
    Ok(Point::new(
        parse_f64(at.value(1), source, at.span.start)?,
        parse_f64(at.value(2), source, at.span.start)?,
    ))
}

fn parse_xy(node: &Node, field: &str, source: &str) -> Result<Point, String> {
    let p = node
        .child(field)
        .ok_or_else(|| line_error(source, node.span.start, &format!("missing {field}")))?;
    Ok(Point::new(
        parse_f64(p.value(1), source, p.span.start)?,
        parse_f64(p.value(2), source, p.span.start)?,
    ))
}

fn parse_pts(node: &Node, source: &str) -> Result<Vec<Point>, String> {
    let mut pts = Vec::new();
    for child in &node.children {
        if child.name() == "xy" || child.name() == "start" {
            pts.push(Point::new(
                parse_f64(child.value(1), source, child.span.start)?,
                parse_f64(child.value(2), source, child.span.start)?,
            ));
        }
    }
    Ok(pts)
}

fn parse_polygon_pts(node: &Node, source: &str) -> Result<Vec<Point>, String> {
    let pts = node
        .child("pts")
        .ok_or_else(|| line_error(source, node.span.start, "polygon missing pts"))?;
    let mut outline = parse_pts(pts, source)?;
    if outline.len() < 3 {
        return Err("zone outline needs at least three points".into());
    }
    if outline.first() != outline.last()
        && let Some(first) = outline.first().copied()
    {
        outline.push(first);
    }
    Ok(outline)
}

fn parse_f64(raw: &str, source: &str, offset: usize) -> Result<f64, String> {
    let n: f64 = raw
        .parse()
        .map_err(|_| line_error(source, offset, &format!("invalid number {raw:?}")))?;
    if !n.is_finite() || n.abs() > 100_000.0 {
        return Err(line_error(
            source,
            offset,
            "number outside finite geometry bounds",
        ));
    }
    Ok(n)
}

fn parse_u32(raw: &str, source: &str, offset: usize) -> Result<u32, String> {
    raw.parse::<u32>()
        .map_err(|_| line_error(source, offset, &format!("invalid integer {raw:?}")))
}

fn uuid_id(node: &Node) -> Option<Id> {
    let raw = node.field("uuid");
    if raw.is_empty() {
        return None;
    }
    let tail = raw.rsplit('-').next()?;
    u64::from_str_radix(tail, 16).ok()
}

fn format_parse_error(text: &str, err: pcbcraft_library::Error) -> String {
    match err {
        pcbcraft_library::Error::Syntax { offset, message } => line_error(text, offset, &message),
        pcbcraft_library::Error::Limit(label) => format!("interchange file exceeds {label}"),
        pcbcraft_library::Error::Invalid(msg) => msg,
    }
}

fn library_error(text: &str, err: LibraryError) -> String {
    match err {
        LibraryError::Syntax { offset, message } => line_error(text, offset, &message),
        LibraryError::Limit(label) => format!("interchange file exceeds {label}"),
        LibraryError::Invalid(msg) => msg,
    }
}

fn line_error(text: &str, offset: usize, message: &str) -> String {
    let line = line_number_at_offset(text, offset);
    format!("line {line}: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_board_reports_line_number() {
        let text = "(kicad_pcb\n (segment (start 0 0) (end 1))\n)\n";
        match read_board_interchange(text) {
            Err(err) => assert!(err.contains("line"), "expected a line number in {err:?}"),
            Ok(_) => panic!("expected malformed board import to fail"),
        }
    }
}
