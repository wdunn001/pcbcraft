//! Versioned text clipboard for bounded design selections.
use crate::Error;
use pcbcraft_doc::*;
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ENVELOPE: &str = "PCBCRAFT selection/1";
const MAX_BYTES: usize = 512 * 1024;
pub const MAX_SELECTION_IDS: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct SelectionFragment {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    nets: Vec<Net>,
    components: Vec<Component>,
    footprints: Vec<Footprint>,
    tracks: Vec<Track>,
    vias: Vec<Via>,
    zones: Vec<CopperZone>,
    wires: Vec<Wire>,
    drawn_wires: Vec<DrawnWire>,
    labels: Vec<Label>,
    graphics: Vec<Graphic>,
    no_connects: Vec<NoConnectFlag>,
    texts: Vec<BoardText>,
}

fn shift_point(p: Point, offset: Point) -> Result<Point, Error> {
    let shifted = Point::new(p.x + offset.x, p.y + offset.y);
    if !shifted.valid() {
        return Err(Error::Invalid("paste offset is out of range".into()));
    }
    Ok(shifted)
}

fn shift_points(points: &[Point], offset: Point) -> Result<Vec<Point>, Error> {
    points.iter().map(|p| shift_point(*p, offset)).collect()
}

fn collect_net_ids(fragment: &SelectionFragment) -> BTreeSet<Id> {
    let mut nets = BTreeSet::new();
    for net in &fragment.nets {
        nets.insert(net.id);
    }
    for track in &fragment.tracks {
        nets.insert(track.net);
    }
    for via in &fragment.vias {
        nets.insert(via.net);
    }
    for zone in &fragment.zones {
        nets.insert(zone.net);
    }
    for wire in &fragment.wires {
        nets.insert(wire.net);
    }
    for label in &fragment.labels {
        nets.insert(label.net);
    }
    for drawn in &fragment.drawn_wires {
        if let Some(net) = drawn.net {
            nets.insert(net);
        }
    }
    for component in &fragment.components {
        for pin in &component.pins {
            if let Some(net) = pin.net {
                nets.insert(net);
            }
        }
    }
    for footprint in &fragment.footprints {
        for pad in &footprint.pads {
            if let Some(net) = pad.net {
                nets.insert(net);
            }
        }
    }
    nets
}

fn fragment_object_ids(fragment: &SelectionFragment) -> BTreeSet<Id> {
    let mut ids = BTreeSet::new();
    for component in &fragment.components {
        ids.insert(component.id);
    }
    for footprint in &fragment.footprints {
        ids.insert(footprint.id);
    }
    for track in &fragment.tracks {
        ids.insert(track.id);
    }
    for via in &fragment.vias {
        ids.insert(via.id);
    }
    for zone in &fragment.zones {
        ids.insert(zone.id);
    }
    for wire in &fragment.wires {
        ids.insert(wire.id);
    }
    for drawn in &fragment.drawn_wires {
        ids.insert(drawn.id);
    }
    for label in &fragment.labels {
        ids.insert(label.id);
    }
    for graphic in &fragment.graphics {
        ids.insert(graphic.id);
    }
    for flag in &fragment.no_connects {
        ids.insert(flag.id);
    }
    for text in &fragment.texts {
        ids.insert(text.id);
    }
    ids
}

fn validate_fragment_refs(fragment: &SelectionFragment) -> Result<(), Error> {
    let objects = fragment_object_ids(fragment);
    let net_ids = collect_net_ids(fragment);
    for net in &fragment.nets {
        if !net_ids.contains(&net.id) {
            return Err(Error::Invalid("clipboard fragment net is unused".into()));
        }
    }
    for net_id in &net_ids {
        if !fragment.nets.iter().any(|n| n.id == *net_id) {
            return Err(Error::Invalid(
                "clipboard fragment net record missing".into(),
            ));
        }
    }
    for footprint in &fragment.footprints {
        if !objects.contains(&footprint.component) {
            return Err(Error::Invalid(
                "clipboard footprint references a component outside the selection".into(),
            ));
        }
    }
    for wire in &fragment.wires {
        if !objects.contains(&wire.from.component) || !objects.contains(&wire.to.component) {
            return Err(Error::Invalid(
                "clipboard wire references a component outside the selection".into(),
            ));
        }
    }
    for flag in &fragment.no_connects {
        if !objects.contains(&flag.pin.component) {
            return Err(Error::Invalid(
                "clipboard no-connect references a component outside the selection".into(),
            ));
        }
    }
    Ok(())
}

