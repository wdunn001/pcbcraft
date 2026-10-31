//! Top and bottom assembly drawings: outline, designators, pin 1, fiducials.
use crate::{DrawPage, Point};
use pcbcraft_doc::{Layer, Project};
use pcbcraft_library::Graphic;
use std::collections::BTreeMap;

const PDF_MAX_OPS: usize = 20_000;

mod labels {
    pub const ASSEMBLY_TOP_TITLE: &str = "Assembly drawing (top)";
    pub const ASSEMBLY_BOTTOM_TITLE: &str = "Assembly drawing (bottom)";
    pub const TITLE: &str = "Title";
    pub const REVISION: &str = "Revision";
    pub const COMPANY: &str = "Company";
    pub const DATE: &str = "Date";
}
use labels as draw_lbl;

#[derive(Clone, Copy, Debug)]
struct AssemblyOptions {
    back: bool,
    mirror: bool,
}

pub fn view_is_assembly(view: &str) -> bool {
    matches!(view, "assembly-top" | "assembly-bottom")
}

pub fn page_bounds(p: &Project, view: &str) -> (Point, Point) {
    let (mut min, mut max) = board_bounds(p);
    let margin = 8.0;
    min.x -= margin;
    min.y -= margin;
    max.x += margin + 55.0;
    max.y += margin + 10.0;
    if view.starts_with("assembly") {
        max.y += 10.0;
    }
    (min, max)
}

pub fn draw(page: &mut impl DrawPage, p: &Project, view: &str) -> Result<(), String> {
    let options = match view {
        "assembly-top" => AssemblyOptions {
            back: false,
            mirror: false,
        },
        "assembly-bottom" => AssemblyOptions {
            back: true,
            mirror: true,
        },
        _ => return Err("view is not an assembly drawing".into()),
    };
    draw_assembly(page, p, &options)
}

pub fn drawing_svg(p: &Project, view: &str) -> Result<String, String> {
    p.validate()?;
    if !view_is_assembly(view) {
        return Err("view is not an assembly drawing".into());
    }
    let mut page = SvgPage::new();
    draw(&mut page, p, view)?;
    let title = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    page.finish(&format!("{title} {view}"))
}

pub fn assembly_drawing_pdfs(p: &Project) -> Result<BTreeMap<String, Vec<u8>>, String> {
    p.validate()?;
    let mut files = BTreeMap::new();
    files.insert(
        "assembly-top.pdf".into(),
        crate::pdf_document(p, "assembly-top")?,
    );
    files.insert(
        "assembly-bottom.pdf".into(),
        crate::pdf_document(p, "assembly-bottom")?,
    );
    Ok(files)
}

fn board_outline_points(p: &Project) -> Vec<Point> {
    match &p.board.outline {
        Some(o) => o.clone(),
        None => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
        ],
    }
}

fn board_bounds(p: &Project) -> (Point, Point) {
    let outline = board_outline_points(p);
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for pt in outline {
        min.x = min.x.min(pt.x);
        min.y = min.y.min(pt.y);
        max.x = max.x.max(pt.x);
        max.y = max.y.max(pt.y);
    }
    if !min.x.is_finite() {
        return (Point::default(), Point::new(p.board.width, p.board.height));
    }
    (min, max)
}

fn map_point(p: Point, mirror: bool, cx: f64) -> Point {
    if mirror {
        Point::new(2.0 * cx - p.x, p.y)
    } else {
        p
    }
}

fn lib_graphic_layer(g: &Graphic) -> &str {
    match g {
        Graphic::Line { layer, .. }
        | Graphic::Rectangle { layer, .. }
        | Graphic::Circle { layer, .. }
        | Graphic::Arc { layer, .. }
        | Graphic::Polyline { layer, .. }
        | Graphic::Text { layer, .. } => layer,
    }
}

fn silk_or_fab_layer(layer: &str) -> bool {
    layer.ends_with("SilkS") || layer.ends_with(".Fab")
}

