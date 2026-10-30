//! `track.round_corners`: fillet joined same-net straight tracks.
use crate::{Error, missing};
use pcbcraft_doc::{Id, Layer, Project, Track};
use pcbcraft_geom::track_corner::{TrackFillet, arc_chord_points, track_fillet};
use pcbcraft_geom::{Point, arc_through};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};

const MAX_CORNERS: usize = 4096;
const MAX_CHORDS: usize = 64;
const DEFAULT_TOLERANCE: f64 = 0.02;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    Arc,
    Segments,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoundCornersParams {
    #[serde(default)]
    ids: Option<Vec<Id>>,
    #[serde(default)]
    radius: f64,
    #[serde(default)]
    layer: Option<Layer>,
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    style: Option<String>,
    #[serde(default)]
    tolerance: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
struct CornerPair {
    a: Id,
    b: Id,
    corner: Point,
}

#[derive(Clone, Copy, Debug)]
struct LegSnapshot {
    start: Point,
    end: Point,
    length: f64,
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) <= 1e-6
}

fn style_from(s: Option<&str>) -> Result<Style, Error> {
    match s {
        None | Some("arc") => Ok(Style::Arc),
        Some("segments") => Ok(Style::Segments),
        Some(_) => Err(Error::Invalid("style must be arc or segments".into())),
    }
}

fn drc_clearance_keys(p: &Project) -> BTreeSet<(String, Vec<Id>)> {
    pcbcraft_rules::drc(p)
        .into_iter()
        .filter(|f| f.code == "clearance" || f.code == "copper_short")
        .map(|f| {
            let mut objects = f.objects;
            objects.sort_unstable();
            (f.code, objects)
        })
        .collect()
}

fn joint_at(p: &Project, corner: Point, net: Id, layer: Layer) -> Option<(Id, Id)> {
    let mut hits: Vec<Id> = p
        .board
        .tracks
        .iter()
        .filter(|t| {
            t.net == net
                && t.layer == layer
                && t.arc.is_none()
                && (near(t.start, corner) || near(t.end, corner))
        })
        .map(|t| t.id)
        .collect();
    hits.sort_unstable();
    hits.dedup();
    if hits.len() == 2 {
        Some((hits[0], hits[1]))
    } else {
        None
    }
}

fn skip_reason_at_joint(p: &Project, corner: Point, net: Id, layer: Layer) -> Option<&'static str> {
    let count = p
        .board
        .tracks
        .iter()
        .filter(|t| {
            t.net == net
                && t.layer == layer
                && (near(t.start, corner) || near(t.end, corner))
        })
        .count();
    if count != 2 {
        if count >= 3 {
            return Some("three or more tracks meet here");
        }
        return Some("not a two-track corner");
    }
    if p.board.tracks.iter().any(|t| {
        t.net == net
            && t.layer == layer
            && t.arc.is_some()
            && (near(t.start, corner) || near(t.end, corner))
    }) {
        return Some("a curved track touches this corner");
    }
    if p.board.vias.iter().any(|v| {
        v.net == net && corner.distance(v.position) <= v.diameter / 2.0 + 1e-6
    }) {
        return Some("a via sits on this corner");
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            if pad.net != Some(net) {
                continue;
            }
            let on_layer = pad
                .geometry
                .as_ref()
                .map(|g| g.layers.iter().any(|l| l == layer.name() || l == "*.Cu"))
                .unwrap_or(layer == Layer::Front);
            if !on_layer {
                continue;
            }
            let anchor = f.pad_position(&pad.number).unwrap_or(pad.offset);
            if corner.distance(anchor) <= pad.bounding_radius() + 1e-6 {
                return Some("a pad sits on this corner");
            }
        }
    }
    None
}

fn away_from(corner: Point, track: &Track) -> Option<Point> {
    if near(track.start, corner) {
        Some(track.end)
    } else if near(track.end, corner) {
        Some(track.start)
    } else {
        None
    }
}

fn shorten_at(track: &mut Track, corner: Point, target: Point) {
    if near(track.start, corner) {
        track.start = target;
    } else {
        track.end = target;
    }
}

fn apply_fillet(
    p: &mut Project,
    pair: CornerPair,
    fillet: TrackFillet,
    style: Style,
    tolerance: f64,
    width: f64,
    net: Id,
    layer: Layer,
    next: &mut Id,
) -> Result<Vec<Id>, Error> {
    let ia = p
        .board
        .tracks
        .iter()
        .position(|t| t.id == pair.a)
        .ok_or_else(missing)?;
    let ib = p
        .board
        .tracks
        .iter()
        .position(|t| t.id == pair.b)
        .ok_or_else(missing)?;
    shorten_at(&mut p.board.tracks[ia], pair.corner, fillet.tangent_a);
    shorten_at(&mut p.board.tracks[ib], pair.corner, fillet.tangent_b);
    let mut added = vec![];
    match style {
        Style::Arc => {
            let id = crate::allocate(next)?;
            added.push(id);
            p.board.tracks.push(Track {
                id,
                net,
                start: fillet.tangent_a,
                end: fillet.tangent_b,
                width,
                layer,
                arc: Some(fillet.arc_mid),
            });
        }
        Style::Segments => {
            let arc = arc_through(fillet.tangent_a, fillet.arc_mid, fillet.tangent_b)
                .ok_or_else(|| Error::Invalid("arc sample failed".into()))?;
            let pts = arc_chord_points(&arc, tolerance, MAX_CHORDS);
            for w in pts.windows(2) {
                if w[0].distance(w[1]) < 1e-6 {
                    continue;
                }
                let id = crate::allocate(next)?;
                added.push(id);
                p.board.tracks.push(Track {
                    id,
                    net,
                    start: w[0],
                    end: w[1],
                    width,
                    layer,
                    arc: None,
                });
            }
        }
    }
    Ok(added)
}

