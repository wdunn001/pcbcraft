//! Silkscreen checks: ink over a pad opening, and ink too close to one.
//!
//! Context: called once from `drc` in `lib.rs`. Reads library footprint silk,
//! `board.texts`, and drawn `ArtKind::Silk` user layers.
//!
//! Local rules:
//! - `silk_over_pad` needs no limit: ink that touches a pad opening is always
//!   an error. `silk_to_copper` uses `FabRules.silk_to_pad`, or
//!   `DEFAULT_SILK_TO_PAD_MM` when the board has no fabrication rules.
//! - The pad opening is the pad outline grown by the footprint's mask margin.
//! - Board text boxes are estimated from the glyph height and the character
//!   count at `TEXT_ADVANCE` times the height per character. The estimate is
//!   conservative, and the message says so.
//! - Drawn silk art has no side and prints on the top, like the plotters.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`BoardText`, `ArtLayer`, `Pad`),
//! `crates/doc/src/fab.rs` (`FabRules`), `crates/geom/src/lib.rs`.
use super::Finding;
use pcbcraft_doc::{ArtKind, ArtShape, Footprint, Id, Layer, Pad, Project};
use pcbcraft_geom::{Point, point_in_polygon, segment_distance};
use pcbcraft_library::Graphic;

/// Silk-to-pad limit used when the board carries no fabrication rules, in
/// millimetres.
const DEFAULT_SILK_TO_PAD_MM: f64 = 0.15;
/// Estimated glyph advance as a multiple of the glyph height.
const TEXT_ADVANCE: f64 = 0.8;

/// One piece of ink: its outline, and whether the outline is a filled box.
struct Ink {
    id: Id,
    what: String,
    side: Layer,
    outline: Vec<Point>,
    closed: bool,
    filled: bool,
    estimated: bool,
}

fn text_box(at: Point, text: &str, size: f64, rotate: f64) -> Vec<Point> {
    let width = text.chars().count() as f64 * size * TEXT_ADVANCE;
    [
        Point::new(0.0, 0.0),
        Point::new(width, 0.0),
        Point::new(width, size),
        Point::new(0.0, size),
    ]
    .iter()
    .map(|p| at.offset(p.rotate(rotate)))
    .collect()
}

fn authored_silk_side(layer: &str) -> Option<Layer> {
    match layer {
        "F.SilkS" => Some(Layer::Front),
        "B.SilkS" => Some(Layer::Back),
        _ => layer.contains("SilkS").then(|| {
            if layer.starts_with('B') {
                Layer::Back
            } else {
                Layer::Front
            }
        }),
    }
}

/// Board side for a silk graphic once the footprint is placed (mirrors courtyard).
fn placed_silk_side(f: &Footprint, layer: &str) -> Option<Layer> {
    let authored = authored_silk_side(layer)?;
    if f.board_side() == Layer::Front {
        return Some(authored);
    }
    Some(if authored == Layer::Front {
        Layer::Back
    } else {
        Layer::Front
    })
}

fn graphic_layer(g: &Graphic) -> &str {
    match g {
        Graphic::Line { layer, .. }
        | Graphic::Rectangle { layer, .. }
        | Graphic::Circle { layer, .. }
        | Graphic::Arc { layer, .. }
        | Graphic::Polyline { layer, .. }
        | Graphic::Text { layer, .. } => layer,
    }
}

fn to_world(f: &Footprint, p: Point) -> Point {
    let local = if f.board_side() == Layer::Back {
        Point::new(-p.x, p.y)
    } else {
        p
    };
    f.position.offset(local.rotate(-f.rotation))
}

