//! What the desktop app gives the Library dock for distributor sources: an HTTPS client
//! and the key store for the person's own keys.
//!
//! # Rules
//! - Calls go to allowlisted distributor hosts over HTTPS only, with a timeout and a reply size cap.
//! - Redirects are never followed, so a key cannot travel to another host.
//! - Offline mode answers before any socket is opened.
//! - Keys live in the system keychain; when it cannot be opened they fall back to a file in the
//!   app's settings folder, and the dock says so.
//!
//! # Dependencies
//! - crates/library/src/http_client.rs
//! - crates/library/src/credentials.rs
//!
//! # Do not
//! - Log a request, a header or a body: they carry the key.

use pcbcraft_library::credentials::{CredentialError, FileKeyStore, KeyStore};
use pcbcraft_library::http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Hosts distributor sources may call.
const ALLOWED_HOSTS: &[&str] = &["api.digikey.com"];
/// Most bytes of a reply read.
const MAX_REPLY: u64 = 4 * 1024 * 1024;
/// Time for one call.
const TIMEOUT: Duration = Duration::from_secs(20);
/// The keychain service name.
const SERVICE: &str = "PCBCraft part sources";

/// The blocking transport behind distributor sources.
pub struct DistributorHttp {
    agent: ureq::Agent,
    offline: Arc<AtomicBool>,
}

impl DistributorHttp {
    /// A client that stops when `offline` is set.
    pub fn new(offline: Arc<AtomicBool>) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .redirects(0)
                .timeout(TIMEOUT)
                .user_agent(concat!("PCBCraft/", env!("CARGO_PKG_VERSION")))
                .build(),
            offline,
        }
    }
}

/// The host of an `https://` URL, or `None` for any other scheme.
fn https_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.contains('@') && !host.contains(':')).then_some(host)
}

/// Contract: crates/library/src/http_client.rs
impl HttpClient for DistributorHttp {
    fn call(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(HttpError::Failed("Offline mode is on.".into()));
        }
        let allowed = https_host(&request.url).is_some_and(|h| ALLOWED_HOSTS.contains(&h));
        if !allowed {
            return Err(HttpError::Failed("That address is not allowed.".into()));
        }
        let mut call = self.agent.request(&request.method, &request.url);
        for (name, value) in &request.headers {
            call = call.set(name, value);
        }
        if request.method == "POST" && !request.headers.contains_key("Content-Type") {
            call = call.set("Content-Type", "application/x-www-form-urlencoded");
        }
        let reply = if request.body.is_empty() && request.method != "POST" {
            call.call()
        } else {
            call.send_bytes(&request.body)
        };
        let response = match reply {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => {
                return Err(HttpError::Failed(match t.kind() {
                    ureq::ErrorKind::Dns => "The address could not be found.".into(),
                    ureq::ErrorKind::ConnectionFailed => "The server did not answer.".into(),
                    _ => "The connection broke or timed out.".into(),
                }));
            }
        };
        let status = response.status();
        let mut headers = BTreeMap::new();
        if let Some(wait) = response.header("Retry-After") {
            headers.insert("Retry-After".to_owned(), wait.to_owned());
        }
        let mut body = Vec::new();
        response
            .into_reader()
            .take(MAX_REPLY)
            .read_to_end(&mut body)
            .map_err(|_| HttpError::Failed("The reply could not be read.".into()))?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

/// The system keychain, one entry per value.
struct Keychain;

impl Keychain {
    fn entry(name: &str) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(SERVICE, name).map_err(|e| CredentialError::Failed(e.to_string()))
    }
}

/// Contract: crates/library/src/credentials.rs
impl KeyStore for Keychain {
    fn get(&self, name: &str) -> Result<Option<String>, CredentialError> {
        match Self::entry(name)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CredentialError::Failed(e.to_string())),
        }
    }

    fn set(&self, name: &str, value: &str) -> Result<(), CredentialError> {
        if name.len() > 128 || value.len() > 4096 {
            return Err(CredentialError::Failed(
                "the key name or value is too long".into(),
            ));
        }
        Self::entry(name)?
            .set_password(value)
            .map_err(|e| CredentialError::Failed(e.to_string()))
    }

    fn delete(&self, name: &str) -> Result<(), CredentialError> {
        match Self::entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CredentialError::Failed(e.to_string())),
        }
    }
}

/// The keychain when it answers, otherwise a file at `fallback`, otherwise nothing.
///
/// # Rules
/// - The keychain is probed with a read of a name nothing stores; any answer but an error counts.
/// - With no keychain and no settings folder there is no store, so keys are never kept in a temporary file.
pub fn key_store(fallback: Option<std::path::PathBuf>) -> Option<Arc<dyn KeyStore>> {
    match Keychain.get("probe") {
        Ok(_) => Some(Arc::new(Keychain)),
        Err(_) => fallback.map(|path| Arc::new(FileKeyStore::new(path)) as Arc<dyn KeyStore>),
    }
}

#[cfg(test)]
mod tests {
    use super::https_host;

    #[test]
    fn only_plain_https_hosts_are_read_from_an_address() {
        assert_eq!(
            https_host("https://api.digikey.com/x?y"),
            Some("api.digikey.com")
        );
        assert_eq!(https_host("http://api.digikey.com/x"), None);
        assert_eq!(https_host("https://user@api.digikey.com/x"), None);
        assert_eq!(https_host("https://api.digikey.com:8443/x"), None);
    }
}
