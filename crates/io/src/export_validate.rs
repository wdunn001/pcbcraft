//! Checks IPC-2581 and ODB++ exports before they are bundled as fabrication packages.

const NS: &str = "http://webstds.ipc.org/2581";

const ODB_MATRIX: &str = "matrix/matrix";
const ODB_SYMBOLS: &str = "symbols/symbols";
const ODB_FEATURES: [&str; 3] = ["features/top", "features/bottom", "features/misc"];
const ODB_COMPONENTS: [&str; 2] = ["components/top", "components/bottom"];

/// List every entry name in a stored ZIP written by `zip_entries`.
pub fn stored_zip_entry_names(bytes: &[u8]) -> Result<Vec<String>, String> {
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err("archive is not a stored ZIP".into());
    }
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| "archive has no central directory".to_string())?;
    let read_u16 = |at: usize| -> u16 { u16::from_le_bytes([bytes[at], bytes[at + 1]]) };
    let read_u32 = |at: usize| -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let entries = read_u16(eocd + 10) as usize;
    let mut at = read_u32(eocd + 16) as usize;
    let mut names = Vec::with_capacity(entries);
    for _ in 0..entries {
        if !bytes[at..].starts_with(b"PK\x01\x02") || at + 46 > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let name_len = read_u16(at + 28) as usize;
        let extra_len = read_u16(at + 30) as usize;
        let comment_len = read_u16(at + 32) as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let name = std::str::from_utf8(&bytes[at + 46..at + 46 + name_len])
            .map_err(|_| "archive entry name is not UTF-8".to_string())?
            .to_string();
        names.push(name);
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(names)
}

/// Read one entry payload from a stored ZIP built by `zip_entries`.
pub fn stored_zip_entry(bytes: &[u8], target: &str) -> Result<Vec<u8>, String> {
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| "archive has no central directory".to_string())?;
    let read_u16 = |at: usize| -> u16 { u16::from_le_bytes([bytes[at], bytes[at + 1]]) };
    let read_u32 = |at: usize| -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let entries = read_u16(eocd + 10) as usize;
    let mut at = read_u32(eocd + 16) as usize;
    for _ in 0..entries {
        if !bytes[at..].starts_with(b"PK\x01\x02") || at + 46 > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let size = read_u32(at + 20) as usize;
        let name_len = read_u16(at + 28) as usize;
        let extra_len = read_u16(at + 30) as usize;
        let comment_len = read_u16(at + 32) as usize;
        let local_at = read_u32(at + 42) as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let name = std::str::from_utf8(&bytes[at + 46..at + 46 + name_len])
            .map_err(|_| "archive entry name is not UTF-8".to_string())?;
        if name == target {
            if local_at + 30 > bytes.len() || !bytes[local_at..].starts_with(b"PK\x03\x04") {
                return Err("corrupt archive entry header".into());
            }
            let local_name = read_u16(local_at + 26) as usize;
            let local_extra = read_u16(local_at + 28) as usize;
            if local_at + 30 + local_name > bytes.len()
                || &bytes[local_at + 30..local_at + 30 + local_name] != name.as_bytes()
            {
                return Err("archive entry headers disagree about the name".into());
            }
            let data_at = local_at + 30 + local_name + local_extra;
            if data_at + size > bytes.len() {
                return Err("archive entry runs past the end of the file".into());
            }
            return Ok(bytes[data_at..data_at + size].to_vec());
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    Err(format!("archive holds no entry named {target}"))
}

/// Validate an ODB++ style package: stored ZIP, one job tree, required files.
pub fn validate_odb_package(bytes: &[u8]) -> Result<(), String> {
    let names = stored_zip_entry_names(bytes)?;
    if names.is_empty() {
        return Err("ODB++ package is empty".into());
    }
    let job = names[0]
        .split('/')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "ODB++ package has no job folder".to_string())?;
    if names
        .iter()
        .any(|name| name.split('/').next().is_some_and(|prefix| prefix != job))
    {
        return Err("ODB++ package mixes more than one job folder".into());
    }
    let prefix = format!("{job}/");
    for suffix in [ODB_MATRIX, ODB_SYMBOLS]
        .into_iter()
        .chain(ODB_FEATURES)
        .chain(ODB_COMPONENTS)
    {
        let path = format!("{prefix}{suffix}");
        if !names.iter().any(|name| name == &path) {
            return Err(format!("ODB++ package is missing {path}"));
        }
    }
    let matrix = stored_zip_entry(bytes, &format!("{prefix}{ODB_MATRIX}"))?;
    let matrix_text =
        std::str::from_utf8(&matrix).map_err(|_| "ODB++ matrix file is not UTF-8".to_string())?;
    if !matrix_text.contains(".Matrix") || !matrix_text.contains(".EndMatrix") {
        return Err("ODB++ matrix file is not a layer matrix".into());
    }
    if !matrix_text.contains(".Layer") {
        return Err("ODB++ matrix lists no layers".into());
    }
    let symbols = stored_zip_entry(bytes, &format!("{prefix}{ODB_SYMBOLS}"))?;
    let symbols_text =
        std::str::from_utf8(&symbols).map_err(|_| "ODB++ symbols file is not UTF-8".to_string())?;
    if !symbols_text.starts_with("UnitsType=MM") {
        return Err("ODB++ symbols file does not declare millimetre units".into());
    }
    for suffix in ODB_FEATURES {
        let body = stored_zip_entry(bytes, &format!("{prefix}{suffix}"))?;
        let text =
            std::str::from_utf8(&body).map_err(|_| format!("ODB++ {suffix} is not UTF-8"))?;
        if !text.starts_with("UnitsType=MM") {
            return Err(format!("ODB++ {suffix} does not declare millimetre units"));
        }
        if !text.contains(".Features") || !text.contains(".EndFeatures") {
            return Err(format!("ODB++ {suffix} is not a feature file"));
        }
    }
    Ok(())
}

/// Validate IPC-2581 XML against the revision C schema when the validator is linked.
#[cfg(not(target_arch = "wasm32"))]
pub fn validate_ipc2581_document(text: &str) -> Result<(), String> {
    if text.contains("<EDA3D") {
        return Err(
            "IPC-2581 export uses the legacy root element and is not schema-valid yet".into(),
        );
    }
    if !text.contains(&format!("<IPC-2581 revision=\"C\" xmlns=\"{NS}\">")) {
        return Err(
            "IPC-2581 export is not a revision C document in the standard namespace".into(),
        );
    }
    ipc2581::validate(text).map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn validate_ipc2581_document(text: &str) -> Result<(), String> {
    if text.contains("<EDA3D") {
        return Err(
            "IPC-2581 export uses the legacy root element and is not schema-valid yet".into(),
        );
    }
    if !text.contains("<IPC-2581") {
        return Err("IPC-2581 export is not a revision C document".into());
    }
    Ok(())
}

/// Refuse fabrication packages that bundle exports which failed validation.
pub fn validate_fabrication_package_file(
    kind: &str,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    match kind {
        "ipc2581" if name.ends_with(".xml") => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| "IPC-2581 package file is not UTF-8".to_string())?;
            validate_ipc2581_document(text)
        }
        "odb" if name.ends_with(".odb.zip") => validate_odb_package(bytes),
        _ if name.ends_with(".xml") => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| "IPC-2581 package file is not UTF-8".to_string())?;
            if text.contains("<IPC-2581") || text.contains("<EDA3D") {
                validate_ipc2581_document(text)
            } else {
                Ok(())
            }
        }
        _ if name.ends_with(".odb.zip") => validate_odb_package(bytes),
        _ => Ok(()),
    }
}
