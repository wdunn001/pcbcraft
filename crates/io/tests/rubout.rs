//! Oracle tests for copper rubout hatch paths (issue 202).
use pcbcraft_doc::{Footprint, Layer, Net, Pad, Project};
use pcbcraft_geom::{
    Point, Ring, polygon_boundary_distance, polygon_difference, polygon_offset, polygon_union,
    segment_distance, OffsetJoin,
};
use pcbcraft_io::{
    CamMirrorAxis, CncProfile, IsolationOptions, RubRegion, Side, Spindle, Tool,
    cam_layer_shapes as layer_shapes, isolation_gcode, isolation_paths, rubout_paths,
};

fn rubout_tool() -> Tool {
    Tool {
        diameter_mm: 1.0,
        feed_mm_min: 400.0,
        plunge_mm_min: 80.0,
        depth_mm: 0.08,
        passes: 1,
        spindle: Spindle::Rpm(18000),
    }
}

fn isolation_tool() -> Tool {
    Tool {
        diameter_mm: 0.2,
        feed_mm_min: 300.0,
        plunge_mm_min: 60.0,
        depth_mm: 0.08,
        passes: 1,
        spindle: Spindle::Rpm(18000),
    }
}

fn single_pad_20mm_board() -> Project {
    let mut p = Project::default();
    p.board.width = 20.0;
    p.board.height = 20.0;
    p.board.thickness = 1.6;
    p.nets.push(Net {
        id: 1,
        name: "A".into(),
        ..Net::default()
    });
    p.board.footprints.push(Footprint {
        id: 1,
        component: 0,
        position: Point::new(10.0, 10.0),
        rotation: 0.0,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 2.0,
            net: Some(1),
            castellated: None,
            geometry: None,
        }],
        locked: None,
        side: None,
    });
    p
}

fn pad_ring(p: &Project) -> Ring {
    layer_shapes(p, Layer::Front)
        .ok()
        .and_then(|s| s.into_iter().next().map(|shape| shape.outline))
        .unwrap_or_default()
}

#[test]
fn rubout_hatch_stays_clear_of_pad_and_covers_the_region() -> Result<(), String> {
    let p = single_pad_20mm_board();
    let region = RubRegion {
        rect: Some((Point::new(0.0, 0.0), Point::new(20.0, 20.0))),
        tool: rubout_tool(),
        stepover_pct: 50,
        angle_deg: 0.0,
    };
    let keep = isolation_paths(
        &p,
        &IsolationOptions {
            side: Side::Top,
            tool: isolation_tool(),
            overlap_pct: 50,
            rubout: None,
            mirror_axis: CamMirrorAxis::Y,
            alignment_holes: false,
        },
    )?;
    let paths = rubout_paths(
        &p,
        Side::Top,
        &region,
        &keep,
        isolation_tool().diameter_mm,
    )?;
    assert!(!paths.is_empty());
    let pad = pad_ring(&p);
    let half = region.tool.diameter_mm / 2.0;
    let stepover = region.tool.diameter_mm * 0.5;
    for path in &paths {
        for window in path.windows(2) {
            let d = polygon_boundary_distance(window[0], &pad)
                .min(polygon_boundary_distance(window[1], &pad));
            assert!(
                d + 0.02 >= half,
                "rubout passes {d:.3} mm from pad copper (limit {half:.3})"
            );
        }
    }
    let copper = vec![pad];
    let grown = polygon_offset(&polygon_union(&copper), half, OffsetJoin::Round);
    let rect = vec![
        Point::new(0.0, 0.0),
        Point::new(20.0, 0.0),
        Point::new(20.0, 20.0),
        Point::new(0.0, 20.0),
        Point::new(0.0, 0.0),
    ];
    let free = polygon_difference(&[rect], &grown);
    assert!(!free.is_empty());
    let samples = [
        Point::new(1.0, 1.0),
        Point::new(19.0, 1.0),
        Point::new(1.0, 19.0),
        Point::new(19.0, 19.0),
        Point::new(10.0, 1.0),
    ];
    for sample in samples {
        if polygon_boundary_distance(sample, &pad) < half + 0.05 {
            continue;
        }
        let mut best = f64::INFINITY;
        for path in &paths {
            for window in path.windows(2) {
                best = best.min(segment_distance(sample, sample, window[0], window[1]));
            }
        }
        assert!(
            best <= stepover + 0.15,
            "sample ({:.1},{:.1}) is {best:.3} mm from rubout (limit {stepover:.3})",
            sample.x,
            sample.y
        );
    }
    Ok(())
}

#[test]
fn rubout_output_is_stable_and_skips_isolation_rings() -> Result<(), String> {
    let p = single_pad_20mm_board();
    let opts = IsolationOptions {
        side: Side::Top,
        tool: isolation_tool(),
        overlap_pct: 50,
        rubout: Some(RubRegion {
            rect: Some((Point::new(0.0, 0.0), Point::new(20.0, 20.0))),
            tool: rubout_tool(),
            stepover_pct: 50,
            angle_deg: 15.0,
        }),
        mirror_axis: CamMirrorAxis::Y,
        alignment_holes: false,
    };
    let profile = CncProfile::default();
    let a = isolation_gcode(&p, &opts, &profile)?;
    let b = isolation_gcode(&p, &opts, &profile)?;
    assert_eq!(a.files.len(), 2);
    assert_eq!(a.files, b.files);
    let keep = isolation_paths(&p, &opts)?;
    let rub_only = rubout_paths(
        &p,
        Side::Top,
        opts.rubout.as_ref().ok_or("rubout")?,
        &keep,
        opts.tool.diameter_mm,
    )?;
    for path in &rub_only {
        for window in path.windows(2) {
            for ring in &keep {
                for (c, d) in pcbcraft_geom::polygon_edges(ring) {
                    assert!(
                        segment_distance(window[0], window[1], c, d) >= 1e-6,
                        "rubout segment crosses isolation"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn rubout_tool_smaller_than_isolation_is_refused() {
    let p = single_pad_20mm_board();
    let region = RubRegion {
        rect: None,
        tool: isolation_tool(),
        stepover_pct: 50,
        angle_deg: 0.0,
    };
    let err = rubout_paths(
        &p,
        Side::Top,
        &region,
        &[],
        rubout_tool().diameter_mm,
    )
    .err()
    .unwrap_or_default();
    assert!(err.contains("smaller than the isolation tool"));
}
