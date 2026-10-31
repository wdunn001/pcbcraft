//! 3D software renderer benchmark for large boards (issue 721).

use pcbcraft_io::{bench_project_600_parts, board_mesh, MeshLayers};
use std::time::Instant;

const BASELINE_MS: u64 = 9000;
const REGRESSION_RATIO: f64 = 1.30;

#[test]
fn six_hundred_parts_render_stays_within_the_baseline() -> Result<(), String> {
    let project = bench_project_600_parts();
    let mesh = board_mesh(&project, MeshLayers::default())?;
    let bodies = mesh
        .parts
        .iter()
        .filter(|p| p.kind == pcbcraft_io::PartKind::Body)
        .count();
    if bodies < 600 {
        return Err(format!("expected 600 bodies, got {bodies}"));
    }
    let basis = bench_camera();
    let start = Instant::now();
    let _ = pcbcraft_ui_egui::board3d::bench_render(&mesh, &basis, 800, 600);
    let elapsed = start.elapsed().as_millis() as u64;
    let limit = ((BASELINE_MS as f64) * REGRESSION_RATIO).ceil() as u64;
    if elapsed > limit {
        return Err(format!(
            "3D render took {elapsed} ms, above the {limit} ms gate (baseline {BASELINE_MS} ms + 30%)"
        ));
    }
    Ok(())
}

fn bench_camera() -> pcbcraft_ui_egui::board3d::BenchBasis {
    pcbcraft_ui_egui::board3d::BenchBasis::isometric([100.0, 100.0, 0.0], 280.0)
}
