//! Poured copper: the fill that a zone actually becomes.
//!
//! A [`CopperZone`] stores an outline and the cutouts a designer drew. The fill
//! is computed from that and the rest of the board: keepouts first, then
//! foreign copper grown by its required clearance, then the board edge, then
//! same-net pads joined solid or through thermal spokes, then a morphological
//! opening that removes necks narrower than the minimum copper width, and
//! finally the island policy. Computing on demand keeps the document free of
//! derived copper that could go stale.
//!
//! Every step is bounded: a zone may carry [`MAX_OUTLINE_VERTICES`] vertices
//! and a layer may carry [`MAX_FILL_ITEMS`] copper items before the fill
//! refuses with a plain error rather than grinding on a pathological board.

use crate::{
    CopperZone, Footprint, Id, Keepout, Layer, Pad, Project, RuleArea, ZoneConnect, ZonePour,
};
use pcbcraft_geom::poly::{self, Piece, Region, Ring};
use pcbcraft_geom::{Point, polygon_area};

/// Most vertices a poured outline may carry before the fill gives up.
pub const MAX_OUTLINE_VERTICES: usize = 256;
/// Most copper items the fill considers on one copper layer.
pub const MAX_FILL_ITEMS: usize = 2048;
/// Fewest spokes a pad needs for the pour to count as connected to it.
const MIN_SPOKES: usize = 2;

/// The outcome of pouring one zone.
#[derive(Clone, Debug)]
pub struct ZoneFill {
    /// The zone this fill belongs to.
    pub zone: Id,
    /// Net the poured copper carries.
    pub net: Id,
    /// Copper side the pour sits on.
    pub layer: Layer,
    /// The copper that ships, one piece per connected area, holes included.
    pub pieces: Region,
    /// Narrowest copper the pour kept, in millimetres.
    pub min_width: f64,
    /// Detached pieces the fill found.
    pub islands: usize,
    /// Detached pieces the policy dropped.
    pub removed: usize,
    /// True when the zone touches none of its own net and was therefore kept
    /// whole as a deliberate shield or heatsink.
    pub unconnected: bool,
    /// Spokes that reach the poured copper.
    pub spokes: usize,
    /// Same-net pads or vias left with fewer than two spokes.
    pub starved: Vec<Id>,
}
impl ZoneFill {
    /// Total poured copper area in square millimetres.
    pub fn area(&self) -> f64 {
        poly::region_area(&self.pieces)
    }
    /// True when a point falls on poured copper.
    pub fn contains(&self, point: Point) -> bool {
        poly::region_contains(&self.pieces, point)
    }
}

/// One piece of copper on a layer, seen the way the pour sees it.
struct Item {
    id: Id,
    net: Id,
    kind: Kind,
    piece: Piece,
    at: Point,
    /// Rotation of the part, used to line thermal spokes up with a pad.
    rotation: f64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Pad,
    Via,
    Track,
    Zone,
    Hole,
}

/// Poured copper for every zone on the board, in zone order.
pub fn zone_fills(project: &Project) -> Result<Vec<ZoneFill>, String> {
    project
        .board
        .zones
        .iter()
        .map(|zone| zone_fill(project, zone))
        .collect()
}

