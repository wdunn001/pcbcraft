//! Summaries used by interchange tests: part names, rigid placements and axis-aligned bounds.
use crate::solidcraft_format::SolidcraftDesign;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartSnapshot {
    pub name: String,
    pub transform: [[f64; 4]; 4],
    pub bounds: Bounds,
}

pub fn design_as_value(design: &SolidcraftDesign) -> Result<Value, String> {
    serde_json::to_value(design).map_err(|e| e.to_string())
}

fn column_major(m: &Value) -> Result<[[f64; 4]; 4], String> {
    let mut out = [[0.0; 4]; 4];
    for (c, column) in out.iter_mut().enumerate() {
        for (r, cell) in column.iter_mut().enumerate() {
            *cell = m
                .get(c)
                .and_then(|col| col.get(r))
                .and_then(Value::as_f64)
                .ok_or_else(|| "bad transform".to_string())?;
        }
    }
    Ok(out)
}

fn apply(transform: &[[f64; 4]; 4], point: [f64; 3]) -> [f64; 3] {
    let mut out = [0.0; 3];
    for (r, cell) in out.iter_mut().enumerate() {
        *cell = transform[0][r] * point[0]
            + transform[1][r] * point[1]
            + transform[2][r] * point[2]
            + transform[3][r];
    }
    out
}

fn mesh_bounds(doc: &Value, body: &str) -> Result<Bounds, String> {
    let mesh = doc
        .get("features")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| f.get("meshes").and_then(Value::as_array))
        .flatten()
        .find(|m| m.get("name").and_then(Value::as_str) == Some(body))
        .ok_or_else(|| format!("no body {body}"))?;
    let positions: Vec<f64> = mesh
        .get("positions")
        .and_then(Value::as_array)
        .ok_or_else(|| "positions must be a list".to_string())?
        .iter()
        .map(|v| v.as_f64().ok_or_else(|| "bad coordinate".to_string()))
        .collect::<Result<_, _>>()?;
    if positions.len() < 3 {
        return Err(format!("body {body} has no vertices"));
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for chunk in positions.chunks(3) {
        for axis in 0..3 {
            min[axis] = min[axis].min(chunk[axis]);
            max[axis] = max[axis].max(chunk[axis]);
        }
    }
    Ok(Bounds { min, max })
}

fn transform_bounds(transform: &[[f64; 4]; 4], local: &Bounds) -> Bounds {
    let corners = [
        [local.min[0], local.min[1], local.min[2]],
        [local.min[0], local.min[1], local.max[2]],
        [local.min[0], local.max[1], local.min[2]],
        [local.min[0], local.max[1], local.max[2]],
        [local.max[0], local.min[1], local.min[2]],
        [local.max[0], local.min[1], local.max[2]],
        [local.max[0], local.max[1], local.min[2]],
        [local.max[0], local.max[1], local.max[2]],
    ];
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for corner in corners {
        let world = apply(transform, corner);
        for axis in 0..3 {
            min[axis] = min[axis].min(world[axis]);
            max[axis] = max[axis].max(world[axis]);
        }
    }
    Bounds { min, max }
}

/// Every occurrence with a mesh body: name, transform and world-space bounds.
pub fn part_snapshots(design: &SolidcraftDesign) -> Result<Vec<PartSnapshot>, String> {
    let doc = design_as_value(design)?;
    let occurrences = doc
        .get("occurrences")
        .and_then(Value::as_array)
        .ok_or_else(|| "occurrences must be a list".to_string())?;
    let mut out = Vec::new();
    for occurrence in occurrences {
        let name = occurrence
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "occurrence name is missing".to_string())?;
        let transform = column_major(
            occurrence
                .get("transform")
                .ok_or_else(|| "occurrence transform is missing".to_string())?,
        )?;
        let local = mesh_bounds(&doc, name)?;
        out.push(PartSnapshot {
            name: name.to_string(),
            transform,
            bounds: transform_bounds(&transform, &local),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Count components and mesh bodies for SolidCraft reader check scripts.
pub fn solidcraft_counts(design: &SolidcraftDesign) -> Result<(usize, usize, usize), String> {
    let doc = design_as_value(design)?;
    let components = doc
        .get("components")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let occurrences = doc
        .get("occurrences")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let mut bodies = BTreeMap::new();
    if let Some(features) = doc.get("features").and_then(Value::as_array) {
        for feature in features {
            if let Some(meshes) = feature.get("meshes").and_then(Value::as_array) {
                for mesh in meshes {
                    if let Some(name) = mesh.get("name").and_then(Value::as_str) {
                        bodies.insert(name.to_string(), ());
                    }
                }
            }
        }
    }
    Ok((components, occurrences, bodies.len()))
}

pub fn bounds_close(a: &Bounds, b: &Bounds, eps: f64) -> bool {
    (0..3).all(|axis| (a.min[axis] - b.min[axis]).abs() <= eps && (a.max[axis] - b.max[axis]).abs() <= eps)
}

pub fn transforms_close(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4], eps: f64) -> bool {
    (0..4).all(|c| (0..4).all(|r| (a[c][r] - b[c][r]).abs() <= eps))
}

/// Compare two designs for round-trip equality (ignores key order).
pub fn designs_equal(a: &SolidcraftDesign, b: &SolidcraftDesign) -> bool {
    match (design_as_value(a), design_as_value(b)) {
        (Ok(av), Ok(bv)) => normalize_value(&av) == normalize_value(&bv),
        _ => false,
    }
}

fn normalize_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for key in keys {
                out.insert(
                    key.clone(),
                    normalize_value(map.get(&key).unwrap_or(&Value::Null)),
                );
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(normalize_value).collect()),
        other => other.clone(),
    }
}
