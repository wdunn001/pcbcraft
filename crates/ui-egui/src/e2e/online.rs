//! Stories for the online part transports: the line about what is kept on this
//! computer, offline mode, and the browser transport. Every answer comes from
//! the recordings next to the library crate's tests; no story reaches a host.
use super::super::*;
use super::support::*;
use pcbcraft_library::cad_cache::CacheStats;
use pcbcraft_library::open_library::{FetchError, Fetcher};
use pcbcraft_library::web_transport::{Dispatch, WebTransport};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn library_app() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    app
}

struct Started(Arc<Mutex<Vec<String>>>);

/// Contract: crates/library/src/web_transport.rs
impl Dispatch for Started {
    fn start(&self, url: &str) {
        if let Ok(mut v) = self.0.lock() {
            v.push(url.to_owned());
        }
    }
}

#[test]
fn a_newcomer_sees_how_big_the_saved_parts_are_and_how_old() {
    let app = library_app();
    let none = CacheStats {
        cap: 512 * 1024 * 1024,
        ..CacheStats::default()
    };
    assert_eq!(
        app.cache_line(&none, 1_000),
        "Kept on this computer: 0 KB of 512.0 MB. Nothing is saved yet."
    );
    let three_days = CacheStats {
        files: 4,
        bytes: 3 * 1024 * 1024 + 200 * 1024,
        cap: 512 * 1024 * 1024,
        listings: 2,
        oldest_listing: Some(1_000),
    };
    let line = app.cache_line(&three_days, 1_000 + 3 * 86_400 + 5);
    assert!(line.contains("3.2 MB of 512.0 MB"), "{line}");
    assert!(line.contains("3 days old"), "{line}");
    let hours = app.cache_line(&three_days, 1_000 + 5 * 3600);
    assert!(hours.contains("5 hours old"), "{hours}");
    let fresh = app.cache_line(&three_days, 1_000 + 60);
    assert!(fresh.contains("under an hour"), "{fresh}");
}

#[test]
fn a_newcomer_goes_offline_and_is_told_only_saved_parts_are_shown() -> Result<(), String> {
    let mut app = library_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Schematic);
    use_recorded_library(&mut app, "offline-story")?;
    app.part_search = "resistor".into();
    assert!(
        waits_for_text(&ctx, &mut app, "R_Small")?,
        "online, the list shows the parts: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Kept on this computer")?,
        "the list says what the cache holds"
    );
    // Offline: the transport can no longer be reached, only the saved copies answer.
    let flag = Arc::new(AtomicBool::new(true));
    let dir = std::env::temp_dir().join(format!(
        "pcbcraft-open-{}-offline-story",
        std::process::id()
    ));
    app.use_open_source(Box::new(sources::CacheOnly), Some(dir));
    app.part_sources_offline_for_test(flag.clone());
    app.part_search = "resistor".into();
    assert!(
        waits_for_text(&ctx, &mut app, "R_Small")?,
        "offline, parts saved before still list: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Offline: only parts saved on this computer")?,
        "offline says so in plain words"
    );
    assert!(flag.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn a_newcomer_uses_the_online_source_in_the_browser_build() -> Result<(), String> {
    let starts = Arc::new(Mutex::new(Vec::new()));
    let offline = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(WebTransport::new(
        Box::new(Started(starts.clone())),
        &["gitlab.com"],
        offline.clone(),
    ));
    let mut app = library_app().with_web_transport(transport.clone(), offline);
    let recorded = RecordedLibrary::load()?;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Schematic);
    app.part_search = "resistor".into();
    let mut seen = false;
    for _ in 0..600 {
        // The page plays the browser: reply to what the transport asked for.
        let asked: Vec<String> = starts
            .lock()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default();
        for url in asked {
            let reply: Result<Vec<u8>, FetchError> = recorded.get(&url);
            transport.answer(&url, reply);
        }
        if paints_text(&ctx, &mut app, "R_Small")? {
            seen = true;
            break;
        }
    }
    assert!(
        seen,
        "the browser build lists parts from recorded answers: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_why_online_parts_are_off_when_the_browser_refuses() -> Result<(), String> {
    let starts = Arc::new(Mutex::new(Vec::new()));
    let offline = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(WebTransport::new(
        Box::new(Started(starts)),
        &["gitlab.com"],
        offline.clone(),
    ));
    let mut app = library_app().with_web_transport(transport.clone(), offline);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Schematic);
    app.part_search = "resistor".into();
    transport.refuse_cross_origin();
    assert!(
        paints_text(&ctx, &mut app, "does not let the page read")?,
        "a refused host is explained, not shown as an empty list"
    );
    Ok(())
}
