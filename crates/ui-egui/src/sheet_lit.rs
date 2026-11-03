//! What a picked schematic layer row lights on the drawing: the parts on it
//! and the nets that run through them.
//!
//! Context: on the Schematic tab the tree's layer rows are the fixed rows of a
//! page (Symbols, Power symbols). Picking one is recorded as `TreeView::selected_key`
//! (`sec:sheet:<page>/layer:<row>`); this module reads that key, works out the
//! parts and nets it stands for, and paints them in the accent colour over the
//! normal drawing. It never switches tab and never edits the project.
//!
//! # Dependencies
//! - crates/ui-egui/src/sheet_rows.rs (`sheet_row_ids`, `select_sheet_row`)
//! - crates/ui-egui/src/canvas.rs (`symbol`, called from `paint_schematic`)
//! - crates/doc/src/lib.rs (`Project::wire_path`, `Wire`, `Label`)
//!
//! # Do not
//! - Keep a copy of the lit set between frames; derive it from the picked row.

use super::*;
use std::collections::HashSet;

/// The parts and nets a picked schematic layer row lights.
///
/// # Rules
/// - `parts` holds component ids, `nets` holds net ids.
/// - Both are empty when no layer row of a page is picked.
///
/// # Dependencies
/// - crates/ui-egui/src/sheet_lit.rs (`App::sheet_lit` builds it)
///
/// # Do not
/// - Store it; it is cheap to rebuild from the tree selection each frame.
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct SheetLit {
    /// Components on the picked layer.
    pub(crate) parts: HashSet<Id>,
    /// Nets that run through those components.
    pub(crate) nets: HashSet<Id>,
}

impl App {
    /// The parts and nets lit by the picked layer row of a schematic page.
    ///
    /// # Rules
    /// - Symbols lights the page's parts and every net wired to them.
    /// - Power symbols lights the power nets and every part wired to them.
    /// - Other rows light nothing.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/sheet_rows.rs (`sheet_row_ids`)
    ///
    /// # Do not
    /// - Call it on the layout tab; layout dims layers instead.
    pub(crate) fn sheet_lit(&self) -> Option<SheetLit> {
        let key = self.tree.selected_key.as_deref()?;
        let (page, row) = key.strip_prefix("sec:sheet:")?.split_once("/layer:")?;
        let page: Id = page.parse().ok()?;
        let p = &self.project;
        let mut lit = SheetLit::default();
        match row {
            "symbols" => {
                lit.parts = self.sheet_row_ids(page, "symbols").into_iter().collect();
                for w in &p.wires {
                    if lit.parts.contains(&w.from.component) || lit.parts.contains(&w.to.component)
                    {
                        lit.nets.insert(w.net);
                    }
                }
            }
            "power" => {
                for id in self.sheet_row_ids(page, "power") {
                    if let Some(l) = p.labels.iter().find(|l| l.id == id) {
                        lit.nets.insert(l.net);
                    }
                }
                for w in p.wires.iter().filter(|w| lit.nets.contains(&w.net)) {
                    lit.parts.insert(w.from.component);
                    lit.parts.insert(w.to.component);
                }
            }
            _ => return None,
        }
        Some(lit)
    }

    /// Paint the lit parts and nets over the schematic in the accent colour.
    ///
    /// # Rules
    /// - Draws nothing when no layer row is picked.
    /// - Wires of lit nets are redrawn on top; lit parts are redrawn selected.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/canvas.rs (`symbol`)
    ///
    /// # Do not
    /// - Paint on the layout tab.
    pub(crate) fn paint_sheet_lit(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        page: Id,
    ) {
        let Some(lit) = self.sheet_lit() else {
            return;
        };
        let p = &self.project;
        let stroke = Stroke::new(3.5, self.accent());
        for w in p
            .wires
            .iter()
            .filter(|w| lit.nets.contains(&w.net) && p.on_schematic_page(page, w.id))
        {
            if let Some(path) = p.wire_path(w) {
                painter.add(egui::Shape::line(
                    path.iter().map(|pt| camera.screen(*pt, rect)).collect(),
                    stroke,
                ));
            }
        }
        for c in p
            .components
            .iter()
            .filter(|c| lit.parts.contains(&c.id))
            .flat_map(|c| c.schematic_views())
            .filter(|c| c.sheet == page)
        {
            self.symbol(painter, rect, camera, &c, true, false);
        }
    }
}

#[cfg(test)]
impl App {
    /// How many parts and nets are lit now, for the stories.
    pub(crate) fn sheet_lit_for_test(&self) -> Option<(usize, usize)> {
        self.sheet_lit().map(|l| (l.parts.len(), l.nets.len()))
    }
}
