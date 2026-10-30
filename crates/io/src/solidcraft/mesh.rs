//! SCMESH01 layout for SolidCraft container mesh blobs.
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"SCMESH01";

/// Encode one mesh body for a container blob.
pub fn encode(positions: &[f64], triangles: &[u32]) -> Result<Vec<u8>, String> {
    if !positions.len().is_multiple_of(3) || !triangles.len().is_multiple_of(3) {
        return Err("mesh arrays have the wrong length".into());
    }
    if positions.iter().any(|v| !v.is_finite()) {
        return Err("mesh coordinates must be finite numbers".into());
    }
    let size = 24usize
        .checked_add(positions.len().saturating_mul(8))
        .and_then(|n| n.checked_add(triangles.len().saturating_mul(4)))
        .ok_or_else(|| "mesh is too large".to_string())?;
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(positions.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(triangles.len() as u64).to_le_bytes());
    for value in positions {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in triangles {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

/// Decode a container mesh blob into flat arrays.
pub fn decode(bytes: &[u8]) -> Result<(Vec<f64>, Vec<u32>), String> {
    if bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err("unsupported mesh blob version".into());
    }
    let positions = usize::try_from(u64::from_le_bytes(
        bytes
            .get(8..16)
            .ok_or("truncated mesh blob")?
            .try_into()
            .map_err(|_| "truncated mesh blob")?,
    ))
    .map_err(|_| "mesh blob counts are out of range".to_string())?;
    let triangles = usize::try_from(u64::from_le_bytes(
        bytes
            .get(16..24)
            .ok_or("truncated mesh blob")?
            .try_into()
            .map_err(|_| "truncated mesh blob")?,
    ))
    .map_err(|_| "mesh blob counts are out of range".to_string())?;
    if !positions.is_multiple_of(3) || !triangles.is_multiple_of(3) {
        return Err("invalid mesh blob lengths".into());
    }
    let middle = positions
        .checked_mul(8)
        .and_then(|n| n.checked_add(24))
        .ok_or_else(|| "mesh size overflow".to_string())?;
    let total = triangles
        .checked_mul(4)
        .and_then(|n| n.checked_add(middle))
        .ok_or_else(|| "mesh size overflow".to_string())?;
    if total != bytes.len() {
        return Err("invalid mesh blob lengths".into());
    }
    let pos_bytes = bytes
        .get(24..middle)
        .ok_or_else(|| "truncated mesh".to_string())?;
    let tri_bytes = bytes
        .get(middle..)
        .ok_or_else(|| "truncated mesh".to_string())?;
    let flat: Vec<f64> = pos_bytes
        .chunks_exact(8)
        .map(|chunk| {
            let array: [u8; 8] = chunk.try_into().map_err(|_| "truncated mesh")?;
            Ok(f64::from_le_bytes(array))
        })
        .collect::<Result<_, String>>()?;
    let indices: Vec<u32> = tri_bytes
        .chunks_exact(4)
        .map(|chunk| {
            let array: [u8; 4] = chunk.try_into().map_err(|_| "truncated mesh")?;
            Ok(u32::from_le_bytes(array))
        })
        .collect::<Result<_, String>>()?;
    Ok((flat, indices))
}

/// SHA-256 hex digest of blob bytes, the name under `blobs/` in a container.
pub fn blob_name(bytes: &[u8], extension: &str) -> String {
    format!("blobs/{:x}.{extension}", Sha256::digest(bytes))
}