/// The poured copper of one zone, after knockouts, reliefs, the width opening
/// and the island policy.
pub fn zone_fill(project: &Project, zone: &CopperZone) -> Result<ZoneFill, String> {
    let settings = zone.pour_settings();
    let layer = zone.layer;
    let mut fill = ZoneFill {
        zone: zone.id,
        net: zone.net,
        layer,
        pieces: Vec::new(),
        min_width: settings
            .min_width
            .unwrap_or_else(|| default_pour_min_width(project)),
        islands: 0,
        removed: 0,
        unconnected: false,
        spokes: 0,
        starved: Vec::new(),
    };
    if !zone.filled {
        return Ok(fill);
    }
    if zone.outline.len() > MAX_OUTLINE_VERTICES {
        return Err(format!(
            "zone {} outline needs at most {MAX_OUTLINE_VERTICES} vertices to pour",
            zone.id
        ));
    }
    let Some(outline) = ring(&zone.outline) else {
        return Ok(fill);
    };
    let items = items(project, layer)?;
    if items.len() > MAX_FILL_ITEMS {
        return Err(format!(
            "zone {} sits on a layer with more than {MAX_FILL_ITEMS} copper items to pour around",
            zone.id
        ));
    }

    // Keepouts go first: subtracting them after the knockouts leaves ragged
    // copper along their edges.
    let keepouts: Vec<Piece> = project
        .board
        .rule_areas
        .iter()
        .filter(|area| keepout_blocks(area, KeepoutKind::Zones, layer, zone.net))
        .map(|area| rectangle(area.keepout_rect()))
        .collect();
    let mut copper = poly::difference(&[vec![outline]], &keepouts);

    // Foreign copper, manual cutouts, drilled holes and circular cutouts, each
    // grown by the clearance the rules demand of it.
    let mut knockout: Vec<Piece> = Vec::new();
    for item in &items {
        if item.net == zone.net && item.net != 0 {
            continue;
        }
        let clearance =
            project.required_clearance_at(zone.net, item.net, item.at, item.at, layer, layer);
        let grown =
            poly::offset(std::slice::from_ref(&item.piece), clearance).ok_or_else(|| {
                format!(
                    "zone {} clearance growth left the coordinate range",
                    zone.id
                )
            })?;
        knockout.extend(grown);
    }
    for cutout in &zone.cutouts {
        if let Some(hole) = ring(cutout) {
            knockout.push(vec![hole]);
        }
    }
    for cutout in &project.board.cutouts {
        knockout.push(poly::circle(
            cutout.center,
            cutout.radius + project.board.clearance,
        ));
    }
    copper = poly::difference(&copper, &knockout);

    // Copper never reaches the board edge, and never grows outside the board.
    if let Some(inset) = poly::offset(&[board_piece(project)], -project.board.clearance) {
        copper = poly::intersect(&copper, &inset);
    }

    // Same-net pads and vias either join solid, join through spokes, or keep
    // out of the pour entirely.
    let before_relief = copper.clone();
    let mut gap: Vec<Piece> = Vec::new();
    let mut relief: Vec<Relief> = Vec::new();
    for item in &items {
        if item.net != zone.net || item.net == 0 || item.id == zone.id {
            continue;
        }
        let connect = match item.kind {
            Kind::Pad => settings.pad_connect,
            Kind::Via => settings.via_connect,
            _ => continue,
        };
        if connect == ZoneConnect::Solid {
            continue;
        }
        if poly::intersect(std::slice::from_ref(&item.piece), &copper).is_empty() {
            continue;
        }
        let grown = poly::offset(std::slice::from_ref(&item.piece), settings.relief_gap)
            .ok_or_else(|| format!("zone {} relief growth left the coordinate range", zone.id))?;
        if connect == ZoneConnect::None {
            gap.extend(grown);
            continue;
        }
        gap.extend(poly::difference(&grown, std::slice::from_ref(&item.piece)));
        relief.push(Relief {
            grown: poly::offset(std::slice::from_ref(&item.piece), settings.relief_gap)
                .unwrap_or_default(),
            id: item.id,
            center: item.at,
            piece: item.piece.clone(),
            rotation: item.rotation,
        });
    }
    copper = poly::difference(&copper, &gap);

    // A neck narrower than the minimum copper width is not manufacturable, so
    // the opening removes it before the fill ships.
    let mut opened = open(&copper, fill.min_width);
    opened.retain(|piece| poly::piece_area(piece) >= 3.0 * fill.min_width * fill.min_width);

    // Spokes survive only when their far end lands on copper the opening kept.
    let mut spokes: Vec<Piece> = Vec::new();
    let mut kept_spokes = 0;
    for target in &relief {
        let reach = target.reach();
        let mut kept = 0;
        // Poured copper outside the cleared disc: a spoke only counts when it
        // lands there, not on the pad it starts from.
        let beyond = poly::difference(&opened, &target.grown);
        let count = f64::from(settings.spokes);
        let first = if settings.spokes == 2 { 0.0 } else { 45.0 };
        for step in 0..settings.spokes {
            let angle = first + 360.0 * f64::from(step) / count + target.rotation;
            let direction = Point::new(1.0, 0.0).rotate(angle);
            let far = Point::new(
                target.center.x + direction.x * (reach + settings.relief_gap + 1.0),
                target.center.y + direction.y * (reach + settings.relief_gap + 1.0),
            );
            let bar = poly::capsule(target.center, far, settings.spoke_width / 2.0);
            let clipped = poly::intersect(&[bar], &before_relief);
            if clipped.is_empty() {
                continue;
            }
            if poly::intersect(&clipped, &beyond).is_empty() {
                continue;
            }
            spokes.extend(clipped);
            kept += 1;
        }
        kept_spokes += kept;
        if kept < MIN_SPOKES {
            fill.starved.push(target.id);
        }
    }
    fill.spokes = kept_spokes;
    let mut pieces = poly::union(&opened, &spokes);

    // Islands: a piece that touches no same-net copper has nothing holding it
    // down in the etch bath.
    let copper_of_net: Vec<Piece> = if zone.is_generated() {
        Vec::new()
    } else {
        items
            .iter()
            .filter(|item| item.net == zone.net && item.net != 0 && item.id != zone.id)
            .map(|item| item.piece.clone())
            .collect()
    };
    let touches = |piece: &Piece| {
        zone.is_generated()
            || !poly::intersect(std::slice::from_ref(piece), &copper_of_net).is_empty()
    };
    let connected = pieces.iter().filter(|piece| touches(piece)).count();
    let all_detached = connected == 0;
    fill.unconnected = all_detached && !pieces.is_empty();
    let mut kept: Region = Vec::with_capacity(pieces.len());
    for piece in pieces.drain(..) {
        if touches(&piece) {
            kept.push(piece);
            continue;
        }
        fill.islands += 1;
        if all_detached {
            // A zone that touches nothing is a deliberate shield or heatsink,
            // not debris: keep it whole and report it once.
            kept.push(piece);
            continue;
        }
        let drop = match settings.islands {
            crate::ZoneIslands::Keep => false,
            crate::ZoneIslands::Remove => true,
            crate::ZoneIslands::RemoveBelow(area) => poly::piece_area(&piece) < area,
        };
        if drop {
            fill.removed += 1;
        } else {
            kept.push(piece);
        }
    }
    fill.pieces = kept;
    Ok(fill)
}

