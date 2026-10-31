//! True-size UV print artwork: colour PNG, white mask, optional gloss, and SVG in millimetres.
use crate::layers::{
    FabLayer, Shape, ShapePolarity, fabrication_point, legend_shapes, profile_edge,
};
use crate::raster::{RasterDepth, RasterOptions, RasterPixels, rasterize_shapes};
use image::{GrayImage, Luma, Rgba, RgbaImage};
use pcbcraft_doc::{Fiducial, Layer, Project, UvPrinterProfile};
use pcbcraft_geom::Point;

const MAX_PIXELS: u64 = 64_000_000;
const STUDIO_MAX_AXIS_PX: u32 = 12_800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UvPrintSide {
    Top,
    Bottom,
}

impl UvPrintSide {
    pub fn layer(self) -> Layer {
        match self {
            UvPrintSide::Top => Layer::Front,
            UvPrintSide::Bottom => Layer::Back,
        }
    }

    pub fn file_stem(self) -> &'static str {
        match self {
            UvPrintSide::Top => "top",
            UvPrintSide::Bottom => "bottom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UvPrintWhiteLayer {
    Separate,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UvPrintExperiment {
    None,
    Mask,
    Resist,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UvPrintOptions {
    pub side: UvPrintSide,
    pub dpi: u32,
    pub white_layer: UvPrintWhiteLayer,
    pub gloss: bool,
    pub bleed_mm: f64,
    pub include_outline: bool,
    pub include_fiducials: bool,
    pub experiment: UvPrintExperiment,
    pub include_values: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UvPrintArtwork {
    pub width_px: u32,
    pub height_px: u32,
    pub width_mm: f64,
    pub height_mm: f64,
    pub dpi: u32,
    pub colour_png: Vec<u8>,
    pub white_png: Option<Vec<u8>>,
    pub gloss_png: Option<Vec<u8>>,
    pub svg: String,
    pub skipped: Vec<String>,
}

pub fn eufymake_e1_profile() -> UvPrinterProfile {
    pcbcraft_doc::eufymake_e1_uv_printer()
}

pub fn uv_print_artwork(
    p: &Project,
    side: UvPrintSide,
    profile: &UvPrinterProfile,
    options: &UvPrintOptions,
) -> Result<UvPrintArtwork, String> {
    profile.validate()?;
    p.validate()?;
    validate_options(profile, options)?;
    if options.experiment == UvPrintExperiment::Resist {
        return Err(
            "experiment resist is reserved until the shop workflow decides print order".into(),
        );
    }
    let bleed = options.bleed_mm;
    let w_mm = p.board.width;
    let h_mm = p.board.height;
    if w_mm > profile.bed_width_mm {
        return Err(format!(
            "board width {:.1} mm exceeds the {:.0} mm bed",
            w_mm, profile.bed_width_mm
        ));
    }
    if h_mm > profile.bed_height_mm {
        return Err(format!(
            "board height {:.1} mm exceeds the {:.0} mm bed",
            h_mm, profile.bed_height_mm
        ));
    }
    let w_px = pixel_extent(w_mm, bleed, options.dpi);
    let h_px = pixel_extent(h_mm, bleed, options.dpi);
    let pixels = u64::from(w_px) * u64::from(h_px);
    if w_px > STUDIO_MAX_AXIS_PX {
        return Err(format!(
            "width would be {w_px} pixels, above the {STUDIO_MAX_AXIS_PX} pixel studio import limit"
        ));
    }
    if h_px > STUDIO_MAX_AXIS_PX {
        return Err(format!(
            "height would be {h_px} pixels, above the {STUDIO_MAX_AXIS_PX} pixel studio import limit"
        ));
    }
    if pixels > MAX_PIXELS {
        return Err(format!(
            "artwork would be {w_px} by {h_px} ({pixels} pixels), above the 64 million pixel limit"
        ));
    }
    let legend = legend_shapes(
        p,
        side.layer(),
        profile.pad_clearance_mm,
        options.include_values,
    )?;
    let mut shapes = legend.shapes;
    if options.include_outline {
        shapes.push(Shape::Stroke(
            profile_edge(p),
            0.12,
            true,
            ShapePolarity::Positive,
        ));
    }
    if options.include_fiducials {
        for fid in fiducials_on_side(p, side) {
            let center = fabrication_point(p, fid.position);
            shapes.push(Shape::Disk(
                center,
                fid.diameter / 2.0,
                ShapePolarity::Positive,
            ));
        }
    }
    let raster_opts = RasterOptions {
        layer: if side == UvPrintSide::Top {
            FabLayer::LegendTop
        } else {
            FabLayer::LegendBottom
        },
        dpi: options.dpi,
        mirror: false,
        invert: false,
        outline: false,
        registration: None,
        drill_marks: None,
        margin_mm: bleed,
        depth: RasterDepth::Grey8,
    };
    let raster = rasterize_shapes(p, &shapes, &raster_opts)?;
    let RasterPixels::Grey(grey) = raster.pixels else {
        return Err("expected grey legend matte".into());
    };
    let colour_png = rgba_png_from_matte(&grey, options.dpi)?;
    let white_png = if options.white_layer == UvPrintWhiteLayer::Separate {
        Some(white_mask_png(&grey, options.dpi, profile.white_choke_mm)?)
    } else {
        None
    };
    let gloss_png = if options.gloss && profile.has_gloss_channel() {
        Some(gloss_mask_png(&grey, options.dpi)?)
    } else {
        None
    };
    let svg = uv_svg(p, side, &shapes, w_mm, h_mm, options)?;
    Ok(UvPrintArtwork {
        width_px: w_px,
        height_px: h_px,
        width_mm: w_mm + 2.0 * bleed,
        height_mm: h_mm + 2.0 * bleed,
        dpi: options.dpi,
        colour_png,
        white_png,
        gloss_png,
        svg,
        skipped: legend.skipped,
    })
}

fn validate_options(profile: &UvPrinterProfile, options: &UvPrintOptions) -> Result<(), String> {
    if !profile.allows_dpi(options.dpi) {
        return Err(format!(
            "dpi {} is not allowed for profile {}; choose one of {:?}",
            options.dpi, profile.name, profile.dpi_options
        ));
    }
    if !(0.0..=10.0).contains(&options.bleed_mm) {
        return Err("bleed_mm must be between 0 and 10".into());
    }
    Ok(())
}

fn pixel_extent(extent_mm: f64, bleed_mm: f64, dpi: u32) -> u32 {
    ((extent_mm + 2.0 * bleed_mm) * f64::from(dpi) / 25.4)
        .ceil()
        .max(1.0) as u32
}

fn fiducials_on_side(p: &Project, side: UvPrintSide) -> Vec<&Fiducial> {
    p.board
        .fiducials
        .iter()
        .filter(|f| match side {
            UvPrintSide::Top => f.layer == Layer::Front,
            UvPrintSide::Bottom => f.layer == Layer::Back,
        })
        .collect()
}

fn rgba_png_from_matte(grey: &GrayImage, dpi: u32) -> Result<Vec<u8>, String> {
    let (w, h) = (grey.width(), grey.height());
    let mut rgba = RgbaImage::new(w, h);
    for (x, y, l) in grey.enumerate_pixels() {
        let ink = l.0[0] < 128;
        if ink {
            rgba.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        } else {
            rgba.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        }
    }
    encode_rgba_png(&rgba, dpi)
}

fn white_mask_png(grey: &GrayImage, dpi: u32, choke_mm: f64) -> Result<Vec<u8>, String> {
    let mut mask = opaque_matte(grey);
    if choke_mm > 0.0 {
        erode_matte(&mut mask, choke_mm, dpi);
    }
    encode_grey_png(&mask, dpi)
}

fn gloss_mask_png(grey: &GrayImage, dpi: u32) -> Result<Vec<u8>, String> {
    let mask = opaque_matte(grey);
    encode_grey_png(&mask, dpi)
}

fn opaque_matte(grey: &GrayImage) -> GrayImage {
    let (w, h) = (grey.width(), grey.height());
    let mut out = GrayImage::new(w, h);
    for (x, y, l) in grey.enumerate_pixels() {
        let v = if l.0[0] < 128 { 255 } else { 0 };
        out.put_pixel(x, y, Luma([v]));
    }
    out
}

fn erode_matte(img: &mut GrayImage, choke_mm: f64, dpi: u32) {
    let radius_px = (choke_mm * f64::from(dpi) / 25.4).ceil().max(1.0) as i32;
    let (w, h) = (img.width() as i32, img.height() as i32);
    let src: GrayImage = img.clone();
    for y in 0..h {
        for x in 0..w {
            let mut keep = true;
            'scan: for dy in -radius_px..=radius_px {
                for dx in -radius_px..=radius_px {
                    if dx * dx + dy * dy > radius_px * radius_px {
                        continue;
                    }
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        keep = false;
                        break 'scan;
                    }
                    if src.get_pixel(nx as u32, ny as u32).0[0] < 128 {
                        keep = false;
                        break 'scan;
                    }
                }
            }
            let v = if keep { 255 } else { 0 };
            img.put_pixel(x as u32, y as u32, Luma([v]));
        }
    }
}

fn encode_rgba_png(img: &RgbaImage, dpi: u32) -> Result<Vec<u8>, String> {
    let (w, h) = (img.width(), img.height());
    let xppu = (f64::from(dpi) / 0.0254).round() as u32;
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_pixel_dims(Some(png::PixelDimensions {
        xppu,
        yppu: xppu,
        unit: png::Unit::Meter,
    }));
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(img.as_raw())
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

fn encode_grey_png(img: &GrayImage, dpi: u32) -> Result<Vec<u8>, String> {
    let (w, h) = (img.width(), img.height());
    let xppu = (f64::from(dpi) / 0.0254).round() as u32;
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_pixel_dims(Some(png::PixelDimensions {
        xppu,
        yppu: xppu,
        unit: png::Unit::Meter,
    }));
    {
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(img.as_raw())
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn uv_svg(
    p: &Project,
    side: UvPrintSide,
    shapes: &[Shape],
    w_mm: f64,
    h_mm: f64,
    options: &UvPrintOptions,
) -> Result<String, String> {
    let bleed = options.bleed_mm;
    let total_w = w_mm + 2.0 * bleed;
    let total_h = h_mm + 2.0 * bleed;
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total_w}mm\" height=\"{total_h}mm\" viewBox=\"0 0 {total_w} {total_h}\">\n"
    );
    s.push_str("<g id=\"legend\">\n");
    for shape in shapes {
        s.push_str(&shape_svg_mm(shape, p, bleed)?);
    }
    s.push_str("</g>\n");
    if options.include_outline {
        s.push_str("<g id=\"outline\">\n");
        s.push_str(&polyline_svg(&profile_edge(p), bleed));
        s.push_str("</g>\n");
    }
    if options.include_fiducials {
        s.push_str("<g id=\"fiducials\">\n");
        for fid in fiducials_on_side(p, side) {
            let fab = fabrication_point(p, fid.position);
            let cx = fab.x + bleed;
            let cy = fab.y + bleed;
            let r = fid.diameter / 2.0;
            s.push_str(&format!(
                "<circle cx=\"{cx:.4}\" cy=\"{cy:.4}\" r=\"{r:.4}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.05\"/>\n"
            ));
        }
        s.push_str("</g>\n");
    }
    s.push_str("</svg>\n");
    Ok(s)
}

fn polyline_svg(points: &[Point], bleed: f64) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let coords: Vec<String> = points
        .iter()
        .map(|pt| format!("{:.4},{:.4}", pt.x + bleed, pt.y + bleed))
        .collect();
    format!(
        "<polyline fill=\"none\" stroke=\"black\" stroke-width=\"0.1\" points=\"{}\"/>\n",
        coords.join(" ")
    )
}

fn shape_svg_mm(shape: &Shape, p: &Project, bleed: f64) -> Result<String, String> {
    let _ = p;
    match shape {
        Shape::Stroke(points, width, closed, _) => {
            if points.len() < 2 {
                return Ok(String::new());
            }
            let coords: Vec<String> = points
                .iter()
                .map(|pt| format!("{:.4},{:.4}", pt.x + bleed, pt.y + bleed))
                .collect();
            let tag = if *closed { "polygon" } else { "polyline" };
            Ok(format!(
                "<{tag} fill=\"none\" stroke=\"black\" stroke-width=\"{width:.4}\" points=\"{}\"/>\n",
                coords.join(" ")
            ))
        }
        Shape::Disk(center, radius, _) => {
            let cx = center.x + bleed;
            let cy = center.y + bleed;
            Ok(format!(
                "<circle cx=\"{cx:.4}\" cy=\"{cy:.4}\" r=\"{radius:.4}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.05\"/>\n"
            ))
        }
        Shape::Polygon(points, _) => {
            if points.len() < 3 {
                return Ok(String::new());
            }
            let coords: Vec<String> = points
                .iter()
                .map(|pt| format!("{:.4},{:.4}", pt.x + bleed, pt.y + bleed))
                .collect();
            Ok(format!(
                "<polygon fill=\"none\" stroke=\"black\" stroke-width=\"0.05\" points=\"{}\"/>\n",
                coords.join(" ")
            ))
        }
    }
}

pub fn write_uv_print_job_files(
    p: &Project,
    directory: &std::path::Path,
    profile: &UvPrinterProfile,
    options: &UvPrintOptions,
) -> Result<Vec<String>, String> {
    let art = uv_print_artwork(p, options.side, profile, options)?;
    let stem = options.side.file_stem();
    let mut written = Vec::new();
    let colour_name = format!("{stem}-colour.png");
    crate::atomic_write(&directory.join(&colour_name), &art.colour_png)?;
    written.push(colour_name);
    if let Some(white) = &art.white_png {
        let name = format!("{stem}-white.png");
        crate::atomic_write(&directory.join(&name), white)?;
        written.push(name);
    }
    if let Some(gloss) = &art.gloss_png {
        let name = format!("{stem}-gloss.png");
        crate::atomic_write(&directory.join(&name), gloss)?;
        written.push(name);
    }
    let svg_name = format!("{stem}.svg");
    crate::atomic_write(&directory.join(&svg_name), art.svg.as_bytes())?;
    written.push(svg_name);
    Ok(written)
}

#[cfg(test)]
fn phys_from_png(bytes: &[u8]) -> Result<(u32, u32), String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info().map_err(|e| e.to_string())?;
    let dims = reader
        .info()
        .pixel_dims
        .ok_or_else(|| "png missing pHYs chunk".to_string())?;
    Ok((dims.xppu, dims.yppu))
}

#[cfg(test)]
fn decode_rgba_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut rgba = vec![
        0u8;
        reader
            .output_buffer_size()
            .ok_or_else(|| "png frame size is unknown".to_string())?
    ];
    reader.next_frame(&mut rgba).map_err(|e| e.to_string())?;
    Ok((reader.info().width, reader.info().height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Footprint, Project};
    use pcbcraft_library::{FootprintDefinition, FootprintPad, Graphic};

    fn small_pad() -> FootprintPad {
        FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::new(0.0, 3.0),
            rotation: 0.0,
            size: Point::new(0.5, 0.5),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        }
    }