fn collect_corners(p: &Project, params: &RoundCornersParams) -> Vec<CornerPair> {
    let selected: Option<HashSet<Id>> = params.ids.as_ref().map(|v| v.iter().copied().collect());
    let mut seen: HashSet<(Id, Id)> = HashSet::new();
    let mut corners = vec![];
    let mut tracks: Vec<&Track> = p.board.tracks.iter().collect();
    tracks.sort_by_key(|t| t.id);
    for t in tracks {
        if t.arc.is_some() {
            continue;
        }
        if params.net.is_some_and(|n| t.net != n) {
            continue;
        }
        if params.layer.is_some_and(|l| t.layer != l) {
            continue;
        }
        if selected.as_ref().is_some_and(|s| !s.contains(&t.id)) {
            continue;
        }
        for corner in [t.start, t.end] {
            let Some((a, b)) = joint_at(p, corner, t.net, t.layer) else {
                continue;
            };
            if selected.as_ref().is_some_and(|s| !s.contains(&a) || !s.contains(&b)) {
                continue;
            }
            let key = if a < b { (a, b) } else { (b, a) };
            if seen.insert(key) {
                corners.push(CornerPair {
                    a: key.0,
                    b: key.1,
                    corner,
                });
            }
        }
    }
    corners.sort_by(|x, y| x.a.cmp(&y.a).then(x.b.cmp(&y.b)));
    corners
}

pub fn run(
    p: &mut Project,
    next: &mut Id,
    params: RoundCornersParams,
) -> Result<Value, Error> {
    if params.net.is_some_and(|id| !p.nets.iter().any(|n| n.id == id)) {
        return Err(missing());
    }
    let style = style_from(params.style.as_deref())?;
    let tolerance = params.tolerance.unwrap_or(DEFAULT_TOLERANCE);
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(Error::Invalid("tolerance must be positive".into()));
    }
    if !params.radius.is_finite() || params.radius < 0.0 {
        return Err(Error::Invalid("radius must be zero or positive".into()));
    }
    let corners = collect_corners(p, &params);
    if corners.is_empty() {
        return Err(Error::Invalid("no corners match the request".into()));
    }
    let snapshots: HashMap<Id, LegSnapshot> = p
        .board
        .tracks
        .iter()
        .map(|t| {
            (
                t.id,
                LegSnapshot {
                    start: t.start,
                    end: t.end,
                    length: t.start.distance(t.end),
                },
            )
        })
        .collect();
    let mut rounded = 0usize;
    let mut skipped: Vec<Value> = vec![];
    for pair in corners {
        if rounded + skipped.len() >= MAX_CORNERS {
            return Err(Error::Invalid(
                "track.round_corners would change too many corners at once".into(),
            ));
        }
        let track_a = p
            .board
            .tracks
            .iter()
            .find(|t| t.id == pair.a)
            .cloned()
            .ok_or_else(missing)?;
        let track_b = p
            .board
            .tracks
            .iter()
            .find(|t| t.id == pair.b)
            .cloned()
            .ok_or_else(missing)?;
        if track_a.arc.is_some() || track_b.arc.is_some() {
            skipped.push(json!({"a": pair.a, "b": pair.b, "reason": "a curved track touches this corner"}));
            continue;
        }
        if let Some(reason) = skip_reason_at_joint(p, pair.corner, track_a.net, track_a.layer) {
            skipped.push(json!({"a": pair.a, "b": pair.b, "reason": reason}));
            continue;
        }
        let away_a = away_from(pair.corner, &track_a).ok_or_else(missing)?;
        let away_b = away_from(pair.corner, &track_b).ok_or_else(missing)?;
        let snap_a = snapshots.get(&pair.a).ok_or_else(missing)?;
        let snap_b = snapshots.get(&pair.b).ok_or_else(missing)?;
        let leg_a = snap_a.length;
        let leg_b = snap_b.length;
        let width = track_a.width.min(track_b.width);
        let radius = if params.radius <= 0.0 {
            width * 3.0
        } else {
            params.radius
        };
        let Some(fillet) = track_fillet(
            pair.corner,
            away_a,
            away_b,
            leg_a,
            leg_b,
            radius,
        ) else {
            skipped.push(json!({"a": pair.a, "b": pair.b, "reason": "corner is too shallow to round"}));
            continue;
        };
        if !fillet.tangent_a.valid() || !fillet.tangent_b.valid() {
            skipped.push(json!({"a": pair.a, "b": pair.b, "reason": "tangent point is out of range"}));
            continue;
        }
        let before = drc_clearance_keys(p);
        let mut trial = p.clone();
        let mut trial_next = *next;
        let added = apply_fillet(
            &mut trial,
            pair,
            fillet,
            style,
            tolerance,
            width,
            track_a.net,
            track_a.layer,
            &mut trial_next,
        )?;
        trial.validate().map_err(Error::Invalid)?;
        let after = drc_clearance_keys(&trial);
        let new_violation = after.iter().any(|finding| !before.contains(finding));
        if new_violation {
            skipped.push(json!({"a": pair.a, "b": pair.b, "reason": "clearance would be broken"}));
            continue;
        }
        apply_fillet(
            p,
            pair,
            fillet,
            style,
            tolerance,
            width,
            track_a.net,
            track_a.layer,
            next,
        )?;
        rounded += 1;
        let _ = added;
    }
    Ok(json!({"rounded": rounded, "skipped": skipped}))
}
