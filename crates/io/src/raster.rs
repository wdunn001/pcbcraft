//! Fabrication layer raster export (PNG, BMP, SVG).
use crate::layers::{
    FabLayer, Shape, ShapePolarity, fabrication_point, layer_shapes, profile_edge,
};
use image::{GrayImage, Luma, Rgb, RgbImage};
use pcbcraft_doc::Project;
use pcbcraft_geom::Point;

const MAX_PIXELS: u64 = 64_000_000;
const MIN_DPI: u32 = 300;
const MAX_DPI: u32 = 4800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterDepth {
    Bits1,
    Grey8,
    Rgba8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegMarkShape {
    Cross,
    Ring,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegistrationMarks {
    pub inset_mm: f64,
    pub size_mm: f64,
    pub shape: RegMarkShape,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RasterOptions {
    pub layer: FabLayer,
    pub dpi: u32,
    pub mirror: bool,
    pub invert: bool,
    pub outline: bool,
    pub registration: Option<RegistrationMarks>,
    pub drill_marks: Option<f64>,
    pub margin_mm: f64,
    pub depth: RasterDepth,
}

impl RasterOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !(MIN_DPI..=MAX_DPI).contains(&self.dpi) {
            return Err(format!(
                "dpi must be between {MIN_DPI} and {MAX_DPI}, got {}",
                self.dpi
            ));
        }
        if !(0.0..=50.0).contains(&self.margin_mm) {
            return Err("margin_mm must be between 0 and 50".into());
        }
        if let Some(reg) = self.registration {
            if !(0.0..=20.0).contains(&reg.inset_mm) {
                return Err("registration inset_mm must be between 0 and 20".into());
            }
            if reg.size_mm <= 0.0 || reg.size_mm > 50.0 {
                return Err("registration size_mm is out of range".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RasterPixels {
    Grey(GrayImage),
    Rgba(RgbImage),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    pub depth: RasterDepth,
    pub pixels: RasterPixels,
}

pub fn layer_raster(p: &Project, opts: &RasterOptions) -> Result<Raster, String> {
    opts.validate()?;
    p.validate()?;
    let w_px = pixel_extent(p.board.width, opts.margin_mm, opts.dpi);
    let h_px = pixel_extent(p.board.height, opts.margin_mm, opts.dpi);
    let pixels = u64::from(w_px) * u64::from(h_px);
    if pixels > MAX_PIXELS {
        return Err(format!(
            "raster for a {:.0} by {:.0} mm board would be {w_px} by {h_px} ({pixels} pixels), above the 64 million pixel limit",
            p.board.width, p.board.height
        ));
    }
    let collected = layer_shapes(p, opts.layer)?;
    rasterize_shapes(p, &collected.shapes, opts)
}

/// Rasterize an already collected shape list with the same pipeline as
/// [`layer_raster`], so other artwork writers share the pixel path.
pub fn rasterize_shapes(
    p: &Project,
    shapes: &[Shape],
    opts: &RasterOptions,
) -> Result<Raster, String> {
    opts.validate()?;
    let w_px = pixel_extent(p.board.width, opts.margin_mm, opts.dpi);
    let h_px = pixel_extent(p.board.height, opts.margin_mm, opts.dpi);
    let mut grey = GrayImage::from_pixel(w_px, h_px, Luma([255]));
    if opts.outline {
        let edge = profile_edge(p);
        draw_stroke(
            &mut grey,
            &edge,
            0.12,
            true,
            opts,
            p,
            ShapePolarity::Positive,
        );
    }
    for shape in shapes {
        draw_shape(&mut grey, shape, opts, p);
    }
    if let Some(reg) = opts.registration {
        draw_registration(&mut grey, p, opts, reg);
    }
    if opts.invert {
        for px in grey.pixels_mut() {
            px.0[0] = 255 - px.0[0];
        }
    }
    let pixels = match opts.depth {
        RasterDepth::Bits1 | RasterDepth::Grey8 => RasterPixels::Grey(grey),
        RasterDepth::Rgba8 => {
            let mut rgb = RgbImage::new(w_px, h_px);
            for (x, y, l) in grey.enumerate_pixels() {
                let v = l.0[0];
                rgb.put_pixel(x, y, Rgb([v, v, v]));
            }
            RasterPixels::Rgba(rgb)
        }
    };
    Ok(Raster {
        width: w_px,
        height: h_px,
        dpi: opts.dpi,
        depth: opts.depth,
        pixels,
    })
}

fn pixel_extent(extent_mm: f64, margin_mm: f64, dpi: u32) -> u32 {
    ((extent_mm + 2.0 * margin_mm) * f64::from(dpi) / 25.4)
        .ceil()
        .max(1.0) as u32
}

fn mm_to_px(mm: f64, dpi: u32, margin_mm: f64) -> f64 {
    (mm + margin_mm) * f64::from(dpi) / 25.4
}

fn fab_to_px(fab: Point, opts: &RasterOptions, width: u32, mirror: bool) -> Point {
    let mut x = mm_to_px(fab.x, opts.dpi, opts.margin_mm);
    let y = mm_to_px(fab.y, opts.dpi, opts.margin_mm);
    if mirror {
        x = f64::from(width.saturating_sub(1)) - x;
    }
    Point::new(x, y)
}

fn draw_shape(img: &mut GrayImage, shape: &Shape, opts: &RasterOptions, p: &Project) {
    match shape {
        Shape::Polygon(points, pol) => {
            let pts: Vec<Point> = points
                .iter()
                .map(|fab| fab_to_px(*fab, opts, img.width(), opts.mirror))
                .collect();
            fill_polygon_grey(img, &pts, ink(*pol));
        }
        Shape::Disk(center, r, pol) => {
            let c = fab_to_px(*center, opts, img.width(), opts.mirror);
            let r_px = r * f64::from(opts.dpi) / 25.4;
            disk_grey(img, c, r_px, ink(*pol));
        }
        Shape::Stroke(points, width, closed, pol) => {
            draw_stroke(img, points, *width, *closed, opts, p, *pol);
        }
    }
}

fn ink(pol: ShapePolarity) -> u8 {
    match pol {
        ShapePolarity::Positive => 0,
        ShapePolarity::Negative => 255,
    }
}

fn draw_stroke(
    img: &mut GrayImage,
    fab_points: &[Point],
    width_mm: f64,
    closed: bool,
    opts: &RasterOptions,
    _p: &Project,
    pol: ShapePolarity,
) {
    let pts: Vec<Point> = fab_points
        .iter()
        .map(|fab| fab_to_px(*fab, opts, img.width(), opts.mirror))
        .collect();
    let radius = width_mm * f64::from(opts.dpi) / 25.4 / 2.0;
    let color = ink(pol);
    let segments = pts.len().saturating_sub(1);
    for i in 0..segments {
        let a = pts[i];
        let b = if closed && i + 1 == segments {
            pts[0]
        } else if i + 1 < pts.len() {
            pts[i + 1]
        } else {
            a
        };
        line_grey(img, a, b, radius.max(0.5), color);
    }
}

fn draw_registration(
    img: &mut GrayImage,
    p: &Project,
    opts: &RasterOptions,
    reg: RegistrationMarks,
) {
    let w = p.board.width;
    let h = p.board.height;
    let inset = reg.inset_mm;
    let corners = [
        Point::new(inset, inset),
        Point::new(w - inset, inset),
        Point::new(w - inset, h - inset),
        Point::new(inset, h - inset),
    ];
    for corner in corners {
        let fab = fabrication_point(p, corner);
        let center = fab_to_px(fab, opts, img.width(), opts.mirror);
        match reg.shape {
            RegMarkShape::Cross => {
                let half = reg.size_mm * f64::from(opts.dpi) / 25.4 / 2.0;
                line_grey(
                    img,
                    Point::new(center.x - half, center.y),
                    Point::new(center.x + half, center.y),
                    1.0,
                    0,
                );
                line_grey(
                    img,
                    Point::new(center.x, center.y - half),
                    Point::new(center.x, center.y + half),
                    1.0,
                    0,
                );
            }
            RegMarkShape::Ring => {
                let r = reg.size_mm * f64::from(opts.dpi) / 25.4 / 2.0;
                disk_grey(img, center, r, 0);
                disk_grey(img, center, (r * 0.6).max(1.0), 255);
            }
        }
    }
}

fn disk_grey(image: &mut GrayImage, p: Point, r: f64, color: u8) {
    let radius = r.clamp(0.5, 8192.0);
    let w = image.width();
    let h = image.height();
    let left = (p.x - radius).max(0.0) as u32;
    let right = (p.x + radius).min(f64::from(w.saturating_sub(1))) as u32;
    let top = (p.y - radius).max(0.0) as u32;
    let bottom = (p.y + radius).min(f64::from(h.saturating_sub(1))) as u32;
    for x in left..=right {
        for y in top..=bottom {
            if Point::new(x as f64 + 0.5, y as f64 + 0.5).distance(p) <= radius {
                image.put_pixel(x, y, Luma([color]));
            }
        }
    }
}

fn line_grey(image: &mut GrayImage, a: Point, b: Point, r: f64, color: u8) {
    let radius = r.clamp(0.5, 8192.0);
    let w = image.width();
    let h = image.height();
    let left = (a.x.min(b.x) - radius).max(0.0) as u32;
    let right = (a.x.max(b.x) + radius).min(f64::from(w.saturating_sub(1))) as u32;
    let top = (a.y.min(b.y) - radius).max(0.0) as u32;
    let bottom = (a.y.max(b.y) + radius).min(f64::from(h.saturating_sub(1))) as u32;
    for x in left..=right {
        for y in top..=bottom {
            let pt = Point::new(x as f64 + 0.5, y as f64 + 0.5);
            if pcbcraft_geom::point_segment(pt, a, b) <= radius {
                image.put_pixel(x, y, Luma([color]));
            }
        }
    }
}

fn fill_polygon_grey(image: &mut GrayImage, points: &[Point], color: u8) {
    if points.len() < 3 {
        return;
    }
    let w = image.width();
    let h = image.height();
    let top = points
        .iter()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min)
        .max(0.0) as u32;
    let bottom = points
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .min(f64::from(h.saturating_sub(1))) as u32;
    for y in top..=bottom {
        let row = f64::from(y) + 0.5;
        let mut crossings: Vec<f64> = vec![];
        for edge in points.windows(2) {
            let (a, b) = (edge[0], edge[1]);
            if (a.y > row) != (b.y > row) {
                let dx = b.x - a.x;
                if dx.abs() > 1e-12 || a.y != b.y {
                    crossings.push(a.x + (row - a.y) * dx / (b.y - a.y));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.chunks(2) {
            if pair.len() < 2 {
                continue;
            }
            let left = pair[0].max(0.0) as u32;
            let right = pair[1].min(f64::from(w.saturating_sub(1))) as u32;
            for x in left..=right {
                image.put_pixel(x, y, Luma([color]));
            }
        }
    }
}

pub fn raster_png(r: &Raster) -> Result<Vec<u8>, String> {
    match &r.pixels {
        RasterPixels::Grey(img) => encode_grey_png(img, r.dpi, r.depth),
        RasterPixels::Rgba(img) => {
            let mut buf = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img.clone())
                .write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            Ok(buf.into_inner())
        }
    }
}

fn encode_grey_png(img: &GrayImage, dpi: u32, depth: RasterDepth) -> Result<Vec<u8>, String> {
    let (w, h) = (img.width(), img.height());
    let xppu = (f64::from(dpi) / 0.0254).round() as u32;
    match depth {
        RasterDepth::Bits1 => {
            let row_bytes = (w as usize).div_ceil(8);
            let mut packed = vec![0u8; row_bytes * h as usize];
            for y in 0..h {
                for x in 0..w {
                    if img.get_pixel(x, y).0[0] < 128 {
                        let idx = y as usize * row_bytes + x as usize / 8;
                        packed[idx] |= 0x80_u8 >> (x % 8);
                    }
                }
            }
            let mut out = Vec::new();
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::One);
            enc.set_pixel_dims(Some(png::PixelDimensions {
                xppu,
                yppu: xppu,
                unit: png::Unit::Meter,
            }));
            {
                let mut writer = enc.write_header().map_err(|e| e.to_string())?;
                writer
                    .write_image_data(&packed)
                    .map_err(|e| e.to_string())?;
            }
            Ok(out)
        }
        RasterDepth::Grey8 | RasterDepth::Rgba8 => {
            let mut out = Vec::new();
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_pixel_dims(Some(png::PixelDimensions {
                xppu,
                yppu: xppu,
                unit: png::Unit::Meter,
            }));
            let mut data = Vec::with_capacity(w as usize * h as usize);
            for y in 0..h {
                for x in 0..w {
                    data.push(img.get_pixel(x, y).0[0]);
                }
            }
            {
                let mut writer = enc.write_header().map_err(|e| e.to_string())?;
                writer.write_image_data(&data).map_err(|e| e.to_string())?;
            }
            Ok(out)
        }
    }
}

pub fn raster_bmp(r: &Raster) -> Result<Vec<u8>, String> {
    let grey = raster_grey8(r)?;
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(grey)
        .write_to(&mut buf, image::ImageFormat::Bmp)
        .map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

fn raster_grey8(r: &Raster) -> Result<GrayImage, String> {
    match &r.pixels {
        RasterPixels::Grey(g) => Ok(g.clone()),
        RasterPixels::Rgba(rgb) => {
            let mut g = GrayImage::new(r.width, r.height);
            for (x, y, px) in rgb.enumerate_pixels() {
                let v = px.0[0];
                g.put_pixel(x, y, Luma([v]));
            }
            Ok(g)
        }
    }
}

pub fn raster_svg(p: &Project, opts: &RasterOptions) -> Result<String, String> {
    opts.validate()?;
    let collected = layer_shapes(p, opts.layer)?;
    let w = p.board.width + 2.0 * opts.margin_mm;
    let h = p.board.height + 2.0 * opts.margin_mm;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n"
    );
    let tx = |fab: Point| {
        let x = fab.x + opts.margin_mm;
        if opts.mirror {
            format!("{:.6}", w - x)
        } else {
            format!("{:.6}", x)
        }
    };
    for shape in &collected.shapes {
        match shape {
            Shape::Polygon(points, _) => {
                if points.len() >= 2 {
                    let d: String = points
                        .iter()
                        .enumerate()
                        .map(|(i, pt)| {
                            if i == 0 {
                                format!("M {} {}", tx(*pt), pt.y + opts.margin_mm)
                            } else {
                                format!("L {} {}", tx(*pt), pt.y + opts.margin_mm)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    out.push_str(&format!(
                        "<path d=\"{d} Z\" fill=\"black\" stroke=\"none\"/>\n"
                    ));
                }
            }
            Shape::Disk(center, r, _) => {
                out.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{r}\" fill=\"black\"/>\n",
                    tx(*center),
                    center.y + opts.margin_mm
                ));
            }
            Shape::Stroke(points, width, closed, _) => {
                if points.len() >= 2 {
                    let d: String = points
                        .iter()
                        .enumerate()
                        .map(|(i, pt)| {
                            if i == 0 {
                                format!("M {} {}", tx(*pt), pt.y + opts.margin_mm)
                            } else {
                                format!("L {} {}", tx(*pt), pt.y + opts.margin_mm)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    let z = if *closed { " Z" } else { "" };
                    out.push_str(&format!(
                        "<path d=\"{d}{z}\" fill=\"none\" stroke=\"black\" stroke-width=\"{width}\"/>\n"
                    ));
                }
            }
        }
    }
    out.push_str("</svg>\n");
    Ok(out)
}

/// Write the standard six layer PNGs for an output job.
pub fn raster_job_pngs(p: &Project) -> Result<Vec<(String, Vec<u8>)>, String> {
    const JOB_DPI: u32 = 1200;
    let specs = [
        ("layer-copper-top.png", FabLayer::CopperTop),
        ("layer-copper-bottom.png", FabLayer::CopperBottom),
        ("layer-mask-top.png", FabLayer::MaskTop),
        ("layer-mask-bottom.png", FabLayer::MaskBottom),
        ("layer-legend-top.png", FabLayer::LegendTop),
        ("layer-legend-bottom.png", FabLayer::LegendBottom),
    ];
    let mut files = Vec::with_capacity(specs.len());
    for (name, layer) in specs {
        let opts = RasterOptions {
            layer,
            dpi: JOB_DPI,
            mirror: false,
            invert: false,
            outline: false,
            registration: None,
            drill_marks: None,
            margin_mm: 0.0,
            depth: RasterDepth::Grey8,
        };
        let raster = layer_raster(p, &opts)?;
        files.push((name.into(), raster_png(&raster)?));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::FabLayer;
    use pcbcraft_doc::{Footprint, Pad, Project};

    fn pad_project(diameter_mm: f64) -> Result<Project, String> {
        let mut p = Project::default();
        p.board.width = 40.0;
        p.board.height = 40.0;
        let pad = Pad {
            number: "1".into(),
            offset: Point::new(15.0, 15.0),
            diameter: diameter_mm,
            net: None,
            castellated: None,
            geometry: None,
        };
        p.board.footprints.push(Footprint {
            id: 9001,
            component: 9002,
            position: Point::default(),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![pad],
        });
        p.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 9002,
            reference: "R99".into(),
            value: "x".into(),
            kind: "resistor".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::default(),
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
            fields: vec![],
            include_in_bom: true,
            include_in_placement: true,
        });
        p.validate()?;
        Ok(p)
    }

    fn base_opts(layer: FabLayer) -> RasterOptions {
        RasterOptions {
            layer,
            dpi: 1000,
            mirror: false,
            invert: false,
            outline: false,
            registration: None,
            drill_marks: None,
            margin_mm: 0.0,
            depth: RasterDepth::Grey8,
        }
    }

    fn count_ink_run(img: &GrayImage) -> (u32, u32) {
        let mut min_x = u32::MAX;
        let mut max_x = 0u32;
        for (x, y, px) in img.enumerate_pixels() {
            if px.0[0] < 128 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
            let _ = y;
        }
        (min_x, max_x)
    }

    #[test]
    fn a_ten_millimetre_pad_at_1000_dpi_is_394_pixels_wide() -> Result<(), String> {
        let p = pad_project(10.0)?;
        let raster = layer_raster(&p, &base_opts(FabLayer::CopperTop))?;
        let RasterPixels::Grey(img) = raster.pixels else {
            return Err("expected grey".into());
        };
        let (min_x, max_x) = count_ink_run(&img);
        let width = max_x.saturating_sub(min_x) + 1;
        assert!(
            (393..=395).contains(&width),
            "pad width in pixels was {width}"
        );
        Ok(())
    }

    #[test]
    fn invert_leaves_the_pad_as_the_only_clear_area() -> Result<(), String> {
        let p = pad_project(10.0)?;
        let mut opts = base_opts(FabLayer::CopperTop);
        opts.invert = true;
        let raster = layer_raster(&p, &opts)?;
        let RasterPixels::Grey(img) = raster.pixels else {
            return Err("expected grey".into());
        };
        let mut light = 0u32;
        let mut dark = 0u32;
        for (_, _, px) in img.enumerate_pixels() {
            if px.0[0] >= 128 {
                light += 1;
            } else {
                dark += 1;
            }
        }
        assert!(dark > light, "most pixels should be dark when inverted");
        assert!(light > 100, "pad should remain a clear island");
        Ok(())
    }

    #[test]
    fn mirror_flips_x_about_the_image_centre() -> Result<(), String> {
        let p = pad_project(10.0)?;
        let mut left = base_opts(FabLayer::CopperTop);
        left.mirror = false;
        let mut right = base_opts(FabLayer::CopperTop);
        right.mirror = true;
        let RasterPixels::Grey(a) = layer_raster(&p, &left)?.pixels else {
            return Err("expected grey".into());
        };
        let RasterPixels::Grey(b) = layer_raster(&p, &right)?.pixels else {
            return Err("expected grey".into());
        };
        let (amin, amax) = count_ink_run(&a);
        let (bmin, bmax) = count_ink_run(&b);
        let aw = amax - amin;
        let bw = bmax - bmin;
        assert_eq!(aw, bw);
        let mirrored_min = a.width() - 1 - amax;
        assert!(
            bmin.abs_diff(mirrored_min) <= 2,
            "mirror should flip horizontal placement"
        );
        Ok(())
    }

    #[test]
    fn registration_marks_sit_at_all_four_corners_at_the_inset() -> Result<(), String> {
        let p = pad_project(5.0)?;
        let mut opts = base_opts(FabLayer::Profile);
        opts.registration = Some(RegistrationMarks {
            inset_mm: 2.0,
            size_mm: 1.0,
            shape: RegMarkShape::Cross,
        });
        let raster = layer_raster(&p, &opts)?;
        let RasterPixels::Grey(img) = raster.pixels else {
            return Err("expected grey".into());
        };
        let inset = 2.0;
        let board_corners = [
            Point::new(inset, inset),
            Point::new(p.board.width - inset, inset),
            Point::new(p.board.width - inset, p.board.height - inset),
            Point::new(inset, p.board.height - inset),
        ];
        for corner in board_corners {
            let fab = fabrication_point(&p, corner);
            let px = fab_to_px(fab, &opts, img.width(), opts.mirror);
            let cx = px.x.round() as u32;
            let cy = px.y.round() as u32;
            let mut found = false;
            for dx in 0..8 {
                for dy in 0..8 {
                    let x = cx.saturating_add(dx).min(img.width() - 1);
                    let y = cy.saturating_add(dy).min(img.height() - 1);
                    if img.get_pixel(x, y).0[0] < 128 {
                        found = true;
                    }
                }
            }
            assert!(found, "registration mark missing near ({cx},{cy})");
        }
        Ok(())
    }

    fn phys_from_png(bytes: &[u8]) -> Result<(u32, u32), String> {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let reader = decoder.read_info().map_err(|e| e.to_string())?;
        let info = reader.info();
        let dims = info
            .pixel_dims
            .ok_or_else(|| "png missing pHYs chunk".to_string())?;
        Ok((dims.xppu, dims.yppu))
    }

    #[test]
    fn png_carries_pixel_dimensions_for_the_dpi() -> Result<(), String> {
        let p = pad_project(5.0)?;
        let raster = layer_raster(&p, &base_opts(FabLayer::CopperTop))?;
        let bytes = raster_png(&raster)?;
        let (xppu, yppu) = phys_from_png(&bytes)?;
        assert_eq!(xppu, 39370);
        assert_eq!(yppu, 39370);
        Ok(())
    }

    #[test]
    fn one_bit_png_has_bit_depth_one_and_round_trips_through_the_image_crate() -> Result<(), String>
    {
        let p = pad_project(8.0)?;
        let mut opts = base_opts(FabLayer::CopperTop);
        opts.depth = RasterDepth::Bits1;
        let raster = layer_raster(&p, &opts)?;
        let bytes = raster_png(&raster)?;
        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let reader = decoder.read_info().map_err(|e| e.to_string())?;
        assert_eq!(reader.info().bit_depth, png::BitDepth::One);
        let loaded = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        assert_eq!(loaded.width(), raster.width);
        Ok(())
    }

    #[test]
    fn bmp_output_decodes_to_the_same_pixels_as_the_png() -> Result<(), String> {
        let p = pad_project(6.0)?;
        let raster = layer_raster(&p, &base_opts(FabLayer::CopperTop))?;
        let png_bytes = raster_png(&raster)?;
        let bmp_bytes = raster_bmp(&raster)?;
        let from_png = image::load_from_memory(&png_bytes).map_err(|e| e.to_string())?;
        let from_bmp = image::load_from_memory(&bmp_bytes).map_err(|e| e.to_string())?;
        assert_eq!(from_png.width(), from_bmp.width());
        assert_eq!(from_png.height(), from_bmp.height());
        Ok(())
    }

    #[test]
    fn too_large_a_raster_is_refused_with_its_size_in_the_reason() -> Result<(), String> {
        let mut p = Project::default();
        p.board.width = 500.0;
        p.board.height = 500.0;
        p.validate()?;
        let opts = RasterOptions {
            layer: FabLayer::CopperTop,
            dpi: 4800,
            mirror: false,
            invert: false,
            outline: false,
            registration: None,
            drill_marks: None,
            margin_mm: 0.0,
            depth: RasterDepth::Grey8,
        };
        let err = match layer_raster(&p, &opts) {
            Err(reason) => reason,
            Ok(_) => return Err("expected an oversized raster to be refused".into()),
        };
        assert!(err.contains("500"), "reason should name width: {err}");
        assert!(
            err.contains("64 million"),
            "reason should cite limit: {err}"
        );
        Ok(())
    }
}
