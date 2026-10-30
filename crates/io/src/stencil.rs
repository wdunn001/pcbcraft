//! Solder paste stencil package: paste Gerbers, a laser cut path, and a manifest
//! with area-ratio warnings.

use crate::gerber::gerber_paste_stencil;
use pcbcraft_doc::{Footprint, Layer, Pad, Project};
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Minimum recommended area ratio (opening area over perimeter times thickness).
pub const DEFAULT_MIN_AREA_RATIO: f64 = 0.66;
/// Default stencil foil thickness used when settings omit one.
pub const DEFAULT_STENCIL_THICKNESS_MM: f64 = 0.12;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StencilOptions {
    #[serde(default)]
    pub kerf_mm: f64,
    #[serde(default)]
    pub reduction_pct: f64,
    #[serde(default = "default_thickness")]
    pub thickness_mm: f64,
    #[serde(default = "default_true")]
    pub cut_file: bool,
}

fn default_thickness() -> f64 {
    DEFAULT_STENCIL_THICKNESS_MM
}

fn default_true() -> bool {
    true
}

impl Default for StencilOptions {
    fn default() -> Self {
        Self {
            kerf_mm: 0.0,
            reduction_pct: 0.0,
            thickness_mm: DEFAULT_STENCIL_THICKNESS_MM,
            cut_file: true,
        }
    }
}

pub fn stencil_options_from_settings(settings: &serde_json::Value) -> Result<StencilOptions, String> {
    if settings.is_null() {
        return Ok(StencilOptions::default());
    }
    let opts: StencilOptions = serde_json::from_value(settings.clone())
        .map_err(|e| format!("stencil settings are invalid: {e}"))?;
    validate_stencil_options(&opts)
}

