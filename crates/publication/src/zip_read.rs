//! Read stored ZIP archives produced by the IO crate without decompressing.

use std::collections::BTreeMap;

/// Maximum uncompressed bytes summed across all entries in one bundle.
pub const MAX_BUNDLE_UNCOMPRESSED: u64 = 256 * 1024 * 1024;
/// Maximum number of entries in one bundle archive.
pub const MAX_BUNDLE_ENTRIES: usize = 512;
/// Maximum uncompressed size of one entry.
pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// One entry read from a stored ZIP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
}

/// Read a stored ZIP written by `pcbcraft_io::zip_entries`.
pub fn read_stored_zip(bytes: &[u8]) -> Result<Vec<ZipEntry>, String> {
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| "not a stored archive: no directory".to_string())?;
    let read_u16 = |at: usize| -> Result<u16, String> {
        if at + 2 > bytes.len() {
            return Err("corrupt archive directory".into());
        }
        Ok(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
    };
    let read_u32 = |at: usize| -> Result<u32, String> {
        if at + 4 > bytes.len() {
            return Err("corrupt archive directory".into());
        }
        Ok(u32::from_le_bytes([
            bytes[at],
            bytes[at + 1],
            bytes[at + 2],
            bytes[at + 3],
        ]))
    };
    let entries = read_u16(eocd + 10)? as usize;
    if entries > MAX_BUNDLE_ENTRIES {
        return Err("the archive lists too many entries".into());
    }
    let mut at = read_u32(eocd + 16)? as usize;
    let mut out: Vec<ZipEntry> = Vec::new();
    let mut total: u64 = 0;
    for _ in 0..entries {
        if !bytes[at..].starts_with(b"PK\x01\x02") || at + 46 > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let size = read_u32(at + 20)? as u64;
        let name_len = read_u16(at + 28)? as usize;
        let extra_len = read_u16(at + 30)? as usize;
        let comment_len = read_u16(at + 32)? as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let name_bytes = &bytes[at + 46..at + 46 + name_len];
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| "archive entry name is not UTF-8".to_string())?
            .to_string();
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(format!("{name} is not a flat bundle path"));
        }
        let local_at = read_u32(at + 42)? as usize;
        if local_at + 30 > bytes.len() || !bytes[local_at..].starts_with(b"PK\x03\x04") {
            return Err("corrupt archive entry header".into());
        }
        let local_name = read_u16(local_at + 26)? as usize;
        let local_extra = read_u16(local_at + 28)? as usize;
        if local_at + 30 + local_name > bytes.len()
            || &bytes[local_at + 30..local_at + 30 + local_name] != name_bytes
        {
            return Err("archive entry headers disagree about the name".into());
        }
        let data_at = local_at + 30 + local_name + local_extra;
        let data_end = data_at
            .checked_add(size as usize)
            .ok_or_else(|| "archive entry size overflow".to_string())?;
        if data_end > bytes.len() {
            return Err(format!("{name} extends past the archive"));
        }
        if size > MAX_ENTRY_BYTES {
            return Err(format!("{name} exceeds the per-entry size cap"));
        }
        total = total
            .checked_add(size)
            .ok_or_else(|| "archive uncompressed total overflow".to_string())?;
        if total > MAX_BUNDLE_UNCOMPRESSED {
            return Err("archive uncompressed total exceeds the bundle cap".into());
        }
        out.push(ZipEntry {
            name,
            data: bytes[data_at..data_end].to_vec(),
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// Map entry names to bytes, rejecting duplicate names.
pub fn entries_map(entries: &[ZipEntry]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut map = BTreeMap::new();
    for entry in entries {
        if map.insert(entry.name.clone(), entry.data.clone()).is_some() {
            return Err(format!("duplicate archive entry {}", entry.name));
        }
    }
    Ok(map)
}