fn footprint_outline_path(
    p: &Project,
    f: &pcbcraft_doc::Footprint,
    map: impl Fn(Point) -> Point,
) -> Vec<Point> {
    if let Some(c) = p.component(f.component)
        && let Some(lib) = c.library_footprint.as_ref()
    {
        let mut pts = Vec::new();
        for g in &lib.definition.graphics {
            if lib_graphic_layer(g).contains("CrtYd") {
                for pt in g.path_points() {
                    pts.push(map(f.position.offset(pt.rotate(-f.rotation))));
                }
            }
        }
        if pts.len() >= 3 {
            return pts;
        }
    }
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for pad in &f.pads {
        let (hx, hy) = pad
            .geometry
            .as_ref()
            .map(|g| (g.size.x / 2.0, g.size.y / 2.0))
            .unwrap_or((pad.diameter / 2.0, pad.diameter / 2.0));
        for corner in [
            Point::new(-hx, -hy),
            Point::new(hx, -hy),
            Point::new(hx, hy),
            Point::new(-hx, hy),
        ] {
            let w = map(f.position.offset(corner.rotate(-f.rotation)));
            min.x = min.x.min(w.x);
            min.y = min.y.min(w.y);
            max.x = max.x.max(w.x);
            max.y = max.y.max(w.y);
        }
    }
    vec![
        map(Point::new(min.x, min.y)),
        map(Point::new(max.x, min.y)),
        map(Point::new(max.x, max.y)),
        map(Point::new(min.x, max.y)),
    ]
}

fn pad_one_triangle(
    f: &pcbcraft_doc::Footprint,
    map: impl Fn(Point) -> Point,
) -> Option<Vec<Point>> {
    let pad = f.pads.iter().find(|p| p.number == "1")?;
    let (hx, hy) = pad
        .geometry
        .as_ref()
        .map(|g| (g.size.x / 2.0, g.size.y / 2.0))
        .unwrap_or((pad.diameter / 2.0, pad.diameter / 2.0));
    let centre = map(f.position.offset(pad.offset.rotate(-f.rotation)));
    let s = hx.min(hy).max(0.2);
    Some(vec![
        map(centre.offset(Point::new(0.0, s))),
        map(centre.offset(Point::new(-s * 0.866, -s * 0.5))),
        map(centre.offset(Point::new(s * 0.866, -s * 0.5))),
    ])
}

fn draw_fiducial_cross(
    page: &mut impl DrawPage,
    fid: &pcbcraft_doc::Fiducial,
    map: impl Fn(Point) -> Point,
    rgb: [f64; 3],
) -> Result<(), String> {
    let c = map(fid.position);
    let r = fid.diameter / 2.0;
    page.circle(c, r, rgb, 0.12, false)?;
    let arm = r * 1.4;
    page.path(
        &[
            map(Point::new(c.x - arm, c.y)),
            map(Point::new(c.x + arm, c.y)),
        ],
        rgb,
        0.12,
        false,
    )?;
    page.path(
        &[
            map(Point::new(c.x, c.y - arm)),
            map(Point::new(c.x, c.y + arm)),
        ],
        rgb,
        0.12,
        false,
    )
}

fn meta_value(p: &Project, key: &str) -> String {
    p.meta.get(key).cloned().unwrap_or_default()
}

