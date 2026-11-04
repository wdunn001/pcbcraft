//! Layout cells for the Parts grid: where each footprint sits on the board.
//!
//! Context: issue #169. When the Layout columns checkbox is on, the Parts grid
//! shows five read-only columns after the Part number column: Side, Rotation,
//! Locked, X and Y. These come from the placed footprint, not the component.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (Project, Footprint, Layer, Point)
//! - crates/ui-egui/src/parts_table.rs (calls layout_cells for each row)
//!
//! # Do not
//! - Write footprints from here; the grid only reads them.
use pcbcraft_doc::{Footprint, Id, Layer, Project};

/// The five layout values shown for one part.
#[derive(Clone, Debug)]
pub(crate) struct LayoutCells {
    pub side: String,
    pub rotation: String,
    pub locked: bool,
    pub x: String,
    pub y: String,
}

/// Look up the placed footprint for a component and return its layout cells.
///
/// Why: the Parts grid shows footprint position only when the checkbox is on,
/// and only for parts that have a placed footprint.
///
/// # Rules
/// - Returns None when no footprint has component == component_id.
/// - Side reads 'Top' or 'Bottom' from Footprint::board_side.
/// - Rotation shows one decimal; X and Y show two decimals in millimetres.
/// - Locked is true only when Footprint::locked is Some(true).
///
/// # Dependencies
/// - crates/doc/src/lib.rs (Project::footprints, Footprint::board_side)
///
/// # Do not
/// - Create a footprint for a part that has none.
pub(crate) fn layout_cells(project: &Project, component_id: Id) -> Option<LayoutCells> {
    let footprint = project
        .footprints
        .iter()
        .find(|f| f.component == component_id)?;
    let side = if footprint.board_side() == Layer::Front {
        "Top".to_owned()
    } else {
        "Bottom".to_owned()
    };
    let locked = footprint.locked == Some(true);
    let x = format!("{:.2}", footprint.position.x);
    let y = format!("{:.2}", footprint.position.y);
    let rotation = format!("{:.1}", footprint.rotation);
    Some(LayoutCells {
        side,
        rotation,
        locked,
        x,
        y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Point;

    #[test]
    fn a_back_side_footprint_reports_bottom() {
        let mut project = Project::default();
        let comp_id = pcbcraft_doc::Id::new("comp");
        let fp_id = pcbcraft_doc::Id::new("fp");
        project.components.push(pcbcraft_doc::Component {
            id: comp_id,
            reference: "R1".into(),
            value: "10k".into(),
            ..Default::default()
        });
        project.footprints.push(Footprint {
            id: fp_id,
            component: comp_id,
            position: Point::new(5.0, 3.0),
            rotation: 90.0,
            locked: Some(true),
            side: Some(Layer::Back),
            pads: vec![],
            preserved: Default::default(),
        });
        let cells = layout_cells(&project, comp_id);
        assert!(cells.is_some(), "layout_cells found the footprint");
        let cells = cells.unwrap_or(LayoutCells {
            side: "Top".into(),
            rotation: "0.0".into(),
            locked: false,
            x: "0.00".into(),
            y: "0.00".into(),
        });
        assert_eq!(cells.side, "Bottom", "back-side footprint shows Bottom");
        assert_eq!(cells.rotation, "90.0");
        assert_eq!(cells.locked, true);
        assert_eq!(cells.x, "5.00");
        assert_eq!(cells.y, "3.00");
    }
}
