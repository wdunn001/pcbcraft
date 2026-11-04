//! Silkscreen line width check for footprint library graphics.
//!
//! Context: called once from `drc` in `lib.rs`. Inspects library footprint
//! graphics on silk layers and reports any line thinner than the
//! `FabRules.silk_line` limit. Board text and drawn silk layers are
//! handled by `silk.rs` and are not examined here.
//!
//! Local rules:
//! - One finding per footprint: report only the thinnest violating line.
//! - Skip graphics without a width field (Text).
//! - Equal to the limit is acceptable; only strictly less than fires.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`Project`, `Footprint`, `Component`),
//! `crates/doc/src/fab.rs` (`FabRules`), `crates/library/src/lib.rs`
//! (`FootprintDefinition`, `Graphic`).
use super::Finding;
use pcbcraft_doc::{Id, Project};
use pcbcraft_library::Graphic;

/// Find the thinnest silk line on a footprint that violates the limit.
///
/// Returns `None` when the footprint has no library definition, no silk
/// graphics, or all silk lines meet the limit. Returns the thinnest
/// violating width when one is found.
fn thinnest_silk_line(project: &Project, footprint_id: Id, limit: f64) -> Option<f64> {
    let f = project
        .board
        .footprints
        .iter()
        .find(|fp| fp.id == footprint_id)?;
    let component = project.component(f.component)?;
    let definition = component.library_footprint.as_ref()?.definition.clone();
    let mut thinnest: Option<f64> = None;
    for graphic in &definition.graphics {
        let (width, layer) = match graphic {
            Graphic::Line { width, layer, .. }
            | Graphic::Rectangle { width, layer, .. }
            | Graphic::Circle { width, layer, .. }
            | Graphic::Arc { width, layer, .. }
            | Graphic::Polyline { width, layer, .. } => (*width, layer),
            Graphic::Text { .. } => continue,
        };
        if !layer.contains("SilkS") {
            continue;
        }
        if width + 1e-6 < limit {
            thinnest = Some(match thinnest {
                Some(current) => current.min(width),
                None => width,
            });
        }
    }
    thinnest
}

/// `silk_line` findings: library footprint silk lines thinner than the limit.
///
/// # Rules
/// Severity comes from `project.rule_severity` with default "warning".
/// One finding per footprint with at least one violating silk line,
/// reporting the thinnest width found.
///
/// # Dependencies
/// `crates/rules/src/lib.rs` (`drc`), `crates/doc/src/fab.rs` (`FabRules`),
/// `crates/library/src/lib.rs` (`FootprintDefinition`, `Graphic`).
///
/// # Do not
/// Do not examine board text or drawn silk layers here; `silk.rs` owns those.
pub(crate) fn silk_line_findings(project: &Project) -> Vec<Finding> {
    let limit = match project.board.rules.as_ref().and_then(|r| r.silk_line) {
        Some(l) => l,
        None => return vec![],
    };
    let mut result = vec![];
    for f in &project.board.footprints {
        let Some(thinnest) = thinnest_silk_line(project, f.id, limit) else {
            continue;
        };
        let reference = project
            .component(f.component)
            .map_or_else(|| format!("footprint {}", f.id), |c| c.reference.clone());
        result.push(Finding {
            code: "silk_line",
            severity: project.rule_severity("silk_line", "warning"),
            message: format!(
                "Silk line on {} is {:.3} mm wide; the maker prints at least {:.3} mm",
                reference, thinnest, limit
            ),
            objects: vec![f.id],
            hint: Some("Make the silk line thicker or remove it.".into()),
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use pcbcraft_doc::{Footprint, Pad, component_template};
    use pcbcraft_geom::Point;
    use pcbcraft_library::{FootprintDefinition, Graphic};

    fn part(p: &mut Project, id: Id, at: Point, width: f64, layer: &str) {
        if let Some(mut c) = component_template("resistor", id, format!("R{id}"), at) {
            c.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
                library: "test".into(),
                definition: FootprintDefinition {
                    name: "T".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![],
                    graphics: vec![Graphic::Line {
                        start: Point::new(-1.0, 0.0),
                        end: Point::new(1.0, 0.0),
                        width,
                        layer: layer.into(),
                    }],
                    model: None,
                    rules: None,
                    box_body: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                    retained: vec![],
                },
            });
            p.components.push(c);
        }
        p.board.footprints.push(Footprint {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: id + 10,
            component: id,
            position: at,
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
    }

    fn found(p: &Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code == "silk_line")
            .collect()
    }

    #[test]
    fn silk_thinner_than_the_limit_fires_and_thick_silk_stays_quiet() {
        // A footprint with a 0.10 mm silk line against a 0.15 mm limit fires;
        // a 0.15 mm line (equal) stays quiet.
        let mut p = Project::default();
        part(&mut p, 2, Point::new(10.0, 10.0), 0.10, "F.SilkS");
        p.board.rules = Some(FabRules {
            silk_line: Some(0.15),
            ..FabRules::default()
        });
        let f = found(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "silk_line");
        assert_eq!(f[0].severity, "warning");
        assert!(
            f[0].message.contains("0.100 mm"),
            "message must report the thin width: {}",
            f[0].message
        );
        assert!(
            f[0].message.contains("0.150 mm"),
            "message must report the limit: {}",
            f[0].message
        );

        // Same footprint, silk line exactly the limit: stays quiet.
        let mut p2 = Project::default();
        part(&mut p2, 2, Point::new(10.0, 10.0), 0.15, "F.SilkS");
        p2.board.rules = Some(FabRules {
            silk_line: Some(0.15),
            ..FabRules::default()
        });
        let f2 = found(&p2);
        assert!(f2.is_empty(), "equal to limit should not fire: {:?}", f2);

        // Same 0.10 mm line on a fab layer (not silk): stays quiet.
        let mut p3 = Project::default();
        part(&mut p3, 2, Point::new(10.0, 10.0), 0.10, "F.Fab");
        p3.board.rules = Some(FabRules {
            silk_line: Some(0.15),
            ..FabRules::default()
        });
        let f3 = found(&p3);
        assert!(f3.is_empty(), "non-silk layer should not fire: {:?}", f3);

        // No silk_line rule set: stays quiet even with thin line.
        let mut p4 = Project::default();
        part(&mut p4, 2, Point::new(10.0, 10.0), 0.10, "F.SilkS");
        p4.board.rules = Some(FabRules::default());
        let f4 = found(&p4);
        assert!(f4.is_empty(), "no rule should not fire: {:?}", f4);
    }
}