fn simple_title_block(
    page: &mut impl DrawPage,
    p: &Project,
    origin: Point,
    title: &str,
) -> Result<(), String> {
    let ink = [0.05, 0.05, 0.05];
    let w = 42.0;
    let h = 28.0;
    let rect = [
        origin,
        Point::new(origin.x + w, origin.y),
        Point::new(origin.x + w, origin.y + h),
        Point::new(origin.x, origin.y + h),
    ];
    page.path(&rect, ink, 0.15, false)?;
    let design = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    let revision = meta_value(p, "revision");
    let company = meta_value(p, "company");
    let date = meta_value(p, "date");
    let rows: [(&str, &str); 5] = [
        (draw_lbl::TITLE, title),
        (draw_lbl::REVISION, revision.as_str()),
        (draw_lbl::COMPANY, company.as_str()),
        (draw_lbl::DATE, date.as_str()),
        ("Design", design),
    ];
    let mut y = origin.y + h - 3.0;
    for (label, value) in rows {
        page.text(
            Point::new(origin.x + 1.5, y),
            1.8,
            ink,
            &format!("{label}: {value}"),
        )?;
        y -= 4.5;
    }
    Ok(())
}

fn draw_footprint_silk(
    page: &mut impl DrawPage,
    p: &Project,
    f: &pcbcraft_doc::Footprint,
    back: bool,
    map: impl Fn(Point) -> Point + Copy,
) -> Result<(), String> {
    let Some(c) = p.component(f.component) else {
        return Ok(());
    };
    let Some(lib) = c.library_footprint.as_ref() else {
        return Ok(());
    };
    let want_prefix = if back { "B." } else { "F." };
    for g in &lib.definition.graphics {
        let layer = lib_graphic_layer(g);
        if !layer.starts_with(want_prefix) || !silk_or_fab_layer(layer) {
            continue;
        }
        let mut g = g.clone();
        if let Graphic::Text { text, .. } = &mut g {
            *text = text
                .replace("${REFERENCE}", &c.reference)
                .replace("${VALUE}", &c.value)
                .replace("%R", &c.reference)
                .replace("%V", &c.value);
        }
        let at = f.position;
        let rot = f.rotation;
        let spin = |pt: Point| map(at.offset(pt.rotate(-rot)));
        if let Graphic::Text {
            position,
            size,
            text,
            ..
        } = &g
        {
            page.text(spin(*position), size.y, [0.05, 0.05, 0.05], text)?;
            continue;
        }
        let points: Vec<Point> = g.path_points().iter().map(|pt| spin(*pt)).collect();
        let (width, fill, _) = match &g {
            Graphic::Line { width, .. } | Graphic::Arc { width, .. } => (*width, false),
            Graphic::Rectangle { width, fill, .. }
            | Graphic::Circle { width, fill, .. }
            | Graphic::Polyline { width, fill, .. } => (
                *width,
                matches!(fill.as_str(), "solid" | "background" | "outline"),
            ),
            Graphic::Text { .. } => (0.0, false),
        };
        page.path(&points, [0.05, 0.05, 0.05], width, fill)?;
    }
    Ok(())
}

fn draw_assembly(
    page: &mut impl DrawPage,
    p: &Project,
    options: &AssemblyOptions,
) -> Result<(), String> {
    const MAX_PLACED: usize = 5_000;
    if p.board.footprints.len() > MAX_PLACED {
        return Err("assembly drawing exceeds the placed-object limit".into());
    }
    let (bmin, bmax) = board_bounds(p);
    let cx = (bmin.x + bmax.x) / 2.0;
    let map = |pt: Point| map_point(pt, options.mirror, cx);
    let ink = [0.05, 0.05, 0.05];
    let outline = board_outline_points(p)
        .into_iter()
        .map(map)
        .collect::<Vec<_>>();
    page.path(&outline, ink, 0.2, false)?;
    let refs: std::collections::BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    for f in &p.board.footprints {
        let on_back = f.board_side() == Layer::Back;
        if on_back != options.back {
            continue;
        }
        let outline = footprint_outline_path(p, f, map);
        page.path(&outline, ink, 0.12, false)?;
        draw_footprint_silk(page, p, f, options.back, map)?;
        if let Some(tri) = pad_one_triangle(f, map) {
            page.path(&tri, ink, 0.05, true)?;
        }
        if let Some(c) = refs.get(&f.component) {
            page.text(map(f.position), 1.6, ink, &c.reference)?;
        }
    }
    for fid in &p.board.fiducials {
        let on_back = fid.layer == Layer::Back;
        if on_back == options.back {
            draw_fiducial_cross(page, fid, map, ink)?;
        }
    }
    let title = if options.back {
        draw_lbl::ASSEMBLY_BOTTOM_TITLE
    } else {
        draw_lbl::ASSEMBLY_TOP_TITLE
    };
    simple_title_block(
        page,
        p,
        Point::new(bmax.x + 6.0, bmin.y),
        title,
    )?;
    Ok(())
}

