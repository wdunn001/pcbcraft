//! Drawing frame layout and rendering for PDF, SVG and plot output.

use pcbcraft_doc::{DrawingFrame, Id, Project, sheet_page_numbers, PAGE_SIZES};
use pcbcraft_geom::Point;

use crate::{DrawPage, escape};

/// Layout of one enabled drawing frame in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingFrameLayout {
    pub width_mm: f64,
    pub height_mm: f64,
    pub band_mm: f64,
    pub inner_lo: Point,
    pub inner_hi: Point,
    pub title_lo: Point,
    pub title_hi: Point,
    pub cols: usize,
    pub rows: usize,
    pub size_name: String,
}

pub fn uses_drawing_frame(p: &Project, view: &str) -> bool {
    p.drawing_frame.enabled
        && matches!(
            view,
            "schematic" | "assembly-top" | "assembly-bottom" | "fabrication"
        )
}

fn title_block_width(sheet_width: f64) -> f64 {
    (sheet_width * 0.32).clamp(36.0, 110.0)
}

fn content_fits(w: f64, h: f64, band: f64, tb_h: f64, cw: f64, ch: f64) -> bool {
    cw <= w - 2.0 * band && ch <= h - 2.0 * band - tb_h
}

/// Pick page dimensions and the inner drawable rectangle for schematic plots.
pub fn drawing_frame_layout(
    p: &Project,
    sheet: Id,
    content_min: Point,
    content_max: Point,
) -> DrawingFrameLayout {
    let frame = &p.drawing_frame;
    let cw = (content_max.x - content_min.x).max(1.0);
    let ch = (content_max.y - content_min.y).max(1.0);
    let settings = p.page_settings_of(sheet);
    let paper = Some(frame.page_size.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            Some(settings.size.clone()).filter(|s| !s.is_empty())
        })
        .or_else(|| {
            p.meta
                .get("paper")
                .map(|s| s.trim().to_ascii_uppercase())
                .filter(|s| !s.is_empty())
        });
    let picked = paper
        .as_deref()
        .and_then(|name| PAGE_SIZES.iter().find(|s| s.0.eq_ignore_ascii_case(name)))
        .or_else(|| {
            PAGE_SIZES.iter().find(|s| {
                let band = (s.1 * 0.02).clamp(3.0, 8.0);
                let tb_h = title_block_width(s.1) * 0.25;
                content_fits(s.1, s.2, band, tb_h, cw, ch)
            })
        })
        .copied()
        .unwrap_or(PAGE_SIZES[PAGE_SIZES.len() - 1]);
    let (size_name, w, h) = (picked.0.to_string(), picked.1, picked.2);
    let band = (w * 0.02).clamp(3.0, 8.0);
    let tb_w = title_block_width(w);
    let tb_h = tb_w * 0.25;
    let inner_lo = Point::new(band, band);
    let inner_hi = Point::new(w - band, h - band - tb_h);
    let title_lo = Point::new(w - band - tb_w, h - band - tb_h);
    let title_hi = Point::new(w - band, h - band);
    DrawingFrameLayout {
        width_mm: w,
        height_mm: h,
        band_mm: band,
        inner_lo,
        inner_hi,
        title_lo,
        title_hi,
        cols: ((w / 50.0).round() as usize).clamp(4, 24),
        rows: ((h / 50.0).round() as usize).clamp(3, 16),
        size_name,
    }
}

/// Map schematic content coordinates into the frame's inner drawable area.
pub fn content_plot_transform(
    layout: &DrawingFrameLayout,
    content_min: Point,
    content_max: Point,
) -> (Point, f64) {
    let cw = (content_max.x - content_min.x).max(0.001);
    let ch = (content_max.y - content_min.y).max(0.001);
    let iw = (layout.inner_hi.x - layout.inner_lo.x).max(0.001);
    let ih = (layout.inner_hi.y - layout.inner_lo.y).max(0.001);
    let scale = (iw / cw).min(ih / ch);
    let origin = Point::new(
        layout.inner_lo.x - content_min.x * scale,
        layout.inner_lo.y - content_min.y * scale,
    );
    (origin, scale)
}

