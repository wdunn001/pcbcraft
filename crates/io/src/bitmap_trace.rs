//! PNG reference pictures: strict decoding of attached images and tracing of
//! a two-tone bitmap into millimetre polygons.
//!
//! Only the outer outline of each ink blob is traced; holes inside a blob
//! (the counter of an "O") are not carried over.
use base64::Engine as _;
use image::RgbaImage;
use pcbcraft_doc::EmbeddedFile;
use pcbcraft_geom::Point;
use std::collections::{BTreeMap, BTreeSet};

/// Longest image side accepted, in pixels.
pub const MAX_IMAGE_SIDE: u32 = 4096;
/// Most ink blobs a single trace turns into polygons.
pub const MAX_TRACE_CONTOURS: usize = 32;
/// Hard cap on vertices per traced polygon.
pub const MAX_TRACE_VERTICES: usize = 256;

/// Tuning for [`bitmap_trace_polygons`].
#[derive(Clone, Copy, Debug)]
pub struct TraceOptions {
    /// Luma cut-off, 0 to 255; pixels darker than this count as ink.
    pub threshold: u8,
    /// Simplification tolerance in millimetres.
    pub smooth_mm: f64,
    /// Most vertices per polygon, 3 to [`MAX_TRACE_VERTICES`].
    pub max_vertices: usize,
    /// Millimetres per pixel.
    pub scale: f64,
    /// Trace the light pixels instead of the dark ones.
    pub invert: bool,
}
impl Default for TraceOptions {
    fn default() -> Self {
        Self {
            threshold: 128,
            smooth_mm: 0.05,
            max_vertices: 64,
            scale: 0.1,
            invert: false,
        }
    }
}

/// Raw bytes of a base64 attachment.
pub fn embedded_bytes(file: &EmbeddedFile) -> Result<Vec<u8>, String> {
    if !file.is_binary() {
        return Err(format!("{} is a text attachment, not an image", file.name));
    }
    base64::engine::general_purpose::STANDARD
        .decode(file.data.as_bytes())
        .map_err(|_| format!("{} is not valid base64", file.name))
}

/// Decode PNG bytes, refusing animated, 16-bit and oversized pictures before
/// any pixel is expanded.
pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("the attachment is not a PNG image".into());
    }
    let reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| format!("the PNG header is unreadable: {e}"))?;
    let info = reader.info();
    if info.animation_control.is_some() {
        return Err("animated PNG images are not supported".into());
    }
    if info.bit_depth == png::BitDepth::Sixteen {
        return Err("16-bit PNG images are not supported".into());
    }
    if info.width == 0
        || info.height == 0
        || info.width > MAX_IMAGE_SIDE
        || info.height > MAX_IMAGE_SIDE
    {
        return Err(format!(
            "the PNG must be 1 to {MAX_IMAGE_SIDE} pixels on each side"
        ));
    }
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|e| format!("the PNG could not be decoded: {e}"))?;
    Ok(decoded.to_rgba8())
}

/// Decode an attachment straight to pixels.
pub fn decode_embedded_png(file: &EmbeddedFile) -> Result<RgbaImage, String> {
    decode_png(&embedded_bytes(file)?)
}

