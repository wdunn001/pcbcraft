//! Snap markers and auto-align preview for the library 3D model editor (#709).
//!
//! The full viewport lives on the library workspace branch (#702); this module holds
//! shared snap painting and preview state so the model page can wire it in without
//! duplicating footprint geometry.

use super::Camera;
use super::library::{library_geometry_paths, library_paths_extent};
use egui::{Color32, Rect, Stroke};
use pcbcraft_geom::Point;
use pcbcraft_library::{FootprintDefinition, SNAP_RADIUS_MM, SnapTargetKind, footprint_snap_targets, snap_point};
use pcbcraft_io::AutoAlignResult;

/// User-visible auto-align preview before Apply.
#[derive(Clone, Debug, Default)]
pub struct ModelAlignPreview {
    pub active: bool,
    pub result: Option<AutoAlignResult>,
}

impl ModelAlignPreview {
    pub fn clear(&mut self) {
        self.active = false;
        self.result = None;
    }
}

/// Paint snap targets and an optional active snap marker on a 2D footprint overlay.
pub fn paint_snap_overlay(
    painter: &egui::Painter,
    rect: Rect,
    camera: Camera,
    def: &FootprintDefinition,
    cursor_world: Option<Point>,
    snap_disabled: bool,
) -> Option<Point> {
    let targets = footprint_snap_targets(def);
    let radius_px = (SNAP_RADIUS_MM * camera.zoom).max(4.0);
    for t in &targets {
        let screen = camera.screen(t.position, rect);
        let color = match t.kind {
            SnapTargetKind::PadCentre => Color32::from_rgb(90, 200, 140),
            SnapTargetKind::Origin => Color32::from_rgb(240, 200, 80),
            SnapTargetKind::CourtyardCorner | SnapTargetKind::CourtyardMidpoint => {
                Color32::from_rgb(120, 160, 220)
            }
            SnapTargetKind::BodyCorner => Color32::from_rgb(180, 140, 220),
        };
        painter.circle_stroke(screen, 3.0, Stroke::new(1.0, color));
    }
    if snap_disabled {
        return cursor_world;
    }
    let cursor = cursor_world?;
    let snapped = snap_point(cursor, &targets, SNAP_RADIUS_MM).map(|(p, _)| p);
    if let Some(at) = snapped {
        let screen = camera.screen(at, rect);
        painter.circle_filled(screen, radius_px * 0.35, Color32::from_rgb(255, 120, 80));
        painter.circle_stroke(screen, radius_px * 0.5, Stroke::new(2.0, Color32::WHITE));
    }
    snapped.or(Some(cursor))
}

/// Fit the camera to footprint artwork for the overlay card.
pub fn overlay_camera(def: &FootprintDefinition, rect: Rect) -> Camera {
    let paths = library_geometry_paths(None, Some(def), 1);
    let (c, extent) = library_paths_extent(&paths);
    Camera {
        center: c,
        zoom: ((rect.width() - 32.0) / extent.x as f32)
            .min((rect.height() - 32.0) / extent.y as f32)
            .clamp(0.01, 60.0),
        mirror: false,
        turns: 0,
    }
}
