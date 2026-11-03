//! Helpers shared by every headless UI story and by the unit tests in `lib.rs`.

use super::super::*;
use pcbcraft_library::open_library::{FetchError, Fetcher};
use std::collections::BTreeMap;

/// Answers the open library from the recordings kept beside the library
/// crate's own tests, so a story can search real part names with no network.
///
/// Contract: crates/library/src/open_library.rs
pub(crate) struct RecordedLibrary {
    index: BTreeMap<String, String>,
}

impl RecordedLibrary {
    fn folder() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/open_library")
    }

    /// Load the recorded index of URLs and answer files.
    pub(crate) fn load() -> Result<Self, String> {
        let text = std::fs::read_to_string(Self::folder().join("index.json"))
            .map_err(|e| e.to_string())?;
        let index = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok(Self { index })
    }
}

/// Contract: crates/library/src/open_library.rs
impl Fetcher for RecordedLibrary {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let file = self
            .index
            .get(url)
            .ok_or_else(|| FetchError::Failed(format!("not recorded: {url}")))?;
        std::fs::read(Self::folder().join(file)).map_err(|e| FetchError::Failed(e.to_string()))
    }
}

/// Switch the open library on over the recordings and keep its files in a
/// folder of its own, as pressing "Turn on Open library" does with a live
/// connection.
pub(crate) fn use_recorded_library(app: &mut App, name: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("pcbcraft-open-{}-{name}", std::process::id()));
    app.use_open_source(Box::new(RecordedLibrary::load()?), Some(dir));
    Ok(())
}

/// How long a story gives the library worker thread to answer before it
/// gives up. A busy machine can starve that thread for many seconds.
const WORKER_ANSWER_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

