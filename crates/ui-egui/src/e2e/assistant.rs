//! Stories for the Assistant pane, driven with a fake model (no network).
use super::super::*;
use super::support::*;
use pcbcraft_assistant::{ChatBackend, Event, PromptPart, ToolRequest};
use std::sync::mpsc::{Receiver, Sender, channel};

/// A model that answers any message with one request to add a 10k resistor.
struct FakeModel {
    tools: Sender<ToolRequest>,
    calls: Vec<(String, Value)>,
    events: Vec<Event>,
    answers: Vec<Receiver<Value>>,
    /// Text sent as the last words of every message, after its tool calls are answered.
    closing: Option<String>,
}

impl FakeModel {
    fn new(tools: Sender<ToolRequest>, calls: Vec<(String, Value)>) -> Self {
        Self {
            tools,
            calls,
            events: vec![Event::Ready {
                agent: "fake".into(),
                images: false,
            }],
            answers: Vec::new(),
            closing: None,
        }
    }
}

impl ChatBackend for FakeModel {
    fn poll(&mut self) -> Vec<Event> {
        let mut out = std::mem::take(&mut self.events);
        let before = self.answers.len();
        self.answers.retain(|answer| answer.try_recv().is_err());
        if before > 0 && self.answers.is_empty() {
            if let Some(text) = self.closing.take() {
                out.push(Event::Text {
                    thought: false,
                    text,
                });
            }
            out.push(Event::TurnEnded {
                reason: String::new(),
            });
        }
        out
    }
    fn prompt(&mut self, _: &[PromptPart]) {
        for (name, args) in self.calls.clone() {
            let (reply, answer) = channel();
            let _ = self.tools.send(ToolRequest { name, args, reply });
            self.answers.push(answer);
        }
        self.events.push(Event::Text {
            thought: false,
            text: "I will add the resistor.".into(),
        });
    }
    fn cancel(&mut self) {}
    fn new_session(&mut self) {}
    fn answer_permission(&mut self, _: u64, _: Option<&str>) {}
    fn authenticate(&mut self, _: &str) {}
    fn set_model(&mut self, _: &str) {}
    fn set_mode(&mut self, _: &str) {}
    fn shutdown(&mut self, _: bool) {}
}

fn add_part(reference: &str, value: &str, x: f64) -> (String, Value) {
    (
        "execute".into(),
        json!({"command":"component.add","params":{
            "kind":"resistor","reference":reference,"value":value,"position":{"x":x,"y":20.0}}}),
    )
}

fn with_fake_model() -> App {
    with_calls(vec![add_part("R1", "10k", 20.0)])
}

fn with_calls(calls: Vec<(String, Value)>) -> App {
    with_calls_in(calls, &pcbcraft_engine::user_config::UserConfig::detached())
}

/// The fake-model window keeping its settings (and so its saved
/// conversation) in `config`.
fn with_calls_in(
    calls: Vec<(String, Value)>,
    config: &pcbcraft_engine::user_config::UserConfig,
) -> App {
    let shared = super::support::session_with(config);
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.preferences.aside_lower_tab = 1;
    let model = FakeModel::new(
        app.assistant
            .start_context(std::path::PathBuf::new())
            .tools
            .unwrap_or_else(|| channel().0),
        calls,
    );
    app.assistant_use(Box::new(model));
    app
}

fn has_r1(app: &App) -> bool {
    app.project.components.iter().any(|c| c.reference == "R1")
}

#[test]
fn a_newcomer_opens_the_assistant_and_is_told_how_to_connect_a_model() -> Result<(), String> {
    let http = pcbcraft_assistant::CountingHttp::new(Ok((200, b"{}".to_vec())));
    let mut app = app_with_http(&http, false);
    app.preferences.aside_lower_tab = 0;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Assistant")?;
    assert!(paints_text(&ctx, &mut app, "No model is connected")?);
    assert!(paints_text(&ctx, &mut app, "Nothing leaves this computer")?);
    Ok(())
}

#[test]
fn a_newcomer_asks_the_assistant_to_add_a_resistor() -> Result<(), String> {
    let mut app = with_fake_model();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.assistant.input = "add a 10k resistor".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "Add resistor R1 (10k)")?,
        "the card names the change"
    );
    assert!(!has_r1(&app), "nothing runs before the user answers");
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(has_r1(&app), "the part is on the sheet");
    assert!(
        paints_text(&ctx, &mut app, "Ready")?,
        "the model finished its turn"
    );
    Ok(())
}

