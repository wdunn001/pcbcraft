#[cfg(not(target_arch = "wasm32"))]
use sha2::{Digest, Sha256};
#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};

pub struct DatasheetCacheResult {
    pub sha256: String,
    pub path: String,
    pub retrieved_at: String,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_datasheet_cache(
    url: &str,
    extra_headers: &[(String, String)],
) -> Result<DatasheetCacheResult, String> {
    if url.is_empty() || url.len() > 4096 {
        return Err("datasheet URL invalid".into());
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("datasheet URL must be http or https".into());
    }
    const MAX_BYTES: usize = 32 * 1024 * 1024;
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    if let Some(len) = response
        .header("content-length")
        .and_then(|v| v.trim().parse::<usize>().ok())
        && len > MAX_BYTES
    {
        return Err("datasheet exceeds 32 MiB".into());
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|e| format!("read body: {e}"))?;
    if body.len() > MAX_BYTES {
        return Err("datasheet exceeds 32 MiB".into());
    }
    let mut hasher = Sha256::new();
    hasher.update(&body);
    let digest = hasher.finalize();
    let sha256 = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let dir = cache_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{sha256}.pdf"));
    if !path.is_file() {
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        file.write_all(&body).map_err(|e| e.to_string())?;
    }
    let retrieved_at = format_timestamp();
    Ok(DatasheetCacheResult {
        sha256,
        path: path.display().to_string(),
        retrieved_at,
    })
}

#[cfg(target_arch = "wasm32")]
pub fn fetch_datasheet_cache(
    _url: &str,
    _extra_headers: &[(String, String)],
) -> Result<DatasheetCacheResult, String> {
    Err("datasheet download is not available on wasm builds".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn cache_dir() -> Result<std::path::PathBuf, String> {
    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .or_else(|| std::env::var("LOCALAPPDATA").ok())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| "no cache directory".to_string())?;
    Ok(base.join("pcbcraft").join("datasheets"))
}

#[cfg(not(target_arch = "wasm32"))]
fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
