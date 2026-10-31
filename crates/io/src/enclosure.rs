//! Enclosure STEP scan: bounding box, named datum points and planar faces only.
//! Pure Rust text parsing; no solid kernel.

use pcbcraft_doc::Datum;
use std::collections::BTreeMap;

/// Largest enclosure STEP accepted, in bytes.
pub const MAX_ENCLOSURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENCLOSURE_ENTITIES: usize = 50_000;
const MAX_ENCLOSURE_LINES: usize = 400_000;
const MAX_PLANAR_FACES: usize = 512;
const MAX_FACE_VERTICES: usize = 256;

/// Named point read from the exchange file.
#[derive(Clone, Debug, PartialEq)]
pub struct ScannedDatum {
    pub name: String,
    pub position: [f64; 3],
}

/// One planar face as a polygon loop in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanarFace {
    pub polygon: Vec<[f64; 3]>,
}

/// Cached axis-aligned bounds in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanBounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// Result of scanning an enclosure STEP file.
#[derive(Clone, Debug, PartialEq)]
pub struct EnclosureScan {
    pub bounds: ScanBounds,
    pub datums: Vec<ScannedDatum>,
    pub planar_faces: Vec<PlanarFace>,
    pub hidden_face_count: usize,
    pub notice: Option<String>,
}

struct Exchange {
    entities: BTreeMap<u32, String>,
}

/// Read an enclosure STEP file for bounds, datums and planar faces.
pub fn read_enclosure_step(bytes: &[u8]) -> Result<EnclosureScan, String> {
    if bytes.len() > MAX_ENCLOSURE_BYTES {
        return Err("the enclosure file is over the size limit".into());
    }
    let text = String::from_utf8_lossy(bytes);
    let exchange = parse_data_section(&text)?;
    let scale = unit_scale_to_mm(&exchange);
    scan_exchange(&exchange, scale)
}