/// A pad or via that joins the pour through thermal spokes.
struct Relief {
    /// The pad copper grown by the relief gap: the cleared disc that spokes cross.
    grown: Region,
    id: Id,
    center: Point,
    piece: Piece,
    rotation: f64,
}

impl Relief {
    /// How far the pad copper reaches from its centre.
    fn reach(&self) -> f64 {
        self.piece
            .iter()
            .flatten()
            .map(|point| point.distance(self.center))
            .fold(0.0, f64::max)
    }
}

/// Shrink by half the minimum width, grow back with round corners, and keep
/// only what the original copper still covers.
fn open(region: &[Piece], width: f64) -> Region {
    if !width.is_finite() || width <= 0.0 {
        return region.to_vec();
    }
    let half = width / 2.0;
    let Some(shrunk) = poly::offset(region, -half) else {
        return region.to_vec();
    };
    let Some(grown) = poly::offset(&shrunk, half) else {
        return region.to_vec();
    };
    poly::intersect(&grown, region)
}

/// Narrowest copper the board already uses: the thinnest track or class
/// minimum, never below a fab-friendly 0.05 mm, never above 1 mm.
pub fn default_pour_min_width(project: &Project) -> f64 {
    let tracks = project
        .board
        .tracks
        .iter()
        .map(|track| track.width)
        .fold(f64::INFINITY, f64::min);
    let classes = project
        .net_classes
        .iter()
        .map(|class| class.min_width.unwrap_or(class.width))
        .fold(f64::INFINITY, f64::min);
    let narrowest = tracks.min(classes);
    if narrowest.is_finite() {
        narrowest.clamp(0.05, 1.0)
    } else {
        0.2
    }
}