enum ObjectSlot {
    Component,
    Footprint,
    Track,
    Via,
    Zone,
    Wire,
    DrawnWire,
    Label,
    Graphic,
    NoConnect,
    BoardText,
}

fn classify(p: &Project, id: Id) -> Result<ObjectSlot, Error> {
    if p.components.iter().any(|c| c.id == id) {
        return Ok(ObjectSlot::Component);
    }
    if p.board.footprints.iter().any(|f| f.id == id) {
        return Ok(ObjectSlot::Footprint);
    }
    if p.board.tracks.iter().any(|t| t.id == id) {
        return Ok(ObjectSlot::Track);
    }
    if p.board.vias.iter().any(|v| v.id == id) {
        return Ok(ObjectSlot::Via);
    }
    if p.board.zones.iter().any(|z| z.id == id) {
        return Ok(ObjectSlot::Zone);
    }
    if p.wires.iter().any(|w| w.id == id) {
        return Ok(ObjectSlot::Wire);
    }
    if p.drawn_wires.iter().any(|w| w.id == id) {
        return Ok(ObjectSlot::DrawnWire);
    }
    if p.labels.iter().any(|l| l.id == id) {
        return Ok(ObjectSlot::Label);
    }
    if p.graphics.iter().any(|g| g.id == id) {
        return Ok(ObjectSlot::Graphic);
    }
    if p.no_connects.iter().any(|f| f.id == id) {
        return Ok(ObjectSlot::NoConnect);
    }
    if p.board.texts.iter().any(|t| t.id == id) {
        return Ok(ObjectSlot::BoardText);
    }
    Err(Error::Invalid(
        "selection id is not a copyable object".into(),
    ))
}

fn referenced_net_ids(p: &Project, id: Id, kind: &ObjectSlot) -> BTreeSet<Id> {
    let mut nets = BTreeSet::new();
    match kind {
        ObjectSlot::Component => {
            if let Some(c) = p.component(id) {
                for pin in &c.pins {
                    if let Some(net) = pin.net {
                        nets.insert(net);
                    }
                }
            }
        }
        ObjectSlot::Footprint => {
            if let Some(f) = p.board.footprints.iter().find(|f| f.id == id) {
                for pad in &f.pads {
                    if let Some(net) = pad.net {
                        nets.insert(net);
                    }
                }
            }
        }
        ObjectSlot::Track => {
            if let Some(t) = p.board.tracks.iter().find(|t| t.id == id) {
                nets.insert(t.net);
            }
        }
        ObjectSlot::Via => {
            if let Some(v) = p.board.vias.iter().find(|v| v.id == id) {
                nets.insert(v.net);
            }
        }
        ObjectSlot::Zone => {
            if let Some(z) = p.board.zones.iter().find(|z| z.id == id) {
                nets.insert(z.net);
            }
        }
        ObjectSlot::Wire => {
            if let Some(w) = p.wires.iter().find(|w| w.id == id) {
                nets.insert(w.net);
            }
        }
        ObjectSlot::DrawnWire => {
            if let Some(w) = p.drawn_wires.iter().find(|w| w.id == id)
                && let Some(net) = w.net
            {
                nets.insert(net);
            }
        }
        ObjectSlot::Label => {
            if let Some(l) = p.labels.iter().find(|l| l.id == id) {
                nets.insert(l.net);
            }
        }
        ObjectSlot::Graphic | ObjectSlot::NoConnect | ObjectSlot::BoardText => {}
    }
    nets
}