fn meta_or_settings(p: &Project, sheet: Id, key: &str) -> String {
    if let Some(v) = p.meta.get(key).filter(|v| !v.trim().is_empty()) {
        return v.clone();
    }
    let settings = p.page_settings_of(sheet);
    match key {
        "title" => settings.title,
        "revision" => settings.revision,
        "date" => settings.date,
        "company" => settings.company,
        "drawn_by" => settings.drawn_by,
        _ => String::new(),
    }
}

/// Resolve `${name}` tokens in a title-block template.
pub fn resolve_meta_template(p: &Project, sheet: Id, template: &str) -> String {
    let (page, pages) = sheet_page_numbers(p, sheet);
    let title = meta_or_settings(p, sheet, "title");
    let title = if title.trim().is_empty() {
        p.name.clone()
    } else {
        title
    };
    let design = if p.name.trim().is_empty() {
        "untitled board".to_string()
    } else {
        p.name.trim().to_string()
    };
    let mut out = template.to_string();
    let pairs: [(&str, String); 9] = [
        ("title", title),
        ("revision", meta_or_settings(p, sheet, "revision")),
        ("company", meta_or_settings(p, sheet, "company")),
        ("date", meta_or_settings(p, sheet, "date")),
        ("drawn_by", meta_or_settings(p, sheet, "drawn_by")),
        ("name", design),
        ("page", page.to_string()),
        ("pages", pages.to_string()),
        (
            "size",
            if p.drawing_frame.page_size.is_empty() {
                p.page_settings_of(sheet).size.clone()
            } else {
                p.drawing_frame.page_size.clone()
            },
        ),
    ];
    for (key, value) in pairs {
        out = out.replace(&format!("${{{key}}}"), &value);
    }
    out
}

fn draw_title_block_fields(
    page: &mut impl DrawPage,
    p: &Project,
    sheet: Id,
    frame: &DrawingFrame,
    layout: &DrawingFrameLayout,
    ink: [f64; 3],
) -> Result<(), String> {
    let rect = [
        layout.title_lo,
        Point::new(layout.title_hi.x, layout.title_lo.y),
        layout.title_hi,
        Point::new(layout.title_lo.x, layout.title_hi.y),
    ];
    page.path(&rect, ink, 0.15, false)?;
    let tb_h = layout.title_hi.y - layout.title_lo.y;
    let split = layout.title_lo.y + tb_h * 0.44;
    page.path(
        &[
            Point::new(layout.title_lo.x, split),
            Point::new(layout.title_hi.x, split),
        ],
        ink,
        0.12,
        false,
    )?;
    let mut y = layout.title_hi.y - 2.5;
    for field in &frame.fields {
        let value = resolve_meta_template(p, sheet, &field.template);
        if value.is_empty() && field.label.is_empty() {
            continue;
        }
        let line = if field.label.is_empty() {
            value
        } else {
            format!("{}: {}", field.label, value)
        };
        page.text(Point::new(layout.title_lo.x + 1.5, y), 1.8, ink, &line)?;
        y -= 4.0;
        if y < layout.title_lo.y + 1.0 {
            break;
        }
    }
    if !frame.logo_text.is_empty() {
        page.text(
            Point::new(layout.title_hi.x - 1.5, layout.title_lo.y + 1.5),
            1.6,
            ink,
            &frame.logo_text,
        )?;
    }
    Ok(())
}

