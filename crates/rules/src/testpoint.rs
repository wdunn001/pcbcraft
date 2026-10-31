//! In-circuit probe coverage: which nets have a pad, via or marked test point
//! large enough and spaced for the probe minimums.
use pcbcraft_doc::{Component, Id, Layer, Pad, Project, Via};
use pcbcraft_geom::Point;
use std::collections::{BTreeMap, BTreeSet};

use crate::Finding;

/// Default minimum probe contact diameter when the project names no parameter.
pub const DEFAULT_PROBE_MIN_PAD_MM: f64 = 0.8;
/// Default minimum centre-to-centre spacing between probe sites on different nets.
pub const DEFAULT_PROBE_MIN_SPACING_MM: f64 = 1.27;

/// Probe minimums used by the coverage check and bring-up plan header.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeSettings {
    pub min_pad_mm: f64,
    pub min_spacing_mm: f64,
}

impl ProbeSettings {
    fn valid(min_pad_mm: f64, min_spacing_mm: f64) -> bool {
        min_pad_mm.is_finite()
            && min_spacing_mm.is_finite()
            && (0.05..=10.0).contains(&min_pad_mm)
            && (0.05..=25.0).contains(&min_spacing_mm)
    }
}

/// Reads probe minimums from named project parameters when they are valid.
pub fn probe_settings(project: &Project) -> ProbeSettings {
    let from_param = |name: &str| -> Option<f64> {
        project
            .parameters
            .get(name)
            .map(|p| p.value)
            .filter(|v| v.is_finite())
    };
    let min_pad_mm = from_param("probe_min_pad_mm")
        .filter(|v| ProbeSettings::valid(*v, DEFAULT_PROBE_MIN_SPACING_MM))
        .unwrap_or(DEFAULT_PROBE_MIN_PAD_MM);
    let min_spacing_mm = from_param("probe_min_spacing_mm")
        .filter(|v| ProbeSettings::valid(min_pad_mm, *v))
        .unwrap_or(DEFAULT_PROBE_MIN_SPACING_MM);
    ProbeSettings {
        min_pad_mm,
        min_spacing_mm,
    }
}

/// One place a fixture probe could touch the board.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeSite {
    pub net: Id,
    pub object: Id,
    pub position: Point,
    pub diameter_mm: f64,
    pub layer: Layer,
    pub kind: &'static str,
}

/// Nets that appear on the board (pads or vias) and whether each has coverage.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageReport {
    pub settings: ProbeSettings,
    pub uncovered: Vec<Id>,
    pub covered: BTreeSet<Id>,
}

pub fn component_is_test_point(c: &Component) -> bool {
    if let Some(sym) = &c.library_symbol {
        if sym.definition.description.eq_ignore_ascii_case("test point") {
            return true;
        }
        if sym.definition.reference == "TP" {
            return true;
        }
    }
    if let Some(fp) = &c.library_footprint {
        let name = fp.definition.name.as_str();
        if name.starts_with("Test_") || name.contains("TestPoint") {
            return true;
        }
    }
    false
}

fn pad_size_mm(pad: &Pad) -> f64 {
    if let Some(g) = &pad.geometry {
        g.size.x.max(g.size.y).max(pad.diameter)
    } else {
        pad.diameter
    }
}

fn collect_board_nets(project: &Project) -> BTreeSet<Id> {
    let mut nets = BTreeSet::new();
    for f in &project.board.footprints {
        for pad in &f.pads {
            if let Some(net) = pad.net.filter(|n| *n != 0) {
                nets.insert(net);
            }
        }
    }
    for via in &project.board.vias {
        if via.net != 0 {
            nets.insert(via.net);
        }
    }
    nets
}

/// Every candidate probe site on the layout before size and spacing filters.
pub fn probe_sites(project: &Project) -> Result<Vec<ProbeSite>, String> {
    project.validate()?;
    let refs: BTreeMap<u64, &Component> = project.components.iter().map(|c| (c.id, c)).collect();
    let mut sites = Vec::new();
    for f in &project.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let test_part = component_is_test_point(c);
        for pad in &f.pads {
            let Some(net) = pad.net.filter(|n| *n != 0) else {
                continue;
            };
            let tht = pad
                .geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "thru_hole");
            if !tht && !test_part {
                continue;
            }
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            if !world.x.is_finite() || !world.y.is_finite() {
                return Err("pad position is not a number".into());
            }
            let diameter_mm = pad_size_mm(pad);
            if diameter_mm <= 0.0 || !diameter_mm.is_finite() {
                return Err("pad size is not a positive number".into());
            }
            let kind = if test_part {
                "test pad"
            } else {
                "through hole"
            };
            sites.push(ProbeSite {
                net,
                object: f.id,
                position: world,
                diameter_mm,
                layer: f.pad_layer(pad),
                kind,
            });
        }
    }
    for via in &project.board.vias {
        if via.net == 0 || via.diameter <= 0.0 || !via.diameter.is_finite() {
            continue;
        }
        if !via.position.x.is_finite() || !via.position.y.is_finite() {
            return Err("via position is not a number".into());
        }
        sites.push(ProbeSite {
            net: via.net,
            object: via.id,
            position: via.position,
            diameter_mm: via.diameter,
            layer: Layer::Front,
            kind: "via",
        });
    }
    Ok(sites)
}

fn site_has_room(site: &ProbeSite, others: &[ProbeSite], min_spacing_mm: f64) -> bool {
    for other in others {
        if other.object == site.object {
            continue;
        }
        if other.net == site.net {
            continue;
        }
        if site.position.distance(other.position) < min_spacing_mm {
            return false;
        }
    }
    true
}

