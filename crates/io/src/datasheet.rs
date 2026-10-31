//! HTTP transport for datasheet PDF fetch. Only this module in `pcbcraft-io`
//! opens a socket for datasheets.
//!
//! # Rules
//! - Plain GET over http or https to any host. No credentials in the URL.
//! - Every call uses a timeout, [`pcbcraft_library::datasheet::MAX_PDF_BYTES`]
//!   as the body cap and at most [`pcbcraft_library::datasheet::MAX_REDIRECTS`]
//!   redirects.
//! - Conditional requests send `If-None-Match` when an ETag is known.
//!
//! # Dependencies
//! - crates/library/src/datasheet.rs
use pcbcraft_library::datasheet::{
    accept_datasheet_url, DatasheetError, DatasheetTransport, FetchAnswer, MAX_PDF_BYTES,
    MAX_REDIRECTS,
};
use std::io::Read;
use std::time::Duration;

/// Whole-call timeout for one fetch, redirects included per hop.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Desktop HTTP transport for datasheet PDFs.
///
/// # Do not
/// - Call from the UI thread; use a worker thread like other network work.
pub struct HttpDatasheetTransport {
    agent: ureq::Agent,
    timeout: Duration,
}

impl HttpDatasheetTransport {
    /// Same transport with a shorter or longer whole-call timeout (tests).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// A transport with the default 30 second timeout.
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout_connect(Duration::from_secs(10))
            .user_agent(&format!(
                "PCBCraft/{} (+https://github.com/tangency-labs/pcbcraft)",
                env!("CARGO_PKG_VERSION")
            ))
            .build();
        Self {
            agent,
            timeout: FETCH_TIMEOUT,
        }
    }

    fn read_capped(response: ureq::Response) -> Result<Vec<u8>, DatasheetError> {
        let too_big = || DatasheetError::Invalid("the file is larger than the size limit".into());
        if let Some(len) = response
            .header("content-length")
            .and_then(|v| v.trim().parse::<usize>().ok())
            && len > MAX_PDF_BYTES
        {
            return Err(too_big());
        }
        let mut body = Vec::new();
        response
            .into_reader()
            .take(MAX_PDF_BYTES as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| DatasheetError::Unavailable("the connection broke before the answer arrived".into()))?;
        if body.len() > MAX_PDF_BYTES {
            return Err(too_big());
        }
        Ok(body)
    }
}

impl Default for HttpDatasheetTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl DatasheetTransport for HttpDatasheetTransport {
    fn fetch(
        &self,
        url: &str,
        etag: Option<&str>,
        offline: bool,
    ) -> Result<FetchAnswer, DatasheetError> {
        if offline {
            return Err(DatasheetError::Offline(
                "There is no network and no saved copy.".into(),
            ));
        }
        let mut current = url.to_owned();
        for _ in 0..=MAX_REDIRECTS {
            let mut request = self.agent.get(&current).timeout(self.timeout);
            if let Some(tag) = etag.filter(|t| !t.is_empty()) {
                request = request.set("If-None-Match", tag);
            }
            let response = match request.call() {
                Ok(r) => r,
                Err(ureq::Error::Status(404, _)) => return Err(DatasheetError::NotFound),
                Err(ureq::Error::Status(_, r)) => r,
                Err(e) => return Err(transport_error(&e)),
            };
            let status = response.status();
            match status {
                200..=299 => {
                    let tag = response
                        .header("etag")
                        .map(|s| s.trim().trim_matches('"').to_owned())
                        .filter(|s| !s.is_empty());
                    let bytes = Self::read_capped(response)?;
                    return Ok(FetchAnswer::Fresh { bytes, etag: tag });
                }
                301 | 302 | 303 | 307 | 308 => {
                    let Some(location) = response.header("location") else {
                        return Err(DatasheetError::Invalid(
                            "the host sent a bad redirect".into(),
                        ));
                    };
                    current = resolve(&current, location)?;
                    current = accept_datasheet_url(&current)?;
                }
                304 => return Ok(FetchAnswer::NotModified),
                404 => return Err(DatasheetError::NotFound),
                other => {
                    return Err(DatasheetError::Unavailable(format!(
                        "the host answered {other}"
                    )));
                }
            }
        }
        Err(DatasheetError::Invalid(
            "the host redirected too many times".into(),
        ))
    }
}