/// Draw frames, sleeping a little between them, until `done` holds or a
/// minute has passed. Returns whether `done` held.
///
/// The library browser answers on a worker thread, so a story that acts and
/// then looks at the next frame races that thread whenever the machine is
/// busy. This waits for the app state itself instead of counting frames. The
/// limit is a clock deadline, so slow frames do not stretch it.
///
/// # Rules
/// - Draws at least one frame before it first asks `done`.
/// - Sleeps 25 ms between frames so the worker thread gets the processor.
///
/// # Dependencies
/// - crates/ui-egui/src/e2e/support.rs (frame, the one frame driver)
/// - crates/clock/src/lib.rs (instant_now, the deadline clock)
///
/// # Do not
/// - Use it to hide a result that never arrives; the caller still asserts.
pub(crate) fn waits_until(
    ctx: &egui::Context,
    app: &mut App,
    mut done: impl FnMut(&mut App) -> bool,
) -> bool {
    let started = pcbcraft_clock::instant_now();
    loop {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
        if done(app) {
            return true;
        }
        if started.elapsed() > WORKER_ANSWER_LIMIT {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Draw frames until `needle` is on screen, giving a worker thread up to
/// a minute to answer.
pub(crate) fn waits_for_text(
    ctx: &egui::Context,
    app: &mut App,
    needle: &str,
) -> Result<bool, String> {
    let started = pcbcraft_clock::instant_now();
    loop {
        if paints_text(ctx, app, needle)? {
            return Ok(true);
        }
        if started.elapsed() > WORKER_ANSWER_LIMIT {
            return Ok(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Draw frames until the control called `id` is painted and switched on, so
/// a story never presses or types into a control the worker has not yet
/// let the page draw. Fails with the control's name after a minute.
pub(crate) fn waits_for_control(
    ctx: &egui::Context,
    app: &mut App,
    id: &str,
) -> Result<(), String> {
    if waits_until(ctx, app, |app| on_screen(ctx, app, id)) {
        Ok(())
    } else {
        Err(format!("{id} never appeared on screen"))
    }
}

/// True when the named words appear anywhere on screen this frame or the
/// next two, so tests can assert what a person actually sees.
pub(crate) fn paints_text(
    ctx: &egui::Context,
    app: &mut App,
    needle: &str,
) -> Result<bool, String> {
    for _ in 0..3 {
        let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
        if output.shapes.iter().any(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                return text.galley.text().contains(needle);
            }
            false
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Default width of the left aside (`browser` panel) beside the canvas.
pub(crate) const LEFT_ASIDE: f32 = 320.0;

/// Scroll the side docks until `needle` appears: the layers tree on the
/// left first, then the Properties column on the right.
pub(crate) fn scrolled_to(
    ctx: &egui::Context,
    app: &mut App,
    needle: &str,
) -> Result<bool, String> {
    if scrolled_to_at(ctx, app, needle, 150.0)? {
        return Ok(true);
    }
    scrolled_to_at(ctx, app, needle, 1170.0)
}

/// Scroll the left aside (Layers tree or Library tab) until `needle` is visible.
pub(crate) fn scrolled_to_left(
    ctx: &egui::Context,
    app: &mut App,
    needle: &str,
) -> Result<bool, String> {
    scrolled_to_at(ctx, app, needle, LEFT_ASIDE / 2.0)
}
/// Same, but over a panel whose horizontal centre is at `x`: 150 is the
/// left dock, 700 the middle page (Rules, Settings, Checks).
pub(crate) fn scrolled_to_at(
    ctx: &egui::Context,
    app: &mut App,
    needle: &str,
    x: f32,
) -> Result<bool, String> {
    // One fixed pointer can land on a text field, which swallows the
    // wheel; step down the same column so the scroll always finds a gap.
    let lanes = [(x, 500.0), (x, 300.0), (x, 700.0), (x, 880.0)];
    // Down first, then back up, in case the label sits above where an earlier
    // search left the scroll.
    for step in 0..300 {
        let (lane_x, lane_y) = lanes[step % lanes.len()];
        let direction = if step < 150 { -60.0 } else { 60.0 };
        // Small point-sized steps: a line-sized jump can sail a short
        // button clean past the viewport between two polls.
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, direction),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
        let clickable = output.shapes.iter().any(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text().contains(needle)
            {
                let center = text.pos + text.galley.size() / 2.0;
                return shape.clip_rect.contains(center);
            }
            false
        });
        if clickable {
            return Ok(true);
        }
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(egui::pos2(lane_x, lane_y)), wheel],
            egui::Modifiers::NONE,
        );
    }
    Ok(false)
}
pub(crate) fn self_names(app: &App) -> Vec<String> {
    app.project
        .board
        .layer_records
        .iter()
        .map(|r| r.name.clone())
        .collect()
}
thread_local! {
    /// When set, `frame` paints on a screen tall enough to show a whole page,
    /// so the tooltip walk can hover controls that sit below the usual fold.
    pub(crate) static TALL_SCREEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// When set, `frame` uses this width and height for tour replay (#774).
    pub(crate) static REPLAY_SCREEN: std::cell::Cell<Option<(f32, f32)>> =
        const { std::cell::Cell::new(None) };
}
pub(crate) fn frame(
    ctx: &egui::Context,
    app: &mut App,
    mut events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    ctx.enable_accesskit();
    // The first pass on a fresh context only installs the fonts and is
    // discarded; spend it on an empty input so the caller's events land.
    if !ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("design-fonts-installed"))
            .unwrap_or(false)
    }) {
        let mut warm = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1440.0, 940.0),
                )),
                ..Default::default()
            },
            |ui| app.show(ui),
        );
        warm.textures_delta.clear();
    }
    super::audit::record_touches(ctx, app, &events);
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    let (width, height) = REPLAY_SCREEN.with(|cell| cell.get()).unwrap_or((
        1440.0,
        if TALL_SCREEN.with(std::cell::Cell::get) {
            6000.0
        } else {
            940.0
        },
    ));
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(width, height),
            )),
            events,
            ..Default::default()
        },
        |ui| app.show(ui),
    );
    super::audit::remember_labels(&output);
    // This input test has no GPU renderer; intentionally discard texture uploads.
    output.textures_delta.clear();
    output
}

/// One frame at a tour replay size, then optional extra painting on top.
pub(crate) fn replay_frame(
    ctx: &egui::Context,
    app: &mut App,
    width: f32,
    height: f32,
    paint: impl FnOnce(&mut egui::Ui),
) -> egui::FullOutput {
    let mut paint = Some(paint);
    REPLAY_SCREEN.with(|cell| cell.set(Some((width, height))));
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(width, height),
            )),
            ..Default::default()
        },
        |ui| {
            app.show(ui);
            if let Some(paint) = paint.take() {
                paint(ui);
            }
        },
    );
    REPLAY_SCREEN.with(|cell| cell.set(None));
    output.textures_delta.clear();
    output
}

