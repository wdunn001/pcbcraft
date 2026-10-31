//! Extract the first STEP file from a vendor parts archive dropped on import.
use crate::model_load::{ModelError, MAX_MODEL_BYTES};

const MAX_ZIP_BYTES: usize = MAX_MODEL_BYTES;

fn bad(why: &str) -> ModelError {
    ModelError(why.to_string())
}

fn safe_zip_name(name: &[u8]) -> Result<String, ModelError> {
    if name.is_empty() || name.len() > 255 {
        return Err(bad("the archive entry name is invalid"));
    }
    if name.starts_with(b"/") || name.windows(2).any(|w| w == b"..") {
        return Err(bad("the archive entry path is not allowed"));
    }
    let text = std::str::from_utf8(name).map_err(|_| bad("the archive entry name is not UTF-8"))?;
    if text.contains('\\') || text.contains(':') {
        return Err(bad("the archive entry path is not allowed"));
    }
    Ok(text.to_owned())
}

fn is_step_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".step") || lower.ends_with(".stp")
}

/// Read a zip archive and return the first STEP entry name and bytes, in path order.
pub fn first_step_from_vendor_zip(bytes: &[u8]) -> Result<(String, Vec<u8>), ModelError> {
    if bytes.len() > MAX_ZIP_BYTES {
        return Err(bad("the vendor archive is over the size limit"));
    }
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| bad("the vendor archive is not a zip file"))?;
    let read_u16 = |at: usize| -> u16 { u16::from_le_bytes([bytes[at], bytes[at + 1]]) };
    let read_u32 = |at: usize| -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let entries = read_u16(eocd + 10) as usize;
    let mut at = read_u32(eocd + 16) as usize;
    let mut steps: Vec<(String, Vec<u8>)> = Vec::new();
    let mut total: u64 = 0;
    for _ in 0..entries {
        if !bytes[at..].starts_with(b"PK\x01\x02") || at + 46 > bytes.len() {
            return Err(bad("the vendor archive directory is corrupt"));
        }
        let size = read_u32(at + 20) as usize;
        let name_len = read_u16(at + 28) as usize;
        let extra_len = read_u16(at + 30) as usize;
        let comment_len = read_u16(at + 32) as usize;
        let local_at = read_u32(at + 42) as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err(bad("the vendor archive directory is corrupt"));
        }
        let name = safe_zip_name(&bytes[at + 46..at + 46 + name_len])?;
        if is_step_name(&name) {
            if local_at + 30 > bytes.len() || !bytes[local_at..].starts_with(b"PK\x03\x04") {
                return Err(bad("the vendor archive entry header is corrupt"));
            }
            let local_name = read_u16(local_at + 26) as usize;
            let local_extra = read_u16(local_at + 28) as usize;
            if local_at + 30 + local_name > bytes.len() {
                return Err(bad("the vendor archive entry header is corrupt"));
            }
            let data_at = local_at + 30 + local_name + local_extra;
            if data_at + size > bytes.len() {
                return Err(bad("the vendor archive entry runs past the end of the file"));
            }
            total += size as u64;
            if total > MAX_ZIP_BYTES as u64 {
                return Err(bad("the vendor archive contents exceed the size budget"));
            }
            steps.push((name, bytes[data_at..data_at + size].to_vec()));
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    steps.sort_by(|a, b| a.0.cmp(&b.0));
    steps
        .into_iter()
        .next()
        .ok_or_else(|| bad("the vendor archive holds no STEP file"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip_entries;

    #[test]
    fn vendor_zip_returns_the_first_step_in_name_order() -> Result<(), String> {
        let zip = zip_entries(&[
            ("readme.txt".into(), b"hi".to_vec()),
            ("parts/B.stp".into(), b"STEP-B".to_vec()),
            ("parts/A.step".into(), b"STEP-A".to_vec()),
        ])
        .map_err(|e| e.to_string())?;
        let (name, body) = first_step_from_vendor_zip(&zip).map_err(|e| e.0)?;
        assert_eq!(name, "parts/A.step");
        assert_eq!(body, b"STEP-A");
        Ok(())
    }
}
