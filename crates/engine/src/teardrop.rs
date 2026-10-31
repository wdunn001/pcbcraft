//! Grow and remove teardrop copper where tracks meet pads and vias.
use super::*;
use pcbcraft_geom::Point;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct TeardropParams {
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    layer: Option<Layer>,
    #[serde(default = "teardrop_depth")]
    depth: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeardropRemoveParams {
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    layer: Option<Layer>,
}

fn teardrop_depth() -> f64 {
    0.5
}

/// One track endpoint that lands on a pad or via: the junction point, the
/// unit direction along the track, the copper radius at the junction, the
/// track width, the track span, and the net and layer to tag the teardrop.
type Junction = (Point, (f64, f64), f64, f64, f64, Id, Layer);

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "teardrop.add",
            description: "Grow teardrop copper where tracks meet pads and vias so junctions survive etching and hand soldering; filter by net or layer, and junctions that already have a teardrop are left alone",
            input_schema: schema(
                json!({"net":{"type":"integer","minimum":1},"layer":{"type":"string","maxLength":64,"description":"copper layer name for the current stackup"},"depth":{"type":"number","description":"how far the teardrop runs along the track from the junction, millimetres; default 0.5"}}),
                &[],
            ),
        },
        CommandSpec {
            id: "teardrop.remove",
            description: "Take teardrop copper away again from every junction it was grown on, optionally filtered by net or layer",
            input_schema: schema(
                json!({"net":{"type":"integer","minimum":1},"layer":{"type":"string","maxLength":64,"description":"copper layer name for the current stackup"}}),
                &[],
            ),
        },
    ]
}

pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    Some(match command {
        "teardrop.add" => add(p, next, params),
        "teardrop.remove" => remove(p, params),
        _ => return None,
    })
}

fn add(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: TeardropParams = parse(params)?;
    let layer_filter = optional_command_layer(p, params)?;
    if a.net.is_some_and(|id| !p.nets.iter().any(|n| n.id == id)) {
        return Err(missing());
    }
    if !(0.05..=5.0).contains(&a.depth) {
        return Err(Error::Invalid(
            "teardrop depth must be between 0.05 and 5 millimetres".into(),
        ));
    }
    let mut junctions: Vec<Junction> = Vec::new();
    for track in &p.board.tracks {
        if layer_filter.is_some_and(|layer| track.layer != layer)
            || a.net.is_some_and(|net| track.net != net)
        {
            continue;
        }
        for (end, other) in [(track.start, track.end), (track.end, track.start)] {
            let dx = other.x - end.x;
            let dy = other.y - end.y;
            let span = (dx * dx + dy * dy).sqrt();
            if span < 0.05 {
                continue;
            }
            let radius = p
                .board
                .vias
                .iter()
                .find(|v| v.position.distance(end) <= v.diameter / 2.0 + 0.25)
                .map(|v| v.diameter / 2.0)
                .or_else(|| {
                    p.board.footprints.iter().find_map(|footprint| {
                        footprint.pads.iter().find_map(|pad| {
                            let anchor = footprint.pad_position(&pad.number).unwrap_or(pad.offset);
                            (end.distance(anchor) <= pad.bounding_radius() + 0.25)
                                .then(|| pad.bounding_radius())
                        })
                    })
                });
            let Some(radius) = radius else { continue };
            junctions.push((
                end,
                (dx / span, dy / span),
                radius,
                track.width,
                span,
                track.net,
                track.layer,
            ));
        }
    }
    if junctions.len() > 512 {
        return Err(Error::Invalid(
            "too many junctions for one teardrop pass; filter by net or layer".into(),
        ));
    }
    let total = junctions.len();
    let mut added = 0usize;
    for (end, u, radius, width, span, net, layer) in junctions {
        let half_w = (width / 2.0).max(0.05);
        let reach = radius.clamp(0.05, 3.0);
        if reach <= half_w + 0.01 {
            continue;
        }
        let depth = a.depth.min(span * 0.5);
        let v = (-u.1, u.0);
        let corners = [
            Point::new(end.x + v.0 * reach, end.y + v.1 * reach),
            Point::new(
                end.x + u.0 * depth + v.0 * half_w,
                end.y + u.1 * depth + v.1 * half_w,
            ),
            Point::new(
                end.x + u.0 * depth - v.0 * half_w,
                end.y + u.1 * depth - v.1 * half_w,
            ),
            Point::new(end.x - v.0 * reach, end.y - v.1 * reach),
        ];
        if !corners.iter().all(|point| point.valid()) {
            continue;
        }
        if p.board.zones.iter().any(|zone| {
            zone.kind.as_deref() == Some("teardrop")
                && zone.layer == layer
                && zone.net == net
                && zone
                    .outline
                    .first()
                    .is_some_and(|first| first.distance(corners[0]) < 0.01)
        }) {
            continue;
        }
        let mut outline = corners.to_vec();
        outline.push(corners[0]);
        let id = allocate(next)?;
        p.board.zones.push(pcbcraft_doc::CopperZone {
            id,
            net,
            layer,
            outline,
            cutouts: vec![],
            filled: true,
            kind: Some("teardrop".into()),
            pour: None,
        });
        added += 1;
    }
    Ok(json!({"added": added, "junctions": total}))
}