/// True when a keepout area forbids one kind of copper on a layer for a net.
pub fn keepout_blocks(area: &RuleArea, kind: KeepoutKind, layer: Layer, net: Id) -> bool {
    let Some(keepout) = area.active_keepout() else {
        return false;
    };
    keepout.forbids(kind) && area.keepout_applies(layer, net)
}

/// The copper kinds a keepout can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeepoutKind {
    /// Tracks and other routed copper.
    Tracks,
    /// Vias.
    Vias,
    /// Pads.
    Pads,
    /// Poured zones and other copper regions.
    Zones,
}

impl Keepout {
    /// Whether this keepout forbids one kind of copper.
    pub fn forbids(&self, kind: KeepoutKind) -> bool {
        match kind {
            KeepoutKind::Tracks => self.tracks,
            KeepoutKind::Vias => self.vias,
            KeepoutKind::Pads => self.pads,
            KeepoutKind::Zones => self.zones,
        }
    }
}

/// Ids of the pads' footprints, vias, holes, tracks and zones the pour has to
/// consider on one copper layer, in board order.
///
/// # Rules
/// - Same list `zone_fill` and `keepout_hits` walk, so a test of this list is a
///   test of both.
///
/// # Dependencies
/// - `crates/doc/src/fill.rs` (`items`) builds the list; `crates/doc/src/lib.rs`
///   (`Project::via_span_layers`) decides which vias reach the layer.
///
/// # Do not
/// - Use this to edit the board; it reports, it never changes the document.
pub fn layer_obstacles(project: &Project, layer: Layer) -> Result<Vec<Id>, String> {
    Ok(items(project, layer)?.iter().map(|item| item.id).collect())
}

/// Copper shapes on one layer, each with the net and rotation the pour needs.
fn items(project: &Project, layer: Layer) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    for footprint in &project.board.footprints {
        for (id, pad, rotation, at, piece) in pad_items(footprint) {
            if !pad_reaches(project, footprint, &pad, layer) {
                continue;
            }
            items.push(Item {
                id,
                net: pad.net.unwrap_or(0),
                kind: Kind::Pad,
                piece,
                at,
                rotation,
            });
        }
    }
    for via in &project.board.vias {
        // A via is copper only on the layers of its span: outside it, the
        // layer neither clears an antipad nor joins the via.
        if !project.via_span_layers(via).contains(&layer) {
            continue;
        }
        items.push(Item {
            id: via.id,
            net: via.net,
            kind: Kind::Via,
            piece: poly::circle(via.position, via.diameter / 2.0),
            at: via.position,
            rotation: 0.0,
        });
    }
    for hole in &project.board.holes {
        items.push(Item {
            id: hole.id,
            net: 0,
            kind: Kind::Hole,
            piece: poly::circle(hole.position, hole.diameter / 2.0),
            at: hole.position,
            rotation: 0.0,
        });
    }
    for track in project
        .board
        .tracks
        .iter()
        .filter(|track| track.layer == layer)
    {
        let piece = match track.curve() {
            None => poly::capsule(track.start, track.end, track.width / 2.0),
            Some(arc) => {
                let mut bars = Vec::new();
                let chords = 16;
                for step in 0..chords {
                    let start = arc.point_at(f64::from(step) / f64::from(chords));
                    let end = arc.point_at(f64::from(step + 1) / f64::from(chords));
                    bars.push(poly::capsule(start, end, track.width / 2.0));
                }
                poly::union(&bars, &[])
                    .into_iter()
                    .next()
                    .unwrap_or_default()
            }
        };
        items.push(Item {
            id: track.id,
            net: track.net,
            kind: Kind::Track,
            piece,
            at: Point::new(
                (track.start.x + track.end.x) / 2.0,
                (track.start.y + track.end.y) / 2.0,
            ),
            rotation: 0.0,
        });
    }
    for zone in project
        .board
        .zones
        .iter()
        .filter(|zone| zone.layer == layer)
    {
        let Some(outline) = ring(&zone.outline) else {
            continue;
        };
        let mut piece = vec![outline];
        piece.extend(zone.cutouts.iter().filter_map(|cutout| ring(cutout)));
        let at = zone
            .outline
            .iter()
            .fold(Point::default(), |sum, point| sum.offset(*point));
        let count = zone.outline.len().max(1) as f64;
        items.push(Item {
            id: zone.id,
            net: zone.net,
            kind: Kind::Zone,
            piece,
            at: Point::new(at.x / count, at.y / count),
            rotation: 0.0,
        });
    }
    Ok(items)
}