#[test]
fn a_newcomer_denies_a_change_and_the_design_stays_as_it_was() -> Result<(), String> {
    let mut app = with_fake_model();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.assistant.input = "add a 10k resistor".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::tap(&ctx, &mut app, "Discard")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(!has_r1(&app));
    assert!(paints_text(&ctx, &mut app, "Discarded")?);
    Ok(())
}

fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

#[test]
fn a_newcomer_allows_two_changes_and_takes_them_back_with_one_undo() -> Result<(), String> {
    let mut app = with_calls(vec![
        add_part("R1", "10k", 20.0),
        add_part("R2", "4k7", 30.0),
    ]);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add two resistors".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    assert!(paints_text(&ctx, &mut app, "Add resistor R1 (10k)")?);
    assert!(
        paints_text(&ctx, &mut app, "Assistant (2)")?,
        "the tab counts the two questions waiting"
    );
    assert!(
        paints_text(&ctx, &mut app, "Parts added")?,
        "the card previews the change"
    );
    for _ in 0..2 {
        authoring::tests::tap(&ctx, &mut app, "Apply")?;
        settle(&ctx, &mut app);
    }
    assert!(has_r1(&app));
    assert_eq!(app.project.components.len(), 2);
    assert!(
        app.assistant
            .transcript
            .items
            .iter()
            .any(|i| matches!(i, pcbcraft_assistant::Item::UndoStep { .. })),
        "the conversation ends with an undo row: {:?}",
        app.assistant.transcript.items
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Undo")?,
        "the Undo button is on screen"
    );
    authoring::tests::tap(&ctx, &mut app, "Undo")?;
    settle(&ctx, &mut app);
    assert!(
        app.project.components.is_empty(),
        "one Undo takes back both parts"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_a_malformed_change_refused_with_the_reason() -> Result<(), String> {
    let bad = (
        "execute".to_string(),
        json!({"command":"component.add","params":{"kind":"resistor","position":"nowhere"}}),
    );
    let mut app = with_calls(vec![bad]);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add a resistor".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    assert!(
        !paints_text(&ctx, &mut app, "Apply")?,
        "a refused change has no Apply button"
    );
    assert!(app.project.components.is_empty());
    assert!(
        app.assistant
            .transcript
            .items
            .iter()
            .any(|i| matches!(i, pcbcraft_assistant::Item::Note { error: true, .. })),
        "the reason is in the conversation"
    );
    Ok(())
}

/// A server stand-in that lists two models and counts how often it is asked.
fn fake_http(calls: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> pcbcraft_assistant::HttpFn {
    std::sync::Arc::new(move |_| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok((
            200,
            br#"{"data":[{"id":"small-model"},{"id":"big-model"}]}"#.to_vec(),
        ))
    })
}

/// A keychain that remembers keys in memory.
#[derive(Default)]
struct FakeKeychain(std::sync::Mutex<std::collections::BTreeMap<String, String>>);

impl pcbcraft_assistant::Secrets for FakeKeychain {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        let map = self.0.lock().map_err(|e| e.to_string())?;
        Ok(map.get(id).cloned())
    }
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        let mut map = self.0.lock().map_err(|e| e.to_string())?;
        map.insert(id.into(), secret.into());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        let mut map = self.0.lock().map_err(|e| e.to_string())?;
        map.remove(id);
        Ok(())
    }
}

fn settings_app(
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    keys: std::sync::Arc<FakeKeychain>,
) -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None).with_host(Host {
        http: Some(fake_http(calls)),
        secrets: Some(keys),
    });
    app.refresh();
    app.assistant.settings_open = true;
    app.change_tab(Tab::Settings);
    app
}

#[test]
fn a_newcomer_adds_a_local_model_server_and_tests_it() -> Result<(), String> {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut app = settings_app(calls.clone(), std::sync::Arc::new(FakeKeychain::default()));
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    // Fold the section away and open it again, as a first-time visitor would.
    authoring::tests::tap(&ctx, &mut app, "v Assistant settings")?;
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "> Assistant settings")?;
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Add connection")?;
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Test")?;
    for _ in 0..30 {
        settle(&ctx, &mut app);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        paints_text(&ctx, &mut app, "2 models")?,
        "the test result is shown"
    );
    authoring::tests::tap(&ctx, &mut app, "Add")?;
    settle(&ctx, &mut app);
    assert_eq!(app.preferences.assistant_connections.len(), 1);
    assert_eq!(
        app.preferences.assistant_default, app.preferences.assistant_connections[0].id,
        "the first connection is the one in use"
    );
    assert!(
        paints_text(&ctx, &mut app, "Local model server")?,
        "the row is listed"
    );
    Ok(())
}

