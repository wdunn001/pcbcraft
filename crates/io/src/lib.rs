//! Native project persistence and original, limited KiCad PCB / preview writers.
use pcbcraft_doc::{Project, ProjectLibrary};
use pcbcraft_geom::Point;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Component;
use std::path::Path;
mod board_mesh;
mod drawn;
mod fab_export;
mod gerber;
mod layers;
mod library_render;
mod panel_export;
mod raster;
pub use board_mesh::{
    BoardMesh, COPPER_THICKNESS, MASK_THICKNESS, MAX_TRIANGLES, MeshLayers, MeshPart,
    ModelPlacement, PartKind, SILK_THICKNESS, apply_matrix, board_mesh, model_matrix,
};
pub use drawn::{DrawnLayerPlots, drawn_layer_plots};
pub use gerber::gerber_copper;
pub use layers::{
    FabLayer, GerberCopperCounts, LayerShapes, Shape, ShapePolarity, copper_shape_counts,
    fabrication_point, gerber_copper_counts, layer_shapes,
};
use library_render::*;
pub use panel_export::{panel_fabrication_files, panel_placement_text};
pub use pcbcraft_doc::Layer;
pub use raster::{
    Raster, RasterDepth, RasterOptions, RasterPixels, RegMarkShape, RegistrationMarks,
    layer_raster, raster_bmp, raster_job_pngs, raster_png, raster_svg,
};

pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

pub fn load(path: &Path) -> Result<Project, String> {
    let text = read_text(path)?;
    let project: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    project.validate()?;
    Ok(project)
}
pub fn read_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut text = String::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err("project exceeds 8 MiB".into());
    }
    Ok(text)
}
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("file exceeds 8 MiB".into());
    }
    Ok(bytes)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary.write_all(bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(target_arch = "wasm32")]
pub fn atomic_write(_path: &Path, _bytes: &[u8]) -> Result<(), String> {
    Err("host filesystem writes are unavailable in the browser; download support is planned".into())
}
pub fn save(project: &Project, path: &Path) -> Result<(), String> {
    project.validate()?;
    let bytes = serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("project exceeds 8 MiB".into());
    }
    atomic_write(path, &bytes)
}
fn quote(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}
/// SVG fragment for one project graphic: its outline as a stroked polyline, or a
/// text element for the text shape.
fn project_graphic_svg(g: &pcbcraft_doc::Graphic) -> String {
    if g.shape == "text" {
        let anchor = g.points.first().copied().unwrap_or_default();
        return format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"{}\">{}</text>",
            anchor.x,
            anchor.y,
            g.width,
            escape(&g.text)
        );
    }
    let points = g
        .outline()
        .unwrap_or_else(|| g.points.clone())
        .iter()
        .map(|p| format!("{},{}", p.x, p.y))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "<polyline points=\"{points}\" fill=\"none\" stroke=\"#5ae0bc\" stroke-width=\"{}\"/>",
        g.width.min(0.4)
    )
}
/// Text drawn for a label, with per-kind punctuation.
fn label_text(l: &pcbcraft_doc::Label, net_name: &str) -> String {
    match l.kind.as_str() {
        "global" => format!("{{{net_name}}}"),
        "hierarchical" => format!("/{net_name}"),
        "bus" => l.text.clone(),
        _ => net_name.to_string(),
    }
}
fn uuid(id: u64, kind: usize) -> String {
    format!("{kind:08x}-0000-4000-8000-{id:012x}")
}
fn footprint_bounds(p: &Project, f: &pcbcraft_doc::Footprint) -> (Point, Point) {
    let points = f.drawing_points(p);
    let min = points
        .iter()
        .fold(Point::new(f64::INFINITY, f64::INFINITY), |bounds, p| {
            Point::new(bounds.x.min(p.x), bounds.y.min(p.y))
        });
    let max = points.iter().fold(
        Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        |bounds, p| Point::new(bounds.x.max(p.x), bounds.y.max(p.y)),
    );
    if min.valid() && max.valid() {
        (min, max)
    } else {
        (Point::new(-3.4, -1.4), Point::new(3.4, 1.4))
    }
}

/// CRC-32 (IEEE, reflected polynomial) over a byte slice, for ZIP entries.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc = (crc >> 8) ^ CRC_TABLE[((crc as u8) ^ byte) as usize];
    }
    !crc
}
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// A self-contained project archive: a ZIP file (stored, no compression)
/// holding design.json plus one entry per attached text file, written
/// without any compression dependency.
pub fn archive_zip(p: &Project) -> Result<Vec<u8>, String> {
    let design = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    let mut entries: Vec<(String, Vec<u8>)> = vec![("design.json".into(), design.into_bytes())];
    for file in &p.files {
        entries.push((
            format!("files/{}", file.name),
            file.data.clone().into_bytes(),
        ));
    }
    zip_bytes(&entries)
}

/// Build a stored (uncompressed) ZIP file from named byte entries.
fn zip_bytes(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut zip: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut offset = 0u32;
    for (name, data) in entries {
        if name.len() > 255 {
            return Err("archive entry name is too long".into());
        }
        let crc = crc32(data);
        let mut local = Vec::new();
        local.extend_from_slice(b"PK\x03\x04");
        local.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name.as_bytes());
        local.extend_from_slice(data);
        let mut dir = Vec::new();
        dir.extend_from_slice(b"PK\x01\x02");
        dir.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        dir.extend_from_slice(&crc.to_le_bytes());
        dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        dir.extend_from_slice(&(name.len() as u16).to_le_bytes());
        dir.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        dir.extend_from_slice(&offset.to_le_bytes());
        dir.extend_from_slice(name.as_bytes());
        zip.extend_from_slice(&local);
        central.extend_from_slice(&dir);
        offset += local.len() as u32;
    }
    let directory_start = zip.len() as u32;
    zip.extend_from_slice(&central);
    zip.extend_from_slice(b"PK\x05\x06");
    zip.extend_from_slice(&[0, 0, 0, 0]);
    zip.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    zip.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    zip.extend_from_slice(&(central.len() as u32).to_le_bytes());
    zip.extend_from_slice(&directory_start.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    Ok(zip)
}

/// Read back the project stored in an archive written by `archive_zip`.
pub fn archive_project(bytes: &[u8]) -> Result<Project, String> {
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| "not a project archive: no archive directory".to_string())?;
    let read_u16 = |at: usize| -> u16 { u16::from_le_bytes([bytes[at], bytes[at + 1]]) };
    let read_u32 = |at: usize| -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let entries = read_u16(eocd + 10) as usize;
    let mut at = read_u32(eocd + 16) as usize;
    let mut design: Option<&[u8]> = None;
    for _ in 0..entries {
        if !bytes[at..].starts_with(b"PK\x01\x02") || at + 46 > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let size = read_u32(at + 20) as usize;
        let name_len = read_u16(at + 28) as usize;
        let extra_len = read_u16(at + 30) as usize;
        let comment_len = read_u16(at + 32) as usize;
        let local_at = read_u32(at + 42) as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let name = &bytes[at + 46..at + 46 + name_len];
        if name == b"design.json" {
            if local_at + 30 > bytes.len() || !bytes[local_at..].starts_with(b"PK\x03\x04") {
                return Err("corrupt archive entry header".into());
            }
            let local_name = read_u16(local_at + 26) as usize;
            let local_extra = read_u16(local_at + 28) as usize;
            if local_at + 30 + local_name > bytes.len()
                || &bytes[local_at + 30..local_at + 30 + local_name] != name
            {
                return Err("archive entry headers disagree about the name".into());
            }
            let data_at = local_at + 30 + local_name + local_extra;
            if data_at + size > bytes.len() {
                return Err("archive entry runs past the end of the file".into());
            }
            design = Some(&bytes[data_at..data_at + size]);
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    let design = design.ok_or_else(|| "archive holds no design.json".to_string())?;
    let text = std::str::from_utf8(design).map_err(|_| "design.json is not UTF-8".to_string())?;
    let project: Project =
        serde_json::from_str(text).map_err(|e| format!("design.json is invalid: {e}"))?;
    project
        .validate()
        .map_err(|e| format!("archived design is invalid: {e}"))?;
    Ok(project)
}

/// A small vector PDF writer: the drawing is collected as path and text
/// operations in page space (PostScript points, origin bottom left) and
/// assembled into a one-page document without any external dependency.
struct PdfPage {
    ops: String,
    count: usize,
    scale: f64,
    origin_x: f64,
    origin_y: f64,
    /// True emits PDF font resources; false emits PostScript font setup.
    pdf_font: bool,
}
/// Any surface that can receive the shared view drawing: path outlines,
/// circles and text, in millimetre design coordinates.
trait DrawPage {
    fn path(&mut self, points: &[Point], rgb: [f64; 3], w: f64, fill: bool) -> Result<(), String>;
    fn circle(
        &mut self,
        center: Point,
        radius: f64,
        rgb: [f64; 3],
        w: f64,
        fill: bool,
    ) -> Result<(), String>;
    fn text(&mut self, at: Point, size: f64, rgb: [f64; 3], value: &str) -> Result<(), String>;
}
impl DrawPage for PdfPage {
    fn path(&mut self, points: &[Point], rgb: [f64; 3], w: f64, fill: bool) -> Result<(), String> {
        PdfPage::path(self, points, rgb, w, fill)
    }
    fn circle(
        &mut self,
        center: Point,
        radius: f64,
        rgb: [f64; 3],
        w: f64,
        fill: bool,
    ) -> Result<(), String> {
        PdfPage::circle(self, center, radius, rgb, w, fill)
    }
    fn text(&mut self, at: Point, size: f64, rgb: [f64; 3], value: &str) -> Result<(), String> {
        PdfPage::text(self, at, size, rgb, value)
    }
}
const PDF_MAX_OPS: usize = 20_000;
const PDF_K: f64 = 72.0 / 25.4;

impl PdfPage {
    pub(crate) fn new() -> Self {
        PdfPage {
            ops: String::new(),
            count: 0,
            scale: 1.0,
            origin_x: 0.0,
            origin_y: 0.0,
            pdf_font: true,
        }
    }
    fn fit(&mut self, min: Point, max: Point) {
        self.fit_to(842.0, 595.0, 28.0, min, max);
    }
    fn fit_to(&mut self, page_w: f64, page_h: f64, margin: f64, min: Point, max: Point) {
        let design_w = (max.x - min.x).max(1.0);
        let design_h = (max.y - min.y).max(1.0);
        let fit_w = (page_w - 2.0 * margin) / (design_w * PDF_K);
        let fit_h = (page_h - 2.0 * margin) / (design_h * PDF_K);
        self.scale = fit_w.min(fit_h).clamp(0.02, 4.0);
        let draw_w = design_w * PDF_K * self.scale;
        let draw_h = design_h * PDF_K * self.scale;
        self.origin_x = (page_w - draw_w) / 2.0 - min.x * PDF_K * self.scale;
        self.origin_y = (page_h + draw_h) / 2.0 + min.y * PDF_K * self.scale;
    }
    fn at(&self, p: Point) -> (f64, f64) {
        (
            self.origin_x + p.x * PDF_K * self.scale,
            self.origin_y - p.y * PDF_K * self.scale,
        )
    }
    fn n(v: f64) -> String {
        format!("{:.3}", v + 0.0)
    }
    fn escape(s: &str) -> String {
        let ascii: String = s
            .chars()
            .map(|c| {
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '?'
                }
            })
            .collect();
        ascii
            .replace('\\', r"\\")
            .replace('(', r"\(")
            .replace(')', r"\)")
    }
    fn op(&mut self, text: &str) -> Result<(), String> {
        self.count += 1;
        if self.count > PDF_MAX_OPS {
            return Err("the drawing is too complex for one PDF page".into());
        }
        self.ops.push_str(text);
        self.ops.push('\n');
        Ok(())
    }
    fn color(&mut self, rgb: [f64; 3], stroke: bool) -> Result<(), String> {
        let [r, g, b] = rgb;
        self.op(&format!(
            "{} {} {} {}",
            Self::n(r),
            Self::n(g),
            Self::n(b),
            if stroke { "RG" } else { "rg" }
        ))
    }
    fn width(&mut self, w: f64) -> Result<(), String> {
        self.op(&format!("{} w", Self::n((w * PDF_K * self.scale).max(0.2))))
    }
    /// Trace a polyline subpath; `close` connects the last point to the first.
    fn subpath(&mut self, points: &[Point], close: bool) -> Result<(), String> {
        if points.len() < 2 {
            return Ok(());
        }
        let (x, y) = self.at(points[0]);
        self.op(&format!("{} {} m", Self::n(x), Self::n(y)))?;
        for p in &points[1..] {
            let (x, y) = self.at(*p);
            self.op(&format!("{} {} l", Self::n(x), Self::n(y)))?;
        }
        if close {
            self.op("h")?;
        }
        Ok(())
    }
    fn path(&mut self, points: &[Point], rgb: [f64; 3], w: f64, fill: bool) -> Result<(), String> {
        if points.len() < 2 {
            return Ok(());
        }
        self.color(rgb, true)?;
        self.color(rgb, false)?;
        self.width(w)?;
        self.subpath(points, true)?;
        self.op(if fill { "B" } else { "S" })
    }
    /// A circle approximated by four cubic Bézier quarters.
    fn circle(
        &mut self,
        center: Point,
        radius: f64,
        rgb: [f64; 3],
        w: f64,
        fill: bool,
    ) -> Result<(), String> {
        const C: f64 = 0.552_284_749_8;
        self.color(rgb, true)?;
        self.color(rgb, false)?;
        self.width(w)?;
        let (x, y) = self.at(Point::new(center.x - radius, center.y));
        self.op(&format!("{} {} m", Self::n(x), Self::n(y)))?;
        let k = radius * C;
        let arcs = [
            (
                Point::new(center.x - radius, center.y + k),
                Point::new(center.x - k, center.y + radius),
                Point::new(center.x, center.y + radius),
            ),
            (
                Point::new(center.x + k, center.y + radius),
                Point::new(center.x + radius, center.y + k),
                Point::new(center.x + radius, center.y),
            ),
            (
                Point::new(center.x + radius, center.y - k),
                Point::new(center.x + k, center.y - radius),
                Point::new(center.x, center.y - radius),
            ),
            (
                Point::new(center.x - k, center.y - radius),
                Point::new(center.x - radius, center.y - k),
                Point::new(center.x - radius, center.y),
            ),
        ];
        for (c1, c2, end) in arcs {
            let (x1, y1) = self.at(c1);
            let (x2, y2) = self.at(c2);
            let (x3, y3) = self.at(end);
            self.op(&format!(
                "{} {} {} {} {} {} c",
                Self::n(x1),
                Self::n(y1),
                Self::n(x2),
                Self::n(y2),
                Self::n(x3),
                Self::n(y3)
            ))?;
        }
        self.op("h")?;
        self.op(if fill { "B" } else { "S" })
    }
    fn text(&mut self, at: Point, size: f64, rgb: [f64; 3], value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Ok(());
        }
        let (x, y) = self.at(at);
        self.color(rgb, false)?;
        self.op(&format!(
            "BT {} {} {} Td ({}) Tj ET",
            if self.pdf_font {
                format!(
                    "/F1 {} Tf",
                    Self::n((size * PDF_K * self.scale).clamp(4.0, 72.0))
                )
            } else {
                format!(
                    "/Helvetica findfont {} scalefont setfont",
                    Self::n((size * PDF_K * self.scale).clamp(4.0, 72.0))
                )
            },
            Self::n(x),
            Self::n(y),
            Self::escape(value),
        ))
    }
}

/// Draw one library graphic at a position.
fn pdf_graphic(
    page: &mut impl DrawPage,
    g: &pcbcraft_library::Graphic,
    at: Point,
    rotation: f64,
) -> Result<(), String> {
    let spin = |p: Point| at.offset(p.rotate(-rotation));
    if let pcbcraft_library::Graphic::Text {
        position,
        size,
        text,
        ..
    } = g
    {
        return page.text(spin(*position), size.y, [0.0, 0.0, 0.0], text);
    }
    let points: Vec<Point> = g.path_points().iter().map(|p| spin(*p)).collect();
    let (width, fill, layer) = graphic_style(g);
    let [r8, g8, b8] = layer_color(layer).1;
    let rgb = [r8 as f64 / 255.0, g8 as f64 / 255.0, b8 as f64 / 255.0];
    page.path(&points, rgb, width, fill)
}

/// Draw one project graphic (free board or schematic artwork).
fn pdf_project_graphic(page: &mut impl DrawPage, g: &pcbcraft_doc::Graphic) -> Result<(), String> {
    if g.shape == "text" {
        let anchor = g.points.first().copied().unwrap_or_default();
        return page.text(anchor, g.width.max(1.0), [0.0, 0.0, 0.0], &g.text);
    }
    let points = g.outline().unwrap_or_else(|| g.points.clone());
    page.path(&points, [0.1, 0.45, 0.35], g.width.min(0.4), false)
}

/// Render one view of the design onto the page.
fn pdf_draw(page: &mut impl DrawPage, p: &Project, view: &str) -> Result<(), String> {
    let front = [0.85, 0.25, 0.15];
    let back = [0.2, 0.45, 0.8];
    let copper_edge = [0.0, 0.0, 0.0];
    let pad_gold = [0.72, 0.55, 0.15];
    let ink = [0.05, 0.05, 0.05];
    let wire_green = [0.0, 0.45, 0.35];
    if view == "pcb" {
        match &p.board.outline {
            Some(o) => page.path(o, copper_edge, 0.2, true)?,
            None => {
                let rect = [
                    Point::new(0.0, 0.0),
                    Point::new(p.board.width, 0.0),
                    Point::new(p.board.width, p.board.height),
                    Point::new(0.0, p.board.height),
                ];
                page.path(&rect, copper_edge, 0.2, true)?;
            }
        }
        for c in &p.board.cutouts {
            page.circle(c.center, c.radius, [1.0, 1.0, 1.0], 0.1, true)?;
        }
        for z in &p.board.zones {
            let rgb = if z.layer == pcbcraft_doc::Layer::Front {
                front
            } else {
                back
            };
            page.path(&z.outline, rgb, 0.1, z.filled)?;
            for cut in &z.cutouts {
                page.path(cut, rgb, 0.1, false)?;
            }
        }
        for t in &p.board.tracks {
            let rgb = if t.layer == pcbcraft_doc::Layer::Front {
                front
            } else {
                back
            };
            match t.curve() {
                Some(arc) => {
                    let mut points = vec![t.start];
                    let steps = 24;
                    for i in 1..steps {
                        let f = i as f64 / steps as f64;
                        points.push(arc.point_at(f));
                    }
                    points.push(t.end);
                    page.path(&points, rgb, t.width, false)?;
                }
                None => page.path(&[t.start, t.end], rgb, t.width, false)?,
            }
        }
        for v in &p.board.vias {
            page.circle(v.position, v.diameter / 2.0, front, 0.05, true)?;
            if v.drill > 0.0 {
                page.circle(v.position, v.drill / 2.0, [1.0, 1.0, 1.0], 0.0, true)?;
            }
        }
        for t in &p.board.texts {
            let rgb = if t.layer == pcbcraft_doc::Layer::Front {
                ink
            } else {
                [0.2, 0.35, 0.6]
            };
            page.text(t.position, t.size.max(1.0), rgb, &board_text(p, t))?;
        }
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            pdf_project_graphic(page, g)?;
        }
        for f in &p.board.footprints {
            if let Some(library) = p
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
            {
                for g in &library.definition.graphics {
                    let mut g = g.clone();
                    if let pcbcraft_library::Graphic::Text { text, .. } = &mut g
                        && let Some(c) = p.component(f.component)
                    {
                        *text = text
                            .replace("${REFERENCE}", &c.reference)
                            .replace("${VALUE}", &c.value)
                            .replace("%R", &c.reference)
                            .replace("%V", &c.value);
                    }
                    pdf_graphic(page, &g, f.position, f.rotation)?;
                }
            }
            for pad in &f.pads {
                let points: Vec<Point> = pad
                    .outline_points()
                    .into_iter()
                    .map(|pos| f.position.offset(pos.rotate(-f.rotation)))
                    .collect();
                page.path(&points, pad_gold, 0.0, true)?;
                let drill: Vec<Point> = pad
                    .drill_outline_points()
                    .into_iter()
                    .map(|pos| f.position.offset(pos.rotate(-f.rotation)))
                    .collect();
                if !drill.is_empty() {
                    page.path(&drill, [1.0, 1.0, 1.0], 0.0, true)?;
                }
            }
            if let Some(c) = p.component(f.component) {
                let (min, max) = footprint_bounds(p, f);
                let label_x = (min.x + max.x) / 2.0;
                for (label, y) in [(&c.reference, min.y - 1.2), (&c.value, max.y + 1.2)] {
                    let point = f
                        .position
                        .offset(Point::new(label_x, y).rotate(-f.rotation));
                    page.text(point, 1.0, ink, label)?;
                }
            }
        }
    } else {
        for w in &p.wires {
            if let Some(path) = p.wire_path(w) {
                page.path(&path, wire_green, 0.2, false)?;
            }
        }
        for w in &p.drawn_wires {
            page.path(&w.points, wire_green, 0.2, false)?;
        }
        for g in p.graphics.iter().filter(|g| g.layer == "schematic") {
            pdf_project_graphic(page, g)?;
        }
        for l in &p.labels {
            if l.kind == "junction" {
                page.circle(l.position, 0.35, wire_green, 0.0, true)?;
            } else {
                page.text(l.position, 1.5, ink, &label_text(l, p.net_name(l.net)))?;
            }
        }
        for fc in &p.no_connects {
            if let Some(point) = p.pin_position(&fc.pin) {
                page.path(
                    &[
                        Point::new(point.x - 0.6, point.y - 0.6),
                        Point::new(point.x + 0.6, point.y + 0.6),
                    ],
                    front,
                    0.25,
                    false,
                )?;
                page.path(
                    &[
                        Point::new(point.x - 0.6, point.y + 0.6),
                        Point::new(point.x + 0.6, point.y - 0.6),
                    ],
                    front,
                    0.25,
                    false,
                )?;
            }
        }
        for c in p.components.iter().flat_map(|c| c.schematic_views()) {
            if let Some(library) = &c.library_symbol {
                for g in library
                    .definition
                    .graphics
                    .iter()
                    .filter(|g| (g.unit == 0 || g.unit == library.unit) && g.body_style <= 1)
                {
                    pdf_graphic(page, &g.primitive, c.position, c.rotation)?;
                }
                for pin in library.definition.pins.iter().filter(|pin| {
                    (pin.unit == 0 && library.unit == 1 || pin.unit == library.unit)
                        && pin.body_style <= 1
                }) {
                    let angle = pin.angle.to_radians();
                    let local_end = pin.position.offset(Point::new(
                        pin.length * angle.cos(),
                        -pin.length * angle.sin(),
                    ));
                    let start = c.position.offset(pin.position.rotate(-c.rotation));
                    let end = c.position.offset(local_end.rotate(-c.rotation));
                    page.path(&[start, end], ink, 0.15, false)?;
                    page.text(start, 0.9, ink, &pin.number)?;
                }
            } else {
                let box_points = [
                    Point::new(-3.0, -1.5),
                    Point::new(3.0, -1.5),
                    Point::new(3.0, 1.5),
                    Point::new(-3.0, 1.5),
                ];
                let body: Vec<Point> = box_points
                    .iter()
                    .map(|p| c.position.offset(p.rotate(-c.rotation)))
                    .collect();
                page.path(&body, ink, 0.15, false)?;
                for stub in [[-5.0, -3.0], [3.0, 5.0]] {
                    let a = c
                        .position
                        .offset(Point::new(stub[0], 0.0).rotate(-c.rotation));
                    let b = c
                        .position
                        .offset(Point::new(stub[1], 0.0).rotate(-c.rotation));
                    page.path(&[a, b], ink, 0.2, false)?;
                }
            }
            page.text(
                Point::new(c.position.x, c.position.y - 3.0),
                1.5,
                ink,
                &format!("{} {}", c.reference, c.value),
            )?;
        }
    }
    Ok(())
}

/// Export the design as a one-page vector PDF document, in bytes.
pub fn pdf_document(p: &Project, view: &str) -> Result<Vec<u8>, String> {
    p.validate()?;
    if view != "pcb" && view != "schematic" {
        return Err("view must be pcb or schematic".into());
    }
    let (min, max) = preview_bounds(p, view);
    let mut page = PdfPage::new();
    page.fit(min, max);
    page.color([1.0, 1.0, 1.0], false)?;
    page.op("0 0 842 595 re f")?;
    pdf_draw(&mut page, p, view)?;
    let content = page.ops.as_bytes();
    let mut pdf: Vec<u8> = Vec::new();
    let mut offsets = [0u32; 5];
    pdf.extend_from_slice(b"%PDF-1.4\n");
    for (index, body) in [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 842 595] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            content.len(),
            String::from_utf8_lossy(content)
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ]
    .iter()
    .enumerate()
    {
        offsets[index] = pdf.len() as u32;
        pdf.extend_from_slice(
            format!("{} 0 obj\n{}\nendobj\n", index + 1, body)
                .as_bytes(),
        );
    }
    let xref_at = pdf.len() as u32;
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets.iter() {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n").as_bytes(),
    );
    Ok(pdf)
}

/// Draw one view as a standalone PostScript document: the same path, circle
/// and text operations the PDF writer emits, wrapped in a PostScript header
/// with a letter-size landscape page box and closed by showpage.
pub fn postscript(p: &Project, view: &str) -> Result<String, String> {
    p.validate()?;
    if view != "pcb" && view != "schematic" {
        return Err("view must be pcb or schematic".into());
    }
    let (min, max) = preview_bounds(p, view);
    let mut page = PdfPage::new();
    page.pdf_font = false;
    page.fit_to(792.0, 612.0, 36.0, min, max);
    page.color([1.0, 1.0, 1.0], false)?;
    page.op("0 0 792 612 re f")?;
    pdf_draw(&mut page, p, view)?;
    let title = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    Ok(format!(
        "%!PS-Adobe-3.0\n%%Title: PCBCraft {view} view of {title}\n%%BoundingBox: 0 0 792 612\n%%Pages: 1\n%%EndComments\n%%Page: 1 1\n{}showpage\n%%EOF\n",
        page.ops
    ))
}

/// Page surface for a pen plotter: every path becomes one pen-up move and
/// one pen-down polyline in plotter units, circles become 32-segment
/// polygons, fills plot as outlines and text becomes a box of its own
/// approximate width so label positions still reach the paper.
struct Hpgl {
    body: String,
    ops: usize,
    scale: f64,
    ox: f64,
    oy: f64,
}
impl Hpgl {
    fn new() -> Self {
        Self {
            body: String::new(),
            ops: 0,
            scale: 1.0,
            ox: 0.0,
            oy: 0.0,
        }
    }
    fn fit(&mut self, min: Point, max: Point) {
        let w = (max.x - min.x).max(0.001);
        let h = (max.y - min.y).max(0.001);
        self.scale = (950.0 / w).min(700.0 / h);
        self.ox = 25.0 - min.x * self.scale;
        self.oy = 25.0 - min.y * self.scale;
    }
    fn xy(&self, x: f64, y: f64) -> (i64, i64) {
        (
            (x * self.scale + self.ox).round() as i64,
            (y * self.scale + self.oy).round() as i64,
        )
    }
}
impl DrawPage for Hpgl {
    fn path(
        &mut self,
        points: &[Point],
        _rgb: [f64; 3],
        _w: f64,
        _fill: bool,
    ) -> Result<(), String> {
        self.ops += 1;
        if self.ops > 20_000 {
            return Err("plot file exceeds the pen-move limit".into());
        }
        if points.len() < 2 {
            return Ok(());
        }
        let pts: Vec<(i64, i64)> = points.iter().map(|q| self.xy(q.x, q.y)).collect();
        let rest: Vec<String> = pts[1..].iter().map(|(x, y)| format!("{x},{y}")).collect();
        self.body.push_str(&format!(
            "PU{},{};PD{};\n",
            pts[0].0,
            pts[0].1,
            rest.join(",")
        ));
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
        let ring: Vec<Point> = (0..=32)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 32.0;
                Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
            })
            .collect();
        self.path(&ring, rgb, w, fill)
    }
    fn text(&mut self, at: Point, size: f64, rgb: [f64; 3], value: &str) -> Result<(), String> {
        let w = size * 0.62 * value.chars().count().max(1) as f64;
        let box_points = [
            at,
            Point::new(at.x + w, at.y),
            Point::new(at.x + w, at.y + size),
            Point::new(at.x, at.y + size),
            at,
        ];
        self.path(&box_points, rgb, size * 0.1, false)
    }
}

/// Draw one view as an HP-GL pen plotter file that a classic vector plotter
/// can run directly, with copper, pads, zones and text label boxes reduced
/// to pen moves.
pub fn hpgl_plot(p: &Project, view: &str) -> Result<String, String> {
    p.validate()?;
    if view != "pcb" && view != "schematic" {
        return Err("view must be pcb or schematic".into());
    }
    let (min, max) = preview_bounds(p, view);
    let mut page = Hpgl::new();
    page.fit(min, max);
    pdf_draw(&mut page, p, view)?;
    Ok(format!("IN;SP1;\n{}PU;\n", page.body))
}

/// An ODB++ style board package: a stored ZIP holding one job tree with a
/// layer matrix, polygon pad symbols, per-layer copper features and placed
/// components with their pad stacks, written without any compression
/// dependency.
pub fn odb_package(p: &Project) -> Result<Vec<u8>, String> {
    p.validate()?;
    const MAX_FEATURES: usize = 100_000;
    const MAX_SYMBOLS: usize = 256;
    const MAX_COMPONENTS: usize = 5_000;
    let folded: String = p
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let mut job = folded.trim_matches('_').to_string();
    job.truncate(24);
    if job.is_empty() {
        job = "board".into();
    }
    let mirror = |x: f64, back: bool| -> f64 { if back { -x } else { x } };
    let spin = |rotation: f64, back: bool| -> f64 {
        if back {
            (180.0 + rotation).rem_euclid(360.0)
        } else {
            (-rotation).rem_euclid(360.0)
        }
    };
    let um = |v: f64| (v.max(0.001) * 1_000.0).round() as u64;
    let mut symbols: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    let mut symbol = |name: String, w: f64, h: f64, round: bool| -> Result<String, String> {
        if symbols.contains_key(&name) {
            return Ok(name);
        }
        if symbols.len() >= MAX_SYMBOLS {
            return Err("odb export needs more than 256 distinct pad shapes".into());
        }
        let points = if round {
            (0..16)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / 16.0;
                    (w / 2.0 * a.cos(), h / 2.0 * a.sin())
                })
                .collect()
        } else {
            vec![
                (-w / 2.0, -h / 2.0),
                (w / 2.0, -h / 2.0),
                (w / 2.0, h / 2.0),
                (-w / 2.0, h / 2.0),
            ]
        };
        symbols.insert(name.clone(), points);
        Ok(name)
    };
    struct Feat {
        body: String,
        ops: usize,
        pos: usize,
    }
    impl Feat {
        fn new() -> Self {
            Self {
                body: String::new(),
                ops: 0,
                pos: 1,
            }
        }
        fn bump(&mut self) -> Result<usize, String> {
            self.ops += 1;
            if self.ops > MAX_FEATURES {
                return Err("odb export exceeds the feature limit".into());
            }
            let pos = self.pos;
            self.pos += 1;
            Ok(pos)
        }
        fn symbol(&mut self, name: &str, x: f64, y: f64, r: f64) -> Result<(), String> {
            let pos = self.bump()?;
            self.body.push_str(&format!(
                "\t.Symbol {name} {pos} P R {x:.6} {y:.6} {r:.3} N\n"
            ));
            Ok(())
        }
        pub(crate) fn line(
            &mut self,
            w: f64,
            x1: f64,
            y1: f64,
            x2: f64,
            y2: f64,
        ) -> Result<(), String> {
            let pos = self.bump()?;
            self.body.push_str(&format!(
                "\t.Line {w:.6} {pos} P R {x1:.6} {y1:.6} {x2:.6} {y2:.6}\n"
            ));
            Ok(())
        }
        pub(crate) fn arc(
            &mut self,
            w: f64,
            from: (f64, f64),
            to: (f64, f64),
            center: (f64, f64),
            cw: bool,
        ) -> Result<(), String> {
            let pos = self.bump()?;
            let (x1, y1) = from;
            let (x2, y2) = to;
            let (cx, cy) = center;
            self.body.push_str(&format!(
                "\t.Arc {w:.6} {pos} P R {x1:.6} {y1:.6} {x2:.6} {y2:.6} {cx:.6} {cy:.6} {}\n",
                u8::from(cw)
            ));
            Ok(())
        }
        fn surface(
            &mut self,
            outline: &[(f64, f64)],
            holes: &[Vec<(f64, f64)>],
        ) -> Result<(), String> {
            let pos = self.bump()?;
            self.ops += outline.len() + holes.iter().map(Vec::len).sum::<usize>();
            if self.ops > MAX_FEATURES {
                return Err("odb export exceeds the feature limit".into());
            }
            self.body
                .push_str(&format!("\t.Surface {pos} P R\n\t\t.BeginPolygon\n"));
            fn ring(body: &mut String, begin: &str, end: &str, points: &[(f64, f64)]) {
                body.push_str(&format!("\t\t.{begin}\n"));
                for (x, y) in points.iter().chain(points.first()) {
                    body.push_str(&format!("\t\t\t{x:.6} {y:.6}\n"));
                }
                body.push_str(&format!("\t\t.{end}\n"));
            }
            ring(&mut self.body, "BeginPolygon", "EndPolygon", outline);
            for hole in holes {
                ring(&mut self.body, "BeginHole", "EndHole", hole);
            }
            self.body.push_str("\tEndSurface\n");
            Ok(())
        }
    }
    let mut top = Feat::new();
    let mut bottom = Feat::new();
    let mut misc = Feat::new();
    for z in &p.board.zones {
        if !z.filled {
            continue;
        }
        let back = z.layer == pcbcraft_doc::Layer::Back;
        let file = if back { &mut bottom } else { &mut top };
        let outline: Vec<(f64, f64)> = z.outline.iter().map(|q| (mirror(q.x, back), q.y)).collect();
        let holes: Vec<Vec<(f64, f64)>> = z
            .cutouts
            .iter()
            .map(|ring| ring.iter().map(|q| (mirror(q.x, back), q.y)).collect())
            .collect();
        file.surface(&outline, &holes)?;
    }
    for t in &p.board.tracks {
        let back = t.layer == pcbcraft_doc::Layer::Back;
        let file = if back { &mut bottom } else { &mut top };
        let (x1, y1) = (mirror(t.start.x, back), t.start.y);
        let (x2, y2) = (mirror(t.end.x, back), t.end.y);
        if let Some(arc) = t.curve() {
            let cw = if back {
                arc.sweep > 0.0
            } else {
                arc.sweep < 0.0
            };
            file.arc(
                t.width,
                (x1, y1),
                (x2, y2),
                (mirror(arc.center.x, back), arc.center.y),
                cw,
            )?;
        } else {
            file.line(t.width, x1, y1, x2, y2)?;
        }
    }
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    if p.board.footprints.len() > MAX_COMPONENTS {
        return Err("odb export exceeds the placed-component limit".into());
    }
    let mut comps_top = String::new();
    let mut comps_bottom = String::new();
    let mut placed = 0usize;
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let back = f.pads.iter().any(|pad| {
            pad.geometry.as_ref().is_some_and(|g| {
                g.pad_type == "smd"
                    && !g.layers.is_empty()
                    && !g.layers.iter().any(|l| l.starts_with("F."))
            })
        });
        placed += 1;
        let side = if back {
            &mut comps_bottom
        } else {
            &mut comps_top
        };
        side.push_str(&format!(
            "\t.Component \"{}\" {placed} P R {:.6} {:.6} {:.3} N\n",
            c.reference,
            mirror(f.position.x, back),
            f.position.y,
            spin(f.rotation, back)
        ));
        let package = match &c.library_footprint {
            Some(footprint) => footprint.definition.name.clone(),
            None => c.kind.clone(),
        };
        side.push_str(&format!("\t\t.Package \"{package}\"\n"));
        for (i, pad) in f.pads.iter().enumerate() {
            side.push_str(&format!(
                "\t\t.Feature \"{}\" {} P R {:.6} {:.6} 0.000 A\n\t\tEndFeature\n",
                pad.number,
                i + 1,
                if back { -pad.offset.x } else { pad.offset.x },
                pad.offset.y
            ));
        }
        side.push_str("\tEndComponent\n");
        for pad in &f.pads {
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            let through = pad
                .geometry
                .as_ref()
                .is_some_and(|g| g.drill.is_some() || g.pad_type.starts_with("thru_hole"));
            let on_back = pad.geometry.as_ref().is_some_and(|g| {
                !g.layers.is_empty() && !g.layers.iter().any(|l| l.starts_with("F."))
            });
            let name = match &pad.geometry {
                Some(g) => {
                    let w = um(g.size.x);
                    let h = um(g.size.y);
                    match g.shape.as_str() {
                        "circle" => symbol(format!("C{w}"), g.size.x, g.size.y, true)?,
                        "oval" => symbol(format!("O{w}x{h}"), g.size.x, g.size.y, true)?,
                        "rect" | "roundrect" => {
                            symbol(format!("S{w}x{h}"), g.size.x, g.size.y, false)?
                        }
                        _ => symbol(
                            format!("C{}", um(pad.diameter)),
                            pad.diameter,
                            pad.diameter,
                            true,
                        )?,
                    }
                }
                None => symbol(
                    format!("C{}", um(pad.diameter)),
                    pad.diameter,
                    pad.diameter,
                    true,
                )?,
            };
            if through || !on_back {
                top.symbol(&name, world.x, world.y, spin(f.rotation, false))?;
            }
            if through || on_back {
                bottom.symbol(
                    &name,
                    mirror(world.x, true),
                    world.y,
                    spin(f.rotation, true),
                )?;
            }
        }
    }
    for v in &p.board.vias {
        let name = symbol(format!("C{}", um(v.diameter)), v.diameter, v.diameter, true)?;
        top.symbol(&name, v.position.x, v.position.y, 0.0)?;
        bottom.symbol(&name, mirror(v.position.x, true), v.position.y, 0.0)?;
    }
    let edge = board_edge_points(p);
    if let Some(first) = edge.last().copied() {
        let mut prev = first;
        for point in &edge {
            misc.line(0.1, prev.0, prev.1, point.0, point.1)?;
            prev = *point;
        }
    }
    for c in &p.board.cutouts {
        for i in 0..32 {
            let a1 = std::f64::consts::TAU * i as f64 / 32.0;
            let a2 = std::f64::consts::TAU * (i + 1) as f64 / 32.0;
            misc.line(
                0.1,
                c.center.x + c.radius * a1.cos(),
                c.center.y + c.radius * a1.sin(),
                c.center.x + c.radius * a2.cos(),
                c.center.y + c.radius * a2.sin(),
            )?;
        }
    }
    let matrix = format!(
        ".Extract 0\n.Header\n\t.Creator PCBCraft\n\t.Title {job}\n.EndHeader\n.Matrix\n\t.\
         Job {job} 0 1\n\t.Step STEP1 0\n\t\t.Layer L1 GTL 1 0 0 Polarity\n\t\t.EndLayer\n\t\t.\
         Layer L2 GBL 2 0 0 Polarity\n\t\t.EndLayer\n\t\t.Layer MISC GML 3 0 0 Polarity\n\t\t.\
         EndLayer\n\t.EndStep\n.EndMatrix\n"
    );
    let mut symbols_text = String::from("UnitsType=MM\n");
    for (name, points) in &symbols {
        symbols_text.push_str(&format!(".Symbol {name}\n\t.Boundary\n"));
        for (x, y) in points.iter().chain(points.first()) {
            symbols_text.push_str(&format!("\t\t{x:.6} {y:.6}\n"));
        }
        symbols_text.push_str("\t.EndBoundary\n.EndSymbol\n");
    }
    let features = |comment: &str, file: &Feat| -> String {
        format!(
            "UnitsType=MM\n.Features\n\t.Comment {comment}\n{}\tEndFeatures\n",
            file.body
        )
    };
    let components =
        |body: &str| -> String { format!("UnitsType=MM\n.Components\n{body}\tEndComponents\n") };
    let entries = vec![
        (format!("{job}/matrix/matrix"), matrix.into_bytes()),
        (format!("{job}/symbols/symbols"), symbols_text.into_bytes()),
        (
            format!("{job}/features/top"),
            features("copper on the front side", &top).into_bytes(),
        ),
        (
            format!("{job}/features/bottom"),
            features("copper on the back side", &bottom).into_bytes(),
        ),
        (
            format!("{job}/features/misc"),
            features("board outline and cutouts", &misc).into_bytes(),
        ),
        (
            format!("{job}/components/top"),
            components(&comps_top).into_bytes(),
        ),
        (
            format!("{job}/components/bottom"),
            components(&comps_bottom).into_bytes(),
        ),
    ];
    zip_bytes(&entries)
}

/// A minimal AP214 (STEP) writer. The board outline and every placed pad
/// outline become closed prisms (two caps and one planar side face per edge)
/// in one shape representation. Coordinates are millimetres on the board
/// plane; Z spans the board thickness. Written by hand, no dependency.
const STEP_MAX_ENTITIES: u32 = 60_000;
/// Polygon rings are thinned to at most this many corners so rounded pads
/// stay affordable in one file.
const STEP_MAX_RING: usize = 32;

type Coords = (f64, f64, f64);

struct StepDoc {
    lines: Vec<String>,
    next: u32,
}

impl StepDoc {
    fn new() -> Self {
        StepDoc {
            lines: Vec::new(),
            next: 1,
        }
    }
    fn add(&mut self, body: &str) -> Result<u32, String> {
        if self.next > STEP_MAX_ENTITIES {
            return Err("the board is too complex for one STEP file".to_string());
        }
        let id = self.next;
        self.next += 1;
        self.lines.push(format!("#{id} = {body};"));
        Ok(id)
    }
    fn set(&mut self, id: u32, body: &str) {
        self.lines[id as usize - 1] = format!("#{id} = {body};");
    }
    fn n(v: f64) -> String {
        format!("{:.4}", v + 0.0)
    }
    fn point(&mut self, c: Coords) -> Result<u32, String> {
        self.add(&format!(
            "CARTESIAN_POINT('',({},{},{}))",
            Self::n(c.0),
            Self::n(c.1),
            Self::n(c.2)
        ))
    }
    fn direction(&mut self, d: Coords) -> Result<u32, String> {
        self.add(&format!(
            "DIRECTION('',({},{},{}))",
            Self::n(d.0),
            Self::n(d.1),
            Self::n(d.2)
        ))
    }
    fn vector(&mut self, dir: u32, length: f64) -> Result<u32, String> {
        self.add(&format!(
            "VECTOR('',#{dir},{})",
            Self::n(length.max(0.0001))
        ))
    }
    /// One planar face bounded by a closed loop over the given corners, with
    /// the plane normal and an in-plane reference direction.
    fn face(
        &mut self,
        corners: &[Coords],
        normal: Coords,
        reference: Coords,
    ) -> Result<u32, String> {
        let count = corners.len();
        let mut points = Vec::new();
        for c in corners {
            points.push(self.point(*c)?);
        }
        let mut vertices = Vec::new();
        for p in &points {
            vertices.push(self.add(&format!("VERTEX_POINT('',#{p})"))?);
        }
        let mut oriented = Vec::new();
        for index in 0..count {
            let next = (index + 1) % count;
            let a = corners[index];
            let b = corners[next];
            let delta = (b.0 - a.0, b.1 - a.1, b.2 - a.2);
            let length = (delta.0 * delta.0 + delta.1 * delta.1 + delta.2 * delta.2).sqrt();
            let dir = self.direction(delta)?;
            let vec = self.vector(dir, length)?;
            let line = self.add(&format!("LINE('',#{},#{vec})", points[index]))?;
            let edge = self.add(&format!(
                "EDGE_LINE('',#{line},#{},#{})",
                vertices[index], vertices[next]
            ))?;
            oriented.push(self.add(&format!("ORIENTED_EDGE('',*,*,#{edge},.T.)"))?);
        }
        let listed: Vec<String> = oriented.iter().map(|e| format!("#{e}")).collect();
        let loop_id = self.add(&format!("EDGE_LOOP('',({}))", listed.join(",")))?;
        let bound = self.add(&format!("FACE_OUTER_BOUND('',#{loop_id},.T.)"))?;
        let axis = self.direction(normal)?;
        let ref_dir = self.direction(reference)?;
        let placement = self.add(&format!(
            "AXIS2_PLACEMENT_3D('',#{},#{axis},#{ref_dir})",
            points[0]
        ))?;
        let plane = self.add(&format!("PLANE('',#{placement})"))?;
        self.add(&format!("ADVANCED_FACE((),(#{bound}),#{plane},.T.)"))
    }
    /// Extrude a closed polygon from z=0 to z=depth, returning the advanced
    /// face ids that close the prism.
    fn prism(&mut self, ring: &[Point], depth: f64) -> Result<Vec<u32>, String> {
        let ring = thin_ring(ring);
        let count = ring.len();
        if count < 3 {
            return Ok(Vec::new());
        }
        let mut faces = Vec::new();
        for index in 0..count {
            let next = (index + 1) % count;
            let a = ring[index];
            let b = ring[next];
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let length = (dx * dx + dy * dy).sqrt();
            if length < 1e-9 {
                continue;
            }
            let corners = [
                (a.x, a.y, 0.0),
                (b.x, b.y, 0.0),
                (b.x, b.y, depth),
                (a.x, a.y, depth),
            ];
            faces.push(self.face(
                &corners,
                (dy / length, -dx / length, 0.0),
                (dx / length, dy / length, 0.0),
            )?);
        }
        let bottom: Vec<Coords> = ring.iter().map(|p| (p.x, p.y, 0.0)).collect();
        let mut top: Vec<Coords> = ring.iter().map(|p| (p.x, p.y, depth)).collect();
        top.reverse();
        faces.push(self.face(&bottom, (0.0, 0.0, -1.0), (1.0, 0.0, 0.0))?);
        faces.push(self.face(&top, (0.0, 0.0, 1.0), (1.0, 0.0, 0.0))?);
        Ok(faces)
    }
}

/// Drop the duplicated closing vertex and thin long rings so rounded pads
/// stay affordable.
fn thin_ring(points: &[Point]) -> Vec<Point> {
    let mut ring: Vec<Point> = points.to_vec();
    if let (Some(first), Some(last)) = (ring.first().copied(), ring.last().copied())
        && ring.len() > 1
        && (first.x - last.x).abs() < 1e-9
        && (first.y - last.y).abs() < 1e-9
    {
        ring.pop();
    }
    while ring.len() > STEP_MAX_RING {
        ring = ring.into_iter().step_by(2).collect();
    }
    ring
}

/// Export the board as an AP214 STEP file: the outline and every placed pad
/// as a solid prism, in millimetres.
pub fn step_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    let depth = p.board.thickness;
    let outline: Vec<Point> = match &p.board.outline {
        Some(o) if o.len() >= 3 => o.clone(),
        _ => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
        ],
    };
    let mut doc = StepDoc::new();
    doc.add("PRODUCT('BOARD','pcbcraft board','',('Mechanical Design'))")?;
    doc.add("PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE('','',#1,.NOT_KNOWN.)")?;
    doc.add("PRODUCT_CONTEXT('',#1,'mechanical')")?;
    doc.add("PRODUCT_DEFINITION('design','',#2,#3)")?;
    doc.add("PRODUCT_DEFINITION_SHAPE('','',#4)")?;
    let shape = doc.add("SHAPE_REPRESENTATION('board',(#14,#15),#7)")?;
    doc.add("(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#16))GLOBAL_UNIT_ASSIGNED_CONTEXT((#8,#9,#10))REPRESENTATION_CONTEXT('','',''))")?;
    doc.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.)LENGTH_MEASURE())")?;
    doc.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))")?;
    doc.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_MEASURE())")?;
    doc.add("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.001),#8,'distance_accuracy_value','confusion accuracy')")?;
    let origin = doc.point((0.0, 0.0, 0.0))?;
    let axis = doc.direction((0.0, 0.0, 1.0))?;
    let reference = doc.direction((1.0, 0.0, 0.0))?;
    let base = doc.add(&format!(
        "AXIS2_PLACEMENT_3D('',#{origin},#{axis},#{reference})"
    ))?;
    doc.add("SHAPE_DEFINITION_REPRESENTATION(#5,#6)")?;
    let mut items = vec![base];
    items.extend(doc.prism(&outline, depth)?);
    for footprint in &p.board.footprints {
        for pad in &footprint.pads {
            let ring: Vec<Point> = pad
                .outline_points()
                .into_iter()
                .map(|pos| footprint.position.offset(pos.rotate(-footprint.rotation)))
                .collect();
            items.extend(doc.prism(&ring, depth)?);
        }
    }
    let listed: Vec<String> = items.iter().map(|i| format!("#{i}")).collect();
    doc.set(
        shape,
        &format!("SHAPE_REPRESENTATION('board',({}),#7)", listed.join(",")),
    );
    let mut out = String::new();
    out.push_str("ISO-10303-21;\nHEADER;\n");
    out.push_str("FILE_DESCRIPTION(('pcbcraft board export'),'2;1');\n");
    out.push_str("FILE_NAME('board.step','','','(pcbcraft)','pcbcraft','pcbcraft','');\n");
    out.push_str("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\n");
    out.push_str("ENDSEC;\nDATA;\n");
    for line in &doc.lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(out)
}
/// Board as a VRML 2.0 scene for mechanical viewers: one solid prism for the
/// board outline (the rectangle when no free-form outline is recorded) and one
/// prism per placed pad, each an indexed face set with a bottom cap, four side
/// faces per ring segment and a top cap.
pub fn vrml_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PRISMS: usize = 20_000;
    const MAX_FACES: usize = 200_000;
    let depth = p.board.thickness;
    let outline: Vec<Point> = match &p.board.outline {
        Some(o) if o.len() >= 3 => o.clone(),
        _ => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
        ],
    };
    let mut rings = vec![outline];
    for footprint in &p.board.footprints {
        for pad in &footprint.pads {
            rings.push(
                pad.outline_points()
                    .into_iter()
                    .map(|pos| footprint.position.offset(pos.rotate(-footprint.rotation)))
                    .collect(),
            );
        }
    }
    if rings.len() > MAX_PRISMS {
        return Err("mechanical scene exceeds the solid limit".into());
    }
    let mut out = String::new();
    out.push_str("#VRML V2.0 utf8\n# pcbcraft board\n");
    let mut faces = 0usize;
    for ring in &rings {
        let base = 0usize;
        let ring = thin_ring(ring);
        let count = ring.len();
        if count < 3 {
            continue;
        }
        faces += 4 * count;
        if faces > MAX_FACES {
            return Err("mechanical scene exceeds the face limit".into());
        }
        let mut coords = String::new();
        for index in 0..count {
            let next = (index + 1) % count;
            let a = ring[index];
            let b = ring[next];
            coords.push_str(&format!(
                "{} {} {},{} {} {},{} {} {},{} {} {},",
                f64s(a.x),
                f64s(a.y),
                "0",
                f64s(b.x),
                f64s(b.y),
                "0",
                f64s(b.x),
                f64s(b.y),
                f64s(depth),
                f64s(a.x),
                f64s(a.y),
                f64s(depth),
            ));
        }

        let mut idx = Vec::new();
        for quad in 0..count {
            let q = base + quad * 4;
            idx.push(format!("{} {} {} {} -1", q, q + 1, q + 2, q + 3));
        }
        let cap: Vec<usize> = (0..count).map(|index| base + index * 4).collect();
        idx.push(format!(
            "{} -1",
            cap.iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ));
        let topcap: Vec<usize> = (0..count).rev().map(|index| base + index * 4 + 2).collect();
        idx.push(format!(
            "{} -1",
            topcap
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ));
        out.push_str(&format!(
            "Shape {{\n  appearance Appearance {{ material Material {{ diffuseColor 0.13 0.55 0.13 }} }}\n  geometry IndexedFaceSet {{\n    coord Coordinate {{ point [\n      {}\n    ] }}\n    coordIndex [\n      {}\n    ] solid\n  }}\n}}\n",
            coords,
            idx.join("\n      "),
        ));
    }
    if out.matches("IndexedFaceSet").count() == 0 {
        return Err("board has no solids to export".into());
    }
    Ok(out)
}
fn f64s(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { "0".into() } else { format!("{r}") }
}

pub fn kicad_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    let b = &p.board;
    let mut s = format!(
        "(kicad_pcb (version 20240108) (generator \"pcbcraft\")\n (general (thickness {})) (paper \"A4\")\n (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (34 \"B.Paste\" user) (35 \"F.Paste\" user) (36 \"B.SilkS\" user \"b.silkscreen\") (37 \"F.SilkS\" user \"f.silkscreen\") (38 \"B.Mask\" user) (39 \"F.Mask\" user) (44 \"Edge.Cuts\" user) (46 \"B.CrtYd\" user) (47 \"F.CrtYd\" user) (48 \"B.Fab\" user) (49 \"F.Fab\" user))\n (setup (pad_to_mask_clearance 0))\n (net 0 \"\")\n",
        b.thickness
    );
    for (i, n) in p.nets.iter().enumerate() {
        s.push_str(&format!(" (net {} {})\n", i + 1, quote(&n.name)));
    }
    let ordinal = |net: Option<u64>| {
        p.nets
            .iter()
            .position(|n| Some(n.id) == net)
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    for f in &b.footprints {
        let Some(c) = p.component(f.component) else {
            return Err("missing component".into());
        };
        let name = c
            .library_footprint
            .as_ref()
            .map(|l| {
                if l.library == "project" {
                    format!("pcbcraft_{}", l.definition.name)
                } else {
                    format!("{}:{}", l.library, l.definition.name)
                }
            })
            .unwrap_or_else(|| format!("pcbcraft_{}", c.kind));
        let attr = if f.pads.iter().any(|p| {
            p.geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "thru_hole")
        }) {
            "through_hole"
        } else {
            "smd"
        };
        let (min, max) = footprint_bounds(p, f);
        let label_x = (min.x + max.x) / 2.0;
        let reference_y = min.y - 1.2;
        let value_y = max.y + 1.2;
        s.push_str(&format!(" (footprint {} (layer \"F.Cu\") (uuid {}) (at {} {} {})\n (property \"Reference\" {} (at {label_x} {reference_y} {}) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15))))\n (property \"Value\" {} (at {label_x} {value_y} {}) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15))))\n (attr {attr})\n",quote(&name),uuid(f.id,1),f.position.x,f.position.y,f.rotation,quote(&c.reference),f.rotation,quote(&c.value),f.rotation));
        if let Some(library) = &c.library_footprint {
            if let Some(m) = &library.definition.model {
                s.push_str(&format!(
                    " (model {} (offset (xyz {} {} {})) (scale (xyz {} {} {})) (rotate (xyz {} {} {})))\n",
                    quote(&m.path),
                    m.offset[0],
                    m.offset[1],
                    m.offset[2],
                    m.scale[0],
                    m.scale[1],
                    m.scale[2],
                    m.rotate[0],
                    m.rotate[1],
                    m.rotate[2]
                ));
            }
            if let Some(r) = &library.definition.rules {
                if let Some(v) = r.clearance {
                    s.push_str(&format!(" (clearance {v})\n"));
                }
                if let Some(v) = r.mask_margin {
                    s.push_str(&format!(" (solder_mask_margin {v})\n"));
                }
                if let Some(v) = r.paste_margin {
                    s.push_str(&format!(" (solder_paste_margin {v})\n"));
                }
            }
            for graphic in &library.definition.graphics {
                s.push_str(&kicad_graphic(graphic, f.rotation)?);
            }
        } else {
            s.push_str(" (fp_rect (start -3.4 -1.4) (end 3.4 1.4) (stroke (width 0.12) (type solid)) (fill none) (layer \"F.SilkS\"))\n");
        }
        for (i, pad) in f.pads.iter().enumerate() {
            let net = pad.net.map(|n| p.net_name(n)).unwrap_or("");
            let (pad_type, shape, size, rotation, layers, ratio, drill) =
                if let Some(g) = &pad.geometry {
                    if !matches!(g.pad_type.as_str(), "smd" | "thru_hole")
                        || matches!(g.shape_kind(), pcbcraft_library::PadShape::Unsupported)
                        || (g.pad_type == "smd" && g.drill.is_some())
                        || (g.pad_type == "thru_hole" && g.drill.is_none())
                    {
                        return Err(
                        "KiCad export supports standard SMD or plated through-hole pad geometry"
                            .into(),
                    );
                    }
                    (
                        g.pad_type.as_str(),
                        match g.shape_kind() {
                            pcbcraft_library::PadShape::Chamfered(_) => "roundrect",
                            _ => g.shape.as_str(),
                        },
                        g.size,
                        f.rotation + g.rotation,
                        g.layers
                            .iter()
                            .map(|l| quote(l))
                            .collect::<Vec<_>>()
                            .join(" "),
                        kicad_pad_shape_detail(g),
                        g.drill
                            .as_ref()
                            .map(|d| {
                                let shape = if (d.size.x - d.size.y).abs() < 1e-9 {
                                    d.size.x.to_string()
                                } else {
                                    format!("oval {} {}", d.size.x, d.size.y)
                                };
                                format!(" (drill {shape} (offset {} {}))", d.offset.x, d.offset.y)
                            })
                            .unwrap_or_default(),
                    )
                } else {
                    (
                        "smd",
                        "circle",
                        Point::new(pad.diameter, pad.diameter),
                        f.rotation,
                        "\"F.Cu\" \"F.Mask\"".into(),
                        String::new(),
                        String::new(),
                    )
                };
            s.push_str(&format!(" (pad {} {pad_type} {shape} (at {} {} {rotation}) (size {} {}) (layers {layers}){ratio}{drill} (net {} {}) (uuid {}))\n",quote(&pad.number),pad.offset.x,pad.offset.y,size.x,size.y,ordinal(pad.net),quote(net),uuid(f.id,i+2)));
        }
        s.push_str(" )\n");
    }
    for t in &b.tracks {
        match t.arc {
            Some(mid) => s.push_str(&format!(
                " (arc (start {} {}) (mid {} {}) (end {} {}) (width {}) (layer {}) (net {}) (uuid {}))\n",
                t.start.x,
                t.start.y,
                mid.x,
                mid.y,
                t.end.x,
                t.end.y,
                t.width,
                quote(&p.layer_name(t.layer)),
                ordinal(Some(t.net)),
                uuid(t.id, 1)
            )),
            None => s.push_str(&format!(
                " (segment (start {} {}) (end {} {}) (width {}) (layer {}) (net {}) (uuid {}))\n",
                t.start.x,
                t.start.y,
                t.end.x,
                t.end.y,
                t.width,
                quote(&p.layer_name(t.layer)),
                ordinal(Some(t.net)),
                uuid(t.id, 1)
            )),
        }
    }
    match &b.outline {
        Some(o) => {
            s.push_str(" (gr_poly (pts");
            for pt in o {
                s.push_str(&format!(" (start {} {})", pt.x, pt.y));
            }
            s.push_str(
                ") (stroke (width 0.05) (type solid)) (fill none) (layer \"Edge.Cuts\"))\n",
            );
        }
        None => s.push_str(&format!(" (gr_rect (start 0 0) (end {} {}) (stroke (width 0.05) (type solid)) (fill none) (layer \"Edge.Cuts\"))\n",b.width,b.height)),
    }
    for c in &b.cutouts {
        s.push_str(&format!(" (gr_circle (center {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (fill none) (layer \"Edge.Cuts\") (uuid {}))\n",c.center.x,c.center.y,c.center.x+c.radius,c.center.y,uuid(c.id,1)));
    }
    s.push_str(")\n");
    Ok(s)
}

/// Shape-specific pad expressions. A chamfered pad is a KiCad roundrect with
/// zero rounding plus chamfer corners. A custom pad is a rectangular anchor
/// inside one filled polygon primitive, so its copper is the polygon.
fn kicad_pad_shape_detail(g: &pcbcraft_library::FootprintPad) -> String {
    match g.shape_kind() {
        pcbcraft_library::PadShape::RoundRect { ratio } => format!(" (roundrect_rratio {ratio})"),
        pcbcraft_library::PadShape::Chamfered(c) => {
            let corners = [
                (c.corners.top_left, "top_left"),
                (c.corners.top_right, "top_right"),
                (c.corners.bottom_left, "bottom_left"),
                (c.corners.bottom_right, "bottom_right"),
            ]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
            .join(" ");
            format!(
                " (roundrect_rratio 0) (chamfer_ratio {}) (chamfer {corners})",
                c.ratio
            )
        }
        pcbcraft_library::PadShape::Custom { polygon } => format!(
            " (options (clearance outline) (anchor rect)) (primitives (gr_poly (pts{}) (width 0) (fill yes)))",
            polygon
                .iter()
                .map(|v| format!(" (xy {} {})", v.x, v.y))
                .collect::<String>()
        ),
        _ => String::new(),
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn preview_bounds(p: &Project, view: &str) -> (Point, Point) {
    let extend = |min: &mut Point, max: &mut Point, point: Point| {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    };
    if view == "pcb" {
        let mut min = Point::new(p.board.width, p.board.height);
        let mut max = Point::default();
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            for point in g.points.iter().flat_map(|pt| {
                g.outline()
                    .unwrap_or_else(|| vec![*pt])
                    .into_iter()
                    .chain(std::iter::once(*pt))
            }) {
                extend(&mut min, &mut max, point);
            }
        }
        for t in &p.board.texts {
            if t.position.x.is_finite() && t.position.y.is_finite() {
                extend(&mut min, &mut max, t.position);
            }
        }
        if min.x.is_finite() && max.x.is_finite() {
            return (
                Point::new(min.x.min(0.0), min.y.min(0.0)),
                Point::new(max.x.max(p.board.width), max.y.max(p.board.height)),
            );
        }
        return (Point::default(), Point::new(p.board.width, p.board.height));
    }
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for c in p.components.iter().flat_map(|c| c.schematic_views()) {
        for point in c.drawing_points() {
            let point = c.position.offset(point.rotate(-c.rotation));
            extend(&mut min, &mut max, point);
        }
    }
    for w in &p.drawn_wires {
        for point in &w.points {
            extend(&mut min, &mut max, *point);
        }
    }
    for g in p.graphics.iter().filter(|g| g.layer == "schematic") {
        for point in &g.points {
            extend(&mut min, &mut max, *point);
        }
        if let Some(outline) = g.outline() {
            for point in outline {
                extend(&mut min, &mut max, point);
            }
        }
    }
    if !min.x.is_finite() || !min.y.is_finite() || !max.x.is_finite() || !max.y.is_finite() {
        return (Point::default(), Point::new(60.0, 40.0));
    }
    (min, max)
}
/// Edge.Cuts corner or vertex list for fabrication output: the traced
/// outline when the board has one, the plain rectangle otherwise.
fn board_edge_points(p: &Project) -> Vec<(f64, f64)> {
    match &p.board.outline {
        Some(o) => o.iter().map(|pt| (pt.x, pt.y)).collect(),
        None => vec![
            (0.0, 0.0),
            (p.board.width, 0.0),
            (p.board.width, p.board.height),
            (0.0, p.board.height),
        ],
    }
}

#[derive(Clone, Copy)]
struct ProjectDrillHit {
    d: u64,
    span: u64,
    x: f64,
    y: f64,
    ux: f64,
    uy: f64,
    plated: bool,
}

fn project_drill_hits(p: &Project) -> Result<Vec<ProjectDrillHit>, String> {
    const MAX_HITS: usize = 50_000;
    let mut hits = Vec::new();
    let push = |hit: ProjectDrillHit, hits: &mut Vec<ProjectDrillHit>| {
        hits.push(hit);
    };
    for v in &p.board.vias {
        if v.drill > 0.0 {
            push(
                ProjectDrillHit {
                    d: (v.drill * 1_000.0).round() as u64,
                    span: 0,
                    x: v.position.x,
                    y: v.position.y,
                    ux: 0.0,
                    uy: 0.0,
                    plated: true,
                },
                &mut hits,
            );
        }
    }
    for h in &p.board.holes {
        let d = (h.diameter * 1_000.0).round() as u64;
        let span = if h.is_slot() {
            (h.start().distance(h.finish()) * 1_000.0).round() as u64
        } else {
            0
        };
        let (x, y, ux, uy) = if h.is_slot() {
            let center = h.center();
            let half = Point::new(
                (h.finish().x - h.start().x) / 2.0,
                (h.finish().y - h.start().y) / 2.0,
            );
            (center.x, center.y, half.x, half.y)
        } else {
            (h.position.x, h.position.y, 0.0, 0.0)
        };
        push(
            ProjectDrillHit {
                d,
                span,
                x,
                y,
                ux,
                uy,
                plated: false,
            },
            &mut hits,
        );
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let Some(g) = pad.geometry.as_ref() else {
                continue;
            };
            let Some(drill) = g.drill.as_ref() else {
                continue;
            };
            let (wide, narrow) = (
                drill.size.x.max(drill.size.y),
                drill.size.x.min(drill.size.y),
            );
            let d = (narrow * 1_000.0).round() as u64;
            if d == 0 {
                continue;
            }
            let span = ((wide - narrow) * 1_000.0).round() as u64;
            let local = pad.offset.offset(drill.offset.rotate(-g.rotation));
            let world = f.position.offset(local.rotate(-f.rotation));
            let (ux, uy) = if span == 0 {
                (0.0, 0.0)
            } else {
                let axis = if drill.size.x >= drill.size.y {
                    Point::new((wide - narrow) / 2.0, 0.0)
                } else {
                    Point::new(0.0, (wide - narrow) / 2.0)
                };
                let dir = axis.rotate(-(g.rotation + f.rotation));
                (dir.x, dir.y)
            };
            push(
                ProjectDrillHit {
                    d,
                    span,
                    x: world.x,
                    y: world.y,
                    ux,
                    uy,
                    plated: g.pad_type == "thru_hole",
                },
                &mut hits,
            );
        }
    }
    if hits.len() > MAX_HITS {
        return Err("gerber export exceeds the drill hit limit".into());
    }
    Ok(hits)
}

fn excellon_drill_file(
    hits: &[ProjectDrillHit],
    origin: Point,
    inch: bool,
    scale: f64,
    plated_filter: Option<bool>,
) -> String {
    type DrillHit = (f64, f64, f64, f64);
    let mut drills: BTreeMap<(u64, u64), Vec<DrillHit>> = BTreeMap::new();
    for hit in hits {
        if plated_filter.is_some_and(|plated| hit.plated != plated) {
            continue;
        }
        drills
            .entry((hit.d, hit.span))
            .or_default()
            .push((hit.x, hit.y, hit.ux, hit.uy));
    }
    let (drill_header, drill_prec) = if inch {
        ("INCH,TZ", 4)
    } else {
        ("METRIC,TZ", 3)
    };
    let mut drill = format!("M48\nFMAT,2\n{drill_header}\n");
    for (n, (d, _)) in drills.keys().enumerate() {
        drill.push_str(&format!(
            "T{}C{:.drill_prec$}\n",
            n + 1,
            *d as f64 / 1000.0 * scale
        ));
    }
    drill.push_str("%\nG90\nG05\n");
    for (n, holes) in drills.values().enumerate() {
        drill.push_str(&format!("T{}\n", n + 1));
        for (x, y, ux, uy) in holes {
            let (x, y) = (x - origin.x, y - origin.y);
            let (x, y) = (x * scale, y * scale);
            let (ux, uy) = (ux * scale, uy * scale);
            if ux == 0.0 && uy == 0.0 {
                drill.push_str(&format!("X{x:.drill_prec$}Y{y:.drill_prec$}\n"));
            } else {
                drill.push_str(&format!(
                    "X{a:.drill_prec$}Y{b:.drill_prec$}G85X{c:.drill_prec$}Y{d:.drill_prec$}\n",
                    a = x - ux,
                    b = y - uy,
                    c = x + ux,
                    d = y + uy
                ));
            }
        }
    }
    drill.push_str("M30\n");
    drill
}

/// RS-274X copper/outline layers plus an Excellon drill file, as filename to
/// text. Copper layers carry zones, tracks, via and pad copper; the outline
/// layer carries the board edge and circular cutouts; the drill file carries
/// every plated and unplated hole, round holes as single hits and oval holes
/// as G85 spans along the rotated long axis. Back-copper coordinates are
/// mirrored about x=0 as fabrication houses expect for bottom-side films.
pub fn gerber_files(p: &Project) -> Result<BTreeMap<String, String>, String> {
    p.validate()?;
    const MAX_OPS: usize = 100_000;
    const MAX_APERTURES: usize = 64;
    // An inch project gets imperial fabrication files; coordinates are still
    // stored in millimetres, so every written value is rescaled here.
    let inch = p.unit == "inch";
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    enum Ap {
        Circle(u64),
        Rect(u64, u64),
        Obround(u64, u64),
        Round(u64, u64, u64),
    }
    #[derive(Clone)]
    struct LayerFile {
        apertures: BTreeMap<Ap, u32>,
        body: String,
        arcs: String,
        ops: usize,
        scale: f64,
    }
    impl LayerFile {
        fn new(scale: f64) -> Self {
            Self {
                apertures: BTreeMap::new(),
                body: String::new(),
                arcs: String::new(),
                ops: 0,
                scale,
            }
        }
        fn code(&mut self, ap: Ap) -> Result<u32, String> {
            if let Some(code) = self.apertures.get(&ap) {
                return Ok(*code);
            }
            if self.apertures.len() >= MAX_APERTURES {
                return Err("gerber export needs more than 64 distinct apertures".into());
            }
            let code = 10 + self.apertures.len() as u32;
            self.apertures.insert(ap, code);
            Ok(code)
        }
        fn flash(&mut self, code: u32, x: f64, y: f64) -> Result<(), String> {
            self.ops += 1;
            if self.ops > MAX_OPS {
                return Err("gerber export exceeds the object limit".into());
            }
            self.body.push_str(&format!(
                "D{code}X{:.6}Y{:.6}D03*\n",
                x * self.scale,
                y * self.scale
            ));
            Ok(())
        }
        fn stroke(&mut self, code: u32, points: &[(f64, f64)], closed: bool) -> Result<(), String> {
            self.ops += points.len();
            if self.ops > MAX_OPS {
                return Err("gerber export exceeds the object limit".into());
            }
            self.body.push_str(&format!("D{code}D02*\n"));
            for (i, (x, y)) in points.iter().enumerate() {
                self.body.push_str(&format!(
                    "X{:.6}Y{:.6}D0{}*\n",
                    x * self.scale,
                    y * self.scale,
                    if i == 0 { 2 } else { 1 }
                ));
            }
            if closed && let Some((x, y)) = points.first() {
                self.body.push_str(&format!(
                    "X{:.6}Y{:.6}D01*\n",
                    x * self.scale,
                    y * self.scale
                ));
            }
            Ok(())
        }
        fn fill(&mut self, code: u32, points: &[(f64, f64)]) -> Result<(), String> {
            self.ops += points.len();
            if self.ops > MAX_OPS {
                return Err("gerber export exceeds the object limit".into());
            }
            self.body.push_str(&format!("G36*\nD{code}D02*\n"));
            for (i, (x, y)) in points.iter().enumerate() {
                self.body.push_str(&format!(
                    "X{:.6}Y{:.6}D0{}*\n",
                    x * self.scale,
                    y * self.scale,
                    if i == 0 { 2 } else { 1 }
                ));
            }
            self.body.push_str("G37*\n");
            Ok(())
        }
        fn finish(&self, mirror: bool, name: &str) -> String {
            let mirror = if mirror { "D" } else { "A" };
            let mo = if self.scale != 1.0 { "MOIN" } else { "MOMM" };
            let mut out =
                format!("G04 {name} generated by PCBCraft *\n%FSLANX*X{mirror}Y{mirror}{mo}*%\n");
            for (ap, code) in &self.apertures {
                match *ap {
                    Ap::Circle(d) => out.push_str(&format!(
                        "%ADD{code}C,{:.6}*%\n",
                        d as f64 / 1000.0 * self.scale
                    )),
                    Ap::Rect(w, h) => out.push_str(&format!(
                        "%ADD{code}R,{:.6}X{:.6}*%\n",
                        w as f64 / 1000.0 * self.scale,
                        h as f64 / 1000.0 * self.scale
                    )),
                    Ap::Obround(w, h) => out.push_str(&format!(
                        "%ADD{code}O,{:.6}X{:.6}*%\n",
                        w as f64 / 1000.0 * self.scale,
                        h as f64 / 1000.0 * self.scale
                    )),
                    Ap::Round(w, h, r) => out.push_str(&format!(
                        "%ADD{code}R2,{} R1,{:.6}X{:.6} {r} 0 0*%\n",
                        if r * 2 == h {
                            "0".to_string()
                        } else {
                            format!("{:.6}", r as f64 / 1000.0 * self.scale)
                        },
                        w as f64 / 1000.0 * self.scale,
                        h as f64 / 1000.0 * self.scale
                    )),
                }
            }
            out.push_str("%LPD*%\nG01*\n");
            out.push_str(&self.body);
            out.push_str(&self.arcs);
            out.push_str("M02*\n");
            out
        }
    }
    let mut front = LayerFile::new(scale);
    let mut back = LayerFile::new(scale);
    let mirror =
        |x: f64, y: f64, back: bool| -> (f64, f64) { if back { (-x + 0.0, y) } else { (x, y) } };
    for z in &p.board.zones {
        if !z.filled {
            continue;
        }
        let is_back = z.layer == pcbcraft_doc::Layer::Back;
        let file = if is_back { &mut back } else { &mut front };
        let code = file.code(Ap::Circle(1))?;
        let outline: Vec<(f64, f64)> = z
            .outline
            .iter()
            .chain(z.cutouts.iter().flatten())
            .map(|q| mirror(q.x, q.y, is_back))
            .collect();
        file.fill(code, &outline)?;
    }
    for t in &p.board.tracks {
        let is_back = t.layer == pcbcraft_doc::Layer::Back;
        let file = if is_back { &mut back } else { &mut front };
        let w = (t.width * 1_000.0).round() as u64;
        let code = file.code(Ap::Circle(w.max(1)))?;
        if let Some(arc) = t.curve() {
            file.ops += 1;
            if file.ops > MAX_OPS {
                return Err("gerber export exceeds the object limit".into());
            }
            let (sx, sy) = mirror(t.start.x, t.start.y, is_back);
            let (ex, ey) = mirror(t.end.x, t.end.y, is_back);
            let (cx, cy) = mirror(arc.center.x, arc.center.y, is_back);
            let clockwise = if is_back {
                arc.sweep > 0.0
            } else {
                arc.sweep < 0.0
            };
            file.body.push_str(&format!(
                "D{code}D02*\nX{:.6}Y{:.6}D02*\nX{:.6}Y{:.6}I{:.6}J{:.6}G0{}*\n",
                sx,
                sy,
                ex,
                ey,
                cx - sx,
                cy - sy,
                if clockwise { 2 } else { 3 }
            ));
        } else {
            file.stroke(
                code,
                &[
                    mirror(t.start.x, t.start.y, is_back),
                    mirror(t.end.x, t.end.y, is_back),
                ],
                false,
            )?;
        }
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let Some(g) = pad.geometry.as_ref() else {
                continue;
            };
            if g.pad_type == "unplated" {
                continue;
            }
            let world = f.pad_world_position(pad);
            let through = g.pad_type == "thru_hole" || g.drill.is_some();
            let is_back = !g.layers.is_empty() && !g.layers.iter().any(|l| l.starts_with("F."));
            let ap = match g.shape.as_str() {
                "circle" => {
                    let w = (g.size.x * 1_000.0).round() as u64;
                    Ap::Circle(w.max(1))
                }
                "rect" => {
                    let w = (g.size.x * 1_000.0).round() as u64;
                    let h = (g.size.y * 1_000.0).round() as u64;
                    Ap::Rect(w.max(1), h.max(1))
                }
                "oval" => {
                    let w = (g.size.x * 1_000.0).round() as u64;
                    let h = (g.size.y * 1_000.0).round() as u64;
                    Ap::Obround(w.max(1), h.max(1))
                }
                "roundrect" => {
                    let w = (g.size.x * 1_000.0).round() as u64;
                    let h = (g.size.y * 1_000.0).round() as u64;
                    Ap::Round(
                        w.max(1),
                        h.max(1),
                        ((w.min(h) as f64 * g.roundrect_ratio.unwrap_or(0.0))
                            .round()
                            .max(1.0)) as u64,
                    )
                }
                _ => Ap::Circle((pad.diameter.max(0.001) * 1_000.0).round() as u64),
            };
            let (front_code, back_code) = if through {
                (front.code(ap.clone())?, back.code(ap.clone())?)
            } else if is_back {
                (0, back.code(ap)?)
            } else {
                (front.code(ap)?, 0)
            };
            if front_code != 0 {
                front.flash(front_code, world.x, world.y)?;
            }
            if back_code != 0 {
                let (x, y) = mirror(world.x, world.y, true);
                back.flash(back_code, x, y)?;
            }
        }
    }
    for v in &p.board.vias {
        let d = (v.diameter * 1_000.0).round() as u64;
        let ap = Ap::Circle(d.max(1));
        let front_code = front.code(ap.clone())?;
        let back_code = back.code(ap)?;
        front.flash(front_code, v.position.x, v.position.y)?;
        let (x, y) = mirror(v.position.x, v.position.y, true);
        back.flash(back_code, x, y)?;
    }
    let mut outline = LayerFile::new(scale);
    let edge_code = outline.code(Ap::Circle(100))?;
    outline.stroke(edge_code, &board_edge_points(p), true)?;
    for c in &p.board.cutouts {
        outline.ops += 1;
        if outline.ops > MAX_OPS {
            return Err("gerber export exceeds the object limit".into());
        }
        outline.arcs.push_str(&format!(
            "X{:.6}Y{:.6}D02*\nI{:.6}J{:.6}G02*\n",
            (c.center.x - c.radius) * scale,
            c.center.y * scale,
            c.radius * scale,
            0.0
        ));
    }
    let hits = project_drill_hits(p)?;
    let origin = p.board.drill_origin;
    let drill = excellon_drill_file(&hits, origin, inch, scale, None);
    let plated_drill = excellon_drill_file(&hits, origin, inch, scale, Some(true));
    let npth_drill = excellon_drill_file(&hits, origin, inch, scale, Some(false));
    let mut map = outline.clone();
    let marker = map.code(Ap::Circle(100))?;
    for hit in &hits {
        let (x, y) = (hit.x - origin.x, hit.y - origin.y);
        if hit.ux == 0.0 && hit.uy == 0.0 {
            map.flash(marker, x, y)?;
        } else {
            map.stroke(
                marker,
                &[(x - hit.ux, y - hit.uy), (x + hit.ux, y + hit.uy)],
                false,
            )?;
        }
    }
    let mut files = BTreeMap::new();
    files.insert(
        "pcb-F.Cu.gbr".into(),
        front.finish(false, "front copper layer"),
    );
    files.insert(
        "pcb-B.Cu.gbr".into(),
        back.finish(true, "back copper layer"),
    );
    files.insert(
        "pcb-Edge.gbr".into(),
        outline.finish(false, "board outline"),
    );
    files.insert(
        "pcb-DrillMap.gbr".into(),
        map.finish(false, "drill hole map"),
    );
    files.insert("pcb-Drill.drl".into(), drill);
    files.insert("pcb-Plated.drl".into(), plated_drill);
    files.insert("pcb-NPTH.drl".into(), npth_drill);
    for (name, text) in drawn::gerber_layer_files(p)? {
        files.insert(name, text);
    }
    let extras = fab_export::gerber_files(p)?;
    for key in [
        "pcb-F.Cu.gbr",
        "pcb-B.Cu.gbr",
        "pcb-F.Mask.gbr",
        "pcb-B.Mask.gbr",
        "pcb-F.Paste.gbr",
        "pcb-B.Paste.gbr",
        "pcb-Edge.gbr",
        "pcb-DrillMap.gbr",
        "pcb-Plated.drl",
        "pcb-NPTH.drl",
        "pcb-Rout.drl",
        "README-fabrication.txt",
    ] {
        if let Some(text) = extras.get(key) {
            files.insert(key.to_string(), text.clone());
        }
    }
    if !files.contains_key("pcb-NPTH.drl") {
        files.insert("pcb-NPTH.drl".into(), npth_drill);
    }
    Ok(files)
}

/// Gerber drill-map layer with one marker per tool and a legend table.
pub fn drill_map_plot(p: &Project) -> Result<String, String> {
    fab_export::drill_map_plot(p)
}

/// Plain-text fabrication readme for the gerber bundle.
pub fn fab_readme_text(p: &Project, file_names: &[String]) -> Result<String, String> {
    fab_export::fab_readme_text(p, file_names)
}

pub use fab_export::via_mask_opening;
/// Bill of materials as CSV: identical parts collapsed into one row with a
/// sorted, quoted reference list. Requested property keys become extra
/// columns; with no request, the columns the project itself defines are
/// printed under their labels.
pub fn bom_csv(p: &Project, fields: &[String], variant: Option<&str>) -> Result<String, String> {
    p.validate()?;
    if fields.len() > 16 {
        return Err("a bill of materials lists at most 16 properties".into());
    }
    for key in fields {
        if key.is_empty() || key.len() > 64 {
            return Err("property names must be 1 to 64 bytes".into());
        }
    }
    let variant = match variant.map(str::trim) {
        None | Some("") => None,
        Some(name) => Some(
            p.variants
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| format!("no variant named {name}"))?,
        ),
    };
    // Columns come from the caller's list, or from the columns the project
    // itself defines when the caller asks for none.
    let columns: Vec<(String, String)> = if fields.is_empty() {
        p.bom_fields
            .iter()
            .map(|f| {
                (
                    f.key.clone(),
                    if f.label.is_empty() {
                        f.key.clone()
                    } else {
                        f.label.clone()
                    },
                )
            })
            .collect()
    } else {
        fields
            .iter()
            .map(|key| (key.clone(), key.clone()))
            .collect()
    };
    let mut groups: BTreeMap<(String, String, String, Vec<String>), Vec<String>> = BTreeMap::new();
    for c in &p.components {
        if variant.is_some_and(|v| v.dnp.contains(&c.id)) {
            continue;
        }
        let footprint = match variant
            .and_then(|v| v.footprints.iter().find(|o| o.component == c.id))
        {
            Some(swap) => swap.footprint.clone(),
            None => match &c.library_footprint {
                Some(footprint) => format!("{}:{}", footprint.library, footprint.definition.name),
                None => "starter pads".into(),
            },
        };
        let values: Vec<String> = columns
            .iter()
            .map(|(key, _)| c.field(key).unwrap_or("").to_string())
            .collect();
        groups
            .entry((c.value.clone(), c.kind.clone(), footprint, values))
            .or_default()
            .push(c.reference.clone());
    }
    let mut out = String::from("quantity,references,value,kind,footprint");
    for (_, header) in &columns {
        out.push(',');
        out.push_str(&csv_cell(header));
    }
    out.push('\n');
    for ((value, kind, footprint, values), mut references) in groups {
        references.sort();
        let joined = references.join(",");
        out.push_str(&references.len().to_string());
        for cell in [joined, value, kind, footprint] {
            out.push(',');
            out.push_str(&csv_cell(&cell));
        }
        for value in values {
            out.push(',');
            out.push_str(&csv_cell(&value));
        }
        out.push('\n');
    }
    Ok(out)
}
/// Quote a CSV cell only when it needs it: commas, quotes or newlines.
fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}
/// Plain-text netlist: one line per connected net, listing the component
/// pins it carries as `REFERENCE.PIN`, sorted by net name and reference.
fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Write a single-file IPC-2581 style assembly exchange document: board outline,
/// nets with their pins, placed components, drill holes and copper by layer,
/// all as XML. Values are millimetres.
pub fn ipc2581_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_ELEMENTS: usize = 50_000;
    let mut elements = 0usize;
    let mut count = |n: usize| -> Result<(), String> {
        elements += n;
        if elements > MAX_ELEMENTS {
            return Err("IPC-2581 export exceeds the element limit".into());
        }
        Ok(())
    };
    let nets: BTreeMap<u64, String> = p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
    let mut net_pins: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for c in &p.components {
        for pin in &c.pins {
            if let Some(net) = pin.net {
                let name = nets
                    .get(&net)
                    .ok_or("pin references an unknown net")?
                    .as_str();
                net_pins
                    .entry(name)
                    .or_default()
                    .push(format!("{}.{}", c.reference, pin.number));
            }
        }
    }
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<EDA3D version=\"IPC-2581\" revision=\"A\" units=\"MM\">\n<BOARD_DATA width=\"{:.3}\" height=\"{:.3}\">\n",
        p.board.width, p.board.height
    ));
    count(2)?;
    out.push_str("<CONTOUR>\n");
    count(1)?;
    let outline: Vec<Point> = match &p.board.outline {
        Some(o) if o.len() >= 3 => o.clone(),
        _ => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
            Point::new(0.0, 0.0),
        ],
    };
    for point in &outline {
        out.push_str(&format!(
            "<VERTEX x=\"{:.3}\" y=\"{:.3}\"/>\n",
            point.x, point.y
        ));
        count(1)?;
    }
    out.push_str("</CONTOUR>\n");
    for (name, pins) in &net_pins {
        out.push_str(&format!("<NET name=\"{}\">\n", xml_escape(name)));
        count(1)?;
        for pin in pins {
            out.push_str(&format!("<PIN id=\"{}\"/>\n", xml_escape(pin)));
            count(1)?;
        }
        out.push_str("</NET>\n");
    }
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let back = f.pads.iter().any(|pad| {
            pad.geometry.as_ref().is_some_and(|g| {
                g.pad_type == "smd"
                    && !g.layers.is_empty()
                    && !g.layers.iter().any(|l| l.starts_with("F."))
            })
        });
        let side = if back { "bottom" } else { "top" };
        out.push_str(&format!(
            "<COMPONENT ref=\"{}\" package=\"{}\" x=\"{:.3}\" y=\"{:.3}\" rotation=\"{:.1}\" side=\"{}\"/>\n",
            xml_escape(&c.reference),
            xml_escape(&c.kind),
            f.position.x,
            f.position.y,
            f.rotation,
            side
        ));
        count(1)?;
    }
    out.push_str("</BOARD_DATA>\n<MANUFACTURING_DATA>\n");
    count(2)?;
    for via in &p.board.vias {
        out.push_str(&format!(
            "<HOLE x=\"{:.3}\" y=\"{:.3}\" diameter=\"{:.3}\" drill=\"{:.3}\" net=\"{}\" plated=\"true\"/>\n",
            via.position.x,
            via.position.y,
            via.diameter,
            via.drill,
            xml_escape(nets.get(&via.net).map(String::as_str).unwrap_or(""))
        ));
        count(1)?;
    }
    for hole in &p.board.holes {
        match hole.end {
            None => out.push_str(&format!(
                "<HOLE x=\"{:.3}\" y=\"{:.3}\" diameter=\"{:.3}\" plated=\"false\"/>\n",
                hole.position.x, hole.position.y, hole.diameter
            )),
            Some(_) => out.push_str(&format!(
                "<HOLE x=\"{:.3}\" y=\"{:.3}\" diameter=\"{:.3}\" x_end=\"{:.3}\" y_end=\"{:.3}\" length=\"{:.3}\" plated=\"false\"/>\n",
                hole.start().x,
                hole.start().y,
                hole.diameter,
                hole.finish().x,
                hole.finish().y,
                hole.length()
            )),
        }
        count(1)?;
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let Some(g) = &pad.geometry else { continue };
            if g.pad_type != "thru_hole" {
                continue;
            }
            let Some(drill) = &g.drill else { continue };
            let position = f.position.offset(pad.offset.rotate(f.rotation));
            out.push_str(&format!(
                "<HOLE x=\"{:.3}\" y=\"{:.3}\" diameter=\"{:.3}\" drill=\"{:.3}\" net=\"{}\" plated=\"true\"/>\n",
                position.x,
                position.y,
                pad.diameter,
                drill.size.x.max(drill.size.y),
                xml_escape(nets.get(&pad.net.unwrap_or(0)).map(String::as_str).unwrap_or(""))
            ));
            count(1)?;
        }
    }
    for (layer, name) in [
        (pcbcraft_doc::Layer::Front, "circuit0"),
        (pcbcraft_doc::Layer::Back, "circuit1"),
    ] {
        let layer_name = layer.name();
        out.push_str(&format!(
            "<LAYER name=\"{name}\" copper=\"{layer_name}\">\n"
        ));
        count(1)?;
        for track in p.board.tracks.iter().filter(|t| t.layer == layer) {
            out.push_str(&format!(
                "<TRACK start_x=\"{:.3}\" start_y=\"{:.3}\" end_x=\"{:.3}\" end_y=\"{:.3}\" width=\"{:.3}\" net=\"{}\"/>\n",
                track.start.x,
                track.start.y,
                track.end.x,
                track.end.y,
                track.width,
                xml_escape(nets.get(&track.net).map(String::as_str).unwrap_or(""))
            ));
            count(1)?;
        }
        for f in &p.board.footprints {
            for pad in &f.pads {
                let position = f.position.offset(pad.offset.rotate(f.rotation));
                let (w, h) = pad
                    .geometry
                    .as_ref()
                    .map(|g| (g.size.x, g.size.y))
                    .unwrap_or((pad.diameter, pad.diameter));
                out.push_str(&format!(
                    "<PAD layer=\"{layer_name}\" x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" net=\"{}\"/>\n",
                    position.x,
                    position.y,
                    w,
                    h,
                    xml_escape(nets.get(&pad.net.unwrap_or(0)).map(String::as_str).unwrap_or(""))
                ));
                count(1)?;
            }
        }
        out.push_str("</LAYER>\n");
    }
    out.push_str("</MANUFACTURING_DATA>\n</EDA3D>\n");
    Ok(out)
}

/// Parse a footprint assignment text: one pair per line, a component reference
/// first and the footprint file after whitespace; blank lines and # comments are
/// ignored, matching the files footprint assignment exports produce.
pub fn read_fp_assignments(text: &str) -> Result<Vec<(String, String)>, String> {
    const MAX_ROWS: usize = 64;
    let mut rows = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((reference, path)) = line.split_once([' ', '\t']) else {
            return Err(format!(
                "line {} needs a reference and a footprint path",
                number + 1
            ));
        };
        let reference = reference.trim();
        let path = path.trim();
        if reference.is_empty() || reference.len() > 64 || reference.contains(['*', '?']) {
            return Err(format!(
                "line {} has a reference outside 1 to 64 plain bytes",
                number + 1
            ));
        }
        if path.is_empty() || path.len() > 1024 {
            return Err(format!(
                "line {} has a path outside 1 to 1024 bytes",
                number + 1
            ));
        }
        rows.push((reference.to_string(), path.to_string()));
        if rows.len() > MAX_ROWS {
            return Err("an assignment file may hold up to 64 rows".into());
        }
    }
    if rows.is_empty() {
        return Err("an assignment file needs at least one row".into());
    }
    Ok(rows)
}

pub fn netlist_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PINS: usize = 50_000;
    let names: std::collections::BTreeMap<u64, String> =
        p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
    let mut rows: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pins = 0usize;
    for c in &p.components {
        for pin in &c.pins {
            let Some(net) = pin.net else {
                continue;
            };
            pins += 1;
            if pins > MAX_PINS {
                return Err("netlist exceeds the pin limit".into());
            }
            let name = names
                .get(&net)
                .ok_or("pin references an unknown net")?
                .clone();
            rows.entry(name)
                .or_default()
                .push(format!("{}.{}", c.reference, pin.number));
        }
    }
    let mut out = String::from("# PCBCraft netlist\n");
    out.push_str(&format!(
        "# board: {} x {} mm\n# nets: {}\n",
        p.board.width,
        p.board.height,
        rows.len()
    ));
    for (name, list) in &mut rows {
        list.sort();
        out.push_str(name);
        for entry in list {
            out.push(' ');
            out.push_str(entry);
        }
        out.push('\n');
    }
    Ok(out)
}
/// Plain-text design report: a board summary, one row per component with
/// its package and placement, and one row per net listing every pin that
/// sits on it.
pub fn report_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PINS: usize = 50_000;
    const MAX_ROWS: usize = 20_000;
    let names: BTreeMap<u64, String> = p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
    let mut rows: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pins = 0usize;
    for c in &p.components {
        for pin in &c.pins {
            let Some(net) = pin.net else {
                continue;
            };
            pins += 1;
            if pins > MAX_PINS {
                return Err("report exceeds the pin limit".into());
            }
            let name = names
                .get(&net)
                .ok_or("pin references an unknown net")?
                .clone();
            rows.entry(name)
                .or_default()
                .push(format!("{}.{}", c.reference, pin.number));
        }
    }
    if p.components.len() + rows.len() > MAX_ROWS {
        return Err("report exceeds the row limit".into());
    }
    let title = p.name.trim();
    let mut out = format!(
        "Design report for {}\n",
        if title.is_empty() {
            "untitled board"
        } else {
            title
        }
    );
    out.push_str(&format!(
        "Board: {:.3} x {:.3} mm, thickness {:.3} mm, clearance {:.3} mm, finish {}\n",
        p.board.width, p.board.height, p.board.thickness, p.board.clearance, p.board.finish
    ));
    out.push_str(&format!(
        "Counts: {} components, {} nets, {} tracks, {} vias, {} zones, {} holes\n\n",
        p.components.len(),
        p.nets.len(),
        p.board.tracks.len(),
        p.board.vias.len(),
        p.board.zones.len(),
        p.board.holes.len()
    ));
    out.push_str("COMPONENTS\nREF,VALUE,PACKAGE,X,Y,ROTATION\n");
    let mut refs: Vec<&pcbcraft_doc::Component> = p.components.iter().collect();
    refs.sort_by(|a, b| a.reference.cmp(&b.reference));
    for c in refs {
        let package = c
            .library_footprint
            .as_ref()
            .map(|f| f.definition.name.as_str())
            .unwrap_or("-");
        out.push_str(&format!(
            "{},{},{},{:.3},{:.3},{:.1}\n",
            c.reference,
            if c.value.is_empty() { "-" } else { &c.value },
            package,
            c.position.x,
            c.position.y,
            c.rotation.rem_euclid(360.0)
        ));
    }
    out.push_str("\nNETS\nNET,PINS\n");
    for (name, list) in &rows {
        let mut list = list.clone();
        list.sort();
        out.push_str(&format!("{},\"{}\"\n", name, list.join(" ")));
    }
    Ok(out)
}
/// Netlist as a HyperLynx-style simulation netlist CSV: one line per net,
/// the net name followed by every `reference.pin` that sits on it, comma
/// Drill tool chart as CSV: one row per distinct drill diameter and slot
/// span listing plating and hit count, with a total row and the recorded
/// drill tolerance, so a drill press operator can pick tools at a glance.
pub fn drill_chart_csv(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_TOOLS: usize = 64;
    let hits = project_drill_hits(p)?;
    let mut tools: BTreeMap<(u64, u64, bool), usize> = BTreeMap::new();
    for hit in hits {
        *tools.entry((hit.d, hit.span, hit.plated)).or_default() += 1;
    }
    if tools.len() > MAX_TOOLS {
        return Err("drill chart needs 64 or fewer distinct drill tools".into());
    }
    let total: usize = tools.values().sum();
    let title = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    let mut out = format!("# Drill tool chart for {title}\n");
    if let Some(t) = p.board.machining.as_ref().and_then(|m| m.drill_tolerance) {
        out.push_str(&format!("# Drill tolerance +/-{t:.3} mm\n"));
    }
    out.push_str("TOOL,DIAMETER_MM,SLOT_MM,PLATED,HOLES\n");
    for (n, ((d, span, plated), hits)) in tools.iter().enumerate() {
        out.push_str(&format!(
            "T{},{:.3},{:.3},{},{}\n",
            n + 1,
            *d as f64 / 1_000.0,
            *span as f64 / 1_000.0,
            if *plated { "yes" } else { "no" },
            hits
        ));
    }
    out.push_str(&format!("TOTAL,,,,{total}\n"));
    Ok(out)
}

/// Board statistics as CSV: one `KEY,VALUE` row per counter covering the
/// outline, parts, nets, pins, routing, vias, zones and documentation items,
/// so a design summary for quotes or reviews regenerates from the project.
pub fn stats_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    let title = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    let mut pins = 0usize;
    let mut connected = 0usize;
    let mut library_footprints = 0usize;
    for c in &p.components {
        if c.library_footprint.is_some() {
            library_footprints += 1;
        }
        for pin in &c.pins {
            pins += 1;
            if pin.net.is_some() {
                connected += 1;
            }
        }
    }
    let mut track_length = 0.0;
    let mut arced = 0usize;
    for t in &p.board.tracks {
        track_length += (t.end.x - t.start.x).hypot(t.end.y - t.start.y);
        if t.arc.is_some() {
            arced += 1;
        }
    }
    let mut drills = 0usize;
    for v in &p.board.vias {
        if v.drill > 0.0 {
            drills += 1;
        }
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            if pad.geometry.as_ref().is_some_and(|g| g.drill.is_some()) {
                drills += 1;
            }
        }
    }
    let rows: [(&str, String); 19] = [
        ("board_width_mm", format!("{:.3}", p.board.width)),
        ("board_height_mm", format!("{:.3}", p.board.height)),
        ("board_thickness_mm", format!("{:.3}", p.board.thickness)),
        ("components", p.components.len().to_string()),
        ("library_footprints", library_footprints.to_string()),
        ("nets", p.nets.len().to_string()),
        ("pins", pins.to_string()),
        ("pins_connected_to_nets", connected.to_string()),
        ("pins_without_net", (pins - connected).to_string()),
        ("track_segments", p.board.tracks.len().to_string()),
        ("track_arcs", arced.to_string()),
        ("track_length_mm", format!("{:.1}", track_length)),
        ("vias", p.board.vias.len().to_string()),
        ("drilled_holes_total", drills.to_string()),
        ("copper_zones", p.board.zones.len().to_string()),
        ("copper_cutouts", p.board.cutouts.len().to_string()),
        ("fiducials", p.board.fiducials.len().to_string()),
        ("board_texts", p.board.texts.len().to_string()),
        ("graphics", p.graphics.len().to_string()),
    ];
    let mut out = format!("# Board statistics for {title}\nKEY,VALUE\n");
    for (key, value) in rows {
        out.push_str(&format!("{key},{value}\n"));
    }
    Ok(out)
}

/// Board stackup report as plain text: board size, substrate and finish
/// data, every recorded layer note with its role and thickness, and any
/// machining tolerances, as one fabrication summary sheet.
pub fn stackup_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    let title = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    let mut out = format!(
        "Stackup report for {title}\n\
         Board: {:.3} x {:.3} x {:.3} mm\n\
         Substrate permittivity: {:.2}\n\
         Surface finish: {}\n\
         Drill origin: {:.3}, {:.3} mm\n\n\
         Layer notes:\nNAME,KIND,THICKNESS_MM\n",
        p.board.width,
        p.board.height,
        p.board.thickness,
        p.board.er,
        p.board.finish,
        p.board.drill_origin.x,
        p.board.drill_origin.y
    );
    if p.board.layer_records.is_empty() {
        out.push_str("(none recorded)\n");
    }
    let mut total = 0.0;
    for l in &p.board.layer_records {
        out.push_str(&format!("{},{},{:.3}\n", l.name, l.kind, l.thickness));
        total += l.thickness;
    }
    out.push_str(&format!("Total recorded thickness: {:.3} mm\n", total));
    if let Some(m) = &p.board.machining {
        out.push_str("\nMachining notes:\n");
        if let Some(v) = m.edge_plating {
            out.push_str(&format!("Edge plating: {}\n", if v { "yes" } else { "no" }));
        }
        if let Some(v) = m.drill_tolerance {
            out.push_str(&format!("Drill tolerance: +/- {:.3} mm\n", v));
        }
        if let Some(v) = m.edge_tolerance {
            out.push_str(&format!("Edge tolerance: +/- {:.3} mm\n", v));
        }
    }
    Ok(out)
}

/// separated, nets sorted by name and pins sorted by reference and number.
pub fn hyperlynx_csv(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PINS: usize = 50_000;
    let names: BTreeMap<u64, String> = p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
    let mut rows: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pins = 0usize;
    for c in &p.components {
        for pin in &c.pins {
            let Some(net) = pin.net else {
                continue;
            };
            pins += 1;
            if pins > MAX_PINS {
                return Err("netlist exceeds the pin limit".into());
            }
            let name = names
                .get(&net)
                .ok_or("pin references an unknown net")?
                .clone();
            rows.entry(name)
                .or_default()
                .push(format!("{}.{}", c.reference, pin.number));
        }
    }
    if rows.is_empty() {
        return Err("design has no connected nets to export".into());
    }
    let mut out = String::new();
    for (name, mut list) in rows {
        list.sort();
        out.push_str(&name);
        for entry in list {
            out.push(',');
            out.push_str(&entry);
        }
        out.push('\n');
    }
    Ok(out)
}
/// Pick-and-place placement file: one row per placed footprint in the classic
/// centroid format (`SMT|THT,reference,X,Y,rotation,side,package`), sorted by
/// reference. Side comes from the pad layers; height is unknown to this tool.
/// In-circuit test netlist in IPC-D-356A style fixed columns: five-digit net
/// number, twelve-column component reference, ten-column pin name, a 000000
/// operation code and eight-digit X and Y coordinates in micrometres, sorted
/// by net number, then reference, then pin. Comment lines start with `*`.
pub fn ipc356_netlist(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PINS: usize = 50_000;
    let numbers: BTreeMap<u64, usize> = p
        .nets
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i + 1))
        .collect();
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let mut rows: Vec<(usize, String, String, i64, i64)> = Vec::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        if c.reference.len() > 12 {
            return Err("reference is too long for the test netlist".into());
        }
        for pad in &f.pads {
            if pad.number.len() > 10 {
                return Err("pin name is too long for the test netlist".into());
            }
            let net = match pad.net {
                Some(n) => *numbers.get(&n).ok_or("pad references an unknown net")?,
                None => 0,
            };
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            if !world.x.is_finite() || !world.y.is_finite() {
                return Err("pad position is not a number".into());
            }
            let (x, y) = (
                (world.x * 1000.0).round() as i64,
                (world.y * 1000.0).round() as i64,
            );
            if x < 0 || y < 0 || x > 99_999_999 || y > 99_999_999 {
                return Err("pad coordinates fall outside the test file range".into());
            }
            rows.push((net, c.reference.clone(), pad.number.clone(), x, y));
        }
    }
    if rows.len() > MAX_PINS {
        return Err("test netlist exceeds the pin limit".into());
    }
    rows.sort();
    let mut out = format!(
        "*- PCBCraft IPC-D-356A test netlist\n*- board {} x {} mm, {} nets\n",
        p.board.width,
        p.board.height,
        p.nets.len()
    );
    for (net, reference, pin, x, y) in &rows {
        out.push_str(&format!(
            "{net:05}{reference:<12}{pin:<10}000000{x:08}{y:08}\n"
        ));
    }
    Ok(out)
}

pub fn placement_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PLACED: usize = 5_000;
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    if p.board.footprints.len() > MAX_PLACED {
        return Err("placement file exceeds the placed-object limit".into());
    }
    let mut rows: Vec<String> = Vec::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let back = f.pads.iter().any(|pad| {
            pad.geometry.as_ref().is_some_and(|g| {
                g.pad_type == "smd"
                    && !g.layers.is_empty()
                    && !g.layers.iter().any(|l| l.starts_with("F."))
            })
        });
        let tht = f.pads.iter().any(|pad| {
            pad.geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "thru_hole")
        });
        let package = match &c.library_footprint {
            Some(footprint) => footprint.definition.name.clone(),
            None => c.kind.clone(),
        };
        rows.push(format!(
            "{},\"{}\",{:.3},{:.3},{:.1},{},\"{}\"",
            if tht { "THT" } else { "SMT" },
            c.reference,
            f.position.x,
            f.position.y,
            f.rotation.rem_euclid(360.0),
            if back { "B" } else { "T" },
            package
        ));
    }
    for (i, f) in p.board.fiducials.iter().enumerate() {
        rows.push(format!(
            "SMT,\"FID{:02}\",{:.3},{:.3},0.0,{},FIDUCIAL",
            i + 1,
            f.position.x,
            f.position.y,
            if f.layer == pcbcraft_doc::Layer::Back {
                "B"
            } else {
                "T"
            }
        ));
    }
    rows.sort_by(|a, b| {
        a.split('"')
            .nth(1)
            .unwrap_or("")
            .cmp(b.split('"').nth(1).unwrap_or(""))
    });
    Ok(format!(
        "File Format,ASCII,\n#\n#\tPick and Place File\t{}\n#\nSMT,THT,X,Y,Rotation,Side,Package\n{}\n",
        p.name,
        rows.join("\n")
    ))
}
/// Fabrication plot report as CSV: one row per copper layer counting the
/// elements a plot of that layer would draw (pads, tracks, vias, zones,
/// text, fiducials and holes) with the bounding box of that layer's
/// elements in millimetres, plus a totals row. Through-hole elements are
/// listed on both copper layers, as both would be plotted.
pub fn plot_csv(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_ELEMENTS: usize = 50_000;
    #[derive(Default)]
    struct LayerPlot {
        pads: usize,
        tracks: usize,
        vias: usize,
        zones: usize,
        texts: usize,
        fiducials: usize,
        holes: usize,
        min: Option<(f64, f64)>,
        max: Option<(f64, f64)>,
    }
    let mut layers: BTreeMap<&str, LayerPlot> = BTreeMap::new();
    layers.insert("F.Cu", LayerPlot::default());
    layers.insert("B.Cu", LayerPlot::default());
    let mut elements = 0usize;
    let mut touch = |layers: &mut BTreeMap<&str, LayerPlot>,
                     layer: &str,
                     seen: &mut dyn FnMut(&mut LayerPlot),
                     points: &[(f64, f64)]|
     -> Result<(), String> {
        elements += 1;
        if elements > MAX_ELEMENTS {
            return Err("board exceeds the plot element limit".into());
        }
        let plot = layers
            .get_mut(layer)
            .ok_or_else(|| format!("unknown layer {layer}"))?;
        seen(plot);
        for &(x, y) in points {
            plot.min = Some(match plot.min {
                Some((mx, my)) => (mx.min(x), my.min(y)),
                None => (x, y),
            });
            plot.max = Some(match plot.max {
                Some((mx, my)) => (mx.max(x), my.max(y)),
                None => (x, y),
            });
        }
        Ok(())
    };
    for f in &p.board.footprints {
        for pad in &f.pads {
            let Some(geometry) = &pad.geometry else {
                continue;
            };
            let centre = f.position.offset(geometry.position.rotate(-f.rotation));
            let points = [(centre.x, centre.y)];
            for layer in &geometry.layers {
                let name = match layer.as_str() {
                    "F.Cu" | "B.Cu" => Some(layer.as_str()),
                    "*.Cu" => None,
                    _ => continue,
                };
                match name {
                    Some(name) => touch(&mut layers, name, &mut |plot| plot.pads += 1, &points)?,
                    None => {
                        for name in ["F.Cu", "B.Cu"] {
                            touch(&mut layers, name, &mut |plot| plot.pads += 1, &points)?
                        }
                    }
                }
            }
        }
    }
    for track in &p.board.tracks {
        let points = [(track.start.x, track.start.y), (track.end.x, track.end.y)];
        touch(
            &mut layers,
            &p.layer_name(track.layer),
            &mut |plot| plot.tracks += 1,
            &points,
        )?;
    }
    for via in &p.board.vias {
        let points = [(via.position.x, via.position.y)];
        for name in ["F.Cu", "B.Cu"] {
            touch(&mut layers, name, &mut |plot| plot.vias += 1, &points)?;
        }
    }
    for zone in &p.board.zones {
        let points: Vec<(f64, f64)> = zone.outline.iter().map(|pt| (pt.x, pt.y)).collect();
        touch(
            &mut layers,
            &p.layer_name(zone.layer),
            &mut |plot| plot.zones += 1,
            &points,
        )?;
    }
    for text in &p.board.texts {
        let points = [(text.position.x, text.position.y)];
        touch(
            &mut layers,
            &p.layer_name(text.layer),
            &mut |plot| plot.texts += 1,
            &points,
        )?;
    }
    for fiducial in &p.board.fiducials {
        let points = [(fiducial.position.x, fiducial.position.y)];
        touch(
            &mut layers,
            &p.layer_name(fiducial.layer),
            &mut |plot| plot.fiducials += 1,
            &points,
        )?;
    }
    for hole in &p.board.holes {
        let points = [
            (hole.start().x, hole.start().y),
            (hole.finish().x, hole.finish().y),
        ];
        for name in ["F.Cu", "B.Cu"] {
            touch(&mut layers, name, &mut |plot| plot.holes += 1, &points)?;
        }
    }
    let coord = |v: Option<(f64, f64)>, first: bool| -> String {
        match v {
            Some((x, y)) => format!("{:.3}", if first { x } else { y }),
            None => String::new(),
        }
    };
    let mut out =
        String::from("layer,pads,tracks,vias,zones,text,fiducials,holes,min_x,min_y,max_x,max_y\n");
    let mut totals = [0usize; 7];
    for (name, plot) in &layers {
        let counts = [
            plot.pads,
            plot.tracks,
            plot.vias,
            plot.zones,
            plot.texts,
            plot.fiducials,
            plot.holes,
        ];
        for (total, value) in totals.iter_mut().zip(counts) {
            *total += value;
        }
        let mut row = vec![name.to_string()];
        row.extend(counts.iter().map(|v| v.to_string()));
        row.push(coord(plot.min, true));
        row.push(coord(plot.min, false));
        row.push(coord(plot.max, true));
        row.push(coord(plot.max, false));
        out.push_str(&row.join(","));
        out.push('\n');
    }
    let mut row = vec!["total".to_string()];
    row.extend(totals.iter().map(|v| v.to_string()));
    row.extend(std::iter::repeat_n(String::new(), 4));
    out.push_str(&row.join(","));
    out.push('\n');
    Ok(out)
}
/// Board as a legacy trace file for legacy importers and viewers: every pad
/// with its absolute position in whole mils and a front or back layer code,
/// followed by every net with its member pads. Coordinates are whole mils.
pub fn cmp_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_LINES: usize = 50_000;
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let mils = |v: f64| (v / 0.0254).round() as i64;
    let mut components: Vec<String> = Vec::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        for pad in &f.pads {
            let back = pad.geometry.as_ref().is_some_and(|g| {
                !g.layers.is_empty()
                    && if g.pad_type == "smd" {
                        !g.layers.iter().any(|l| l.starts_with("F."))
                    } else {
                        g.layers.iter().any(|l| l == "B.Cu")
                    }
            });
            let at = f.position.offset(pad.offset.rotate(-f.rotation));
            components.push(format!(
                "$CD {}\t{}\t{}\t{}\t{}\t{}\t0",
                c.reference,
                pad.number,
                mils(at.x),
                mils(at.y),
                if back { 1 } else { 0 },
                usize::from(pad.diameter.max(0.0) > 0.0)
            ));
        }
    }
    let mut nets: Vec<String> = Vec::new();
    for n in &p.nets {
        let mut members: BTreeSet<(u64, usize)> = BTreeSet::new();
        for f in &p.board.footprints {
            for (i, pad) in f.pads.iter().enumerate() {
                if pad.net == Some(n.id) {
                    members.insert((f.component, i + 1));
                }
            }
        }
        if members.is_empty() {
            continue;
        }
        let list = members
            .iter()
            .map(|(c, i)| format!("{}\t{}", c, i))
            .collect::<Vec<_>>()
            .join("\t");
        nets.push(format!("{}\t{}\t{}", n.name, members.len(), list));
    }
    if components.is_empty() && nets.is_empty() {
        return Err("board has no placed pads or connected nets to export".into());
    }
    if components.len() + nets.len() > MAX_LINES {
        return Err("trace file exceeds the element limit".into());
    }
    Ok(format!(
        "$FILE\nTYPE:BOARD\nCODE:ASCII\n#\n{}\n$COMPONENTS\n{}\n$END\n#\n$NETS\n{}\n$END\n#\n$END\n",
        p.name,
        components.join("\n"),
        nets.join("\n")
    ))
}
/// Board as a GenCAD-style interchange text: tab-delimited component, pin and
/// net records with positions in whole mils. `%C` lines are one per placed
/// footprint in order, `%P` lines one per pad (shape, size and a front/back
/// layer), `%N` lines one per net that has placed pads, naming its members as
/// component and pad indexes.
pub fn gencad_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_RECORDS: usize = 50_000;
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let mils = |v: f64| (v / 0.0254).round() as i64;
    let mut out = String::new();
    let mut records = 0usize;
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        records += 1;
        if records > MAX_RECORDS {
            return Err("interchange file exceeds the record limit".into());
        }
        out.push_str(&format!(
            "%C\t{}\t{}\t{}\t{}\t{}\t0\t0\t0\t0\n",
            c.reference,
            c.value,
            mils(f.position.x),
            mils(f.position.y),
            mils(f.rotation),
        ));
    }
    if p.board.footprints.is_empty() {
        return Err("board has no placed footprints to export".into());
    }
    for (ci, f) in p.board.footprints.iter().enumerate() {
        for pad in &f.pads {
            records += 1;
            if records > MAX_RECORDS {
                return Err("interchange file exceeds the record limit".into());
            }
            let at = f.position.offset(pad.offset.rotate(-f.rotation));
            let back = pad.geometry.as_ref().is_some_and(|g| {
                !g.layers.is_empty()
                    && if g.pad_type == "smd" {
                        !g.layers.iter().any(|l| l.starts_with("F."))
                    } else {
                        true
                    }
            });
            let shape = match pad
                .geometry
                .as_ref()
                .map(|g| (g.shape.as_str(), g.pad_type.as_str()))
            {
                Some(("circle", "thru_hole")) => "round",
                Some(("oval", _)) => "oval",
                _ => "rect",
            };
            out.push_str(&format!(
                "%P\t{}\t1\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                ci + 1,
                pad.number,
                mils(at.x),
                mils(at.y),
                shape,
                usize::from(pad.diameter.max(0.0) > 0.0),
                mils(pad.geometry.as_ref().map_or(pad.diameter, |g| g.size.x)),
                mils(pad.geometry.as_ref().map_or(pad.diameter, |g| g.size.y)),
                usize::from(back),
            ));
        }
    }
    for n in &p.nets {
        let mut members: BTreeSet<(usize, usize)> = BTreeSet::new();
        for (ci, f) in p.board.footprints.iter().enumerate() {
            for (pi, pad) in f.pads.iter().enumerate() {
                if pad.net == Some(n.id) {
                    members.insert((ci + 1, pi + 1));
                }
            }
        }
        if members.is_empty() {
            continue;
        }
        records += 1;
        if records > MAX_RECORDS {
            return Err("interchange file exceeds the record limit".into());
        }
        let list = members
            .iter()
            .flat_map(|(c, i)| [c.to_string(), i.to_string()])
            .collect::<Vec<_>>()
            .join("\t");
        out.push_str(&format!("%N\t{}\t{}\t{}\n", n.name, members.len(), list));
    }
    Ok(out)
}
/// Board as an IDF mechanical drawing for enclosure tools: board size, origin
/// and reference, one entry per placed footprint with its position, side and
/// rotation, and a matching default-height entry per component. The native
/// format stores no component heights, so heights are emitted as 1 mm.
pub fn idf_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PLACED: usize = 5_000;
    if p.board.footprints.len() > MAX_PLACED {
        return Err("mechanical drawing exceeds the placed-object limit".into());
    }
    if p.board.footprints.is_empty() {
        return Err("board has no placed footprints to export".into());
    }
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let n3 = |v: f64| -> String {
        let r = (v * 1000.0).round() / 1000.0;
        if r == 0.0 {
            "0.000".into()
        } else {
            format!("{r:.3}")
        }
    };
    let mut body = String::new();
    for (pin, f) in p.board.footprints.iter().enumerate() {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let back = f.pads.iter().any(|pad| {
            pad.geometry.as_ref().is_some_and(|g| {
                g.pad_type == "smd"
                    && !g.layers.is_empty()
                    && !g.layers.iter().any(|l| l.starts_with("F."))
            })
        });
        body.push_str(&format!(
            ".END_BODY {}\t{}\t{}\t{}\t{}\t{}\n",
            c.reference,
            n3(f.position.x),
            n3(f.position.y),
            if back { "BACK" } else { "FRONT" },
            n3(f.rotation),
            pin
        ));
    }
    let mut pinbody = String::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        pinbody.push_str(&format!(
            ".END_PIN {{\t}}\t{}\t{}\t{}\t0.000\t0.000\t1.000\t0\n",
            c.reference,
            n3(f.position.x),
            n3(f.position.y)
        ));
    }
    Ok(format!(
        ".FILE_IDF\n.3DIDF VERSION: 2.0\n.UNITS:\tmillmm\n\
         .BOARD_PATH\n\t.BODY {w} {h} 0.000 0.000\n\t.OFFSET 0.000 0.000\n\t.REFERENCE \"{name}\"\n.END_BOARD_PATH\n\
         .PINUP 0\n.PINUP_END\n.TPIN 0\n.TPIN_END\n.PLPIN 0\n.PLPIN_END\n\
         .ELEC_PATH\n{body}.END_ELEC_PATH\n\
         .PIN_FILE\n.PIN {count}\n{pin}.END_PIN_FILE\n.END_FILE_IDF\n",
        w = n3(p.board.width),
        h = n3(p.board.height),
        name = p.name.replace('"', "'"),
        body = body,
        count = p.board.footprints.len(),
        pin = pinbody,
    ))
}
/// Board as a Specctra-style board file for external autorouters: the board
/// boundary, layer stack with a through via type, net classes, every net with
/// its pins, component and pin placements, track wires (arcs sampled into
/// segments), vias, zone pours and rectangular rule areas as keepouts.
/// Coordinates are whole micrometres at a resolution of 1000 per millimetre.
pub fn specctra_dsn(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_ELEMENTS: usize = 50_000;
    let mut count = 0usize;
    let bump = |n: &mut usize| -> Result<(), String> {
        *n += 1;
        if *n > MAX_ELEMENTS {
            return Err("board file exceeds the element limit".into());
        }
        Ok(())
    };
    let um = |v: f64| -> Result<i64, String> {
        let scaled = (v * 1000.0).round();
        if !scaled.is_finite() || scaled.abs() > 2_000_000_000.0 {
            return Err("board coordinate falls outside the file range".into());
        }
        Ok(scaled as i64)
    };
    let point =
        |q: &Point| -> Result<String, String> { Ok(format!("({} {})", um(q.x)?, um(q.y)?)) };
    let numbers: BTreeMap<u64, usize> = p
        .nets
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i + 1))
        .collect();
    let net_number = |id: Option<u64>| -> Result<usize, String> {
        match id {
            None => Ok(0),
            Some(n) => numbers
                .get(&n)
                .copied()
                .ok_or_else(|| "element references an unknown net".to_string()),
        }
    };
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let mut out = format!(
        "(PCB {}\n  (resolution 1000.0 1000.0)\n  (units mm)\n",
        quote(&p.name)
    );
    out.push_str("  (network\n");
    for (i, net) in p.nets.iter().enumerate() {
        bump(&mut count)?;
        let mut pins: Vec<String> = Vec::new();
        for f in &p.board.footprints {
            let c = refs
                .get(&f.component)
                .ok_or("footprint references an unknown component")?;
            for pad in &f.pads {
                if pad.net == Some(net.id) {
                    pins.push(format!("{}.{}", quote(&c.reference), quote(&pad.number)));
                }
            }
        }
        out.push_str(&format!(
            "    (net {} {} (pins {}))\n",
            i + 1,
            quote(&net.name),
            pins.join(" ")
        ));
    }
    for class in &p.net_classes {
        bump(&mut count)?;
        out.push_str(&format!(
            "    (netclass {} (use {}))\n",
            quote(&class.name),
            quote(&class.name)
        ));
    }
    out.push_str("  )\n");
    for class in &p.net_classes {
        out.push_str(&format!(
            "  (classpath {} (circuit (minClearance {}) (sameNetClearance {}) (defaultTraceWidth {})) (layer \"F.Cu\" \"(c: {}))\" (layer \"B.Cu\" \"(c: {}))\")\n",
            quote(&class.name),
            um(class.clearance)?,
            um(class.clearance)?,
            um(class.width)?,
            um(class.width)?,
            um(class.width)?
        ));
    }
    let clearance = um(p.board.clearance)?;
    out.push_str(&format!(
        "  (structure\n    (layer \"signal\" \"F.Cu\" (circuit (minClearance {c}) (viaSize 600 600) (viaDepth 0 0)))\n    (layer \"signal\" \"B.Cu\" (circuit (minClearance {c}) (viaSize 600 600) (viaDepth 0 0)))\n    (layer \"through_via\" \"VIA\" (circuit (viaType \"VIA\")))\n    (layer \"boundary\" \"BOARD_BOUNDARY\" (boundary (direction \"positive\")))\n    (via \"VIA\" (type \"through_via\") (shape (circle (path (0 0)) (width 600))))\n  )\n",
        c = clearance
    ));
    out.push_str("  (image\n");
    let corners: Vec<Point> = board_edge_points(p)
        .iter()
        .map(|q| Point::new(q.0, q.1))
        .collect();
    let boundary: Result<Vec<String>, String> = corners.iter().map(point).collect();
    out.push_str(&format!(
        "    (polygon (layer BOARD_BOUNDARY) (net 0) (path {}))\n",
        boundary?.join(" ")
    ));
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        bump(&mut count)?;
        out.push_str(&format!(
            "    (component (name {}) (place (type fixed) (side top) (rotate {:.1}) (at {})))\n",
            quote(&c.reference),
            f.rotation.rem_euclid(360.0),
            point(&f.position)?
        ));
        for pad in &f.pads {
            bump(&mut count)?;
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            out.push_str(&format!(
                "    (pin (name {}.{}) (length 0) (at {}) (shape (circle (path (0 0)) (width {}))) (number {}))\n",
                quote(&c.reference),
                quote(&pad.number),
                point(&world)?,
                um(pad.diameter.max(0.1))?,
                quote(&pad.number)
            ));
        }
    }
    for t in &p.board.tracks {
        bump(&mut count)?;
        let layer = if t.layer == pcbcraft_doc::Layer::Back {
            "B.Cu"
        } else {
            "F.Cu"
        };
        let width = um(t.width)?;
        let net = net_number(Some(t.net))?;
        let segments: Vec<(Point, Point)> = match t.curve() {
            Some(arc) => {
                let steps = 8;
                let at = |i: usize| {
                    let a = arc.start_angle + arc.sweep * (i as f64 / steps as f64);
                    Point::new(
                        arc.center.x + arc.radius * a.cos(),
                        arc.center.y + arc.radius * a.sin(),
                    )
                };
                (0..steps).map(|i| (at(i), at(i + 1))).collect()
            }
            None => vec![(t.start, t.end)],
        };
        for (a, b) in segments {
            out.push_str(&format!(
                "    (wire (path (net {net}) (layer {layer}) (width {width}) {} {}))\n",
                point(&a)?,
                point(&b)?
            ));
        }
    }
    for v in &p.board.vias {
        bump(&mut count)?;
        let net = net_number(Some(v.net))?;
        out.push_str(&format!(
            "    (via (net {net}) (type VIA) (at {}))\n",
            point(&v.position)?
        ));
    }
    for z in &p.board.zones {
        bump(&mut count)?;
        let layer = if z.layer == pcbcraft_doc::Layer::Back {
            "B.Cu"
        } else {
            "F.Cu"
        };
        let net = net_number(Some(z.net))?;
        let path: Result<Vec<String>, String> = z.outline.iter().map(point).collect();
        out.push_str(&format!(
            "    (polygon (layer {layer}) (net {net}) (path {}))\n",
            path?.join(" ")
        ));
    }
    for area in &p.board.rule_areas {
        bump(&mut count)?;
        let layers: Vec<&str> = match area.layer {
            Some(l) => vec![if l == pcbcraft_doc::Layer::Back {
                "B.Cu"
            } else {
                "F.Cu"
            }],
            None => vec!["F.Cu", "B.Cu"],
        };
        let lo = Point::new(area.region.x, area.region.y);
        let hi = Point::new(
            area.region.x + area.region.width,
            area.region.y + area.region.height,
        );
        for layer in layers {
            out.push_str(&format!(
                "    (keepout (layer {layer}) (net 0) (rect {} {}))\n",
                point(&lo)?,
                point(&hi)?
            ));
        }
    }
    out.push_str("  )\n)\n");
    Ok(out)
}

/// Board drawing as an ASCII DXF (R12 entities): tracks become lines, vias
/// and round pads circles, oval and rectangular pads closed polylines, zones
/// and board-layer graphics outlines, arcs three-point arcs, labels text.
/// The board edge and circular cutouts are drawn on their own layer.
/// Resolve `${...}` tokens in a board text from the component it is bound
/// to: `${REFERENCE}`, `${VALUE}`, `${FOOTPRINT}` and every custom field
/// key. Unbound text and unknown tokens are returned unchanged.
pub fn board_text(p: &Project, t: &pcbcraft_doc::BoardText) -> String {
    let Some(bound) = t.component else {
        return t.text.clone();
    };
    let Some(c) = p.component(bound) else {
        return t.text.clone();
    };
    let mut out = t
        .text
        .replace("${REFERENCE}", &c.reference)
        .replace("${VALUE}", &c.value);
    if out.contains("${FOOTPRINT}") {
        let name = c
            .library_footprint
            .as_ref()
            .map_or("", |f| f.definition.name.as_str());
        out = out.replace("${FOOTPRINT}", name);
    }
    for field in &c.fields {
        let token = format!("${{{}}}", field.key);
        if out.contains(&token) {
            out = out.replace(&token, &field.value);
        }
    }
    out
}

/// How many entities one DXF drawing may carry.
const MAX_DXF_ENTITIES: usize = 20_000;

/// Writer for one ASCII DXF R12 drawing, in millimetres, that tracks the
/// entity count and the drawing extents as entities arrive.
pub(crate) struct DxfWriter {
    entities: String,
    count: usize,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}
impl DxfWriter {
    /// An empty drawing whose extents grow with the first entity.
    fn new() -> Self {
        Self {
            entities: String::new(),
            count: 0,
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        }
    }
    /// Grow the drawing extents to include a point.
    pub(crate) fn follow(&mut self, x: f64, y: f64) {
        if self.count == 0 {
            self.min_x = x;
            self.min_y = y;
            self.max_x = x;
            self.max_y = y;
        } else {
            self.min_x = self.min_x.min(x);
            self.min_y = self.min_y.min(y);
            self.max_x = self.max_x.max(x);
            self.max_y = self.max_y.max(y);
        }
    }
    /// Append one entity: its type, then the layer it belongs to, then its
    /// own group codes. R12 wants the type first; putting a placeholder
    /// entity in front makes third-party readers drop the real entity onto
    /// layer 0 and lose the layer name.
    pub(crate) fn keep(&mut self, layer: &str, kind: &str, body: String) -> Result<(), String> {
        self.count += 1;
        if self.count > MAX_DXF_ENTITIES {
            return Err("DXF export exceeds the entity limit".into());
        }
        self.entities.push_str("0\n");
        self.entities.push_str(kind);
        self.entities.push_str("\n8\n");
        self.entities.push_str(layer);
        self.entities.push('\n');
        self.entities.push_str(&body);
        Ok(())
    }
    fn line(&mut self, layer: &str, a: (f64, f64), b: (f64, f64)) -> Result<(), String> {
        self.follow(a.0, a.1);
        self.follow(b.0, b.1);
        self.keep(
            layer,
            "LINE",
            format!(
                "10\n{:.3}\n20\n{:.3}\n11\n{:.3}\n21\n{:.3}\n",
                a.0, a.1, b.0, b.1
            ),
        )
    }
    fn arc(
        &mut self,
        layer: &str,
        c: (f64, f64),
        r: f64,
        from_deg: f64,
        to_deg: f64,
    ) -> Result<(), String> {
        self.follow(c.0 - r, c.1 - r);
        self.follow(c.0 + r, c.1 + r);
        self.keep(
            layer,
            "ARC",
            format!(
                "10\n{:.3}\n20\n{:.3}\n40\n{:.3}\n50\n{:.6}\n51\n{:.6}\n",
                c.0, c.1, r, from_deg, to_deg
            ),
        )
    }
    pub(crate) fn circle(&mut self, layer: &str, x: f64, y: f64, r: f64) -> Result<(), String> {
        self.follow(x - r, y - r);
        self.follow(x + r, y + r);
        self.keep(
            layer,
            "CIRCLE",
            format!("10\n{x:.3}\n20\n{y:.3}\n40\n{r:.3}\n"),
        )
    }
    pub(crate) fn polyline(
        &mut self,
        layer: &str,
        points: &[(f64, f64)],
        closed: bool,
    ) -> Result<(), String> {
        let mut body = format!("90\n{}\n70\n{}\n", points.len(), u8::from(closed));
        for (x, y) in points {
            self.follow(*x, *y);
            body.push_str(&format!("10\n{x:.3}\n20\n{y:.3}\n"));
        }
        self.keep(layer, "LWPOLYLINE", body)
    }
    pub(crate) fn text(
        &mut self,
        layer: &str,
        x: f64,
        y: f64,
        height: f64,
        rotation: f64,
        value: &str,
    ) -> Result<(), String> {
        self.follow(x, y);
        self.keep(
            layer,
            "TEXT",
            format!("10\n{x:.3}\n20\n{y:.3}\n40\n{height:.3}\n50\n{rotation:.1}\n1\n{value}\n"),
        )
    }
    /// The whole drawing: millimetre header, one ENTITIES section, EOF.
    pub(crate) fn finish(self) -> String {
        format!(
            "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n9\n$EXTMIN\n10\n{:.3}\n20\n{:.3}\n9\n$EXTMAX\n10\n{:.3}\n20\n{:.3}\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
            self.min_x, self.min_y, self.max_x, self.max_y, self.entities
        )
    }
}

pub fn dxf_board(p: &Project) -> Result<String, String> {
    p.validate()?;

    fn xy(q: &pcbcraft_geom::Point) -> (f64, f64) {
        (q.x, q.y)
    }
    let mut w = DxfWriter::new();
    let corners = board_edge_points(p);
    w.polyline("Edge", &corners, true)?;
    for c in &p.board.cutouts {
        w.circle("Edge", c.center.x, c.center.y, c.radius)?;
    }
    for t in &p.board.tracks {
        let layer = if t.layer == pcbcraft_doc::Layer::Back {
            "B_Cu"
        } else {
            "F_Cu"
        };
        match t.curve() {
            Some(arc) => {
                let deg = |a: f64| {
                    let mut d = a.to_degrees() % 360.0;
                    if d < 0.0 {
                        d += 360.0;
                    }
                    d
                };
                let end_angle = arc.start_angle + arc.sweep;
                let (from, to) = if arc.sweep >= 0.0 {
                    (arc.start_angle, end_angle)
                } else {
                    (end_angle, arc.start_angle)
                };
                w.arc(layer, xy(&arc.center), arc.radius, deg(from), deg(to))?;
            }
            None => w.line(layer, xy(&t.start), xy(&t.end))?,
        }
    }
    for v in &p.board.vias {
        w.circle("Via", v.position.x, v.position.y, v.diameter / 2.0)?;
        if v.drill > 0.0 {
            w.circle("Drill", v.position.x, v.position.y, v.drill / 2.0)?;
        }
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            let back = pad.geometry.as_ref().is_some_and(|g| {
                !g.layers.is_empty() && !g.layers.iter().any(|l| l.starts_with("F."))
            });
            let layer = if back { "B_Pads" } else { "F_Pads" };
            let geometry = match &pad.geometry {
                Some(g) => g,
                None => {
                    w.circle(layer, world.x, world.y, pad.diameter / 2.0)?;
                    continue;
                }
            };
            if geometry.shape == "circle" {
                w.circle(
                    layer,
                    world.x,
                    world.y,
                    geometry.size.x.max(geometry.size.y) / 2.0,
                )?;
            } else {
                let half_x = geometry.size.x / 2.0;
                let half_y = geometry.size.y / 2.0;
                w.polyline(
                    layer,
                    &[
                        (world.x - half_x, world.y - half_y),
                        (world.x + half_x, world.y - half_y),
                        (world.x + half_x, world.y + half_y),
                        (world.x - half_x, world.y + half_y),
                    ],
                    true,
                )?;
            }
            if let Some(drill) = &geometry.drill {
                w.circle(
                    "Drill",
                    world.x,
                    world.y,
                    drill.size.x.max(drill.size.y) / 2.0,
                )?;
            }
        }
    }
    for z in &p.board.zones {
        let layer = if z.layer == pcbcraft_doc::Layer::Back {
            "B_Cu"
        } else {
            "F_Cu"
        };
        w.polyline(layer, &z.outline.iter().map(xy).collect::<Vec<_>>(), true)?;
        for cutout in &z.cutouts {
            w.polyline(layer, &cutout.iter().map(xy).collect::<Vec<_>>(), true)?;
        }
    }
    for g in p.graphics.iter().filter(|g| g.layer == "board") {
        let layer = "Board_Graphics";
        if g.shape == "arc" && g.points.len() == 3 {
            let (a, b, c) = (g.points[0], g.points[1], g.points[2]);
            let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
            if d.abs() > 1e-9 {
                let a2 = a.x * a.x + a.y * a.y;
                let b2 = b.x * b.x + b.y * b.y;
                let c2 = c.x * c.x + c.y * c.y;
                let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
                let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
                let angle = |x: f64, y: f64| (y - uy).atan2(x - ux).to_degrees().rem_euclid(360.0);
                let x0 = angle(a.x, a.y);
                let xm = angle(b.x, b.y);
                let x1 = angle(c.x, c.y);
                let ccw = (x1 - x0).rem_euclid(360.0);
                let to_mid = (xm - x0).rem_euclid(360.0);
                let (start, end) = if to_mid <= ccw { (x0, x1) } else { (x1, x0) };
                let radius = ((a.x - ux).powi(2) + (a.y - uy).powi(2)).sqrt();
                w.follow(ux - radius, uy - radius);
                w.follow(ux + radius, uy + radius);
                w.keep(
                    layer,
                    "ARC",
                    format!(
                        "10\n{ux:.3}\n20\n{uy:.3}\n40\n{radius:.3}\n50\n{start:.3}\n51\n{end:.3}\n"
                    ),
                )?;
                continue;
            }
        }
        if let Some(outline) = g.outline() {
            let closed = g.shape != "line";
            w.polyline(layer, &outline.iter().map(xy).collect::<Vec<_>>(), closed)?;
        } else if g.shape == "text"
            && let Some(anchor) = g.points.first()
        {
            w.text(layer, anchor.x, anchor.y, g.width, 0.0, &g.text)?;
        }
    }
    for t in &p.board.texts {
        let layer = if t.layer == pcbcraft_doc::Layer::Back {
            "B_Silk"
        } else {
            "F_Silk"
        };
        w.text(
            layer,
            t.position.x,
            t.position.y,
            t.size,
            t.rotation,
            &board_text(p, t),
        )?;
    }
    drawn::write_dxf_layers(p, &mut w)?;
    Ok(w.finish())
}
/// A drawable entity read back from a DXF drawing file.
#[derive(Clone, Debug, PartialEq)]
pub enum DxfShape {
    Line {
        start: Point,
        end: Point,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Arc {
        center: Point,
        radius: f64,
        start_deg: f64,
        end_deg: f64,
    },
    Polyline {
        points: Vec<Point>,
        closed: bool,
    },
    Text {
        at: Point,
        height: f64,
        rotation: f64,
        value: String,
    },
}
/// One entity of a DXF drawing, with the layer name it was drawn on.
#[derive(Clone, Debug, PartialEq)]
pub struct DxfEntity {
    pub layer: String,
    pub shape: DxfShape,
}

#[derive(Default)]
struct DxfAcc {
    name: String,
    layer: String,
    points: Vec<Point>,
    pending_x: Option<f64>,
    pending_y: Option<f64>,
    end: Option<Point>,
    end_x: Option<f64>,
    end_y: Option<f64>,
    radius: Option<f64>,
    start_deg: Option<f64>,
    end_deg: Option<f64>,
    closed: bool,
    value: String,
}

impl DxfAcc {
    fn finish(&self) -> Result<Option<DxfShape>, String> {
        let number = |v: Option<f64>, what: &str| -> Result<f64, String> {
            v.filter(|v| v.is_finite())
                .ok_or_else(|| format!("DXF {what} is missing or not a number"))
        };
        match self.name.as_str() {
            "LINE" => {
                if self.points.len() != 1 {
                    return Err("DXF line needs one start point".into());
                }
                let end = self
                    .end
                    .filter(|p| p.valid())
                    .ok_or("DXF line needs a valid end point")?;
                let start = self.points[0];
                if !start.valid() {
                    return Err("DXF line start is out of range".into());
                }
                Ok(Some(DxfShape::Line { start, end }))
            }
            "CIRCLE" => {
                if self.points.len() != 1 {
                    return Err("DXF circle needs one centre".into());
                }
                let radius = number(self.radius, "circle radius")?;
                if radius <= 0.0 || radius > 100_000.0 {
                    return Err("DXF circle radius is out of range".into());
                }
                if !self.points[0].valid() {
                    return Err("DXF circle centre is out of range".into());
                }
                Ok(Some(DxfShape::Circle {
                    center: self.points[0],
                    radius,
                }))
            }
            "ARC" => {
                if self.points.len() != 1 {
                    return Err("DXF arc needs one centre".into());
                }
                let radius = number(self.radius, "arc radius")?;
                let start_deg = number(self.start_deg, "arc start angle")?;
                let end_deg = number(self.end_deg, "arc end angle")?;
                if radius <= 0.0 || radius > 100_000.0 {
                    return Err("DXF arc radius is out of range".into());
                }
                if !self.points[0].valid() {
                    return Err("DXF arc centre is out of range".into());
                }
                Ok(Some(DxfShape::Arc {
                    center: self.points[0],
                    radius,
                    start_deg,
                    end_deg,
                }))
            }
            "LWPOLYLINE" | "POLYLINE" => {
                if self.points.len() < 2 || self.points.len() > 4096 {
                    return Err("DXF polylines need 2 to 4096 vertices".into());
                }
                if !self.points.iter().all(|p| p.valid()) {
                    return Err("DXF polyline vertex is out of range".into());
                }
                Ok(Some(DxfShape::Polyline {
                    points: self.points.clone(),
                    closed: self.closed,
                }))
            }
            "TEXT" => {
                if self.points.len() != 1 {
                    return Err("DXF text needs one anchor".into());
                }
                if !self.points[0].valid() {
                    return Err("DXF text anchor is out of range".into());
                }
                let height = number(self.radius, "text height")?;
                if height <= 0.0 || height > 10_000.0 {
                    return Err("DXF text height is out of range".into());
                }
                if self.value.len() > 1024 {
                    return Err("DXF text is too long".into());
                }
                Ok(Some(DxfShape::Text {
                    at: self.points[0],
                    height,
                    rotation: self.start_deg.unwrap_or(0.0),
                    value: self.value.clone(),
                }))
            }
            _ => Ok(None),
        }
    }
}

/// Read the drawable entities of an ASCII DXF drawing file. Entities on
/// unsupported entity types are skipped; malformed or oversized geometry is
/// rejected. Units are taken as millimetres, matching the exporter.
pub fn dxf_entities(text: &str) -> Result<Vec<DxfEntity>, String> {
    const MAX_ENTITIES: usize = 20_000;
    let mut out: Vec<DxfEntity> = Vec::new();
    let mut in_entities = false;
    let mut section = false;
    let mut acc: Option<DxfAcc> = None;
    let mut carried_layer: Option<String> = None;
    let flush = |acc: &mut Option<DxfAcc>, out: &mut Vec<DxfEntity>| -> Result<(), String> {
        let Some(a) = acc.take() else { return Ok(()) };
        if let Some(shape) = a.finish()? {
            if out.len() >= MAX_ENTITIES {
                return Err("DXF drawing exceeds 20000 entities".into());
            }
            out.push(DxfEntity {
                layer: a.layer.clone(),
                shape,
            });
        }
        Ok(())
    };
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(first) = lines.next() {
        let second = match lines.next() {
            Some(s) => s,
            None => return Err("DXF file has an odd number of lines".into()),
        };
        let Ok(what) = first.trim().parse::<i32>() else {
            return Err(format!("DXF group code \"{first}\" is not a number"));
        };
        let value = second.trim();
        match what {
            0 => {
                flush(&mut acc, &mut out)?;
                let name = value.to_string();
                if name == "SECTION" {
                    section = true;
                    in_entities = false;
                } else if name == "ENDSEC" || name == "ENDTAB" || name == "EOF" {
                    section = false;
                    in_entities = false;
                } else if in_entities && name != "ENTITY" {
                    acc = Some(DxfAcc {
                        layer: carried_layer.take().unwrap_or_default(),
                        name,
                        ..Default::default()
                    });
                }
            }
            2 if section => {
                in_entities = value == "ENTITIES";
                section = false;
            }
            _ if in_entities => {
                if acc.is_none() {
                    if what == 8 {
                        carried_layer = Some(value.to_string());
                    }
                    continue;
                }
                let Some(a) = acc.as_mut() else { continue };
                match what {
                    8 => a.layer = value.to_string(),
                    10 | 20 | 11 | 21 => {
                        let v = value
                            .parse::<f64>()
                            .ok()
                            .filter(|v| v.is_finite())
                            .ok_or_else(|| format!("DXF coordinate \"{value}\" is not a number"))?;
                        match what {
                            10 => a.pending_x = Some(v),
                            20 => a.pending_y = Some(v),
                            11 => a.end_x = Some(v),
                            _ => a.end_y = Some(v),
                        }
                        if let (Some(x), Some(y)) = (a.pending_x, a.pending_y) {
                            a.pending_x = None;
                            a.pending_y = None;
                            a.points.push(Point::new(x, y));
                        }
                        if let (Some(x), Some(y)) = (a.end_x, a.end_y) {
                            a.end_x = None;
                            a.end_y = None;
                            a.end = Some(Point::new(x, y));
                        }
                    }
                    40 => {
                        a.radius = value.parse::<f64>().ok().filter(|v| v.is_finite());
                    }
                    50 => {
                        a.start_deg = value.parse::<f64>().ok().filter(|v| v.is_finite());
                    }
                    51 => {
                        a.end_deg = value.parse::<f64>().ok().filter(|v| v.is_finite());
                    }
                    70 => {
                        a.closed = value
                            .parse::<i64>()
                            .map(|flags| flags & 1 == 1)
                            .unwrap_or(false);
                    }
                    1 => a.value = value.to_string(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    flush(&mut acc, &mut out)?;
    Ok(out)
}

pub fn svg(p: &Project, view: &str) -> Result<String, String> {
    p.validate()?;
    if view != "pcb" && view != "schematic" {
        return Err("view must be pcb or schematic".into());
    }
    let (min, max) = preview_bounds(p, view);
    let width = (max.x - min.x).max(1.0) + 16.0;
    let height = (max.y - min.y).max(1.0) + 16.0;
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000\" height=\"680\" viewBox=\"{} {} {width} {height}\"><rect x=\"{}\" y=\"{}\" width=\"{width}\" height=\"{height}\" fill=\"#131c29\"/><g font-family=\"sans-serif\" font-size=\"1.5\" fill=\"#dbe7f2\">",
        min.x - 8.0,
        min.y - 8.0,
        min.x - 8.0,
        min.y - 8.0
    );
    if view == "pcb" {
        match &p.board.outline {
            Some(o) => {
                s.push_str("<polygon points=\"");
                for pt in o {
                    s.push_str(&format!("{},{} ", pt.x, pt.y));
                }
                s.push_str("\" fill=\"#193a36\" stroke=\"#5ae0bc\" stroke-width=\"0.15\"/>");
            }
            None => s.push_str(&format!("<rect width=\"{}\" height=\"{}\" rx=\"0.4\" fill=\"#193a36\" stroke=\"#5ae0bc\" stroke-width=\"0.15\"/>",p.board.width,p.board.height)),
        }
        for c in &p.board.cutouts {
            s.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#131c29\" stroke=\"#5ae0bc\" stroke-width=\"0.1\"/>",c.center.x,c.center.y,c.radius));
        }
        for z in &p.board.zones {
            let color = if z.layer == pcbcraft_doc::Layer::Front {
                "#f58d79"
            } else {
                "#74a7ec"
            };
            let points: Vec<String> = z
                .outline
                .iter()
                .map(|pt| format!("{} {}", pt.x, pt.y))
                .collect();
            s.push_str(&format!(
                "<polygon points=\"{}\" fill=\"{color}\" fill-opacity=\"{}\" stroke=\"{color}\" stroke-width=\"0.1\"{} />",
                points.join(" "),
                if z.filled { "0.3" } else { "0" },
                if z.filled { "" } else { " stroke-dasharray=\"0.6 0.4\"" },
            ));
            for cut in &z.cutouts {
                let cut_points: Vec<String> =
                    cut.iter().map(|pt| format!("{} {}", pt.x, pt.y)).collect();
                s.push_str(&format!(
                    "<polygon points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"0.1\" stroke-dasharray=\"0.6 0.3\"/>",
                    cut_points.join(" ")
                ));
            }
        }
        for t in &p.board.tracks {
            let color = if t.layer == pcbcraft_doc::Layer::Front {
                "#f58d79"
            } else {
                "#74a7ec"
            };
            let d = match t.curve() {
                Some(arc) => format!(
                    "M {} {} A {:.6} {:.6} 0 {} {} {} {}",
                    t.start.x,
                    t.start.y,
                    arc.radius,
                    arc.radius,
                    usize::from(arc.sweep.abs() > std::f64::consts::PI),
                    usize::from(arc.sweep < 0.0),
                    t.end.x,
                    t.end.y
                ),
                None => format!("M {} {} L {} {}", t.start.x, t.start.y, t.end.x, t.end.y),
            };
            s.push_str(&format!("<path d=\"{d}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",t.width));
        }
        for v in &p.board.vias {
            s.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#f58d79\" stroke=\"#f58d79\" stroke-width=\"0.05\"/>",v.position.x,v.position.y,v.diameter/2.0));
            if v.drill > 0.0 {
                s.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#131c29\"/>",
                    v.position.x,
                    v.position.y,
                    v.drill / 2.0
                ));
            }
        }
        for t in &p.board.texts {
            let spin = if t.rotation == 0.0 {
                String::new()
            } else {
                format!(
                    " transform=\"rotate({} {} {})\"",
                    -t.rotation, t.position.x, t.position.y
                )
            };
            s.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{}\"{}>{}</text>",
                t.position.x,
                t.position.y,
                t.size,
                if t.layer == pcbcraft_doc::Layer::Front {
                    "#dbe7f2"
                } else {
                    "#9ec5ff"
                },
                spin,
                escape(&board_text(p, t))
            ));
        }
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            s.push_str(&project_graphic_svg(g));
        }
        for f in &p.board.footprints {
            if let Some(library) = p
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
            {
                s.push_str(&format!(
                    "<g transform=\"translate({} {}) rotate({})\">",
                    f.position.x, f.position.y, -f.rotation
                ));
                for g in &library.definition.graphics {
                    let mut g = g.clone();
                    if let pcbcraft_library::Graphic::Text { text, .. } = &mut g
                        && let Some(c) = p.component(f.component)
                    {
                        *text = text
                            .replace("${REFERENCE}", &c.reference)
                            .replace("${VALUE}", &c.value)
                            .replace("%R", &c.reference)
                            .replace("%V", &c.value);
                    }
                    s.push_str(&svg_graphic(&g));
                }
                s.push_str("</g>");
            }
            for pad in &f.pads {
                let points = pad
                    .outline_points()
                    .into_iter()
                    .map(|pos| f.position.offset(pos.rotate(-f.rotation)))
                    .collect::<Vec<_>>();
                s.push_str(&svg_path(&points, "#e6c27d", 0.0, "#e6c27d"));
                let drill = pad
                    .drill_outline_points()
                    .into_iter()
                    .map(|pos| f.position.offset(pos.rotate(-f.rotation)))
                    .collect::<Vec<_>>();
                if !drill.is_empty() {
                    s.push_str(&format!(
                        "<g data-drill=\"{}\">{}</g>",
                        escape(&pad.number),
                        svg_path(&drill, "#131c29", 0.0, "#131c29")
                    ));
                }
            }
            if let Some(c) = p.component(f.component) {
                let (min, max) = footprint_bounds(p, f);
                let label_x = (min.x + max.x) / 2.0;
                for (label, y) in [(&c.reference, min.y - 1.2), (&c.value, max.y + 1.2)] {
                    let point = f
                        .position
                        .offset(Point::new(label_x, y).rotate(-f.rotation));
                    s.push_str(&format!("<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"middle\" font-size=\"1\" transform=\"rotate({} {} {})\">{}</text>",point.x,point.y,-f.rotation,point.x,point.y,escape(label)));
                }
            }
        }
    } else {
        for w in &p.wires {
            if let Some(path) = p.wire_path(w) {
                let points = path
                    .iter()
                    .map(|p| format!("{},{}", p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ");
                s.push_str(&format!("<polyline points=\"{points}\" fill=\"none\" stroke=\"#5ae0bc\" stroke-width=\"0.2\"/>"));
            }
        }
        for w in &p.drawn_wires {
            let points = w
                .points
                .iter()
                .map(|p| format!("{},{}", p.x, p.y))
                .collect::<Vec<_>>()
                .join(" ");
            s.push_str(&format!("<polyline points=\"{points}\" fill=\"none\" stroke=\"#5ae0bc\" stroke-width=\"0.2\"/>"));
        }
        for g in p.graphics.iter().filter(|g| g.layer == "schematic") {
            s.push_str(&project_graphic_svg(g));
        }
        for l in &p.labels {
            if l.kind == "junction" {
                s.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"0.35\" fill=\"#5ae0bc\"/>",
                    l.position.x, l.position.y
                ));
            } else {
                s.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\">{}</text>",
                    l.position.x,
                    l.position.y,
                    escape(&label_text(l, p.net_name(l.net)))
                ));
            }
        }
        for f in &p.no_connects {
            if let Some(point) = p.pin_position(&f.pin) {
                s.push_str(&format!("<path d=\"M {} {} L {} {} M {} {} L {} {}\" stroke=\"#f58d79\" stroke-width=\"0.25\"/>",point.x-0.6,point.y-0.6,point.x+0.6,point.y+0.6,point.x-0.6,point.y+0.6,point.x+0.6,point.y-0.6));
            }
        }
        for c in p.components.iter().flat_map(|c| c.schematic_views()) {
            s.push_str(&format!(
                "<g transform=\"translate({} {}) rotate({})\">",
                c.position.x, c.position.y, -c.rotation
            ));
            if let Some(library) = &c.library_symbol {
                for g in library
                    .definition
                    .graphics
                    .iter()
                    .filter(|g| (g.unit == 0 || g.unit == library.unit) && g.body_style <= 1)
                {
                    s.push_str(&svg_graphic(&g.primitive));
                }
                for pin in library.definition.pins.iter().filter(|pin| {
                    (pin.unit == 0 && library.unit == 1 || pin.unit == library.unit)
                        && pin.body_style <= 1
                }) {
                    let angle = pin.angle.to_radians();
                    let end = pin.position.offset(Point::new(
                        pin.length * angle.cos(),
                        -pin.length * angle.sin(),
                    ));
                    s.push_str(&svg_path(&[pin.position, end], "#dbe7f2", 0.15, "none"));
                    s.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" font-size=\"0.9\">{}</text>",
                        pin.position.x + 0.3,
                        pin.position.y - 0.3,
                        escape(&pin.number)
                    ));
                }
            } else {
                s.push_str("<rect x=\"-3\" y=\"-1.5\" width=\"6\" height=\"3\" fill=\"#1e2c40\" stroke=\"#dbe7f2\" stroke-width=\"0.15\"/><path d=\"M -5 0 H -3 M 3 0 H 5\" stroke=\"#dbe7f2\" stroke-width=\"0.2\"/>");
            }
            s.push_str(&format!(
                "</g><text x=\"{}\" y=\"{}\" text-anchor=\"middle\">{} · {}</text>",
                c.position.x,
                c.position.y - 3.0,
                escape(&c.reference),
                escape(&c.value)
            ));
        }
    }
    s.push_str("</g></svg>");
    Ok(s)
}

/// One drawable entity read from an SVG drawing. Coordinates are board-space
/// millimetres, y up, normalised by the importer.
#[derive(Clone, Debug, PartialEq)]
pub enum SvgShape {
    Line {
        start: Point,
        end: Point,
    },
    Polyline {
        points: Vec<Point>,
        closed: bool,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Text {
        at: Point,
        height: f64,
        value: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct SvgEntity {
    /// The element's stroke colour, kept only so importers can honour layer hints.
    pub layer: String,
    pub shape: SvgShape,
}
/// Parse one attribute value out of a tag body, honouring word boundaries so
/// `x1` never satisfies a search for `x`.
fn svg_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let bytes = tag.as_bytes();
    let mut from = 0usize;
    while let Some(offset) = tag[from..].find(name) {
        let start = from + offset;
        let end = start + name.len();
        let before_ok = start == 0
            || !bytes[start - 1].is_ascii_alphanumeric()
                && bytes[start - 1] != b'-'
                && bytes[start - 1] != b':';
        let mut after = end;
        while after < tag.len() && tag.as_bytes()[after].is_ascii_whitespace() {
            after += 1;
        }
        if before_ok && tag.as_bytes().get(after) == Some(&b'=') {
            let quote = tag.as_bytes().get(after + 1).copied()?;
            if quote != b'"' && quote != b'\'' {
                return None;
            }
            let value_start = after + 2;
            let value_end = tag[value_start..].find(quote as char)? + value_start;
            return Some(&tag[value_start..value_end]);
        }
        from = end;
    }
    None
}
/// Read a numeric attribute, converting the common physical units to millimetres.
fn svg_length(tag: &str, name: &str) -> Result<Option<f64>, String> {
    let Some(raw) = svg_attr(tag, name) else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    let digits = trimmed
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(digits);
    let value: f64 = number
        .parse()
        .map_err(|_| format!("SVG attribute {name} is not a number"))?;
    if !value.is_finite() {
        return Err(format!("SVG attribute {name} is not finite"));
    }
    let scale: f64 = match unit.trim() {
        "" | "px" => 25.4 / 96.0,
        "mm" => 1.0,
        "cm" => 10.0,
        "in" => 25.4,
        "pt" => 25.4 / 72.0,
        other => {
            return Err(format!(
                "SVG attribute {name} has unsupported unit \"{other}\""
            ));
        }
    };
    Ok(Some(value * scale))
}
/// Undo the five predefined XML entities.
fn svg_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let replaced = match tail.strip_prefix("amp;") {
            Some(t) => Some(('&', t)),
            None => match tail.strip_prefix("lt;") {
                Some(t) => Some(('<', t)),
                None => match tail.strip_prefix("gt;") {
                    Some(t) => Some(('>', t)),
                    None => match tail.strip_prefix("quot;") {
                        Some(t) => Some(('"', t)),
                        None => tail.strip_prefix("apos;").map(|t| ('\'', t)),
                    },
                },
            },
        };
        match replaced {
            Some((ch, t)) => {
                out.push(ch);
                rest = t;
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}
fn svg_point(x: f64, y: f64, height: f64) -> Result<Point, String> {
    let p = Point::new(x, height - y);
    if !p.valid() {
        return Err("SVG coordinates are out of range".into());
    }
    Ok(p)
}
fn svg_numbers(list: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for piece in list
        .split([' ', ',', '\t', '\n', '\r'])
        .filter(|p| !p.is_empty())
    {
        let value: f64 = piece
            .parse()
            .map_err(|_| format!("SVG coordinate list holds \"{piece}\""))?;
        if !value.is_finite() {
            return Err("SVG coordinate list holds a non-finite number".into());
        }
        out.push(value);
    }
    Ok(out)
}
/// Split path data into command letters and numbers, including the sign-flush
/// form where "-7" starts a new number without a separator.
fn svg_path_tokens(d: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in d.chars() {
        if c.is_ascii_alphabetic() || c == ',' || c.is_ascii_whitespace() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            if c.is_ascii_alphabetic() {
                out.push(c.to_string());
            }
        } else if c == '-' || c == '+' {
            if current.ends_with('e') || current.ends_with('E') {
                current.push(c);
            } else {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                current.push(c);
            }
        } else if c == '.'
            && current.contains('.')
            && !current.ends_with('e')
            && !current.ends_with('E')
        {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            current.push(c);
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
const SVG_MAX_POINTS: usize = 256;
/// Execute a path's d commands into subpaths. Lines, moves, axis-aligned
/// segments and closes are honoured; curves and arcs are rejected outright so
/// an importer never silently drops geometry.
fn svg_path_polylines(d: &str, height: f64) -> Result<Vec<(Vec<Point>, bool)>, String> {
    let tokens = svg_path_tokens(d);
    let mut subs: Vec<(Vec<Point>, bool)> = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut pen = Point::default();
    let mut origin = Point::default();
    let mut index = 0usize;
    let mut command = ' ';
    let number = |tokens: &Vec<String>, i: &mut usize| -> Result<f64, String> {
        let value: f64 = tokens
            .get(*i)
            .ok_or_else(|| "SVG path ends mid-command".to_string())?
            .parse()
            .map_err(|_| "SVG path holds a non-number".to_string())?;
        if !value.is_finite() {
            return Err("SVG path holds a non-finite number".into());
        }
        *i += 1;
        Ok(value)
    };
    while index < tokens.len() {
        if let Some(letter) = tokens[index]
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
        {
            command = letter;
            index += 1;
        }
        match command {
            'M' | 'm' => {
                let x = number(&tokens, &mut index)?;
                let y = number(&tokens, &mut index)?;
                pen = if command == 'M' {
                    Point::new(x, y)
                } else {
                    Point::new(pen.x + x, pen.y + y)
                };
                if current.len() > 1 {
                    subs.push((std::mem::take(&mut current), false));
                } else {
                    current.clear();
                }
                origin = pen;
                current.push(pen);
                command = if command == 'M' { 'L' } else { 'l' };
            }
            'L' | 'l' => {
                let x = number(&tokens, &mut index)?;
                let y = number(&tokens, &mut index)?;
                pen = if command == 'L' {
                    Point::new(x, y)
                } else {
                    Point::new(pen.x + x, pen.y + y)
                };
                if current.len() >= SVG_MAX_POINTS {
                    return Err("SVG path subpath exceeds 256 points".into());
                }
                current.push(pen);
            }
            'H' | 'h' => {
                let x = number(&tokens, &mut index)?;
                pen = Point::new(if command == 'H' { x } else { pen.x + x }, pen.y);
                if current.len() >= SVG_MAX_POINTS {
                    return Err("SVG path subpath exceeds 256 points".into());
                }
                current.push(pen);
            }
            'V' | 'v' => {
                let y = number(&tokens, &mut index)?;
                pen = Point::new(pen.x, if command == 'V' { y } else { pen.y + y });
                if current.len() >= SVG_MAX_POINTS {
                    return Err("SVG path subpath exceeds 256 points".into());
                }
                current.push(pen);
            }
            'Z' | 'z' => {
                if current.len() > 1 {
                    pen = origin;
                    current.push(pen);
                    subs.push((std::mem::take(&mut current), true));
                } else {
                    current.clear();
                }
            }
            other => {
                return Err(format!(
                    "SVG path command '{other}' is not supported, only M L H V Z"
                ));
            }
        }
    }
    if current.len() > 1 {
        subs.push((current, false));
    }
    subs.into_iter()
        .map(|(points, closed)| -> Result<(Vec<Point>, bool), String> {
            Ok((
                points
                    .iter()
                    .map(|q| svg_point(q.x, q.y, height))
                    .collect::<Result<Vec<_>, _>>()?,
                closed,
            ))
        })
        .collect::<Result<Vec<_>, _>>()
}
/// Read the drawable entities of an SVG drawing as board graphics material.
/// Lines, polylines, polygons, rectangles, circles, simple paths and text are
/// honoured; groups are descended into and unsupported elements are skipped.
/// The page height from the root tag flips y so the drawing lands y-up, and
/// units convert to millimetres.
pub fn svg_entities(text: &str) -> Result<Vec<SvgEntity>, String> {
    const MAX_ENTITIES: usize = 512;
    if text.len() > 1_048_576 {
        return Err("SVG drawing exceeds 1 MiB".into());
    }
    let open = text
        .find("<svg")
        .ok_or_else(|| "not an SVG drawing: no <svg> root".to_string())?;
    let open_end = text[open..]
        .find('>')
        .ok_or_else(|| "unclosed <svg> tag".to_string())?;
    let root = &text[open..open + open_end];
    let height = match svg_length(root, "height")? {
        Some(h) => h,
        None => match svg_attr(root, "viewBox").map(svg_numbers) {
            Some(Ok(v)) if v.len() >= 4 => v[3],
            _ => 100.0,
        },
    };
    if !(height.is_finite() && height > 0.0 && height <= 100_000.0) {
        return Err("SVG page height is out of range".into());
    }
    let mut out: Vec<SvgEntity> = Vec::new();
    let mut depth = 0usize;
    let mut cursor = open + open_end + 1;
    let push = |shape: SvgShape, layer: String, out: &mut Vec<SvgEntity>| -> Result<(), String> {
        if out.len() >= MAX_ENTITIES {
            return Err("SVG drawing exceeds 512 entities".into());
        }
        out.push(SvgEntity { layer, shape });
        Ok(())
    };
    while let Some(lt) = text[cursor..].find('<') {
        let at = cursor + lt;
        let rest = &text[at..];
        if rest.starts_with("<!--") {
            let end = rest
                .find("-->")
                .ok_or_else(|| "unterminated SVG comment".to_string())?;
            cursor = at + end + 3;
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            let end = rest
                .find('>')
                .ok_or_else(|| "unclosed SVG tag".to_string())?;
            cursor = at + end + 1;
            continue;
        }
        if rest.starts_with("</") {
            let end = rest
                .find('>')
                .ok_or_else(|| "unclosed SVG tag".to_string())?;
            depth = depth.saturating_sub(1);
            cursor = at + end + 1;
            continue;
        }
        let end = rest
            .find('>')
            .ok_or_else(|| "unclosed SVG tag".to_string())?;
        let self_closed = rest.as_bytes()[end - 1] == b'/';
        let tag = rest[1..end - if self_closed { 1 } else { 0 }].trim_start();
        let name_len = tag
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == ':'))
            .unwrap_or(tag.len());
        let name = tag[..name_len].to_ascii_lowercase();
        let inside = depth == 0;
        if !self_closed {
            depth += 1;
        }
        cursor = at + end + 1;
        if !inside {
            continue;
        }
        let stroke = svg_attr(tag, "stroke").unwrap_or("");
        let layer = if stroke.eq_ignore_ascii_case("none") {
            String::new()
        } else {
            stroke.to_string()
        };
        match name.as_str() {
            "line" => {
                let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                    svg_length(tag, "x1")?,
                    svg_length(tag, "y1")?,
                    svg_length(tag, "x2")?,
                    svg_length(tag, "y2")?,
                ) else {
                    return Err("SVG line needs x1 y1 x2 y2".into());
                };
                push(
                    SvgShape::Line {
                        start: svg_point(x1, y1, height)?,
                        end: svg_point(x2, y2, height)?,
                    },
                    layer,
                    &mut out,
                )?;
            }
            "polyline" | "polygon" => {
                let raw = svg_attr(tag, "points")
                    .ok_or_else(|| "SVG polygon has no points".to_string())?;
                let numbers = svg_numbers(raw)?;
                if numbers.len() < 4 || numbers.len() % 2 != 0 {
                    return Err("SVG points list needs at least two pairs".into());
                }
                if numbers.len() / 2 > SVG_MAX_POINTS {
                    return Err("SVG polygon exceeds 256 points".into());
                }
                let points = numbers
                    .chunks(2)
                    .map(|q| svg_point(q[0], q[1], height))
                    .collect::<Result<Vec<_>, _>>()?;
                push(
                    SvgShape::Polyline {
                        points,
                        closed: name == "polygon",
                    },
                    layer,
                    &mut out,
                )?;
            }
            "rect" => {
                let x = svg_length(tag, "x")?.unwrap_or(0.0);
                let y = svg_length(tag, "y")?.unwrap_or(0.0);
                let (Some(w), Some(h)) = (svg_length(tag, "width")?, svg_length(tag, "height")?)
                else {
                    return Err("SVG rect needs width and height".into());
                };
                if w <= 0.0 || h <= 0.0 || w > 100_000.0 || h > 100_000.0 {
                    return Err("SVG rect size is out of range".into());
                }
                let corners = [
                    svg_point(x, y, height)?,
                    svg_point(x + w, y, height)?,
                    svg_point(x + w, y + h, height)?,
                    svg_point(x, y + h, height)?,
                ];
                push(
                    SvgShape::Polyline {
                        points: corners.to_vec(),
                        closed: true,
                    },
                    layer,
                    &mut out,
                )?;
            }
            "circle" => {
                let cx = svg_length(tag, "cx")?.ok_or_else(|| "SVG circle needs cx".to_string())?;
                let cy = svg_length(tag, "cy")?.ok_or_else(|| "SVG circle needs cy".to_string())?;
                let r = svg_length(tag, "r")?.ok_or_else(|| "SVG circle needs r".to_string())?;
                if r <= 0.0 || r > 1_000.0 {
                    return Err("SVG circle radius is out of range".into());
                }
                push(
                    SvgShape::Circle {
                        center: svg_point(cx, cy, height)?,
                        radius: r,
                    },
                    layer,
                    &mut out,
                )?;
            }
            "path" => {
                let d = svg_attr(tag, "d").ok_or_else(|| "SVG path has no d".to_string())?;
                for (points, closed) in svg_path_polylines(d, height)? {
                    if points.len() < 2 {
                        continue;
                    }
                    push(
                        SvgShape::Polyline { points, closed },
                        layer.to_string(),
                        &mut out,
                    )?;
                }
            }
            "text" => {
                let x = svg_length(tag, "x")?.unwrap_or(0.0);
                let y = svg_length(tag, "y")?.unwrap_or(0.0);
                let size = svg_length(tag, "font-size")?.unwrap_or(2.5);
                if size <= 0.0 || size > 1_000.0 {
                    return Err("SVG text size is out of range".into());
                }
                let close = text[cursor..]
                    .find("</text>")
                    .ok_or_else(|| "unclosed SVG text".to_string())?;
                let value = svg_unescape(&text[cursor..cursor + close]);
                cursor += close + 7;
                depth = depth.saturating_sub(1);
                if value.len() > 128 {
                    return Err("SVG text is too long".into());
                }
                push(
                    SvgShape::Text {
                        at: svg_point(x, y, height)?,
                        height: size,
                        value,
                    },
                    layer,
                    &mut out,
                )?;
            }
            "g" => {}
            _ => {}
        }
    }
    Ok(out)
}

/// One component inside a sheet file, placed from a built-in symbol template.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SheetComponent {
    pub kind: String,
    pub reference: Option<String>,
    pub value: Option<String>,
    pub position: Point,
    pub rotation: f64,
    pub fields: Vec<(String, String)>,
}
/// One hierarchy port inside a sheet file; the net is named, never an id.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SheetPort {
    pub name: String,
    pub position: Point,
    pub net: Option<String>,
}
/// A schematic page written as JSON that imports as a new sheet.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SheetFile {
    pub name: Option<String>,
    pub components: Vec<SheetComponent>,
    pub ports: Vec<SheetPort>,
}

const SHEET_MAX_COMPONENTS: usize = 256;
const SHEET_MAX_PORTS: usize = 64;
const SHEET_MAX_FIELDS: usize = 32;

fn sheet_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
    what: &str,
) -> Result<(), String> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("{what} has an unexpected key {key}"));
        }
    }
    Ok(())
}

fn sheet_opt_text<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
    max: usize,
    what: &str,
) -> Result<Option<&'a str>, String> {
    match object.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) if text.len() <= max => Ok(Some(text.as_str())),
        Some(_) => Err(format!(
            "{what} has a {key} that is not text or is longer than {max} bytes"
        )),
    }
}

fn sheet_text<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
    max: usize,
    what: &str,
) -> Result<&'a str, String> {
    sheet_opt_text(object, key, max, what)?
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("{what} has no {key}"))
}

fn sheet_point(value: &serde_json::Value, what: &str) -> Result<Point, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{what} has a position that is not an object"))?;
    sheet_keys(object, &["x", "y"], &format!("{what} position"))?;
    let number = |key: &str| -> Result<f64, String> {
        object
            .get(key)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("{what} has a position {key} that is not a number"))
    };
    let point = Point::new(number("x")?, number("y")?);
    if !point.valid() {
        return Err(format!("{what} has a position outside the allowed range"));
    }
    Ok(point)
}

fn sheet_rotation(
    object: &serde_json::Map<String, serde_json::Value>,
    what: &str,
) -> Result<f64, String> {
    match object.get("rotation") {
        None | Some(serde_json::Value::Null) => Ok(0.0),
        Some(serde_json::Value::Number(n)) => n
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{what} has a rotation that is not a finite number")),
        Some(_) => Err(format!("{what} has a rotation that is not a number")),
    }
}

/// Read a schematic page written as JSON: components placed from the built-in
/// symbol templates plus hierarchy ports named by their net. Unknown keys are
/// refused and every count, name and position is bounded so a typo fails
/// loudly instead of importing half a page.
pub fn sheet_file(text: &str) -> Result<SheetFile, String> {
    if text.len() > 1 << 20 {
        return Err("sheet file exceeds 1 MiB".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("sheet file is not valid JSON: {e}"))?;
    let root = value
        .as_object()
        .ok_or_else(|| "sheet file is not a JSON object".to_string())?;
    sheet_keys(root, &["name", "components", "ports"], "the sheet file")?;
    let mut out = SheetFile {
        name: sheet_opt_text(root, "name", 128, "the sheet file")?.map(str::to_owned),
        ..Default::default()
    };
    match root.get("components") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Array(items)) if items.len() <= SHEET_MAX_COMPONENTS => {
            for item in items {
                let object = item
                    .as_object()
                    .ok_or_else(|| "a component is not a JSON object".to_string())?;
                sheet_keys(
                    object,
                    &[
                        "kind",
                        "reference",
                        "value",
                        "position",
                        "rotation",
                        "fields",
                    ],
                    "a component",
                )?;
                let position = object
                    .get("position")
                    .ok_or_else(|| "a component has no position".to_string())
                    .and_then(|v| sheet_point(v, "a component"))?;
                let mut fields: Vec<(String, String)> = Vec::new();
                match object.get("fields") {
                    None | Some(serde_json::Value::Null) => {}
                    Some(serde_json::Value::Array(list)) if list.len() <= SHEET_MAX_FIELDS => {
                        let mut keys: Vec<String> = Vec::new();
                        for entry in list {
                            let object = entry
                                .as_object()
                                .ok_or_else(|| "a field is not a JSON object".to_string())?;
                            sheet_keys(object, &["key", "value"], "a field")?;
                            let key = sheet_text(object, "key", 64, "a field")?;
                            if keys.iter().any(|k| k == key) {
                                return Err(format!(
                                    "field {key} is written twice on one component"
                                ));
                            }
                            keys.push(key.to_owned());
                            let value =
                                sheet_opt_text(object, "value", 256, "a field")?.unwrap_or("");
                            fields.push((key.to_owned(), value.to_owned()));
                        }
                    }
                    Some(serde_json::Value::Array(_)) => {
                        return Err("a component carries more than 32 fields".into());
                    }
                    Some(_) => return Err("component fields are not an array".into()),
                }
                out.components.push(SheetComponent {
                    kind: sheet_text(object, "kind", 64, "a component")?.to_owned(),
                    reference: sheet_opt_text(object, "reference", 64, "a component")?
                        .filter(|text| !text.is_empty())
                        .map(str::to_owned),
                    value: sheet_opt_text(object, "value", 256, "a component")?.map(str::to_owned),
                    position,
                    rotation: sheet_rotation(object, "a component")?,
                    fields,
                });
            }
        }
        Some(serde_json::Value::Array(_)) => {
            return Err("the sheet file holds more than 256 components".into());
        }
        Some(_) => return Err("sheet components are not an array".into()),
    }
    match root.get("ports") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Array(items)) if items.len() <= SHEET_MAX_PORTS => {
            for item in items {
                let object = item
                    .as_object()
                    .ok_or_else(|| "a port is not a JSON object".to_string())?;
                sheet_keys(object, &["name", "position", "net"], "a port")?;
                let name = sheet_text(object, "name", 63, "a port")?;
                if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                    return Err("a port name must be 1 to 63 letters, digits or underscores".into());
                }
                let position = object
                    .get("position")
                    .ok_or_else(|| "a port has no position".to_string())
                    .and_then(|v| sheet_point(v, "a port"))?;
                let net = sheet_opt_text(object, "net", 128, "a port")?
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned);
                out.ports.push(SheetPort {
                    name: name.to_owned(),
                    position,
                    net,
                });
            }
        }
        Some(serde_json::Value::Array(_)) => {
            return Err("the sheet file holds more than 64 ports".into());
        }
        Some(_) => return Err("sheet ports are not an array".into()),
    }
    if out.components.is_empty() && out.ports.is_empty() {
        return Err("the sheet file has nothing to import".into());
    }
    Ok(out)
}

/// A bounded CPU preview for headless MCP. The desktop has its own actual UI capture.
pub fn preview_png(p: &Project, view: &str) -> Result<Vec<u8>, String> {
    p.validate()?;
    if view != "pcb" && view != "schematic" {
        return Err("view must be pcb or schematic".into());
    }
    let mut image = image::RgbImage::from_pixel(1000, 680, image::Rgb([19, 28, 41]));
    let (mut min, mut max) = preview_bounds(p, view);
    let scale = if view == "pcb" {
        (880.0 / p.board.width).min(560.0 / p.board.height)
    } else {
        min = min.offset(Point::new(-4.0, -4.0));
        max = max.offset(Point::new(4.0, 4.0));
        (880.0 / (max.x - min.x).max(1.0)).min(560.0 / (max.y - min.y).max(1.0))
    };
    let to_pixel =
        |p: Point| Point::new(60.0 + (p.x - min.x) * scale, 60.0 + (p.y - min.y) * scale);
    if view == "pcb" {
        let w = (p.board.width * scale).min(880.0) as u32;
        let h = (p.board.height * scale).min(560.0) as u32;
        for x in 60..60 + w {
            for y in 60..60 + h {
                image.put_pixel(x, y, image::Rgb([25, 58, 54]));
            }
        }
        for c in &p.board.cutouts {
            disk(
                &mut image,
                to_pixel(c.center),
                c.radius * scale,
                [19, 28, 41],
            );
        }
        for z in &p.board.zones {
            let color = if z.layer == pcbcraft_doc::Layer::Front {
                [245, 141, 121]
            } else {
                [116, 167, 236]
            };
            let points: Vec<Point> = z.outline.iter().map(|pt| to_pixel(*pt)).collect();
            if z.filled {
                fill_polygon(&mut image, &points, color);
            } else {
                for w in points.windows(2) {
                    line(&mut image, w[0], w[1], 0.6, color);
                }
            }
            for cut in &z.cutouts {
                let points: Vec<Point> = cut.iter().map(|pt| to_pixel(*pt)).collect();
                fill_polygon(&mut image, &points, [25, 58, 54]);
            }
        }
        for t in &p.board.tracks {
            let color = if t.layer == pcbcraft_doc::Layer::Front {
                [245, 141, 121]
            } else {
                [116, 167, 236]
            };
            match t.curve() {
                Some(arc) => {
                    let steps = 16;
                    for i in 0..steps {
                        let a = to_pixel(arc.point_at(i as f64 / steps as f64));
                        let b = to_pixel(arc.point_at((i + 1) as f64 / steps as f64));
                        line(&mut image, a, b, t.width * scale / 2.0, color);
                    }
                }
                None => {
                    line(
                        &mut image,
                        to_pixel(t.start),
                        to_pixel(t.end),
                        t.width * scale / 2.0,
                        color,
                    );
                }
            }
        }
        for v in &p.board.vias {
            let point = to_pixel(v.position);
            let radius = (v.diameter / 2.0 * scale).max(1.0);
            line(
                &mut image,
                Point::new(point.x - radius, point.y),
                Point::new(point.x + radius, point.y),
                radius * 2.0,
                [245, 141, 121],
            );
            if v.drill > 0.0 {
                let hole = (v.drill / 2.0 * scale).max(0.5);
                line(
                    &mut image,
                    Point::new(point.x - hole, point.y),
                    Point::new(point.x + hole, point.y),
                    hole * 2.0,
                    [19, 28, 41],
                );
            }
        }
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            for pair in g.outline().unwrap_or_default().windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(&mut image, to_pixel(*a), to_pixel(*b), 1.0, [220, 220, 220]);
                }
            }
        }
        for f in &p.board.footprints {
            if let Some(library) = p
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
            {
                for g in &library.definition.graphics {
                    png_graphic(
                        &mut image,
                        g,
                        |point| to_pixel(f.position.offset(point.rotate(-f.rotation))),
                        scale,
                    );
                }
            }
            for pad in &f.pads {
                let points = pad
                    .outline_points()
                    .into_iter()
                    .map(|pos| to_pixel(f.position.offset(pos.rotate(-f.rotation))))
                    .collect::<Vec<_>>();
                polygon(&mut image, &points, [230, 194, 125]);
                let drill = pad
                    .drill_outline_points()
                    .into_iter()
                    .map(|pos| to_pixel(f.position.offset(pos.rotate(-f.rotation))))
                    .collect::<Vec<_>>();
                polygon(&mut image, &drill, [19, 28, 41]);
            }
        }
    } else {
        for w in &p.wires {
            if let Some(points) = p.wire_path(w) {
                for pair in points.windows(2) {
                    if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                        line(&mut image, to_pixel(*a), to_pixel(*b), 1.0, [90, 224, 188]);
                    }
                }
            }
        }
        for w in &p.drawn_wires {
            for pair in w.points.windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(&mut image, to_pixel(*a), to_pixel(*b), 1.0, [90, 224, 188]);
                }
            }
        }
        for g in p.graphics.iter().filter(|g| g.layer == "schematic") {
            for pair in g.outline().unwrap_or_default().windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(&mut image, to_pixel(*a), to_pixel(*b), 1.0, [90, 224, 188]);
                }
            }
        }
        for l in p.labels.iter().filter(|l| l.kind == "junction") {
            let point = to_pixel(l.position);
            line(
                &mut image,
                Point::new(point.x - 2.0, point.y),
                Point::new(point.x + 2.0, point.y),
                2.0,
                [90, 224, 188],
            );
        }
        for f in &p.no_connects {
            if let Some(anchor) = p.pin_position(&f.pin) {
                let point = to_pixel(anchor);
                line(
                    &mut image,
                    Point::new(point.x - 3.0, point.y - 3.0),
                    Point::new(point.x + 3.0, point.y + 3.0),
                    1.0,
                    [245, 141, 121],
                );
                line(
                    &mut image,
                    Point::new(point.x - 3.0, point.y + 3.0),
                    Point::new(point.x + 3.0, point.y - 3.0),
                    1.0,
                    [245, 141, 121],
                );
            }
        }
        for c in p.components.iter().flat_map(|c| c.schematic_views()) {
            if let Some(library) = &c.library_symbol {
                for g in library
                    .definition
                    .graphics
                    .iter()
                    .filter(|g| (g.unit == 0 || g.unit == library.unit) && g.body_style <= 1)
                {
                    png_graphic(
                        &mut image,
                        &g.primitive,
                        |point| to_pixel(c.position.offset(point.rotate(-c.rotation))),
                        scale,
                    );
                }
                for pin in library.definition.pins.iter().filter(|pin| {
                    (pin.unit == 0 && library.unit == 1 || pin.unit == library.unit)
                        && pin.body_style <= 1
                }) {
                    if !pin.visible {
                        continue;
                    }
                    let end = pin.stem_end();
                    line(
                        &mut image,
                        to_pixel(c.position.offset(pin.position.rotate(-c.rotation))),
                        to_pixel(c.position.offset(end.rotate(-c.rotation))),
                        0.7,
                        [219, 231, 242],
                    );
                    schematic_pin_decorations_png(&mut image, pin, &|point| {
                        to_pixel(c.position.offset(point.rotate(-c.rotation)))
                    });
                }
                continue;
            }
            let outline = [
                Point::new(-3.0, -1.5),
                Point::new(3.0, -1.5),
                Point::new(3.0, 1.5),
                Point::new(-3.0, 1.5),
                Point::new(-3.0, -1.5),
            ];
            for pair in outline.windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(
                        &mut image,
                        to_pixel(c.position.offset(a.rotate(-c.rotation))),
                        to_pixel(c.position.offset(b.rotate(-c.rotation))),
                        1.0,
                        [219, 231, 242],
                    );
                }
            }
            for pin in &c.pins {
                line(
                    &mut image,
                    to_pixel(c.position.offset(pin.offset.rotate(-c.rotation))),
                    to_pixel(
                        c.position.offset(
                            Point::new(pin.offset.x.signum() * 3.0, 0.0).rotate(-c.rotation),
                        ),
                    ),
                    1.0,
                    [219, 231, 242],
                );
            }
        }
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(bytes.into_inner())
}
fn disk(image: &mut image::RgbImage, p: Point, r: f64, color: [u8; 3]) {
    let radius = r.clamp(0.7, 1200.0);
    let left = (p.x - radius).max(0.0) as u32;
    let right = (p.x + radius).min(999.0) as u32;
    let top = (p.y - radius).max(0.0) as u32;
    let bottom = (p.y + radius).min(679.0) as u32;
    for x in left..=right {
        for y in top..=bottom {
            if Point::new(x as f64, y as f64).distance(p) <= radius {
                image.put_pixel(x, y, image::Rgb(color));
            }
        }
    }
}
fn line(image: &mut image::RgbImage, a: Point, b: Point, r: f64, color: [u8; 3]) {
    // Pixel-space bounding scan clips hostile/out-of-view geometry without long stepping loops.
    let radius = r.clamp(0.7, 1200.0);
    let left = (a.x.min(b.x) - radius).max(0.0) as u32;
    let right = (a.x.max(b.x) + radius).min(999.0) as u32;
    let top = (a.y.min(b.y) - radius).max(0.0) as u32;
    let bottom = (a.y.max(b.y) + radius).min(679.0) as u32;
    for x in left..=right {
        for y in top..=bottom {
            if pcbcraft_geom::point_segment(Point::new(x as f64, y as f64), a, b) <= radius {
                image.put_pixel(x, y, image::Rgb(color));
            }
        }
    }
}

fn fill_polygon(image: &mut image::RgbImage, points: &[Point], color: [u8; 3]) {
    // Scanline fill in pixel space: pixel centres inside the polygon get the
    // zone colour. Hostile or off-view geometry is clipped to the image.
    if points.len() < 3 {
        return;
    }
    let top = points
        .iter()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min)
        .max(0.0) as u32;
    let bottom = points
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .min(679.0) as u32;
    for y in top..=bottom {
        let row = f64::from(y) + 0.5;
        let mut crossings: Vec<f64> = vec![];
        for edge in points.windows(2) {
            let (a, b) = (edge[0], edge[1]);
            if (a.y > row) != (b.y > row) {
                crossings.push(a.x + (row - a.y) * (b.x - a.x) / (b.y - a.y));
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.chunks(2) {
            if pair.len() < 2 {
                continue;
            }
            let left = pair[0].max(0.0) as u32;
            let right = (pair[1].min(999.0) as u32).min(999);
            for x in left..=right {
                image.put_pixel(x, y, image::Rgb(color));
            }
        }
    }
}

// ---- Library text writers -------------------------------------------------

/// Quote a value as a library text token, or emit it bare when it is a
/// simple word.
fn lib_token(value: &str, default: &str) -> String {
    let value = if value.is_empty() { default } else { value };
    if value
        .chars()
        .any(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '"' | '(' | ')' | '\0'))
    {
        lib_text(value)
    } else {
        value.to_owned()
    }
}

/// Quote a value as a library text string with escapes the reader decodes.
fn lib_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn lib_xy(x: f64, y: f64) -> String {
    format!("{x} {y}")
}

/// Emit one artwork primitive. Library symbol bodies are stored y-up while
/// the file frame is y-down, so `flip` mirrors every ordinate back.
fn lib_graphic(graphic: &pcbcraft_library::Graphic, flip: bool) -> String {
    use pcbcraft_library::Graphic;
    let y = |v: f64| if flip { -v } else { v };
    match graphic {
        Graphic::Line {
            start,
            end,
            width,
            layer,
        } => format!(
            "(fp_line (start {}) (end {}) (stroke (width {width})) (layer {}))",
            lib_xy(start.x, y(start.y)),
            lib_xy(end.x, y(end.y)),
            lib_layer(layer)
        ),
        Graphic::Rectangle {
            start,
            end,
            width,
            fill,
            layer,
        } => format!(
            "(fp_rect (start {}) (end {}) (stroke (width {width})) (fill (type {})) (layer {}))",
            lib_xy(start.x, y(start.y)),
            lib_xy(end.x, y(end.y)),
            lib_token(fill, "none"),
            lib_layer(layer)
        ),
        Graphic::Circle {
            center,
            radius,
            width,
            fill,
            layer,
        } => format!(
            "(fp_circle (center {}) (end {}) (stroke (width {width})) (fill (type {})) (layer {}))",
            lib_xy(center.x, y(center.y)),
            lib_xy(center.x + radius, y(center.y)),
            lib_token(fill, "none"),
            lib_layer(layer)
        ),
        Graphic::Arc {
            start,
            mid,
            end,
            width,
            layer,
        } => format!(
            "(fp_arc (start {}) (mid {}) (end {}) (stroke (width {width})) (layer {}))",
            lib_xy(start.x, y(start.y)),
            lib_xy(mid.x, y(mid.y)),
            lib_xy(end.x, y(end.y)),
            lib_layer(layer)
        ),
        Graphic::Polyline {
            points,
            width,
            fill,
            layer,
        } => format!(
            "(fp_poly (pts {}) (stroke (width {width})) (fill (type {})) (layer {}))",
            points
                .iter()
                .map(|p| format!("(xy {} {})", p.x, y(p.y)))
                .collect::<Vec<_>>()
                .join(" "),
            lib_token(fill, "none"),
            lib_layer(layer)
        ),
        Graphic::Text {
            position,
            angle,
            text,
            size,
            layer,
        } => format!(
            "(fp_text graphics {} (at {} {} {angle}) (effects (font (size {} {}))) (layer {}))",
            lib_text(text),
            position.x,
            y(position.y),
            size.x,
            size.y,
            lib_layer(layer)
        ),
    }
}

fn lib_layer(layer: &str) -> String {
    lib_text(layer)
}

fn append_indented_source(out: &mut String, source: &str, indent: &str) {
    for line in source.trim().lines() {
        out.push_str(indent);
        out.push_str(line);
        out.push('\n');
    }
}

fn append_retained_nodes(out: &mut String, retained: &[String], indent: &str) {
    for node in retained {
        append_indented_source(out, node, indent);
    }
}

/// Format one footprint definition as a footprint library text file. The
/// result re-parses to the same pads, artwork, model reference and rule
/// overrides; placed footprint and board state are untouched.
pub fn footprint_library_text(
    footprint: &pcbcraft_library::FootprintDefinition,
) -> Result<String, String> {
    footprint
        .validate()
        .map_err(|e| format!("footprint is invalid: {e}"))?;
    let mut out = format!(
        "(footprint {}\n  (version 20260206)\n  (generator pcbcraft)\n  (layer \"F.Cu\")\n",
        lib_text(&footprint.name)
    );
    if !footprint.description.is_empty() {
        out.push_str(&format!("  (descr {})\n", lib_text(&footprint.description)));
    }
    if !footprint.keywords.is_empty() {
        out.push_str(&format!("  (tags {})\n", lib_text(&footprint.keywords)));
    }
    out.push_str("  (property \"Reference\" \"REF**\" (at 0 0 0) (layer \"F.SilkS\") (effects (font (size 1.27 1.27) (thickness 0.15))))\n");
    out.push_str(&format!(
        "  (property \"Value\" {} (at 0 0 0) (layer \"F.Fab\") (effects (font (size 1.27 1.27) (thickness 0.15))))\n",
        lib_text(&footprint.name)
    ));
    let smd = footprint.pads.iter().any(|p| p.pad_type == "smd");
    out.push_str(&format!(
        "  (attr {})\n",
        if smd { "smd" } else { "through_hole" }
    ));
    for pad in &footprint.pads {
        if !pad.raw_source.is_empty() {
            append_indented_source(&mut out, &pad.raw_source, "  ");
            continue;
        }
        out.push_str(&format!(
            "  (pad {} {} {} (at {} {} {}) (size {} {})",
            lib_text(&pad.number),
            lib_token(&pad.pad_type, "smd"),
            lib_token(&pad.shape, "circle"),
            pad.position.x,
            pad.position.y,
            pad.rotation,
            pad.size.x,
            pad.size.y
        ));
        if let Some(drill) = &pad.drill {
            if drill.size.x != drill.size.y {
                out.push_str(&format!(" (drill oval {} {}", drill.size.x, drill.size.y));
            } else {
                out.push_str(&format!(" (drill {}", drill.size.x));
            }
            if drill.offset.x != 0.0 || drill.offset.y != 0.0 {
                out.push_str(&format!(" (offset {} {})", drill.offset.x, drill.offset.y));
            }
            out.push(')');
        }
        out.push_str(" (layers");
        for layer in &pad.layers {
            out.push_str(&format!(" {}", lib_text(layer)));
        }
        out.push(')');
        if let Some(ratio) = pad.roundrect_ratio {
            out.push_str(&format!(" (roundrect_rratio {ratio})"));
        }
        out.push_str(")\n");
    }
    for graphic in &footprint.graphics {
        out.push_str(&format!("  {}\n", lib_graphic(graphic, false)));
    }
    if let Some(model) = &footprint.model {
        if model.raw_source.is_empty() {
            let triple = |v: &[f64; 3]| format!("(xyz {} {} {})", v[0], v[1], v[2]);
            out.push_str(&format!(
                "  (model {} (offset {}) (scale {}) (rotate {}))\n",
                lib_text(&model.path),
                triple(&model.offset),
                triple(&model.scale),
                triple(&model.rotate)
            ));
        } else {
            append_indented_source(&mut out, &model.raw_source, "  ");
        }
    }
    if let Some(rules) = &footprint.rules {
        if let Some(value) = rules.clearance {
            out.push_str(&format!("  (clearance {value})\n"));
        }
        if let Some(value) = rules.mask_margin {
            out.push_str(&format!("  (solder_mask_margin {value})\n"));
        }
        if let Some(value) = rules.paste_margin {
            out.push_str(&format!("  (solder_paste_margin {value})\n"));
        }
    }
    append_retained_nodes(&mut out, &footprint.retained, "  ");
    out.push_str("  (embedded_fonts no)\n)\n");
    if out.len() as u64 > MAX_FILE_BYTES {
        return Err("footprint library text exceeds the file size limit".into());
    }
    Ok(out)
}

/// One symbol block inside a symbol library file, without the outer wrapper.
fn symbol_library_inner(symbol: &pcbcraft_library::SymbolDefinition) -> Result<String, String> {
    symbol
        .validate()
        .map_err(|e| format!("symbol is invalid: {e}"))?;
    let mut out = format!("  (symbol {}\n", lib_text(&symbol.name));
    for (key, value) in [
        ("Reference", &symbol.reference),
        ("Value", &symbol.value),
        ("Description", &symbol.description),
        ("ki_keywords", &symbol.keywords),
        ("Footprint", &symbol.footprint),
    ] {
        if !value.is_empty() {
            out.push_str(&format!(
                "    (property \"{key}\" {} (at 0 0 0) (effects (font (size 1.27 1.27) (thickness 0.15))))\n",
                lib_text(value)
            ));
        }
    }
    let mut units: std::collections::BTreeSet<(u32, u32)> =
        symbol.pins.iter().map(|p| (p.unit, p.body_style)).collect();
    units.extend(symbol.graphics.iter().map(|g| (g.unit, g.body_style)));
    for (unit, body_style) in units {
        out.push_str(&format!(
            "    (symbol {}\n",
            lib_text(&format!("{}_{}_{}", symbol.name, unit, body_style))
        ));
        for pin in symbol
            .pins
            .iter()
            .filter(|p| p.unit == unit && p.body_style == body_style)
        {
            let mut pin_line = format!(
                "      (pin {} {} (at {} {} {}) (length {}) (name {}) (number {})",
                lib_token(&pin.electrical_type, "passive"),
                lib_token(&pin.graphical_style, "line"),
                pin.position.x,
                -pin.position.y,
                pin.angle,
                pin.length,
                lib_text(&pin.name),
                lib_text(&pin.number)
            );
            if !pin.visible {
                pin_line.push_str(" (hide yes)");
            }
            pin_line.push_str(")\n");
            out.push_str(&pin_line);
        }
        for graphic in symbol
            .graphics
            .iter()
            .filter(|g| g.unit == unit && g.body_style == body_style)
        {
            out.push_str(&format!(
                "      {}\n",
                lib_graphic(&graphic.primitive, true)
            ));
        }
        out.push_str("    )\n");
    }
    append_retained_nodes(&mut out, &symbol.retained, "    ");
    out.push_str("  )\n");
    Ok(out)
}

/// Format one symbol definition as a symbol library text file. Pins and
/// artwork regroup into unit/body sub-symbols and the stored y-up drawing
/// coordinates mirror back to the file frame; the result re-parses to the
/// same pins and artwork.
pub fn symbol_library_text(symbol: &pcbcraft_library::SymbolDefinition) -> Result<String, String> {
    let mut out = String::from("(kicad_symbol_lib (version 20260206) (generator pcbcraft)\n");
    out.push_str(&symbol_library_inner(symbol)?);
    out.push_str(")\n");
    if out.len() as u64 > MAX_FILE_BYTES {
        return Err("symbol library text exceeds the file size limit".into());
    }
    Ok(out)
}

/// Write several symbol definitions into one symbol library file.
pub fn symbols_library_file_text(
    symbols: &[pcbcraft_library::SymbolDefinition],
) -> Result<String, String> {
    if symbols.is_empty() {
        return Err("no symbols to export".into());
    }
    if symbols.len() > 64 {
        return Err("symbol export lists at most 64 names".into());
    }
    let mut names = BTreeSet::new();
    for symbol in symbols {
        if !names.insert(&symbol.name) {
            return Err("duplicate symbol name in export list".into());
        }
    }
    let mut sorted = symbols.to_vec();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = String::from("(kicad_symbol_lib (version 20260206) (generator pcbcraft)\n");
    for symbol in &sorted {
        out.push_str(&symbol_library_inner(symbol)?);
    }
    out.push_str(")\n");
    if out.len() as u64 > MAX_FILE_BYTES {
        return Err("symbol library text exceeds the file size limit".into());
    }
    Ok(out)
}

/// Write project-library symbols or footprints to library text on disk.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_library_definitions(
    library: &ProjectLibrary,
    kind: &str,
    path: &Path,
    names: &[String],
) -> Result<serde_json::Value, String> {
    library.validate()?;
    if names.is_empty() || names.len() > 64 {
        return Err("export needs 1 to 64 library names".into());
    }
    let mut seen = BTreeSet::new();
    for name in names {
        if name.is_empty() || name.len() > 128 || !seen.insert(name.as_str()) {
            return Err("export names must be unique and 1 to 128 bytes".into());
        }
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("path must not contain ..".into());
    }
    match kind {
        "symbol" => {
            if path.extension().and_then(|e| e.to_str()) != Some("kicad_sym") {
                return Err("path must end in .kicad_sym".into());
            }
            let mut symbols = Vec::with_capacity(names.len());
            for name in names {
                let item = library
                    .symbols
                    .iter()
                    .find(|s| s.name == *name)
                    .ok_or_else(|| format!("unknown project-library symbol: {name}"))?;
                symbols.push(item.clone());
            }
            symbols.sort_by(|a, b| a.name.cmp(&b.name));
            let text = if symbols.len() == 1 {
                symbol_library_text(&symbols[0])?
            } else {
                symbols_library_file_text(&symbols)?
            };
            atomic_write(path, text.as_bytes())?;
            let file = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
                .ok_or_else(|| "path must name a file".to_string())?;
            Ok(serde_json::json!({"path": path.to_string_lossy(), "files": [file]}))
        }
        "footprint" => {
            let mut files = Vec::new();
            if names.len() == 1 {
                if path.extension().and_then(|e| e.to_str()) != Some("kicad_mod") {
                    return Err("path must end in .kicad_mod".into());
                }
            }
            if names.len() == 1 && path.extension().and_then(|e| e.to_str()) == Some("kicad_mod") {
                let name = &names[0];
                let item = library
                    .footprints
                    .iter()
                    .find(|f| f.name == *name)
                    .ok_or_else(|| format!("unknown project-library footprint: {name}"))?;
                let text = footprint_library_text(item)?;
                atomic_write(path, text.as_bytes())?;
                let file = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                    .ok_or_else(|| "path must name a file".to_string())?;
                files.push(file);
                return Ok(serde_json::json!({"path": path.to_string_lossy(), "files": files}));
            }
            if path.exists() && !path.is_dir() {
                return Err(
                    "footprint export needs a folder path unless writing one .kicad_mod file"
                        .into(),
                );
            }
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
            for name in names {
                let item = library
                    .footprints
                    .iter()
                    .find(|f| f.name == *name)
                    .ok_or_else(|| format!("unknown project-library footprint: {name}"))?;
                let text = footprint_library_text(item)?;
                let file = format!("{name}.kicad_mod");
                atomic_write(&path.join(&file), text.as_bytes())?;
                files.push(file);
            }
            Ok(serde_json::json!({"path": path.to_string_lossy(), "files": files}))
        }
        _ => Err("export kind must be symbol or footprint".into()),
    }
}
#[cfg(target_arch = "wasm32")]
pub fn export_library_definitions(
    _library: &ProjectLibrary,
    _kind: &str,
    _path: &Path,
    _names: &[String],
) -> Result<serde_json::Value, String> {
    Err("host filesystem writes are unavailable in the browser; download support is planned".into())
}

/// Render one library footprint as a standalone SVG preview: copper pads
/// with their holes, courtyard and other artwork, pad numbers, all in
/// millimetre coordinates drawn y-up so the view matches the board.
pub fn footprint_svg(footprint: &pcbcraft_library::FootprintDefinition) -> Result<String, String> {
    footprint
        .validate()
        .map_err(|e| format!("footprint is invalid: {e}"))?;
    let mut body = String::new();
    let mut box_min = Point::new(-1.0, -1.0);
    let mut box_max = Point::new(1.0, 1.0);
    let mut grow = |p: Point| {
        box_min.x = box_min.x.min(p.x);
        box_min.y = box_min.y.min(p.y);
        box_max.x = box_max.x.max(p.x);
        box_max.y = box_max.y.max(p.y);
    };
    for g in &footprint.graphics {
        lib_svg_graphic(g, &mut body, &mut grow);
    }
    for pad in &footprint.pads {
        let hx = pad.size.x / 2.0;
        let hy = pad.size.y / 2.0;
        let spin = |p: Point| pad.position.offset(p.rotate(-pad.rotation));
        let radius = match pad.shape.as_str() {
            "circle" | "oval" => hx.min(hy),
            "roundrect" => pad.size.x.min(pad.size.y) * pad.roundrect_ratio.unwrap_or(0.0),
            _ => 0.0,
        };
        if pad.shape == "circle" {
            grow(Point::new(
                pad.position.x + hx.max(hy),
                pad.position.y + hx.max(hy),
            ));
            grow(Point::new(
                pad.position.x - hx.max(hy),
                pad.position.y - hx.max(hy),
            ));
            body.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#b87333\"/>\n",
                svg_num(pad.position.x),
                svg_num(pad.position.y),
                svg_num(hx.max(hy))
            ));
        } else if radius <= 1e-9 {
            for corner in [
                Point::new(-hx, -hy),
                Point::new(hx, -hy),
                Point::new(hx, hy),
                Point::new(-hx, hy),
            ] {
                grow(spin(corner));
            }
            body.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#b87333\" transform=\"rotate({} {} {})\"/>\n",
                svg_num(-hx),
                svg_num(-hy),
                svg_num(pad.size.x),
                svg_num(pad.size.y),
                svg_num(pad.rotation),
                svg_num(pad.position.x),
                svg_num(pad.position.y)
            ));
        } else {
            let mut points = Vec::new();
            for (x, y, start) in [
                (hx - radius, hy - radius, 0.0),
                (-hx + radius, hy - radius, 90.0),
                (-hx + radius, -hy + radius, 180.0),
                (hx - radius, -hy + radius, 270.0),
            ] {
                for i in 0..=16 {
                    let angle = (start + f64::from(i) * 90.0 / 16.0).to_radians();
                    points.push(spin(Point::new(
                        x + radius * angle.cos(),
                        y + radius * angle.sin(),
                    )));
                }
            }
            for p in &points {
                grow(*p);
            }
            let outline = points
                .iter()
                .map(|p| format!("{},{}", svg_num(p.x), svg_num(p.y)))
                .collect::<Vec<_>>()
                .join(" ");
            body.push_str(&format!(
                "<polygon points=\"{outline}\" fill=\"#b87333\"/>\n"
            ));
        }
        if let Some(drill) = &pad.drill {
            let dr = (drill.size.x.max(drill.size.y)) / 2.0;
            let center = pad.position.offset(drill.offset.rotate(-pad.rotation));
            grow(Point::new(center.x + dr, center.y + dr));
            grow(Point::new(center.x - dr, center.y - dr));
            if drill.size.x != drill.size.y {
                let along = Point::new(drill.size.x.max(drill.size.y) / 2.0 - dr, 0.0)
                    .rotate(-pad.rotation);
                body.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#202020\" stroke-width=\"{}\" stroke-linecap=\"round\"/>\n",
                    svg_num(center.x - along.x),
                    svg_num(center.y - along.y),
                    svg_num(center.x + along.x),
                    svg_num(center.y + along.y),
                    svg_num(dr * 2.0)
                ));
            } else {
                body.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#202020\"/>\n",
                    svg_num(center.x),
                    svg_num(center.y),
                    svg_num(dr)
                ));
            }
        }
        if !pad.number.is_empty() {
            body.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\" fill=\"#202020\">{}</text>\n",
                svg_num(pad.position.x),
                svg_num(pad.position.y),
                svg_num(hx.min(hy).clamp(0.3, 1.0) * 1.2),
                lib_svg_escape(&pad.number)
            ));
        }
    }
    Ok(svg_document(&footprint.name, box_min, box_max, body))
}

fn symbol_pin_svg(
    pin: &pcbcraft_library::SymbolPin,
    body: &mut String,
    grow: &mut impl FnMut(Point),
) {
    grow(pin.position);
    let end = pin.stem_end();
    grow(end);
    if pin.visible {
        body.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"0.25\"/>\n",
            svg_num(pin.position.x),
            svg_num(pin.position.y),
            svg_num(end.x),
            svg_num(end.y)
        ));
        for (center, radius) in pin.decoration_bubbles() {
            grow(center);
            grow(center.offset(Point::new(radius, radius)));
            grow(center.offset(Point::new(-radius, -radius)));
            body.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.12\"/>\n",
                svg_num(center.x),
                svg_num(center.y),
                svg_num(radius)
            ));
        }
        for path in pin.decoration_paths() {
            for p in &path {
                grow(*p);
            }
            if path.len() >= 3 {
                let coords = path
                    .iter()
                    .map(|p| format!("{},{}", svg_num(p.x), svg_num(p.y)))
                    .collect::<Vec<_>>()
                    .join(" ");
                body.push_str(&format!(
                    "<polygon points=\"{coords}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.12\"/>\n"
                ));
            } else if path.len() == 2 {
                body.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"0.12\"/>\n",
                    svg_num(path[0].x),
                    svg_num(path[0].y),
                    svg_num(path[1].x),
                    svg_num(path[1].y)
                ));
            }
        }
    }
    body.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"0.8\" text-anchor=\"middle\" fill=\"black\">{}</text>\n",
        svg_num(pin.position.x),
        svg_num(pin.position.y),
        lib_svg_escape(&pin.number)
    ));
}

fn schematic_pin_decorations_png(
    image: &mut image::RgbImage,
    pin: &pcbcraft_library::SymbolPin,
    map: &impl Fn(Point) -> Point,
) {
    let ink = [219, 231, 242];
    for (center, radius) in pin.decoration_bubbles() {
        let center_px = map(center);
        let edge_px = map(center.offset(Point::new(radius, 0.0)));
        disk(image, center_px, center_px.distance(edge_px), ink);
    }
    for path in pin.decoration_paths() {
        if path.len() >= 3 {
            let points: Vec<Point> = path.iter().map(|p| map(*p)).collect();
            fill_polygon(image, &points, ink);
            for pair in points.windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(image, *a, *b, 0.35, ink);
                }
            }
            if let (Some(first), Some(last)) = (points.first(), points.last()) {
                line(image, *last, *first, 0.35, ink);
            }
        } else if path.len() == 2 {
            line(image, map(path[0]), map(path[1]), 0.35, ink);
        }
    }
}

/// Render one library symbol as a standalone SVG preview: body and unit
/// artwork with pin lines and pin numbers, in millimetre coordinates drawn
/// y-up like the schematic sheet.
pub fn symbol_svg(symbol: &pcbcraft_library::SymbolDefinition) -> Result<String, String> {
    if symbol.name.trim().is_empty() {
        return Err("symbol is missing a name".into());
    }
    let mut body = String::new();
    let mut box_min = Point::new(-1.0, -1.0);
    let mut box_max = Point::new(1.0, 1.0);
    let mut grow = |p: Point| {
        box_min.x = box_min.x.min(p.x);
        box_min.y = box_min.y.min(p.y);
        box_max.x = box_max.x.max(p.x);
        box_max.y = box_max.y.max(p.y);
    };
    for g in symbol.graphics.iter().filter(|g| g.body_style <= 1) {
        lib_svg_graphic(&g.primitive, &mut body, &mut grow);
    }
    for pin in symbol.pins.iter().filter(|pin| pin.body_style <= 1) {
        symbol_pin_svg(pin, &mut body, &mut grow);
    }
    Ok(svg_document(&symbol.name, box_min, box_max, body))
}

fn svg_num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    r.to_string()
}

fn lib_svg_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn lib_svg_graphic(g: &pcbcraft_library::Graphic, body: &mut String, grow: &mut impl FnMut(Point)) {
    match g {
        pcbcraft_library::Graphic::Line {
            start, end, width, ..
        } => {
            grow(*start);
            grow(*end);
            body.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_num(start.x),
                svg_num(start.y),
                svg_num(end.x),
                svg_num(end.y),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Rectangle {
            start,
            end,
            width,
            fill,
            ..
        } => {
            grow(*start);
            grow(*end);
            body.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_num(start.x.min(end.x)),
                svg_num(start.y.min(end.y)),
                svg_num((end.x - start.x).abs()),
                svg_num((end.y - start.y).abs()),
                svg_fill(fill),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Circle {
            center,
            radius,
            width,
            fill,
            ..
        } => {
            grow(Point::new(center.x + radius, center.y + radius));
            grow(Point::new(center.x - radius, center.y - radius));
            body.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_num(center.x),
                svg_num(center.y),
                svg_num(*radius),
                svg_fill(fill),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Arc {
            start,
            mid,
            end,
            width,
            ..
        } => {
            grow(*start);
            grow(*mid);
            grow(*end);
            let (center, radius) = match svg_arc_center(start, mid, end) {
                Some(v) => v,
                None => return,
            };
            let sweep = if lib_svg_arc_ccw(&center, start, mid, end) {
                1
            } else {
                0
            };
            body.push_str(&format!(
                "<path d=\"M {},{} A {},{} 0 0 {},{} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_num(start.x),
                svg_num(start.y),
                svg_num(radius),
                svg_num(radius),
                sweep,
                svg_num(end.x),
                svg_num(end.y),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Polyline {
            points,
            width,
            fill,
            ..
        } => {
            if points.len() < 2 {
                return;
            }
            for p in points {
                grow(*p);
            }
            let list = points
                .iter()
                .map(|p| format!("{},{}", svg_num(p.x), svg_num(p.y)))
                .collect::<Vec<_>>()
                .join(" ");
            body.push_str(&format!(
                "<polyline points=\"{list}\" fill=\"{}\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_fill(fill),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Text {
            position,
            text,
            size,
            ..
        } => {
            grow(*position);
            body.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"black\">{}</text>\n",
                svg_num(position.x),
                svg_num(position.y),
                svg_num(size.x.max(0.5)),
                lib_svg_escape(text)
            ));
        }
    }
}

fn svg_fill(fill: &str) -> &'static str {
    if fill.is_empty() || fill.eq_ignore_ascii_case("none") || fill.eq_ignore_ascii_case("solid") {
        "none"
    } else {
        "black"
    }
}

fn svg_arc_center(start: &Point, mid: &Point, end: &Point) -> Option<(Point, f64)> {
    let d =
        2.0 * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let start2 = start.x * start.x + start.y * start.y;
    let mid2 = mid.x * mid.x + mid.y * mid.y;
    let end2 = end.x * end.x + end.y * end.y;
    let ux = (start2 * (mid.y - end.y) + mid2 * (end.y - start.y) + end2 * (start.y - mid.y)) / d;
    let uy = (start2 * (end.x - mid.x) + mid2 * (start.x - end.x) + end2 * (mid.x - start.x)) / d;
    let center = Point::new(ux, uy);
    let radius = ((center.x - start.x).powi(2) + (center.y - start.y).powi(2)).sqrt();
    Some((center, radius))
}

fn lib_svg_arc_ccw(center: &Point, start: &Point, mid: &Point, end: &Point) -> bool {
    let cross = |a: &Point, b: &Point| {
        (a.x - center.x) * (b.y - center.y) - (a.y - center.y) * (b.x - center.x)
    };
    let quarter = |a: &Point, b: &Point| {
        let dot = (a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y);
        cross(a, b) >= 0.0 && dot >= 0.0
    };
    if quarter(start, mid) {
        cross(start, mid) >= 0.0
    } else if quarter(mid, end) {
        cross(mid, end) >= 0.0
    } else {
        cross(start, end) >= 0.0
    }
}

fn svg_document(name: &str, box_min: Point, box_max: Point, body: String) -> String {
    let margin = 1.0;
    let width = (box_max.x - box_min.x + 2.0 * margin) * 10.0;
    let height = (box_max.y - box_min.y + 2.0 * margin) * 10.0;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{} {} {} {}\">\n<title>{}</title>\n<g transform=\"scale(1,-1)\">\n{body}</g>\n</svg>\n",
        svg_num(width),
        svg_num(height),
        svg_num((box_min.x - margin) * 10.0),
        svg_num(-(box_max.y + margin) * 10.0),
        svg_num(width),
        svg_num(height),
        lib_svg_escape(name)
    )
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn archive_round_trips_a_project_and_its_attached_files() -> Result<(), String> {
        let mut project = Project {
            library_table: Vec::new(),
            padstacks: Vec::new(),
            name: "Archive test".into(),
            ..Project::default()
        };
        project.files.push(pcbcraft_doc::EmbeddedFile {
            name: "notes.txt".into(),
            data: "hello from the archive".into(),
        });
        let zip = archive_zip(&project)?;
        assert!(
            zip.starts_with(b"PK\x03\x04"),
            "a ZIP starts with a local header"
        );
        assert!(zip.windows(4).any(|w| w == b"PK\x05\x06"));
        assert_eq!(
            crc32(b"123456789"),
            0xcbf4_3926,
            "the CRC must match the value every unzip tool expects"
        );
        let back = archive_project(&zip)?;
        assert_eq!(back, project, "the archive must restore the exact design");
        assert!(archive_project(b"not a zip at all").is_err());
        let mut truncated = zip.clone();
        truncated.truncate(zip.len() - 30);
        assert!(archive_project(&truncated).is_err());
        let mut wrong = zip.clone();
        let at = wrong
            .windows(11)
            .position(|w| w == b"design.json")
            .ok_or("entry name missing")?;
        wrong[at] = b"x"[0];
        assert!(
            archive_project(&wrong).is_err(),
            "a renamed design is not found"
        );
        Ok(())
    }

    #[test]
    fn postscript_writes_a_printable_document_for_both_views() -> Result<(), String> {
        let mut project = Project::default();
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 101,
            net: 1,
            start: Point::new(5.0, 5.0),
            end: Point::new(40.0, 30.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
            arc: Some(Point::new(20.0, 20.0)),
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 102,
            net: 1,
            position: Point::new(40.0, 30.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 103,
            text: "GND (pour) 100%".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            size: 1.0,
            layer: pcbcraft_doc::Layer::Front,
            component: None,
        });
        project.drawn_wires.push(pcbcraft_doc::DrawnWire {
            id: 104,
            net: None,
            points: vec![Point::new(10.0, 5.0), Point::new(20.0, 5.0)],
        });
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        project.labels.push(pcbcraft_doc::Label {
            id: 105,
            kind: "global".into(),
            net: 1,
            position: Point::new(10.0, 5.0),
            rotation: 0.0,
            text: String::new(),
        });
        project.validate()?;
        let pcb = postscript(&project, "pcb")?;
        assert!(
            pcb.starts_with("%!PS-Adobe-3.0\n%%Title: PCBCraft pcb view of Untitled circuit\n"),
            "the file opens as a PostScript document"
        );
        assert!(
            pcb.contains("%%BoundingBox: 0 0 792 612"),
            "the page box is declared for printers"
        );
        assert!(
            pcb.contains(" findfont") && pcb.contains(" Tj"),
            "text selects Helvetica rather than a PDF font resource"
        );
        assert!(
            pcb.contains(" m\n") && pcb.contains("\nS\n"),
            "copper paths are built and stroked"
        );
        assert!(
            pcb.ends_with("showpage\n%%EOF\n"),
            "the page is sent to the device"
        );
        let sch = postscript(&project, "schematic")?;
        assert!(
            sch.contains("%%Title: PCBCraft schematic view of"),
            "the schematic view draws too"
        );
        assert!(
            postscript(&project, "wrong").is_err(),
            "only the two known views can be drawn"
        );
        Ok(())
    }

    #[test]
    fn stats_text_counts_parts_nets_and_routing() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 500,
            name: "GND".into(),
            ..Default::default()
        });
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 600,
            reference: "R1".into(),
            value: "10k".into(),
            kind: "resistor".into(),
            position: Point::new(20.0, 20.0),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(0.0, 0.0),
                    net: Some(500),
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: String::new(),
                    offset: Point::new(2.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 601,
            net: 500,
            start: Point::new(0.0, 0.0),
            end: Point::new(30.0, 40.0),
            width: 0.25,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 602,
            net: 500,
            position: Point::new(30.0, 40.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.validate()?;
        let text = stats_text(&project)?;
        assert!(
            text.starts_with("# Board statistics for Untitled circuit\nKEY,VALUE\n"),
            "the sheet names the design and opens the counter table"
        );
        assert!(
            text.contains("components,1") && text.contains("nets,1") && text.contains("pins,2"),
            "part and net counters are listed"
        );
        assert!(
            text.contains("pins_connected_to_nets,1") && text.contains("pins_without_net,1"),
            "connected and floating pins are split honestly"
        );
        assert!(
            text.contains("track_length_mm,50.0"),
            "the 3-4-5 track is measured as 50 mm"
        );
        assert!(
            text.contains("drilled_holes_total,1"),
            "the via drill is counted once"
        );
        Ok(())
    }

    #[test]
    fn drill_chart_csv_counts_holes_per_plated_and_unplated_tool() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 500,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 501,
            net: 500,
            position: Point::new(10.0, 10.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 502,
            net: 500,
            position: Point::new(20.0, 10.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 503,
            position: Point::new(1.5, 1.5),
            diameter: 3.2,
            end: None,
        });
        project.board.machining = Some(pcbcraft_doc::MachiningNotes {
            edge_plating: None,
            drill_tolerance: Some(0.05),
            edge_tolerance: None,
        });
        project.validate()?;
        let text = drill_chart_csv(&project)?;
        assert!(
            text.starts_with("# Drill tool chart for Untitled circuit\n"),
            "the chart names the design"
        );
        assert!(
            text.contains("# Drill tolerance +/-0.050 mm"),
            "the recorded drill tolerance heads the sheet"
        );
        assert!(
            text.contains("T1,0.300,0.000,yes,2"),
            "the via drill is one plated tool with two hits"
        );
        assert!(
            text.contains("T2,3.200,0.000,no,1"),
            "the mounting hole is an unplated tool"
        );
        assert!(
            text.contains("TOTAL,,,,3"),
            "the total row counts every hit"
        );
        Ok(())
    }

    #[test]
    fn drill_chart_counts_slots_by_width_and_length() -> Result<(), String> {
        let mut project = Project::default();
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 510,
            position: Point::new(10.0, 10.0),
            diameter: 1.0,
            end: Some(Point::new(14.0, 10.0)),
        });
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 511,
            position: Point::new(10.0, 20.0),
            diameter: 1.0,
            end: Some(Point::new(14.0, 20.0)),
        });
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 512,
            position: Point::new(30.0, 10.0),
            diameter: 1.0,
            end: None,
        });
        project.validate()?;
        let text = drill_chart_csv(&project)?;
        assert!(
            text.contains("T1,1.000,0.000,no,1"),
            "a round hole of the same width is its own tool: {text}"
        );
        assert!(
            text.contains("T2,1.000,4.000,no,2"),
            "both slots share one tool keyed by width and span: {text}"
        );
        assert!(text.contains("TOTAL,,,,3"), "every hit is counted: {text}");
        Ok(())
    }
    #[test]
    fn a_slotted_mounting_hole_writes_one_g85_span() -> Result<(), String> {
        let mut project = Project::default();
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 520,
            position: Point::new(10.0, 10.0),
            diameter: 1.0,
            end: Some(Point::new(14.0, 10.0)),
        });
        project.validate()?;
        let files = gerber_files(&project)?;
        let drill = &files["pcb-Drill.drl"];
        assert!(
            drill.contains("T1C1.000\n"),
            "the tool is the slot width: {drill}"
        );
        assert!(
            drill.contains("X10.000Y10.000G85X14.000Y10.000\n"),
            "one hit spans the two centres: {drill}"
        );
        assert!(drill.ends_with("M30\n"));
        let map = &files["pcb-DrillMap.gbr"];
        assert!(
            map.contains("X10.000000Y10.000000D02*\nX14.000000Y10.000000D01*\n"),
            "the drill map draws the slot as a stroke from end to end: {map}"
        );
        Ok(())
    }
    #[test]
    fn stackup_text_lists_layer_notes_and_machining_tolerances() -> Result<(), String> {
        let mut project = Project::default();
        project.board.layer_records.push(pcbcraft_doc::LayerRecord {
            name: "L1".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: None,
        });
        project.board.layer_records.push(pcbcraft_doc::LayerRecord {
            name: "core".into(),
            kind: "core".into(),
            thickness: 1.2,
            copper_role: None,
        });
        project.board.machining = Some(pcbcraft_doc::MachiningNotes {
            edge_plating: Some(true),
            drill_tolerance: Some(0.05),
            edge_tolerance: None,
        });
        project.validate()?;
        let text = stackup_text(&project)?;
        assert!(
            text.starts_with("Stackup report for Untitled circuit\n"),
            "the sheet names the design"
        );
        assert!(
            text.contains("L1,copper,0.035") && text.contains("core,core,1.200"),
            "every recorded layer note is listed with its role and thickness"
        );
        assert!(
            text.contains("Total recorded thickness: 1.235 mm"),
            "the recorded layers are totalled"
        );
        assert!(
            text.contains("Edge plating: yes") && text.contains("Drill tolerance: +/- 0.050 mm"),
            "machining notes reach the fabrication sheet"
        );
        assert!(
            !text.contains("Edge tolerance"),
            "tolerances that were never recorded stay off the sheet"
        );
        let bare = stackup_text(&Project::default())?;
        assert!(
            bare.contains("(none recorded)"),
            "a board without layer notes says so"
        );
        Ok(())
    }

    #[test]
    fn hpgl_plot_writes_pen_moves_for_both_views() -> Result<(), String> {
        let mut project = real_library_project()?;
        project.nets.push(pcbcraft_doc::Net {
            id: 500,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 501,
            net: 500,
            start: Point::new(5.0, 5.0),
            end: Point::new(40.0, 30.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 502,
            net: 500,
            position: Point::new(12.0, 12.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.validate()?;
        let pcb = hpgl_plot(&project, "pcb")?;
        assert!(pcb.starts_with("IN;SP1;\n"), "the plotter is initialised");
        assert!(
            pcb.contains("PU") && pcb.contains(";PD"),
            "copper is drawn with pen-down polylines"
        );
        assert!(pcb.ends_with("PU;\n"), "the pen is lifted at the end");
        let sch = hpgl_plot(&project, "schematic")?;
        assert!(
            sch.contains("PU") && sch.contains(";PD"),
            "the schematic view plots too"
        );
        assert!(
            hpgl_plot(&project, "side").is_err(),
            "only the two known views can be plotted"
        );
        Ok(())
    }

    #[test]
    fn odb_package_holds_a_matrix_symbols_features_and_components_tree() -> Result<(), String> {
        let mut project = real_library_project()?;
        project.nets.push(pcbcraft_doc::Net {
            id: 500,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 501,
            net: 500,
            start: Point::new(5.0, 5.0),
            end: Point::new(40.0, 30.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 502,
            net: 500,
            position: Point::new(12.0, 12.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.validate()?;
        let package = odb_package(&project)?;
        assert!(
            package.starts_with(b"PK\x03\x04"),
            "the package is a ZIP file"
        );
        let text = String::from_utf8_lossy(&package).into_owned();
        assert!(
            text.contains("untitled_circuit/matrix/matrix"),
            "the job folder is named from the project"
        );
        assert!(
            text.contains("untitled_circuit/symbols/symbols")
                && text.contains("untitled_circuit/features/top")
                && text.contains("untitled_circuit/features/bottom")
                && text.contains("untitled_circuit/features/misc")
                && text.contains("untitled_circuit/components/top"),
            "the tree carries matrix, symbols, feature and component files"
        );
        assert!(
            text.contains(".Layer L1 GTL") && text.contains(".Layer L2 GBL"),
            "both copper layers are listed in the matrix"
        );
        assert!(
            text.contains(".Symbol C600") && text.contains(".EndSymbol"),
            "the via becomes a named circle symbol with a polygon boundary"
        );
        assert!(
            text.contains(".Symbol S"),
            "footprint pads become named rectangle symbols"
        );
        assert!(
            text.contains(".Line 0.300000"),
            "the track is written as a line feature with its width"
        );
        assert!(
            text.contains(".Component \"R1\"") && text.contains(".Feature \"1\""),
            "the placed resistor and its pad stack are listed"
        );
        Ok(())
    }

    #[test]
    fn pdf_documents_render_both_views_with_valid_cross_references() -> Result<(), String> {
        let mut project = Project::default();
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 101,
            net: 1,
            start: Point::new(5.0, 5.0),
            end: Point::new(40.0, 30.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
            arc: Some(Point::new(20.0, 20.0)),
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 102,
            net: 1,
            position: Point::new(40.0, 30.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 103,
            text: "GND (pour) 100%".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            size: 1.0,
            layer: pcbcraft_doc::Layer::Front,
            component: None,
        });
        project.drawn_wires.push(pcbcraft_doc::DrawnWire {
            id: 104,
            net: None,
            points: vec![Point::new(10.0, 5.0), Point::new(20.0, 5.0)],
        });
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        project.labels.push(pcbcraft_doc::Label {
            id: 105,
            kind: "global".into(),
            net: 1,
            position: Point::new(10.0, 5.0),
            rotation: 0.0,
            text: String::new(),
        });
        project.validate()?;
        for view in ["pcb", "schematic"] {
            let pdf = pdf_document(&project, view)?;
            assert!(pdf.starts_with(b"%PDF-1.4"), "a PDF names its version");
            assert!(pdf.windows(6).any(|w| w == b"%%EOF\n"));
            let text = String::from_utf8(pdf.clone()).map_err(|_| "PDFs are ASCII")?;
            let xref_at = text.find("\nxref\n").map(|at| at + 1).unwrap_or(usize::MAX);
            let stated: usize = text
                .rsplit_once("startxref\n")
                .map(|(_, tail)| tail.lines().next().unwrap_or_default())
                .and_then(|line| line.trim().parse().ok())
                .unwrap_or(usize::MAX);
            assert_eq!(
                stated, xref_at,
                "the trailer points exactly at the cross-reference table"
            );
            assert_eq!(
                &text[xref_at..xref_at + 4],
                "xref",
                "the cross-reference table sits where the trailer says"
            );
            let entries: Vec<usize> = text[xref_at..]
                .lines()
                .skip(2)
                .take(6)
                .filter_map(|l| l.split(' ').next().and_then(|v| v.parse().ok()))
                .collect();
            assert_eq!(
                entries.len(),
                6,
                "the free slot and the five objects are listed"
            );
            assert_eq!(entries[0], 0, "the first slot is the free entry");
            for (index, offset) in entries[1..].iter().enumerate() {
                assert!(
                    text[*offset..].starts_with(&format!("{} 0 obj", index + 1)),
                    "cross-reference entry {} points at object {}",
                    index + 1,
                    index + 1
                );
            }
            assert!(text.contains("BT /F1"));
            if view == "pcb" {
                assert!(
                    text.contains("(GND \\(pour\\) 100%)"),
                    "board text lands on the page with brackets escaped"
                );
            } else {
                assert!(
                    text.contains("({GND})"),
                    "the schematic page draws the global label"
                );
            }
        }
        assert!(pdf_document(&project, "layout").is_err());
        assert!(pdf_document(&Project::default(), "pcb")?.starts_with(b"%PDF-1.4"));
        Ok(())
    }

    #[test]
    fn vrml_board_extrudes_outline_and_pads_as_indexed_solids() -> Result<(), String> {
        let project = real_library_project()?;
        let scene = vrml_board(&project)?;
        assert!(
            scene.starts_with("#VRML V2.0 utf8\n"),
            "the scene declares VRML 2.0"
        );
        assert!(
            scene.matches("Shape {").count() == scene.matches("IndexedFaceSet").count(),
            "every shape carries exactly one solid"
        );
        assert!(
            scene.matches("IndexedFaceSet").count() >= 3,
            "the outline and both pads each become a solid"
        );
        for chunk in scene.split("Shape {").skip(1) {
            let points = chunk
                .split_once("point [")
                .ok_or("a solid is missing its points")?
                .1
                .split_once("]")
                .ok_or("points unterminated")?
                .0;
            let count = points
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty() && p.split_whitespace().count() == 3)
                .count();
            let indices = chunk
                .split_once("coordIndex [")
                .ok_or("a solid is missing its indices")?
                .1
                .split_once(']')
                .ok_or("indices unterminated")?
                .0;
            let highest = indices
                .split_whitespace()
                .filter(|t| *t != "-1")
                .map(|t| t.parse::<usize>().unwrap_or(usize::MAX))
                .max()
                .ok_or("a solid has no indices")?;
            assert!(
                highest < count,
                "every index names a point inside the solid"
            );
            assert!(
                indices.matches("-1").count() >= count / 4 + 2,
                "every quad and both caps are terminated"
            );
        }
        assert!(
            scene.contains("diffuseColor 0.13 0.55 0.13"),
            "copper solids are tinted"
        );
        Ok(())
    }

    #[test]
    fn step_board_extrudes_outline_and_pads_with_intact_references() -> Result<(), String> {
        let project = real_library_project()?;
        let step = step_board(&project)?;
        assert!(step.starts_with("ISO-10303-21;\nHEADER;"));
        assert!(step.ends_with("ENDSEC;\nEND-ISO-10303-21;\n"));
        assert!(step.contains("AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }"));
        let data = step
            .split_once("DATA;\n")
            .ok_or("data section missing")?
            .1
            .split_once("ENDSEC;")
            .ok_or("data section unterminated")?
            .0;
        let mut defined = std::collections::BTreeSet::new();
        for line in data.lines() {
            if let Some(rest) = line.strip_prefix('#') {
                let id: u32 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|t| t.split_whitespace().next())
                    .unwrap_or_default()
                    .parse()
                    .map_err(|_| "entity ids must be numbers")?;
                if !defined.insert(id) {
                    return Err("duplicate entity ids".into());
                }
            }
        }
        assert_eq!(defined.iter().next(), Some(&1), "numbering starts at one");
        assert_eq!(
            defined.len(),
            *defined.iter().next_back().unwrap_or(&0) as usize,
            "every id up to the highest is defined"
        );
        let mut references = Vec::new();
        let bytes = data.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'#' {
                let mut end = index + 1;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                let id: u32 = data[index + 1..end].parse().map_err(|_| "bad reference")?;
                references.push(id);
                index = end;
            } else {
                index += 1;
            }
        }
        assert!(
            references.iter().all(|r| defined.contains(r)),
            "every reference names a defined entity"
        );
        let faces = data.matches("ADVANCED_FACE(").count();
        let loops = data.matches("EDGE_LOOP(").count();
        assert_eq!(faces, loops, "every face has exactly one outer bound");
        assert!(
            faces >= 18,
            "the outline and both pads close at least six faces each"
        );
        assert!(data.contains("CARTESIAN_POINT('',(0.0000,0.0000,0.0000))"));
        let ring = vec![Point::new(0.0, 0.0); 100];
        assert!(
            thin_ring(&ring).len() <= 32,
            "long rounded rings are thinned for one file"
        );
        Ok(())
    }

    use std::collections::BTreeMap;
    fn real_library_project() -> Result<Project, String> {
        let definition = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/device.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "R")
        .ok_or("fixture resistor missing")?;
        let footprint = pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let pins = definition
            .pins
            .iter()
            .filter(|p| p.unit <= 1 && p.body_style <= 1)
            .map(|p| pcbcraft_doc::Pin {
                number: p.number.clone(),
                name: p.name.clone(),
                offset: p.position,
                net: None,
                electrical_type: p.electrical_type.clone(),
                direction: Some(p.angle),
                unit: p.unit,
            })
            .collect();
        let pads = footprint
            .pads
            .iter()
            .map(|p| pcbcraft_doc::Pad {
                number: p.number.clone(),
                offset: p.position,
                diameter: p.size.x.min(p.size.y),
                net: None,
                castellated: None,
                geometry: Some(p.clone()),
            })
            .collect();
        let mut project = Project::default();
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "library".into(),
            position: Point::new(20.0, 20.0),
            rotation: 0.0,
            pins,
            units: vec![],
            library_symbol: Some(pcbcraft_doc::LibrarySymbol {
                library: "Device".into(),
                unit: 1,
                definition,
            }),
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Resistor_SMD".into(),
                definition: footprint,
            }),
            fields: vec![],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 2,
            component: 1,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads,
        });
        project.validate()?;
        Ok(project)
    }
    fn extract_footprint(board: &str) -> Result<&str, String> {
        let start = board.find("(footprint ").ok_or("no exported footprint")?;
        let mut depth = 0;
        let mut quote = false;
        let mut escape = false;
        for (offset, c) in board[start..].char_indices() {
            if escape {
                escape = false;
                continue;
            }
            if quote && c == '\\' {
                escape = true;
                continue;
            }
            if c == '"' {
                quote = !quote;
            }
            if quote {
                continue;
            }
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    return Ok(&board[start..start + offset + 1]);
                }
            }
        }
        Err("unclosed exported footprint".into())
    }
    fn real_dip_project() -> Result<Project, String> {
        let mut project = real_library_project()?;
        let definition = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/lm358.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "LM358")
        .ok_or("fixture LM358 missing")?;
        let footprint = pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/DIP-8_W7.62mm.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let c = &mut project.components[0];
        c.reference = "U1".into();
        c.value = "LM358".into();
        c.pins = definition
            .pins
            .iter()
            .filter(|p| p.body_style <= 1)
            .map(|p| pcbcraft_doc::Pin {
                number: p.number.clone(),
                name: p.name.clone(),
                offset: p.position,
                net: None,
                electrical_type: p.electrical_type.clone(),
                direction: Some(p.angle),
                unit: p.unit,
            })
            .collect();
        c.library_symbol = Some(pcbcraft_doc::LibrarySymbol {
            library: "Amplifier_Operational".into(),
            unit: 1,
            definition,
        });
        c.units = vec![
            pcbcraft_doc::SymbolUnitPlacement {
                unit: 2,
                position: Point::new(40.0, 20.0),
                rotation: 90.0,
            },
            pcbcraft_doc::SymbolUnitPlacement {
                unit: 3,
                position: Point::new(60.0, 20.0),
                rotation: 0.0,
            },
        ];
        project.board.footprints[0].pads = footprint
            .pads
            .iter()
            .map(|p| pcbcraft_doc::Pad {
                number: p.number.clone(),
                offset: p.position,
                diameter: p.size.x.min(p.size.y),
                net: None,
                castellated: None,
                geometry: Some(p.clone()),
            })
            .collect();
        c.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
            library: "Package_DIP".into(),
            definition: footprint,
        });
        project.validate()?;
        Ok(project)
    }
    #[test]
    fn through_hole_export_preserves_drills_layers_and_offset_slots() -> Result<(), String> {
        let mut project = real_dip_project()?;
        project.board.footprints[0].rotation = 90.0;
        // A slot with a local offset tests metadata beyond the circular DIP fixture.
        let slot = pcbcraft_library::Drill {
            size: Point::new(0.8, 0.5),
            offset: Point::new(0.1, 0.0),
        };
        let c = &mut project.components[0];
        let source = c.library_footprint.as_mut().ok_or("no footprint")?;
        source.definition.pads[0].drill = Some(slot.clone());
        project.board.footprints[0].pads[0].geometry = Some(source.definition.pads[0].clone());
        let board = kicad_board(&project)?;
        assert!(board.contains("(attr through_hole)"));
        assert!(board.contains("(drill oval 0.8 0.5 (offset 0.1 0))"));
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        assert_eq!(exported.pads.len(), 8);
        assert!(
            exported
                .pads
                .iter()
                .all(|p| p.pad_type == "thru_hole" && p.layers == vec!["*.Cu", "*.Mask"])
        );
        assert_eq!(exported.pads[0].drill, Some(slot));
        assert_eq!(exported.pads[0].rotation, 90.0);
        Ok(())
    }
    #[test]
    fn dip_reference_and_value_are_outside_actual_footprint_bounds() -> Result<(), String> {
        let mut project = real_dip_project()?;
        project.board.footprints[0].rotation = 90.0;
        let f = &project.board.footprints[0];
        let (min, max) = footprint_bounds(&project, f);
        let x = (min.x + max.x) / 2.0;
        let board = kicad_board(&project)?;
        assert!(board.contains(&format!(
            "(property \"Reference\" \"U1\" (at {x} {} 90)",
            min.y - 1.2
        )));
        assert!(board.contains(&format!(
            "(property \"Value\" \"LM358\" (at {x} {} 90)",
            max.y + 1.2
        )));
        assert!(
            max.y > 7.62,
            "DIP8 geometry extends beyond generic 2.5mm value location"
        );
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        let original = project.components[0]
            .library_footprint
            .as_ref()
            .ok_or("missing footprint")?;
        let mut graphics = original.definition.graphics.clone();
        for g in &mut graphics {
            if let pcbcraft_library::Graphic::Text { angle, .. } = g {
                *angle += 90.0;
            }
        }
        assert_eq!(
            exported.graphics, graphics,
            "property placement must not alter library artwork"
        );
        Ok(())
    }
    #[test]
    fn all_symbol_units_and_drilled_holes_appear_in_previews() -> Result<(), String> {
        let project = real_dip_project()?;
        let views = project.components[0].schematic_views();
        assert_eq!(views.len(), 3);
        let schematic = svg(&project, "schematic")?;
        assert!(schematic.contains("translate(40 20) rotate(-90)"));
        assert!(schematic.contains("translate(60 20)"));
        for view in views {
            assert!(schematic.contains(&format!("{} ·", escape(&view.reference))));
        }
        assert!(!preview_png(&project, "schematic")?.is_empty());
        let pcb = svg(&project, "pcb")?;
        assert_eq!(pcb.matches("data-drill=").count(), 8);
        let image = image::load_from_memory(&preview_png(&project, "pcb")?)
            .map_err(|e| e.to_string())?
            .to_rgb8();
        let scale = (880.0 / project.board.width).min(560.0 / project.board.height);
        let x = (60.0 + 10.0 * scale).round() as u32;
        let y = (60.0 + 10.0 * scale).round() as u32;
        assert_eq!(image.get_pixel(x, y).0, [19, 28, 41]);
        assert_eq!(
            image.get_pixel(x + (0.65 * scale) as u32, y).0,
            [230, 194, 125]
        );
        Ok(())
    }
    #[test]
    fn shared_physical_pin_is_rendered_only_on_primary_unit() -> Result<(), String> {
        let mut project = real_library_project()?;
        let mut definition = project.components[0]
            .library_symbol
            .as_ref()
            .ok_or("no symbol")?
            .definition
            .clone();
        definition.pins[1].unit = 2;
        let mut common = definition.pins[0].clone();
        common.number = "C".into();
        common.name = "Common".into();
        common.unit = 0;
        common.position = Point::new(-20.0, 0.0);
        common.angle = 0.0;
        definition.pins.push(common);
        project.components[0] = pcbcraft_doc::library_component(
            definition,
            "test".into(),
            1,
            "X1".into(),
            Point::new(20.0, 20.0),
            None,
        )?;
        project.board.footprints.clear();
        project.validate()?;
        let schematic = svg(&project, "schematic")?;
        assert_eq!(
            schematic.matches(">C</text>").count(),
            1,
            "unit0 pin is one physical pin in primary unit, not a ghost pin in every view"
        );
        Ok(())
    }
    #[test]
    fn actual_footprint_export_preserves_geometry_layers_and_rotation() -> Result<(), String> {
        let mut project = real_library_project()?;
        project.board.footprints[0].rotation = 90.0;
        let board = kicad_board(&project)?;
        assert!(!board.contains("(start -3.4 -1.4)"));
        assert!(board.contains("(35 \"F.Paste\" user)"));
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        assert_eq!(exported.name, "Resistor_SMD:R_0805_2012Metric");
        let original = project.components[0]
            .library_footprint
            .as_ref()
            .ok_or("no embedded footprint")?;
        assert_eq!(exported.pads.len(), original.definition.pads.len());
        for (actual, expected) in exported.pads.iter().zip(&original.definition.pads) {
            assert_eq!(actual.number, expected.number);
            assert_eq!(actual.shape, expected.shape);
            assert_eq!(actual.size, expected.size);
            assert_eq!(actual.position, expected.position);
            assert_eq!(actual.layers, expected.layers);
            assert_eq!(actual.roundrect_ratio, expected.roundrect_ratio);
            assert_eq!(actual.rotation, expected.rotation + 90.0);
        }
        let mut expected_graphics = original.definition.graphics.clone();
        for graphic in &mut expected_graphics {
            if let pcbcraft_library::Graphic::Text { angle, .. } = graphic {
                *angle += 90.0;
            }
        }
        assert_eq!(exported.graphics, expected_graphics);
        let native = serde_json::to_string(&project).map_err(|e| e.to_string())?;
        let restored: Project = serde_json::from_str(&native).map_err(|e| e.to_string())?;
        restored.validate()?;
        assert_eq!(restored, project);
        Ok(())
    }
    #[test]
    fn project_footprint_export_uses_embedded_name_without_external_nickname() -> Result<(), String>
    {
        let mut project = real_library_project()?;
        let footprint = project.components[0]
            .library_footprint
            .as_mut()
            .ok_or("missing footprint")?;
        footprint.library = "project".into();
        footprint.definition.name = "Edited0805".into();
        let board = kicad_board(&project)?;
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        assert_eq!(exported.name, "pcbcraft_Edited0805");
        assert!(!board.contains("project:Edited0805"));
        assert_eq!(exported.pads.len(), 2);
        Ok(())
    }
    #[test]
    fn drawn_wires_render_in_the_schematic_preview_without_components() -> Result<(), String> {
        let mut project = Project::default();
        project.drawn_wires.push(pcbcraft_doc::DrawnWire {
            id: 1,
            net: None,
            points: vec![
                Point::new(10.0, 10.0),
                Point::new(20.0, 10.0),
                Point::new(20.0, 20.0),
            ],
        });
        project.validate()?;
        let schematic = svg(&project, "schematic")?;
        assert!(
            schematic.contains("points=\"10,10 20,10 20,20\""),
            "drawn wire vertices must render in order"
        );
        Ok(())
    }
    #[test]
    fn rotated_board_text_carries_its_angle_in_the_pcb_preview() -> Result<(), String> {
        let mut project = Project::default();
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 1,
            text: "Acme Labs".into(),
            position: Point::new(10.0, 5.0),
            size: 2.0,
            layer: pcbcraft_doc::Layer::Front,
            rotation: 90.0,
            component: None,
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 2,
            text: "Rev B".into(),
            position: Point::new(30.0, 5.0),
            size: 1.0,
            layer: pcbcraft_doc::Layer::Back,
            rotation: 0.0,
            component: None,
        });
        project.validate()?;
        let pcb = svg(&project, "pcb")?;
        assert!(
            pcb.contains("<text x=\"10\" y=\"5\" font-size=\"2\" fill=\"#dbe7f2\" transform=\"rotate(-90 10 5)\">Acme Labs</text>"),
            "a turned label must render with its angle about the anchor"
        );
        assert!(
            pcb.contains("<text x=\"30\" y=\"5\" font-size=\"1\" fill=\"#9ec5ff\">Rev B</text>"),
            "an upright label keeps the plain form"
        );
        Ok(())
    }
    #[test]
    fn bound_board_text_resolves_component_tokens_in_the_previews() -> Result<(), String> {
        let mut project = real_library_project()?;
        project.components[0]
            .fields
            .push(pcbcraft_doc::ComponentField {
                key: "MPN".into(),
                value: "RC0805".into(),
            });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 90,
            text: "${REFERENCE} ${VALUE} ${MPN}".into(),
            position: Point::new(12.0, 12.0),
            size: 1.0,
            layer: pcbcraft_doc::Layer::Front,
            rotation: 0.0,
            component: Some(1),
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 91,
            text: "${FOOTPRINT} ${MYSTERY}".into(),
            position: Point::new(14.0, 12.0),
            size: 1.0,
            layer: pcbcraft_doc::Layer::Front,
            rotation: 0.0,
            component: Some(1),
        });
        project.validate()?;
        let pcb = svg(&project, "pcb")?;
        assert!(
            pcb.contains(">R1 1k RC0805</text>"),
            "a bound label resolves reference, value and custom fields"
        );
        assert!(
            pcb.contains(">R_0805_2012Metric ${MYSTERY}</text>"),
            "the footprint token resolves and an unknown token stays put"
        );
        let dxf = dxf_board(&project)?;
        assert!(
            dxf.contains("R1 1k RC0805"),
            "the drawing file carries the resolved string too"
        );
        let pdf = pdf_document(&project, "pcb")?;
        assert!(
            String::from_utf8_lossy(&pdf).contains("R1 1k RC0805"),
            "the PDF drawing carries the resolved string"
        );
        Ok(())
    }
    #[test]
    fn zones_render_in_the_pcb_previews() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.zones.push(pcbcraft_doc::CopperZone {
            id: 2,
            net: 1,
            layer: pcbcraft_doc::Layer::Front,
            outline: vec![
                Point::new(5.0, 5.0),
                Point::new(45.0, 5.0),
                Point::new(45.0, 25.0),
                Point::new(5.0, 25.0),
                Point::new(5.0, 5.0),
            ],
            cutouts: vec![vec![
                Point::new(30.0, 15.0),
                Point::new(40.0, 15.0),
                Point::new(40.0, 20.0),
                Point::new(30.0, 20.0),
                Point::new(30.0, 15.0),
            ]],
            filled: true,

            kind: None,
        });
        project.validate()?;
        let pcb = svg(&project, "pcb")?;
        assert!(
            pcb.contains(
                "<polygon points=\"5 5 45 5 45 25 5 25 5 5\" fill=\"#f58d79\" fill-opacity=\"0.3\""
            ),
            "the zone outline must render as a filled copper polygon"
        );
        assert!(
            pcb.contains("<polygon points=\"30 15 40 15 40 20 30 20 30 15\" fill=\"none\""),
            "the zone cutout must render as an unfilled dashed outline"
        );
        let image = image::load_from_memory(&preview_png(&project, "pcb")?)
            .map_err(|e| e.to_string())?
            .to_rgb8();
        assert_eq!(
            image.get_pixel(500, 200).0,
            [245, 141, 121],
            "inside the zone the copper colour must be painted"
        );
        assert_eq!(
            image.get_pixel(700, 400).0,
            [25, 58, 54],
            "inside the zone cutout the substrate colour must show through"
        );
        assert_eq!(
            image.get_pixel(100, 600).0,
            [25, 58, 54],
            "outside the zone the board keeps its substrate colour"
        );
        Ok(())
    }
    #[test]
    fn inch_projects_export_imperial_fabrication_files() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 8,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 3,
            net: 8,
            start: Point::new(25.4, 25.4),
            end: Point::new(50.8, 25.4),
            width: 0.508,
            layer: pcbcraft_doc::Layer::Back,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 4,
            net: 8,
            position: Point::new(50.8, 25.4),
            diameter: 0.6,
            drill: 0.3,
        });
        project.unit = "inch".into();
        project.validate()?;
        let files = gerber_files(&project)?;
        let back = &files["pcb-B.Cu.gbr"];
        assert!(
            back.contains("MOIN*%"),
            "the format statement declares inches"
        );
        assert!(
            back.contains("X-1.000000Y1.000000D02*"),
            "25.4 mm becomes exactly one inch, mirrored on the back: {back}"
        );
        assert!(
            back.contains("C,0.020000*%"),
            "a 0.508 mm track is a 0.02 inch flash: {back}"
        );
        let drill = &files["pcb-Plated.drl"];
        assert!(
            drill.starts_with("M48\nPlated,1,2,PTH\nFMAT,2\nINCH,TZ\n"),
            "the drill header declares inches: {drill}"
        );
        assert!(
            drill.contains("T1C0.0118\n"),
            "a 0.3 mm bit is a 0.0118 inch bit"
        );
        assert!(
            drill.contains("X2.0000Y1.0000\n"),
            "the hit lands at exactly two inches by one inch: {drill}"
        );
        Ok(())
    }

    #[test]
    fn gerber_files_export_copper_outline_and_drills() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 8,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 3,
            net: 8,
            start: Point::new(10.0, 10.0),
            end: Point::new(30.0, 10.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Back,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 4,
            net: 8,
            position: Point::new(30.0, 10.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.zones.push(pcbcraft_doc::CopperZone {
            id: 5,
            net: 8,
            layer: pcbcraft_doc::Layer::Front,
            outline: vec![
                Point::new(5.0, 5.0),
                Point::new(45.0, 5.0),
                Point::new(45.0, 25.0),
                Point::new(5.0, 25.0),
                Point::new(5.0, 5.0),
            ],
            cutouts: vec![],
            filled: true,

            kind: None,
        });
        project.board.cutouts.push(pcbcraft_doc::Cutout {
            id: 6,
            center: Point::new(46.0, 20.0),
            radius: 2.0,
        });
        let through_pad = pcbcraft_doc::Pad {
            number: "1".into(),
            offset: Point::new(0.0, 0.0),
            diameter: 1.6,
            net: None,
            castellated: None,
            geometry: Some(pcbcraft_library::FootprintPad {
                number: "1".into(),
                pad_type: "thru_hole".into(),
                shape: "circle".into(),
                position: Point::new(0.0, 0.0),
                rotation: 0.0,
                size: Point::new(1.6, 1.6),
                layers: vec!["*.Cu".into(), "*.Mask".into()],
                drill: Some(pcbcraft_library::Drill {
                    size: Point::new(0.8, 0.8),
                    offset: Point::default(),
                }),
                roundrect_ratio: None,
                raw_source: String::new(),
                chamfer: None,
                polygon: Vec::new(),
            }),
        };
        let embedded_pad = through_pad.geometry.as_ref().ok_or("pad geometry")?.clone();
        let mut pad2 = through_pad.clone();
        pad2.number = "2".into();
        pad2.offset = Point::new(2.0, 0.0);
        if let Some(g) = pad2.geometry.as_mut() {
            g.number = "2".into();
            g.position = Point::new(2.0, 0.0);
        }
        let mut embedded_pad2 = embedded_pad.clone();
        embedded_pad2.number = "2".into();
        embedded_pad2.position = Point::new(2.0, 0.0);
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 2,
            component: 1,
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            locked: None,
            pads: vec![through_pad, pad2],
        });
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(20.0, 30.0),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(-1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(180.0),
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: String::new(),
                    offset: Point::new(1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(0.0),
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "ThroughHole".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "PAD-1.6".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![embedded_pad, embedded_pad2],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                    retained: vec![],
                },
            }),
            fields: vec![],
        });
        project.validate()?;
        let files = gerber_files(&project)?;
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            vec![
                "README-fabrication.txt",
                "pcb-B.Cu.gbr",
                "pcb-B.Mask.gbr",
                "pcb-B.Paste.gbr",
                "pcb-Drill.drl",
                "pcb-DrillMap.gbr",
                "pcb-Edge.gbr",
                "pcb-F.Cu.gbr",
                "pcb-F.Mask.gbr",
                "pcb-F.Paste.gbr",
                "pcb-NPTH.drl",
                "pcb-Plated.drl",
            ]
        );
        for key in files.keys() {
            let text = &files[key];
            if key.ends_with(".gbr") {
                assert!(
                    text.contains("%TF.FileFunction,"),
                    "{key} opens with a file function attribute"
                );
            }
        }
        let map = &files["pcb-DrillMap.gbr"];
        assert!(
            map.contains("%TF.FileFunction,Drillmap*%"),
            "the drill map declares its role"
        );
        assert!(
            map.contains("G04 LEGEND T1,"),
            "the drill map legend lists the first tool"
        );
        assert!(
            map.contains("X30.000000Y10.000000D03*\n"),
            "the via hole is flashed on the map"
        );
        assert!(
            map.contains("X2.000000Y0.000000D03*\n"),
            "the plated pad hole is flashed on the map"
        );
        assert!(map.ends_with("M02*\n"), "the map ends cleanly");
        let front = &files["pcb-F.Cu.gbr"];
        assert!(
            front.starts_with("G04 front copper layer generated by PCBCraft *\n")
                && front.contains("%TF.FileFunction,Copper,L1,Top*%")
                && front.contains("%FSLAX46Y46*%")
        );
        assert!(front.contains("%ADD10C,0.001000*%\n"), "zone fill aperture");
        assert!(
            front.contains("%ADD12C,0.600000*%\n"),
            "via copper aperture"
        );
        assert!(
            front.contains("%ADD11C,1.600000*%\n"),
            "through-hole pad aperture"
        );
        assert!(
            front.contains("G36*\nD02*\nX5.000000Y5.000000D02*\n"),
            "the poured zone must appear as a filled region"
        );
        assert!(
            front.contains("D12*\nX30.000000Y10.000000D03*\n"),
            "the via must flash on the front layer"
        );
        assert!(
            front.contains("D11*\nX2.000000Y0.000000D03*\n"),
            "the plated pad must flash on the front layer"
        );
        assert!(front.ends_with("M02*\n"));
        let back = &files["pcb-B.Cu.gbr"];
        assert!(
            back.contains("%TF.FileFunction,Copper,L2,Bot*%"),
            "back copper carries its file function"
        );
        assert!(
            back.contains("X-30.000000Y10.000000D01*\n"),
            "the back track end carries the mirrored coordinate"
        );
        assert!(
            back.contains("D12*\nX-30.000000Y10.000000D03*\n"),
            "the via must flash on the back layer too"
        );
        let outline = &files["pcb-Edge.gbr"];
        assert!(
            outline.contains("X0.000000Y0.000000D02*\nX60.000000Y0.000000D01*\n"),
            "the board edge must be traced"
        );
        assert!(
            outline.contains("X44.000000Y20.000000D02*\nI2.000000J0.000000G02*\n"),
            "the circular cutout must trace a circle"
        );
        let drill = &files["pcb-Plated.drl"];
        assert!(drill.starts_with("M48\nPlated,1,2,PTH\nFMAT,2\nMETRIC,TZ\n"));
        assert!(drill.contains("T1C0.300\n"), "via drill tool");
        assert!(drill.contains("T2C0.800\n"), "pad drill tool");
        assert!(drill.contains("T1\nX30.000Y10.000\n"), "via hole hit");
        assert!(drill.contains("T2\nX0.000Y0.000\n"), "pad hole hit");
        assert!(drill.ends_with("M30\n"));
        assert!(
            files["README-fabrication.txt"].contains("pcb-Plated.drl"),
            "the readme lists plated drills"
        );
        Ok(())
    }
    #[test]
    fn oval_drills_export_as_spanned_slots_following_pad_rotation() -> Result<(), String> {
        let mut project = Project::default();
        let oval_geometry = || pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "oval".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.5),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        let oval_pad = |component: u64| pcbcraft_doc::Footprint {
            id: 10 + component,
            component,
            position: if component == 1 {
                Point::new(10.0, 10.0)
            } else {
                Point::new(30.0, 10.0)
            },
            rotation: if component == 1 { 0.0 } else { 90.0 },
            locked: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(0.0, 0.0),
                diameter: 1.6,
                net: None,
                castellated: None,
                geometry: Some(oval_geometry()),
            }],
        };
        for id in 1..=2u64 {
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: format!("J{id}"),
                value: "slot".into(),
                kind: "connector".into(),
                position: Point::new(10.0, 30.0),
                rotation: 0.0,
                pins: vec![pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(0.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                }],
                units: vec![],
                library_symbol: None,
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Slots".into(),
                    definition: pcbcraft_library::FootprintDefinition {
                        name: "SLOT-1".into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![oval_geometry()],
                        graphics: vec![],
                        model: None,
                        rules: None,
                        raw_source: String::new(),
                        unsupported: vec![],
                        retained: vec![],
                    },
                }),
                fields: vec![],
            });
            project.board.footprints.push(oval_pad(id));
        }
        project.validate()?;
        let files = gerber_files(&project)?;
        let rout = &files["pcb-Rout.drl"];
        assert!(
            rout.starts_with("M48\nRout\n"),
            "oval slots land in the rout file"
        );
        assert!(
            rout.contains("T1C0.500\n"),
            "one narrow tool carries both slots"
        );
        assert!(
            rout.contains("X9.850Y10.000G85X10.150Y10.000\n"),
            "an unrotated oval spans along x"
        );
        assert!(
            rout.contains("X30.000Y10.150G85X30.000Y9.850\n"),
            "a rotated footprint rotates its slot"
        );
        assert!(rout.ends_with("M30\n"));
        Ok(())
    }
    #[test]
    fn placement_text_lists_placed_parts_by_reference_with_side_and_rotation() -> Result<(), String>
    {
        let mut project = Project::default();
        let geometry = |number: &str, x: f64, back: bool| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: if back { "thru_hole" } else { "smd" }.into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: if back {
                vec![
                    "F.Cu".into(),
                    "B.Cu".into(),
                    "F.Mask".into(),
                    "B.Mask".into(),
                ]
            } else {
                vec!["F.Cu".into(), "F.Mask".into()]
            },
            drill: back.then(|| pcbcraft_library::Drill {
                size: Point::new(0.5, 0.5),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        let component =
            |id: u64, reference: &str, definition: &str, back: bool| pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Test".into(),
                    definition: pcbcraft_library::FootprintDefinition {
                        name: definition.into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![geometry("1", -1.0, back), geometry("2", 1.0, back)],
                        graphics: vec![],
                        model: None,
                        rules: None,
                        raw_source: String::new(),
                        unsupported: vec![],
                        retained: vec![],
                    },
                }),
                fields: vec![],
            };
        let footprint = |id: u64, component: u64, x: f64, y: f64, rotation: f64, back: bool| {
            pcbcraft_doc::Footprint {
                id,
                component,
                position: Point::new(x, y),
                rotation,
                locked: None,
                pads: vec![
                    pcbcraft_doc::Pad {
                        number: "1".into(),
                        offset: Point::new(-1.0, 0.0),
                        diameter: 1.0,
                        net: None,
                        castellated: None,
                        geometry: Some(geometry("1", -1.0, back)),
                    },
                    pcbcraft_doc::Pad {
                        number: "2".into(),
                        offset: Point::new(1.0, 0.0),
                        diameter: 1.0,
                        net: None,
                        castellated: None,
                        geometry: Some(geometry("2", 1.0, back)),
                    },
                ],
            }
        };
        project.components.push(component(1, "R5", "R0603", false));
        project.components.push(component(2, "U1", "SOIC-2", true));
        project
            .board
            .footprints
            .push(footprint(10, 1, 12.0, 8.0, 90.0, false));
        project
            .board
            .footprints
            .push(footprint(11, 2, 20.0, 15.0, -45.0, true));
        project.validate()?;
        let text = placement_text(&project)?;
        assert!(
            text.starts_with("File Format,ASCII,\n"),
            "a centroid file starts with its format line"
        );
        assert!(
            text.contains("SMT,THT,X,Y,Rotation,Side,Package\n"),
            "the column header names the fields"
        );
        let rows: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("SMT,\"") || l.starts_with("THT,\""))
            .collect();
        assert_eq!(
            rows.first(),
            Some(&"SMT,\"R5\",12.000,8.000,90.0,T,\"R0603\""),
            "front parts carry their position, rotation and package"
        );
        assert_eq!(
            rows.last(),
            Some(&"THT,\"U1\",20.000,15.000,315.0,T,\"SOIC-2\""),
            "plated through-hole pads are listed as THT and the rotation wraps positive"
        );
        assert_eq!(rows.len(), 2, "one row per placed part");
        Ok(())
    }
    #[test]
    fn report_text_lists_parts_and_net_pins_under_a_board_summary() -> Result<(), String> {
        let mut project = Project {
            name: "LED indicator".into(),
            ..Default::default()
        };
        for (id, reference) in [(1, "R1"), (2, "D1")] {
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "2V".into(),
                kind: "led".into(),
                position: Point::new(10.0, 5.0),
                rotation: -90.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: Some(7),
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            });
        }
        project.nets.push(pcbcraft_doc::Net {
            id: 7,
            name: "SIG".into(),
            ..Default::default()
        });
        project.validate()?;
        let text = report_text(&project)?;
        assert!(
            text.starts_with("Design report for LED indicator\n"),
            "the report opens with the design name"
        );
        assert!(
            text.contains("Board: ") && text.contains("Counts: 2 components, 1 nets"),
            "the summary counts the design: {text}"
        );
        assert!(
            text.contains("D1,2V,-,10.000,5.000,270.0\nR1"),
            "parts sort by reference and the rotation wraps positive: {text}"
        );
        assert!(
            text.contains("SIG,\"D1.1 R1.1\"\n"),
            "the net lists every pin on it: {text}"
        );
        Ok(())
    }

    #[test]
    fn cmp_text_lists_every_pad_and_connected_net_in_mils() -> Result<(), String> {
        let mut project = Project::default();
        let geometry = |number: &str, x: f64, back: bool| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: if back { "thru_hole" } else { "smd" }.into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: if back {
                vec![
                    "F.Cu".into(),
                    "B.Cu".into(),
                    "F.Mask".into(),
                    "B.Mask".into(),
                ]
            } else {
                vec!["F.Cu".into(), "F.Mask".into()]
            },
            drill: back.then(|| pcbcraft_library::Drill {
                size: Point::new(0.5, 0.5),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        let component =
            |id: u64, reference: &str, definition: &str, back: bool| pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: Some(50),
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Test".into(),
                    definition: pcbcraft_library::FootprintDefinition {
                        name: definition.into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![geometry("1", -1.0, back), geometry("2", 1.0, back)],
                        graphics: vec![],
                        model: None,
                        rules: None,
                        raw_source: String::new(),
                        unsupported: vec![],
                        retained: vec![],
                    },
                }),
                fields: vec![],
            };
        let footprint = |id: u64, component: u64, x: f64, y: f64, rotation: f64, back: bool| {
            pcbcraft_doc::Footprint {
                id,
                component,
                position: Point::new(x, y),
                rotation,
                locked: None,
                pads: vec![
                    pcbcraft_doc::Pad {
                        number: "1".into(),
                        offset: Point::new(-1.0, 0.0),
                        diameter: 1.0,
                        net: Some(50),
                        castellated: None,
                        geometry: Some(geometry("1", -1.0, back)),
                    },
                    pcbcraft_doc::Pad {
                        number: "2".into(),
                        offset: Point::new(1.0, 0.0),
                        diameter: 1.0,
                        net: None,
                        castellated: None,
                        geometry: Some(geometry("2", 1.0, back)),
                    },
                ],
            }
        };
        project.nets.push(pcbcraft_doc::Net {
            id: 50,
            name: "PWR".into(),
            target_length: None,
            length_tolerance: None,
        });
        project.components.push(component(1, "R5", "R0603", false));
        project.components.push(component(2, "U1", "SOIC-2", true));
        project
            .board
            .footprints
            .push(footprint(10, 1, 12.0, 8.0, 0.0, false));
        project
            .board
            .footprints
            .push(footprint(11, 2, 20.0, 15.0, 0.0, true));
        project.validate()?;
        let text = cmp_text(&project)?;
        assert!(
            text.starts_with("$FILE\nTYPE:BOARD\nCODE:ASCII\n"),
            "a trace file starts with its header"
        );
        let pads: Vec<&str> = text.lines().filter(|l| l.starts_with("$CD ")).collect();
        assert_eq!(pads.len(), 4, "one line per placed pad");
        assert!(
            pads.contains(&"$CD R5\t1\t433\t315\t0\t1\t0"),
            "front pad one of R5 sits at 11 mm, 8 mm in whole mils: {pads:?}"
        );
        assert!(
            pads.iter()
                .any(|l| l.starts_with("$CD U1\t1\t") && l.contains("\t1\t")),
            "back-side pads carry the back layer code"
        );
        let net_line = text
            .lines()
            .find(|l| l.starts_with("PWR\t"))
            .ok_or("the named net is missing")?;
        assert!(
            net_line.starts_with("PWR\t2\t1\t1\t2\t1"),
            "the net lists both footprints' first pad: {net_line}"
        );
        assert!(
            text.ends_with("$NETS\n") || text.contains("$NETS"),
            "sections close"
        );
        assert_eq!(
            cmp_text(&Project::default()).err(),
            Some("board has no placed pads or connected nets to export".into()),
            "an empty board is rejected"
        );
        Ok(())
    }
    #[test]
    fn ipc2581_text_carries_nets_placement_and_copper_as_xml() -> Result<(), String> {
        let mut project = Project::default();
        let geometry = |number: &str, x: f64, back: bool| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: if back { "thru_hole" } else { "smd" }.into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: if back {
                vec![
                    "F.Cu".into(),
                    "B.Cu".into(),
                    "F.Mask".into(),
                    "B.Mask".into(),
                ]
            } else {
                vec!["F.Cu".into(), "F.Mask".into()]
            },
            drill: back.then(|| pcbcraft_library::Drill {
                size: Point::new(0.5, 0.5),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        let component =
            |id: u64, reference: &str, definition: &str, back: bool| pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: Some(50),
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Test".into(),
                    definition: pcbcraft_library::FootprintDefinition {
                        name: definition.into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![geometry("1", -1.0, back), geometry("2", 1.0, back)],
                        graphics: vec![],
                        model: None,
                        rules: None,
                        raw_source: String::new(),
                        unsupported: vec![],
                        retained: vec![],
                    },
                }),
                fields: vec![],
            };
        let footprint =
            |id: u64, component: u64, x: f64, y: f64, back: bool| pcbcraft_doc::Footprint {
                id,
                component,
                position: Point::new(x, y),
                rotation: 0.0,
                locked: None,
                pads: vec![
                    pcbcraft_doc::Pad {
                        number: "1".into(),
                        offset: Point::new(-1.0, 0.0),
                        diameter: 1.0,
                        net: Some(50),
                        castellated: None,
                        geometry: Some(geometry("1", -1.0, back)),
                    },
                    pcbcraft_doc::Pad {
                        number: "2".into(),
                        offset: Point::new(1.0, 0.0),
                        diameter: 1.0,
                        net: None,
                        castellated: None,
                        geometry: Some(geometry("2", 1.0, back)),
                    },
                ],
            };
        project.nets.push(pcbcraft_doc::Net {
            id: 50,
            name: "P&&R".into(),
            target_length: None,
            length_tolerance: None,
        });
        project.components.push(component(1, "R5", "R0603", false));
        project.components.push(component(2, "U1", "SOIC-2", true));
        project
            .board
            .footprints
            .push(footprint(10, 1, 12.0, 8.0, false));
        project
            .board
            .footprints
            .push(footprint(11, 2, 20.0, 15.0, true));
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 20,
            net: 50,
            start: Point::new(11.0, 8.0),
            end: Point::new(19.0, 15.0),
            width: 0.25,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 30,
            net: 50,
            position: Point::new(15.0, 11.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 40,
            position: Point::new(2.0, 2.0),
            diameter: 3.0,
            end: None,
        });
        project.validate()?;
        let text = ipc2581_text(&project)?;
        assert!(
            text.starts_with(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<EDA3D version=\"IPC-2581\""
            ),
            "the document opens as XML naming the standard"
        );
        assert!(
            text.contains("<NET name=\"P&amp;&amp;R\">"),
            "net names are escaped for XML"
        );
        assert!(
            text.contains("<PIN id=\"R5.1\"/>") && text.contains("<PIN id=\"U1.1\"/>"),
            "the net lists the pins that share it"
        );
        assert!(
            text.contains("<COMPONENT ref=\"U1\" package=\"resistor\" x=\"20.000\" y=\"15.000\""),
            "placed parts carry their reference, package and position"
        );
        assert!(
            text.contains("<TRACK start_x=\"11.000\"") && text.contains("net=\"P&amp;&amp;R\""),
            "front copper is listed with its net"
        );
        assert!(
            text.contains("drill=\"0.300\"") && text.contains("plated=\"true\""),
            "the plated via is a hole"
        );
        assert!(
            text.contains("drill=\"3.000\" plated=\"false\"")
                || text.contains("diameter=\"3.000\" plated=\"false\""),
            "the unplated mounting hole is marked unplated"
        );
        assert!(
            text.contains("<LAYER name=\"circuit1\" copper=\"B.Cu\">"),
            "both copper layers get their own section"
        );
        assert!(text.ends_with("</EDA3D>\n"), "the document closes cleanly");
        Ok(())
    }
    #[test]
    fn idf_board_carries_size_and_placed_footprints_for_mechanical_tools() -> Result<(), String> {
        let mut project = Project::default();
        project.board.width = 60.0;
        project.board.height = 40.0;
        let geometry = |number: &str, x: f64| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "R5".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pins: vec![pcbcraft_doc::Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::new(-1.0, 0.0),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Test".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "TEST_ONE".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![geometry("1", -1.0)],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                    retained: vec![],
                },
            }),
            fields: vec![],
        });
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 2,
            reference: "U1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(20.0, 10.0),
            rotation: 0.0,
            pins: vec![pcbcraft_doc::Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::new(-1.0, 0.0),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Test".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "TEST_ONE".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![geometry("1", 0.0)],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                    retained: vec![],
                },
            }),
            fields: vec![],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 10,
            component: 1,
            position: Point::new(12.5, 8.25),
            rotation: 90.0,
            locked: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(-1.0, 0.0),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: Some(geometry("1", -1.0)),
            }],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 11,
            component: 2,
            position: Point::new(30.0, 20.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(0.0, 0.0),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: Some(geometry("1", 0.0)),
            }],
        });
        project.validate()?;
        let text = idf_board(&project)?;
        assert!(
            text.starts_with(".FILE_IDF\n.3DIDF VERSION: 2.0\n.UNITS:\tmillmm\n"),
            "a mechanical drawing declares itself in millimetres"
        );
        assert!(
            text.contains(".BODY 60.000 40.000 0.000 0.000"),
            "the board outline size is carried"
        );
        assert!(
            text.contains(".END_BODY R5\t12.500\t8.250\tFRONT\t90.000\t0\n"),
            "front parts list position, side and rotation"
        );
        assert!(
            text.contains(".END_BODY U1\t30.000\t20.000\tFRONT\t0.000\t1\n"),
            "every part is listed in placement order"
        );
        assert!(
            text.contains(".PIN 2\n") && text.matches(".END_PIN {\t}").count() == 2,
            "the embedded pin file lists every part with a default height"
        );
        assert!(text.ends_with(".END_FILE_IDF\n"), "the file closes");
        assert_eq!(
            idf_board(&Project::default()).err(),
            Some("board has no placed footprints to export".into()),
            "an empty board is rejected"
        );
        Ok(())
    }
    #[test]
    fn fiducial_marks_travel_in_the_pick_and_place_export() -> Result<(), String> {
        let mut project = Project::default();
        project.board.fiducials.extend([
            pcbcraft_doc::Fiducial {
                id: 901,
                position: Point::new(2.0, 3.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            },
            pcbcraft_doc::Fiducial {
                id: 902,
                position: Point::new(58.0, 37.0),
                diameter: 1.2,
                layer: pcbcraft_doc::Layer::Back,
            },
        ]);
        let text = placement_text(&project)?;
        assert!(
            text.contains("SMT,\"FID01\",2.000,3.000,0.0,T,FIDUCIAL"),
            "the front mark is listed as a top-side fiducial"
        );
        assert!(
            text.contains("SMT,\"FID02\",58.000,37.000,0.0,B,FIDUCIAL"),
            "the back mark is listed on the bottom side"
        );
        Ok(())
    }

    #[test]
    fn specctra_dsn_carries_nets_placements_tracks_vias_and_keepouts() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.extend([
            pcbcraft_doc::Net {
                id: 5,
                name: "GND".into(),
                ..Default::default()
            },
            pcbcraft_doc::Net {
                id: 9,
                name: "SIG".into(),
                ..Default::default()
            },
        ]);
        project.net_classes.push(pcbcraft_doc::NetClass {
            id: 11,
            name: "POWER".into(),
            clearance: 0.4,
            width: 0.6,
            min_width: None,
            max_width: None,
            target_length: None,
            length_tolerance: None,
            via_diameter: None,
            via_drill: None,
            pair_width: None,
            pair_gap: None,
            nets: vec![5],
            profile: None,
        });
        let component =
            |id: u64, reference: &str, nets: [Option<u64>; 2]| pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: ["1", "2"]
                    .iter()
                    .map(|number| pcbcraft_doc::Pin {
                        number: (*number).into(),
                        name: String::new(),
                        offset: Point::default(),
                        net: nets[if *number == "1" { 0 } else { 1 }],
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    })
                    .collect(),
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            };
        project
            .components
            .push(component(1, "R1", [Some(9), Some(5)]));
        project.components.push(component(2, "R2", [Some(9), None]));
        let pad = |number: &str, x: f64, y: f64, net: Option<u64>| pcbcraft_doc::Pad {
            number: number.into(),
            offset: Point::new(x, y),
            diameter: 0.6,
            net,
            castellated: None,
            geometry: None,
        };
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 3,
            component: 1,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pad("1", -1.0, 0.0, Some(9)), pad("2", 1.0, 0.0, Some(5))],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 4,
            component: 2,
            position: Point::new(30.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pad("1", -1.0, 0.0, Some(9)), pad("2", 1.0, 0.0, None)],
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 6,
            net: 9,
            start: Point::new(11.0, 10.0),
            end: Point::new(29.0, 10.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 7,
            net: 9,
            position: Point::new(20.0, 15.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.rule_areas.push(pcbcraft_doc::RuleArea {
            id: 8,
            name: None,
            net: None,
            layer: None,
            clearance: 0.5,
            max_height: None,
            region: pcbcraft_doc::RuleRegion {
                x: 5.0,
                y: 5.0,
                width: 10.0,
                height: 8.0,
            },
            profile: None,
        });
        let dsn = specctra_dsn(&project)?;
        assert!(
            dsn.starts_with("(PCB \""),
            "the file is an s-expression PCB"
        );
        assert!(dsn.contains("(resolution 1000.0 1000.0)"));
        assert!(
            dsn.contains("(net 2 \"SIG\" (pins \"R1\".\"1\" \"R2\".\"1\"))"),
            "nets list their pins by reference and pad number"
        );
        assert!(dsn.contains("(netclass \"POWER\" (use \"POWER\"))"));
        assert!(
            dsn.contains("(classpath \"POWER\" (circuit (minClearance 400)"),
            "classes carry their clearance in micrometres"
        );
        assert!(
            dsn.contains("(component (name \"R1\") (place (type fixed) (side top) (rotate 0.0) (at (10000 10000))))"),
            "components are placed with fixed type"
        );
        assert!(
            dsn.contains("(pin (name \"R1\".\"1\") (length 0) (at (9000 10000))"),
            "pads land at their world position"
        );
        assert!(
            dsn.contains(
                "(wire (path (net 2) (layer F.Cu) (width 300) (11000 10000) (29000 10000)))"
            ),
            "tracks become net-carrying wires"
        );
        assert!(dsn.contains("(via (net 2) (type VIA) (at (20000 15000)))"));
        assert!(
            dsn.contains("(polygon (layer BOARD_BOUNDARY) (net 0) (path (0 0) (60000 0) (60000 40000) (0 40000)))"),
            "the board edge closes the boundary"
        );
        assert!(
            dsn.contains("(keepout (layer F.Cu) (net 0) (rect (5000 5000) (15000 13000)))")
                && dsn.contains("(keepout (layer B.Cu) (net 0) (rect (5000 5000) (15000 13000)))"),
            "rule areas become keepouts on both copper layers"
        );
        Ok(())
    }

    #[test]
    fn ipc356_netlist_orders_pads_by_net_and_prints_fixed_columns() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.extend([
            pcbcraft_doc::Net {
                id: 5,
                name: "GND".into(),
                ..Default::default()
            },
            pcbcraft_doc::Net {
                id: 9,
                name: "SIG".into(),
                ..Default::default()
            },
        ]);
        let component =
            |id: u64, reference: &str, nets: [Option<u64>; 2]| pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: ["1", "2"]
                    .iter()
                    .map(|number| pcbcraft_doc::Pin {
                        number: (*number).into(),
                        name: String::new(),
                        offset: Point::default(),
                        net: nets[if *number == "1" { 0 } else { 1 }],
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    })
                    .collect(),
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            };
        project.components.push(component(1, "R2", [None, Some(9)]));
        project
            .components
            .push(component(2, "R1", [Some(5), Some(9)]));
        let pad = |number: &str, x: f64, y: f64, net: Option<u64>| pcbcraft_doc::Pad {
            number: number.into(),
            offset: Point::new(x, y),
            diameter: 0.6,
            net,
            castellated: None,
            geometry: None,
        };
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 3,
            component: 1,
            position: Point::new(20.0, 10.0),
            rotation: 90.0,
            locked: None,
            pads: vec![pad("1", -1.0, 0.0, None), pad("2", 1.0, 0.0, Some(9))],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 4,
            component: 2,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pad("1", 0.0, 0.0, Some(5)), pad("2", 2.0, 0.0, Some(9))],
        });
        let text = ipc356_netlist(&project)?;
        let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('*')).collect();
        assert_eq!(lines.len(), 4, "one line per pad");
        assert!(
            lines[0].starts_with("00000R2"),
            "unconnected pads sort first"
        );
        assert!(
            lines[1].starts_with("00001R1"),
            "nets number in project order"
        );
        assert!(lines[2].starts_with("00002R1"));
        assert!(
            lines[3].starts_with("00002R2"),
            "same net sorts by reference"
        );
        for line in &lines {
            assert_eq!(line.chars().count(), 49, "fixed 49-column records");
        }
        assert!(
            lines[0].ends_with("0002000000011000"),
            "pad coordinates follow the rotated footprint"
        );
        assert!(lines[1].ends_with("0001000000010000"));
        Ok(())
    }

    #[test]
    fn drill_origin_shifts_drill_coordinates_but_not_copper() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 7,
            name: "SIG".into(),
            ..Default::default()
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 9,
            net: 7,
            position: Point::new(30.0, 20.0),
            diameter: 0.7,
            drill: 0.3,
        });
        project.board.drill_origin = Point::new(20.0, 10.0);
        project.validate()?;
        let files = gerber_files(&project)?;
        let drill = &files["pcb-Plated.drl"];
        assert!(
            drill.contains("X10.000Y10.000\n"),
            "drill coordinates are measured from the recorded origin"
        );
        assert!(
            !drill.contains("X30.000Y20.000"),
            "absolute coordinates are gone once an origin is recorded"
        );
        let copper = &files["pcb-F.Cu.gbr"];
        assert!(
            copper.contains("X30.000000Y20.000000") || copper.contains("X30.000Y20.000"),
            "copper layers keep absolute coordinates"
        );
        project.board.drill_origin = Point::new(-5.0, 0.0);
        let files = gerber_files(&project)?;
        assert!(
            files["pcb-Plated.drl"].contains("X35.000Y20.000\n"),
            "a negative origin shifts the other way"
        );
        Ok(())
    }
    #[test]
    fn a_traced_board_outline_replaces_the_rectangle_in_fabrication_output() -> Result<(), String> {
        let mut project = Project::default();
        project.board.outline = Some(vec![
            Point::new(0.0, 0.0),
            Point::new(60.0, 0.0),
            Point::new(60.0, 40.0),
            Point::new(30.0, 40.0),
            Point::new(30.0, 20.0),
            Point::new(0.0, 20.0),
        ]);
        project.validate()?;
        let board = kicad_board(&project)?;
        assert!(
            board.contains("gr_poly") && !board.contains("gr_rect"),
            "the outline exports as one polygon, not a rectangle"
        );
        let files = gerber_files(&project)?;
        let outline = &files["pcb-Edge.gbr"];
        assert!(
            outline.contains("X30.000000Y40.000000D01*")
                && outline.contains("X30.000000Y20.000000D01*"),
            "the gerber edge layer traces every vertex"
        );
        let drawing = svg(&project, "pcb")?;
        assert!(
            drawing.contains("<polygon") && !drawing.contains("<rect width=\"60\""),
            "the preview draws the traced shape"
        );
        let dxf = dxf_board(&project)?;
        assert!(
            dxf.contains("30.0") && dxf.contains("40.0"),
            "the DXF edge polyline carries the outline vertices"
        );
        Ok(())
    }
    #[test]
    fn curved_tracks_export_as_true_arcs_in_every_format() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "A".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 2,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(20.0, 10.0),
            width: 0.2,
            layer: pcbcraft_doc::Layer::Front,
            arc: Some(Point::new(15.0, 12.0)),
        });
        project.validate()?;
        let board = kicad_board(&project)?;
        assert!(
            board.contains("(arc (start 10 10) (mid 15 12) (end 20 10)"),
            "the board export keeps the curve as an arc, not a segment"
        );
        let files = gerber_files(&project)?;
        let copper = &files["pcb-F.Cu.gbr"];
        assert!(
            copper.contains("G02*") && copper.contains("J-5.250000"),
            "the gerber draws a clockwise arc to the computed centre"
        );
        let drawing = svg(&project, "pcb")?;
        assert!(
            drawing.contains("A 7.250000 7.250000 0 0 1 20 10"),
            "the preview arc uses the true radius and flags"
        );
        let dxf = dxf_board(&project)?;
        assert!(
            dxf.contains("0\nARC") && dxf.contains("40\n7.250"),
            "the DXF drawing carries an ARC entity"
        );
        Ok(())
    }
    #[test]
    fn dxf_board_draws_tracks_vias_arcs_and_labels_as_entities() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 7,
            name: "SIG".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 10,
            net: 7,
            start: Point::new(10.0, 10.0),
            end: Point::new(30.0, 10.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 11,
            net: 7,
            position: Point::new(30.0, 20.0),
            diameter: 0.7,
            drill: 0.3,
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 12,
            layer: "board".into(),
            shape: "arc".into(),
            points: vec![
                Point::new(40.0, 10.0),
                Point::new(47.071, 17.071),
                Point::new(54.0, 10.0),
            ],
            width: 0.2,
            text: String::new(),
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 13,
            layer: "schematic".into(),
            shape: "line".into(),
            points: vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)],
            width: 0.2,
            text: String::new(),
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 14,
            text: "TOP".into(),
            position: Point::new(5.0, 5.0),
            size: 1.5,
            layer: pcbcraft_doc::Layer::Front,
            rotation: 90.0,
            component: None,
        });
        project.validate()?;
        let dxf = dxf_board(&project)?;
        assert!(
            dxf.starts_with("0\nSECTION\n2\nHEADER\n"),
            "a DXF file starts with a header section"
        );
        assert!(dxf.contains("$INSUNITS\n70\n4\n"), "millimetre units");
        assert!(
            dxf.ends_with("0\nENDSEC\n0\nEOF\n"),
            "a DXF file ends at EOF"
        );
        assert!(
            dxf.contains("0\nLWPOLYLINE\n8\nEdge\n90\n4\n70\n1\n"),
            "the board edge is a closed four-vertex polyline"
        );
        assert!(
            dxf.contains("0\nLINE\n8\nF_Cu\n10\n10.000\n20\n10.000\n11\n30.000\n21\n10.000\n"),
            "the front track becomes a line on the front copper layer"
        );
        assert!(
            dxf.contains("0\nCIRCLE\n8\nVia\n10\n30.000\n20\n20.000\n40\n0.350\n"),
            "the via pad becomes a circle"
        );
        assert!(
            dxf.contains("0\nCIRCLE\n8\nDrill\n10\n30.000\n20\n20.000\n40\n0.150\n"),
            "the via drill becomes its own circle"
        );
        assert!(
            dxf.contains("0\nARC\n8\nBoard_Graphics\n10\n47.000\n20\n10.071\n40\n7.000\n50\n359.419\n51\n180.581\n"),
            "a three-point board arc becomes a centre-radius arc"
        );
        assert!(
            !dxf.contains("0\nLINE\n8\nBoard_Graphics\n"),
            "schematic graphics stay out of the board drawing"
        );
        assert!(
            dxf.contains("0\nTEXT\n8\nF_Silk\n10\n5.000\n20\n5.000\n40\n1.500\n50\n90.0\n1\nTOP\n"),
            "board labels become rotated text entities"
        );
        assert!(
            !dxf.contains("\n0\nENTITY\n"),
            "entities carry their own type, not a placeholder"
        );
        assert_eq!(dxf_entities(&dxf)?.len(), 6, "six entities in all");
        Ok(())
    }
    #[test]
    fn dxf_entities_reads_back_the_shape_kinds_it_exports() -> Result<(), String> {
        let text = "0\nSECTION\n2\nENTITIES\n\
            0\nLINE\n8\nF_Cu\n10\n1.5\n20\n2.5\n11\n6.0\n21\n2.5\n\
            0\nCIRCLE\n8\nDrill\n10\n30\n20\n20\n40\n0.15\n\
            0\nARC\n8\nF_Cu\n10\n47\n20\n10\n40\n7\n50\n359\n51\n180\n\
            0\nLWPOLYLINE\n8\nEdge\n90\n4\n70\n1\n10\n0\n20\n0\n10\n60\n20\n0\n10\n60\n20\n40\n10\n0\n20\n40\n\
            0\nTEXT\n8\nF_Silk\n10\n5\n20\n5\n40\n1.5\n50\n90\n1\nR7\n\
            0\nSPLINE\n8\nX\n10\n1\n20\n1\n\
            0\nENDSEC\n0\nEOF\n";
        let entities = dxf_entities(text)?;
        assert_eq!(entities.len(), 5, "the unsupported spline is skipped");
        assert_eq!(
            entities[0],
            DxfEntity {
                layer: "F_Cu".into(),
                shape: DxfShape::Line {
                    start: Point::new(1.5, 2.5),
                    end: Point::new(6.0, 2.5),
                }
            }
        );
        assert!(matches!(
            entities[1].shape,
            DxfShape::Circle {
                center: Point { x: 30.0, y: 20.0 },
                radius: 0.15
            }
        ));
        assert!(matches!(
            entities[2].shape,
            DxfShape::Arc {
                radius: 7.0,
                start_deg: 359.0,
                end_deg: 180.0,
                ..
            }
        ));
        match &entities[3].shape {
            DxfShape::Polyline { points, closed } => {
                assert!(*closed, "the closed polyline reads back closed");
                assert_eq!(points.len(), 4);
                assert_eq!(points[3], Point::new(0.0, 40.0));
            }
            _ => return Err("a polyline should read back as a polyline".into()),
        }
        assert_eq!(
            entities[4],
            DxfEntity {
                layer: "F_Silk".into(),
                shape: DxfShape::Text {
                    at: Point::new(5.0, 5.0),
                    height: 1.5,
                    rotation: 90.0,
                    value: "R7".into(),
                }
            }
        );
        Ok(())
    }

    #[test]
    fn dxf_entities_reads_the_paired_export_format_with_layers() -> Result<(), String> {
        let mut project = Project::default();
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 1,
            layer: "board".into(),
            shape: "line".into(),
            points: vec![Point::new(1.0, 2.0), Point::new(9.0, 2.0)],
            width: 0.2,
            text: String::new(),
        });
        let text = dxf_board(&project)?;
        let entities = dxf_entities(&text)?;
        assert!(
            entities.iter().any(|e| e.layer == "Edge"
                && matches!(&e.shape, DxfShape::Polyline { closed: true, points } if points.len() == 4)),
            "the board outline reads back as a closed rectangle"
        );
        assert!(
            entities.iter().any(|e| e.layer == "Board_Graphics"
                && matches!(&e.shape, DxfShape::Polyline { points, closed: false }
                    if points == &vec![Point::new(1.0, 2.0), Point::new(9.0, 2.0)])),
            "the board graphic reads back on its layer"
        );
        Ok(())
    }

    #[test]
    fn dxf_entities_rejects_malformed_drawings() {
        let bad = [
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nX\n",
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n1\n20\n2\n",
            "0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n10\n1\n20\n2\n40\n0\n",
            "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n70\n1\n10\n1\n20\n1\n",
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n1e999\n20\n2\n11\n3\n21\n4\n",
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nnot-a-number\n20\n2\n11\n3\n21\n4\n",
        ];
        for text in bad {
            assert!(dxf_entities(text).is_err(), "{text} should be rejected");
        }
    }

    #[test]
    fn bom_csv_groups_identical_parts_and_quotes_tricky_cells() -> Result<(), String> {
        let mut project = Project::default();
        for (id, reference, value) in [
            (1, "R3", "10k"),
            (2, "R1", "10k"),
            (3, "R2", "10,000"),
            (4, "C1", "100n"),
        ] {
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: value.into(),
                kind: if value == "100n" {
                    "capacitor"
                } else {
                    "resistor"
                }
                .into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![pcbcraft_doc::ComponentField {
                    key: "Tolerance".into(),
                    value: if reference == "C1" {
                        "10%".into()
                    } else {
                        "".into()
                    },
                }],
            });
        }
        project.validate()?;
        let fields = ["Tolerance".to_string()];
        let text = bom_csv(&project, &fields, None)?;
        assert_eq!(
            text.lines().next(),
            Some("quantity,references,value,kind,footprint,Tolerance"),
            "requested properties become extra columns"
        );
        assert_eq!(
            text.lines().nth(1),
            Some("1,R2,\"10,000\",resistor,starter pads,"),
            "rows sort by value, kind and footprint"
        );
        assert_eq!(
            text.lines().nth(2),
            Some("1,C1,100n,capacitor,starter pads,10%"),
            "the property column follows the requested key"
        );
        assert_eq!(
            text.lines().nth(3),
            Some("2,\"R1,R3\",10k,resistor,starter pads,"),
            "identical parts collapse into one sorted reference list"
        );
        assert_eq!(
            bom_csv(&project, &[], None)?.lines().count(),
            4,
            "no extra columns, same three groups"
        );
        assert!(
            bom_csv(&project, &["".to_string()], None).is_err(),
            "empty key"
        );
        assert!(
            bom_csv(&project, &["x".repeat(65)], None).is_err(),
            "overlong key"
        );
        assert!(
            bom_csv(
                &project,
                &(1..=17).map(|i| format!("f{i}")).collect::<Vec<String>>(),
                None,
            )
            .is_err(),
            "too many properties"
        );
        Ok(())
    }
    #[test]
    fn bom_csv_prints_project_defined_columns_under_their_labels() -> Result<(), String> {
        let mut project = Project::default();
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "R1".into(),
            value: "10k".into(),
            kind: "resistor".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(-1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: String::new(),
                    offset: Point::new(1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![
                pcbcraft_doc::ComponentField {
                    key: "MPN".into(),
                    value: "RC0603-10K".into(),
                },
                pcbcraft_doc::ComponentField {
                    key: "Rating".into(),
                    value: "0.1W".into(),
                },
            ],
        });
        project.bom_fields = vec![
            pcbcraft_doc::BomField {
                key: "MPN".into(),
                label: "Manufacturer part".into(),
            },
            pcbcraft_doc::BomField {
                key: "Rating".into(),
                label: String::new(),
            },
        ];
        project.validate()?;
        let text = bom_csv(&project, &[], None)?;
        assert!(
            text.starts_with("quantity,references,value,kind,footprint,Manufacturer part,Rating\n"),
            "recorded columns print under their labels, falling back to the key"
        );
        assert!(
            text.contains("RC0603-10K,0.1W"),
            "recorded columns read the matching component properties"
        );
        let explicit = bom_csv(&project, &["MPN".to_string()], None)?;
        assert!(
            explicit.lines().next().is_some_and(|l| l.ends_with(",MPN")),
            "an explicit request overrides the recorded columns"
        );
        project.bom_fields.push(pcbcraft_doc::BomField {
            key: "MPN".into(),
            label: String::new(),
        });
        assert!(
            bom_csv(&project, &[], None).is_err(),
            "duplicate recorded keys are refused"
        );
        Ok(())
    }
    #[test]
    fn bom_csv_follows_a_variants_skips_and_footprint_swaps() -> Result<(), String> {
        let mut project = Project::default();
        for (id, reference) in [(1, "R1"), (2, "R2")] {
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "10k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: [(-1.0, "1"), (1.0, "2")]
                    .map(|(x, number)| pcbcraft_doc::Pin {
                        number: number.into(),
                        name: String::new(),
                        offset: Point::new(x, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    })
                    .to_vec(),
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            });
        }
        project.variants.push(pcbcraft_doc::Variant {
            name: "Slim".into(),
            note: None,
            dnp: vec![2],
            footprints: vec![pcbcraft_doc::FootprintOverride {
                component: 1,
                footprint: "R_0603".into(),
            }],
        });
        project.validate()?;
        let plain = bom_csv(&project, &[], None)?;
        assert!(
            plain.contains("starter pads") && plain.contains("R1,R2") && !plain.contains("R_0603"),
            "the plain bill lists both parts with the placed footprint"
        );
        let slim = bom_csv(&project, &[], Some("Slim"))?;
        assert!(
            slim.contains("R_0603") && !slim.contains("R2"),
            "the variant bill swaps the package and skips the unpopulated part"
        );
        assert!(
            bom_csv(&project, &[], Some("Ghost")).is_err(),
            "an unknown variant name is refused"
        );
        assert!(
            bom_csv(&project, &[], Some("")).is_ok(),
            "a blank variant name prints the plain bill"
        );
        Ok(())
    }
    #[test]
    fn library_footprint_svg_shows_pads_holes_and_artwork() -> Result<(), String> {
        let definition = pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/DIP-8_W7.62mm.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let svg = footprint_svg(&definition)?;
        assert!(svg.starts_with("<?xml"), "the file is a real SVG document");
        assert!(svg.contains("<title>DIP-8_W7.62mm</title>"));
        let circles = svg.matches("<circle").count();
        assert!(
            (15..=16).contains(&circles),
            "eight pads plus eight holes, oval drills drawn as slots: {circles}"
        );
        assert!(
            svg.matches("<polygon").count() + svg.matches("<rect").count() >= 1,
            "rectangular artwork or pad outlines are drawn"
        );
        assert!(svg.contains("scale(1,-1)"), "the view is drawn y-up");
        assert!(
            svg.contains("width=\"") && svg.contains("mm\""),
            "sizes are in millimetres"
        );
        let empty = pcbcraft_library::FootprintDefinition {
            name: "BLANK".into(),
            description: String::new(),
            keywords: String::new(),
            pads: Vec::new(),
            graphics: Vec::new(),
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: Vec::new(),
            retained: Vec::new(),
        };
        let svg = footprint_svg(&empty)?;
        assert!(svg.starts_with("<?xml"), "an empty footprint still renders");
        Ok(())
    }
    #[test]
    fn library_symbol_svg_shows_pins_and_body_artwork() -> Result<(), String> {
        let symbol = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/lm358.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "LM358")
        .ok_or("fixture symbol missing")?;
        let svg = symbol_svg(&symbol)?;
        assert!(svg.starts_with("<?xml"), "the file is a real SVG document");
        assert!(svg.contains("<title>LM358</title>"));
        assert!(symbol.pins.len() >= 8, "the fixture carries a full package");
        assert!(
            svg.matches("<line").count() >= symbol.pins.len(),
            "every pin of every unit is drawn as a line"
        );
        assert!(svg.contains(">8<"), "pin numbers are labelled");
        assert!(
            svg.contains("<rect") || svg.contains("<polyline"),
            "the body is drawn"
        );
        assert_eq!(
            svg.matches("<svg").count(),
            1,
            "exactly one root element is opened"
        );
        Ok(())
    }
    #[test]
    fn read_fp_assignments_skips_comments_and_rejects_bad_rows() -> Result<(), String> {
        let rows = read_fp_assignments("# header\n\nR1 /lib/R.kicad_mod\nR2\t/lib/D.kicad_mod\n")?;
        assert_eq!(
            rows,
            vec![
                ("R1".to_string(), "/lib/R.kicad_mod".to_string()),
                ("R2".to_string(), "/lib/D.kicad_mod".to_string())
            ]
        );
        assert!(
            read_fp_assignments("# only comments\n").is_err(),
            "a file with no rows is refused"
        );
        assert!(
            read_fp_assignments("R1").is_err(),
            "a row without a footprint path is refused"
        );
        assert!(
            read_fp_assignments("R*1 /lib/R.kicad_mod").is_err(),
            "a wildcard reference is refused"
        );
        let many = "R1 /lib/R.kicad_mod\n".repeat(65);
        assert!(
            read_fp_assignments(&many).is_err(),
            "the row count stays bounded"
        );
        Ok(())
    }
    #[test]
    fn netlist_text_lists_every_net_with_sorted_component_pins() -> Result<(), String> {
        let mut project = Project::default();
        for (id, name) in [(7, "UNUSED"), (8, "SIG"), (9, "GND")] {
            project.nets.push(pcbcraft_doc::Net {
                id,
                name: name.into(),
                ..Default::default()
            });
        }
        for (id, reference, p1, p2) in [(1, "R2", 9u64, 8), (2, "R1", 9, 9)] {
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: Some(p1),
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: if reference == "R1" { None } else { Some(p2) },
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            });
        }
        project.validate()?;
        let text = netlist_text(&project)?;
        assert!(
            text.starts_with("# PCBCraft netlist\n"),
            "a parseable header"
        );
        assert!(
            text.contains("# nets: 2\n"),
            "nets without pins are not listed"
        );
        assert!(
            text.contains("GND R1.1 R2.1\n"),
            "pins sort by reference within the net"
        );
        assert!(text.contains("SIG R2.2\n"), "each net gets its own line");
        assert!(!text.contains("R1.2"), "a pin with no net is left out");
        Ok(())
    }
    #[test]
    fn gencad_text_records_components_pins_and_nets_in_mils() -> Result<(), String> {
        let project = real_library_project()?;
        let text = gencad_text(&project)?;
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            !lines.iter().any(|l| l.starts_with("%N")),
            "a board without connected nets simply omits net records"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("%C\t")),
            "components are listed"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("%P\t")),
            "pins are listed"
        );
        for line in &lines {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields[0] {
                "%C" => {
                    assert_eq!(fields.len(), 10, "component records are fixed width");
                    assert!(!fields[1].is_empty(), "the reference is named");
                    for f in &fields[3..6] {
                        assert!(f.parse::<i64>().is_ok(), "positions are whole mils");
                    }
                }
                "%P" => {
                    assert_eq!(fields.len(), 11, "pin records are fixed width");
                    let component: usize =
                        fields[1].parse().map_err(|_| "pin indexes a component")?;
                    let placed = lines.iter().filter(|l| l.starts_with("%C")).count();
                    assert!(
                        component >= 1 && component <= placed,
                        "pin component indexes name listed components"
                    );
                    assert!(
                        ["round", "rect", "oval"].contains(&fields[6]),
                        "pin shapes are limited"
                    );
                    for f in [&fields[3], &fields[4], &fields[8], &fields[9]] {
                        assert!(
                            f.parse::<i64>().is_ok(),
                            "pin numbers are whole mils or flags"
                        );
                    }
                }
                "%N" => {
                    assert!(fields.len() >= 4, "net records name at least one member");
                    let count: usize = fields[2].parse().map_err(|_| "net counts its members")?;
                    assert_eq!(
                        fields.len() - 3,
                        count * 2,
                        "the member list matches the count"
                    );
                    for f in &fields[3..] {
                        let index: usize = f.parse().map_err(|_| "members are indexes")?;
                        assert!(index >= 1, "indexes start at one");
                    }
                }
                other => return Err(format!("unexpected record {other}")),
            }
        }
        Ok(())
    }

    #[test]
    fn gencad_text_lists_net_members_by_component_and_pad_index() -> Result<(), String> {
        let mut project = Project::default();
        let geometry = |number: &str, x: f64, back: bool| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: if back { "thru_hole" } else { "smd" }.into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: if back {
                vec![
                    "F.Cu".into(),
                    "B.Cu".into(),
                    "F.Mask".into(),
                    "B.Mask".into(),
                ]
            } else {
                vec!["F.Cu".into(), "F.Mask".into()]
            },
            drill: back.then(|| pcbcraft_library::Drill {
                size: Point::new(0.5, 0.5),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        let component =
            |id: u64, reference: &str, definition: &str, back: bool| pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: Some(50),
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Test".into(),
                    definition: pcbcraft_library::FootprintDefinition {
                        name: definition.into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![geometry("1", -1.0, back), geometry("2", 1.0, back)],
                        graphics: vec![],
                        model: None,
                        rules: None,
                        raw_source: String::new(),
                        unsupported: vec![],
                        retained: vec![],
                    },
                }),
                fields: vec![],
            };
        let footprint = |id: u64, component: u64, x: f64, y: f64, rotation: f64, back: bool| {
            pcbcraft_doc::Footprint {
                id,
                component,
                position: Point::new(x, y),
                rotation,
                locked: None,
                pads: vec![
                    pcbcraft_doc::Pad {
                        number: "1".into(),
                        offset: Point::new(-1.0, 0.0),
                        diameter: 1.0,
                        net: Some(50),
                        castellated: None,
                        geometry: Some(geometry("1", -1.0, back)),
                    },
                    pcbcraft_doc::Pad {
                        number: "2".into(),
                        offset: Point::new(1.0, 0.0),
                        diameter: 1.0,
                        net: None,
                        castellated: None,
                        geometry: Some(geometry("2", 1.0, back)),
                    },
                ],
            }
        };
        project.nets.push(pcbcraft_doc::Net {
            id: 50,
            name: "PWR".into(),
            target_length: None,
            length_tolerance: None,
        });
        project.components.push(component(1, "R5", "R0603", false));
        project.components.push(component(2, "U1", "SOIC-2", true));
        project
            .board
            .footprints
            .push(footprint(10, 1, 12.0, 8.0, 0.0, false));
        project
            .board
            .footprints
            .push(footprint(11, 2, 20.0, 15.0, 0.0, true));
        project.validate()?;
        let text = gencad_text(&project)?;
        let net_line = text
            .lines()
            .find(|l| l.starts_with("%N\t"))
            .ok_or("the named net is missing")?;
        assert_eq!(
            net_line, "%N\tPWR\t2\t1\t1\t2\t1",
            "the net names both footprints' first pad by component and pad index"
        );
        let pin_lines: Vec<&str> = text.lines().filter(|l| l.starts_with("%P\t")).collect();
        assert_eq!(pin_lines.len(), 4, "one record per placed pad");
        assert!(
            pin_lines[0].starts_with("%P\t1\t1\t1\t"),
            "pin records name their component, pad number and index"
        );
        assert_eq!(
            gencad_text(&Project::default()).err(),
            Some("board has no placed footprints to export".into()),
            "an empty board is rejected"
        );
        Ok(())
    }

    #[test]
    fn plot_csv_counts_each_copper_layer_and_totals() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 5,
            name: "GND".into(),
            ..Default::default()
        });
        let geometry = pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::new(2.0, 0.0),
            rotation: 0.0,
            size: Point::new(1.7, 1.7),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.9, 0.9),
                offset: Point::new(0.0, 0.0),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        let geometry_two = pcbcraft_library::FootprintPad {
            number: "2".into(),
            position: Point::new(-2.0, 0.0),
            ..geometry.clone()
        };
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(10.0, 20.0),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(8.0, 20.0),
                    net: Some(5),
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: String::new(),
                    offset: Point::new(12.0, 20.0),
                    net: Some(5),
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Test".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "TEST-TH".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![geometry.clone(), geometry_two.clone()],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                    retained: vec![],
                },
            }),
            fields: vec![],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 11,
            component: 1,
            position: Point::new(10.0, 20.0),
            rotation: 90.0,
            locked: None,
            pads: vec![
                pcbcraft_doc::Pad {
                    number: "1".into(),
                    offset: Point::new(2.0, 0.0),
                    diameter: 1.7,
                    net: Some(5),
                    castellated: None,
                    geometry: Some(geometry),
                },
                pcbcraft_doc::Pad {
                    number: "2".into(),
                    offset: Point::new(-2.0, 0.0),
                    diameter: 1.7,
                    net: Some(5),
                    castellated: None,
                    geometry: Some(geometry_two),
                },
            ],
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 21,
            net: 5,
            start: Point::new(1.0, 2.0),
            end: Point::new(8.0, 2.0),
            width: 0.25,
            layer: pcbcraft_doc::Layer::Back,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 31,
            net: 5,
            position: Point::new(4.0, 5.0),
            diameter: 0.8,
            drill: 0.4,
        });
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 41,
            position: Point::new(0.5, 0.5),
            diameter: 3.0,
            end: None,
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 51,
            text: "TOP".into(),
            position: Point::new(9.0, 9.0),
            size: 1.0,
            layer: pcbcraft_doc::Layer::Front,
            rotation: 0.0,
            component: None,
        });
        project.validate().map_err(|e| format!("validate: {e}"))?;
        let text = plot_csv(&project)?;
        let rows: Vec<Vec<&str>> = text
            .trim_end()
            .lines()
            .map(|line| line.split(',').collect())
            .collect();
        assert_eq!(
            rows[0],
            vec![
                "layer",
                "pads",
                "tracks",
                "vias",
                "zones",
                "text",
                "fiducials",
                "holes",
                "min_x",
                "min_y",
                "max_x",
                "max_y"
            ]
        );
        // the through-hole pad, via and hole land on both copper layers;
        // the track is back-only and the text is front-only
        assert_eq!(rows[1][0], "B.Cu");
        assert_eq!(rows[2][0], "F.Cu");
        for row in &rows[1..3] {
            assert_eq!(row[1], "2", "{name}: two through-hole pads", name = row[0]);
            assert_eq!(row[3], "1", "{name}: one via", name = row[0]);
            assert_eq!(row[7], "1", "{name}: one hole", name = row[0]);
        }
        assert_eq!(rows[1][2], "1", "the back-only track is on the back row");
        assert_eq!(
            rows[2][2], "0",
            "the back-only track is not on the front row"
        );
        assert_eq!(rows[1][5], "0");
        assert_eq!(rows[2][5], "1", "the front-only text is on the front row");
        // the pad sits at (10, 22): offset (2, 0) rotated 90 degrees about (10, 20)
        assert_eq!(
            rows[2][8], "0.500",
            "the hole at (0.5, 0.5) sets the minimum"
        );
        assert_eq!(rows[2][11], "22.000", "the rotated pad sets the maximum y");
        assert_eq!(rows[3][0], "total");
        assert_eq!(rows[3][1], "4", "two pads counted on both layers each");
        assert_eq!(rows[3][2], "1");
        assert_eq!(rows[3][3], "2");
        assert_eq!(rows[3][5], "1");
        assert_eq!(rows[3][7], "2");
        let empty = plot_csv(&Project::default())?;
        assert!(
            empty
                .lines()
                .nth(1)
                .ok_or_else(|| "missing back layer row".to_string())?
                .contains(",0,0,0,0,0,0,0,"),
            "an empty board plots nothing on either layer"
        );
        Ok(())
    }

    #[test]
    fn hyperlynx_csv_lists_nets_with_comma_separated_component_pins() -> Result<(), String> {
        let mut project = Project::default();
        for (id, name) in [(7, "UNUSED"), (8, "SIG"), (9, "GND")] {
            project.nets.push(pcbcraft_doc::Net {
                id,
                name: name.into(),
                ..Default::default()
            });
        }
        for (id, reference, p1, p2) in [(1, "R2", 9u64, 8), (2, "R1", 9, 9)] {
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: Some(p1),
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: if reference == "R1" { None } else { Some(p2) },
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            });
        }
        project.validate()?;
        let text = hyperlynx_csv(&project)?;
        assert_eq!(
            text, "GND,R1.1,R2.1\nSIG,R2.2\n",
            "nets sort by name, pins by reference, comma separated"
        );
        assert!(
            hyperlynx_csv(&Project::default()).err()
                == Some("design has no connected nets to export".into()),
            "an unconnected design is rejected rather than writing an empty file"
        );
        Ok(())
    }

    #[test]
    fn vias_render_pad_and_drill_in_the_pcb_preview() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 2,
            net: 1,
            position: Point::new(30.0, 20.0),
            diameter: 1.4,
            drill: 0.8,
        });
        project.validate()?;
        let pcb = svg(&project, "pcb")?;
        assert!(
            pcb.contains("<circle cx=\"30\" cy=\"20\" r=\"0.7\""),
            "the via pad must render at half its diameter"
        );
        assert!(
            pcb.contains("<circle cx=\"30\" cy=\"20\" r=\"0.4\" fill=\"#131c29\""),
            "the drilled hole must render as board interior"
        );
        Ok(())
    }
    #[test]
    fn labels_and_no_connect_flags_render_in_the_schematic_preview() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "VCC".into(),
            ..Default::default()
        });
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 3,
            reference: "R9".into(),
            value: "1 k".into(),
            kind: "resistor".into(),
            position: Point::new(50.0, 30.0),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(-1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(180.0),
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: String::new(),
                    offset: Point::new(1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(0.0),
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
        });
        project.drawn_wires.push(pcbcraft_doc::DrawnWire {
            id: 2,
            net: Some(1),
            points: vec![Point::new(10.0, 5.0), Point::new(20.0, 5.0)],
        });
        project.labels.push(pcbcraft_doc::Label {
            id: 4,
            kind: "global".into(),
            net: 1,
            position: Point::new(10.0, 5.0),
            rotation: 0.0,
            text: String::new(),
        });
        project.labels.push(pcbcraft_doc::Label {
            id: 5,
            kind: "junction".into(),
            net: 1,
            position: Point::new(20.0, 5.0),
            rotation: 0.0,
            text: String::new(),
        });
        project.no_connects.push(pcbcraft_doc::NoConnectFlag {
            id: 6,
            pin: pcbcraft_doc::PinRef {
                component: 3,
                pin: "2".into(),
            },
        });
        project.validate()?;
        let schematic = svg(&project, "schematic")?;
        assert!(
            schematic.contains(">{VCC}</text>"),
            "a global label must render its braced net name"
        );
        assert!(
            schematic.contains("<circle cx=\"20\" cy=\"5\" r=\"0.35\""),
            "a junction must render as a dot on its wire vertex"
        );
        assert!(
            schematic.contains("stroke=\"#f58d79\""),
            "a no-connect flag must render its cross at the pin"
        );
        Ok(())
    }
    #[test]
    fn graphics_render_outlines_and_text_in_previews() -> Result<(), String> {
        let mut project = Project::default();
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 1,
            layer: "schematic".into(),
            shape: "rect".into(),
            points: vec![Point::new(10.0, 10.0), Point::new(20.0, 15.0)],
            width: 0.2,
            text: String::new(),
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 2,
            layer: "schematic".into(),
            shape: "text".into(),
            points: vec![Point::new(12.0, 12.0)],
            width: 1.5,
            text: "REV B".into(),
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 3,
            layer: "board".into(),
            shape: "circle".into(),
            points: vec![Point::new(30.0, 20.0)],
            width: 4.0,
            text: String::new(),
        });
        project.validate()?;
        let schematic = svg(&project, "schematic")?;
        assert!(
            schematic.contains("points=\"10,10 20,10 20,15 10,15 10,10\""),
            "rect graphic must render as a closed outline"
        );
        assert!(
            schematic.contains(">REV B</text>"),
            "text graphic must render its content"
        );
        assert!(
            !schematic.contains("30,20"),
            "board graphics must not leak into the schematic preview"
        );
        let pcb = svg(&project, "pcb")?;
        let circle = pcb
            .match_indices("<polyline points=\"34")
            .next()
            .map(|(i, _)| i)
            .ok_or("circle graphic missing from the pcb preview")?;
        assert!(
            pcb[circle..].contains("30,24"),
            "circle must pass through its topmost point"
        );
        Ok(())
    }
    #[test]
    fn previews_use_actual_symbol_and_non_circular_copper() -> Result<(), String> {
        let project = real_library_project()?;
        let schematic = svg(&project, "schematic")?;
        assert!(!schematic.contains("width=\"6\" height=\"3\""));
        assert!(schematic.contains("<polyline"));
        let pcb = svg(&project, "pcb")?;
        assert!(pcb.contains("#75928b"));
        assert!(pcb.contains("#e6c27d"));
        assert!(!pcb.contains("${REFERENCE}"));
        assert!(pcb.contains(">R1</text>"));
        let image = image::load_from_memory(&preview_png(&project, "pcb")?)
            .map_err(|e| e.to_string())?
            .to_rgb8();
        let scale = (880.0 / project.board.width).min(560.0 / project.board.height);
        let pad = &project.board.footprints[0].pads[0];
        let x = (60.0 + (10.0 + pad.offset.x) * scale).round() as u32;
        let y = (60.0 + (10.0 + 0.6) * scale).floor() as u32;
        // At y=0.6 mm the real 1.4-mm-tall pad still has copper; a generic
        // 1.025-mm-diameter circular replacement would miss this location.
        assert_eq!(image.get_pixel(x, y).0, [230, 194, 125]);
        assert!(!preview_png(&project, "schematic")?.is_empty());
        Ok(())
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    #[ignore = "requires installed kicad-cli"]
    fn kicad_cli_reads_rotated_real_library_export() -> Result<(), String> {
        for mut project in [real_library_project()?, real_dip_project()?] {
            project.board.footprints[0].rotation = 90.0;
            let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
            let board = directory.path().join("library-board.kicad_pcb");
            std::fs::write(&board, kicad_board(&project)?).map_err(|e| e.to_string())?;
            let output = std::process::Command::new("kicad-cli")
                .args([
                    "pcb",
                    "export",
                    "svg",
                    "--mode-single",
                    "--layers",
                    "F.Cu,F.SilkS,F.Fab,F.CrtYd",
                    "--output",
                ])
                .arg(directory.path().join("library-board.svg"))
                .arg(&board)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "KiCad rejected export: {} {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            assert!(
                std::fs::read_dir(directory.path())
                    .map_err(|e| e.to_string())?
                    .filter_map(Result::ok)
                    .any(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("svg"))
            );
            let output = std::process::Command::new("kicad-cli")
                .args(["pcb", "export", "drill", "--format", "excellon", "--output"])
                .arg(directory.path())
                .arg(&board)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "KiCad rejected drill export: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            if project.board.footprints[0].pads.iter().any(|p| {
                p.geometry
                    .as_ref()
                    .is_some_and(|g| g.pad_type == "thru_hole")
            }) {
                let mut positions = 0;
                for entry in std::fs::read_dir(directory.path()).map_err(|e| e.to_string())? {
                    let path = entry.map_err(|e| e.to_string())?.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("drl") {
                        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                        positions += text
                            .lines()
                            .filter(|line| line.starts_with('X') && line.contains('Y'))
                            .count();
                    }
                }
                assert_eq!(
                    positions, 8,
                    "DIP8 must export all eight plated drill holes"
                );
            }
        }
        Ok(())
    }
    #[test]
    fn atomic_save_replaces_existing_project_and_load_validates() -> Result<(), String> {
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = directory.path().join("design.pcbcraft");
        let mut p = Project::default();
        save(&p, &path)?;
        p.name = "Changed".into();
        save(&p, &path)?;
        assert_eq!(load(&path)?, p);
        atomic_write(&path, b"{\"format\":\"bad\"}")?;
        assert!(load(&path).is_err());
        Ok(())
    }
    #[test]
    fn document_roundtrip_and_unsupported_format() {
        let p = Project::default();
        let text = serde_json::to_string(&p);
        assert!(text.is_ok());
        if let Ok(text) = text {
            let loaded = serde_json::from_str::<Project>(&text);
            assert!(loaded.is_ok_and(|v| v == p));
        }
        let mut bad = p;
        bad.version = 999;
        assert!(bad.validate().is_err());
    }
    #[test]
    fn footprint_library_text_round_trips_through_the_reader() -> Result<(), String> {
        for source in [
            include_str!("../../library/tests/fixtures/R_0805_2012Metric.kicad_mod"),
            include_str!("../../library/tests/fixtures/DIP-8_W7.62mm.kicad_mod"),
        ] {
            let footprint = pcbcraft_library::read_footprint(source).map_err(|e| e.to_string())?;
            let text = footprint_library_text(&footprint)?;
            let again = pcbcraft_library::read_footprint(&text).map_err(|e| e.to_string())?;
            assert_eq!(again.name, footprint.name);
            assert_eq!(again.description, footprint.description);
            assert_eq!(again.keywords, footprint.keywords);
            assert_eq!(again.pads.len(), footprint.pads.len());
            for (a, b) in again.pads.iter().zip(&footprint.pads) {
                assert_eq!(a.number, b.number);
                assert_eq!(a.pad_type, b.pad_type);
                assert_eq!(a.shape, b.shape);
                assert_eq!(a.position, b.position);
                assert_eq!(a.rotation, b.rotation);
                assert_eq!(a.size, b.size);
                assert_eq!(a.layers, b.layers);
                assert_eq!(a.drill, b.drill);
                assert_eq!(a.roundrect_ratio, b.roundrect_ratio);
            }
            assert_eq!(again.graphics, footprint.graphics);
            assert_eq!(
                again
                    .model
                    .as_ref()
                    .map(|m| (&m.path, m.offset, m.scale, m.rotate)),
                footprint
                    .model
                    .as_ref()
                    .map(|m| (&m.path, m.offset, m.scale, m.rotate))
            );
            assert_eq!(again.rules, footprint.rules);
            assert_eq!(again.retained, footprint.retained);
        }
        Ok(())
    }
    #[test]
    fn library_fixtures_survive_lossless_write_round_trip() -> Result<(), String> {
        let source = "(footprint RetainTest (version 1) (generator x) (layer \"F.Cu\") (attr smd) \
            (pad 1 smd rect (at 0 0 0) (size 1 1) (layers F.Cu)) (legacy_marker yes) (embedded_fonts no))\n";
        let footprint = pcbcraft_library::read_footprint(source).map_err(|e| e.to_string())?;
        let text = footprint_library_text(&footprint)?;
        assert!(text.contains("legacy_marker"));
        let again = pcbcraft_library::read_footprint(&text).map_err(|e| e.to_string())?;
        assert_eq!(again.retained.len(), 1);
        assert_eq!(again.pads, footprint.pads);
        Ok(())
    }
    #[test]
    fn symbols_library_file_text_writes_multiple_definitions() -> Result<(), String> {
        let symbols = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/device.kicad_sym"
        ))
        .map_err(|e| e.to_string())?;
        let chosen: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "R" || s.name == "C")
            .cloned()
            .collect();
        assert_eq!(chosen.len(), 2);
        let text = symbols_library_file_text(&chosen)?;
        let again = pcbcraft_library::read_symbols(&text).map_err(|e| e.to_string())?;
        assert_eq!(again.len(), 2);
        assert!(again.iter().any(|s| s.name == "R"));
        assert!(again.iter().any(|s| s.name == "C"));
        Ok(())
    }
    #[test]
    fn symbol_library_text_round_trips_through_the_reader() -> Result<(), String> {
        for source in [
            include_str!("../../library/tests/fixtures/device.kicad_sym"),
            include_str!("../../library/tests/fixtures/lm358.kicad_sym"),
        ] {
            let symbols = pcbcraft_library::read_symbols(source).map_err(|e| e.to_string())?;
            for symbol in symbols
                .iter()
                .filter(|s| !s.pins.is_empty() && !s.graphics.is_empty())
            {
                let text = symbol_library_text(symbol)?;
                let again = pcbcraft_library::read_symbols(&text).map_err(|e| e.to_string())?;
                assert_eq!(again.len(), 1, "{} wrote stray symbols", symbol.name);
                let again = &again[0];
                assert_eq!(again.name, symbol.name);
                assert_eq!(again.reference, symbol.reference);
                assert_eq!(again.value, symbol.value);
                assert_eq!(again.description, symbol.description);
                assert_eq!(again.keywords, symbol.keywords);
                assert_eq!(again.footprint, symbol.footprint);
                assert_eq!(again.pins, symbol.pins);
                assert_eq!(again.graphics, symbol.graphics);
            }
        }
        Ok(())
    }

    #[test]
    fn symbol_library_text_round_trips_graphical_style_and_hide() -> Result<(), String> {
        let symbol = pcbcraft_library::SymbolDefinition {
            name: "DECO".into(),
            reference: "U".into(),
            value: "DECO".into(),
            description: String::new(),
            keywords: String::new(),
            footprint: String::new(),
            set: String::new(),
            pins: vec![pcbcraft_library::SymbolPin {
                number: "1".into(),
                name: "CLK".into(),
                position: Point::new(5.0, 0.0),
                angle: 180.0,
                length: 2.54,
                electrical_type: "input".into(),
                unit: 1,
                body_style: 1,
                graphical_style: "inverted_clock".into(),
                visible: false,
            }],
            graphics: vec![pcbcraft_library::UnitGraphic {
                unit: 1,
                body_style: 1,
                primitive: pcbcraft_library::Graphic::Rectangle {
                    start: Point::new(-2.0, -2.0),
                    end: Point::new(2.0, 2.0),
                    width: 0.12,
                    fill: "none".into(),
                    layer: "symbol".into(),
                },
            }],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
            retained: vec![],
        };
        symbol.validate().map_err(|e| e.to_string())?;
        let text = symbol_library_text(&symbol)?;
        assert!(text.contains("inverted_clock"));
        assert!(text.contains("(hide yes)"));
        let again = pcbcraft_library::read_symbols(&text).map_err(|e| e.to_string())?;
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].pins, symbol.pins);
        Ok(())
    }

    #[test]
    fn symbol_svg_inverted_clock_pin_has_circle_at_connection() -> Result<(), String> {
        let symbol = pcbcraft_library::SymbolDefinition {
            name: "DECO".into(),
            reference: "U".into(),
            value: "DECO".into(),
            description: String::new(),
            keywords: String::new(),
            footprint: String::new(),
            set: String::new(),
            pins: vec![pcbcraft_library::SymbolPin {
                number: "1".into(),
                name: "CLK".into(),
                position: Point::new(0.0, 0.0),
                angle: 0.0,
                length: 2.54,
                electrical_type: "input".into(),
                unit: 1,
                body_style: 1,
                graphical_style: "inverted_clock".into(),
                visible: true,
            }],
            graphics: vec![],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
            retained: vec![],
        };
        let svg = symbol_svg(&symbol)?;
        assert!(
            svg.contains("<circle"),
            "inverted clock bubble renders as a circle"
        );
        Ok(())
    }

    #[test]
    fn schematic_preview_png_shows_clock_wedge_pixels() -> Result<(), String> {
        let definition = pcbcraft_library::SymbolPin {
            number: "1".into(),
            name: "CLK".into(),
            position: Point::new(-5.0, 0.0),
            angle: 0.0,
            length: 2.54,
            electrical_type: "input".into(),
            unit: 1,
            body_style: 1,
            graphical_style: "clock".into(),
            visible: true,
        };
        let symbol = pcbcraft_library::SymbolDefinition {
            name: "CLKPART".into(),
            reference: "U".into(),
            value: "CLKPART".into(),
            description: String::new(),
            keywords: String::new(),
            footprint: String::new(),
            set: String::new(),
            pins: vec![definition.clone()],
            graphics: vec![pcbcraft_library::UnitGraphic {
                unit: 1,
                body_style: 1,
                primitive: pcbcraft_library::Graphic::Rectangle {
                    start: Point::new(-3.0, -3.0),
                    end: Point::new(0.0, 3.0),
                    width: 0.12,
                    fill: "none".into(),
                    layer: "symbol".into(),
                },
            }],
            extends: None,
            raw_source: String::new(),
            unsupported: vec![],
            retained: vec![],
        };
        symbol.validate().map_err(|e| e.to_string())?;
        let mut project = Project::default();
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "U1".into(),
            value: "CLKPART".into(),
            kind: "library".into(),
            position: Point::new(40.0, 30.0),
            rotation: 0.0,
            pins: vec![pcbcraft_doc::Pin {
                number: definition.number.clone(),
                name: definition.name.clone(),
                offset: definition.position,
                net: None,
                electrical_type: definition.electrical_type.clone(),
                direction: Some(definition.angle),
                unit: definition.unit,
            }],
            units: vec![],
            library_symbol: Some(pcbcraft_doc::LibrarySymbol {
                library: "project".into(),
                unit: 1,
                definition: symbol,
            }),
            library_footprint: None,
            fields: vec![],
        });
        project.validate()?;
        let png = preview_png(&project, "schematic")?;
        let image = image::load_from_memory(&png)
            .map_err(|e| e.to_string())?
            .to_rgb8();
        let anchor = project.components[0].position.offset(definition.position);
        let (min, max) = preview_bounds(&project, "schematic");
        let scale =
            (880.0 / (max.x - min.x + 8.0).max(1.0)).min(560.0 / (max.y - min.y + 8.0).max(1.0));
        let px = (60.0 + (anchor.x - (min.x - 4.0)) * scale).round() as u32;
        let py = (60.0 + (anchor.y - (min.y - 4.0)) * scale).round() as u32;
        let pixel = image.get_pixel(px.min(999), py.min(679));
        assert!(
            pixel.0[0] > 40 || pixel.0[1] > 40,
            "connection area should carry pin ink, got {:?}",
            pixel.0
        );
        Ok(())
    }

    #[test]
    fn svg_entities_reads_shape_kinds_with_a_y_flip_and_unit_conversion() -> Result<(), String> {
        let drawing = "<svg width=\"2in\" height=\"5cm\">\
            <line x1=\"10mm\" y1=\"10mm\" x2=\"1pt\" y2=\"0mm\" stroke=\"#ff0000\"/>\
            <!-- a comment between shapes -->\
            <polyline points=\"1,2 3,4 5,6\"/>\
            <polygon points=\"1,2 3,4 5,6 7,8\" stroke=\"EDGE\"/>\
            <rect x=\"0mm\" y=\"40mm\" width=\"3mm\" height=\"2mm\"/>\
            <circle cx=\"5mm\" cy=\"45mm\" r=\"2mm\"/>\
            <text x=\"5mm\" y=\"45mm\" font-size=\"2mm\">A &amp; B</text>\
            <g><line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/></g>\
            <ellipse cx=\"1\" cy=\"1\" rx=\"1\" ry=\"1\"/>\
            </svg>";
        let out = svg_entities(drawing)?;
        assert_eq!(
            out.len(),
            6,
            "grouped shapes and unknown shapes are skipped"
        );
        assert_eq!(out[0].layer, "#ff0000");
        match &out[0].shape {
            SvgShape::Line { start, end } => {
                assert!((start.x - 10.0).abs() < 1e-9, "mm suffix");
                assert!((start.y - 40.0).abs() < 1e-9, "50mm page flips y up");
                assert!((end.x - 25.4 / 72.0).abs() < 1e-9, "pt scale");
                assert!((end.y - 50.0).abs() < 1e-9);
            }
            other => Err(format!("expected a line, got {other:?}"))?,
        }
        match &out[1].shape {
            SvgShape::Polyline { points, closed } => {
                assert_eq!(points.len(), 3);
                assert!(!closed, "polylines stay open");
            }
            other => Err(format!("expected a polyline, got {other:?}"))?,
        }
        match &out[2].shape {
            SvgShape::Polyline { points, closed } => {
                assert_eq!(points.len(), 4);
                assert!(closed, "polygons close");
                assert_eq!(out[2].layer, "EDGE");
            }
            other => Err(format!("expected a polygon, got {other:?}"))?,
        }
        match &out[3].shape {
            SvgShape::Polyline { points, closed } => {
                assert!(closed);
                assert_eq!(points.len(), 4);
                assert!((points[0].y - 10.0).abs() < 1e-9, "y 40mm flips to 10mm");
                assert!((points[2].x - 3.0).abs() < 1e-9);
            }
            other => Err(format!("expected a rect outline, got {other:?}"))?,
        }
        match &out[4].shape {
            SvgShape::Circle { center, radius } => {
                assert!((center.y - 5.0).abs() < 1e-9);
                assert!((radius - 2.0).abs() < 1e-9);
            }
            other => Err(format!("expected a circle, got {other:?}"))?,
        }
        match &out[5].shape {
            SvgShape::Text { at, height, value } => {
                assert_eq!(value, "A & B", "the five predefined entities decode");
                assert!((height - 2.0).abs() < 1e-9);
                assert!((at.y - 5.0).abs() < 1e-9);
            }
            other => Err(format!("expected text, got {other:?}"))?,
        }
        Ok(())
    }

    #[test]
    fn sheet_file_reads_components_and_ports_with_defaults() -> Result<(), String> {
        let file = sheet_file(
            r#"{"name":"power","components":[
                {"kind":"resistor","reference":"R1","position":{"x":10,"y":20},
                 "fields":[{"key":"Datasheet","value":"x"}]},
                {"kind":"led","position":{"x":-5,"y":2.5},"rotation":90,"value":""}],
             "ports":[{"name":"VIN","position":{"x":0,"y":0},"net":"5V"},
                      {"name":"GND","position":{"x":1,"y":1}}]}"#,
        )?;
        assert_eq!(file.name.as_deref(), Some("power"));
        assert_eq!(file.components.len(), 2);
        assert_eq!(file.components[0].reference.as_deref(), Some("R1"));
        assert_eq!(
            file.components[0].fields,
            vec![("Datasheet".to_string(), "x".to_string())]
        );
        assert_eq!(file.components[1].rotation, 90.0);
        assert!(file.components[1].reference.is_none());
        assert_eq!(file.components[1].value.as_deref(), Some(""));
        assert_eq!(file.ports.len(), 2);
        assert_eq!(file.ports[0].net.as_deref(), Some("5V"));
        assert!(file.ports[1].net.is_none());
        Ok(())
    }

    #[test]
    fn sheet_file_refuses_malformed_pages() {
        let many = vec![r#"{"kind":"led","position":{"x":0,"y":0}}"#; 257].join(",");
        for text in [
            "not json",
            "[]",
            r#"{"components":[]}"#,
            r#"{"extra":1,"components":[{"kind":"led","position":{"x":0,"y":0}}]}"#,
            r#"{"components":[{"kind":"led","position":{"x":0,"y":0},"rot":0}]}"#,
            r#"{"components":[{"position":{"x":0,"y":0}}]}"#,
            r#"{"components":[{"kind":"led","position":{"x":0,"y":0},"rotation":"x"}]}"#,
            r#"{"components":[{"kind":"led","position":{"x":0,"y":0},"z":1}]}"#,
            r#"{"components":[{"kind":"led","position":{"x":0,"y":0},"fields":[
                {"key":"a","value":"x"},{"key":"a"}]}]}"#,
            r#"{"ports":[{"name":"VIN-1","position":{"x":0,"y":0}}]}"#,
            r#"{"ports":[{"name":"A","position":{"x":1e12,"y":0}}]}"#,
            r#"{"ports":[{"name":"A","position":{"x":0,"y":0},"extra":2}]}"#,
            &format!(r#"{{"components":[{many}]}}"#),
        ] {
            assert!(sheet_file(text).is_err(), "{text} should fail");
        }
    }

    #[test]
    fn svg_entities_runs_paths_and_falls_back_to_the_view_box_height() -> Result<(), String> {
        let drawing =
            "<svg viewBox=\"0 0 10 30\"><path d=\"M1 2h3l-3-4v-2zM 4 4 5,5 6,6M 8 8 9 9\"/></svg>";
        let out = svg_entities(drawing)?;
        assert_eq!(out.len(), 3, "each subpath becomes one polyline");
        match &out[0].shape {
            SvgShape::Polyline { points, closed } => {
                assert!(closed, "Z closes the subpath");
                assert_eq!(points.len(), 5);
                assert!(
                    (points[1].x - 4.0).abs() < 1e-9,
                    "h is relative and sign-flush"
                );
                assert!((points[2].x - 1.0).abs() < 1e-9 && (points[2].y - 32.0).abs() < 1e-9);
                assert!((points[3].y - 34.0).abs() < 1e-9, "v is relative");
                assert!(
                    (points[0].x - points[4].x).abs() < 1e-9
                        && (points[0].y - points[4].y).abs() < 1e-9,
                    "Z returns to the subpath start"
                );
            }
            other => Err(format!("expected a closed subpath, got {other:?}"))?,
        }
        match &out[1].shape {
            SvgShape::Polyline { points, closed } => {
                assert!(!closed);
                assert_eq!(points.len(), 3, "M keeps collecting implicit Ls");
                assert!((points[0].x - 4.0).abs() < 1e-9 && (points[2].x - 6.0).abs() < 1e-9);
            }
            other => Err(format!("expected an open subpath, got {other:?}"))?,
        }
        match &out[2].shape {
            SvgShape::Polyline { points, closed } => {
                assert_eq!(points.len(), 2, "the third M starts a new subpath");
                assert!(!closed);
                assert!(
                    (points[0].x - 8.0).abs() < 1e-9 && (points[0].y - 22.0).abs() < 1e-9,
                    "viewBox height 30 flips y"
                );
            }
            other => Err(format!("expected the last subpath, got {other:?}"))?,
        }
        Ok(())
    }

    #[test]
    fn svg_entities_rejects_malformed_drawings() {
        let bad = [
            "no markup at all",
            "<svg width=\"1mm\" height=\"0mm\"><line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/></svg>",
            "<svg width=\"1mm\" height=\"1pc\"><line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/></svg>",
            "<svg><path d=\"M0 0 C1 1 2 2 3 3\"/></svg>",
            "<svg><path d=\"M0 0 Q1 1 2 2\"/></svg>",
            "<svg><polyline points=\"1,2 3\"/></svg>",
            "<svg><polyline points=\"1,2 x,4\"/></svg>",
            "<svg><polygon points=\"1,2 3,4 5,6 7,8 9,10 11,12 13,14 15,16 17,18 19,20 21,22 23,24 25,26 27,28 29,30 31,32 33,34 35,36 37,38 39,40 41,42 43,44 45,46 47,48 49,50 51,52 53,54 55,56 57,58 59,60 61,62 63,64 65,66 67,68 69,70 71,72 73,74 75,76 77,78 79,80 81,82 83,84 85,86 87,88 89,90 91,92 93,94 95,96 97,98 99,100 101,102 103,104 105,106 107,108 109,110 111,112 113,114 115,116 117,118 119,120 121,122 123,124 125,126 127,128 129,130 131,132 133,134 135,136 137,138 139,140 141,142 143,144 145,146 147,148 149,150 151,152 153,154 155,156 157,158 159,160 161,162 163,164 165,166 167,168 169,170 171,172 173,174 175,176 177,178 179,180 181,182 183,184 185,186 187,188 189,190 191,192 193,194 195,196 197,198 199,200 201,202 203,204 205,206 207,208 209,210 211,212 213,214 215,216 217,218 219,220 221,222 223,224 225,226 227,228 229,230 231,232 233,234 235,236 237,238 239,240 241,242 243,244 245,246 247,248 249,250 251,252 253,254 255,256 257,258 259,260 261,262 263,264 265,266 267,268 269,270 271,272 273,274 275,276 277,278 279,280 281,282 283,284 285,286 287,288 289,290 291,292 293,294 295,296 297,298 299,300 301,302 303,304 305,306 307,308 309,310 311,312 313,314 315,316 317,318 319,320 321,322 323,324 325,326 327,328 329,330 331,332 333,334 335,336 337,338 339,340 341,342 343,344 345,346 347,348 349,350 351,352 353,354 355,356 357,358 359,360 361,362 363,364 365,366 367,368 369,370 371,372 373,374 375,376 377,378 379,380 381,382 383,384 385,386 387,388 389,390 391,392 393,394 395,396 397,398 399,400 401,402 403,404 405,406 407,408 409,410 411,412 413,414 415,416 417,418 419,420 421,422 423,424 425,426 427,428 429,430 431,432 433,434 435,436 437,438 439,440 441,442 443,444 445,446 447,448 449,450 451,452 453,454 455,456 457,458 459,460 461,462 463,464 465,466 467,468 469,470 471,472 473,474 475,476 477,478 479,480 481,482 483,484 485,486 487,488 489,490 491,492 493,494 495,496 497,498 499,500 501,502 503,504 505,506 507,508 509,510 511,512 513,514\"/></svg>",
            "<svg><circle cx=\"1\" cy=\"1\" r=\"0\"/></svg>",
            "<svg><circle cx=\"1\" cy=\"1\" r=\"1001mm\"/></svg>",
            "<svg><rect x=\"0\" y=\"0\" width=\"0\" height=\"2\"/></svg>",
            "<svg><text x=\"1\" y=\"1\">abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz</text></svg>",
            "<svg><line x1=\"0\" y1=\"0\" x2=\"1\"/></svg>",
            "<svg><line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"",
            "<svg><!-- oops <line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/></svg>",
            "<svg><line x1=\"1e999\" y1=\"0\" x2=\"1\" y2=\"1\"/></svg>",
        ];
        for text in bad {
            let e = svg_entities(text);
            assert!(e.is_err(), "{text} should be rejected, got {e:?}");
        }
        let many = format!(
            "<svg>{}</svg>",
            "<line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/>".repeat(513)
        );
        assert!(svg_entities(&many).is_err(), "the entity cap is 512");
        let fits = format!(
            "<svg>{}</svg>",
            "<line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/>".repeat(512)
        );
        assert_eq!(svg_entities(&fits).map(|v| v.len()), Ok(512));
    }

    /// The 0805 fixture with pad 1 chamfered on its outer corners and pad 2 a
    /// concave custom polygon, plus a front track for the copper writer.
    fn shaped_pad_project() -> Result<Project, String> {
        let mut project = real_library_project()?;
        let c = &mut project.components[0];
        let source = c.library_footprint.as_mut().ok_or("no footprint")?;
        let first = source.definition.pads.get_mut(0).ok_or("no pad 1")?;
        first.shape = "chamfered".into();
        first.roundrect_ratio = None;
        first.chamfer = Some(pcbcraft_library::PadChamfer {
            ratio: 0.25,
            corners: pcbcraft_library::ChamferCorners {
                top_left: true,
                bottom_left: true,
                ..Default::default()
            },
        });
        let second = source.definition.pads.get_mut(1).ok_or("no pad 2")?;
        second.shape = "custom".into();
        second.roundrect_ratio = None;
        second.size = Point::new(0.5, 0.5);
        second.polygon = vec![
            Point::new(-0.5, -0.7),
            Point::new(0.5, -0.7),
            Point::new(0.5, 0.7),
            Point::new(0.0, 0.4),
            Point::new(-0.5, 0.7),
        ];
        let pads = source.definition.pads.clone();
        let placed = &mut project.board.footprints[0];
        for (pad, geometry) in placed.pads.iter_mut().zip(pads) {
            pad.diameter = geometry.size.x.min(geometry.size.y);
            pad.geometry = Some(geometry);
        }
        project.nets.push(pcbcraft_doc::Net {
            id: 7,
            name: "probe".into(),
            target_length: None,
            length_tolerance: None,
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 8,
            net: 7,
            start: Point::new(5.0, 30.0),
            end: Point::new(15.0, 30.0),
            width: 0.25,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.validate()?;
        Ok(project)
    }
    #[test]
    fn parameters_survive_save_and_open() -> Result<(), String> {
        let mut project = Project::default();
        project.parameters.insert(
            "hole_pitch".into(),
            pcbcraft_doc::Parameter {
                value: 3.2,
                note: "mounting holes".into(),
            },
        );
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 5,
            position: Point::new(3.2, 8.0),
            diameter: 1.0,
            end: None,
        });
        project.bindings.push(pcbcraft_doc::Binding {
            object: 5,
            field: pcbcraft_doc::Field::HoleX,
            parameter: "hole_pitch".into(),
        });
        project.validate()?;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("parameters.pcbcraft");
        save(&project, &path)?;
        let loaded = load(&path)?;
        assert_eq!(loaded, project);
        assert_eq!(loaded.parameters["hole_pitch"].value, 3.2);
        assert_eq!(loaded.parameters["hole_pitch"].note, "mounting holes");
        assert_eq!(loaded.bindings[0].field, pcbcraft_doc::Field::HoleX);
        Ok(())
    }
    #[test]
    fn projects_without_parameters_still_load() -> Result<(), String> {
        let project = Project::default();
        let text = serde_json::to_string(&project).map_err(|e| e.to_string())?;
        assert!(!text.contains("\"parameters\""));
        assert!(!text.contains("\"bindings\""));
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("plain.pcbcraft");
        std::fs::write(&path, &text).map_err(|e| e.to_string())?;
        let loaded = load(&path)?;
        assert!(loaded.parameters.is_empty());
        assert!(loaded.bindings.is_empty());
        assert_eq!(loaded, project);
        Ok(())
    }
    #[test]
    fn chamfered_and_custom_pads_survive_project_save_and_load() -> Result<(), String> {
        let project = shaped_pad_project()?;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("shaped.pcbcraft");
        save(&project, &path)?;
        let loaded = load(&path)?;
        assert_eq!(loaded, project);
        let pads = &loaded.board.footprints[0].pads;
        assert!(pads.iter().any(|p| {
            p.geometry
                .as_ref()
                .is_some_and(|g| g.chamfer.is_some_and(|c| c.ratio == 0.25))
        }));
        assert!(
            pads.iter()
                .any(|p| p.geometry.as_ref().is_some_and(|g| g.polygon.len() == 5))
        );
        Ok(())
    }
    #[test]
    fn chamfered_and_custom_pads_survive_kicad_export_and_reimport() -> Result<(), String> {
        let project = shaped_pad_project()?;
        let board = kicad_board(&project)?;
        assert!(board.contains("(chamfer_ratio 0.25) (chamfer top_left bottom_left)"));
        assert!(board.contains("(primitives (gr_poly (pts (xy -0.5 -0.7)"));
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        // A footprint embedded in a board also carries placement expressions
        // (uuid, at); only pad-level refusals matter here.
        assert!(
            exported.unsupported.iter().all(|u| !u.contains("pad")),
            "{:?}",
            exported.unsupported
        );
        let source = &project.components[0]
            .library_footprint
            .as_ref()
            .ok_or("no footprint")?
            .definition;
        for (back, original) in exported.pads.iter().zip(&source.pads) {
            assert_eq!(back.shape, original.shape);
            assert_eq!(back.chamfer, original.chamfer);
            assert_eq!(back.polygon, original.polygon);
            assert_eq!(back.size, original.size);
            assert_eq!(back.roundrect_ratio, original.roundrect_ratio);
        }
        Ok(())
    }
    /// Reads back the aperture macros and flashes of a writer-generated file and
    /// returns each flashed outline in board coordinates (y-down).
    fn gerber_outlines(text: &str, height: f64) -> Result<Vec<Vec<Point>>, String> {
        let mut macros: BTreeMap<String, Vec<Point>> = BTreeMap::new();
        let mut apertures: BTreeMap<String, Vec<Point>> = BTreeMap::new();
        let mut current: Option<String> = None;
        let mut outlines = vec![];
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            if let Some(name) = line.strip_prefix("%AM").and_then(|l| l.strip_suffix('*')) {
                let primitive = lines.next().ok_or("macro without primitive")?;
                let numbers = primitive
                    .trim_end_matches("*%")
                    .split(',')
                    .map(|n| n.parse::<f64>().map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                let (head, rest) = numbers.split_at(3);
                assert_eq!(head[..2], [4.0, 1.0], "outline primitive, exposure on");
                assert_eq!(rest.last(), Some(&0.0), "no macro rotation");
                let vertices: Vec<Point> = rest[..rest.len() - 1]
                    .chunks(2)
                    .map(|xy| Point::new(xy[0], xy[1]))
                    .collect();
                assert_eq!(vertices.len() as f64, head[2] + 1.0);
                assert_eq!(vertices.first(), vertices.last(), "outline is closed");
                macros.insert(name.to_owned(), vertices);
            } else if let Some(def) = line.strip_prefix("%ADD").and_then(|l| l.strip_suffix("*%")) {
                let split = def.find(|c: char| !c.is_ascii_digit()).ok_or("bad AD")?;
                let (code, template) = def.split_at(split);
                if let Some(vertices) = macros.get(template) {
                    apertures.insert(format!("D{code}"), vertices.clone());
                }
            } else if let Some(code) = line.strip_suffix('*').filter(|l| l.starts_with('D')) {
                current = Some(code.to_owned());
            } else if let Some(flash) = line.strip_suffix("D03*") {
                let (x, y) = flash
                    .strip_prefix('X')
                    .and_then(|l| l.split_once('Y'))
                    .ok_or("bad flash")?;
                let at = Point::new(
                    x.parse::<f64>().map_err(|e| e.to_string())? / 1e6,
                    y.parse::<f64>().map_err(|e| e.to_string())? / 1e6,
                );
                if let Some(vertices) = current.as_ref().and_then(|c| apertures.get(c)) {
                    outlines.push(
                        vertices
                            .iter()
                            .map(|v| Point::new(at.x + v.x, height - (at.y + v.y)))
                            .collect(),
                    );
                }
            }
        }
        Ok(outlines)
    }
    #[test]
    fn gerber_copper_flashes_chamfered_and_custom_pads_through_outline_macros() -> Result<(), String>
    {
        let mut project = shaped_pad_project()?;
        project.board.footprints[0].rotation = 30.0;
        let text = gerber_copper(&project, pcbcraft_doc::Layer::Front)?;
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/shaped_pads_F_Cu.gbr");
        if std::env::var_os("PCBCRAFT_BLESS").is_some() {
            std::fs::write(&fixture, &text).map_err(|e| e.to_string())?;
        }
        let expected = std::fs::read_to_string(&fixture).map_err(|e| e.to_string())?;
        assert_eq!(
            text, expected,
            "Gerber output drifted from the reviewed fixture"
        );
        assert!(text.contains("%FSLAX46Y46*%") && text.contains("%MOMM*%"));
        assert!(text.ends_with("M02*\n"));
        assert_eq!(
            text.matches("%AMPAD").count(),
            2,
            "one macro per shaped pad"
        );
        assert!(text.contains("%ADD10C,0.25*%"), "track aperture");
        // Oracle: macro vertices plus flash points must reproduce the outlines
        // the document gives for the rotated footprint, chamfer cuts included.
        let outlines = gerber_outlines(&text, project.board.height)?;
        let f = &project.board.footprints[0];
        assert_eq!(outlines.len(), 2);
        for (pad, outline) in f.pads.iter().zip(&outlines) {
            let expected: Vec<Point> = pad
                .outline_points()
                .into_iter()
                .map(|v| f.position.offset(v.rotate(-f.rotation)))
                .collect();
            assert_eq!(expected.len(), outline.len());
            for (a, b) in expected.iter().zip(outline) {
                assert!(a.distance(*b) < 2e-6, "{a:?} vs {b:?}");
            }
        }
        assert_eq!(
            outlines[0].len(),
            7,
            "rectangle with two corners cut, closed"
        );
        assert_eq!(outlines[1].len(), 6, "five-vertex custom polygon, closed");
        // The back layer of an SMD-only board has the back track only (none here).
        let back = gerber_copper(&project, pcbcraft_doc::Layer::Back)?;
        assert!(!back.contains("D03*") && !back.contains("%AM"));
        assert!(back.contains("%TF.FileFunction,Copper,L2,Bot*%"));
        Ok(())
    }

    fn sample_board_with_two_by_two_panel() -> Result<pcbcraft_doc::Project, String> {
        let json = include_str!("../tests/fixtures/sample_led_panel_base.json");
        let mut p: pcbcraft_doc::Project = serde_json::from_str(json).map_err(|e| e.to_string())?;
        p.validate()?;
        p.panel.enabled = true;
        p.panel.columns = 2;
        p.panel.rows = 2;
        p.panel.pitch_x = 60.0;
        p.panel.pitch_y = 40.0;
        p.panel.rail = 3.0;
        Ok(p)
    }

    #[test]
    fn panel_gerber_carries_part_array_attribute() -> Result<(), String> {
        let p = sample_board_with_two_by_two_panel()?;
        let files = panel_fabrication_files(&p)?;
        for name in ["pcb-F.Cu.gbr", "pcb-B.Cu.gbr"] {
            let copper = files.get(name).ok_or("missing copper layer")?;
            assert!(
                copper.contains("%TF.Part,Array*%"),
                "{name} should declare an array panel"
            );
        }
        Ok(())
    }

    fn peg_footprint_project() -> Result<Project, String> {
        let peg = |x: f64, y: f64| pcbcraft_library::FootprintPad {
            number: String::new(),
            pad_type: "unplated".into(),
            shape: "circle".into(),
            position: Point::new(x, y),
            rotation: 0.0,
            size: Point::default(),
            layers: vec![],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(3.0, 3.0),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        };
        let thru = |number: &str, x: f64| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        };
        let definition = pcbcraft_library::FootprintDefinition {
            name: "PegConn".into(),
            description: String::new(),
            keywords: String::new(),
            pads: vec![
                thru("1", -2.54),
                thru("2", 2.54),
                peg(-5.0, 5.0),
                peg(5.0, 5.0),
            ],
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
            retained: vec![],
        };
        pcbcraft_doc::validate_footprint_support(&definition)?;
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "SIG".into(),
            ..Default::default()
        });
        let mut component =
            pcbcraft_doc::component_template("resistor", 2, "R1".into(), Point::default())
                .ok_or("template")?;
        component.pins[0].net = Some(1);
        component.pins[1].net = Some(1);
        component.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
            library: "test".into(),
            definition: definition.clone(),
        });
        project.components.push(component);
        let placed: Vec<pcbcraft_doc::Pad> = definition
            .pads
            .iter()
            .map(|g| pcbcraft_doc::Pad {
                number: g.number.clone(),
                offset: g.position,
                diameter: g.size.x.min(g.size.y),
                net: project.components[0]
                    .pins
                    .iter()
                    .find(|p| p.number == g.number)
                    .and_then(|p| p.net),
                castellated: None,
                geometry: Some(g.clone()),
            })
            .collect();
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 3,
            component: 2,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads: placed,
        });
        project.validate()?;
        Ok(project)
    }

    #[test]
    fn gerber_files_footprint_unplated_pegs_export_only_to_npth_drill() -> Result<(), String> {
        let project = peg_footprint_project()?;
        let files = gerber_files(&project)?;
        let plated = &files["pcb-Plated.drl"];
        let npth = &files["pcb-NPTH.drl"];
        assert!(
            plated.contains("T1C0.800\n"),
            "numbered through-hole pads stay on the plated drill file"
        );
        assert!(
            !plated.contains("X5.000Y15.000\n") && !plated.contains("X15.000Y15.000\n"),
            "peg coordinates must not appear in the plated drill file: {plated}"
        );
        assert!(
            npth.contains("T1C3.000\n"),
            "both pegs share one unplated tool"
        );
        assert!(
            npth.contains("X5.000Y15.000\n") && npth.contains("X15.000Y15.000\n"),
            "peg hits land in the NPTH drill file: {npth}"
        );
        let front = &files["pcb-F.Cu.gbr"];
        assert!(
            !front.contains("X5.000000Y15.000000D03"),
            "unplated pegs must not flash copper"
        );
        Ok(())
    }

    /// A two layer board fixture for the drawn layer plots: a cut outline with
    /// a hole and a label, and a silk panel mark.
    fn drawn_layer_project() -> Project {
        let mut p = Project::default();
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 1,
            name: "Card Outline".into(),
            kind: pcbcraft_doc::ArtKind::Cut,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![
                pcbcraft_doc::ArtShape::Rect {
                    x: 10.0,
                    y: 10.0,
                    width: 40.0,
                    height: 30.0,
                    rotate: 0.0,
                },
                pcbcraft_doc::ArtShape::Circle {
                    center: Point::new(30.0, 25.0),
                    radius: 3.0,
                },
            ],
        });
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 2,
            name: "Panel Silk".into(),
            kind: pcbcraft_doc::ArtKind::Silk,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![pcbcraft_doc::ArtShape::Line {
                start: Point::new(12.0, 12.0),
                end: Point::new(48.0, 12.0),
            }],
        });
        p
    }

    #[test]
    fn drawn_layers_write_one_file_per_layer() -> Result<(), String> {
        let p = drawn_layer_project();
        let plots = drawn_layer_plots(&p)?;
        let names: Vec<&str> = plots.files.keys().map(|k| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "card-outline.dxf",
                "card-outline.hpg",
                "card-outline.svg",
                "panel-silk.dxf",
                "panel-silk.gbr"
            ],
            "a cut layer gets a mill path and a silk layer gets a print file"
        );
        assert!(
            plots.skipped.is_empty(),
            "nothing in the fixture needs skipping"
        );
        let gerbers = gerber_files(&p)?;
        assert!(
            gerbers.contains_key("panel-silk.gbr"),
            "the silk print joins the Gerber set"
        );
        let drawing = dxf_board(&p)?;
        assert!(
            drawing.contains("8\ncard-outline\n"),
            "the board DXF carries the drawn cut layer"
        );
        assert!(
            drawing.contains("8\npanel-silk\n"),
            "the board DXF carries the drawn silk layer"
        );
        Ok(())
    }

    #[test]
    fn gerber_files_plated_and_unplated_drill_files_list_only_their_tools() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 901,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 902,
            net: 901,
            position: Point::new(5.0, 5.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 903,
            position: Point::new(15.0, 5.0),
            diameter: 3.0,
            end: None,
        });
        project.validate()?;
        let files = gerber_files(&project)?;
        let plated = &files["pcb-Plated.drl"];
        let npth = &files["pcb-NPTH.drl"];
        assert!(plated.contains("T1C0.300\n") && !plated.contains("T1C3.000\n"));
        assert!(npth.contains("T1C3.000\n") && !npth.contains("0.300"));
        Ok(())
    }

    #[test]
    fn drill_map_plot_carries_one_legend_row_per_tool() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 911,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 912,
            net: 911,
            position: Point::new(5.0, 5.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.holes.push(pcbcraft_doc::Hole {
            id: 913,
            position: Point::new(15.0, 5.0),
            diameter: 3.0,
            end: None,
        });
        project.validate()?;
        let map = drill_map_plot(&project)?;
        assert!(map.contains("G04 LEGEND T1,"));
        assert!(map.contains("G04 LEGEND T2,"));
        Ok(())
    }

    #[test]
    fn panel_two_by_two_sample_board_drill_and_placement() -> Result<(), String> {
        let p = sample_board_with_two_by_two_panel()?;
        let single_drill = gerber_files(&p)?
            .get("pcb-Drill.drl")
            .ok_or("single-board drill missing")?
            .clone();
        let single_hits = single_drill
            .lines()
            .filter(|line| line.starts_with('X'))
            .count();
        let files = panel_fabrication_files(&p)?;
        let drill = files
            .get("pcb-Drill.drl")
            .ok_or("panel drill file missing")?;
        assert!(
            drill.contains("MechanicalDrill,Breakout"),
            "breakaway holes must carry the breakout aperture function"
        );
        assert!(
            drill.contains("MechanicalDrill,Tooling"),
            "tooling holes must carry the tooling aperture function"
        );
        let panel_hits = drill.lines().filter(|line| line.starts_with('X')).count();
        assert!(
            panel_hits > single_hits * 4,
            "panel drill must repeat every board hit on each copy plus mechanical holes"
        );
        let placement = panel_placement_text(&p)?;
        for reference in ["J1_1", "R1_2", "D1_3", "J1_4"] {
            assert!(
                placement.contains(reference),
                "placement must suffix {reference}"
            );
        }
        assert!(
            !placement.contains("FID"),
            "board fiducials must not be duplicated into panel placement"
        );
        Ok(())
    }

    #[test]
    fn drawn_layer_dxf_reads_back_with_the_same_figures() -> Result<(), String> {
        let p = drawn_layer_project();
        let plots = drawn_layer_plots(&p)?;
        let text = plots
            .files
            .get("card-outline.dxf")
            .ok_or("the cut layer writes a DXF")?;
        assert!(
            text.contains("$INSUNITS\n70\n4\n"),
            "a mill plot declares millimetres"
        );
        let entities = dxf_entities(text)?;
        assert_eq!(entities.len(), 2, "an outline and a hole");
        assert!(
            entities.iter().all(|e| e.layer == "card-outline"),
            "every figure keeps the layer name"
        );
        let outline = entities
            .iter()
            .find_map(|e| match &e.shape {
                DxfShape::Polyline { points, closed } if *closed => Some(points.clone()),
                _ => None,
            })
            .ok_or("the rectangle reads back as a closed polyline")?;
        assert_eq!(outline.len(), 4, "four corners");
        for (got, want) in outline.iter().zip([
            Point::new(10.0, 10.0),
            Point::new(50.0, 10.0),
            Point::new(50.0, 40.0),
            Point::new(10.0, 40.0),
        ]) {
            assert!(
                (got.x - want.x).abs() < 0.001 && (got.y - want.y).abs() < 0.001,
                "corner {got:?} should be {want:?}"
            );
        }
        let hole = entities
            .iter()
            .find_map(|e| match &e.shape {
                DxfShape::Circle { center, radius } => Some((*center, *radius)),
                _ => None,
            })
            .ok_or("the hole stays a true circle")?;
        assert!(
            (hole.0.x - 30.0).abs() < 0.001
                && (hole.0.y - 25.0).abs() < 0.001
                && (hole.1 - 3.0).abs() < 0.001,
            "the hole keeps its centre and radius"
        );
        Ok(())
    }

    #[test]
    fn drawn_layer_svg_reads_back_with_the_same_figures() -> Result<(), String> {
        let p = drawn_layer_project();
        let plots = drawn_layer_plots(&p)?;
        let text = plots
            .files
            .get("card-outline.svg")
            .ok_or("the cut layer writes an SVG")?;
        assert!(
            text.contains("width=\"51.000mm\"") && text.contains("height=\"41.000mm\""),
            "a laser plot is sized in millimetres"
        );
        assert!(
            text.contains("stroke=\"#0000ff\""),
            "cut lines stay blue, the usual laser rule"
        );
        let entities = svg_entities(text)?;
        assert_eq!(entities.len(), 2, "an outline and a hole");
        let outline = entities
            .iter()
            .find_map(|e| match &e.shape {
                SvgShape::Polyline { points, closed } if *closed => Some(points.clone()),
                _ => None,
            })
            .ok_or("the rectangle reads back as a closed polygon")?;
        assert_eq!(outline.len(), 4, "four corners");
        for (got, want) in outline.iter().zip([
            Point::new(10.0, 10.0),
            Point::new(50.0, 10.0),
            Point::new(50.0, 40.0),
            Point::new(10.0, 40.0),
        ]) {
            assert!(
                (got.x - want.x).abs() < 0.001 && (got.y - want.y).abs() < 0.001,
                "corner {got:?} should be {want:?}"
            );
        }
        let hole = entities
            .iter()
            .find_map(|e| match &e.shape {
                SvgShape::Circle { center, radius } => Some((*center, *radius)),
                _ => None,
            })
            .ok_or("the hole stays a true circle")?;
        assert!(
            (hole.0.x - 30.0).abs() < 0.001
                && (hole.0.y - 25.0).abs() < 0.001
                && (hole.1 - 3.0).abs() < 0.001,
            "the hole keeps its centre and radius, got {hole:?} in {text}"
        );
        Ok(())
    }

    #[test]
    fn drawn_layer_file_names_cannot_leave_the_folder() -> Result<(), String> {
        let mut p = Project::default();
        for (id, name) in [
            (1, "../../etc/passwd"),
            (2, "Card Outline"),
            (3, "card/outline"),
        ] {
            p.board.user_layers.push(pcbcraft_doc::ArtLayer {
                id,
                name: name.into(),
                kind: pcbcraft_doc::ArtKind::Other,
                dotted: false,
                dot_pitch: 1.0,
                dot_size: 0.5,
                visible: true,
                shapes: vec![pcbcraft_doc::ArtShape::Line {
                    start: Point::new(1.0, 1.0),
                    end: Point::new(2.0, 2.0),
                }],
            });
        }
        let plots = drawn_layer_plots(&p)?;
        for name in plots.files.keys() {
            assert!(
                !name.contains('/') && !name.contains('\\') && !name.contains(".."),
                "{name} must stay inside the output folder"
            );
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'),
                "{name} holds only safe characters"
            );
        }
        assert!(
            plots.files.contains_key("etc-passwd.dxf"),
            "a path-like name reduces to a plain stem"
        );
        assert!(
            plots.files.contains_key("card-outline.dxf")
                && plots.files.contains_key("card-outline-3.dxf"),
            "two names that reduce alike are told apart by the layer id"
        );
        let mut blank = Project::default();
        blank.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 9,
            name: "///".into(),
            kind: pcbcraft_doc::ArtKind::Other,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![pcbcraft_doc::ArtShape::Line {
                start: Point::new(1.0, 1.0),
                end: Point::new(2.0, 2.0),
            }],
        });
        let blank = drawn_layer_plots(&blank)?;
        assert!(
            blank.files.contains_key("layer-9.dxf"),
            "a name with no usable characters falls back to the layer id"
        );
        Ok(())
    }

    #[test]
    fn drill_chart_csv_lists_footprint_pegs_as_unplated() -> Result<(), String> {
        let project = peg_footprint_project()?;
        let text = drill_chart_csv(&project)?;
        assert!(
            text.contains("T1,0.800,0.000,yes,2"),
            "numbered pads are plated hits"
        );
        assert!(
            text.contains("T2,3.000,0.000,no,2"),
            "locating pegs are unplated hits"
        );
        Ok(())
    }

    #[test]
    fn mask_art_is_written_as_positive_other_not_soldermask() -> Result<(), String> {
        let mut p = Project::default();
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 4,
            name: "Ink Mask".into(),
            kind: pcbcraft_doc::ArtKind::Mask,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![pcbcraft_doc::ArtShape::Line {
                start: Point::new(5.0, 5.0),
                end: Point::new(15.0, 5.0),
            }],
        });
        let plots = drawn_layer_plots(&p)?;
        let text = plots
            .files
            .get("ink-mask.gbr")
            .ok_or("a mask layer writes a Gerber")?;
        assert!(
            text.contains("%TF.FileFunction,Other,Ink_Mask*%"),
            "mask art is an other layer, not a negative soldermask image"
        );
        assert!(
            text.contains("%TF.FilePolarity,Positive*%"),
            "drawn mask figures are ink, so the polarity is positive"
        );
        assert!(
            !text.contains("Soldermask"),
            "a positive ink layer must never be labelled as a soldermask"
        );
        assert!(
            text.contains("G04 solder mask drawn layer \"Ink Mask\", top side"),
            "the header names the layer and its side"
        );
        Ok(())
    }

    #[test]
    fn perforation_plot_uses_physical_pitch() -> Result<(), String> {
        let mut p = Project::default();
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 5,
            name: "Snap Line".into(),
            kind: pcbcraft_doc::ArtKind::Perforation,
            dotted: true,
            dot_pitch: 2.0,
            dot_size: 0.4,
            visible: true,
            shapes: vec![pcbcraft_doc::ArtShape::Line {
                start: Point::new(0.0, 0.0),
                end: Point::new(10.0, 0.0),
            }],
        });
        let plots = drawn_layer_plots(&p)?;
        let dxf = plots
            .files
            .get("snap-line.dxf")
            .ok_or("a perforation writes a mill path")?;
        let entities = dxf_entities(dxf)?;
        assert_eq!(
            entities.len(),
            6,
            "a ten millimetre line at a two millimetre pitch is six holes"
        );
        for (i, entity) in entities.iter().enumerate() {
            let DxfShape::Circle { center, radius } = &entity.shape else {
                return Err("each perforation mark is a drilled circle".into());
            };
            assert!(
                (center.x - i as f64 * 2.0).abs() < 0.001,
                "hole {i} sits at {} millimetres",
                i as f64 * 2.0
            );
            assert!(
                (radius - 0.2).abs() < 0.001,
                "the dot size is a real diameter, not screen pixels"
            );
        }
        let pen = plots
            .files
            .get("snap-line.hpg")
            .ok_or("a perforation writes a pen path")?;
        assert!(
            pen.matches("PD").count() == 6,
            "the pen path marks the same six holes"
        );
        assert!(
            pen.contains("PU0,0;PD0,0;"),
            "the first mark sits at the origin, forty units per millimetre"
        );
        Ok(())
    }

    #[test]
    fn text_on_a_cut_layer_is_reported_when_skipped() -> Result<(), String> {
        let mut p = Project::default();
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 6,
            name: "Labels".into(),
            kind: pcbcraft_doc::ArtKind::Cut,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![
                pcbcraft_doc::ArtShape::Line {
                    start: Point::new(1.0, 1.0),
                    end: Point::new(5.0, 1.0),
                },
                pcbcraft_doc::ArtShape::Text {
                    at: Point::new(2.0, 3.0),
                    text: "A1".into(),
                    size: 1.5,
                    rotate: 0.0,
                },
            ],
        });
        let plots = drawn_layer_plots(&p)?;
        assert_eq!(
            plots.skipped,
            ["text \"A1\" on layer Labels"],
            "a machine path cannot follow a filled glyph, so the job says so"
        );
        let note = plots.skipped_note().ok_or("a skipped figure is reported")?;
        assert!(
            note.contains("1 figure(s) skipped") && note.contains("\"A1\""),
            "the note names the figure and the layer"
        );
        let dxf = plots
            .files
            .get("labels.dxf")
            .ok_or("the layer still plots")?;
        assert!(
            dxf.contains("0\nTEXT\n8\nlabels\n"),
            "DXF keeps text as text so printer software can convert it"
        );
        let pen = plots.files.get("labels.hpg").ok_or("pen path")?;
        assert!(!pen.contains("A1"), "the pen path leaves the label out");
        Ok(())
    }

    #[test]
    fn mask_and_paste_layers_export_for_smd_pads_with_rules_margins() -> Result<(), String> {
        let smd = pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 0.5),
            layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        let mut project = Project::default();
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "C1".into(),
            value: "cap".into(),
            kind: "capacitor".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Test".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "C0402".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![smd.clone()],
                    graphics: vec![],
                    model: None,
                    rules: Some(pcbcraft_library::FootprintRules {
                        clearance: None,
                        mask_margin: Some(0.1),
                        paste_margin: Some(-0.02),
                    }),
                    raw_source: String::new(),
                    unsupported: vec![],
                    retained: vec![],
                },
            }),
            fields: vec![],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 2,
            component: 1,
            position: Point::new(12.0, 8.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(0.0, 0.0),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: Some(smd),
            }],
        });
        project.validate()?;
        let files = gerber_files(&project)?;
        assert!(files["pcb-F.Mask.gbr"].contains("%TF.FileFunction,Soldermask,Top*%"));
        assert!(files["pcb-F.Paste.gbr"].contains("%TF.FileFunction,Paste,Top*%"));
        assert!(files["pcb-F.Paste.gbr"].contains("D03*"));
        Ok(())
    }

    #[test]
    fn examples_validate_after_load() -> Result<(), String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|e| e == "pcbcraft") {
                load(&path)?;
            }
        }
        Ok(())
    }

    #[test]
    fn fab_readme_lists_every_output_file_and_stackup_summary() -> Result<(), String> {
        let mut project = Project::default();
        project.board.layer_records.push(pcbcraft_doc::LayerRecord {
            name: "L1".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: None,
        });
        project
            .meta
            .insert("contact".into(), "fab@example.test".into());
        project.validate()?;
        let files = gerber_files(&project)?;
        let readme = &files["README-fabrication.txt"];
        for name in files.keys() {
            assert!(readme.contains(name.as_str()), "readme lists {name}");
        }
        assert!(readme.contains("L1"));
        assert!(readme.contains("fab@example.test"));
        assert!(readme.contains("Via protection: open"));
        Ok(())
    }
}
