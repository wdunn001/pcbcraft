//! Layout cells for the Parts grid: where each footprint sits on the board.
//!
//! Context: issue #704, part of the Parts grid epic (#44). When the Layout columns
//! checkbox is on, the Parts grid shows five read-only columns after the Part number
//! column: Side, Rotation, Locked, X and Y. These come from the placed footprint, not
//! the component. Words come from `crates/text/strings/en.txt`.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (Project, Footprint, Layer)
//! - crates/ui-egui/src/parts_table.rs (calls layout_cells for each row)
//! - crates/text/strings/en.txt (parts_table.side_*)
//!
//! # Do not
//! - Write footprints from here; the grid only reads them.
use pcbcraft_doc::{Id, Layer, Project};
use pcbcraft_text::t;

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
/// - Side reads Top or Bottom (from the strings file) from Footprint::board_side.
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
        t!("parts_table.side_top")
    } else {
        t!("parts_table.side_bottom")
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
    use pcbcraft_doc::Footprint;
    use pcbcraft_geom::Point;

    fn footprint(component: Id, side: Option<Layer>, locked: Option<bool>) -> Footprint {
        Footprint {
            id: component + 100,
            component,
            position: Point::new(5.0, 3.0),
            rotation: 90.0,
            locked,
            side,
            pads: vec![],
            preserved: Default::default(),
        }
    }

    #[test]
    fn a_back_side_footprint_reports_bottom() -> Result<(), String> {
        let mut project = Project::default();
        project
            .footprints
            .push(footprint(7, Some(Layer::Back), Some(true)));
        let cells = layout_cells(&project, 7).ok_or("layout_cells found no footprint")?;
        assert_eq!(
            cells.side,
            t!("parts_table.side_bottom"),
            "back-side footprint shows Bottom"
        );
        assert_eq!(cells.rotation, "90.0");
        assert!(cells.locked, "a locked footprint reads as locked");
        assert_eq!(cells.x, "5.00");
        assert_eq!(cells.y, "3.00");
        Ok(())
    }

    #[test]
    fn a_part_without_a_footprint_has_no_layout_cells() {
        let mut project = Project::default();
        project.footprints.push(footprint(7, None, None));
        assert!(layout_cells(&project, 8).is_none());
    }
}