/// Every interactive control the last frame published to assistive
/// technology, as `(Role label, screen rectangle)`.
///
/// Why: the target-size walk must see all widgets, including ones no test
/// knows by name, and the accessibility tree is the one place egui lists them.
///
/// # Rules
/// - Runs two frames so the tree is complete; only roles a pointer can press.
///
/// # Dependencies
/// - `crates/ui-egui/src/target_size.rs` consumes the result.
///
/// # Do not
/// - Use it to find text; it returns controls only.
pub(crate) fn interactive_rects(ctx: &egui::Context, app: &mut App) -> Vec<(String, Rect)> {
    use egui::accesskit::Role;
    ctx.enable_accesskit();
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let mut out = Vec::new();
    if let Some(update) = output.platform_output.accesskit_update {
        for (_, node) in update.nodes {
            let pressable = matches!(
                node.role(),
                Role::Button
                    | Role::CheckBox
                    | Role::RadioButton
                    | Role::ComboBox
                    | Role::TextInput
                    | Role::MultilineTextInput
                    | Role::Slider
                    | Role::SpinButton
                    | Role::Link
                    | Role::Tab
                    | Role::MenuItem
                    | Role::Switch
                    | Role::ColorWell
            );
            if let (true, Some(b)) = (pressable, node.bounds()) {
                let name = format!("{:?} {}", node.role(), node.label().unwrap_or(""));
                out.push((
                    name.trim().to_string(),
                    Rect::from_min_max(
                        egui::pos2(b.x0 as f32, b.y0 as f32),
                        egui::pos2(b.x1 as f32, b.y1 as f32),
                    ),
                ));
            }
        }
    }
    out
}

/// Make the tall-screen mode last for one scope, even when a story bails out.
struct TallScope;
impl TallScope {
    fn enter() -> Self {
        TALL_SCREEN.with(|tall| tall.set(true));
        Self
    }
}
impl Drop for TallScope {
    fn drop(&mut self) {
        TALL_SCREEN.with(|tall| tall.set(false));
    }
}

/// Where the control the gate calls `id` (for example `spinbutton:Board
/// width`) sits right now, found on a screen tall enough for the whole page
/// so a control below the fold needs no scrolling.
fn control_center(ctx: &egui::Context, app: &mut App, id: &str) -> Result<egui::Pos2, String> {
    control_center_within(ctx, app, id, 0.0, f32::INFINITY)
}

/// Same, but only a control whose middle sits right of `min_x` and above
/// `max_y` counts.
fn control_center_within(
    ctx: &egui::Context,
    app: &mut App,
    id: &str,
    min_x: f32,
    max_y: f32,
) -> Result<egui::Pos2, String> {
    // A tooltip left by the last control must not cover this one.
    for _ in 0..3 {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerGone],
            egui::Modifiers::NONE,
        );
    }
    let found: Vec<_> = super::audit::controls(ctx, app)
        .into_iter()
        .filter(|c| {
            c.enabled
                && c.rect.height() >= 1.0
                && c.rect.center().x >= min_x
                && c.rect.center().y < max_y
                && c.id.as_deref() == Some(id)
        })
        .collect();
    found
        .into_iter()
        .next_back()
        .map(|c| c.rect.center())
        .ok_or_else(|| format!("{id} is not on screen, or is switched off"))
}

/// Screen row below which nothing belongs to the tool bar.
const TOOL_BAR_BOTTOM: f32 = 120.0;

/// Click a tool in the tool bar under the tabs. A tool that does not fit the
/// row lives in the "More" menu, so a person opens More and picks it by name.
pub(crate) fn tap_strip(ctx: &egui::Context, app: &mut App, id: &str) -> Result<(), String> {
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    if let Ok(pos) = control_center_within(ctx, app, id, 0.0, TOOL_BAR_BOTTOM) {
        return tap_at(ctx, app, pos);
    }
    let label = match id.strip_prefix("icon:") {
        Some(icon) => match icon {
            "tool-select" => "Select",
            "tool-move" => "Move",
            "tool-bus" => "Bus",
            "tool-label" => "Label",
            "tool-power" => "Power symbol",
            "tool-noconnect" => "No connect",
            "tool-port" => "Port",
            "tool-route" => "Route",
            "tool-via" => "Via",
            "tool-keepout" => "Keepout",
            "tool-line" => "Line",
            "tool-box" => "Box",
            "tool-circle" => "Circle",
            "tool-outline" => "Outline",
            "tool-words" => "Text",
            "tool-draw-shape" => "Draw shape",
            "tool-cutout" => "Cutout",
            "tool-hole" => "Mounting hole",
            "tool-fiducial" => "Assembly mark",
            "tool-pad" => "Pad",
            "tool-measure" => "Measure",
            other => return Err(format!("{other} has no known name")),
        }
        .to_owned(),
        None => id.split_once(':').map_or(id, |(_, label)| label).to_owned(),
    };
    let in_add_menu = matches!(
        id,
        "icon:tool-label"
            | "icon:tool-power"
            | "icon:tool-bus"
            | "icon:tool-port"
            | "icon:tool-noconnect"
            | "icon:tool-pad"
    );
    if in_add_menu && app.tab == Tab::Schematic {
        // The schematic keeps these under the Add menu, opened by its button.
        tap_strip(ctx, app, "icon:tool-add")?;
        frame(ctx, app, vec![], egui::Modifiers::NONE);
        return authoring::tests::tap(ctx, app, &label);
    }
    authoring::tests::tap(ctx, app, "More")?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(ctx, app, &label)
}

