//! What the desktop app offers the assistant: a way to send HTTP requests and the system keychain.
//!
//! Context: `pcbcraft_ui_egui::Host` carries both into the window; the browser build passes
//! neither. Nothing here runs until the user presses Test or sends a message.
//! Rules: redirects are never followed (a redirect is an error, so a key cannot travel on);
//! replies are read up to [`MAX_REPLY`] bytes; error text never includes request headers.
//! Do not: log a request, a header or a key.

use pcbcraft_assistant::{HttpFn, HttpRequest, HttpResponse, Secrets, deliver_streamed_body};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

/// Most bytes of a reply read.
const MAX_REPLY: u64 = 16 * 1024 * 1024;
/// The keychain service name.
const SERVICE: &str = "PCBCraft assistant";

/// The host the window gets.
pub fn host() -> pcbcraft_ui_egui::Host {
    pcbcraft_ui_egui::Host {
        http: Some(http()),
        secrets: Some(Arc::new(Keychain)),
    }
}

fn http() -> HttpFn {
    let agent = ureq::AgentBuilder::new().redirects(0).build();
    Arc::new(move |request: &HttpRequest| send(&agent, request))
}

fn send(agent: &ureq::Agent, request: &HttpRequest) -> Result<HttpResponse, String> {
    let mut call = agent
        .request(request.method, &request.url)
        .timeout(Duration::from_secs(request.timeout_s.max(1)));
    for (name, value) in &request.headers {
        call = call.set(name, value);
    }
    let reply = match &request.body {
        Some(body) => call.send_bytes(body),
        None => call.call(),
    };
    let response = match reply {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => return Err(transport_error(&e)),
    };
    let status = response.status();
let mut headers = Vec::new();
    for name in ["content-type", "content-length", "mcp-session-id"] {
        if let Some(value) = response.header(name) {
            headers.push((name.to_string(), value.to_string()));
        }
    }
    if let Some(out) = &request.stream_out {
        let mut reader = response.into_reader().take(MAX_REPLY);
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    out.send(Ok(pcbcraft_assistant::HttpStreamPart::Chunk(
                        buf[..n].to_vec(),
                    )))
                    .map_err(|_| "The stream ended early.".to_string())?;
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        return deliver_streamed_body(out, status, &[]);
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_REPLY)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_REPLY)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// A short sentence for a failure to reach the server.
fn transport_error(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Dns => "The address could not be found.".into(),
            ureq::ErrorKind::ConnectionFailed => "The server did not answer.".into(),
            ureq::ErrorKind::Io => "The connection broke or timed out.".into(),
            _ => "The request failed.".into(),
        },
        ureq::Error::Status(code, _) => format!("The server answered {code}."),
    }
}

/// The system keychain, one entry per connection.
struct Keychain;

impl Keychain {
    fn entry(id: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())
    }
}

impl Secrets for Keychain {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        match Self::entry(id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        Self::entry(id)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