pub fn copy_text(p: &Project, ids: &[Id], context: Option<&str>) -> Result<String, Error> {
    if let Some(ctx) = context
        && ctx != "board"
        && ctx != "schematic"
    {
        return Err(Error::Invalid(
            "selection context must be board or schematic".into(),
        ));
    }
    if ids.is_empty() || ids.len() > MAX_SELECTION_IDS {
        return Err(Error::Invalid(format!(
            "selection copy accepts 1–{MAX_SELECTION_IDS} ids"
        )));
    }
    let mut seen = BTreeSet::new();
    let mut work = BTreeSet::new();
    for id in ids {
        if !seen.insert(*id) {
            continue;
        }
        let kind = classify(p, *id)?;
        if matches!(kind, ObjectSlot::Footprint) {
            return Err(Error::Invalid(
                "select the component instead of its footprint; the footprint is copied with it"
                    .into(),
            ));
        }
        work.insert(*id);
        if matches!(kind, ObjectSlot::Component)
            && let Some(fp) = p.board.footprints.iter().find(|f| f.component == *id)
        {
            work.insert(fp.id);
        }
    }
    let mut net_ids = BTreeSet::new();
    for id in &work {
        let kind = classify(p, *id)?;
        net_ids.extend(referenced_net_ids(p, *id, &kind));
    }
    let mut fragment = SelectionFragment {
        version: 1,
        context: context.map(str::to_string),
        nets: net_ids
            .iter()
            .filter_map(|id| p.nets.iter().find(|n| n.id == *id).cloned())
            .collect(),
        components: vec![],
        footprints: vec![],
        tracks: vec![],
        vias: vec![],
        zones: vec![],
        wires: vec![],
        drawn_wires: vec![],
        labels: vec![],
        graphics: vec![],
        no_connects: vec![],
        texts: vec![],
    };
    for id in &work {
        match classify(p, *id)? {
            ObjectSlot::Component => {
                if let Some(c) = p.component(*id) {
                    fragment.components.push(c.clone());
                }
            }
            ObjectSlot::Footprint => {
                if let Some(f) = p.board.footprints.iter().find(|f| f.id == *id) {
                    fragment.footprints.push(f.clone());
                }
            }
            ObjectSlot::Track => {
                if let Some(t) = p.board.tracks.iter().find(|t| t.id == *id) {
                    fragment.tracks.push(t.clone());
                }
            }
            ObjectSlot::Via => {
                if let Some(v) = p.board.vias.iter().find(|v| v.id == *id) {
                    fragment.vias.push(v.clone());
                }
            }
            ObjectSlot::Zone => {
                if let Some(z) = p.board.zones.iter().find(|z| z.id == *id) {
                    fragment.zones.push(z.clone());
                }
            }
            ObjectSlot::Wire => {
                if let Some(w) = p.wires.iter().find(|w| w.id == *id) {
                    fragment.wires.push(w.clone());
                }
            }
            ObjectSlot::DrawnWire => {
                if let Some(w) = p.drawn_wires.iter().find(|w| w.id == *id) {
                    fragment.drawn_wires.push(w.clone());
                }
            }
            ObjectSlot::Label => {
                if let Some(l) = p.labels.iter().find(|l| l.id == *id) {
                    fragment.labels.push(l.clone());
                }
            }
            ObjectSlot::Graphic => {
                if let Some(g) = p.graphics.iter().find(|g| g.id == *id) {
                    fragment.graphics.push(g.clone());
                }
            }
            ObjectSlot::NoConnect => {
                if let Some(f) = p.no_connects.iter().find(|f| f.id == *id) {
                    fragment.no_connects.push(f.clone());
                }
            }
            ObjectSlot::BoardText => {
                if let Some(t) = p.board.texts.iter().find(|t| t.id == *id) {
                    fragment.texts.push(t.clone());
                }
            }
        }
    }
    validate_fragment_refs(&fragment)?;
    let body = serde_json::to_string(&fragment).map_err(|e| Error::Invalid(e.to_string()))?;
    if body.len() > MAX_BYTES {
        return Err(Error::Invalid(
            "selection is too large for the clipboard".into(),
        ));
    }
    Ok(format!("{ENVELOPE}\n{body}"))
}

