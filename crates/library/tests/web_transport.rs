//! The browser transport against recorded answers. A fake page plays the
//! browser: it takes each address the transport started and replies from the
//! recorded index. Nothing here reaches a live host.
use pcbcraft_library::cad_cache::CadCache;
use pcbcraft_library::open_library::{CardScope, FetchError, Fetcher, OpenLibrarySource};
use pcbcraft_library::sources::SourceError;
use pcbcraft_library::web_transport::{Dispatch, MAX_IN_FLIGHT, WebTransport};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

mod common;
use common::{Boxed, Recorded};

struct Starts(Arc<Mutex<Vec<String>>>);

impl Dispatch for Starts {
    fn start(&self, url: &str) {
        if let Ok(mut v) = self.0.lock() {
            v.push(url.to_owned());
        }
    }
}

struct Page {
    transport: Arc<WebTransport>,
    starts: Arc<Mutex<Vec<String>>>,
    rec: Recorded,
    offline: Arc<AtomicBool>,
}

fn page() -> Result<Page, Boxed> {
    let starts = Arc::new(Mutex::new(Vec::new()));
    let offline = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(WebTransport::new(
        Box::new(Starts(starts.clone())),
        &["gitlab.com"],
        offline.clone(),
    ));
    Ok(Page {
        transport,
        starts,
        rec: Recorded::new()?,
        offline,
    })
}

impl Page {
    /// The browser replies to everything started so far.
    fn reply_to_started(&self) {
        let urls: Vec<String> = self
            .starts
            .lock()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default();
        for url in urls {
            self.transport.answer(&url, self.rec.get(&url));
        }
    }
    /// Ask the way the Library tab does: repeat each frame until nothing is waiting.
    fn search(&self, query: &str) -> Result<Vec<String>, SourceError> {
        let source =
            OpenLibrarySource::new(Box::new(self.transport.clone()), CadCache::in_memory())
                .with_clock(Box::new(|| 1_000));
        for _ in 0..400 {
            let out = source.search_cards(CardScope::Footprints, query);
            if self.transport.waiting() == 0 && self.starts.lock().is_ok_and(|v| v.is_empty()) {
                return out.map(|cards| cards.into_iter().map(|c| c.sku).collect());
            }
            self.reply_to_started();
        }
        Err(SourceError::Unavailable("never settled".into()))
    }
}

#[test]
fn web_transport_serves_recorded_answers() -> Result<(), Boxed> {
    let p = page()?;
    let url = p
        .rec
        .index
        .keys()
        .find(|u| u.contains("raw"))
        .cloned()
        .ok_or("no recorded file")?;
    assert_eq!(p.transport.get(&url), Err(FetchError::Pending));
    assert_eq!(p.transport.waiting(), 1);
    p.reply_to_started();
    let body = p.transport.get(&url).map_err(|e| format!("{e:?}"))?;
    assert_eq!(Ok(body), p.rec.get(&url));
    assert_eq!(p.transport.waiting(), 0);
    let found = p.search("0603")?;
    assert!(
        found
            .iter()
            .any(|s| s.starts_with("footprint:Resistor_SMD/")),
        "the search lists a recorded footprint: {found:?}"
    );
    Ok(())
}

#[test]
fn only_a_few_requests_are_out_at_once() -> Result<(), Boxed> {
    let p = page()?;
    let urls: Vec<String> = p.rec.index.keys().take(5).cloned().collect();
    for u in &urls {
        assert_eq!(p.transport.get(u), Err(FetchError::Pending));
    }
    assert_eq!(p.starts.lock().map(|v| v.len()).unwrap_or(0), MAX_IN_FLIGHT);
    assert_eq!(p.transport.waiting(), 5);
    p.reply_to_started();
    assert_eq!(p.starts.lock().map(|v| v.len()).unwrap_or(0), MAX_IN_FLIGHT);
    Ok(())
}

#[test]
fn web_build_explains_when_cross_origin_reads_are_refused() -> Result<(), Boxed> {
    let p = page()?;
    let url = p.rec.index.keys().next().cloned().ok_or("empty index")?;
    assert_eq!(p.transport.get(&url), Err(FetchError::Pending));
    p.transport.refuse_cross_origin();
    let why = p.transport.blocked().ok_or("no explanation")?;
    assert!(why.contains("does not let the page read"), "{why}");
    match p.transport.get(&url) {
        Err(FetchError::Failed(m)) => assert_eq!(m, why),
        other => return Err(format!("expected the explanation, got {other:?}").into()),
    }
    assert_eq!(p.transport.waiting(), 0);
    Ok(())
}

#[test]
fn offline_starts_nothing_in_the_browser() -> Result<(), Boxed> {
    let p = page()?;
    p.offline.store(true, Ordering::SeqCst);
    let url = p.rec.index.keys().next().cloned().ok_or("empty index")?;
    assert!(matches!(p.transport.get(&url), Err(FetchError::Offline(_))));
    assert!(p.starts.lock().is_ok_and(|v| v.is_empty()));
    Ok(())
}

#[test]
fn addresses_off_the_list_are_refused_before_the_browser_hears() -> Result<(), Boxed> {
    let p = page()?;
    for bad in [
        "http://gitlab.com/x",
        "https://example.org/x",
        "https://user@gitlab.com/x",
        "https://gitlab.com.example.org/x",
    ] {
        assert!(
            matches!(p.transport.get(bad), Err(FetchError::Failed(_))),
            "{bad}"
        );
    }
    assert!(p.starts.lock().is_ok_and(|v| v.is_empty()));
    Ok(())
}