#[test]
fn a_newcomer_cannot_type_a_key_for_a_plain_http_server_on_another_computer() -> Result<(), String>
{
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut app = settings_app(calls, std::sync::Arc::new(FakeKeychain::default()));
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Add connection")?;
    settle(&ctx, &mut app);
    if let Some(d) = app.assistant.dialog.as_mut() {
        d.set_url("http://10.0.0.5:1234/v1");
    }
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "only sent over https")?,
        "the reason is painted"
    );
    assert!(!paints_text(&ctx, &mut app, "Kept in the system keychain")?);
    Ok(())
}

#[test]
fn a_key_is_stored_in_the_keychain_and_never_in_the_preferences_file() -> Result<(), String> {
    let keys = std::sync::Arc::new(FakeKeychain::default());
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut app = settings_app(calls, keys.clone());
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Add connection")?;
    settle(&ctx, &mut app);
    if let Some(d) = app.assistant.dialog.as_mut() {
        d.set_key("sk-secret-123");
    }
    authoring::tests::tap(&ctx, &mut app, "Add")?;
    settle(&ctx, &mut app);
    let text = serde_json::to_string(&app.preferences).map_err(|e| e.to_string())?;
    assert!(
        !text.contains("sk-secret-123"),
        "the key is not in the preferences"
    );
    assert!(text.contains("\"has_key\":true"));
    let id = app.preferences.assistant_connections[0].id.clone();
    assert_eq!(
        pcbcraft_assistant::Secrets::get(&*keys, &pcbcraft_assistant::key_id(&id))?,
        Some("sk-secret-123".to_string())
    );
    assistant::settings::remove_connection(&mut app, &id);
    assert_eq!(
        pcbcraft_assistant::Secrets::get(&*keys, &pcbcraft_assistant::key_id(&id))?,
        None,
        "remove deletes the keychain entry"
    );
    Ok(())
}

#[test]
fn with_the_assistant_switched_off_nothing_is_probed() -> Result<(), String> {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut app = settings_app(calls.clone(), std::sync::Arc::new(FakeKeychain::default()));
    app.preferences.assistant_enabled = false;
    app.preferences
        .assistant_connections
        .push(pcbcraft_assistant::Connection {
            id: "api-1".into(),
            name: "Home".into(),
            protocol: pcbcraft_assistant::Protocol::OpenAi,
            base_url: "http://localhost:11434/v1".into(),
            model: "m".into(),
            has_key: false,
            images: false,
        });
    app.preferences.assistant_default = "api-1".into();
    let ctx = egui::Context::default();
    for _ in 0..10 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(!assistant::shown(&app));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn a_newcomer_selects_a_part_and_the_assistant_already_knows_which_one() -> Result<(), String> {
    let session = pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?;
    let shared = pcbcraft_mcp::shared(session);
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.preferences.aside_lower_tab = 1;
    let script = pcbcraft_assistant::ScriptBackend::default();
    script.report(Event::Ready {
        agent: "script".into(),
        images: false,
    });
    app.assistant_use(Box::new(script.clone()));
    let r1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .map(|c| c.id)
        .ok_or("the sample has R1")?;
    app.selected = Some(r1);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "move this part right by 5 mm".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    let prompts = script.prompts();
    let first = prompts.first().ok_or("a prompt was sent")?;
    let Some(pcbcraft_assistant::PromptPart::Text(text)) = first.first() else {
        return Err("the prompt starts with text".into());
    };
    assert!(text.contains("Selected: ") && text.contains("R1"), "{text}");
    assert!(
        text.ends_with("move this part right by 5 mm"),
        "the user's words come last"
    );
    Ok(())
}

/// A design path no other test uses, so the saved conversation is ours alone.
fn own_design_path(tag: &str) -> String {
    format!("/tmp/pcbcraft-story-{}-{tag}.pcbcraft", std::process::id())
}

fn forget_file(config: &pcbcraft_engine::user_config::UserConfig, path: &str) {
    if let Some(file) = pcbcraft_assistant::transcript_path(config, Some(path)) {
        pcbcraft_assistant::clear_transcript(&file);
    }
}

#[test]
fn a_newcomer_closes_the_app_and_finds_the_conversation_where_she_left_it() -> Result<(), String> {
    let path = own_design_path("reopen");
    let (config, folder) = super::support::private_config("reopen")?;
    let mut app = with_calls_in(vec![add_part("R1", "10k", 20.0)], &config);
    app.path = path.clone();
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add a 10k resistor".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    settle(&ctx, &mut app);
    app.assistant_save();
    drop(app);

    let shared = super::support::session_with(&config);
    let mut reopened = App::create(shared, Preferences::default(), None, None)
        .with_project_path(Some(path.clone()));
    reopened.refresh();
    reopened.preferences.aside_lower_tab = 1;
    let found = paints_text(&ctx, &mut reopened, "add a 10k resistor")?;
    forget_file(&config, &path);
    let _ = std::fs::remove_dir_all(&folder);
    assert!(found, "the earlier message is on screen again");
    Ok(())
}

#[test]
fn a_newcomer_forgets_a_conversation_and_the_design_stays() -> Result<(), String> {
    let path = own_design_path("forget");
    let (config, folder) = super::support::private_config("forget")?;
    let mut app = with_calls_in(vec![add_part("R1", "10k", 20.0)], &config);
    app.path = path.clone();
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add a 10k resistor".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    settle(&ctx, &mut app);
    app.assistant_save();
    let revision = app.revision;
    authoring::tests::tap(&ctx, &mut app, "Clear")?;
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "Forget this conversation?")?,
        "she is asked first"
    );
    authoring::tests::tap(&ctx, &mut app, "Forget")?;
    settle(&ctx, &mut app);
    let file = pcbcraft_assistant::transcript_path(&config, Some(&path));
    let gone = file.as_ref().is_some_and(|f| !f.exists());
    forget_file(&config, &path);
    let _ = std::fs::remove_dir_all(&folder);
    assert!(app.assistant.transcript.items.is_empty());
    assert!(gone, "the saved file is gone too");
    assert_eq!(app.revision, revision, "the design did not change");
    Ok(())
}