/// Whether a pad has copper on `layer` of this board: a through-hole pad on
/// every copper layer, a surface pad only on the face its part sits on, where
/// the bottom face is the last copper layer of the stackup.
///
/// # Rules
/// - Never compare against `Layer::Back`: on a board with inner copper that
///   constant is the second layer, not the bottom.
///
/// # Dependencies
/// - `crates/doc/src/lib.rs` for `Footprint::board_side` and `Project::copper_layer_count`.
fn pad_reaches(project: &Project, footprint: &Footprint, pad: &Pad, layer: Layer) -> bool {
    let through = match pad.geometry.as_ref() {
        None => false,
        Some(g) => g.pad_type == "thru_hole" || g.layers.iter().any(|l| l == "*.Cu"),
    };
    if through {
        return true;
    }
    let face = if footprint.board_side() == Layer::Front {
        Layer::Front
    } else {
        Layer::from_index(project.copper_layer_count().saturating_sub(1)).unwrap_or(Layer::Back)
    };
    face == layer
}

/// A footprint's pads as world-space copper, with the combined part and pad
/// rotation that thermal spokes line up with.
fn pad_items(footprint: &Footprint) -> Vec<(Id, Pad, f64, Point, Piece)> {
    let mut result = Vec::new();
    for pad in &footprint.pads {
        let Some(piece) = piece_of(&pad.outline_points(), |point| {
            footprint.position.offset(point.rotate(-footprint.rotation))
        }) else {
            continue;
        };
        let at = footprint
            .pad_position(&pad.number)
            .unwrap_or_else(|| centroid(&piece));
        let rotation =
            footprint.rotation + pad.geometry.as_ref().map(|g| g.rotation).unwrap_or(0.0);
        result.push((footprint.id, pad.clone(), rotation, at, piece));
    }
    result
}

fn centroid(piece: &Piece) -> Point {
    let points = piece.first().map(Vec::as_slice).unwrap_or(&[]);
    if points.is_empty() {
        return Point::default();
    }
    let sum = points
        .iter()
        .fold(Point::default(), |sum, point| sum.offset(*point));
    Point::new(sum.x / points.len() as f64, sum.y / points.len() as f64)
}

fn piece_of(points: &[Point], map: impl Fn(Point) -> Point) -> Option<Piece> {
    let world: Ring = points.iter().map(|point| map(*point)).collect();
    ring(&world).map(|ring| vec![ring])
}

/// A closed ring from a vertex list, or none when it is degenerate.
pub(crate) fn ring(points: &[Point]) -> Option<Ring> {
    let mut ring: Ring = points
        .iter()
        .copied()
        .filter(|point| point.valid())
        .collect();
    if ring.first() == ring.last() {
        ring.pop();
    }
    if ring.len() < 3 || polygon_area(&ring).abs() < 1e-12 {
        return None;
    }
    if let Some(first) = ring.first().copied() {
        ring.push(first);
    }
    Some(ring)
}

