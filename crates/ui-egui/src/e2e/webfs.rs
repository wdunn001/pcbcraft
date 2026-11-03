//! Stories for Open, Save and Export through the project host (the browser flow).
//!
//! Context: `webfs.rs`. The app runs with `browser` set and the in-memory provider,
//! so bytes stand in for the browser's file picker and downloads. Every story clicks
//! the real File menu entries.
use super::super::*;
use super::support::*;
use pcbcraft_mcp::{MemHost, ProjectHost};
use std::cell::RefCell;
use std::rc::Rc;

/// A file the "user" picked: its name and bytes.
type Picked = (String, Vec<u8>);

/// A provider that shares its queues with the story so it can feed and read them.
#[derive(Clone, Default)]
struct Shared {
    host: Rc<RefCell<MemHost>>,
    /// Files the "user" will choose; they reach the app only once it asks for a picker.
    staged: Rc<RefCell<Vec<Picked>>>,
}

/// Contract: crates/mcp/src/project_host.rs
impl ProjectHost for Shared {
    fn takes_bytes(&self) -> bool {
        true
    }
    fn request_open(&mut self) {
        let staged: Vec<_> = self.staged.borrow_mut().drain(..).collect();
        self.host.borrow_mut().open_queue.extend(staged);
    }
    fn pick_open(&mut self) -> Option<(String, Vec<u8>)> {
        self.host.borrow_mut().pick_open()
    }
    fn save_download(&mut self, filename: &str, bytes: &[u8]) -> Result<(), String> {
        self.host.borrow_mut().save_download(filename, bytes)
    }
}

fn browser_app(host: &Shared) -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None)
        .with_project_host(Box::new(host.clone()));
    app.browser = true;
    app.refresh();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
}

/// The sample board as the bytes of a project file.
fn sample_bytes() -> Result<Vec<u8>, String> {
    pcbcraft_io::save_bytes(
        &pcbcraft_engine::Session::sample()
            .map_err(|e| e.to_string())?
            .project,
    )
}

#[test]
fn a_newcomer_opens_a_design_in_the_browser() -> Result<(), String> {
    let host = Shared::default();
    host.staged
        .borrow_mut()
        .push(("board.pcbcraft".into(), sample_bytes()?));
    let (ctx, mut app) = browser_app(&host);
    let sample = pcbcraft_engine::Session::sample()
        .map_err(|e| e.to_string())?
        .project;
    assert_ne!(app.project, sample, "starts on a different design");
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Open…")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(app.project, sample, "the parts appear: {}", app.message);
    assert!(
        app.message.contains("Opened board.pcbcraft"),
        "{}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_when_a_file_is_not_a_design() -> Result<(), String> {
    let host = Shared::default();
    host.staged
        .borrow_mut()
        .push(("junk.pcbcraft".into(), b"not json".to_vec()));
    let (ctx, mut app) = browser_app(&host);
    let before = app.revision;
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Open…")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.message.contains("Could not open junk.pcbcraft"),
        "{}",
        app.message
    );
    assert_eq!(app.revision, before, "the design is untouched");
    Ok(())
}

#[test]
fn a_newcomer_saves_a_design_as_a_download() -> Result<(), String> {
    let host = Shared::default();
    let (ctx, mut app) = browser_app(&host);
    app.tool_call("sample_design", json!({}));
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Save")?;
    let downloads = host.host.borrow().downloads.clone();
    let (name, bytes) = downloads.last().ok_or("nothing was downloaded")?;
    assert_eq!(name, "design.pcbcraft");
    match pcbcraft_io::open_document(bytes, name)? {
        pcbcraft_io::OpenKind::Project(project) => {
            assert_eq!(*project, app.project, "the file reopens equal");
        }
        _ => return Err("the download was not a project".into()),
    }
    Ok(())
}

#[test]
fn a_newcomer_downloads_a_bill_of_materials_in_the_browser() -> Result<(), String> {
    let host = Shared::default();
    let (ctx, mut app) = browser_app(&host);
    app.tool_call("sample_design", json!({}));
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    authoring::tests::tap(&ctx, &mut app, "Bill of materials")?;
    authoring::tests::tap(&ctx, &mut app, "Download")?;
    let downloads = host.host.borrow().downloads.clone();
    let (name, bytes) = downloads.last().ok_or("nothing was downloaded")?;
    assert!(name.ends_with(".csv"), "a table file: {name}");
    assert!(!bytes.is_empty(), "the file has rows: {}", app.message);
    Ok(())
}