fn local_connection() -> pcbcraft_assistant::Connection {
    pcbcraft_assistant::Connection {
        id: "api-1".into(),
        name: "Local model".into(),
        protocol: pcbcraft_assistant::Protocol::OpenAi,
        base_url: "http://localhost:11434/v1".into(),
        model: "small-model".into(),
        has_key: false,
        images: false,
    }
}

fn app_with_http(http: &pcbcraft_assistant::CountingHttp, connected: bool) -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None).with_host(Host {
        http: Some(http.http()),
        secrets: None,
    });
    app.refresh();
    app.preferences.aside_lower_tab = 1;
    if connected {
        app.preferences
            .assistant_connections
            .push(local_connection());
        app.preferences.assistant_default = "api-1".into();
    }
    app
}

fn wait_for(ctx: &egui::Context, app: &mut App, text: &str) -> Result<bool, String> {
    for _ in 0..60 {
        if paints_text(ctx, app, text)? {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Ok(false)
}

#[test]
fn a_newcomer_opens_the_assistant_and_learns_how_to_connect_it() -> Result<(), String> {
    let http = pcbcraft_assistant::CountingHttp::new(Ok((200, b"{}".to_vec())));
    let mut app = app_with_http(&http, false);
    let ctx = egui::Context::default();
    assert!(paints_text(&ctx, &mut app, "Nothing leaves this computer")?);
    authoring::tests::tap(&ctx, &mut app, "Open Settings")?;
    settle(&ctx, &mut app);
    assert!(
        scrolled_to_at(&ctx, &mut app, "v Assistant settings", 700.0)?,
        "the Assistant section is on screen"
    );
    for _ in 0..20 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(http.calls(), 0, "nothing was sent");
    Ok(())
}

#[test]
fn a_newcomer_works_offline_with_a_local_model_and_nothing_complains() -> Result<(), String> {
    let reply = br#"{"choices":[{"message":{"role":"assistant","content":"Hello from the local model"},"finish_reason":"stop"}]}"#;
    let http = pcbcraft_assistant::CountingHttp::new(Ok((200, reply.to_vec())));
    let mut app = app_with_http(&http, true);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "hello".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    assert!(
        wait_for(&ctx, &mut app, "Hello from the local model")?,
        "the answer arrives"
    );
    assert!(
        !app.assistant
            .transcript
            .items
            .iter()
            .any(|i| matches!(i, pcbcraft_assistant::Item::Note { error: true, .. })),
        "no error text"
    );
    Ok(())
}

#[test]
fn a_newcomer_loses_the_endpoint_and_gets_a_plain_reason_and_a_retry_button() -> Result<(), String>
{
    let http = pcbcraft_assistant::CountingHttp::new(Err("connection refused".into()));
    let mut app = app_with_http(&http, true);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "hello".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    assert!(
        wait_for(&ctx, &mut app, "connection refused")?,
        "the reason is shown"
    );
    assert!(
        wait_for(&ctx, &mut app, "Retry")?,
        "a Retry button is offered"
    );
    let before = http.calls();
    authoring::tests::tap(&ctx, &mut app, "Retry")?;
    for _ in 0..60 {
        settle(&ctx, &mut app);
        if http.calls() > before {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(http.calls() > before, "Retry sent the message again");
    Ok(())
}

#[test]
fn a_newcomer_presses_stop_and_the_assistant_stops_within_a_frame() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.preferences.aside_lower_tab = 1;
    let script = pcbcraft_assistant::ScriptBackend::default();
    script.report(Event::Ready {
        agent: "script".into(),
        images: false,
    });
    app.assistant_use(Box::new(script.clone()));
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "first".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    app.assistant.input = "second".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Stop")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        script.calls().iter().any(|c| c == "cancel"),
        "the model was told to stop"
    );
    assert!(
        paints_text(&ctx, &mut app, "Stopped")?,
        "the state word reads Stopped"
    );
    assert!(
        app.assistant
            .transcript
            .items
            .iter()
            .any(|i| matches!(i, pcbcraft_assistant::Item::Queued { text } if text == "second")),
        "the queued message stays"
    );
    Ok(())
}

#[test]
fn a_newcomer_in_the_browser_build_is_told_the_assistant_needs_the_desktop_app()
-> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app =
        App::create(shared, Preferences::default(), None, None).with_host(Host::default());
    app.refresh();
    app.preferences.aside_lower_tab = 1;
    let ctx = egui::Context::default();
    assert!(paints_text(&ctx, &mut app, "needs the desktop app")?);
    app.assistant.input = "hello".into();
    app.assistant_send();
    assert!(
        app.assistant.transcript.items.is_empty(),
        "the composer sends nothing"
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_the_assistant_off_and_every_trace_of_it_goes() -> Result<(), String> {
    let http = pcbcraft_assistant::CountingHttp::new(Ok((200, b"{}".to_vec())));
    let mut app = app_with_http(&http, true);
    app.preferences.aside_lower_tab = 0;
    app.preferences.assistant_enabled = false;
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    assert!(!paints_text(&ctx, &mut app, "Assistant")?, "no tab");
    let shortcut = egui::Modifiers {
        command: true,
        ctrl: true,
        shift: true,
        ..Default::default()
    };
    let key = egui::Event::Key {
        key: egui::Key::A,
        physical_key: Some(egui::Key::A),
        pressed: true,
        repeat: false,
        modifiers: shortcut,
    };
    frame(&ctx, &mut app, vec![key], shortcut);
    assert_eq!(
        app.preferences.aside_lower_tab, 0,
        "the shortcut does nothing"
    );
    assert_eq!(http.calls(), 0);
    Ok(())
}

fn with_closing(calls: Vec<(String, Value)>, closing: &str) -> App {
    let mut app = with_calls(calls);
    let shared = app.assistant.start_context(std::path::PathBuf::new());
    let mut model = FakeModel::new(
        shared.tools.unwrap_or_else(|| channel().0),
        vec![("design.summary".into(), json!({}))],
    );
    model.closing = Some(closing.to_string());
    app.assistant_use(Box::new(model));
    app
}

fn batch_of(commands: Vec<Value>) -> (String, Value) {
    ("batch".into(), json!({ "commands": commands }))
}

fn add_command(reference: &str, x: f64) -> Value {
    json!({"command":"component.add","params":{
        "kind":"resistor","reference":reference,"value":"10k","position":{"x":x,"y":20.0}}})
}

#[test]
fn a_newcomer_watches_the_assistant_steps_and_expands_one() -> Result<(), String> {
    let mut app = with_calls(vec![
        ("checks".into(), json!({})),
        add_part("R1", "10k", 20.0),
    ]);
    app.preferences.assistant_mode = pcbcraft_assistant::Mode::Edits;
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add a 10k resistor and check".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    assert!(has_r1(&app), "edits are allowed in this mode");
    assert!(
        paints_text(&ctx, &mut app, "Ran 2 steps in under 1 s")?,
        "a finished run is one line"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Placing")?,
        "the steps stay folded until opened"
    );
    authoring::tests::tap(&ctx, &mut app, "Ran 2 steps in under 1 s")?;
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "Placing")?,
        "the step has a plain name"
    );
    assert!(paints_text(&ctx, &mut app, "Checking")?);
    assert!(
        paints_text(&ctx, &mut app, "Added 1 part")?,
        "the step says what came of it"
    );
    assert!(
        !paints_text(&ctx, &mut app, "component.add")?,
        "command names stay behind Show commands"
    );
    authoring::tests::tap(&ctx, &mut app, "Show commands")?;
    settle(&ctx, &mut app);
    assert!(paints_text(&ctx, &mut app, "component.add")?);
    Ok(())
}

#[test]
fn a_newcomer_reviews_the_change_summary_and_applies_it() -> Result<(), String> {
    let mut app = with_calls(vec![batch_of(vec![
        add_command("R1", 20.0),
        json!({"command":"net.create","params":{"name":"VCC"}}),
    ])]);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add a resistor and a net".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    for row in [
        "Review the changes",
        "Parts added",
        "Nets added",
        "Tracks added",
        "Vias added",
        "Objects untouched",
        "Size changed",
        "Apply",
        "Apply step by step",
        "Discard",
    ] {
        assert!(paints_text(&ctx, &mut app, row)?, "the card shows {row}");
    }
    assert!(!has_r1(&app), "nothing is on the board before Apply");
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    settle(&ctx, &mut app);
    assert!(has_r1(&app));
    assert!(
        paints_text(&ctx, &mut app, "Applied")?,
        "the review card folds to Applied"
    );
    assert!(
        paints_text(&ctx, &mut app, "1 change made")?,
        "the undo card says what was made"
    );
    authoring::tests::tap(&ctx, &mut app, "Undo")?;
    settle(&ctx, &mut app);
    assert!(!has_r1(&app), "one Undo takes it back");
    assert!(paints_text(
        &ctx,
        &mut app,
        "Undone. The board is back as it was."
    )?);
    Ok(())
}