fn library_ink(project: &Project) -> Vec<Ink> {
    let mut ink = vec![];
    for f in &project.board.footprints {
        let Some(def) = project
            .component(f.component)
            .and_then(|c| c.library_footprint.as_ref())
            .map(|l| &l.definition)
        else {
            continue;
        };
        let reference = project
            .component(f.component)
            .map_or_else(|| format!("footprint {}", f.id), |c| c.reference.clone());
        for g in &def.graphics {
            let layer = graphic_layer(g);
            let Some(side) = placed_silk_side(f, layer) else {
                continue;
            };
            if let Graphic::Text {
                position,
                text,
                size,
                angle,
                ..
            } = g
            {
                let height = size.y.max(size.x);
                let world = to_world(f, *position);
                ink.push(Ink {
                    id: f.id,
                    what: format!("silk text \"{text}\" on {reference}"),
                    side,
                    outline: text_box(world, text, height, *angle),
                    closed: true,
                    filled: true,
                    estimated: true,
                });
                continue;
            }
            let points: Vec<Point> = g
                .path_points()
                .into_iter()
                .map(|p| to_world(f, p))
                .collect();
            if points.len() < 2 {
                continue;
            }
            let closed = matches!(g, Graphic::Rectangle { .. } | Graphic::Circle { .. })
                || points
                    .first()
                    .zip(points.last())
                    .is_some_and(|(a, b)| a.distance(*b) < 1e-9);
            ink.push(Ink {
                id: f.id,
                what: format!("silk figure on {reference}"),
                side,
                outline: points,
                closed,
                filled: false,
                estimated: false,
            });
        }
    }
    ink
}

fn ink_of(project: &Project) -> Vec<Ink> {
    let mut ink = library_ink(project);
    for t in &project.board.texts {
        ink.push(Ink {
            id: t.id,
            what: format!("text \"{}\"", t.text),
            side: t.layer,
            outline: text_box(t.position, &t.text, t.size, t.rotation),
            closed: true,
            filled: true,
            estimated: true,
        });
    }
    for layer in project
        .board
        .user_layers
        .iter()
        .filter(|l| l.kind == ArtKind::Silk && l.visible)
    {
        for shape in &layer.shapes {
            let what = format!("silk figure on {}", layer.name);
            if let ArtShape::Text {
                at,
                text,
                size,
                rotate,
            } = shape
            {
                ink.push(Ink {
                    id: layer.id,
                    what: format!("silk text \"{text}\" on {}", layer.name),
                    side: Layer::Front,
                    outline: text_box(*at, text, *size, *rotate),
                    closed: true,
                    filled: true,
                    estimated: true,
                });
                continue;
            }
            for path in shape.paths() {
                ink.push(Ink {
                    id: layer.id,
                    what: what.clone(),
                    side: Layer::Front,
                    outline: path.points,
                    closed: path.closed,
                    filled: false,
                    estimated: false,
                });
            }
        }
    }
    ink
}

fn pad_on(pad: &Pad, side: Layer) -> bool {
    match &pad.geometry {
        None => side == Layer::Front,
        Some(g) => {
            if g.pad_type == "np_thru_hole" || g.pad_type == "unplated" {
                return false;
            }
            let wanted: &[&str] = if side == Layer::Back {
                &["B.Cu", "*.Cu", "F&B.Cu"]
            } else {
                &["F.Cu", "*.Cu", "F&B.Cu"]
            };
            if g.layers.iter().any(|l| wanted.contains(&l.as_str())) {
                return true;
            }
            let other = g.layers.iter().any(|l| l.ends_with(".Cu"));
            side == Layer::Front && !other
        }
    }
}

fn edges(points: &[Point], closed: bool) -> Vec<(Point, Point)> {
    let n = points.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    (0..count)
        .filter_map(|i| Some((*points.get(i)?, *points.get((i + 1) % n)?)))
        .collect()
}

/// Distance in millimetres from the ink to the pad outline; zero when they
/// touch or one lies inside the other.
fn ink_to_pad(ink: &Ink, pad: &[Point]) -> f64 {
    if ink.outline.iter().any(|p| point_in_polygon(*p, pad))
        || (ink.filled && pad.iter().any(|p| point_in_polygon(*p, &ink.outline)))
    {
        return 0.0;
    }
    let pad_edges = edges(pad, true);
    edges(&ink.outline, ink.closed)
        .iter()
        .flat_map(|(a, b)| {
            pad_edges
                .iter()
                .map(move |(c, d)| segment_distance(*a, *b, *c, *d))
        })
        .fold(f64::INFINITY, f64::min)
}

