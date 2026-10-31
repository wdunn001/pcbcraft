//! Bounded STEP exchange parsing and merging for board export. Reads vendor
//! bodies as text, renumbers entity ids, keeps each body's unit context, and
//! writes assembly occurrences. Pure Rust; no file I/O here.

use crate::apply_matrix;
use std::collections::BTreeMap;

/// Largest vendor STEP body accepted for merge, in bytes.
pub const MAX_EXCHANGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_EXCHANGE_ENTITIES: usize = 50_000;
const MAX_EXCHANGE_LINES: usize = 400_000;

/// One parsed DATA section: entity id and the text after `#id =`, without the
/// trailing semicolon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub entities: Vec<(u32, String)>,
}

/// Links created when a vendor body is appended to a board document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorLinks {
    pub product: u32,
    pub product_definition: u32,
    pub product_definition_shape: u32,
    pub shape_representation: u32,
}

/// Parse the DATA section of an AP214 exchange file.
pub fn parse_exchange(text: &str) -> Result<Exchange, String> {
    if text.len() > MAX_EXCHANGE_BYTES {
        return Err("the model file is over the size limit for a board export".into());
    }
    let upper = text.to_ascii_uppercase();
    let data_start = upper
        .find("DATA;")
        .ok_or("the model file has no STEP data section")?;
    let data = &text[data_start + 5..];
    let end = data
        .to_ascii_uppercase()
        .find("ENDSEC;")
        .unwrap_or(data.len());
    let data = &data[..end];
    let mut entities = Vec::new();
    let mut lines = 0usize;
    for line in data.lines() {
        lines += 1;
        if lines > MAX_EXCHANGE_LINES {
            return Err("the model file has too many lines for a board export".into());
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
            .ok_or("the model file has a damaged entity record")?;
        let id: u32 = id_text
            .trim()
            .parse()
            .map_err(|_| "the model file has a damaged entity id")?;
        if id == 0 {
            return Err("the model file has a damaged entity id".into());
        }
        let body = body.trim().trim_end_matches(';').trim();
        if body.is_empty() {
            return Err("the model file has an empty entity record".into());
        }
        entities.push((id, body.to_string()));
        if entities.len() > MAX_EXCHANGE_ENTITIES {
            return Err("the model file has too many entities for a board export".into());
        }
    }
    if entities.is_empty() {
        return Err("the model file holds no entities".into());
    }
    entities.sort_by_key(|(id, _)| *id);
    Ok(Exchange { entities })
}

/// Shift every entity id and in-body `#id` reference by `offset`.
pub fn renumber(exchange: Exchange, offset: u32) -> Exchange {
    if offset == 0 {
        return exchange;
    }
    let map: BTreeMap<u32, u32> = exchange
        .entities
        .iter()
        .map(|(id, _)| (*id, id.saturating_add(offset)))
        .collect();
    Exchange {
        entities: exchange
            .entities
            .into_iter()
            .map(|(id, body)| (id + offset, rewrite_refs(&body, &map)))
            .collect(),
    }
}

fn rewrite_refs(body: &str, map: &BTreeMap<u32, u32>) -> String {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            let mut end = index + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > index + 1 {
                let id: u32 = body[index + 1..end].parse().unwrap_or(0);
                if let Some(new_id) = map.get(&id) {
                    out.push('#');
                    out.push_str(&new_id.to_string());
                    index = end;
                    continue;
                }
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

/// Scale factor from the body's length unit to millimetres.
pub fn unit_scale_to_mm(exchange: &Exchange) -> f64 {
    let joined = exchange
        .entities
        .iter()
        .map(|(_, b)| b.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_uppercase();
    if joined.contains(".INCH.") || joined.contains(".IN.") {
        return 25.4;
    }
    1.0
}

/// Multiply every cartesian point coordinate by `scale`.
pub fn scale_cartesian_points(exchange: &mut Exchange, scale: f64) {
    if (scale - 1.0).abs() < 1e-12 {
        return;
    }
    for (_, body) in &mut exchange.entities {
        *body = scale_points_in_body(body, scale);
    }
}

fn scale_points_in_body(body: &str, scale: f64) -> String {
    let marker = "CARTESIAN_POINT(";
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find(marker) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(open) = tail.find('(') else {
            out.push_str(tail);
            return out;
        };
        let Some(close) = find_matching_paren(tail, open) else {
            out.push_str(tail);
            return out;
        };
        let inside = &tail[open + 1..close];
        let scaled = scale_point_tuple(inside, scale);
        out.push_str(marker);
        out.push('(');
        out.push_str(&scaled);
        out.push(')');
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    out
}

fn find_matching_paren(text: &str, open_index: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in text[open_index..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_index + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn scale_point_tuple(inside: &str, scale: f64) -> String {
    let Some(comma) = inside.find(',') else {
        return inside.to_string();
    };
    let name = &inside[..comma];
    let coords = inside[comma + 1..].trim();
    let Some(open) = coords.find('(') else {
        return inside.to_string();
    };
    let Some(close) = coords.rfind(')') else {
        return inside.to_string();
    };
    let triple = &coords[open + 1..close];
    let parts: Vec<&str> = triple.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return inside.to_string();
    }
    let scaled: Vec<String> = parts
        .iter()
        .map(|p| {
            p.parse::<f64>()
                .map(|v| format!("{:.4}", v * scale))
                .unwrap_or_else(|_| (*p).to_string())
        })
        .collect();
    format!("{name},({},{},{})", scaled[0], scaled[1], scaled[2])
}

fn entity_named<'a>(exchange: &'a Exchange, prefix: &str) -> Option<(u32, &'a str)> {
    exchange
        .entities
        .iter()
        .find(|(_, body)| body.starts_with(prefix))
        .map(|(id, body)| (*id, body.as_str()))
}

fn solid_id(exchange: &Exchange) -> Option<u32> {
    entity_named(exchange, "MANIFOLD_SOLID_BREP(")
        .map(|(id, _)| id)
        .or_else(|| entity_named(exchange, "CLOSED_SHELL(").map(|(id, _)| id))
}

/// Append a vendor body to a board [`StepWriter`] and return product links.
pub fn append_body(
    doc: &mut impl StepWriter,
    exchange: Exchange,
    part_name: &str,
) -> Result<VendorLinks, String> {
    let offset = doc.next_id().saturating_sub(1);
    let mut renumbered = renumber(exchange, offset);
    let scale = unit_scale_to_mm(&renumbered);
    scale_cartesian_points(&mut renumbered, scale);

    let existing_product = entity_named(&renumbered, "PRODUCT(").map(|(id, _)| id);
    let existing_pd = entity_named(&renumbered, "PRODUCT_DEFINITION(").map(|(id, _)| id);
    let existing_pds = entity_named(&renumbered, "PRODUCT_DEFINITION_SHAPE(").map(|(id, _)| id);
    let existing_shape = entity_named(&renumbered, "SHAPE_REPRESENTATION(").map(|(id, _)| id);
    let solid = solid_id(&renumbered);

    for (id, body) in &renumbered.entities {
        doc.push_entity(*id, body)?;
    }

    let safe = part_name.replace('\'', "''");
    let product = if let Some(id) = existing_product {
        id
    } else {
        doc.add(&format!(
            "PRODUCT('{safe}','{safe}','',('Mechanical Design'))"
        ))?
    };
    let product_definition = if let Some(id) = existing_pd {
        id
    } else {
        let formation = doc.add(&format!(
            "PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE('','',#{product},.NOT_KNOWN.)"
        ))?;
        let context = doc.add(&format!("PRODUCT_CONTEXT('',#{product},'mechanical')"))?;
        doc.add(&format!(
            "PRODUCT_DEFINITION('design','',#{formation},#{context})"
        ))?
    };
    let product_definition_shape = if let Some(id) = existing_pds {
        id
    } else {
        doc.add(&format!(
            "PRODUCT_DEFINITION_SHAPE('','',#{product_definition})"
        ))?
    };

    let shape_representation = if let Some(id) = existing_shape {
        id
    } else {
        let solid = solid.ok_or("the model file holds no solid the export can place")?;
        let rep = doc.add(&format!("SHAPE_REPRESENTATION('{safe}',(#{solid}),#7)"))?;
        doc.add(&format!(
            "SHAPE_DEFINITION_REPRESENTATION(#{product_definition_shape},#{rep})"
        ))?;
        rep
    };

    Ok(VendorLinks {
        product,
        product_definition,
        product_definition_shape,
        shape_representation,
    })
}

/// Write one placed-part occurrence under the board assembly.
pub fn write_occurrence(
    doc: &mut impl StepWriter,
    board_product_definition: u32,
    links: &VendorLinks,
    matrix: &[f64; 16],
    part_name: &str,
    board_axis: u32,
    board_reference: u32,
) -> Result<(), String> {
    let safe = part_name.replace('\'', "''");
    let local = placement_from_matrix(doc, matrix)?;
    let global_origin = doc.point((0.0, 0.0, 0.0))?;
    let global = doc.add(&format!(
        "AXIS2_PLACEMENT_3D('',#{global_origin},#{board_axis},#{board_reference})"
    ))?;
    let transform = doc.add(&format!(
        "ITEM_DEFINED_TRANSFORMATION('{safe}',#{local},#{global})"
    ))?;
    let relation = doc.add(&format!(
        "(REPRESENTATION_RELATIONSHIP('{safe}','',#{}) REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{transform}) SHAPE_REPRESENTATION_RELATIONSHIP())",
        links.shape_representation
    ))?;
    let context = doc.add(&format!(
        "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{relation},#{},#{transform})",
        links.product_definition_shape
    ))?;
    doc.add(&format!(
        "NEXT_ASSEMBLY_USAGE_OCCURRENCE('{safe}',#{board_product_definition},#{},'',#{context})",
        links.product_definition
    ))?;
    Ok(())
}

fn placement_from_matrix(doc: &mut impl StepWriter, matrix: &[f64; 16]) -> Result<u32, String> {
    let origin = apply_matrix(matrix, [0.0, 0.0, 0.0]);
    let x = apply_matrix(matrix, [1.0, 0.0, 0.0]);
    let z = apply_matrix(matrix, [0.0, 0.0, 1.0]);
    let x_dir = normalize(sub(x, origin));
    let z_dir = normalize(sub(z, origin));
    let point = doc.point((origin[0], origin[1], origin[2]))?;
    let z_id = doc.direction(z_dir)?;
    let x_id = doc.direction(x_dir)?;
    doc.add(&format!("AXIS2_PLACEMENT_3D('',#{point},#{z_id},#{x_id})"))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn normalize(v: [f64; 3]) -> (f64, f64, f64) {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 {
        (1.0, 0.0, 0.0)
    } else {
        (v[0] / len, v[1] / len, v[2] / len)
    }
}

/// Minimal STEP writer surface used by [`append_body`] and [`write_occurrence`].
///
/// # Implementations
/// - crates/io/src/lib.rs `impl StepWriter for StepDoc`.
pub trait StepWriter {
    fn next_id(&self) -> u32;
    fn add(&mut self, body: &str) -> Result<u32, String>;
    fn push_entity(&mut self, id: u32, body: &str) -> Result<(), String>;
    fn point(&mut self, c: (f64, f64, f64)) -> Result<u32, String>;
    fn direction(&mut self, d: (f64, f64, f64)) -> Result<u32, String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn read_fixture(name: &str) -> Result<String, String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(path).map_err(|_| format!("fixture {name}"))
    }

    #[test]
    fn step_merge_renumbers_without_clashing_ids() -> Result<(), String> {
        let exchange = parse_exchange(&read_fixture("box.step")?)?;
        let high = exchange
            .entities
            .iter()
            .map(|(id, _)| *id)
            .max()
            .unwrap_or(0);
        let shifted = renumber(exchange, 1000);
        assert!(shifted.entities.iter().all(|(id, _)| *id > 1000));
        assert!(shifted.entities.iter().any(|(id, _)| *id == high + 1000));
        let body = shifted
            .entities
            .iter()
            .find(|(_, b)| b.contains("CLOSED_SHELL"))
            .map(|(_, b)| b.clone())
            .ok_or("shell")?;
        assert!(body.contains("#"), "references stay valid");
        Ok(())
    }

    #[test]
    fn step_merge_keeps_each_bodys_own_unit() -> Result<(), String> {
        let mm = parse_exchange(&read_fixture("box.step")?)?;
        assert!((unit_scale_to_mm(&mm) - 1.0).abs() < 1e-9);
        let inch = parse_exchange(&read_fixture("inch_box.step")?)?;
        assert!((unit_scale_to_mm(&inch) - 25.4).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn step_merge_refuses_oversized_files() {
        let huge = "ISO-10303-21;\nDATA;\n".to_string()
            + &"#1=CARTESIAN_POINT('',(0,0,0));\n".repeat(500_000);
        assert!(parse_exchange(&huge).is_err());
    }
}