#[test]
fn a_newcomer_applies_a_batch_step_by_step_and_can_stop() -> Result<(), String> {
    let mut app = with_calls(vec![batch_of(vec![
        add_command("R1", 20.0),
        add_command("R2", 30.0),
    ])]);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add two resistors".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    authoring::tests::tap(&ctx, &mut app, "Apply step by step")?;
    settle(&ctx, &mut app);
    assert!(
        !has_r1(&app),
        "nothing runs until the first command is applied"
    );
    assert!(app.assistant.waiting() == 1, "one question waits");
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    settle(&ctx, &mut app);
    assert!(has_r1(&app));
    assert_eq!(
        app.project.components.len(),
        1,
        "only the first command ran"
    );
    authoring::tests::tap(&ctx, &mut app, "Discard")?;
    settle(&ctx, &mut app);
    assert_eq!(
        app.project.components.len(),
        1,
        "stopping keeps what was applied"
    );
    assert_eq!(app.assistant.waiting(), 0);
    Ok(())
}

#[test]
fn a_newcomer_picks_a_next_step_card() -> Result<(), String> {
    let mut app = with_closing(
        vec![],
        "All set.\nNext (recommended): Check the board | Run every rule check.\nNext: Add a capacitor | A 10 uF part steadies the input.",
    );
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "what now".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    assert!(paints_text(&ctx, &mut app, "WHAT NEXT")?);
    assert!(paints_text(&ctx, &mut app, "Check the board")?);
    assert!(
        paints_text(&ctx, &mut app, "Recommended")?,
        "one card is the assistant's pick"
    );
    assert!(paints_text(&ctx, &mut app, "Add a capacitor")?);
    assert!(
        !paints_text(
            &ctx,
            &mut app,
            "Next: Add a capacitor | A 10 uF part steadies the input."
        )?,
        "the raw lines do not show"
    );
    assert!(
        paints_text(&ctx, &mut app, "Or tell the assistant what you want next")?,
        "the composer sits below the cards"
    );
    authoring::tests::tap(&ctx, &mut app, "Add a capacitor")?;
    settle(&ctx, &mut app);
    let last_user = app
        .assistant
        .transcript
        .items
        .iter()
        .rev()
        .find_map(|i| match i {
            pcbcraft_assistant::Item::User { text } => Some(text.clone()),
            _ => None,
        });
    assert_eq!(last_user.as_deref(), Some("Add a capacitor"));
    Ok(())
}