/// `silk_over_pad` and `silk_to_copper` for library silk, board text, and art.
///
/// # Rules
/// Severities come from `project.rule_severity`: error for `silk_over_pad`,
/// warning for `silk_to_copper`. One finding per ink piece and footprint.
///
/// # Dependencies
/// `crates/rules/src/lib.rs` (`drc`), `crates/doc/src/fab.rs` (`FabRules`).
///
pub(crate) fn silk_findings(project: &Project) -> Vec<Finding> {
    let limit = project
        .board
        .rules
        .as_ref()
        .and_then(|r| r.silk_to_pad)
        .unwrap_or(DEFAULT_SILK_TO_PAD_MM);
    let mut result = vec![];
    for ink in ink_of(project) {
        for f in &project.board.footprints {
            let margin = project
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
                .and_then(|l| l.definition.rules.as_ref())
                .and_then(|r| r.mask_margin)
                .filter(|m| m.is_finite() && m.abs() <= 1.0)
                .unwrap_or(0.0);
            let mut nearest = f64::INFINITY;
            for pad in f.pads.iter().filter(|p| pad_on(p, ink.side)) {
                let outline: Vec<Point> = pad
                    .outline_points()
                    .into_iter()
                    .map(|v| f.position.offset(v.rotate(-f.rotation)))
                    .collect();
                nearest = nearest.min(ink_to_pad(&ink, &outline) - margin);
            }
            if !nearest.is_finite() {
                continue;
            }
            let note = if ink.estimated {
                " (text size is estimated, and the estimate is conservative)"
            } else {
                ""
            };
            let reference = project
                .component(f.component)
                .map_or_else(|| format!("footprint {}", f.id), |c| c.reference.clone());
            if nearest <= 1e-9 {
                result.push(Finding {
                    code: "silk_over_pad",
                    severity: project.rule_severity("silk_over_pad", "error"),
                    message: format!(
                        "{} covers a pad of {reference}; silk on a pad opening stops solder{note}",
                        ink.what
                    ),
                    objects: vec![ink.id, f.id],
                });
            } else if nearest + 1e-9 < limit {
                result.push(Finding {
                    code: "silk_to_copper",
                    severity: project.rule_severity("silk_to_copper", "warning"),
                    message: format!(
                        "{} is {nearest:.3} mm from a pad of {reference}, at least {limit:.3} mm needed{note}",
                        ink.what
                    ),
                    objects: vec![ink.id, f.id],
                });
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use pcbcraft_doc::{
        ArtLayer, BoardText, Footprint, LibraryFootprint, Pad, component_template,
    };
    use pcbcraft_library::{FootprintDefinition, Graphic};

    /// One part with a round 1.0 mm pad at (10, 10).
    fn board() -> Project {
        let mut p = Project::default();
        if let Some(c) = component_template("resistor", 2, "R2".into(), Point::new(10.0, 10.0)) {
            p.components.push(c);
        }
        p.board.footprints.push(Footprint {
            id: 12,
            component: 2,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: None,
            }],
        });
        p
    }
    fn add_silk_line(p: &mut Project, y: f64) {
        p.board.user_layers.push(ArtLayer {
            id: 40,
            name: "Top Silk".into(),
            kind: ArtKind::Silk,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![ArtShape::Line {
                start: Point::new(8.0, y),
                end: Point::new(12.0, y),
            }],
        });
    }
    fn silk(p: &Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code.starts_with("silk_o") || f.code == "silk_to_copper")
            .collect()
    }
    fn text(p: &mut Project, at: Point, layer: Layer) {
        p.board.texts.push(BoardText {
            id: 41,
            text: "AB".into(),
            position: at,
            size: 1.0,
            layer,
            rotation: 0.0,
            component: None,
        });
    }

    #[test]
    fn board_text_over_a_pad_is_silk_over_pad() {
        let mut p = board();
        // Two characters at 1 mm: an estimated box 1.6 by 1 mm from (9.5, 9.7).
        text(&mut p, Point::new(9.5, 9.7), Layer::Front);
        let f = silk(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "silk_over_pad");
        assert_eq!(f[0].severity, "error");
        assert_eq!(f[0].objects, vec![41, 12]);
        assert!(f[0].message.contains("conservative"));
        assert!(f[0].message.contains("R2"));
    }
    #[test]
    fn board_text_on_the_other_side_does_not_cover_a_top_pad() {
        let mut p = board();
        text(&mut p, Point::new(9.5, 9.7), Layer::Back);
        assert!(silk(&p).is_empty());
    }
    #[test]
    fn library_silk_line_over_a_pad_is_silk_over_pad() {
        let mut p = board();
        if let Some(c) = p.components.first_mut() {
            c.library_footprint = Some(LibraryFootprint {
                library: "test".into(),
                definition: FootprintDefinition {
                    name: "T".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![],
                    graphics: vec![Graphic::Line {
                        start: Point::new(-2.0, 0.0),
                        end: Point::new(2.0, 0.0),
                        width: 0.12,
                        layer: "F.SilkS".into(),
                    }],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            });
        }
        let f = silk(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "silk_over_pad");
        assert_eq!(f[0].objects, vec![12, 12]);
        assert!(f[0].message.contains("R2"));
    }
    #[test]
    fn drawn_silk_layer_over_a_pad_is_reported_here_not_by_277() {
        let mut p = board();
        add_silk_line(&mut p, 10.0);
        let f = silk(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "silk_over_pad");
        assert_eq!(f[0].objects, vec![40, 12]);
        assert!(!f[0].message.contains("conservative"));
    }
    #[test]
    fn silk_close_to_a_pad_is_silk_to_copper() {
        let mut p = board();
        // The pad edge is at y = 10.5; the line sits 0.1 mm above it.
        add_silk_line(&mut p, 10.6);
        let f = silk(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "silk_to_copper");
        assert_eq!(f[0].severity, "warning");
        assert!(f[0].message.contains("0.100 mm"), "{}", f[0].message);
        assert!(f[0].message.contains("0.150 mm"));
    }
    #[test]
    fn fab_rules_silk_limit_replaces_the_default() {
        let mut p = board();
        add_silk_line(&mut p, 10.6);
        p.board.rules = Some(FabRules {
            silk_to_pad: Some(0.05),
            ..FabRules::default()
        });
        assert!(silk(&p).is_empty(), "0.1 mm clears a 0.05 mm limit");
    }
    #[test]
    fn mask_margin_widens_the_pad_opening() {
        let mut p = board();
        add_silk_line(&mut p, 10.6);
        if let Some(c) = p.components.first_mut() {
            c.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
                library: "test".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "T".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![],
                    graphics: vec![],
                    model: None,
                    rules: Some(pcbcraft_library::FootprintRules {
                        clearance: None,
                        mask_margin: Some(0.12),
                        paste_margin: None,
                    }),
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            });
        }
        let f = silk(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(
            f[0].code, "silk_over_pad",
            "0.1 mm gap is inside a 0.12 mm margin"
        );
    }
    #[test]
    fn silk_clear_of_pads_is_clean() {
        let mut p = board();
        add_silk_line(&mut p, 11.0);
        text(&mut p, Point::new(5.0, 5.0), Layer::Front);
        assert!(silk(&p).is_empty());
    }
    #[test]
    fn clean_board_has_no_courtyard_or_silk_findings() {
        let p = board();
        assert!(
            drc(&p)
                .iter()
                .all(|f| !f.code.starts_with("silk_") && f.code != "courtyard_overlap")
        );
    }
}