struct SvgPage {
    body: String,
    ops: usize,
    min: Point,
    max: Point,
}

impl SvgPage {
    fn new() -> Self {
        Self {
            body: String::new(),
            ops: 0,
            min: Point::new(f64::INFINITY, f64::INFINITY),
            max: Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        }
    }
    fn grow(&mut self, p: Point) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }
    fn rgb_hex(rgb: [f64; 3]) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            (rgb[0] * 255.0).round() as u8,
            (rgb[1] * 255.0).round() as u8,
            (rgb[2] * 255.0).round() as u8
        )
    }
    fn finish(self, name: &str) -> Result<String, String> {
        let (min, max) = if self.min.x.is_finite() {
            (self.min, self.max)
        } else {
            (Point::new(-1.0, -1.0), Point::new(1.0, 1.0))
        };
        Ok(svg_document(name, min, max, self.body))
    }
}

impl DrawPage for SvgPage {
    fn path(
        &mut self,
        points: &[Point],
        rgb: [f64; 3],
        w: f64,
        fill: bool,
    ) -> Result<(), String> {
        self.ops += 1;
        if self.ops > PDF_MAX_OPS {
            return Err("the drawing is too complex for one SVG page".into());
        }
        if points.len() < 2 {
            return Ok(());
        }
        for p in points {
            self.grow(*p);
        }
        let list = points
            .iter()
            .map(|p| format!("{},{}", svg_num(p.x), svg_num(p.y)))
            .collect::<Vec<_>>()
            .join(" ");
        let color = Self::rgb_hex(rgb);
        if fill && points.len() >= 3 {
            self.body.push_str(&format!(
                "<polygon points=\"{list}\" fill=\"{color}\" stroke=\"{color}\" stroke-width=\"{}\"/>\n",
                svg_num(w.max(0.05))
            ));
        } else {
            self.body.push_str(&format!(
                "<polyline points=\"{list}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"/>\n",
                svg_num(w.max(0.05))
            ));
        }
        Ok(())
    }
    fn circle(
        &mut self,
        center: Point,
        radius: f64,
        rgb: [f64; 3],
        w: f64,
        fill: bool,
    ) -> Result<(), String> {
        self.ops += 1;
        if self.ops > PDF_MAX_OPS {
            return Err("the drawing is too complex for one SVG page".into());
        }
        self.grow(Point::new(center.x - radius, center.y - radius));
        self.grow(Point::new(center.x + radius, center.y + radius));
        let color = Self::rgb_hex(rgb);
        self.body.push_str(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
            svg_num(center.x),
            svg_num(center.y),
            svg_num(radius),
            if fill { color.as_str() } else { "none" },
            color,
            svg_num(w.max(0.05))
        ));
        Ok(())
    }
    fn text(&mut self, at: Point, size: f64, rgb: [f64; 3], value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Ok(());
        }
        self.ops += 1;
        if self.ops > PDF_MAX_OPS {
            return Err("the drawing is too complex for one SVG page".into());
        }
        self.grow(at);
        self.grow(Point::new(
            at.x + size * value.chars().count() as f64 * 0.5,
            at.y + size,
        ));
        let color = Self::rgb_hex(rgb);
        self.body.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{color}\">{}</text>\n",
            svg_num(at.x),
            svg_num(at.y),
            svg_num(size.max(0.5)),
            svg_escape(value)
        ));
        Ok(())
    }
}