    fn test_fp(graphics: Vec<Graphic>) -> FootprintDefinition {
        FootprintDefinition {
            name: "fp".into(),
            description: String::new(),
            keywords: String::new(),
            pads: vec![small_pad()],
            graphics,
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        }
    }

    fn small_board(w: f64, h: f64) -> Result<Project, String> {
        let mut p = Project::default();
        p.board.width = w;
        p.board.height = h;
        p.validate()?;
        Ok(p)
    }

    fn base_options(side: UvPrintSide) -> UvPrintOptions {
        UvPrintOptions {
            side,
            dpi: 1440,
            white_layer: UvPrintWhiteLayer::Separate,
            gloss: false,
            bleed_mm: 0.3,
            include_outline: true,
            include_fiducials: true,
            experiment: UvPrintExperiment::None,
            include_values: false,
        }
    }

    #[test]
    fn twenty_by_thirty_board_at_1440_dpi_with_bleed_is_1168_by_1735_pixels() -> Result<(), String>
    {
        let p = small_board(20.0, 30.0)?;
        let profile = eufymake_e1_profile();
        let art = uv_print_artwork(
            &p,
            UvPrintSide::Top,
            &profile,
            &base_options(UvPrintSide::Top),
        )?;
        assert!((1167..=1169).contains(&art.width_px));
        assert!((1734..=1736).contains(&art.height_px));
        Ok(())
    }