fn parse_envelope(text: &str) -> Result<SelectionFragment, Error> {
    if text.len() > MAX_BYTES + ENVELOPE.len() + 1 {
        return Err(Error::Invalid("clipboard text is too large".into()));
    }
    let Some((line, body)) = text.split_once('\n') else {
        return Err(Error::Invalid("clipboard text is malformed".into()));
    };
    if line != ENVELOPE {
        return Err(Error::Invalid(
            "clipboard envelope is not recognised".into(),
        ));
    }
    if body.is_empty() {
        return Err(Error::Invalid("clipboard body is empty".into()));
    }
    if body.len() > MAX_BYTES {
        return Err(Error::Invalid("clipboard body is too large".into()));
    }
    let fragment: SelectionFragment = serde_json::from_str(body)
        .map_err(|_| Error::Invalid("clipboard body is not valid JSON".into()))?;
    if fragment.version != 1 {
        return Err(Error::Invalid("clipboard version is not supported".into()));
    }
    validate_fragment_refs(&fragment)?;
    Ok(fragment)
}

fn resolve_net(
    p: &mut Project,
    next: &mut Id,
    net_map: &mut BTreeMap<Id, Id>,
    source: &Net,
) -> Result<Id, Error> {
    if let Some(id) = net_map.get(&source.id) {
        return Ok(*id);
    }
    let target = if let Some(existing) = p.nets.iter().find(|n| n.name == source.name) {
        existing.id
    } else {
        let id = crate::allocate(next)?;
        p.nets.push(Net {
            id,
            name: source.name.clone(),
            target_length: source.target_length,
            length_tolerance: source.length_tolerance,
        });
        id
    };
    net_map.insert(source.id, target);
    Ok(target)
}

