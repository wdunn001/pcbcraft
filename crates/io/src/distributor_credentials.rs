//! Distributor API tokens for native datasheet fetch. Secrets live in the OS keychain, not project JSON.

const SERVICE: &str = "PCBCraft distributor";

fn account_id(distributor: &str) -> Result<String, String> {
    let trimmed = distributor.trim();
    if trimmed.is_empty() || trimmed.len() > 48 {
        return Err("distributor name invalid".into());
    }
    let mut out = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            out.push('_');
        } else {
            return Err("distributor name has unsupported characters".into());
        }
    }
    if out.is_empty() {
        return Err("distributor name invalid".into());
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn entry(distributor: &str) -> Result<keyring::Entry, String> {
    let id = account_id(distributor)?;
    keyring::Entry::new(SERVICE, &id).map_err(|e| e.to_string())
}

/// Store a distributor token in the native keychain.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_distributor_credential(distributor: &str, secret: &str) -> Result<(), String> {
    if secret.is_empty() || secret.len() > 4096 {
        return Err("credential secret invalid".into());
    }
    entry(distributor)?
        .set_password(secret)
        .map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn set_distributor_credential(_distributor: &str, _secret: &str) -> Result<(), String> {
    Err("distributor credentials are not available on wasm builds".into())
}

/// Remove a stored distributor token.
#[cfg(not(target_arch = "wasm32"))]
pub fn clear_distributor_credential(distributor: &str) -> Result<(), String> {
    match entry(distributor)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn clear_distributor_credential(_distributor: &str) -> Result<(), String> {
    Err("distributor credentials are not available on wasm builds".into())
}

/// HTTP headers to attach when fetching from a distributor that requires a user token.
#[cfg(not(target_arch = "wasm32"))]
pub fn auth_headers_for_distributor(distributor: &str) -> Result<Vec<(String, String)>, String> {
    if distributor.trim().is_empty() {
        return Ok(Vec::new());
    }
    match entry(distributor)?.get_password() {
        Ok(secret) if !secret.is_empty() => Ok(vec![(
            "Authorization".into(),
            format!("Bearer {secret}"),
        )]),
        Ok(_) => Ok(Vec::new()),
        Err(keyring::Error::NoEntry) => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn auth_headers_for_distributor(_distributor: &str) -> Result<Vec<(String, String)>, String> {
    Ok(Vec::new())
}