/// Click a control in the Properties column on the right of the window,
/// like `tap_id` but ignoring a control of the same name elsewhere.
pub(crate) fn tap_properties(ctx: &egui::Context, app: &mut App, id: &str) -> Result<(), String> {
    let _tall = TallScope::enter();
    let pos = control_center_within(ctx, app, id, PROPERTIES_LEFT, f32::INFINITY)?;
    tap_at(ctx, app, pos)
}

/// Press choice `slot` (counting from 0) of the `count`-choice segmented
/// control named `group`.
/// A segmented control is one radio group on the accessibility tree, so the
/// choice is picked by its place in the strip, like the hand does.
pub(crate) fn tap_segment(
    ctx: &egui::Context,
    app: &mut App,
    group: &str,
    min_x: f32,
    max_y: f32,
    slot: usize,
    count: usize,
) -> Result<(), String> {
    let prefix = format!("radiobutton:{group}");
    let all = super::audit::controls(ctx, app);
    let strip = all
        .iter()
        .rfind(|c| {
            c.enabled
                && c.rect.center().x >= min_x
                && c.rect.center().y < max_y
                && c.id.as_deref().is_some_and(|id| id.starts_with(&prefix))
        })
        .ok_or_else(|| {
            let radios: Vec<_> = all
                .iter()
                .filter(|c| c.rect.center().y < max_y)
                .map(|c| (c.id.clone(), c.rect))
                .collect();
            format!("the {group} switch is not on screen; radio groups: {radios:?}")
        })?;
    let part = strip.rect.width() / count as f32;
    let x = strip.rect.min.x + part * (slot as f32 + 0.5);
    tap_at(ctx, app, egui::pos2(x, strip.rect.center().y))
}

/// Press choice `slot` of the segmented control `group` in Properties.
pub(crate) fn tap_properties_segment(
    ctx: &egui::Context,
    app: &mut App,
    group: &str,
    slot: usize,
    count: usize,
) -> Result<(), String> {
    let _tall = TallScope::enter();
    tap_segment(ctx, app, group, PROPERTIES_LEFT, f32::INFINITY, slot, count)
}

/// True when the control the gate calls `id` is on screen and switched on.
pub(crate) fn on_screen(ctx: &egui::Context, app: &mut App, id: &str) -> bool {
    let _tall = TallScope::enter();
    control_center(ctx, app, id).is_ok()
}

/// Press each control in turn, like a person working down a list, and
/// return the ones that were not there to press.
pub(crate) fn press_each(ctx: &egui::Context, app: &mut App, ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter(|id| tap_id(ctx, app, id).is_err())
        .map(|id| (*id).to_owned())
        .collect()
}

/// Press `from`, then check that `to` is what the same control reads now.
pub(crate) fn flips_to(
    ctx: &egui::Context,
    app: &mut App,
    from: &str,
    to: &str,
) -> Result<(), String> {
    tap_id(ctx, app, from)?;
    if on_screen(ctx, app, to) {
        Ok(())
    } else {
        Err(format!("after {from} the control should read {to}"))
    }
}

/// Screen column right of which the Properties dock sits.
const PROPERTIES_LEFT: f32 = 1000.0;

/// Click the control the gate calls `id`, like a person pressing it.
pub(crate) fn tap_id(ctx: &egui::Context, app: &mut App, id: &str) -> Result<(), String> {
    let _tall = TallScope::enter();
    let pos = control_center(ctx, app, id)?;
    tap_at(ctx, app, pos)
}