/// Border, optional zone grid and title block for one sheet.
pub fn draw_drawing_frame(
    page: &mut impl DrawPage,
    p: &Project,
    sheet: Id,
    layout: &DrawingFrameLayout,
) -> Result<(), String> {
    let frame = &p.drawing_frame;
    let ink = [0.05, 0.05, 0.05];
    let outer = [
        Point::new(0.0, 0.0),
        Point::new(layout.width_mm, 0.0),
        Point::new(layout.width_mm, layout.height_mm),
        Point::new(0.0, layout.height_mm),
    ];
    page.path(&outer, ink, 0.2, false)?;
    let inner = [
        layout.inner_lo,
        Point::new(layout.inner_hi.x, layout.inner_lo.y),
        layout.inner_hi,
        Point::new(layout.inner_lo.x, layout.inner_hi.y),
    ];
    page.path(&inner, ink, 0.15, false)?;
    if frame.show_zones {
        let zone_ink = [0.35, 0.35, 0.35];
        for i in 1..layout.cols {
            let x = layout.inner_lo.x
                + (layout.inner_hi.x - layout.inner_lo.x) * i as f64 / layout.cols as f64;
            page.path(
                &[
                    Point::new(x, 0.0),
                    Point::new(x, layout.inner_lo.y),
                ],
                zone_ink,
                0.1,
                false,
            )?;
            page.path(
                &[
                    Point::new(x, layout.inner_hi.y),
                    Point::new(x, layout.height_mm),
                ],
                zone_ink,
                0.1,
                false,
            )?;
            let label = (i + 1).to_string();
            page.text(
                Point::new(x - 1.0, layout.band_mm * 0.35),
                1.4,
                zone_ink,
                &label,
            )?;
            page.text(
                Point::new(
                    x - 1.0,
                    layout.height_mm - layout.band_mm * 0.65,
                ),
                1.4,
                zone_ink,
                &label,
            )?;
        }
        for j in 1..layout.rows {
            let y = layout.inner_lo.y
                + (layout.inner_hi.y - layout.inner_lo.y) * j as f64 / layout.rows as f64;
            page.path(
                &[
                    Point::new(0.0, y),
                    Point::new(layout.inner_lo.x, y),
                ],
                zone_ink,
                0.1,
                false,
            )?;
            page.path(
                &[
                    Point::new(layout.inner_hi.x, y),
                    Point::new(layout.width_mm, y),
                ],
                zone_ink,
                0.1,
                false,
            )?;
            let letter = char::from_u32(65 + j as u32)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into());
            page.text(
                Point::new(layout.band_mm * 0.35, y - 0.5),
                1.4,
                zone_ink,
                &letter,
            )?;
            page.text(
                Point::new(layout.width_mm - layout.band_mm * 0.9, y - 0.5),
                1.4,
                zone_ink,
                &letter,
            )?;
        }
    }
    let settings = p.page_settings_of(sheet);
    if !settings.title_hidden {
        draw_title_block_fields(page, p, sheet, frame, layout, ink)?;
    }
    Ok(())
}

pub struct MappedDrawPage<'a, P: DrawPage + ?Sized> {
    pub inner: &'a mut P,
    pub origin: Point,
    pub scale: f64,
}

impl<P: DrawPage + ?Sized> DrawPage for MappedDrawPage<'_, P> {
    fn path(
        &mut self,
        points: &[Point],
        rgb: [f64; 3],
        w: f64,
        fill: bool,
    ) -> Result<(), String> {
        let mapped: Vec<Point> = points
            .iter()
            .map(|pt| {
                Point::new(
                    self.origin.x + pt.x * self.scale,
                    self.origin.y + pt.y * self.scale,
                )
            })
            .collect();
        self.inner.path(&mapped, rgb, w * self.scale, fill)
    }

    fn circle(
        &mut self,
        center: Point,
        radius: f64,
        rgb: [f64; 3],
        w: f64,
        fill: bool,
    ) -> Result<(), String> {
        let at = Point::new(
            self.origin.x + center.x * self.scale,
            self.origin.y + center.y * self.scale,
        );
        self.inner.circle(at, radius * self.scale, rgb, w * self.scale, fill)
    }

    fn text(&mut self, at: Point, size: f64, rgb: [f64; 3], value: &str) -> Result<(), String> {
        let mapped = Point::new(
            self.origin.x + at.x * self.scale,
            self.origin.y + at.y * self.scale,
        );
        self.inner.text(mapped, size * self.scale, rgb, value)
    }
}

/// Draw the frame and scaled view content onto any plot surface.
pub fn render_framed_view(
    page: &mut impl DrawPage,
    p: &Project,
    view: &str,
    layout: &DrawingFrameLayout,
    content_min: Point,
    content_max: Point,
) -> Result<(), String> {
    draw_drawing_frame(page, p, 0, layout)?;
    let (origin, scale) = content_plot_transform(layout, content_min, content_max);
    let mut mapped = MappedDrawPage {
        inner: page,
        origin,
        scale,
    };
    super::pdf_draw(&mut mapped, p, view)
}

