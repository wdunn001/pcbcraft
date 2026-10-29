//! Manufacturing panel fabrication checks (enabled panel settings only).
use crate::Finding;
use pcbcraft_doc::{
    panel_layout, panel_score_lines, PanelSeparation,
    PANEL_ASSEMBLER_MAX_EDGE_MM, PANEL_FIDUCIAL_MARK_COUNT, PANEL_SCORE_COMPONENT_CLEARANCE_MM,
    PANEL_SCORE_COPPER_CLEARANCE_MM, PANEL_TAB_MIN_WIDTH_MM, Project,
};
use pcbcraft_geom::{Point, point_segment, segment_distance};

use super::copper;

fn panel_anchor(project: &Project) -> pcbcraft_doc::Id {
    project
        .board
        .footprints
        .first()
        .map(|f| f.id)
        .or_else(|| project.components.first().map(|c| c.id))
        .unwrap_or(1)
}

fn copper_to_panel_distance(
    start: Point,
    end: Point,
    radius: f64,
    line: pcbcraft_doc::PanelSegment,
) -> f64 {
    let raw = segment_distance(start, end, line.start, line.end);
    (raw - radius).max(0.0)
}

fn footprint_extent(footprint: &pcbcraft_doc::Footprint) -> f64 {
    footprint
        .pads
        .iter()
        .map(|p| p.offset.distance(Point::default()) + p.bounding_radius())
        .fold(0.0, f64::max)
}

