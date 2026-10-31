use crate::error::LoadError;

const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Load document bytes from a web address or a local file path.
pub fn load_bytes(source: &str) -> Result<Vec<u8>, LoadError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(LoadError::Empty);
    }
    let lower = source.to_ascii_lowercase();
    if lower.starts_with("file://") {
        let path = source.trim_start_matches("file://");
        return read_path(path);
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = source;
            return Err(LoadError::Failed(
                "Fetching a datasheet needs the desktop app.".into(),
            ));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            return fetch_url(source);
        }
    }
    if std::path::Path::new(source).exists() {
        return read_path(source);
    }
    Err(LoadError::Unsupported)
}

fn read_path(path: &str) -> Result<Vec<u8>, LoadError> {
    let bytes = std::fs::read(path).map_err(|e| LoadError::Failed(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(LoadError::Failed("The file is too large to open.".into()));
    }
    Ok(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_url(url: &str) -> Result<Vec<u8>, LoadError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .redirects(5)
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|e| LoadError::Failed(e.to_string()))?;
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf)
            .map_err(|e| LoadError::Failed(e.to_string()))?;
        if n == 0 {
            break;
        }
        if bytes.len() + n > MAX_BYTES {
            return Err(LoadError::Failed("The download is too large to open.".into()));
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    Ok(bytes)
}
