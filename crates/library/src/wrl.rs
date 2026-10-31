use crate::part_types::TriangleMesh;

const WRL_INCH_TO_MM: f64 = 2.54;

/// Read a minimal VRML 2.0 subset and build a triangle mesh.
/// Coordinates in open-library WRL files are typically in 0.1 inch units.
pub fn read_wrl_mesh(text: &str, scale: [f64; 3]) -> Result<TriangleMesh, String> {
    let coords = parse_coordinate_block(text)?;
    if coords.len() < 9 {
        return Err("WRL coordinate block too small".into());
    }
    let inch_scale = WRL_INCH_TO_MM * 0.1;
    let vertices: Vec<[f64; 3]> = coords
        .chunks_exact(3)
        .map(|c| {
            [
                c[0] * inch_scale * scale[0],
                c[1] * inch_scale * scale[1],
                c[2] * inch_scale * scale[2],
            ]
        })
        .collect();
    let indices = parse_index_block(text).unwrap_or_else(|| {
        (0..vertices.len() as u32)
            .collect::<Vec<_>>()
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2]])
            .collect()
    });
    let mesh = TriangleMesh {
        vertices,
        triangles: indices,
    };
    mesh.validate().map_err(|e| e.to_string())?;
    Ok(mesh)
}

pub fn read_wrl_file(path: &str, scale: [f64; 3]) -> Result<TriangleMesh, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    if text.len() > 8 * 1024 * 1024 {
        return Err("WRL file exceeds 8 MiB".into());
    }
    read_wrl_mesh(&text, scale)
}

fn parse_coordinate_block(text: &str) -> Result<Vec<f64>, String> {
    let marker = "point [";
    let start = text
        .find(marker)
        .ok_or_else(|| "WRL missing point coordinate block".to_string())?;
    let rest = &text[start + marker.len()..];
    let end = rest.find(']').ok_or_else(|| "WRL coordinate block unclosed".to_string())?;
    parse_floats(rest[..end].as_ref())
}

fn parse_index_block(text: &str) -> Option<Vec<[u32; 3]>> {
    let marker = "coordIndex [";
    let start = text.find(marker)?;
    let rest = &text[start + marker.len()..];
    let end = rest.find(']')?;
    let nums = parse_floats(rest[..end].as_ref()).ok()?;
    let mut tris = Vec::new();
    let mut chunk = Vec::new();
    for n in nums {
        if (n - (-1.0)).abs() < 1e-6 {
            if chunk.len() >= 3 {
                tris.push([chunk[0], chunk[1], chunk[2]]);
            }
            chunk.clear();
        } else if n >= 0.0 {
            chunk.push(n as u32);
        }
    }
    if tris.is_empty() { None } else { Some(tris) }
}

fn parse_floats(body: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for token in body.split_whitespace() {
        if let Ok(v) = token.parse::<f64>() {
            if v.is_finite() {
                out.push(v);
            }
        }
    }
    if out.is_empty() {
        Err("WRL numeric block empty".into())
    } else {
        Ok(out)
    }
}