pub fn validate_stencil_options(o: &StencilOptions) -> Result<(), String> {
    if !(0.0..=0.5).contains(&o.kerf_mm) || !o.kerf_mm.is_finite() {
        return Err("stencil kerf must be 0 to 0.5 mm".into());
    }
    if !(0.0..=50.0).contains(&o.reduction_pct) || !o.reduction_pct.is_finite() {
        return Err("stencil reduction must be 0 to 50 percent".into());
    }
    if !(0.02..=0.5).contains(&o.thickness_mm) || !o.thickness_mm.is_finite() {
        return Err("stencil thickness must be 0.02 to 0.5 mm".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AreaRatioWarning {
    pub reference: String,
    pub pad: String,
    pub side: String,
    pub area_ratio: f64,
    pub minimum: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StencilManifest {
    pub format: String,
    pub version: u32,
    pub files: Vec<String>,
    pub area_ratio_warnings: Vec<AreaRatioWarning>,
}

fn reference_for(p: &Project, footprint: &Footprint) -> String {
    p.component(footprint.component)
        .map(|c| c.reference.clone())
        .unwrap_or_else(|| format!("U{}", footprint.component))
}

fn paste_margin(p: &Project, footprint: &Footprint) -> f64 {
    p.component(footprint.component)
        .and_then(|c| c.library_footprint.as_ref())
        .and_then(|lf| lf.definition.rules.as_ref())
        .and_then(|r| r.paste_margin)
        .unwrap_or(crate::gerber::DEFAULT_PASTE_MARGIN)
}

fn pad_on_layer(fp: &Footprint, pad: &Pad, layer: Layer) -> bool {
    let paste = if layer == Layer::Front {
        "F.Paste"
    } else {
        "B.Paste"
    };
    pad.geometry.as_ref().is_some_and(|g| {
        g.drill.is_none()
            && !g.pad_type.starts_with("thru_hole")
            && !g.pad_type.starts_with("np_thru_hole")
            && g.layers.iter().any(|l| l == paste)
    })
}

fn opening_ring(
    fp: &Footprint,
    pad: &Pad,
    margin: f64,
    reduction_pct: f64,
    kerf_mm: f64,
) -> Option<Vec<Point>> {
    let scale = 1.0 - reduction_pct / 100.0;
    let shrink = (-margin).max(0.0) + kerf_mm / 2.0;
    let grow = margin.max(0.0);
    let local: Vec<Point> = pad
        .outline_points()
        .into_iter()
        .map(|pt| {
            let x = pt.x * scale;
            let y = pt.y * scale;
            if shrink > 0.0 {
                Point::new(x - shrink, y - shrink)
            } else {
                Point::new(x + grow, y + grow)
            }
        })
        .collect();
    if local.len() < 3 {
        return None;
    }
    let pos = fp.pad_position(&pad.number)?;
    let mut ring: Vec<Point> = local.iter().map(|pt| pos.offset(*pt)).collect();
    if let Some(first) = ring.first().copied() {
        ring.push(first);
    }
    Some(ring)
}

fn polygon_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for pair in points.windows(2) {
        area += pair[0].x * pair[1].y - pair[1].x * pair[0].y;
    }
    (area / 2.0).abs()
}

fn polygon_perimeter(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

pub fn area_ratio_warnings(p: &Project, options: &StencilOptions) -> Result<Vec<AreaRatioWarning>, String> {
    p.validate()?;
    validate_stencil_options(options)?;
    let mut warnings = Vec::new();
    for layer in [Layer::Front, Layer::Back] {
        let side = if layer == Layer::Front {
            "top"
        } else {
            "bottom"
        };
        for fp in &p.board.footprints {
            let reference = reference_for(p, fp);
            let margin = paste_margin(p, fp);
            for pad in &fp.pads {
                if !pad_on_layer(fp, pad, layer) {
                    continue;
                }
                let Some(ring) = opening_ring(fp, pad, margin, options.reduction_pct, 0.0) else {
                    continue;
                };
                let area = polygon_area(&ring);
                let perimeter = polygon_perimeter(&ring);
                if perimeter <= 0.0 {
                    continue;
                }
                let ratio = area / (perimeter * options.thickness_mm);
                if ratio < DEFAULT_MIN_AREA_RATIO {
                    warnings.push(AreaRatioWarning {
                        reference: reference.clone(),
                        pad: pad.number.clone(),
                        side: side.into(),
                        area_ratio: (ratio * 1000.0).round() / 1000.0,
                        minimum: DEFAULT_MIN_AREA_RATIO,
                    });
                }
            }
        }
    }
    warnings.sort_by(|a, b| {
        (&a.reference, &a.pad, &a.side).cmp(&(&b.reference, &b.pad, &b.side))
    });
    Ok(warnings)
}

fn board_center_x(p: &Project) -> f64 {
    let edge = p
        .board
        .outline
        .as_ref()
        .map(|o| {
            o.iter()
                .map(|pt| pt.x)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                    (lo.min(x), hi.max(x))
                })
        })
        .unwrap_or((0.0, p.board.width));
    (edge.0 + edge.1) / 2.0
}

fn mirror_ring_x(ring: &mut [Point], center_x: f64) {
    for pt in ring.iter_mut() {
        pt.x = 2.0 * center_x - pt.x;
    }
}

fn mm_num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    r.to_string()
}

fn ring_to_svg_path(ring: &[Point]) -> String {
    let Some(first) = ring.first() else {
        return String::new();
    };
    let mut d = format!("M {} {}", mm_num(first.x), mm_num(first.y));
    for p in ring.iter().skip(1) {
        d.push_str(&format!(" L {} {}", mm_num(p.x), mm_num(p.y)));
    }
    if ring.len() > 2 {
        d.push_str(" Z");
    }
    d
}

fn bounds_of_rings(rings: &[Vec<Point>]) -> (Point, Point) {
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for ring in rings {
        for p in ring {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
    }
    if !min.x.is_finite() {
        return (Point::default(), Point::new(1.0, 1.0));
    }
    (min, max)
}

pub fn stencil_cut_svg(p: &Project, options: &StencilOptions) -> Result<String, String> {
    p.validate()?;
    validate_stencil_options(options)?;
    let center_x = board_center_x(p);
    let mut rings: Vec<Vec<Point>> = Vec::new();
    for layer in [Layer::Front, Layer::Back] {
        for fp in &p.board.footprints {
            let margin = paste_margin(p, fp);
            for pad in &fp.pads {
                if !pad_on_layer(fp, pad, layer) {
                    continue;
                }
                let Some(mut ring) =
                    opening_ring(fp, pad, margin, options.reduction_pct, options.kerf_mm)
                else {
                    continue;
                };
                if layer == Layer::Back {
                    mirror_ring_x(&mut ring, center_x);
                }
                rings.push(ring);
            }
        }
    }
    let (min, max) = bounds_of_rings(&rings);
    let width = (max.x - min.x).max(0.001);
    let height = (max.y - min.y).max(0.001);
    let mut body = String::new();
    body.push_str("<g id=\"paste-openings\" fill=\"none\" stroke=\"black\" stroke-width=\"0.05\">\n");
    for ring in &rings {
        body.push_str(&format!(
            "<path d=\"{}\"/>\n",
            ring_to_svg_path(ring)
        ));
    }
    body.push_str("</g>\n");
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"{min_x} {min_y} {w} {h}\">\n\
         {body}\
         </svg>\n",
        w = mm_num(width),
        h = mm_num(height),
        min_x = mm_num(min.x),
        min_y = mm_num(min.y),
        body = body
    ))
}