pub fn panel_checks(project: &Project) -> Vec<Finding> {
    let Some(settings) = project.panel.as_ref() else {
        return vec![];
    };
    if !settings.enabled {
        return vec![];
    }
    let Ok(layout) = panel_layout(project, settings) else {
        return vec![];
    };
    let mut result = Vec::new();
    let anchor = panel_anchor(project);
    let score_lines = panel_score_lines(&layout);
    if layout.panel_width > PANEL_ASSEMBLER_MAX_EDGE_MM
        || layout.panel_height > PANEL_ASSEMBLER_MAX_EDGE_MM
    {
        result.push(Finding {
            code: "panel_too_large_for_assembler",
            severity: project.rule_severity("panel_too_large_for_assembler", "error"),
            message: format!(
                "Panel is {:.1} by {:.1} mm; assembly panels should stay within {:.0} mm per edge until a fab profile raises the limit",
                layout.panel_width,
                layout.panel_height,
                PANEL_ASSEMBLER_MAX_EDGE_MM
            ),
            objects: vec![anchor],
        });
    }
    if project.board.fiducials.len() < PANEL_FIDUCIAL_MARK_COUNT {
        result.push(Finding {
            code: "panel_missing_fiducials",
            severity: project.rule_severity("panel_missing_fiducials", "error"),
            message: format!(
                "Manufacturing panel needs {} fiducial marks in the rails; {} on the board",
                PANEL_FIDUCIAL_MARK_COUNT,
                project.board.fiducials.len()
            ),
            objects: vec![anchor],
        });
    }
    if settings.separation == PanelSeparation::TabRouting {
        let channel = settings.tab_channel_width;
        if settings.columns > 1 {
            let tab_w = (settings.pitch_x - channel).max(0.0);
            if tab_w + 1e-6 < PANEL_TAB_MIN_WIDTH_MM {
                result.push(Finding {
                    code: "panel_tab_too_narrow",
                    severity: project.rule_severity("panel_tab_too_narrow", "error"),
                    message: format!(
                        "Horizontal breakaway tabs are {:.2} mm wide; at least {:.1} mm is needed for a {:.1} mm routed channel",
                        tab_w,
                        PANEL_TAB_MIN_WIDTH_MM,
                        channel
                    ),
                    objects: vec![anchor],
                });
            }
        }
        if settings.rows > 1 {
            let tab_w = (settings.pitch_y - channel).max(0.0);
            if tab_w + 1e-6 < PANEL_TAB_MIN_WIDTH_MM {
                result.push(Finding {
                    code: "panel_tab_too_narrow",
                    severity: project.rule_severity("panel_tab_too_narrow", "error"),
                    message: format!(
                        "Vertical breakaway tabs are {:.2} mm wide; at least {:.1} mm is needed for a {:.1} mm routed channel",
                        tab_w,
                        PANEL_TAB_MIN_WIDTH_MM,
                        channel
                    ),
                    objects: vec![anchor],
                });
            }
        }
    }
    if !score_lines.is_empty() {
        let items = copper(project);
        for copy in &layout.copies {
            let origin = Point::new(copy.x, copy.y);
            for item in &items {
                if item.layer != pcbcraft_doc::Layer::Front {
                    continue;
                }
                let start = origin.offset(item.start);
                let end = origin.offset(item.end);
                for line in &score_lines {
                    let dist = copper_to_panel_distance(start, end, item.radius, *line);
                    if dist + 1e-6 < PANEL_SCORE_COPPER_CLEARANCE_MM {
                        result.push(Finding {
                            code: "panel_copper_near_score",
                            severity: project.rule_severity("panel_copper_near_score", "error"),
                            message: format!(
                                "Copper on copy row {} column {} is {:.2} mm from a V-score line; keep at least {:.2} mm clear",
                                copy.row + 1,
                                copy.column + 1,
                                dist,
                                PANEL_SCORE_COPPER_CLEARANCE_MM
                            ),
                            objects: vec![item.id],
                        });
                        break;
                    }
                }
            }
        }
        for copy in &layout.copies {
            let origin = Point::new(copy.x, copy.y);
            for footprint in &project.board.footprints {
                let centre = origin.offset(footprint.position);
                let reach = footprint_extent(footprint);
                for line in &score_lines {
                    let dist = point_segment(centre, line.start, line.end) - reach;
                    if dist + 1e-6 < PANEL_SCORE_COMPONENT_CLEARANCE_MM {
                        result.push(Finding {
                            code: "panel_component_near_score",
                            severity: project.rule_severity("panel_component_near_score", "error"),
                            message: format!(
                                "A footprint on copy row {} column {} sits {:.2} mm from a V-score line; keep components at least {:.1} mm away",
                                copy.row + 1,
                                copy.column + 1,
                                dist.max(0.0),
                                PANEL_SCORE_COMPONENT_CLEARANCE_MM
                            ),
                            objects: vec![footprint.id],
                        });
                        break;
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drc;
    use pcbcraft_doc::{
        Fiducial, Footprint, Net, Pad, PanelSeparation, PanelSettings, Track, component_template,
    };

    fn panel_project(settings: PanelSettings) -> Project {
        let mut p = Project::default();
        p.board.width = 30.0;
        p.board.height = 30.0;
        p.panel = Some(settings);
        p
    }

    fn codes(project: &Project) -> Vec<&'static str> {
        drc(project).into_iter().map(|f| f.code).collect()
    }

    #[test]
    fn panel_checks_skip_when_panel_disabled() -> Result<(), String> {
        let mut p = panel_project(PanelSettings {
            enabled: false,
            ..PanelSettings::default()
        });
        assert!(!codes(&p).iter().any(|c| c.starts_with("panel_")));
        p.panel = None;
        assert!(!codes(&p).iter().any(|c| c.starts_with("panel_")));
        Ok(())
    }

    #[test]
    fn panel_too_large_for_assembler_finds_oversize_grid() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 4,
            rows: 4,
            pitch_x: 2.0,
            pitch_y: 2.0,
            rail: 5.0,
            separation: PanelSeparation::TabRouting,
            tab_channel_width: 1.8,
        };
        let p = panel_project(settings);
        let hits: Vec<_> = drc(&p)
            .into_iter()
            .filter(|f| f.code == "panel_too_large_for_assembler")
            .collect();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].message.contains("250"));
        Ok(())
    }

    #[test]
    fn panel_too_large_for_assembler_passes_compact_panel() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 2,
            rows: 2,
            pitch_x: 2.0,
            pitch_y: 2.0,
            rail: 5.0,
            separation: PanelSeparation::TabRouting,
            tab_channel_width: 1.8,
        };
        let mut p = panel_project(settings);
        for i in 0..PANEL_FIDUCIAL_MARK_COUNT {
            p.board.fiducials.push(Fiducial {
                id: 900 + i as u64,
                position: Point::new(1.0 + i as f64, 1.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            });
        }
        assert!(
            !codes(&p).contains(&"panel_too_large_for_assembler"),
            "compact 2x2 panel should fit assembler limit"
        );
        Ok(())
    }

    #[test]
    fn panel_missing_fiducials_when_rails_have_no_marks() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 1,
            rows: 1,
            pitch_x: 0.0,
            pitch_y: 0.0,
            rail: 5.0,
            separation: PanelSeparation::TabRouting,
            tab_channel_width: 1.8,
        };
        let p = panel_project(settings);
        assert!(drc(&p).iter().any(|f| f.code == "panel_missing_fiducials"));
        Ok(())
    }

    #[test]
    fn panel_missing_fiducials_passes_with_three_marks() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 1,
            rows: 1,
            pitch_x: 0.0,
            pitch_y: 0.0,
            rail: 5.0,
            separation: PanelSeparation::TabRouting,
            tab_channel_width: 1.8,
        };
        let mut p = panel_project(settings);
        for i in 0..PANEL_FIDUCIAL_MARK_COUNT {
            p.board.fiducials.push(Fiducial {
                id: 910 + i as u64,
                position: Point::new(2.0 + i as f64, 2.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            });
        }
        assert!(!drc(&p).iter().any(|f| f.code == "panel_missing_fiducials"));
        Ok(())
    }

    #[test]
    fn panel_tab_too_narrow_flags_thin_tabs() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 2,
            rows: 1,
            pitch_x: 3.0,
            pitch_y: 0.0,
            rail: 5.0,
            separation: PanelSeparation::TabRouting,
            tab_channel_width: 1.8,
        };
        let mut p = panel_project(settings);
        for i in 0..PANEL_FIDUCIAL_MARK_COUNT {
            p.board.fiducials.push(Fiducial {
                id: 920 + i as u64,
                position: Point::new(1.0 + i as f64, 1.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            });
        }
        assert!(drc(&p).iter().any(|f| f.code == "panel_tab_too_narrow"));
        Ok(())
    }

    #[test]
    fn panel_tab_too_narrow_passes_wide_tabs() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 2,
            rows: 1,
            pitch_x: 4.0,
            pitch_y: 0.0,
            rail: 5.0,
            separation: PanelSeparation::TabRouting,
            tab_channel_width: 1.8,
        };
        let mut p = panel_project(settings);
        for i in 0..PANEL_FIDUCIAL_MARK_COUNT {
            p.board.fiducials.push(Fiducial {
                id: 930 + i as u64,
                position: Point::new(1.0 + i as f64, 1.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            });
        }
        assert!(!drc(&p).iter().any(|f| f.code == "panel_tab_too_narrow"));
        Ok(())
    }

    #[test]
    fn panel_copper_near_score_flags_edge_tracks() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 2,
            rows: 1,
            pitch_x: 0.0,
            pitch_y: 0.0,
            rail: 5.0,
            separation: PanelSeparation::VScore,
            tab_channel_width: 1.8,
        };
        let mut p = panel_project(settings);
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        p.board.tracks.push(Track {
            id: 50,
            net: 1,
            layer: pcbcraft_doc::Layer::Front,
            start: Point::new(28.0, 15.0),
            end: Point::new(29.9, 15.0),
            width: 0.2,
            arc: None,
        });
        for i in 0..PANEL_FIDUCIAL_MARK_COUNT {
            p.board.fiducials.push(Fiducial {
                id: 940 + i as u64,
                position: Point::new(1.0 + i as f64, 1.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            });
        }
        assert!(drc(&p).iter().any(|f| f.code == "panel_copper_near_score"));
        Ok(())
    }

    #[test]
    fn panel_copper_near_score_passes_when_copper_is_clear() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 2,
            rows: 1,
            pitch_x: 0.0,
            pitch_y: 0.0,
            rail: 5.0,
            separation: PanelSeparation::VScore,
            tab_channel_width: 1.8,
        };
        let mut p = panel_project(settings);
        p.nets.push(Net {
            id: 1,
            name: "sig".into(),
            ..Default::default()
        });
        p.board.tracks.push(Track {
            id: 51,
            net: 1,
            layer: pcbcraft_doc::Layer::Front,
            start: Point::new(5.0, 15.0),
            end: Point::new(20.0, 15.0),
            width: 0.2,
            arc: None,
        });
        for i in 0..PANEL_FIDUCIAL_MARK_COUNT {
            p.board.fiducials.push(Fiducial {
                id: 950 + i as u64,
                position: Point::new(1.0 + i as f64, 1.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            });
        }
        assert!(!drc(&p).iter().any(|f| f.code == "panel_copper_near_score"));
        Ok(())
    }

    #[test]
    fn panel_component_near_score_flags_edge_parts() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 2,
            rows: 1,
            pitch_x: 0.0,
            pitch_y: 0.0,
            rail: 5.0,
            separation: PanelSeparation::VScore,
            tab_channel_width: 1.8,
        };
        let mut p = panel_project(settings);
        if let Some(c) = component_template("resistor", 2, "R1", Point::new(29.0, 15.0)) {
            p.components.push(c);
        }
        p.board.footprints.push(Footprint {
            id: 60,
            component: 2,
            position: Point::new(29.0, 15.0),
            rotation: 0.0,
            locked: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.6,
                net: None,
                castellated: None,
                geometry: None,
            }],
        });
        for i in 0..PANEL_FIDUCIAL_MARK_COUNT {
            p.board.fiducials.push(Fiducial {
                id: 960 + i as u64,
                position: Point::new(1.0 + i as f64, 1.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            });
        }
        assert!(drc(&p).iter().any(|f| f.code == "panel_component_near_score"));
        Ok(())
    }

    #[test]
    fn panel_component_near_score_passes_inboard_parts() -> Result<(), String> {
        let settings = PanelSettings {
            enabled: true,
            columns: 2,
            rows: 1,
            pitch_x: 0.0,
            pitch_y: 0.0,
            rail: 5.0,
            separation: PanelSeparation::VScore,
            tab_channel_width: 1.8,
        };
        let mut p = panel_project(settings);
        if let Some(c) = component_template("resistor", 2, "R1", Point::new(10.0, 15.0)) {
            p.components.push(c);
        }
        p.board.footprints.push(Footprint {
            id: 61,
            component: 2,
            position: Point::new(10.0, 15.0),
            rotation: 0.0,
            locked: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.6,
                net: None,
                castellated: None,
                geometry: None,
            }],
        });
        for i in 0..PANEL_FIDUCIAL_MARK_COUNT {
            p.board.fiducials.push(Fiducial {
                id: 970 + i as u64,
                position: Point::new(1.0 + i as f64, 1.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            });
        }
        assert!(!drc(&p).iter().any(|f| f.code == "panel_component_near_score"));
        Ok(())
    }
}