/// Click the control whose spoken name is exactly `name` (role first, digits kept, for
/// example `CheckBox Pick R2`) while the given keys are held, like a hand holding Shift.
/// Use it where several rows share one folded gate id.
pub(crate) fn tap_named_holding(
    ctx: &egui::Context,
    app: &mut App,
    name: &str,
    modifiers: egui::Modifiers,
) -> Result<(), String> {
    let _tall = TallScope::enter();
    let pos = interactive_rects(ctx, app)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, r)| r.center())
        .ok_or_else(|| format!("{name} is not on screen"))?;
    frame(ctx, app, vec![egui::Event::PointerMoved(pos)], modifiers);
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
            modifiers,
        );
    }
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

/// Press and release at `pos`, like a hand.
pub(crate) fn tap_at(ctx: &egui::Context, app: &mut App, pos: egui::Pos2) -> Result<(), String> {
    // Rest the pointer on it first, as a hand does, before pressing.
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(pos)],
        egui::Modifiers::NONE,
    );
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

/// Click into a field, replace what it holds with `text` and press Enter.
/// Works for number fields and text boxes alike.
pub(crate) fn type_into(
    ctx: &egui::Context,
    app: &mut App,
    id: &str,
    text: &str,
) -> Result<(), String> {
    tap_id(ctx, app, id)?;
    let _tall = TallScope::enter();
    let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        ctx,
        app,
        vec![key(egui::Key::A, egui::Modifiers::COMMAND)],
        egui::Modifiers::COMMAND,
    );
    frame(
        ctx,
        app,
        vec![egui::Event::Text(text.to_owned())],
        egui::Modifiers::NONE,
    );
    frame(
        ctx,
        app,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

/// Select a row of the left aside's Layers tree that is not one object, so
/// Properties shows it: `root` for the board settings, `nets`
/// for the connections editor.
pub(crate) fn open_tree_row(ctx: &egui::Context, app: &mut App, key: &str) {
    // The board's settings are edited on the layout page.
    if key == "root" && app.tab != Tab::Pcb {
        app.change_tab(Tab::Pcb);
    }
    app.region().left_tab = "layers".to_owned();
    app.select(None);
    app.tree.selected_key = Some(key.to_owned());
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

pub(crate) fn key(ctx: &egui::Context, app: &mut App, key: egui::Key) {
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            egui::Modifiers::NONE,
        );
    }
}

/// Reach a tree row with the keyboard cursor, then press one key on it.
pub(crate) fn press_on(ctx: &egui::Context, app: &mut App, row: &str, k: egui::Key) {
    app.tree.focus_row(row);
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    key(ctx, app, k);
}

/// Click the row of the left aside whose words read exactly `label`, ignoring
/// the same words painted on the drawing (net names and part references
/// appear in both places).
pub(crate) fn tap_left(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let at = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                let c = t.pos + t.galley.size() / 2.0;
                (c.x < 320.0 && shape.clip_rect.contains(c)).then_some(c)
            }
            _ => None,
        })
        .next_back()
        .ok_or_else(|| format!("{label} is not in the left aside"))?;
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(at)],
        egui::Modifiers::NONE,
    );
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

/// A settings folder no other test can see: unique per process, tag and call.
///
/// Recents, library sources and saved conversations all live below the
/// folder a session carries, so a story that needs them gets its own and
/// nothing leaks between tests or into the real configuration folder.
pub(crate) fn private_config(
    tag: &str,
) -> Result<(pcbcraft_engine::user_config::UserConfig, std::path::PathBuf), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let folder = std::env::temp_dir().join(format!(
        "pcbcraft-config-{}-{}-{tag}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok((
        pcbcraft_engine::user_config::UserConfig::in_folder(&folder),
        folder,
    ))
}

/// An empty session that keeps its settings in `config`.
pub(crate) fn session_with(
    config: &pcbcraft_engine::user_config::UserConfig,
) -> pcbcraft_mcp::SharedSession {
    let mut session = pcbcraft_engine::Session::default();
    session.config = config.clone();
    pcbcraft_mcp::shared(session)
}

/// Draw frames until the collection browser has finished reading its folders
/// on its worker thread: the catalog and the source list are both in, nothing
/// is pending, and that holds for a few frames in a row (the browser asks for
/// the source list only after the catalog arrives, and switches its controls
/// off while any read is pending).
pub(crate) fn waits_for_library_read(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    let mut steady = 0;
    let settled = waits_until(ctx, app, |app| {
        let done = app.library.catalog.is_some()
            && app.library.sources.is_some()
            && app.library.pending.is_none();
        steady = if done { steady + 1 } else { 0 };
        steady >= 3
    });
    if settled {
        Ok(())
    } else {
        Err("the collection browser never finished reading".into())
    }
}