pub fn stencil_manifest_text(
    p: &Project,
    files: &[&str],
    warnings: &[AreaRatioWarning],
) -> Result<String, String> {
    p.validate()?;
    let manifest = StencilManifest {
        format: "pcbcraft-stencil-package".into(),
        version: 1,
        files: files.iter().map(|s| (*s).to_string()).collect(),
        area_ratio_warnings: warnings.to_vec(),
    };
    serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())
}

/// All files in a stencil package keyed by relative name.
pub fn stencil_files(p: &Project, options: &StencilOptions) -> Result<BTreeMap<String, String>, String> {
    p.validate()?;
    validate_stencil_options(options)?;
    let mut map = BTreeMap::new();
    map.insert(
        "paste-top.gbr".into(),
        gerber_paste_stencil(p, Layer::Front)?,
    );
    map.insert(
        "paste-bottom.gbr".into(),
        gerber_paste_stencil(p, Layer::Back)?,
    );
    let mut names = vec!["paste-top.gbr".to_string(), "paste-bottom.gbr".to_string()];
    if options.cut_file {
        map.insert("stencil-cut.svg".into(), stencil_cut_svg(p, options)?);
        names.push("stencil-cut.svg".into());
    }
    let warnings = area_ratio_warnings(p, options)?;
    let manifest = stencil_manifest_text(
        p,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
        &warnings,
    )?;
    map.insert("manifest.json".into(), manifest);
    Ok(map)
}

/// Paste Gerbers for export-package `paste_gerbers` (top and bottom, not mirrored).
pub fn paste_gerber_files(p: &Project) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    map.insert(
        "paste-top.gbr".into(),
        gerber_paste_stencil(p, Layer::Front)?,
    );
    map.insert(
        "paste-bottom.gbr".into(),
        gerber_paste_stencil(p, Layer::Back)?,
    );
    Ok(map)
}

#[cfg(test)]
mod stencil_tests {
    use super::*;
    use pcbcraft_doc::{Footprint, Project};

    fn back_smd_fixture() -> Result<Project, String> {
        let mut p = Project::default();
        let pad = pcbcraft_doc::Pad {
            number: "1".into(),
            diameter: 1.0,
            offset: Point::default(),
            geometry: Some(pcbcraft_library::FootprintPad {
                number: "1".into(),
                pad_type: "smd".into(),
                shape: "rect".into(),
                position: Point::default(),
                rotation: 0.0,
                size: Point::new(1.2, 0.8),
                layers: vec!["B.Cu".into(), "B.Paste".into(), "B.Mask".into()],
                drill: None,
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            }),
        };
        p.board.footprints.push(Footprint {
            id: 1,
            component: 1,
            position: Point::new(30.0, 20.0),
            rotation: 0.0,
            pads: vec![pad.clone()],
            ..Default::default()
        });
        p.components.push(pcbcraft_doc::Component {
            id: 1,
            reference: "U1".into(),
            ..Default::default()
        });
        p.board.fiducials.push(pcbcraft_doc::Fiducial {
            id: 2,
            position: Point::new(5.0, 5.0),
            diameter: 1.0,
            layer: Layer::Front,
        });
        p.validate()?;
        Ok(p)
    }

    #[test]
    fn stencil_paste_gerber_keeps_bottom_coordinates() -> Result<(), String> {
        let p = back_smd_fixture()?;
        let fab = crate::gerber::gerber_paste(&p, Layer::Back)?;
        let stencil = gerber_paste_stencil(&p, Layer::Back)?;
        assert!(
            fab.contains("X-30000000Y20000000"),
            "fab paste mirrors bottom x"
        );
        assert!(
            stencil.contains("X30000000Y20000000"),
            "stencil paste keeps board x"
        );
        assert!(
            stencil.contains("FiducialMark"),
            "fiducials use their own aperture"
        );
        Ok(())
    }

    #[test]
    fn stencil_cut_mirrors_bottom_openings_only() -> Result<(), String> {
        let p = back_smd_fixture()?;
        let svg = stencil_cut_svg(&p, &StencilOptions::default())?;
        assert!(
            svg.contains("300") && svg.contains("20"),
            "bottom pad appears mirrored in cut view: {svg}"
        );
        Ok(())
    }

    #[test]
    fn stencil_package_lists_area_ratio_warnings_in_manifest() -> Result<(), String> {
        let p = back_smd_fixture()?;
        let opts = StencilOptions {
            thickness_mm: 0.5,
            ..StencilOptions::default()
        };
        let files = stencil_files(&p, &opts)?;
        let manifest = files.get("manifest.json").ok_or("manifest")?;
        assert!(manifest.contains("area_ratio_warnings"));
        assert!(files.contains_key("paste-top.gbr"));
        assert!(files.contains_key("stencil-cut.svg"));
        Ok(())
    }
}
