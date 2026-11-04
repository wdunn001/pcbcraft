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
            return fetch_url(source);
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
            return Err(LoadError::Failed(
                "The download is too large to open.".into(),
            ));
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
fn fetch_url(url: &str) -> Result<Vec<u8>, LoadError> {
    use wasm_bindgen_futures::JsFuture;
    let window = web_sys::window().ok_or(LoadError::Failed(
        "The browser window is not available.".into(),
    ))?;
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    let promise = window.fetch_with_str_and_init(url, &init);
    let response: web_sys::Response = wasm_bindgen_futures::block_on(JsFuture::from(promise))
        .map_err(|_| LoadError::Failed("The fetch could not complete.".into()))?
        .dyn_into()
        .map_err(|_| LoadError::Failed("The browser sent an odd answer.".into()))?;
    match response.status() {
        200..=299 => {}
        code => {
            return Err(LoadError::Failed(format!(
                "The host answered with status {code}."
            )));
        }
    }
    let buffer = response
        .array_buffer()
        .map_err(|_| LoadError::Failed("The answer could not be read.".into()))?;
    let buffer: wasm_bindgen::JsValue = wasm_bindgen_futures::block_on(JsFuture::from(buffer))
        .map_err(|_| LoadError::Failed("The answer could not be read.".into()))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}
