//! The translucent preview of the item an Add tool will place.
//!
//! One pure function ([`placement_ghost`]) decides what the armed tool shows
//! and where; one painter ([`App::paint_placement_ghost`]) draws it. The canvas
//! calls both once per frame, for the schematic and the layout alike.
//!
//! # Rules
//! - A part ghost is painted by the same functions the Library preview uses
//!   (`library_geometry_paths` and `paint_library_paths`).
//! - Every other ghost is painted by the code that paints the real item, in a
//!   translucent colour.
//! - Nothing here writes the project, the history or the message line.
//! - Nothing here asks for a repaint: the pointer moving already does.
//!
//! # Dependencies
//! - crates/ui-egui/src/library.rs (the preview painters)
//! - crates/ui-egui/src/canvas/sheet.rs (power symbols)
//! - crates/ui-egui/src/pad_tool.rs, via_tool.rs, pour_panel.rs (board ghosts)
//!
//! # Do not
//! - Copy a painter here; call the one the real item uses.
use super::*;
use crate::library::{library_geometry_paths, paint_library_paths};

/// Which item the armed Add tool is about to place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GhostKind {
    /// A library, custom or built-in schematic part.
    Part,
    /// A local net label.
    Label,
    /// A power symbol.
    Power,
    /// A bus (the line starts at the first click).
    Bus,
    /// A bus entry.
    BusEntry,
    /// A hierarchy port.
    Port,
    /// A no-connect cross.
    NoConnect,
    /// A note of text on the sheet.
    Text,
    /// A line, rectangle or circle drawn on the sheet.
    Shape,
    /// A pad.
    Pad,
    /// A via.
    Via,
    /// A mounting hole.
    Hole,
    /// An assembly mark.
    Fiducial,
    /// A keepout.
    Keepout,
    /// A board cutout.
    Cutout,
}

/// What the armed Add tool shows under the pointer this frame.
///
/// # Rules
/// - `at` is the grid-snapped spot a click would place at.
/// - `pointer` is the raw spot, for ghosts that snap to pads and tracks themselves.
/// - Placement has no pending turn, so the ghost has no rotation either; a
///   placed item turns afterwards with the rotate key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacementGhost {
    /// The item shown.
    pub kind: GhostKind,
    /// The snapped spot, in world millimetres.
    pub at: Point,
    /// The raw pointer spot, in world millimetres.
    pub pointer: Point,
}

/// The ghost for the armed `tool` on `tab`, or `None` when the tool places
/// nothing on that tab.
///
/// # Rules
/// - Pure: same inputs, same answer, no state read or written.
/// - Select, Move, Wire, Route, Measure and the drawing tools return `None`:
///   they place nothing at one click, or draw their own draft.
pub(crate) fn placement_ghost(
    tool: &Tool,
    tab: Tab,
    pointer: Point,
    snapped: Point,
) -> Option<PlacementGhost> {
    let sheet = tab == Tab::Schematic;
    let board = tab == Tab::Pcb;
    let kind = match tool {
        Tool::Place(_) | Tool::LibraryPlace(..) | Tool::CustomPlace(..) if sheet => GhostKind::Part,
        Tool::SchematicLabel(wire_canvas::SchematicLabelKind::Local) if sheet => GhostKind::Label,
        Tool::SchematicLabel(wire_canvas::SchematicLabelKind::Power) if sheet => GhostKind::Power,
        Tool::Bus if sheet => GhostKind::Bus,
        Tool::BusEntry if sheet => GhostKind::BusEntry,
        Tool::Port if sheet => GhostKind::Port,
        Tool::NoConnect if sheet => GhostKind::NoConnect,
        Tool::SheetText if sheet => GhostKind::Text,
        Tool::SheetShape(_) if sheet => GhostKind::Shape,
        Tool::Pad if sheet || board => GhostKind::Pad,
        Tool::Via if board => GhostKind::Via,
        Tool::Hole if board => GhostKind::Hole,
        Tool::Fiducial if board => GhostKind::Fiducial,
        Tool::Keepout if board => GhostKind::Keepout,
        Tool::Cutout if board => GhostKind::Cutout,
        _ => return None,
    };
    Some(PlacementGhost {
        kind,
        at: snapped,
        pointer,
    })
}

impl App {
    /// Decide this frame's ghost from the pointer and keep the part paths ready.
    ///
    /// # Rules
    /// - `pointer` is `None` when the pointer is off the canvas: the ghost goes.
    /// - The part paths are rebuilt only when the armed part changes.
    pub(crate) fn refresh_placement_ghost(&mut self, pointer: Option<Point>) {
        self.ghost = pointer.and_then(|p| {
            let snapped = self.constrained(snap(p));
            placement_ghost(&self.tool, self.tab, p, snapped)
        });
        let wanted = match &self.tool {
            Tool::LibraryPlace(path, name, _) if self.ghost.is_some() => Some((path, name)),
            Tool::CustomPlace(name, _) if self.ghost.is_some() => Some((name, name)),
            _ => None,
        };
        let Some((path, name)) = wanted else {
            if self.ghost.is_none() {
                self.ghost_paths = None;
            }
            return;
        };
        if self
            .ghost_paths
            .as_ref()
            .is_some_and(|(p, n, _)| p == path && n == name)
        {
            return;
        }
        let (Tool::LibraryPlace(_, _, def) | Tool::CustomPlace(_, def)) = &self.tool else {
            return;
        };
        let paths = library_geometry_paths(Some(def.as_ref()), None, 1);
        self.ghost_paths = Some((path.clone(), name.clone(), paths));
    }