    #[test]
    fn png_carries_phys_56693_for_1440_dpi() -> Result<(), String> {
        let p = small_board(20.0, 30.0)?;
        let profile = eufymake_e1_profile();
        let art = uv_print_artwork(
            &p,
            UvPrintSide::Top,
            &profile,
            &base_options(UvPrintSide::Top),
        )?;
        let (xppu, yppu) = phys_from_png(&art.colour_png)?;
        assert_eq!(xppu, 56693);
        assert_eq!(yppu, 56693);
        Ok(())
    }

    #[test]
    fn pixels_outside_legend_are_transparent() -> Result<(), String> {
        let p = small_board(20.0, 30.0)?;
        let profile = eufymake_e1_profile();
        let art = uv_print_artwork(
            &p,
            UvPrintSide::Top,
            &profile,
            &base_options(UvPrintSide::Top),
        )?;
        let (_, _, rgba) = decode_rgba_png(&art.colour_png)?;
        let mut transparent = 0u32;
        for chunk in rgba.chunks(4) {
            if chunk.get(3) == Some(&0) {
                transparent += 1;
            }
        }
        assert!(transparent > 0, "background should stay transparent");
        Ok(())
    }

    #[test]
    fn white_mask_matches_opaque_colour_pixels() -> Result<(), String> {
        let p = small_board(20.0, 30.0)?;
        let mut profile = eufymake_e1_profile();
        profile.white_choke_mm = 0.0;
        let art = uv_print_artwork(
            &p,
            UvPrintSide::Top,
            &profile,
            &base_options(UvPrintSide::Top),
        )?;
        let (w_px, h_px, colour) = decode_rgba_png(&art.colour_png)?;
        let white_bytes = art.white_png.ok_or("missing white png")?;
        let white = image::load_from_memory(&white_bytes)
            .map_err(|e| e.to_string())?
            .to_luma8();
        assert_eq!(white.width(), w_px);
        assert_eq!(white.height(), h_px);
        for y in 0..h_px {
            for x in 0..w_px {
                let i = (y * w_px + x) as usize * 4;
                let alpha = colour.get(i + 3).copied().unwrap_or(0);
                let mask = white.get_pixel(x, y).0[0];
                if alpha > 0 {
                    assert_eq!(mask, 255, "opaque colour needs white ink at ({x},{y})");
                } else {
                    assert_eq!(
                        mask, 0,
                        "transparent colour must not print white at ({x},{y})"
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn white_choke_shrinks_the_white_mask() -> Result<(), String> {
        let mut p = small_board(30.0, 30.0)?;
        p.components.push(pcbcraft_doc::Component {
            sourcing: Default::default(),
            sheet: 0,
            instance: None,
            prototype: None,
            id: 9402,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![pcbcraft_doc::Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::default(),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "proj".into(),
                definition: test_fp(vec![Graphic::Rectangle {
                    layer: "F.SilkS".into(),
                    start: Point::new(10.0, 10.0),
                    end: Point::new(20.0, 20.0),
                    width: 0.12,
                    fill: "solid".into(),
                }]),
            }),
            fields: vec![],
        });
        p.board.footprints.push(Footprint {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: 9401,
            component: 9402,
            position: Point::default(),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(0.0, 3.0),
                diameter: 0.5,
                net: None,
                castellated: None,
                geometry: Some(small_pad()),
            }],
        });
        p.validate()?;
        let profile = eufymake_e1_profile();
        let loose = base_options(UvPrintSide::Top);
        let tight = loose.clone();
        let mut choked_profile = profile.clone();
        choked_profile.white_choke_mm = 0.5;
        let art_loose = uv_print_artwork(&p, UvPrintSide::Top, &profile, &loose)?;
        let art_tight = uv_print_artwork(&p, UvPrintSide::Top, &choked_profile, &tight)?;
        let white_loose = image::load_from_memory(art_loose.white_png.as_ref().ok_or("white")?)
            .map_err(|e| e.to_string())?
            .to_luma8();
        let white_tight = image::load_from_memory(art_tight.white_png.as_ref().ok_or("white")?)
            .map_err(|e| e.to_string())?
            .to_luma8();
        let count = |img: &GrayImage| img.pixels().filter(|px| px.0[0] > 0).count();
        assert!(
            count(&white_tight) < count(&white_loose),
            "choke should shrink the white mask"
        );
        Ok(())
    }

    #[test]
    fn legend_strokes_under_pad_clearance_are_removed() -> Result<(), String> {
        let mut p = Project::default();
        p.board.width = 30.0;
        p.board.height = 30.0;
        let pad = FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(4.0, 4.0),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        };
        p.board.footprints.push(Footprint {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: 9201,
            component: 9202,
            position: Point::new(15.0, 15.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 4.0,
                net: None,
                castellated: None,
                geometry: Some(pad.clone()),
            }],
        });
        p.components.push(pcbcraft_doc::Component {
            sourcing: Default::default(),
            sheet: 0,
            instance: None,
            prototype: None,
            id: 9202,
            reference: "U1".into(),
            value: "test".into(),
            kind: "connector".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![pcbcraft_doc::Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::default(),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "proj".into(),
                definition: {
                    let mut def = test_fp(vec![Graphic::Line {
                        start: Point::new(-5.0, 0.0),
                        end: Point::new(5.0, 0.0),
                        width: 0.15,
                        layer: "F.SilkS".into(),
                    }]);
                    def.pads = vec![pad];
                    def
                },
            }),
            fields: vec![],
        });
        p.validate()?;
        let profile = eufymake_e1_profile();
        let art = uv_print_artwork(
            &p,
            UvPrintSide::Top,
            &profile,
            &base_options(UvPrintSide::Top),
        )?;
        let (w_px, h_px, colour) = decode_rgba_png(&art.colour_png)?;
        let cx = w_px / 2;
        let cy = h_px / 2;
        let i = (cy * w_px + cx) as usize * 4;
        assert_eq!(
            colour.get(i + 3).copied().unwrap_or(0),
            0,
            "legend stroke over the pad should be cleared"
        );
        Ok(())
    }

    #[test]
    fn four_hundred_by_five_hundred_board_is_refused_for_eufy_bed() -> Result<(), String> {
        let p = small_board(400.0, 500.0)?;
        let profile = eufymake_e1_profile();
        let err = uv_print_artwork(
            &p,
            UvPrintSide::Top,
            &profile,
            &base_options(UvPrintSide::Top),
        )
        .err()
        .ok_or("expected refusal")?;
        assert!(err.contains("bed"), "reason should mention bed: {err}");
        Ok(())
    }

    #[test]
    fn full_bed_at_1440_is_refused_for_studio_pixel_cap() -> Result<(), String> {
        let p = small_board(330.0, 420.0)?;
        let profile = eufymake_e1_profile();
        let err = uv_print_artwork(
            &p,
            UvPrintSide::Top,
            &profile,
            &base_options(UvPrintSide::Top),
        )
        .err()
        .ok_or("expected refusal")?;
        assert!(
            err.contains("12800") || err.contains("studio"),
            "reason should cite pixel cap: {err}"
        );
        Ok(())
    }

    #[test]
    fn uv_svg_groups_legend_outline_and_fiducials() -> Result<(), String> {
        let mut p = small_board(20.0, 30.0)?;
        p.board.fiducials.push(Fiducial {
            id: 1,
            position: Point::new(2.0, 2.0),
            diameter: 1.0,
            layer: Layer::Front,
        });
        let profile = eufymake_e1_profile();
        let art = uv_print_artwork(
            &p,
            UvPrintSide::Top,
            &profile,
            &base_options(UvPrintSide::Top),
        )?;
        assert!(art.svg.contains("id=\"legend\""));
        assert!(art.svg.contains("id=\"outline\""));
        assert!(art.svg.contains("id=\"fiducials\""));
        assert!(art.svg.contains("width=\"20.6mm\""));
        Ok(())
    }

    #[test]
    fn mirror_stays_off_on_bottom_side() -> Result<(), String> {
        let mut p = small_board(25.0, 25.0)?;
        p.components.push(pcbcraft_doc::Component {
            sourcing: Default::default(),
            sheet: 0,
            instance: None,
            prototype: None,
            id: 9302,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![pcbcraft_doc::Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::default(),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "proj".into(),
                definition: test_fp(vec![Graphic::Line {
                    start: Point::new(5.0, 5.0),
                    end: Point::new(10.0, 5.0),
                    width: 0.2,
                    layer: "B.SilkS".into(),
                }]),
            }),
            fields: vec![],
        });
        p.board.footprints.push(Footprint {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: 9301,
            component: 9302,
            position: Point::new(5.0, 5.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(0.0, 3.0),
                diameter: 0.5,
                net: None,
                castellated: None,
                geometry: Some(small_pad()),
            }],
        });
        p.validate()?;
        let profile = eufymake_e1_profile();
        let mut opts = base_options(UvPrintSide::Bottom);
        opts.side = UvPrintSide::Bottom;
        opts.include_outline = false;
        opts.include_fiducials = false;
        let art = uv_print_artwork(&p, UvPrintSide::Bottom, &profile, &opts)?;
        let (w_px, h_px, colour) = decode_rgba_png(&art.colour_png)?;
        let mut ink_x = 0u32;
        'scan: for y in 0..h_px {
            for x in 0..w_px {
                let i = (y * w_px + x) as usize * 4;
                if colour.get(i + 3).copied().unwrap_or(0) > 0 {
                    ink_x = x;
                    break 'scan;
                }
            }
        }
        assert!(ink_x > 0, "bottom legend should land without mirroring");
        Ok(())
    }
}