/// The board surface as a piece: the traced outline, or the plain rectangle.
fn board_piece(project: &Project) -> Piece {
    let board = &project.board;
    if let Some(outline) = &board.outline
        && let Some(ring) = ring(outline)
    {
        return vec![ring];
    }
    vec![vec![
        Point::new(0.0, 0.0),
        Point::new(board.width, 0.0),
        Point::new(board.width, board.height),
        Point::new(0.0, board.height),
        Point::new(0.0, 0.0),
    ]]
}

/// A rule area rectangle as a piece.
fn rectangle(region: crate::RuleRegion) -> Piece {
    let corners = region.axis_corners();
    vec![vec![
        corners[0], corners[1], corners[2], corners[3], corners[0],
    ]]
}

fn rule_area_keepout_piece(project: &Project, area: &RuleArea) -> Option<Piece> {
    if area.active_keepout().is_none() {
        return None;
    }
    let region = area.keepout_rect();
    let corners = region.axis_corners();
    let ring: Ring = if let Some(reference) = area.footprint_ref.as_deref() {
        let component = project.component_by_reference(reference)?;
        let footprint = project
            .board
            .footprints
            .iter()
            .find(|f| f.component == component.id)?;
        corners
            .iter()
            .map(|corner| {
                footprint
                    .position
                    .offset(corner.rotate(-footprint.rotation))
            })
            .collect()
    } else {
        corners.to_vec()
    };
    if ring.len() < 3 {
        return None;
    }
    Some(vec![{
        let mut closed = ring;
        if closed.first() != closed.last() {
            if let Some(first) = closed.first().copied() {
                closed.push(first);
            }
        }
        closed
    }])
}

/// True when a zone's settings are all inside their bounds.
pub fn pour_is_valid(pour: &ZonePour) -> bool {
    pour.valid()
}

/// One object that enters a keepout area.
#[derive(Clone, Copy, Debug)]
pub struct KeepoutHit {
    /// The rule area that forbids it.
    pub area: Id,
    /// The track, via, pad or zone that entered.
    pub object: Id,
    /// Net the object carries.
    pub net: Id,
    /// What kind of copper the object is.
    pub kind: KeepoutKind,
}

impl KeepoutKind {
    /// Plain word for a finding message.
    pub fn label(self) -> &'static str {
        match self {
            Self::Tracks => "track",
            Self::Vias => "via",
            Self::Pads => "pad",
            Self::Zones => "zone",
        }
    }
}

/// Every track, via, pad and zone that enters a keepout area, on every copper
/// layer of the board. One hit per object and area, so a via that crosses on
/// several layers is reported once.
pub fn keepout_hits(project: &Project) -> Result<Vec<KeepoutHit>, String> {
    let mut hits: Vec<KeepoutHit> = Vec::new();
    for layer in project.copper_layers() {
        let items = items(project, layer)?;
        for area in &project.board.rule_areas {
            let Some(keepout) = area.active_keepout() else {
                continue;
            };
            if area.layer.is_some_and(|only| only != layer) {
                continue;
            }
            let Some(rect) = rule_area_keepout_piece(project, area) else {
                continue;
            };
            for item in &items {
                let kind = match item.kind {
                    Kind::Pad => KeepoutKind::Pads,
                    Kind::Via => KeepoutKind::Vias,
                    Kind::Track => KeepoutKind::Tracks,
                    Kind::Zone => KeepoutKind::Zones,
                    Kind::Hole => continue,
                };
                if !keepout.forbids(kind) || !area.keepout_applies(layer, item.net) {
                    continue;
                }
                if poly::intersect(
                    std::slice::from_ref(&item.piece),
                    std::slice::from_ref(&rect),
                )
                .is_empty()
                {
                    continue;
                }
                if hits
                    .iter()
                    .any(|hit| hit.area == area.id && hit.object == item.id)
                {
                    continue;
                }
                hits.push(KeepoutHit {
                    area: area.id,
                    object: item.id,
                    net: item.net,
                    kind,
                });
            }
        }
    }
    Ok(hits)
}
