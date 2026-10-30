//! The desktop transport for online CAD sources: the [`Fetcher`] that the open
//! library source reads through. The library crate stays network-free; this
//! file is the only place that opens a socket for it.
//!
//! # Rules
//! - Plain GET over HTTPS to an allowlisted host. No cookies, no credentials, no user info in the URL.
//! - Every call carries a timeout, a response size cap and a User-Agent naming PCBCraft and its version.
//! - Redirects are followed by hand, at most [`MAX_REDIRECTS`] times, and only to the same host.
//! - Calls are spaced at least [`Policy::min_gap`] apart, so the hosts see a few requests per second at most.
//! - Offline mode answers before any socket is opened.
//! - `Fetcher::get` blocks. Call it from a worker thread; [`HttpFetcher::get_in_background`] does that for a caller on the UI thread.
//!
//! # Dependencies
//! - crates/library/src/open_library.rs
//! - crates/library/src/cad_cache.rs
//! - apps/pcbcraft/src/host.rs (same HTTP client, separate agent: no assistant keys travel here)

use pcbcraft_library::cad::MAX_FILE_BYTES;
use pcbcraft_library::cad_cache::CadCache;
use pcbcraft_library::open_library::{FetchError, Fetcher};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Hosts the open library source reads from.
pub const OPEN_LIBRARY_HOSTS: &[&str] = &["gitlab.com"];
/// Most redirects followed for one call.
pub const MAX_REDIRECTS: usize = 3;

/// The limits one transport works under.
///
/// # Rules
/// - `allowed_hosts` is matched against the whole host name, never a suffix.
/// - `require_https` is on outside tests.
///
/// # Dependencies
/// - crates/library/src/cad.rs (the size cap)
///
/// # Do not
/// - Add a field that carries a credential.
#[derive(Clone, Debug)]
pub struct Policy {
    /// Hosts a request may go to.
    pub allowed_hosts: Vec<String>,
    /// Most body bytes read.
    pub max_bytes: usize,
    /// Time for the whole call, redirects included per hop.
    pub timeout: Duration,
    /// Time to open the connection.
    pub connect_timeout: Duration,
    /// Least time between two calls.
    pub min_gap: Duration,
    /// Refuse plain http. Tests switch this off to reach a loopback server.
    pub require_https: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            allowed_hosts: OPEN_LIBRARY_HOSTS.iter().map(|h| (*h).to_owned()).collect(),
            max_bytes: MAX_FILE_BYTES,
            timeout: Duration::from_secs(20),
            connect_timeout: Duration::from_secs(8),
            min_gap: Duration::from_millis(250),
            require_https: true,
        }
    }
}

/// The User-Agent every call sends.
pub fn user_agent() -> String {
    format!(
        "PCBCraft/{} (+https://github.com/wdunn001/pcbcraft)",
        env!("CARGO_PKG_VERSION")
    )
}

/// The desktop [`Fetcher`].
///
/// # Rules
/// - The offline flag is shared with the settings toggle; flipping it takes effect on the next call.
///
/// # Dependencies
/// - crates/library/src/open_library.rs
///
/// # Do not
/// - Call `get` on the UI thread; use [`HttpFetcher::get_in_background`].
///
/// Contract: crates/library/src/open_library.rs
pub struct HttpFetcher {
    agent: ureq::Agent,
    policy: Policy,
    offline: Arc<AtomicBool>,
    last: Mutex<Option<Instant>>,
}

impl HttpFetcher {
    /// A transport under `policy`, switched off while `offline` is set.
    pub fn new(policy: Policy, offline: Arc<AtomicBool>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout_connect(policy.connect_timeout)
            .user_agent(&user_agent())
            .build();
        Self {
            agent,
            policy,
            offline,
            last: Mutex::new(None),
        }
    }

    /// Run `get` on its own thread and hand back the receiving end, so the
    /// window keeps drawing while the answer travels.
    pub fn get_in_background(self: &Arc<Self>, url: &str) -> Receiver<Result<Vec<u8>, FetchError>> {
        let (tx, rx) = channel();
        let me = Arc::clone(self);
        let url = url.to_owned();
        std::thread::spawn(move || {
            // A closed receiver only means nobody waits any more.
            let _ = tx.send(me.get(&url));
        });
        rx
    }

