//! Compact on-disk and in-project encoding of a loaded model (millimetres, Z up).
use crate::model_load::{LoadedModel, ModelError};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 4] = b"PCBM";
pub const COMPACT_VERSION: u32 = 1;

fn bad(why: &str) -> ModelError {
    ModelError(why.to_string())
}

fn decode_hex_digest(text: &str) -> Result<[u8; 32], ModelError> {
    if text.len() != 64 {
        return Err(bad("the model digest must be 64 lowercase hex digits"));
    }
    let mut out = [0u8; 32];
    let bytes = text.as_bytes();
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = from_hex(bytes[2 * i]).ok_or_else(|| bad("the model digest is not valid hex"))?;
        let lo =
            from_hex(bytes[2 * i + 1]).ok_or_else(|| bad("the model digest is not valid hex"))?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// SHA-256 hex digest of the source file bytes.
pub fn source_digest(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Cache file name piece from loader version and STEP tolerance in millimetres.
pub fn cache_key_label(loader_version: u32, step_tolerance_mm: f64) -> String {
    let tol = (step_tolerance_mm * 1_000_000.0).round() as i64;
    format!("v{loader_version}-t{tol}")
}

/// Disk cache entry name: content hash, loader version and tessellation tolerance.
pub fn cache_entry_name(digest: &str, loader_version: u32, step_tolerance_mm: f64) -> String {
    format!(
        "{}-{}.pcbm",
        digest,
        cache_key_label(loader_version, step_tolerance_mm)
    )
}

/// Encode a loaded model for the disk cache or project embedding.
pub fn encode_compact(
    model: &LoadedModel,
    loader_version: u32,
    step_tolerance_mm: f64,
    source_digest: &str,
) -> Result<Vec<u8>, ModelError> {
    if source_digest.len() != 64
        || !source_digest
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return Err(bad("the model digest must be 64 lowercase hex digits"));
    }
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&COMPACT_VERSION.to_le_bytes());
    out.extend_from_slice(&loader_version.to_le_bytes());
    out.extend_from_slice(&step_tolerance_mm.to_le_bytes());
    out.extend_from_slice(&decode_hex_digest(source_digest)?);
    let n_pos = u32::try_from(model.positions.len())
        .map_err(|_| bad("the model has too many vertices to store"))?;
    let n_idx = u32::try_from(model.indices.len())
        .map_err(|_| bad("the model has too many indices to store"))?;
    out.extend_from_slice(&n_pos.to_le_bytes());
    out.extend_from_slice(&n_idx.to_le_bytes());
    let has_color = u8::from(model.color.is_some());
    out.push(has_color);
    if let Some(c) = model.color {
        for v in c {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for p in &model.positions {
        for v in p {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for i in &model.indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    Ok(out)
}

/// Decode a compact model written by [`encode_compact`].
pub fn decode_compact(bytes: &[u8]) -> Result<LoadedModel, ModelError> {
    let mut off = 0usize;
    let take = |n: usize, b: &[u8]| -> Result<&[u8], ModelError> {
        if off + n > b.len() {
            return Err(bad("the stored model file is truncated"));
        }
        let s = &b[off..off + n];
        off += n;
        Ok(s)
    };
    let magic = take(4, bytes)?;
    if magic != MAGIC {
        return Err(bad("the stored model file is not a PCBM cache entry"));
    }
    let version = u32::from_le_bytes(take(4, bytes)?.try_into().unwrap());
    if version != COMPACT_VERSION {
        return Err(bad("the stored model file uses an unsupported version"));
    }
    let _loader = u32::from_le_bytes(take(4, bytes)?.try_into().unwrap());
    let _tol = f64::from_le_bytes(take(8, bytes)?.try_into().unwrap());
    let _digest = take(32, bytes)?;
    let n_pos = u32::from_le_bytes(take(4, bytes)?.try_into().unwrap()) as usize;
    let n_idx = u32::from_le_bytes(take(4, bytes)?.try_into().unwrap()) as usize;
    let has_color = *take(1, bytes)?.first().unwrap();
    let color = if has_color != 0 {
        let mut c = [0f32; 3];
        for slot in &mut c {
            *slot = f32::from_le_bytes(take(4, bytes)?.try_into().unwrap());
        }
        Some(c)
    } else {
        None
    };
    let mut positions = Vec::with_capacity(n_pos);
    for _ in 0..n_pos {
        let mut p = [0f64; 3];
        for v in &mut p {
            *v = f64::from_le_bytes(take(8, bytes)?.try_into().unwrap());
        }
        positions.push(p);
    }
    let mut indices = Vec::with_capacity(n_idx);
    for _ in 0..n_idx {
        indices.push(u32::from_le_bytes(take(4, bytes)?.try_into().unwrap()));
    }
    if off != bytes.len() {
        return Err(bad("the stored model file has trailing bytes"));
    }
    Ok(LoadedModel {
        positions,
        indices,
        color,
    })
}

/// Base64 form stored on a footprint for travel with the project file.
pub fn encode_compact_base64(
    model: &LoadedModel,
    loader_version: u32,
    step_tolerance_mm: f64,
    source_digest: &str,
) -> Result<String, ModelError> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(encode_compact(
        model,
        loader_version,
        step_tolerance_mm,
        source_digest,
    )?))
}

/// Read a footprint-embedded model.
pub fn decode_compact_base64(text: &str) -> Result<LoadedModel, ModelError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|_| bad("the embedded model is not valid base64"))?;
    decode_compact(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_round_trip_preserves_geometry() -> Result<(), String> {
        let model = LoadedModel {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 2.0, 3.0]],
            indices: vec![0, 1, 2],
            color: Some([0.5, 0.25, 1.0]),
        };
        let digest = source_digest(b"source");
        let bytes = encode_compact(&model, 1, 0.05, &digest).map_err(|e| e.0)?;
        let back = decode_compact(&bytes).map_err(|e| e.0)?;
        assert_eq!(back, model);
        let b64 = encode_compact_base64(&model, 1, 0.05, &digest).map_err(|e| e.0)?;
        assert_eq!(decode_compact_base64(&b64).map_err(|e| e.0)?, model);
        Ok(())
    }
}