pub fn drawing_frame_svg_fragment(
    p: &Project,
    sheet: Id,
    layout: &DrawingFrameLayout,
) -> String {
    let frame = &p.drawing_frame;
    let mut s = String::new();
    s.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#fbfaf3\" stroke=\"#2c4a3e\" stroke-width=\"0.2\"/>",
        layout.width_mm, layout.height_mm
    ));
    s.push_str(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#2c4a3e\" stroke-width=\"0.15\"/>",
        layout.inner_lo.x,
        layout.inner_lo.y,
        layout.inner_hi.x - layout.inner_lo.x,
        layout.inner_hi.y - layout.inner_lo.y
    ));
    if frame.show_zones {
        for i in 1..layout.cols {
            let x = layout.inner_lo.x
                + (layout.inner_hi.x - layout.inner_lo.x) * i as f64 / layout.cols as f64;
            s.push_str(&format!(
                "<line x1=\"{x}\" y1=\"0\" x2=\"{x}\" y2=\"{}\" stroke=\"#9db3a8\" stroke-width=\"0.08\"/>",
                layout.inner_lo.y
            ));
            s.push_str(&format!(
                "<line x1=\"{x}\" y1=\"{}\" x2=\"{x}\" y2=\"{}\" stroke=\"#9db3a8\" stroke-width=\"0.08\"/>",
                layout.inner_hi.y, layout.height_mm
            ));
        }
    }
    if !p.page_settings_of(sheet).title_hidden {
        s.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#fbfaf3\" stroke=\"#2c4a3e\" stroke-width=\"0.12\"/>",
            layout.title_lo.x,
            layout.title_lo.y,
            layout.title_hi.x - layout.title_lo.x,
            layout.title_hi.y - layout.title_lo.y
        ));
        let mut y = layout.title_hi.y - 2.0;
        for field in &frame.fields {
            let value = resolve_meta_template(p, sheet, &field.template);
            let line = if field.label.is_empty() {
                value
            } else {
                format!("{}: {}", field.label, value)
            };
            if line.is_empty() {
                continue;
            }
            s.push_str(&format!(
                "<text x=\"{}\" y=\"{y}\" font-size=\"1.6\" fill=\"#101c17\">{}</text>",
                layout.title_lo.x + 1.5,
                escape(&line)
            ));
            y -= 4.0;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolve_meta_template_substitutes_page_and_meta_keys() -> Result<(), String> {
        let mut p = Project::default();
        p.meta.insert("revision".into(), "B".into());
        p.meta.insert("title".into(), "Widget".into());
        p.drawing_frame.enabled = true;
        p.drawing_frame.page_size = "A4".into();
        let text = resolve_meta_template(&p, 0, "Rev ${revision} · ${title} p${page}/${pages}");
        assert!(text.contains('B'), "{text}");
        assert!(text.contains("Widget"), "{text}");
        assert!(text.contains("p1/1"), "{text}");
        Ok(())
    }

    #[test]
    fn title_block_revision_b_appears_in_schematic_svg_and_pdf() -> Result<(), String> {
        use crate::{pdf_document, svg};
        let mut p = Project::default();
        p.meta.insert("revision".into(), "B".into());
        p.drawing_frame.enabled = true;
        p.drawing_frame.page_size = "A4".into();
        let schematic = svg(&p, "schematic")?;
        assert!(
            schematic.contains("Revision: B"),
            "schematic SVG title block: {schematic}"
        );
        let pdf = pdf_document(&p, "schematic")?;
        let text = String::from_utf8_lossy(&pdf);
        assert!(
            text.contains("Revision: B"),
            "schematic PDF title block missing revision B"
        );
        Ok(())
    }

    #[test]
    fn drawing_frame_a3_svg_outer_size_is_420_by_297_mm() -> Result<(), String> {
        let mut p = Project::default();
        p.drawing_frame.enabled = true;
        p.drawing_frame.page_size = "A3".into();
        p.drawing_frame.show_zones = false;
        let layout = drawing_frame_layout(&p, 0, Point::new(0.0, 0.0), Point::new(40.0, 30.0));
        assert!((layout.width_mm - 420.0).abs() < 1e-6);
        assert!((layout.height_mm - 297.0).abs() < 1e-6);
        let frag = drawing_frame_svg_fragment(&p, 0, &layout);
        assert!(frag.contains("width=\"420\"") && frag.contains("height=\"297\""));
        Ok(())
    }
}