fn parse_data_section(text: &str) -> Result<Exchange, String> {
    let upper = text.to_ascii_uppercase();
    let data_start = upper
        .find("DATA;")
        .ok_or("the enclosure file has no data section")?;
    let data = &text[data_start + 5..];
    let end = data
        .to_ascii_uppercase()
        .find("ENDSEC;")
        .unwrap_or(data.len());
    let data = &data[..end];
    let mut entities = BTreeMap::new();
    let mut lines = 0usize;
    for line in data.lines() {
        lines += 1;
        if lines > MAX_ENCLOSURE_LINES {
            return Err("the enclosure file has too many lines".into());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("/*") {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix('#') else {
            continue;
        };
        let (id_text, body) = rest
            .split_once('=')
            .ok_or("the enclosure file has a damaged entity record")?;
        let id: u32 = id_text
            .trim()
            .parse()
            .map_err(|_| "the enclosure file has a damaged entity id")?;
        if id == 0 {
            return Err("the enclosure file has a damaged entity id".into());
        }
        let body = body.trim().trim_end_matches(';').trim();
        if body.is_empty() {
            return Err("the enclosure file has an empty entity record".into());
        }
        if entities.len() >= MAX_ENCLOSURE_ENTITIES {
            return Err("the enclosure file has too many entities".into());
        }
        entities.insert(id, body.to_string());
    }
    if entities.is_empty() {
        return Err("the enclosure file holds no geometry".into());
    }
    Ok(Exchange { entities })
}

fn unit_scale_to_mm(exchange: &Exchange) -> f64 {
    let joined = exchange
        .entities
        .values()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_uppercase();
    if joined.contains(".INCH.") || joined.contains(".IN.") {
        25.4
    } else {
        1.0
    }
}

fn scan_exchange(exchange: &Exchange, scale: f64) -> Result<EnclosureScan, String> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut point_coords: BTreeMap<u32, [f64; 3]> = BTreeMap::new();
    let mut datums = Vec::new();
    for (id, body) in &exchange.entities {
        if let Some((name, coords)) = parse_cartesian_point(body) {
            let coords = scale_coords(coords, scale);
            point_coords.insert(*id, coords);
            include_point(&mut min, &mut max, coords);
            if Datum::name_valid(&name) {
                datums.push(ScannedDatum {
                    name,
                    position: coords,
                });
            }
        }
    }
    if !min[0].is_finite() {
        return Err("the enclosure file has no measurable points".into());
    }
    datums.sort_by(|a, b| a.name.cmp(&b.name));
    let mut planar_faces = Vec::new();
    let mut hidden_face_count = 0usize;
    for body in exchange.entities.values() {
        if !body.starts_with("ADVANCED_FACE") {
            continue;
        }
        let Some(surface_id) = advanced_face_surface(body) else {
            continue;
        };
        let Some(surface) = exchange.entities.get(&surface_id) else {
            hidden_face_count += 1;
            continue;
        };
        if surface.starts_with("PLANE(") {
            if planar_faces.len() < MAX_PLANAR_FACES {
                if let Some(polygon) = face_polygon(body, &exchange.entities, &point_coords) {
                    if polygon.len() >= 3 && polygon.len() <= MAX_FACE_VERTICES {
                        planar_faces.push(PlanarFace { polygon });
                    }
                }
            }
        } else {
            hidden_face_count += 1;
        }
    }
    let notice = (hidden_face_count > 0).then(|| {
        format!(
            "{hidden_face_count} faces not shown; add a glTF or OBJ export for the picture"
        )
    });
    Ok(EnclosureScan {
        bounds: ScanBounds { min, max },
        datums,
        planar_faces,
        hidden_face_count,
        notice,
    })
}

fn scale_coords(c: [f64; 3], scale: f64) -> [f64; 3] {
    [c[0] * scale, c[1] * scale, c[2] * scale]
}

fn include_point(min: &mut [f64; 3], max: &mut [f64; 3], p: [f64; 3]) {
    for axis in 0..3 {
        if p[axis] < min[axis] {
            min[axis] = p[axis];
        }
        if p[axis] > max[axis] {
            max[axis] = p[axis];
        }
    }
}

fn parse_cartesian_point(body: &str) -> Option<(String, [f64; 3])> {
    if !body.starts_with("CARTESIAN_POINT") {
        return None;
    }
    let open = body.find('(')?;
    let close = find_matching_paren(body, open)?;
    let inside = &body[open + 1..close];
    let (name, tuple) = split_step_args(inside);
    let name = parse_step_string(&name)?;
    let coords = parse_coord_tuple(tuple.trim())?;
    Some((name, coords))
}

fn parse_coord_tuple(text: &str) -> Option<[f64; 3]> {
    let open = text.find('(')?;
    let close = find_matching_paren(text, open)?;
    let inner = &text[open + 1..close];
    let mut parts = inner.split(',');
    let x = parts.next()?.trim().parse().ok()?;
    let y = parts.next()?.trim().parse().ok()?;
    let z = parts.next()?.trim().parse().ok()?;
    if x.is_finite() && y.is_finite() && z.is_finite() {
        Some([x, y, z])
    } else {
        None
    }
}

fn parse_step_string(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('\'') {
        let end = trimmed[1..].find('\'')?;
        Some(trimmed[1..1 + end].to_string())
    } else {
        Some(String::new())
    }
}

fn split_step_args(inside: &str) -> (String, String) {
    if inside.starts_with('\'') {
        if let Some(end) = inside[1..].find('\'') {
            let name = inside[..2 + end].to_string();
            let rest = inside[2 + end..].trim_start_matches(',').trim();
            return (name, rest.to_string());
        }
    }
    (String::new(), inside.to_string())
}

fn find_matching_paren(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if open >= bytes.len() || bytes[open] != b'(' {
        return None;
    }
    let mut depth = 0i32;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn ref_id(body: &str, marker: &str) -> Option<u32> {
    let index = body.find(marker)?;
    let tail = &body[index + marker.len()..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn advanced_face_surface(body: &str) -> Option<u32> {
    let open = body.find('(')?;
    let close = find_matching_paren(body, open)?;
    let parts = split_top_level_commas(&body[open + 1..close]);
    if parts.len() >= 3 {
        return parse_hash_id(parts[2].trim());
    }
    None
}

fn face_polygon(
    face_body: &str,
    entities: &BTreeMap<u32, String>,
    points: &BTreeMap<u32, [f64; 3]>,
) -> Option<Vec<[f64; 3]>> {
    let open = face_body.find('(')?;
    let close = find_matching_paren(face_body, open)?;
    let parts = split_top_level_commas(&face_body[open + 1..close]);
    if parts.len() < 2 {
        return None;
    }
    let bound_id = parse_hash_id(parts[1].trim())?;
    let mut current = bound_id;
    for _ in 0..8 {
        let body = entities.get(&current)?;
        if body.starts_with("FACE_OUTER_BOUND") {
            if let Some(loop_id) = ref_id(body, "#") {
                current = loop_id;
                continue;
            }
        }
        if body.starts_with("EDGE_LOOP") {
            return walk_edge_loop(body, entities, points);
        }
        if let Some(next) = ref_id(body, "#") {
            current = next;
        } else {
            break;
        }
    }
    None
}

fn split_top_level_commas(inner: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (index, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(inner[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(inner[start..].trim().to_string());
    parts
}

fn parse_hash_id(token: &str) -> Option<u32> {
    let mut trimmed = token.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        trimmed = &trimmed[1..trimmed.len() - 1];
    }
    let id_text = trimmed.strip_prefix('#')?;
    id_text
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .ok()
}

fn walk_edge_loop(
    body: &str,
    entities: &BTreeMap<u32, String>,
    points: &BTreeMap<u32, [f64; 3]>,
) -> Option<Vec<[f64; 3]>> {
    let open = body.find('(')?;
    let close = find_matching_paren(body, open)?;
    let inner = &body[open + 1..close];
    let mut ring = Vec::new();
    for token in inner.split(',') {
        let token = token.trim().trim_start_matches('#');
        let edge_id: u32 = token.parse().ok()?;
        if let Some(point) = edge_start_point(edge_id, entities, points) {
            push_vertex(&mut ring, point);
        }
    }
    if ring.len() >= 3 {
        Some(ring)
    } else {
        None
    }
}

fn push_vertex(ring: &mut Vec<[f64; 3]>, point: [f64; 3]) {
    if ring.last().is_some_and(|last| {
        (last[0] - point[0]).abs() < 1e-9
            && (last[1] - point[1]).abs() < 1e-9
            && (last[2] - point[2]).abs() < 1e-9
    }) {
        return;
    }
    ring.push(point);
}

fn edge_start_point(
    edge_id: u32,
    entities: &BTreeMap<u32, String>,
    points: &BTreeMap<u32, [f64; 3]>,
) -> Option<[f64; 3]> {
    let mut current = edge_id;
    for _ in 0..6 {
        let body = entities.get(&current)?;
        if body.starts_with("ORIENTED_EDGE") {
            if let Some(next) = ref_id(body, "#") {
                current = next;
                continue;
            }
        }
        if body.starts_with("EDGE_CURVE") {
            if let Some(vertex_id) = ref_id(body, "#") {
                return vertex_point(vertex_id, entities, points);
            }
        }
        if let Some(next) = ref_id(body, "#") {
            current = next;
        } else {
            break;
        }
    }
    None
}

fn vertex_point(
    vertex_id: u32,
    entities: &BTreeMap<u32, String>,
    points: &BTreeMap<u32, [f64; 3]>,
) -> Option<[f64; 3]> {
    let body = entities.get(&vertex_id)?;
    if body.starts_with("VERTEX_POINT") {
        if let Some(point_id) = ref_id(body, "#") {
            return points.get(&point_id).copied();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn read_fixture(name: &str) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read(path).expect("fixture")
    }

    fn inject_before_endsec(mut text: String, line: &str) -> Vec<u8> {
        if let Some(index) = text.find("ENDSEC;") {
            text.insert_str(index, line);
        } else {
            text.push_str(line);
        }
        text.into_bytes()
    }

    #[test]
    fn enclosure_scan_reads_bounds_and_named_datums() -> Result<(), String> {
        let text = String::from_utf8_lossy(&read_fixture("box.step")).into_owned();
        let bytes = inject_before_endsec(
            text,
            "#200=CARTESIAN_POINT('Mount_A',(5.0,5.0,0.0));\n",
        );
        let scan = read_enclosure_step(&bytes)?;
        assert!((scan.bounds.min[0] - 0.0).abs() < 1e-6);
        assert!((scan.bounds.max[0] - 10.0).abs() < 1e-6);
        assert!((scan.bounds.max[1] - 5.0).abs() < 1e-6);
        assert!((scan.bounds.max[2] - 2.0).abs() < 1e-6);
        assert_eq!(scan.datums.len(), 1);
        assert_eq!(scan.datums[0].name, "Mount_A");
        assert!(!scan.planar_faces.is_empty());
        Ok(())
    }

    #[test]
    fn enclosure_scan_counts_curved_faces_it_cannot_show() -> Result<(), String> {
        let text = String::from_utf8_lossy(&read_fixture("box.step")).into_owned();
        let bytes = inject_before_endsec(
            text,
            "#900=CYLINDRICAL_SURFACE('',#901,2.0);\n#902=ADVANCED_FACE('',(#903),#900,.T.);\n",
        );
        let scan = read_enclosure_step(&bytes)?;
        assert_eq!(scan.hidden_face_count, 1);
        assert_eq!(
            scan.notice.as_deref(),
            Some("1 faces not shown; add a glTF or OBJ export for the picture")
        );
        Ok(())
    }

    #[test]
    fn enclosure_scan_refuses_oversized_input() {
        let huge = b"ISO-10303-21;\nDATA;\n".to_vec()
            + &(b"#1=CARTESIAN_POINT('',(0,0,0));\n".repeat(500_000));
        assert!(read_enclosure_step(&huge).is_err());
    }
}