fn svg_num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    r.to_string()
}

fn svg_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn svg_document(name: &str, box_min: Point, box_max: Point, body: String) -> String {
    let w = (box_max.x - box_min.x).max(1.0);
    let h = (box_max.y - box_min.y).max(1.0);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {w} {h}\"><title>{}</title>{body}</svg>",
        svg_num(box_min.x),
        svg_num(box_min.y),
        svg_escape(name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, Footprint, Layer, Pad, Project};
    use pcbcraft_geom::Point;
    fn two_sided_project() -> Result<Project, String> {
        let footprint = pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let pad_geom = |n: &str| -> Pad {
            let src = footprint
                .pads
                .iter()
                .find(|p| p.number == n)
                .ok_or("pad missing")?;
            Pad {
                number: src.number.clone(),
                offset: src.position,
                diameter: src.size.x.min(src.size.y),
                net: None,
                castellated: None,
                geometry: Some(src.clone()),
            }
        };
        let mut project = Project::default();
        project.board.width = 60.0;
        project.board.height = 40.0;
        project.components.push(Component {
            id: 1,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            ..Default::default()
        });
        project.components.push(Component {
            id: 2,
            reference: "R2".into(),
            value: "2k2".into(),
            kind: "resistor".into(),
            ..Default::default()
        });
        project.board.footprints.push(Footprint {
            id: 10,
            component: 1,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![pad_geom("1")?, pad_geom("2")?],
        });
        project.board.footprints.push(Footprint {
            id: 11,
            component: 2,
            position: Point::new(20.0, 15.0),
            rotation: 0.0,
            locked: None,
            side: Some(Layer::Back),
            pads: {
                let mut p1 = pad_geom("1")?;
                let mut p2 = pad_geom("2")?;
                p1.offset = Point::new(-p1.offset.x, p1.offset.y);
                p2.offset = Point::new(-p2.offset.x, p2.offset.y);
                vec![p1, p2]
            },
        });
        project.validate()?;
        Ok(project)
    }

    #[test]
    fn assembly_sides_list_designators_matching_the_bom() -> Result<(), String> {
        let project = two_sided_project()?;
        let bom = crate::bom_csv(&project, &[], None)?;
        assert!(bom.contains("R1"));
        assert!(bom.contains("R2"));
        let top = drawing_svg(&project, "assembly-top")?;
        let bottom = drawing_svg(&project, "assembly-bottom")?;
        assert!(top.contains(">R1<"), "top lists the top reference");
        assert!(!top.contains(">R2<"), "top omits the bottom reference");
        assert!(bottom.contains(">R2<"), "bottom lists the bottom reference");
        assert!(!bottom.contains(">R1<"), "bottom omits the top reference");
        Ok(())
    }

    #[test]
    fn assembly_bottom_mirrors_x_about_the_board_centre() -> Result<(), String> {
        let project = two_sided_project()?;
        let cx = project.board.width / 2.0;
        let mark = Point::new(10.0, 12.0);
        let mirrored = map_point(mark, true, cx);
        assert_eq!(mirrored.x, 2.0 * cx - mark.x);
        let svg = drawing_svg(&project, "assembly-bottom")?;
        assert!(
            svg.contains(draw_lbl::ASSEMBLY_BOTTOM_TITLE),
            "bottom title is present"
        );
        Ok(())
    }

    #[test]
    fn assembly_pdfs_are_valid_for_both_sides() -> Result<(), String> {
        let project = two_sided_project()?;
        let files = assembly_drawing_pdfs(&project)?;
        assert_eq!(files.len(), 2);
        for (name, bytes) in &files {
            assert!(name.ends_with(".pdf"), "{name}");
            assert!(bytes.starts_with(b"%PDF-1.4"), "{name} is a PDF");
        }
        Ok(())
    }
}