fn is_ink(img: &RgbaImage, x: u32, y: u32, o: &TraceOptions) -> bool {
    let p = img.get_pixel(x, y).0;
    if p[3] < 128 {
        return false;
    }
    let luma = (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
    (luma < u32::from(o.threshold)) != o.invert
}

/// Trace the ink of a picture into closed polygons in millimetres.
///
/// Coordinates are relative to the picture's top-left corner and grow right
/// and down, the same way the board canvas does, so a traced outline lands
/// exactly over the picture that was traced. PNG row 0 is the top.
/// Polygons are returned open (the last vertex is not repeated), largest
/// first, at most [`MAX_TRACE_CONTOURS`] of them.
pub fn bitmap_trace_polygons(
    img: &RgbaImage,
    opts: &TraceOptions,
) -> Result<Vec<Vec<Point>>, String> {
    if !(0.001..=100.0).contains(&opts.scale) || !opts.scale.is_finite() {
        return Err("trace scale must be 0.001 to 100 millimetres per pixel".into());
    }
    if !opts.smooth_mm.is_finite() || !(0.0..=10.0).contains(&opts.smooth_mm) {
        return Err("trace smoothing must be 0 to 10 millimetres".into());
    }
    if !(3..=MAX_TRACE_VERTICES).contains(&opts.max_vertices) {
        return Err(format!(
            "trace max vertices must be 3 to {MAX_TRACE_VERTICES}"
        ));
    }
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 || w > MAX_IMAGE_SIDE as usize || h > MAX_IMAGE_SIDE as usize {
        return Err("the picture size is out of range".into());
    }
    let mut ink = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            ink[y * w + x] = is_ink(img, x as u32, y as u32, opts);
        }
    }
    // Label 4-connected blobs.
    let mut label = vec![0_u32; w * h];
    let mut sizes: Vec<usize> = vec![0];
    for start in 0..w * h {
        if !ink[start] || label[start] != 0 {
            continue;
        }
        let id = sizes.len() as u32;
        let mut count = 0;
        let mut stack = vec![start];
        label[start] = id;
        while let Some(i) = stack.pop() {
            count += 1;
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if ink[j] && label[j] == 0 {
                    label[j] = id;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        sizes.push(count);
    }
    if sizes.len() == 1 {
        return Err("no ink found: adjust the threshold".into());
    }
    let mut order: Vec<u32> = (1..sizes.len() as u32).collect();
    order.sort_by(|a, b| sizes[*b as usize].cmp(&sizes[*a as usize]).then(a.cmp(b)));
    let tolerance_px = opts.smooth_mm / opts.scale;
    let mut out = Vec::new();
    for id in order.into_iter().take(MAX_TRACE_CONTOURS) {
        if sizes[id as usize] < 4 {
            continue;
        }
        let Some(ring) = outer_ring(&label, w, h, id) else {
            continue;
        };
        let ring = simplify_ring(ring, tolerance_px, opts.max_vertices);
        if ring.len() < 3 {
            continue;
        }
        let poly: Vec<Point> = ring
            .iter()
            .map(|(x, y)| Point::new(*x * opts.scale, *y * opts.scale))
            .collect();
        if pcbcraft_geom::polygon_is_simple(&poly) {
            out.push(poly);
        }
    }
    if out.is_empty() {
        return Err("the trace found no usable outline: adjust threshold or smoothing".into());
    }
    Ok(out)
}

/// Walk the pixel-edge boundary of one blob and return its largest loop as
/// grid corners (x right, y down).
fn outer_ring(label: &[u32], w: usize, h: usize, id: u32) -> Option<Vec<(f64, f64)>> {
    let has = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && (x as usize) < w
            && (y as usize) < h
            && label[y as usize * w + x as usize] == id
    };
    // Directed edges keep the blob on the right-hand side (clockwise in
    // image coordinates); shared edges between two blob pixels are skipped.
    let mut next: BTreeMap<(i64, i64), Vec<(i64, i64)>> = BTreeMap::new();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !has(x, y) {
                continue;
            }
            if !has(x, y - 1) {
                next.entry((x, y)).or_default().push((x + 1, y));
            }
            if !has(x + 1, y) {
                next.entry((x + 1, y)).or_default().push((x + 1, y + 1));
            }
            if !has(x, y + 1) {
                next.entry((x + 1, y + 1)).or_default().push((x, y + 1));
            }
            if !has(x - 1, y) {
                next.entry((x, y + 1)).or_default().push((x, y));
            }
        }
    }
    let mut used: BTreeSet<((i64, i64), (i64, i64))> = BTreeSet::new();
    let mut best: Option<(f64, Vec<(f64, f64)>)> = None;
    let starts: Vec<(i64, i64)> = next.keys().copied().collect();
    for start in starts {
        let firsts = next.get(&start).cloned().unwrap_or_default();
        for first in firsts {
            if used.contains(&(start, first)) {
                continue;
            }
            let mut ring = vec![start];
            let (mut from, mut to) = (start, first);
            used.insert((from, to));
            let mut steps = 0_usize;
            loop {
                steps += 1;
                if steps > 4 * w * h + 8 {
                    return None;
                }
                if to == start {
                    break;
                }
                ring.push(to);
                let options = next.get(&to).cloned().unwrap_or_default();
                let dir = (to.0 - from.0, to.1 - from.1);
                // Prefer a right turn so a diagonal touch stays one loop.
                let right = (-dir.1, dir.0);
                let chosen = options
                    .iter()
                    .filter(|n| !used.contains(&(to, **n)))
                    .min_by_key(|n| {
                        let d = (n.0 - to.0, n.1 - to.1);
                        if d == right {
                            0
                        } else if d == dir {
                            1
                        } else {
                            2
                        }
                    })
                    .copied();
                let n = chosen?;
                used.insert((to, n));
                from = to;
                to = n;
            }
            let pts: Vec<(f64, f64)> = ring.iter().map(|(x, y)| (*x as f64, *y as f64)).collect();
            let area = ring_area(&pts).abs();
            if best.as_ref().is_none_or(|(a, _)| area > *a) {
                best = Some((area, pts));
            }
        }
    }
    best.map(|(_, ring)| ring)
}