fn qualifying_sites(sites: &[ProbeSite], settings: ProbeSettings) -> Vec<ProbeSite> {
    let sized: Vec<ProbeSite> = sites
        .iter()
        .filter(|s| s.diameter_mm + 1e-9 >= settings.min_pad_mm)
        .cloned()
        .collect();
    sized
        .iter()
        .filter(|s| site_has_room(s, &sized, settings.min_spacing_mm))
        .cloned()
        .collect()
}

/// Which routed nets lack a probe site that meets the minimum pad and spacing.
pub fn coverage_report(project: &Project) -> Result<CoverageReport, String> {
    let settings = probe_settings(project);
    let board_nets = collect_board_nets(project);
    let sites = probe_sites(project)?;
    let good = qualifying_sites(&sites, settings);
    let mut covered = BTreeSet::new();
    for site in &good {
        covered.insert(site.net);
    }
    let uncovered: Vec<Id> = board_nets
        .iter()
        .copied()
        .filter(|n| !covered.contains(n))
        .collect();
    Ok(CoverageReport {
        settings,
        uncovered,
        covered,
    })
}

/// Design-rule findings for nets that cannot be probed at the recorded minimums.
pub fn coverage_findings(project: &Project) -> Vec<Finding> {
    let Ok(report) = coverage_report(project) else {
        return vec![];
    };
    let min_pad = report.settings.min_pad_mm;
    let min_space = report.settings.min_spacing_mm;
    report
        .uncovered
        .into_iter()
        .map(|net| Finding {
            code: "testpoint_coverage",
            severity: project.rule_severity("testpoint_coverage", "warning"),
            message: format!(
                "{} has no probe pad at least {:.3} mm across with {:.3} mm spacing to other probe sites",
                project.net_name(net),
                min_pad,
                min_space
            ),
            objects: vec![net],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Footprint, Net, Project};

    fn board() -> Project {
        let mut p = Project::default();
        p.board.width = 60.0;
        p.board.height = 40.0;
        p
    }

    fn net(p: &mut Project, id: Id, name: &str) {
        p.nets.push(Net {
            id,
            name: name.into(),
            ..Default::default()
        });
    }

    fn tht_pad(net: Id, x: f64, y: f64, size: f64) -> Pad {
        Pad {
            number: "1".into(),
            offset: Point::new(x, y),
            diameter: size,
            net: Some(net),
            castellated: None,
            geometry: Some(pcbcraft_library::FootprintPad {
                number: "1".into(),
                pad_type: "thru_hole".into(),
                shape: "circle".into(),
                position: Point::new(x, y),
                rotation: 0.0,
                size: Point::new(size, size),
                layers: vec!["F.Cu".into(), "B.Cu".into()],
                drill: Some(pcbcraft_library::Drill {
                    offset: Point::default(),
                    size: Point::new(size / 2.0, size / 2.0),
                }),
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            }),
        }
    }

    #[test]
    fn net_with_large_tht_pad_is_covered() -> Result<(), String> {
        let mut p = board();
        net(&mut p, 1, "SIG");
        p.components.push(Component {
            id: 10,
            reference: "R1".into(),
            value: "R".into(),
            kind: "library".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        });
        p.board.footprints.push(Footprint {
            id: 20,
            component: 10,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![tht_pad(1, 0.0, 0.0, 1.2)],
        });
        p.validate()?;
        let report = coverage_report(&p)?;
        assert!(report.covered.contains(&1));
        assert!(report.uncovered.is_empty());
        Ok(())
    }

    #[test]
    fn small_smd_pad_without_test_mark_is_not_a_probe_site() -> Result<(), String> {
        let mut p = board();
        net(&mut p, 2, "I2C_SDA");
        p.components.push(Component {
            id: 11,
            reference: "U1".into(),
            value: "U".into(),
            kind: "library".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        });
        p.board.footprints.push(Footprint {
            id: 21,
            component: 11,
            position: Point::new(5.0, 5.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 0.6,
                net: Some(2),
                castellated: None,
                geometry: Some(pcbcraft_library::FootprintPad {
                    number: "1".into(),
                    pad_type: "smd".into(),
                    shape: "circle".into(),
                    position: Point::default(),
                    rotation: 0.0,
                    size: Point::new(0.6, 0.6),
                    layers: vec!["F.Cu".into()],
                    drill: None,
                    roundrect_ratio: None,
                    chamfer: None,
                    polygon: vec![],
                    raw_source: String::new(),
                }),
            }],
            ..Default::default()
        });
        p.validate()?;
        let report = coverage_report(&p)?;
        assert_eq!(report.uncovered, vec![2]);
        let findings = coverage_findings(&p);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "testpoint_coverage");
        assert!(
            findings[0].message.contains("0.800"),
            "{}",
            findings[0].message
        );
        Ok(())
    }

    #[test]
    fn via_on_net_counts_as_probe_site() -> Result<(), String> {
        let mut p = board();
        net(&mut p, 3, "GND");
        p.board.vias.push(Via {
            id: 30,
            net: 3,
            position: Point::new(12.0, 12.0),
            diameter: 0.9,
            drill: 0.3,
            from_layer: None,
            to_layer: None,
            covering: None,
        });
        p.validate()?;
        let report = coverage_report(&p)?;
        assert!(report.covered.contains(&3));
        Ok(())
    }
}
