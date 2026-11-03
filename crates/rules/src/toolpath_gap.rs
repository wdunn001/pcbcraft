//! Checks that toolpaths are not closer than the tool diameter plus clearance.
//!
//! Context: issue 621. When planning multiple toolpaths, paths that are too
//! close can cause the tool to crash into adjacent paths. This check measures
//! the centre-line distance between all pairs of segments from different paths
//! and reports when the gap is too small.
//!
//! # Rules
//! - One finding per pair of segments that violates the minimum gap.
//! - Segments from the same path_id are ignored (they are part of the same toolpath).
//! - The minimum gap is tool_diameter_mm + clearance_mm.
//! - Returns at most 50 findings to avoid overwhelming the report.
//!
//! # Dependencies
//! - crates/rules/src/lib.rs (`Finding`)
//! - crates/geom/src/lib.rs (`point_segment`, `segment_distance`)
//!
//! # Do not
//! - Call this from `drc()`; it takes segments as a parameter, not a Project reference.
//! - Report segments from the same path_id; they are not a gap issue.

use super::Finding;
use pcbcraft_geom::segment_distance;

/// Reference to one segment of a toolpath.
///
/// # Rules
/// - `from` and `to` are centre-line coordinates in millimetres.
/// - `path_id` identifies which toolpath this segment belongs to.
///
/// # Dependencies
/// - crates/rules/src/lib.rs (exported there)
///
/// # Do not
/// - Use this to store geometry; it is a lightweight reference.
#[derive(Clone, Debug)]
pub struct ToolpathSegmentRef {
    /// Start of the segment, as x and y in millimetres.
    pub from: [f64; 2],
    /// End of the segment, as x and y in millimetres.
    pub to: [f64; 2],
    /// The toolpath this segment belongs to; segments with the same id are never compared.
    pub path_id: u32,
}

/// Find toolpath gaps that are too small for the tool and clearance.
///
/// # Rules
/// - Severity is "warning"; code is "toolpath_gap".
/// - Returns empty when no segments are too close.
/// - Returns at most 50 findings.
/// - If more than 4000 segments are passed, returns one finding indicating the cap.
///
/// # Dependencies
/// - crates/geom/src/lib.rs (`segment_distance`)
///
/// # Do not
/// - Call this from `drc()`; it is a standalone check for the Manufacture workspace.
pub fn toolpath_gap_findings(
    segments: &[ToolpathSegmentRef],
    tool_diameter_mm: f64,
    clearance_mm: f64,
) -> Vec<Finding> {
    if segments.len() > 4000 {
        return vec![Finding {
            code: "toolpath_gap",
            severity: "warning".to_owned(),
            message: format!(
                "Too many segments to check toolpath gaps ({}); pass at most 4000",
                segments.len()
            ),
            objects: vec![],
            hint: Some("Split the toolpath into smaller batches.".into()),
        }];
    }

    let minimum = tool_diameter_mm + clearance_mm;
    let mut result = vec![];

    for (i, a) in segments.iter().enumerate() {
        for b in segments.iter().skip(i + 1) {
            if a.path_id == b.path_id {
                continue;
            }
            let dist = segment_distance(
                pcbcraft_geom::Point::new(a.from[0], a.from[1]),
                pcbcraft_geom::Point::new(a.to[0], a.to[1]),
                pcbcraft_geom::Point::new(b.from[0], b.from[1]),
                pcbcraft_geom::Point::new(b.to[0], b.to[1]),
            );
            if dist < minimum - 1e-6 {
                result.push(Finding {
                    code: "toolpath_gap",
                    severity: "warning".to_owned(),
                    message: format!(
                        "Toolpath gap is {:.3} mm; need at least {:.3} mm (tool {:.3} mm + clearance {:.3} mm)",
                        dist, minimum, tool_diameter_mm, clearance_mm
                    ),
                    objects: vec![],
                    hint: Some("Move the toolpaths apart or reduce the clearance.".into()),
                });
                if result.len() >= 50 {
                    return result;
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(from: [f64; 2], to: [f64; 2], id: u32) -> ToolpathSegmentRef {
        ToolpathSegmentRef {
            from,
            to,
            path_id: id,
        }
    }

    #[test]
    fn a_toolpath_closer_than_the_tool_is_a_finding() {
        let segments = vec![
            seg([0.0, 0.0], [10.0, 0.0], 1),
            seg([0.0, 0.4], [10.0, 0.4], 2),
        ];
        // The least gap is 0.5 mm tool + 0.1 mm clearance = 0.6 mm; these sit 0.4 mm apart.
        let findings = toolpath_gap_findings(&segments, 0.5, 0.1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "toolpath_gap");
        assert!(findings[0].message.contains("0.400 mm"));
    }

    #[test]
    fn parallel_paths_wider_than_the_tool_are_clean() {
        let segments = vec![
            seg([0.0, 0.0], [10.0, 0.0], 1),
            seg([0.0, 2.0], [10.0, 2.0], 2),
        ];
        let findings = toolpath_gap_findings(&segments, 0.5, 0.1);
        assert!(findings.is_empty());
    }

    #[test]
    fn touching_paths_of_the_same_path_id_are_ignored() {
        let segments = vec![
            seg([0.0, 0.0], [5.0, 0.0], 1),
            seg([5.0, 0.0], [10.0, 0.0], 1),
        ];
        let findings = toolpath_gap_findings(&segments, 0.5, 0.1);
        assert!(findings.is_empty());
    }

    #[test]
    fn crossing_paths_are_a_finding() {
        let segments = vec![
            seg([0.0, 0.0], [10.0, 10.0], 1),
            seg([0.0, 10.0], [10.0, 0.0], 2),
        ];
        let findings = toolpath_gap_findings(&segments, 0.5, 0.1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "toolpath_gap");
    }
}