    /// Paint the ghost the last [`App::refresh_placement_ghost`] decided on.
    pub(crate) fn paint_placement_ghost(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
    ) {
        let Some(ghost) = self.ghost else {
            return;
        };
        let at = ghost.at;
        let tint = self.ghost_tint();
        let stroke = Stroke::new(1.5, tint);
        let mut sc = sheet::SheetColours::for_theme(self.colours().dark);
        sc.wire = tint;
        let spot = camera.screen(at, rect);
        match ghost.kind {
            GhostKind::Part => match &self.tool {
                Tool::Place(kind) => {
                    if let Some(c) = pcbcraft_doc::component_template(kind, 1, "preview".into(), at)
                    {
                        self.symbol(painter, rect, camera, &c, false, true);
                    }
                }
                _ => {
                    if let Some((_, _, paths)) = &self.ghost_paths {
                        let moved: Vec<Vec<Point>> = paths
                            .iter()
                            .map(|path| path.iter().map(|p| p.offset(at)).collect())
                            .collect();
                        paint_library_paths(painter, rect, camera, &moved, tint);
                    }
                }
            },
            GhostKind::Label => {
                let typed = self.edit.label.name.trim();
                let name = if typed.is_empty() {
                    t!("placement.ghost.label")
                } else {
                    typed.to_owned()
                };
                painter.circle_filled(spot, 3.0, tint);
                painter.text(
                    spot,
                    Align2::CENTER_BOTTOM,
                    name,
                    role_font(TextRole::NetName, camera.zoom),
                    tint,
                );
            }
            GhostKind::Power => {
                let style = pcbcraft_doc::Label::POWER_STYLES
                    .get(self.edit.label.style)
                    .copied()
                    .unwrap_or("bar");
                sheet::power_symbol(
                    painter,
                    rect,
                    camera,
                    style,
                    at,
                    self.edit.label.power_net.trim(),
                    &sc,
                );
            }
            GhostKind::Port => {
                let typed = self.port_pick_name.trim();
                let name = if typed.is_empty() {
                    t!("placement.ghost.port")
                } else {
                    typed.to_owned()
                };
                painter.text(
                    spot,
                    Align2::CENTER_CENTER,
                    format!("[{name}]"),
                    role_font(TextRole::PortName, camera.zoom),
                    tint,
                );
            }
            GhostKind::NoConnect => {
                for (a, b) in [((-4.0, -4.0), (4.0, 4.0)), ((-4.0, 4.0), (4.0, -4.0))] {
                    painter.line_segment(
                        [spot + Vec2::new(a.0, a.1), spot + Vec2::new(b.0, b.1)],
                        Stroke::new(1.6, tint),
                    );
                }
            }
            GhostKind::BusEntry => {
                let end = camera.screen(at.offset(Point::new(2.54, -2.54)), rect);
                painter.line_segment([spot, end], Stroke::new(2.0, tint));
            }
            GhostKind::Text => {
                let typed = self.sheet_text_input.trim();
                let words = if typed.is_empty() {
                    t!("placement.ghost.text")
                } else {
                    typed.to_owned()
                };
                painter.text(
                    spot,
                    Align2::LEFT_TOP,
                    words,
                    role_font(TextRole::Reference, camera.zoom),
                    tint,
                );
            }
            GhostKind::Bus | GhostKind::Shape => {
                painter.line_segment(
                    [spot - Vec2::new(6.0, 0.0), spot + Vec2::new(6.0, 0.0)],
                    stroke,
                );
                painter.line_segment(
                    [spot - Vec2::new(0.0, 6.0), spot + Vec2::new(0.0, 6.0)],
                    stroke,
                );
            }
            GhostKind::Pad => self.paint_pad_ghost(painter, rect, camera, at),
            GhostKind::Via => self.paint_via_ghost(painter, rect, camera, ghost.pointer),
            GhostKind::Keepout => self.paint_keepout_ghost(painter, rect, camera, at),
            GhostKind::Fiducial => {
                painter.circle_stroke(
                    spot,
                    (self.fiducial_diameter / 2.0) as f32 * camera.zoom,
                    stroke,
                );
            }
            GhostKind::Hole => {
                painter.circle_stroke(
                    spot,
                    (self.hole_diameter / 2.0) as f32 * camera.zoom,
                    stroke,
                );
            }
            GhostKind::Cutout => {
                if pcbcraft_geom::positive(self.cutout_radius) {
                    painter.circle_stroke(spot, self.cutout_radius as f32 * camera.zoom, stroke);
                }
            }
        }
    }
}

impl App {
    /// The translucent colour every ghost is painted in.
    pub(crate) fn ghost_tint(&self) -> Color32 {
        self.accent().gamma_multiply(0.6)
    }
}