pub fn paste(
    p: &mut Project,
    next: &mut Id,
    text: &str,
    offset: Point,
    renumber: bool,
    unique_reference: fn(&Project, &str) -> String,
) -> Result<(usize, BTreeMap<Id, Id>), Error> {
    if !offset.valid() {
        return Err(Error::Invalid("paste offset is out of range".into()));
    }
    let fragment = parse_envelope(text)?;
    let object_count = fragment.components.len()
        + fragment.footprints.len()
        + fragment.tracks.len()
        + fragment.vias.len()
        + fragment.zones.len()
        + fragment.wires.len()
        + fragment.drawn_wires.len()
        + fragment.labels.len()
        + fragment.graphics.len()
        + fragment.no_connects.len()
        + fragment.texts.len();
    if object_count == 0 {
        return Err(Error::Invalid("clipboard selection is empty".into()));
    }
    let mut net_map = BTreeMap::new();
    for net in &fragment.nets {
        resolve_net(p, next, &mut net_map, net)?;
    }
    let mut id_map = BTreeMap::new();
    let allocate_id = |next: &mut Id| -> Result<Id, Error> { crate::allocate(next) };
    for component in &fragment.components {
        let new_id = allocate_id(next)?;
        id_map.insert(component.id, new_id);
    }
    for footprint in &fragment.footprints {
        let new_id = allocate_id(next)?;
        id_map.insert(footprint.id, new_id);
    }
    for track in &fragment.tracks {
        let new_id = allocate_id(next)?;
        id_map.insert(track.id, new_id);
    }
    for via in &fragment.vias {
        let new_id = allocate_id(next)?;
        id_map.insert(via.id, new_id);
    }
    for zone in &fragment.zones {
        let new_id = allocate_id(next)?;
        id_map.insert(zone.id, new_id);
    }
    for wire in &fragment.wires {
        let new_id = allocate_id(next)?;
        id_map.insert(wire.id, new_id);
    }
    for drawn in &fragment.drawn_wires {
        let new_id = allocate_id(next)?;
        id_map.insert(drawn.id, new_id);
    }
    for label in &fragment.labels {
        let new_id = allocate_id(next)?;
        id_map.insert(label.id, new_id);
    }
    for graphic in &fragment.graphics {
        let new_id = allocate_id(next)?;
        id_map.insert(graphic.id, new_id);
    }
    for flag in &fragment.no_connects {
        let new_id = allocate_id(next)?;
        id_map.insert(flag.id, new_id);
    }
    for text_obj in &fragment.texts {
        let new_id = allocate_id(next)?;
        id_map.insert(text_obj.id, new_id);
    }
    let remap = |id: Id| -> Result<Id, Error> {
        id_map
            .get(&id)
            .copied()
            .ok_or_else(|| Error::Invalid("clipboard id remap failed".into()))
    };
    let remap_net = |id: Id| -> Result<Id, Error> {
        net_map
            .get(&id)
            .copied()
            .ok_or_else(|| Error::Invalid("clipboard net remap failed".into()))
    };
    for mut component in fragment.components {
        let old = component.id;
        component.id = remap(old)?;
        component.position = shift_point(component.position, offset)?;
        for unit in &mut component.units {
            unit.position = shift_point(unit.position, offset)?;
        }
        for pin in &mut component.pins {
            if let Some(net) = pin.net {
                pin.net = Some(remap_net(net)?);
            }
        }
        if renumber {
            component.reference = unique_reference(p, &component.reference);
        }
        p.components.push(component);
    }
    for mut footprint in fragment.footprints {
        let old = footprint.id;
        footprint.id = remap(old)?;
        footprint.component = remap(footprint.component)?;
        footprint.position = shift_point(footprint.position, offset)?;
        for pad in &mut footprint.pads {
            if let Some(net) = pad.net {
                pad.net = Some(remap_net(net)?);
            }
        }
        p.board.footprints.push(footprint);
    }
    for mut track in fragment.tracks {
        let old = track.id;
        track.id = remap(old)?;
        track.net = remap_net(track.net)?;
        track.start = shift_point(track.start, offset)?;
        track.end = shift_point(track.end, offset)?;
        if let Some(arc) = track.arc {
            track.arc = Some(shift_point(arc, offset)?);
        }
        p.board.tracks.push(track);
    }
    for mut via in fragment.vias {
        let old = via.id;
        via.id = remap(old)?;
        via.net = remap_net(via.net)?;
        via.position = shift_point(via.position, offset)?;
        p.board.vias.push(via);
    }
    for mut zone in fragment.zones {
        let old = zone.id;
        zone.id = remap(old)?;
        zone.net = remap_net(zone.net)?;
        zone.outline = shift_points(&zone.outline, offset)?;
        zone.cutouts = zone
            .cutouts
            .iter()
            .map(|cut| shift_points(cut, offset))
            .collect::<Result<Vec<_>, _>>()?;
        p.board.zones.push(zone);
    }
    for mut wire in fragment.wires {
        let old = wire.id;
        wire.id = remap(old)?;
        wire.net = remap_net(wire.net)?;
        wire.from.component = remap(wire.from.component)?;
        wire.to.component = remap(wire.to.component)?;
        p.wires.push(wire);
    }
    for mut drawn in fragment.drawn_wires {
        let old = drawn.id;
        drawn.id = remap(old)?;
        if let Some(net) = drawn.net {
            drawn.net = Some(remap_net(net)?);
        }
        drawn.points = shift_points(&drawn.points, offset)?;
        p.drawn_wires.push(drawn);
    }
    for mut label in fragment.labels {
        let old = label.id;
        label.id = remap(old)?;
        label.net = remap_net(label.net)?;
        label.position = shift_point(label.position, offset)?;
        p.labels.push(label);
    }
    for mut graphic in fragment.graphics {
        let old = graphic.id;
        graphic.id = remap(old)?;
        graphic.points = shift_points(&graphic.points, offset)?;
        p.graphics.push(graphic);
    }
    for mut flag in fragment.no_connects {
        let old = flag.id;
        flag.id = remap(old)?;
        flag.pin.component = remap(flag.pin.component)?;
        p.no_connects.push(flag);
    }
    for mut text_obj in fragment.texts {
        let old = text_obj.id;
        text_obj.id = remap(old)?;
        text_obj.position = shift_point(text_obj.position, offset)?;
        p.board.texts.push(text_obj);
    }
    let count = id_map.len();
    Ok((count, id_map))
}