fn ring_area(r: &[(f64, f64)]) -> f64 {
    let mut a = 0.0;
    for i in 0..r.len() {
        let (p, q) = (r[i], r[(i + 1) % r.len()]);
        a += p.0 * q.1 - q.0 * p.1;
    }
    a / 2.0
}

/// Drop collinear points, then thin the ring with a closed Douglas-Peucker
/// pass, widening the tolerance until the vertex cap holds.
fn simplify_ring(ring: Vec<(f64, f64)>, tolerance: f64, max_vertices: usize) -> Vec<(f64, f64)> {
    let n = ring.len();
    let mut corners: Vec<(f64, f64)> = Vec::new();
    for i in 0..n {
        let (a, b, c) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
        if cross != 0.0 {
            corners.push(b);
        }
    }
    if corners.len() <= 3 {
        return corners;
    }
    let mut tol = tolerance.max(0.0);
    loop {
        let thinned = douglas_closed(&corners, tol);
        if thinned.len() <= max_vertices || thinned.len() <= 3 {
            return thinned;
        }
        tol = if tol <= 0.0 { 0.25 } else { tol * 1.5 };
    }
}

fn douglas_closed(ring: &[(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
    // Split the loop at the two points farthest apart so each half is open.
    let mut far = 0;
    let mut dist = -1.0;
    for (i, p) in ring.iter().enumerate() {
        let d = (p.0 - ring[0].0).powi(2) + (p.1 - ring[0].1).powi(2);
        if d > dist {
            dist = d;
            far = i;
        }
    }
    let mut first: Vec<(f64, f64)> = ring[..=far].to_vec();
    let mut second: Vec<(f64, f64)> = ring[far..].to_vec();
    second.push(ring[0]);
    let mut keep = douglas_open(&mut first, tol);
    let tail = douglas_open(&mut second, tol);
    keep.pop();
    keep.extend(tail);
    keep.pop();
    keep
}

fn douglas_open(pts: &mut [(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (mut index, mut worst) = (0, -1.0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = point_line_distance(*p, a, b);
        if d > worst {
            worst = d;
            index = i;
        }
    }
    if worst <= tol {
        return vec![a, b];
    }
    let mut l = douglas_open(&mut pts[..=index].to_vec(), tol);
    let r = douglas_open(&mut pts[index..], tol);
    l.pop();
    l.extend(r);
    l
}

fn point_line_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/logo-64x64-two-color.png");

    #[test]
    fn bitmap_trace_two_color_fixture_yields_one_polygon_within_vertex_cap() -> Result<(), String> {
        let img = decode_png(FIXTURE)?;
        assert_eq!((img.width(), img.height()), (64, 64));
        let polys = bitmap_trace_polygons(&img, &TraceOptions::default())?;
        assert_eq!(polys.len(), 1, "one ink blob gives one polygon");
        let poly = &polys[0];
        assert!(poly.len() >= 3 && poly.len() <= 64, "within the vertex cap");
        assert_eq!(poly.len(), 6, "an L shape has six corners");
        let xs = poly.iter().map(|p| p.x);
        let ys = poly.iter().map(|p| p.y);
        assert!((xs.clone().fold(f64::INFINITY, f64::min) - 1.0).abs() < 1e-9);
        assert!((xs.fold(f64::NEG_INFINITY, f64::max) - 4.1).abs() < 1e-9);
        // Ink rows 10..=54 span y 1.0 to 5.5 (rows grow downward).
        assert!((ys.clone().fold(f64::INFINITY, f64::min) - 1.0).abs() < 1e-9);
        assert!((ys.fold(f64::NEG_INFINITY, f64::max) - 5.5).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn bitmap_trace_caps_vertices_and_rejects_blank_pictures() -> Result<(), String> {
        let img = decode_png(FIXTURE)?;
        let capped = TraceOptions {
            max_vertices: 4,
            smooth_mm: 0.0,
            ..TraceOptions::default()
        };
        let polys = bitmap_trace_polygons(&img, &capped)?;
        assert!(polys[0].len() <= 4);
        let blank = RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]));
        assert!(bitmap_trace_polygons(&blank, &TraceOptions::default()).is_err());
        let inverted = TraceOptions {
            invert: true,
            ..TraceOptions::default()
        };
        assert_eq!(bitmap_trace_polygons(&img, &inverted)?.len(), 1);
        Ok(())
    }

    #[test]
    fn decode_png_refuses_foreign_and_oversized_data() {
        assert!(decode_png(b"GIF89a").is_err());
        assert!(decode_png(&FIXTURE[..40]).is_err());
    }
}