fn remove(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: TeardropRemoveParams = parse(params)?;
    let layer_filter = optional_command_layer(p, params)?;
    let before = p.board.zones.len();
    p.board.zones.retain(|zone| {
        !(zone.kind.as_deref() == Some("teardrop")
            && a.net.is_none_or(|net| zone.net == net)
            && layer_filter.is_none_or(|layer| zone.layer == layer))
    });
    let removed = before - p.board.zones.len();
    if removed == 0 {
        return Err(Error::Invalid(
            "no teardrop copper matches the request".into(),
        ));
    }
    Ok(json!({"removed": removed}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    #[test]
    fn teardrops_grow_on_junctions_undo_in_one_step_and_can_be_removed() -> Result<(), Error> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":[20.0,20.0]}),
            None,
        )?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":[60.0,40.0]}),
            None,
        )?;
        s.execute(
            "netlist.import",
            json!({"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let p1 = s.project.board.footprints[0]
            .pad_position("1")
            .ok_or_else(|| Error::Invalid("first pad anchor missing".into()))?;
        let p2 = s.project.board.footprints[1]
            .pad_position("1")
            .ok_or_else(|| Error::Invalid("second pad anchor missing".into()))?;
        let net = s
            .project
            .nets
            .first()
            .ok_or_else(|| Error::Invalid("imported net missing".into()))?
            .id;
        s.execute(
            "track.add",
            json!({"net":net,"start":p1,"end":p2,"width":0.3}),
            None,
        )?;
        s.execute(
            "via.place",
            json!({"net":net,"position":p2,"diameter":0.6,"drill":0.3}),
            None,
        )?;
        let before = s.inspect();
        let grown = s.execute("teardrop.add", json!({"depth":0.8}), None)?;
        assert_eq!(grown["result"]["added"], json!(2));
        assert_eq!(grown["result"]["junctions"], json!(2));
        assert_eq!(s.project.board.zones.len(), 2);
        assert!(
            s.project
                .board
                .zones
                .iter()
                .all(|z| z.kind.as_deref() == Some("teardrop") && z.filled),
            "grown copper is tagged poured teardrops"
        );
        assert!(
            s.project.board.zones.iter().all(|z| z.outline.len() == 5),
            "each teardrop is a closed kite"
        );
        s.undo()?;
        assert_eq!(
            s.project.board.zones.len(),
            0,
            "one undo removes the whole pass"
        );
        assert_eq!(
            s.inspect()["project"],
            before["project"],
            "undo leaves the design byte-identical"
        );
        s.execute("teardrop.add", json!({"depth":0.8}), None)?;
        let again = s.execute("teardrop.add", json!({"depth":0.8}), None)?;
        assert_eq!(again["result"]["added"], json!(0), "the pass is idempotent");
        assert_eq!(s.project.board.zones.len(), 2);
        let back = s.execute("teardrop.add", json!({"layer":"B.Cu"}), None)?;
        assert_eq!(
            back["result"]["added"],
            json!(0),
            "front tracks get no back teardrops"
        );
        assert!(
            s.execute("teardrop.add", json!({"net":99}), None).is_err(),
            "an unknown filter net is refused"
        );
        assert!(
            s.execute("teardrop.add", json!({"depth":0.0}), None)
                .is_err(),
            "depth is bounded"
        );
        s.execute("teardrop.add", json!({}), None)?;
        assert_eq!(s.project.board.zones.len(), 2, "default depth works");
        let removed = s.execute("teardrop.remove", json!({"net":net}), None)?;
        assert_eq!(removed["result"]["removed"], json!(2));
        assert!(
            s.project.board.zones.is_empty(),
            "removal leaves no tagged copper"
        );
        assert!(
            s.execute("teardrop.remove", json!({}), None).is_err(),
            "removing when nothing matches is refused"
        );
        Ok(())
    }
}