    fn pace(&self) {
        let wait = match self.last.lock() {
            Ok(mut last) => {
                let now = Instant::now();
                let due = last.map(|t| t + self.policy.min_gap).unwrap_or(now);
                *last = Some(due.max(now));
                due.saturating_duration_since(now)
            }
            Err(_) => Duration::ZERO,
        };
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    fn check(&self, parts: &Parts) -> Result<(), FetchError> {
        if self.policy.require_https && parts.scheme != "https" {
            return Err(FetchError::Failed(
                "Only secure addresses are used for online libraries.".into(),
            ));
        }
        if !self.policy.require_https && parts.scheme != "https" && parts.scheme != "http" {
            return Err(FetchError::Failed(
                "That address is not a web address.".into(),
            ));
        }
        if !self.policy.allowed_hosts.iter().any(|h| h == &parts.host) {
            return Err(FetchError::Failed(format!(
                "{} is not one of the online library sites.",
                parts.host
            )));
        }
        Ok(())
    }
}

/// Contract: crates/library/src/open_library.rs
impl Fetcher for HttpFetcher {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(FetchError::Offline(
                "Online libraries are switched off. Saved parts still open.".into(),
            ));
        }
        let first = Parts::parse(url)?;
        self.check(&first)?;
        let mut current = url.to_owned();
        for _ in 0..=MAX_REDIRECTS {
            self.pace();
            let reply = self.agent.get(&current).timeout(self.policy.timeout).call();
            let response = match reply {
                Ok(r) => r,
                Err(ureq::Error::Status(_, r)) => r,
                Err(e) => return Err(transport_error(&e)),
            };
            let status = response.status();
            match status {
                200..=299 => return read_capped(response, self.policy.max_bytes),
                301 | 302 | 303 | 307 | 308 => {
                    let Some(location) = response.header("location") else {
                        return Err(FetchError::Failed("The host sent a bad redirect.".into()));
                    };
                    let next = resolve(&current, location)?;
                    let parts = Parts::parse(&next)?;
                    if parts.host != first.host {
                        return Err(FetchError::Failed(
                            "The host tried to send the request to another site, so it was stopped."
                                .into(),
                        ));
                    }
                    self.check(&parts)?;
                    current = next;
                }
                404 | 410 => return Err(FetchError::NotFound),
                429 => {
                    let wait = response
                        .header("retry-after")
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .unwrap_or(30);
                    return Err(FetchError::RateLimited {
                        retry_after_secs: wait.min(3600),
                    });
                }
                other => {
                    return Err(FetchError::Failed(format!("The host answered {other}.")));
                }
            }
        }
        Err(FetchError::Failed(
            "The host redirected too many times.".into(),
        ))
    }
}

/// Read a body up to the cap; one byte more is a refusal.
fn read_capped(response: ureq::Response, max: usize) -> Result<Vec<u8>, FetchError> {
    let too_big =
        || FetchError::Failed("The file is larger than the size limit, so it was not read.".into());
    if let Some(len) = response
        .header("content-length")
        .and_then(|v| v.trim().parse::<usize>().ok())
        && len > max
    {
        return Err(too_big());
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(max as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|e| io_error(&e))?;
    if body.len() > max {
        return Err(too_big());
    }
    Ok(body)
}

fn io_error(e: &std::io::Error) -> FetchError {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            FetchError::Failed("The host took too long to answer.".into())
        }
        _ => FetchError::Failed("The connection broke before the answer arrived.".into()),
    }
}

fn transport_error(e: &ureq::Error) -> FetchError {
    match e {
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed => {
                FetchError::Offline("No connection to the online library.".into())
            }
            ureq::ErrorKind::Io => FetchError::Failed("The host took too long to answer.".into()),
            _ => FetchError::Failed("The request could not be sent.".into()),
        },
        ureq::Error::Status(code, _) => FetchError::Failed(format!("The host answered {code}.")),
    }
}

/// Scheme, host and port of an address, with user info refused.
struct Parts {
    scheme: String,
    host: String,
}

impl Parts {
    fn parse(url: &str) -> Result<Self, FetchError> {
        let bad = || FetchError::Failed("That is not a valid web address.".into());
        let (scheme, rest) = url.split_once("://").ok_or_else(bad)?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if authority.is_empty() || authority.contains('@') || url.contains(char::is_whitespace) {
            return Err(bad());
        }
        let host = match authority.rsplit_once(':') {
            Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
            _ => authority,
        };
        Ok(Self {
            scheme: scheme.to_ascii_lowercase(),
            host: host.to_ascii_lowercase(),
        })
    }
}

/// Resolve a Location value against the address it came from.
fn resolve(base: &str, location: &str) -> Result<String, FetchError> {
    if location.contains("://") {
        return Ok(location.to_owned());
    }
    let bad = || FetchError::Failed("The host sent a bad redirect.".into());
    let (scheme, rest) = base.split_once("://").ok_or_else(bad)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let path = location.strip_prefix('/').ok_or_else(bad)?;
    Ok(format!("{scheme}://{authority}/{path}"))
}

