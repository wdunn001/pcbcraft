//! Per-layer board shapes: give a main (copper) layer a shape of its own,
//! send it back to following the board, and list which layers follow.
//!
//! The board outline is the default shape of every layer. Layer shapes are
//! independent of each other and of the outline: they may differ and overlap.
//! Silkscreen, mask and paste have no shape of their own, so a command that
//! names one is refused. Each edit is checked first and written only when it
//! passes, then `Board::refresh_extent` updates the width and height cache.
//!
//! # Dependencies
//! - crates/doc/src/layer_shape.rs
//! - crates/doc/src/outline.rs
//! - crates/geom/src/lib.rs#polygon_is_simple
use super::*;
use pcbcraft_doc::{LayerRef, LayerShape};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "board.layer_shape_set",
            description: "Give one copper layer its own shape: a closed polygon of 3 to 64 distinct corners, optionally with arcs. Silkscreen, mask and paste follow the shape of their copper layer. Layer shapes may differ and overlap",
            input_schema: schema(
                json!({"layer":{"type":"string","description":"copper layer such as F.Cu, In1.Cu or B.Cu"},"points":{"type":"array","items":point_schema(),"maxItems":64},"arcs":{"type":"array","maxItems":64,"description":"optional, one entry per segment: a point the arc passes through, or null for a straight segment","items":{"oneOf":[point_schema(),{"type":"null"}]}}}),
                &["layer", "points"],
            ),
        },
        CommandSpec {
            id: "board.layer_shape_clear",
            description: "Send one copper layer back to following the board outline",
            input_schema: schema(
                json!({"layer":{"type":"string","description":"copper layer such as F.Cu, In1.Cu or B.Cu"}}),
                &["layer"],
            ),
        },
        CommandSpec {
            id: "board.layer_shapes",
            description: "List every copper layer in stack order with whether it follows the board outline and how many corners its shape has, plus the extent that covers the board and all layer shapes",
            input_schema: schema(json!({}), &[]),
        },
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetShape {
    layer: String,
    points: Vec<Point>,
    #[serde(default)]
    arcs: Vec<Option<Point>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClearShape {
    layer: String,
}

/// Check a closed polygon with optional arcs and give back its extent.
///
/// # Rules
/// - 3 to 64 distinct, finite vertices in the first quadrant with real area.
/// - One arc entry per segment when arcs are given.
/// - The outline may not cross itself.
pub(super) fn checked_shape(
    points: &[Point],
    arcs: &[Option<Point>],
) -> Result<(Point, Point), Error> {
    if points.len() < 3 || points.len() > 64 {
        return Err(Error::Invalid(
            "The outline needs at least three points.".into(),
        ));
    }
    let distinct: std::collections::BTreeSet<(u64, u64)> = points
        .iter()
        .map(|p| (p.x.to_bits(), p.y.to_bits()))
        .collect();
    if distinct.len() != points.len() {
        return Err(Error::Invalid("Outline vertices must be distinct.".into()));
    }
    if !points.iter().all(|p| p.valid() && p.x >= 0.0 && p.y >= 0.0) {
        return Err(Error::Invalid(
            "outline vertices must be in range and inside the first quadrant".into(),
        ));
    }
    if pcbcraft_doc::outline_area(points) < 0.01 {
        return Err(Error::Invalid("the traced outline has no area".into()));
    }
    if !arcs.is_empty() && arcs.len() != points.len() {
        return Err(Error::Invalid(
            "arcs needs one entry per segment, null for a straight one".into(),
        ));
    }
    pcbcraft_doc::validate_outline_arcs(points, arcs).map_err(Error::Invalid)?;
    if arcs.is_empty() && !pcbcraft_geom::polygon_is_simple(points) {
        return Err(Error::Invalid("The outline crosses itself.".into()));
    }
    pcbcraft_doc::outline_extent(points, arcs)
        .ok_or_else(|| Error::Invalid("The outline needs at least three points.".into()))
}

/// The copper key a command names, refusing a sub-layer in plain words.
fn copper_key(p: &Project, layer: &str) -> Result<String, Error> {
    match p.board.layer_ref(layer).map_err(Error::Invalid)? {
        LayerRef::Copper(key) => Ok(key),
        LayerRef::Sub { label, .. } => Err(Error::Invalid(format!(
            "{label} follows its layer's shape; set the shape on the copper layer"
        ))),
    }
}

/// Dispatch for the layer shape commands; `None` when `command` is not one.
///
/// # Rules
/// - Writes go through `Board::refresh_extent` so the cache stays in step.
///
/// # Do not
/// - Do not write `p.board.width` or `height` here.
pub(super) fn run(p: &mut Project, command: &str, params: &Value) -> Option<Result<Value, Error>> {
    Some(match command {
        "board.layer_shape_set" => set(p, params),
        "board.layer_shape_clear" => clear(p, params),
        "board.layer_shapes" => list(p, params),
        _ => return None,
    })
}

fn set(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: SetShape = parse(params)?;
    let key = copper_key(p, &a.layer)?;
    checked_shape(&a.points, &a.arcs)?;
    let arcs = if a.arcs.iter().all(Option::is_none) {
        Vec::new()
    } else {
        a.arcs
    };
    let corners = a.points.len();
    p.board.layer_shapes.insert(
        key.clone(),
        LayerShape {
            outline: a.points,
            arcs,
        },
    );
    p.board.refresh_extent();
    Ok(json!({"layer": key, "corners": corners, "width": p.board.width, "height": p.board.height}))
}

fn clear(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ClearShape = parse(params)?;
    let key = copper_key(p, &a.layer)?;
    if p.board.layer_shapes.remove(&key).is_none() {
        return Err(Error::Invalid(format!(
            "Layer {key} already follows the board"
        )));
    }
    p.board.refresh_extent();
    Ok(json!({"layer": key, "width": p.board.width, "height": p.board.height}))
}

fn list(p: &Project, params: &Value) -> Result<Value, Error> {
    if params.as_object().is_none_or(|m| !m.is_empty()) {
        return Err(Error::Invalid(
            "board.layer_shapes takes an empty object".into(),
        ));
    }
    let (lo, hi) = p.board.extent();
    let rows: Vec<Value> = p
        .board
        .layer_shapes_in_order()
        .into_iter()
        .map(|(layer, follows_board, corners)| {
            json!({"layer": layer, "follows_board": follows_board, "corners": corners})
        })
        .collect();
    Ok(json!({"layers": rows, "extent": {"min": lo, "max": hi}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(list: &[(f64, f64)]) -> Value {
        json!(
            list.iter()
                .map(|(x, y)| json!({"x": x, "y": y}))
                .collect::<Vec<_>>()
        )
    }

    fn l_shape() -> Value {
        pts(&[
            (0.0, 0.0),
            (40.0, 0.0),
            (40.0, 10.0),
            (10.0, 10.0),
            (10.0, 30.0),
            (0.0, 30.0),
        ])
    }

    fn u_shape() -> Value {
        pts(&[
            (5.0, 5.0),
            (45.0, 5.0),
            (45.0, 35.0),
            (35.0, 35.0),
            (35.0, 15.0),
            (15.0, 15.0),
            (15.0, 35.0),
            (5.0, 35.0),
        ])
    }

    #[test]
    fn an_l_shaped_outline_reports_its_extent_and_the_cache_follows() -> Result<(), String> {
        let mut s = Session::default();
        s.execute("board.set_outline", json!({"points": l_shape()}), None)
            .map_err(|e| e.to_string())?;
        let (lo, hi) = s.project.board.extent();
        assert_eq!((lo.x, lo.y, hi.x, hi.y), (0.0, 0.0, 40.0, 30.0));
        assert_eq!(s.project.board.extent_size(), (40.0, 30.0));
        assert_eq!(
            (s.project.board.width, s.project.board.height),
            (40.0, 30.0)
        );
        Ok(())
    }

    #[test]
    fn set_rectangle_does_what_resize_does() -> Result<(), String> {
        let mut a = Session::default();
        let mut b = Session::default();
        a.execute("board.resize", json!({"width": 70.0, "height": 25.0}), None)
            .map_err(|e| e.to_string())?;
        b.execute(
            "board.set_rectangle",
            json!({"width": 70.0, "height": 25.0}),
            None,
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(a.project.board, b.project.board);
        assert_eq!(a.project.board.outline.as_ref().map(Vec::len), Some(4));
        assert_eq!(a.project.board.extent_size(), (70.0, 25.0));
        assert!(
            a.execute("board.resize", json!({"width": 0.0, "height": 5.0}), None)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn a_layer_shape_set_then_undo_restores() -> Result<(), String> {
        let mut s = Session::default();
        let before = s.project.board.clone();
        s.execute(
            "board.layer_shape_set",
            json!({"layer": "B.Cu", "points": l_shape()}),
            None,
        )
        .map_err(|e| e.to_string())?;
        assert!(s.project.board.layer_shapes.contains_key("B.Cu"));
        s.undo().map_err(|e| e.to_string())?;
        assert_eq!(s.project.board, before);
        Ok(())
    }

    #[test]
    fn two_layers_keep_different_overlapping_shapes_through_save_load_and_undo()
    -> Result<(), String> {
        let mut s = Session::default();
        s.execute(
            "board.set_rectangle",
            json!({"width": 30.0, "height": 20.0}),
            None,
        )
        .map_err(|e| e.to_string())?;
        s.execute(
            "board.layer_shape_set",
            json!({"layer": "F.Cu", "points": l_shape()}),
            None,
        )
        .map_err(|e| e.to_string())?;
        s.execute(
            "board.layer_shape_set",
            json!({"layer": "B.Cu", "points": u_shape()}),
            None,
        )
        .map_err(|e| e.to_string())?;
        let (lo, hi) = s.project.board.extent();
        assert_eq!((lo.x, lo.y, hi.x, hi.y), (0.0, 0.0, 45.0, 35.0));
        assert_eq!(
            (s.project.board.width, s.project.board.height),
            (45.0, 35.0)
        );
        let text = serde_json::to_string(&s.project).map_err(|e| e.to_string())?;
        let loaded = pcbcraft_io::read_project(&text)?;
        assert_eq!(loaded.board.layer_shapes, s.project.board.layer_shapes);
        assert_eq!(loaded.board.layer_shapes["F.Cu"].outline.len(), 6);
        assert_eq!(loaded.board.layer_shapes["B.Cu"].outline.len(), 8);
        s.undo().map_err(|e| e.to_string())?;
        assert!(s.project.board.layer_shapes.contains_key("F.Cu"));
        assert!(!s.project.board.layer_shapes.contains_key("B.Cu"));
        assert_eq!(s.project.board.extent_size(), (40.0, 30.0));
        s.execute("board.layer_shape_clear", json!({"layer": "F.Cu"}), None)
            .map_err(|e| e.to_string())?;
        assert!(s.project.board.layer_shapes.is_empty());
        assert_eq!(s.project.board.extent_size(), (30.0, 20.0));
        Ok(())
    }

    #[test]
    fn a_sub_layer_follows_its_parent_and_cannot_hold_a_shape() -> Result<(), String> {
        let mut s = Session::default();
        s.execute(
            "board.set_rectangle",
            json!({"width": 30.0, "height": 20.0}),
            None,
        )
        .map_err(|e| e.to_string())?;
        s.execute(
            "board.layer_shape_set",
            json!({"layer": "B.Cu", "points": u_shape()}),
            None,
        )
        .map_err(|e| e.to_string())?;
        let board = &s.project.board;
        let own = board.shape_of("B.Cu").0.len();
        assert_eq!(own, 8);
        assert_eq!(board.shape_of("B.SilkS").0.len(), 8);
        assert_eq!(board.shape_of("B.Mask").0.len(), 8);
        assert_eq!(board.shape_of("B.Paste").0.len(), 8);
        assert_eq!(board.shape_of("F.SilkS").0.len(), 4);
        let Err(message) = s.execute(
            "board.layer_shape_set",
            json!({"layer": "F.SilkS", "points": l_shape()}),
            None,
        ) else {
            return Err("a silkscreen shape was accepted".into());
        };
        assert!(
            message.to_string().contains(
                "Silkscreen follows its layer's shape; set the shape on the copper layer"
            )
        );
        let rows = s.project.board.layer_shapes_in_order();
        assert_eq!(
            rows,
            vec![
                ("F.Cu".to_string(), true, 4),
                ("B.Cu".to_string(), false, 8)
            ]
        );
        let listed = s
            .execute("board.layer_shapes", json!({}), None)
            .map_err(|e| e.to_string())?;
        assert_eq!(listed["result"]["layers"][1]["follows_board"], json!(false));
        Ok(())
    }

    #[test]
    fn invalid_layer_shapes_are_refused_in_plain_words() -> Result<(), String> {
        let mut s = Session::default();
        let bad = [
            json!({"layer":"F.Cu","points": pts(&[(0.0,0.0),(5.0,5.0)])}),
            json!({"layer":"F.Cu","points": pts(&[(0.0,0.0),(10.0,10.0),(10.0,0.0),(0.0,10.0)])}),
            json!({"layer":"F.Cu","points": pts(&[(0.0,0.0),(1.0,0.0),(1.0,1.0)]), "arcs":[null]}),
            json!({"layer":"In9.Cu","points": l_shape()}),
        ];
        for params in bad {
            assert!(
                s.execute("board.layer_shape_set", params.clone(), None)
                    .is_err(),
                "{params} was accepted"
            );
        }
        let nan = s.execute(
            "board.layer_shape_set",
            json!({"layer":"F.Cu","points":[{"x":0.0,"y":0.0},{"x":null,"y":1.0},{"x":4.0,"y":4.0}]}),
            None,
        );
        assert!(nan.is_err());
        assert!(
            s.execute("board.layer_shape_clear", json!({"layer":"F.Cu"}), None)
                .is_err()
        );
        assert!(s.project.board.layer_shapes.is_empty());
        Ok(())
    }
}