fn transport_error(e: &ureq::Error) -> DatasheetError {
    match e {
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Io => DatasheetError::Unavailable(
                "the host took too long to answer".into(),
            ),
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => {
                DatasheetError::Offline("no connection to the host".into())
            }
            _ => DatasheetError::Unavailable("the request could not be sent".into()),
        },
        ureq::Error::Status(code, _) => {
            DatasheetError::Unavailable(format!("the host answered {code}"))
        }
    }
}

fn resolve(base: &str, location: &str) -> Result<String, DatasheetError> {
    if location.contains("://") {
        return accept_datasheet_url(location);
    }
    let bad = || DatasheetError::Invalid("the host sent a bad redirect".into());
    let (scheme, rest) = base.split_once("://").ok_or_else(bad)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let path = location.strip_prefix('/').ok_or_else(bad)?;
    Ok(format!("{scheme}://{authority}/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_library::datasheet::{DatasheetCache, is_pdf};
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const PDF: &[u8] = b"%PDF-1.4\n% minimal\n";

    struct Server {
        port: u16,
        seen: Arc<Mutex<Vec<String>>>,
        connections: Arc<AtomicUsize>,
    }

    fn serve(replies: Vec<Vec<u8>>) -> Result<Server, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let (s, c) = (Arc::clone(&seen), Arc::clone(&connections));
        thread::spawn(move || {
            for reply in replies {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                c.fetch_add(1, Ordering::SeqCst);
                let request = read_request(&mut stream);
                if let Ok(mut guard) = s.lock() {
                    guard.push(request);
                }
                if reply.is_empty() {
                    thread::sleep(Duration::from_secs(3));
                    continue;
                }
                let _ = stream.write_all(&reply);
            }
        });
        Ok(Server {
            port,
            seen,
            connections,
        })
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut got = Vec::new();
        let mut buf = [0u8; 512];
        while !got.windows(4).any(|w| w == b"\r\n\r\n") {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
            }
        }
        String::from_utf8_lossy(&got).into_owned()
    }

    fn ok(body: &[u8], extra_headers: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n",
            body.len()
        )
        .into_bytes()
        .into_iter()
        .chain(body.iter().copied())
        .collect()
    }

    fn status(line: &str, headers: &str) -> Vec<u8> {
        format!("HTTP/1.1 {line}\r\n{headers}Content-Length: 0\r\nConnection: close\r\n\r\n")
            .into_bytes()
    }

    fn url(s: &Server, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", s.port)
    }

    fn cache_dir() -> Result<tempfile::TempDir, std::io::Error> {
        tempfile::tempdir()
    }

    #[test]
    fn fetch_refuses_a_body_over_the_cap() -> TestResult {
        let mut big = vec![0u8; MAX_PDF_BYTES + 1];
        big[..8].copy_from_slice(b"%PDF-1.4");
        let server = serve(vec![ok(&big, "")])?;
        let cache = DatasheetCache::in_memory();
        let transport = HttpDatasheetTransport::new();
        let err = cache
            .fetch_url(&transport, &url(&server, "/big.pdf"), false)
            .err();
        assert!(
            matches!(err, Some(DatasheetError::Invalid(_))),
            "{err:?}"
        );
        Ok(())
    }

    #[test]
    fn fetch_refuses_a_non_pdf_body() -> TestResult {
        let server = serve(vec![ok(b"not a pdf", "")])?;
        let cache = DatasheetCache::in_memory();
        let transport = HttpDatasheetTransport::new();
        let err = cache
            .fetch_url(&transport, &url(&server, "/x.pdf"), false)
            .err();
        assert!(
            matches!(err, Some(DatasheetError::Invalid(m)) if m.contains("not a PDF")),
            "{err:?}"
        );
        Ok(())
    }

    #[test]
    fn fetch_stops_a_redirect_loop() -> TestResult {
        let server = serve(vec![status("302 Found", "Location: /loop.pdf\r\n"); MAX_REDIRECTS + 2])?;
        let cache = DatasheetCache::in_memory();
        let transport = HttpDatasheetTransport::new();
        let err = cache
            .fetch_url(&transport, &url(&server, "/loop.pdf"), false)
            .err();
        assert!(
            matches!(err, Some(DatasheetError::Invalid(m)) if m.contains("redirect")),
            "{err:?}"
        );
        Ok(())
    }

    #[test]
    fn fetch_times_out_when_the_host_is_silent() -> TestResult {
        let server = serve(vec![Vec::new()])?;
        let transport = HttpDatasheetTransport::new().with_timeout(Duration::from_millis(400));
        let cache = DatasheetCache::in_memory();
        let err = cache
            .fetch_url(&transport, &url(&server, "/slow.pdf"), false)
            .err();
        assert!(err.is_some(), "expected a timeout error");
        Ok(())
    }

    #[test]
    fn refresh_uses_a_conditional_request_and_keeps_the_cached_pdf() -> TestResult {
        let server = serve(vec![
            ok(PDF, "ETag: \"v1\"\r\n"),
            status("304 Not Modified", "ETag: \"v1\"\r\n"),
        ])?;
        let tmp = cache_dir()?;
        let cache = DatasheetCache::open(tmp.path());
        let transport = HttpDatasheetTransport::new();
        let address = url(&server, "/part.pdf");
        let (first, bytes) = cache.fetch_url(&transport, &address, false)?;
        assert!(is_pdf(&bytes));
        assert_eq!(first.revision, "v1");
        let (second, again) = cache.fetch_url(&transport, &address, false)?;
        assert_eq!(first.content_sha256, second.content_sha256);
        assert_eq!(bytes, again);
        let seen = server.seen.lock().map_err(|_| "poisoned")?;
        assert_eq!(seen.len(), 2);
        let second_req = seen.get(1).ok_or("missing second request")?.to_ascii_lowercase();
        assert!(second_req.contains("if-none-match"));
        Ok(())
    }

    #[test]
    fn offline_open_reads_a_saved_pdf_without_network() -> TestResult {
        let server = serve(vec![ok(PDF, "")])?;
        let tmp = cache_dir()?;
        let cache = DatasheetCache::open(tmp.path());
        let transport = HttpDatasheetTransport::new();
        let address = url(&server, "/saved.pdf");
        cache.fetch_url(&transport, &address, false)?;
        let (reference, bytes) = cache.fetch_url(&transport, &address, true)?;
        assert_eq!(bytes, PDF);
        assert!(!reference.content_sha256.is_empty());
        Ok(())
    }

    #[test]
    fn eviction_keeps_disk_use_under_the_cap() -> TestResult {
        let tmp = cache_dir()?;
        let cap = PDF.len() as u64 * 2 + 64;
        let cache = DatasheetCache::open(tmp.path().join("cache")).with_cap(cap);
        for i in 0..4u8 {
            let mut body = PDF.to_vec();
            body.push(i);
            assert!(is_pdf(&body));
            let file = tmp.path().join(format!("p{i}.pdf"));
            std::fs::write(&file, &body)?;
            cache.import_local_file(&file)?;
        }
        assert!(
            cache.disk_bytes() <= cap,
            "disk bytes {} > cap {}",
            cache.disk_bytes(),
            cap
        );
        Ok(())
    }

    #[test]
    fn local_file_import_puts_a_pdf_in_the_cache() -> TestResult {
        let tmp = cache_dir()?;
        let pdf_path = tmp.path().join("local.pdf");
        std::fs::write(&pdf_path, PDF)?;
        let absolute = std::fs::canonicalize(&pdf_path)?;
        let cache = DatasheetCache::open(tmp.path().join("cache"));
        let (_reference, bytes) = cache.import_local_file(&absolute)?;
        assert_eq!(bytes, PDF);
        Ok(())
    }
}