/// Empty the online cache: every stored file and listing under `dir`.
///
/// # Rules
/// - Only the cache's own `files` and `listings` folders are removed.
///
/// # Dependencies
/// - crates/library/src/cad_cache.rs
///
/// # Do not
/// - Call it while an import is writing; the settings button disables itself during one.
pub fn purge_cache(dir: &Path) -> Result<(), FetchError> {
    CadCache::open(dir)
        .purge()
        .map_err(|e| FetchError::Failed(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::AtomicUsize;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A loopback server that answers each connection with the next scripted reply.
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
        std::thread::spawn(move || {
            for reply in replies {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                c.fetch_add(1, Ordering::SeqCst);
                let request = read_request(&mut stream);
                if let Ok(mut s) = s.lock() {
                    s.push(request);
                }
                if reply.is_empty() {
                    std::thread::sleep(Duration::from_secs(3));
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

    fn ok(body: &[u8]) -> Vec<u8> {
        let mut r = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nSet-Cookie: a=b\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        r.extend_from_slice(body);
        r
    }

    fn status(line: &str, headers: &str) -> Vec<u8> {
        format!("HTTP/1.1 {line}\r\n{headers}Content-Length: 0\r\nConnection: close\r\n\r\n")
            .into_bytes()
    }

    fn local(max_bytes: usize) -> (HttpFetcher, Arc<AtomicBool>) {
        let offline = Arc::new(AtomicBool::new(false));
        let policy = Policy {
            allowed_hosts: vec!["127.0.0.1".into()],
            max_bytes,
            timeout: Duration::from_millis(600),
            min_gap: Duration::ZERO,
            require_https: false,
            ..Policy::default()
        };
        (HttpFetcher::new(policy, Arc::clone(&offline)), offline)
    }

    fn got(f: &HttpFetcher, u: &str) -> Result<Vec<u8>, String> {
        f.get(u).map_err(|e| format!("{e:?}"))
    }

    fn url(s: &Server, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", s.port)
    }

    #[test]
    fn the_request_names_the_app_and_sends_no_cookie_or_credential() -> TestResult {
        let server = serve(vec![ok(b"one"), ok(b"two")])?;
        let (fetcher, _) = local(1024);
        assert_eq!(got(&fetcher, &url(&server, "/a"))?, b"one");
        assert_eq!(got(&fetcher, &url(&server, "/b"))?, b"two");
        let seen = server.seen.lock().map_err(|_| "poisoned")?;
        let first = seen.first().ok_or("no request")?.to_ascii_lowercase();
        assert!(first.contains(&format!(
            "user-agent: {}",
            user_agent().to_ascii_lowercase()
        )));
        assert!(user_agent().starts_with("PCBCraft/"));
        for request in seen.iter() {
            let lower = request.to_ascii_lowercase();
            assert!(!lower.contains("cookie:") && !lower.contains("authorization:"));
        }
        Ok(())
    }

    #[test]
    fn the_transport_refuses_an_oversized_body() -> TestResult {
        let server = serve(vec![ok(&[7u8; 64]), {
            // No length header: the cap must hold while streaming.
            let mut r = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
            r.extend_from_slice(&[7u8; 64]);
            r
        }])?;
        let (fetcher, _) = local(16);
        for _ in 0..2 {
            let err = fetcher.get(&url(&server, "/big")).err();
            assert!(
                matches!(&err, Some(FetchError::Failed(m)) if m.contains("size limit")),
                "{err:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn offline_mode_opens_no_connection() -> TestResult {
        let server = serve(vec![ok(b"x")])?;
        let (fetcher, offline) = local(1024);
        offline.store(true, Ordering::SeqCst);
        let err = fetcher.get(&url(&server, "/a")).err();
        assert!(matches!(err, Some(FetchError::Offline(_))));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(server.connections.load(Ordering::SeqCst), 0);
        offline.store(false, Ordering::SeqCst);
        assert_eq!(got(&fetcher, &url(&server, "/a"))?, b"x");
        Ok(())
    }

    #[test]
    fn redirects_to_another_host_are_refused() -> TestResult {
        let away = status("302 Found", "Location: http://localhost:9/steal\r\n");
        let server = serve(vec![away])?;
        let (fetcher, _) = local(1024);
        let err = fetcher.get(&url(&server, "/a")).err();
        assert!(
            matches!(&err, Some(FetchError::Failed(m)) if m.contains("another site")),
            "{err:?}"
        );
        Ok(())
    }

    #[test]
    fn a_redirect_on_the_same_host_is_followed() -> TestResult {
        let hop = status("301 Moved Permanently", "Location: /moved\r\n");
        let server = serve(vec![hop, ok(b"there")])?;
        let (fetcher, _) = local(1024);
        assert_eq!(got(&fetcher, &url(&server, "/a"))?, b"there");
        Ok(())
    }

    #[test]
    fn a_host_off_the_list_is_refused_before_any_socket() -> TestResult {
        let (fetcher, _) = local(1024);
        let err = fetcher.get("http://example.com/a").err();
        assert!(matches!(&err, Some(FetchError::Failed(m)) if m.contains("not one of")));
        let err = fetcher.get("http://user:pw@127.0.0.1/a").err();
        assert!(matches!(err, Some(FetchError::Failed(_))));
        let strict = HttpFetcher::new(Policy::default(), Arc::new(AtomicBool::new(false)));
        let err = strict.get("http://gitlab.com/a").err();
        assert!(matches!(&err, Some(FetchError::Failed(m)) if m.contains("secure")));
        Ok(())
    }

    #[test]
    fn statuses_map_to_plain_errors() -> TestResult {
        let server = serve(vec![
            status("404 Not Found", ""),
            status("429 Too Many Requests", "Retry-After: 7\r\n"),
            status("500 Internal Server Error", ""),
        ])?;
        let (fetcher, _) = local(1024);
        assert_eq!(
            fetcher.get(&url(&server, "/a")).err(),
            Some(FetchError::NotFound)
        );
        assert_eq!(
            fetcher.get(&url(&server, "/b")).err(),
            Some(FetchError::RateLimited {
                retry_after_secs: 7
            })
        );
        assert!(
            matches!(fetcher.get(&url(&server, "/c")).err(), Some(FetchError::Failed(m)) if m.contains("500"))
        );
        Ok(())
    }

    #[test]
    fn a_silent_host_times_out() -> TestResult {
        let server = serve(vec![Vec::new()])?;
        let (fetcher, _) = local(1024);
        let started = Instant::now();
        let err = fetcher.get(&url(&server, "/a")).err();
        assert!(matches!(err, Some(FetchError::Failed(_))), "{err:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
        Ok(())
    }

    #[test]
    fn the_background_call_runs_off_the_calling_thread() -> TestResult {
        let server = serve(vec![ok(b"bg")])?;
        let (fetcher, _) = local(1024);
        let rx = Arc::new(fetcher).get_in_background(&url(&server, "/a"));
        let body = rx
            .recv_timeout(Duration::from_secs(5))?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(body, b"bg");
        Ok(())
    }

    #[test]
    fn calls_are_spaced_by_the_minimum_gap() -> TestResult {
        let server = serve(vec![ok(b"1"), ok(b"2"), ok(b"3")])?;
        let policy = Policy {
            allowed_hosts: vec!["127.0.0.1".into()],
            min_gap: Duration::from_millis(150),
            require_https: false,
            ..Policy::default()
        };
        let fetcher = HttpFetcher::new(policy, Arc::new(AtomicBool::new(false)));
        let started = Instant::now();
        for p in ["/a", "/b", "/c"] {
            got(&fetcher, &url(&server, p))?;
        }
        assert!(started.elapsed() >= Duration::from_millis(300));
        Ok(())
    }

    #[test]
    fn purge_empties_the_cache_directory() -> TestResult {
        let dir = tempfile::tempdir()?;
        let cache = CadCache::open(dir.path());
        cache.put("open-library", "symbols/Device.kicad_sym", None, b"data")?;
        assert!(cache.disk_bytes() > 0);
        purge_cache(dir.path()).map_err(|e| format!("{e:?}"))?;
        assert_eq!(CadCache::open(dir.path()).disk_bytes(), 0);
        assert!(!dir.path().join("files").exists());
        Ok(())
    }

    /// Manual check against the real host; run by hand, never in CI:
    /// `cargo test -p pcbcraft live_ -- --ignored --nocapture`.
    #[test]
    #[ignore = "reaches the live open library host"]
    fn live_search_a_resistor_and_fetch_one_symbol() -> TestResult {
        use pcbcraft_library::cad::{CadKind, CadSource};
        use pcbcraft_library::open_library::OpenLibrarySource;
        let dir = tempfile::tempdir()?;
        let fetcher = HttpFetcher::new(Policy::default(), Arc::new(AtomicBool::new(false)));
        let source = OpenLibrarySource::new(Box::new(fetcher), CadCache::open(dir.path()));
        let found = source
            .search(CadKind::Symbol, "resistor")
            .map_err(|e| format!("{e:?}"))?;
        println!("found {} symbols, first: {:?}", found.len(), found.first());
        let first = found.first().ok_or("no results")?;
        let file = source.fetch(first).map_err(|e| format!("{e:?}"))?;
        println!("fetched {} bytes", file.bytes.len());
        assert!(!file.bytes.is_empty());
        Ok(())
    }
}
