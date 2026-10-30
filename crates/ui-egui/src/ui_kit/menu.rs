//! The context menu: one designed panel that every right-click menu uses.
//!
//! Context: design system section 8 and ticket #836. A menu is a list of
//! [`Entry`] values built by the caller (usually through [`Menu`], which orders the
//! groups), drawn by [`show`] or attached to a widget with [`context`]. Callers
//! never draw a row: they get back the value of the entry the person picked.
//! The panel is 6 px rounded with a 1 px `edge` border and a soft shadow. Rows are
//! 28 high with 8 px side padding, a 16 px icon column, the label, the shortcut at
//! the right in muted Mono 11, and a chevron for a submenu. Separators sit between
//! groups, a destructive group (red) comes last, and an optional muted footer
//! carries the modifier hint.
//! Local rules: only one context menu is open at a time; the keyboard drives the
//! open menu (arrows, Home, End, Enter, Space, Right and Left for submenus, Esc,
//! and type-ahead on the first letters of a label); while a menu is open the
//! app's single-key shortcuts are paused (see [`any_open`]).
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs (tokens, radii, heights)
//! - crates/ui-egui/src/fonts.rs (`Role`)
//! - crates/ui-egui/src/ui_kit/icons.rs (`Icon`, chevron, check)
//! - crates/ui-egui/src/ui_kit/menu_icons.rs (row glyphs)
//! - crates/ui-egui/src/ui_kit/focus.rs (focus stroke width lives in theme)
//!
//! # Do not
//! - Use `Response::context_menu`, `ui.button` or `ui.menu_button` for a right-click
//!   menu anywhere else; `no_context_menu_builds_its_own_rows` fails the build.
//! - Put a colour, size or radius literal in a row; add it to `theme.rs`.
use super::icons::{self, ICON_SIZE, Icon};
use crate::fonts::Role;
use crate::theme::{self, FOCUS_STROKE, RADIUS_CARD, RADIUS_CONTROL, Tokens};
use egui::{
    Area, Color32, Context, CornerRadius, Event, FontFamily, FontId, Frame, Id, Key, Margin,
    Modifiers, Order, Pos2, Rect, Response, Sense, Shadow, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo, WidgetType, pos2, vec2,
};

/// Height of a menu row.
pub const ROW_HEIGHT: f32 = theme::CONTROL_HEIGHT;
/// Side padding inside a row, and the gap between icon column and label.
pub const PAD: f32 = 8.0;
/// Narrowest panel.
pub const MIN_WIDTH: f32 = 200.0;
/// Widest panel; longer labels are cut with an ellipsis.
pub const MAX_WIDTH: f32 = 360.0;
/// Space between the panel border and the rows.
const PANEL_MARGIN: i8 = 4;
/// Height of a separator row: 4 above, the 1 px rule, 4 below.
const SEPARATOR_HEIGHT: f32 = 9.0;
/// How long typed letters keep adding to the type-ahead search, in seconds.
const TYPE_AHEAD_SECONDS: f64 = 1.0;
/// Shortcut text size: Mono 11.
const SHORTCUT_SIZE: f32 = 11.0;

/// One choosable row.
///
/// Why: a plain value type the caller fills and the menu draws; the caller keeps
/// no widget state.
///
/// # Rules
/// - `tip` is the tooltip; for a disabled row it says how to enable it.
/// - `value` is handed back by [`show`] when the row is picked.
///
/// # Do not
/// - Put the shortcut in the label; use [`Item::shortcut`].
#[derive(Clone, Debug)]
pub struct Item<T> {
    /// Returned when the row is picked.
    pub value: T,
    /// The visible words, sentence case.
    pub label: String,
    /// The tooltip.
    pub tip: String,
    /// Leading glyph.
    pub icon: Option<Icon>,
    /// Shortcut text at the right, such as `Ctrl+C`.
    pub shortcut: String,
    /// False draws the row muted and ignores it.
    pub enabled: bool,
    /// Draws the row in the error colour.
    pub danger: bool,
    /// `Some(true)` shows a tick in the icon column.
    pub checked: Option<bool>,
}

impl<T> Item<T> {
    /// A row with a label and a tooltip.
    pub fn new(value: T, label: impl Into<String>, tip: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            tip: tip.into(),
            icon: None,
            shortcut: String::new(),
            enabled: true,
            danger: false,
            checked: None,
        }
    }

    /// Lead the row with a glyph.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Show a shortcut at the right.
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = shortcut.into();
        self
    }

    /// Show a tick when `on`.
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }

    /// Disable the row unless `on`; `why` replaces the tooltip and says how to enable it.
    pub fn enabled_if(mut self, on: bool, why: impl Into<String>) -> Self {
        if !on {
            self.enabled = false;
            self.tip = why.into();
        }
        self
    }
}

/// A row that opens a list of rows beside it.
#[derive(Clone, Debug)]
pub struct Submenu<T> {
    /// The visible words.
    pub label: String,
    /// The tooltip.
    pub tip: String,
    /// Leading glyph.
    pub icon: Option<Icon>,
    /// The rows inside: items and separators. A nested submenu is not drawn.
    pub entries: Vec<Entry<T>>,
}

/// Anything a menu can hold.
#[derive(Clone, Debug)]
pub enum Entry<T> {
    /// A choosable row.
    Item(Item<T>),
    /// A row that opens more rows.
    Submenu(Submenu<T>),
    /// A rule between groups.
    Separator,
    /// A muted key list at the bottom (the modifier hint), one row per line.
    Footer {
        /// Rows of (key, action): the key in muted Mono, the action beside it.
        rows: Vec<(String, String)>,
        /// The tooltip.
        tip: String,
    },
}

impl<T> From<Item<T>> for Entry<T> {
    fn from(item: Item<T>) -> Self {
        Entry::Item(item)
    }
}

impl<T> Entry<T> {
    /// A submenu row.
    pub fn submenu(
        label: impl Into<String>,
        tip: impl Into<String>,
        icon: Option<Icon>,
        entries: Vec<Entry<T>>,
    ) -> Self {
        Entry::Submenu(Submenu {
            label: label.into(),
            tip: tip.into(),
            icon,
            entries,
        })
    }

    /// True for rows the keyboard can land on.
    fn selectable(&self) -> bool {
        match self {
            Entry::Item(item) => item.enabled,
            Entry::Submenu(sub) => sub.entries.iter().any(Entry::selectable),
            _ => false,
        }
    }

    fn label(&self) -> &str {
        match self {
            Entry::Item(item) => &item.label,
            Entry::Submenu(sub) => &sub.label,
            _ => "",
        }
    }
}

/// Builds a menu in the fixed order: groups first, the destructive group last, the footer under it.
///
/// Why: every menu groups the same way (object actions, edit, view, then
/// destructive), and building through this type makes it impossible to put a
/// destructive row anywhere but the bottom.
///
/// # Rules
/// - Empty groups are skipped, so a caller can pass a group that turned out empty.
/// - `destructive` rows are drawn in the error colour after the last separator.
///
/// # Do not
/// - Add a separator by hand; groups get one between them.
#[derive(Clone, Debug)]
pub struct Menu<T> {
    groups: Vec<Vec<Entry<T>>>,
    destructive: Vec<Item<T>>,
    footer: Option<(Vec<(String, String)>, String)>,
}

impl<T> Default for Menu<T> {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            destructive: Vec::new(),
            footer: None,
        }
    }
}

impl<T> Menu<T> {
    /// An empty menu.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a group of rows.
    pub fn group(mut self, rows: impl IntoIterator<Item = Entry<T>>) -> Self {
        let rows: Vec<Entry<T>> = rows.into_iter().collect();
        if !rows.is_empty() {
            self.groups.push(rows);
        }
        self
    }

    /// Add a destructive row (Delete, Remove). It is drawn in the error colour, last.
    pub fn destructive(mut self, mut item: Item<T>) -> Self {
        item.danger = true;
        self.destructive.push(item);
        self
    }

    /// Add the muted footer line.
    pub fn footer(mut self, rows: Vec<(String, String)>, tip: impl Into<String>) -> Self {
        if !rows.is_empty() {
            self.footer = Some((rows, tip.into()));
        }
        self
    }

    /// The finished list, separators included.
    pub fn entries(self) -> Vec<Entry<T>> {
        let mut out: Vec<Entry<T>> = Vec::new();
        let mut groups = self.groups;
        if !self.destructive.is_empty() {
            groups.push(self.destructive.into_iter().map(Entry::Item).collect());
        }
        for group in groups {
            if !out.is_empty() {
                out.push(Entry::Separator);
            }
            out.extend(group);
        }
        if let Some((rows, tip)) = self.footer {
            if !out.is_empty() {
                out.push(Entry::Separator);
            }
            out.push(Entry::Footer { rows, tip });
        }
        out
    }
}

/// What a menu did this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<T> {
    /// Still open.
    Open,
    /// A row was picked; the menu is done.
    Picked(T),
    /// Escape, a click outside, a scroll, or nothing left to show.
    Closed,
}

/// Keyboard and pointer state of the open menu, kept in egui memory.
#[derive(Clone, Debug, Default)]
struct State {
    /// The highlighted top-level row (entry index).
    hot: Option<usize>,
    /// The highlight came from the keyboard, so it draws a focus ring.
    by_key: bool,
    /// The submenu that is open (entry index).
    sub: Option<usize>,
    /// Keys act on the submenu rather than the top level.
    in_sub: bool,
    /// The highlighted row inside the open submenu.
    sub_hot: Option<usize>,
    /// Letters typed for type-ahead.
    typed: String,
    /// Time of the last typed letter.
    typed_at: f64,
    /// The frame the menu opened on; presses on that frame belong to the opener.
    opened_frame: u64,
    /// The last frame this menu was drawn.
    last_frame: u64,
}

fn state_id(id: Id) -> Id {
    id.with("pcb-menu-state")
}

fn open_key() -> Id {
    Id::new("pcb-open-context-menu")
}

fn capture_key() -> Id {
    Id::new("pcb-capture-context-menu")
}

/// True while a context menu is drawn, so single-key shortcuts can stand down.
///
/// # Rules
/// - Call from the shortcut handler before it reads keys.
pub fn any_open(ctx: &Context) -> bool {
    let now = ctx.cumulative_frame_nr();
    ctx.data(|d| d.get_temp::<u64>(Id::new("pcb-menu-last-frame")))
        .is_some_and(|last| last + 1 >= now)
}

/// Ask for the menu tagged `tag` to open by itself on its first visible row.
///
/// Why: `--capture` images of a tree row menu and a library card menu need a menu
/// open without a mouse.
///
/// # Do not
/// - Call outside capture mode.
pub fn request_capture(ctx: &Context, tag: Option<&str>) {
    ctx.data_mut(|d| match tag {
        Some(tag) => {
            if d.get_temp::<String>(capture_key()).is_none()
                && d.get_temp::<bool>(capture_key().with("done")).is_none()
            {
                d.insert_temp(capture_key(), tag.to_owned());
            }
        }
        None => {
            d.remove_temp::<String>(capture_key());
        }
    });
}

/// Attach a context menu to `response`: a right click opens it at the pointer.
///
/// Why: the Layers tree, the library cards, the findings rows and the page tabs all
/// open their menu this way.
///
/// # Rules
/// - `entries` runs only while the menu is open, so building it costs nothing at rest.
/// - `tag` names the kind of row for capture mode; it is never shown.
/// - Returns the picked value once, on the frame it is picked.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/menu.rs `show`.
///
/// # Do not
/// - Mutate app state inside `entries`; act on the returned value.
pub fn context<T: Clone>(
    response: &Response,
    tag: &str,
    entries: impl FnOnce() -> Vec<Entry<T>>,
) -> Option<T> {
    let ctx = &response.ctx;
    let id = response.id.with("pcb-context-menu");
    let now = ctx.cumulative_frame_nr();
    if response.secondary_clicked()
        && let Some(at) = ctx.input(|i| i.pointer.interact_pos())
    {
        ctx.data_mut(|d| d.insert_temp(open_key(), (id.value(), at, now)));
    }
    let wanted = ctx.data(|d| d.get_temp::<String>(capture_key()));
    if wanted.as_deref() == Some(tag) && ctx.content_rect().contains(response.rect.center()) {
        ctx.data_mut(|d| {
            d.remove_temp::<String>(capture_key());
            d.insert_temp(capture_key().with("done"), true);
            d.insert_temp(open_key(), (id.value(), response.rect.center(), now));
        });
    }
    let (open_id, at, seen) = ctx.data(|d| d.get_temp::<(u64, Pos2, u64)>(open_key()))?;
    if open_id != id.value() {
        return None;
    }
    if seen + 1 < now {
        // The row was off screen for a frame; its menu is gone.
        ctx.data_mut(|d| d.remove_temp::<(u64, Pos2, u64)>(open_key()));
        return None;
    }
    let list = entries();
    let outcome = if list.is_empty() {
        Outcome::Closed
    } else {
        show(ctx, id, at, &list)
    };
    match outcome {
        Outcome::Open => {
            ctx.data_mut(|d| d.insert_temp(open_key(), (id.value(), at, now)));
            None
        }
        Outcome::Picked(value) => {
            ctx.data_mut(|d| d.remove_temp::<(u64, Pos2, u64)>(open_key()));
            Some(value)
        }
        Outcome::Closed => {
            ctx.data_mut(|d| d.remove_temp::<(u64, Pos2, u64)>(open_key()));
            None
        }
    }
}

/// The shortcut text for Ctrl (Command on a Mac) plus a key, in the platform's words.
pub fn chord(ctx: &Context, key: Key) -> String {
    ctx.format_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, key))
}

/// Split catalog text written as `Label  Shortcut` (two spaces) into its two parts.
///
/// Why: some catalog strings still carry their shortcut in the label; the menu
/// shows the shortcut in its own column.
pub fn split_shortcut(text: &str) -> (String, String) {
    match text.split_once("  ") {
        Some((label, shortcut)) => (label.trim().to_owned(), shortcut.trim().to_owned()),
        None => (text.to_owned(), String::new()),
    }
}

fn panel_frame(t: &Tokens) -> Frame {
    let (alpha, blur) = if t.is_dark() { (110, 18) } else { (48, 16) };
    Frame::new()
        .fill(t.panel)
        .stroke(Stroke::new(theme::BORDER, t.edge))
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(Margin::same(PANEL_MARGIN))
        .shadow(Shadow {
            offset: [0, 6],
            blur,
            spread: 0,
            color: Color32::from_black_alpha(alpha),
        })
}

fn shortcut_font() -> FontId {
    FontId::new(SHORTCUT_SIZE, FontFamily::Monospace)
}

/// Panel width for a list: the widest label plus shortcut, clamped.
fn measure<T>(ui: &Ui, entries: &[Entry<T>], with_chevron_room: bool) -> f32 {
    let t = theme::tokens(ui.ctx());
    let mut widest = 0.0f32;
    for entry in entries {
        let (label, shortcut, chevron) = match entry {
            Entry::Item(item) => (item.label.as_str(), item.shortcut.as_str(), false),
            Entry::Submenu(sub) => (sub.label.as_str(), "", true),
            _ => continue,
        };
        let l = ui
            .painter()
            .layout_no_wrap(label.to_owned(), Role::Body.font(), t.text)
            .size()
            .x;
        let s = if shortcut.is_empty() {
            0.0
        } else {
            ui.painter()
                .layout_no_wrap(shortcut.to_owned(), shortcut_font(), t.muted)
                .size()
                .x
                + 24.0
        };
        let c = if chevron && with_chevron_room {
            ICON_SIZE + PAD
        } else {
            0.0
        };
        widest = widest.max(PAD + ICON_SIZE + PAD + l + s + c + PAD);
    }
    widest.clamp(MIN_WIDTH, MAX_WIDTH)
}

/// What the keyboard did this frame.
#[derive(Default)]
struct Keys {
    up: bool,
    down: bool,
    home: bool,
    end: bool,
    enter: bool,
    left: bool,
    right: bool,
    escape: bool,
    typed: String,
}

fn take_keys(ctx: &Context) -> Keys {
    ctx.input_mut(|i| {
        let mut k = Keys {
            up: i.consume_key(Modifiers::NONE, Key::ArrowUp),
            down: i.consume_key(Modifiers::NONE, Key::ArrowDown),
            home: i.consume_key(Modifiers::NONE, Key::Home),
            end: i.consume_key(Modifiers::NONE, Key::End),
            enter: i.consume_key(Modifiers::NONE, Key::Enter),
            left: i.consume_key(Modifiers::NONE, Key::ArrowLeft),
            right: i.consume_key(Modifiers::NONE, Key::ArrowRight),
            escape: i.consume_key(Modifiers::NONE, Key::Escape),
            typed: String::new(),
        };
        for event in &i.events {
            if let Event::Text(text) = event {
                k.typed.push_str(&text.to_lowercase());
            }
        }
        // Letters typed into the menu are not also shortcuts of the app.
        i.events.retain(|event| match event {
            Event::Text(_) => false,
            Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } => {
                (*modifiers != Modifiers::NONE && *modifiers != Modifiers::SHIFT)
                    || key.name().chars().count() != 1
            }
            _ => true,
        });
        k
    })
}

/// Where the highlight goes when moving through `list` by one step.
fn step(list: &[usize], at: Option<usize>, forward: bool) -> Option<usize> {
    if list.is_empty() {
        return None;
    }
    let pos = at.and_then(|a| list.iter().position(|&x| x == a));
    let next = match (pos, forward) {
        (None, true) => 0,
        (None, false) => list.len() - 1,
        (Some(p), true) => (p + 1) % list.len(),
        (Some(p), false) => (p + list.len() - 1) % list.len(),
    };
    list.get(next).copied()
}

fn selectable_rows<T>(entries: &[Entry<T>]) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.selectable())
        .map(|(i, _)| i)
        .collect()
}

/// The row a type-ahead search lands on.
fn type_ahead<T>(
    entries: &[Entry<T>],
    list: &[usize],
    at: Option<usize>,
    typed: &str,
) -> Option<usize> {
    let start = at.and_then(|a| list.iter().position(|&x| x == a));
    let offset = if typed.chars().count() > 1 { 0 } else { 1 };
    (0..list.len())
        .map(|k| list[(start.map_or(0, |s| s + offset) + k) % list.len()])
        .find(|&i| entries[i].label().to_lowercase().starts_with(typed))
}

/// Everything one list needs while drawing.
struct Draw<'a, T> {
    t: Tokens,
    width: f32,
    hot: &'a mut Option<usize>,
    by_key: bool,
    focus: bool,
    pointer_moved: bool,
    picked: &'a mut Option<T>,
    rects: Vec<Rect>,
    opened_sub: Option<usize>,
    clicked_sub: Option<usize>,
    plain_hover: bool,
}

fn draw_list<T: Clone>(ui: &mut Ui, entries: &[Entry<T>], d: &mut Draw<'_, T>, allow_sub: bool) {
    d.rects = vec![Rect::NOTHING; entries.len()];
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            Entry::Separator => {
                let (rect, _) =
                    ui.allocate_exact_size(vec2(d.width, SEPARATOR_HEIGHT), Sense::hover());
                ui.painter().hline(
                    rect.x_range().shrink(PAD - PANEL_MARGIN as f32),
                    rect.center().y,
                    Stroke::new(theme::BORDER, d.t.line),
                );
            }
            Entry::Footer { rows, tip } => {
                let line = Role::Caption.font().size + 6.0;
                let size = vec2(d.width, line * rows.len() as f32 + 12.0);
                let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
                let key_width = rows
                    .iter()
                    .map(|(k, _)| {
                        ui.painter()
                            .layout_no_wrap(k.clone(), shortcut_font(), d.t.muted)
                            .size()
                            .x
                    })
                    .fold(0.0f32, f32::max);
                for (n, (key, action)) in rows.iter().enumerate() {
                    let y = rect.top() + 6.0 + line * n as f32 + line / 2.0;
                    let k = ui
                        .painter()
                        .layout_no_wrap(key.clone(), shortcut_font(), d.t.muted);
                    ui.painter().galley(
                        pos2(rect.left() + PAD, y - k.size().y / 2.0),
                        k,
                        d.t.muted,
                    );
                    let a = ui.painter().layout_no_wrap(
                        action.clone(),
                        Role::Caption.font(),
                        d.t.muted,
                    );
                    ui.painter().galley(
                        pos2(rect.left() + PAD + key_width + PAD, y - a.size().y / 2.0),
                        a,
                        d.t.muted,
                    );
                }
                response.on_hover_text(tip);
            }
            Entry::Item(item) => {
                let sense = if item.enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                };
                let (rect, response) = ui.allocate_exact_size(vec2(d.width, ROW_HEIGHT), sense);
                d.rects[index] = rect;
                if item.enabled && response.hovered() && d.pointer_moved {
                    *d.hot = Some(index);
                    d.plain_hover = true;
                }
                let hot = *d.hot == Some(index) && item.enabled;
                paint_row(
                    ui,
                    d,
                    rect,
                    &item.label,
                    &item.shortcut,
                    item.icon,
                    item.checked,
                    item.enabled,
                    item.danger,
                    hot,
                    false,
                );
                response.widget_info(|| {
                    WidgetInfo::labeled(WidgetType::Button, item.enabled, item.label.clone())
                });
                if item.enabled && response.clicked() {
                    *d.picked = Some(item.value.clone());
                }
                response.on_hover_text(&item.tip);
            }
            Entry::Submenu(sub) => {
                if !allow_sub {
                    continue;
                }
                let enabled = sub.entries.iter().any(Entry::selectable);
                let sense = if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                };
                let (rect, response) = ui.allocate_exact_size(vec2(d.width, ROW_HEIGHT), sense);
                d.rects[index] = rect;
                if enabled && response.hovered() && d.pointer_moved {
                    *d.hot = Some(index);
                    d.opened_sub = Some(index);
                }
                if enabled && response.clicked() {
                    d.clicked_sub = Some(index);
                }
                let hot = *d.hot == Some(index) && enabled;
                paint_row(
                    ui, d, rect, &sub.label, "", sub.icon, None, enabled, false, hot, true,
                );
                response.widget_info(|| {
                    WidgetInfo::labeled(WidgetType::Button, enabled, sub.label.clone())
                });
                response.on_hover_text(&sub.tip);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_row<T>(
    ui: &Ui,
    d: &Draw<'_, T>,
    rect: Rect,
    label: &str,
    shortcut: &str,
    icon: Option<Icon>,
    checked: Option<bool>,
    enabled: bool,
    danger: bool,
    hot: bool,
    chevron: bool,
) {
    let t = &d.t;
    let painter = ui.painter();
    let fade = |c: Color32| if enabled { c } else { Tokens::faded(c, 0.45) };
    if hot {
        painter.rect_filled(
            rect,
            CornerRadius::same(RADIUS_CONTROL),
            if danger { t.red_tint } else { t.raised },
        );
        if d.by_key && d.focus {
            painter.rect_stroke(
                rect.shrink(FOCUS_STROKE / 2.0),
                CornerRadius::same(RADIUS_CONTROL),
                Stroke::new(FOCUS_STROKE, t.text),
                StrokeKind::Inside,
            );
        }
    }
    let ink = fade(if danger { t.red } else { t.text });
    let glyph = fade(if danger {
        t.red
    } else if hot {
        t.text
    } else {
        t.muted
    });
    let slot = Rect::from_min_size(
        pos2(rect.left() + PAD, rect.center().y - ICON_SIZE / 2.0),
        Vec2::splat(ICON_SIZE),
    );
    if checked == Some(true) {
        (icons::CHECK.0)(painter, slot, fade(t.copper));
    } else if let Some(icon) = icon {
        (icon.0)(painter, slot, glyph);
    }
    let mut right = rect.right() - PAD;
    if chevron {
        let chev = Rect::from_min_size(
            pos2(right - ICON_SIZE, rect.center().y - ICON_SIZE / 2.0),
            Vec2::splat(ICON_SIZE),
        );
        (icons::CHEVRON_RIGHT.0)(painter, chev, glyph);
        right -= ICON_SIZE;
    }
    if !shortcut.is_empty() {
        let galley = painter.layout_no_wrap(shortcut.to_owned(), shortcut_font(), fade(t.muted));
        let at = pos2(
            right - galley.size().x,
            rect.center().y - galley.size().y / 2.0,
        );
        right = at.x - PAD;
        painter.galley(at, galley, fade(t.muted));
    }
    let left = rect.left() + PAD + ICON_SIZE + PAD;
    let mut job =
        egui::text::LayoutJob::simple_singleline(label.to_owned(), Role::Body.font(), ink);
    job.wrap.max_width = (right - left).max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('\u{2026}');
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(left, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
}

/// Draw the menu at `at` and run its keyboard; the caller keeps the position and calls every frame.
///
/// Why: one place decides how a context menu looks and behaves, whether it hangs
/// on a widget ([`context`]) or opens at a canvas point.
///
/// # Rules
/// - Call once per frame while the menu is open; stop calling when it returns
///   anything but `Open`.
/// - A press on the frame the menu opened belongs to the opener and does not close it.
/// - Takes keyboard focus from other widgets while open, so arrows move the menu.
///
/// # Dependencies
/// - crates/ui-egui/src/theme.rs (`tokens`).
///
/// # Do not
/// - Call with an empty list; callers close instead.
pub fn show<T: Clone>(ctx: &Context, id: Id, at: Pos2, entries: &[Entry<T>]) -> Outcome<T> {
    let t = theme::tokens(ctx);
    let now = ctx.cumulative_frame_nr();
    let mut st: State = ctx.data(|d| d.get_temp(state_id(id))).unwrap_or_default();
    if st.last_frame == 0 || st.last_frame + 1 < now {
        st = State {
            opened_frame: now,
            ..State::default()
        };
    }
    st.last_frame = now;
    ctx.data_mut(|d| d.insert_temp(Id::new("pcb-menu-last-frame"), now));
    // The menu owns the keyboard: no tree row or text box keeps focus underneath.
    ctx.memory_mut(|m| {
        if let Some(focused) = m.focused() {
            m.surrender_focus(focused);
        }
    });

    let keys = take_keys(ctx);
    let mut picked: Option<T> = None;
    let mut closed = keys.escape && !st.in_sub;
    if keys.escape && st.in_sub {
        st.in_sub = false;
        st.sub = None;
        st.sub_hot = None;
    }
    let top_rows = selectable_rows(entries);
    let sub_entries = |sub: Option<usize>| -> Option<&[Entry<T>]> {
        match sub.and_then(|i| entries.get(i)) {
            Some(Entry::Submenu(s)) => Some(s.entries.as_slice()),
            _ => None,
        }
    };
    let moved = keys.up || keys.down || keys.home || keys.end || !keys.typed.is_empty();
    if moved {
        st.by_key = true;
    }
    if st.in_sub {
        if let Some(sub_list) = sub_entries(st.sub) {
            let rows = selectable_rows(sub_list);
            if keys.down {
                st.sub_hot = step(&rows, st.sub_hot, true);
            }
            if keys.up {
                st.sub_hot = step(&rows, st.sub_hot, false);
            }
            if keys.home {
                st.sub_hot = rows.first().copied();
            }
            if keys.end {
                st.sub_hot = rows.last().copied();
            }
            if !keys.typed.is_empty() {
                st.typed.push_str(&keys.typed);
                st.typed_at = ctx.input(|i| i.time);
                st.sub_hot = type_ahead(sub_list, &rows, st.sub_hot, &st.typed).or(st.sub_hot);
            }
            if keys.left {
                st.in_sub = false;
                st.sub = None;
                st.sub_hot = None;
            }
            if keys.enter
                && let Some(Entry::Item(item)) = st.sub_hot.and_then(|i| sub_list.get(i))
                && item.enabled
            {
                picked = Some(item.value.clone());
            }
        } else {
            st.in_sub = false;
        }
    } else {
        if ctx.input(|i| i.time) - st.typed_at > TYPE_AHEAD_SECONDS {
            st.typed.clear();
        }
        if keys.down {
            st.hot = step(&top_rows, st.hot, true);
        }
        if keys.up {
            st.hot = step(&top_rows, st.hot, false);
        }
        if keys.home {
            st.hot = top_rows.first().copied();
        }
        if keys.end {
            st.hot = top_rows.last().copied();
        }
        if !keys.typed.is_empty() {
            st.typed.push_str(&keys.typed);
            st.typed_at = ctx.input(|i| i.time);
            st.hot = type_ahead(entries, &top_rows, st.hot, &st.typed).or(st.hot);
        }
        if keys.down || keys.up || keys.home || keys.end || !keys.typed.is_empty() {
            st.sub = None;
        }
        let open_hot_sub = |st: &mut State| {
            if let Some(Entry::Submenu(s)) = st.hot.and_then(|i| entries.get(i)) {
                st.sub = st.hot;
                st.in_sub = true;
                st.sub_hot = selectable_rows(&s.entries).first().copied();
            }
        };
        if keys.right {
            open_hot_sub(&mut st);
        }
        if keys.enter {
            match st.hot.and_then(|i| entries.get(i)) {
                Some(Entry::Item(item)) if item.enabled => picked = Some(item.value.clone()),
                Some(Entry::Submenu(_)) => open_hot_sub(&mut st),
                _ => {}
            }
        }
    }

    let pointer_moved = ctx.input(|i| i.pointer.delta() != Vec2::ZERO);
    let mut hot = st.hot;
    let mut opened_sub = None;
    let mut clicked_sub = None;
    let mut plain_hover = false;
    let mut rects = Vec::new();
    let area = Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .sense(Sense::hover());
    let shown = area.show(ctx, |ui| {
        panel_frame(&t).show(ui, |ui| {
            let width = measure(ui, entries, true) - PANEL_MARGIN as f32 * 2.0;
            ui.set_width(width);
            let mut d = Draw {
                t,
                width,
                hot: &mut hot,
                by_key: st.by_key && !st.in_sub,
                focus: true,
                pointer_moved,
                picked: &mut picked,
                rects: Vec::new(),
                opened_sub: None,
                clicked_sub: None,
                plain_hover: false,
            };
            draw_list(ui, entries, &mut d, true);
            rects = std::mem::take(&mut d.rects);
            opened_sub = d.opened_sub;
            clicked_sub = d.clicked_sub;
            plain_hover = d.plain_hover;
        });
    });
    let panel = shown.response.rect;
    st.hot = hot;
    if pointer_moved {
        if let Some(i) = opened_sub {
            if st.sub != Some(i) {
                st.sub = Some(i);
                st.sub_hot = None;
            }
            st.in_sub = false;
        } else if plain_hover {
            st.sub = None;
            st.in_sub = false;
        }
        if st.hot.is_some() && (opened_sub.is_some() || plain_hover) {
            st.by_key = false;
        }
    }
    if let Some(i) = clicked_sub {
        // A click on the row opens its submenu (hovering already did); it never closes it.
        st.sub = Some(i);
        st.in_sub = false;
    }
    let mut sub_rect = None;
    if let Some(i) = st.sub
        && let Some(Entry::Submenu(sub)) = entries.get(i)
        && let Some(row) = rects.get(i).copied()
    {
        let sub_width = measure_list(ctx, &sub.entries);
        let screen = ctx.content_rect();
        let x = if panel.right() + sub_width <= screen.right() {
            panel.right() - 2.0
        } else {
            panel.left() - sub_width + 2.0
        };
        let at = pos2(x, row.top() - PANEL_MARGIN as f32);
        let mut sub_pick: Option<T> = None;
        let mut sub_hot = st.sub_hot;
        let inner = Area::new(id.with("pcb-submenu"))
            .order(Order::Foreground)
            .fixed_pos(at)
            .constrain(true)
            .sense(Sense::hover())
            .show(ctx, |ui| {
                panel_frame(&t).show(ui, |ui| {
                    let width = sub_width - PANEL_MARGIN as f32 * 2.0;
                    ui.set_width(width);
                    let mut d = Draw {
                        t,
                        width,
                        hot: &mut sub_hot,
                        by_key: st.by_key && st.in_sub,
                        focus: true,
                        pointer_moved,
                        picked: &mut sub_pick,
                        rects: Vec::new(),
                        opened_sub: None,
                        clicked_sub: None,
                        plain_hover: false,
                    };
                    draw_list(ui, &sub.entries, &mut d, false);
                });
            });
        sub_rect = Some(inner.response.rect);
        if pointer_moved && sub_hot != st.sub_hot && sub_hot.is_some() {
            st.in_sub = true;
            st.by_key = false;
        }
        st.sub_hot = sub_hot;
        if sub_pick.is_some() {
            picked = sub_pick;
        }
    }

    // A press outside every panel closes the menu; so does scrolling the view.
    let pressed_outside = st.opened_frame != now
        && ctx.input(|i| i.pointer.any_pressed())
        && ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !panel.contains(p) && !sub_rect.is_some_and(|r| r.contains(p)));
    let scrolled = st.opened_frame != now
        && ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, Event::MouseWheel { .. }))
        });
    ctx.data_mut(|d| d.insert_temp(state_id(id), st));
    if let Some(value) = picked {
        ctx.data_mut(|d| d.remove_temp::<State>(state_id(id)));
        return Outcome::Picked(value);
    }
    if closed || pressed_outside || scrolled {
        closed = true;
    }
    if closed {
        ctx.data_mut(|d| d.remove_temp::<State>(state_id(id)));
        return Outcome::Closed;
    }
    Outcome::Open
}

/// Width of a submenu panel, measured without a `Ui`.
fn measure_list<T>(ctx: &Context, entries: &[Entry<T>]) -> f32 {
    let t = theme::tokens(ctx);
    let mut widest = 0.0f32;
    for entry in entries {
        if let Entry::Item(item) = entry {
            let (l, s) = ctx.fonts_mut(|f| {
                (
                    f.layout_no_wrap(item.label.clone(), Role::Body.font(), t.text)
                        .size()
                        .x,
                    if item.shortcut.is_empty() {
                        0.0
                    } else {
                        f.layout_no_wrap(item.shortcut.clone(), shortcut_font(), t.muted)
                            .size()
                            .x
                            + 24.0
                    },
                )
            });
            widest = widest.max(PAD + ICON_SIZE + PAD + l + s + PAD);
        }
    }
    widest.clamp(MIN_WIDTH, MAX_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label_of<T>(entry: &Entry<T>) -> String {
        match entry {
            Entry::Item(item) => item.label.clone(),
            Entry::Submenu(sub) => sub.label.clone(),
            Entry::Separator => "-".into(),
            Entry::Footer { rows, .. } => format!("footer:{}", rows.len()),
        }
    }

    #[test]
    fn groups_are_split_by_separators_and_destructive_rows_come_last() {
        let list = Menu::new()
            .destructive(Item::new(1, "Delete", "Delete it."))
            .group([Item::new(2, "Copy", "Copy it.").into()])
            .group(Vec::<Entry<i32>>::new())
            .group([Item::new(3, "Fit view", "Fit.").into()])
            .footer(vec![("Shift+click".into(), "Add".into())], "Modifier keys.")
            .entries();
        let labels: Vec<String> = list.iter().map(label_of).collect();
        assert_eq!(
            labels,
            ["Copy", "-", "Fit view", "-", "Delete", "-", "footer:1"],
            "an empty group leaves no double separator; Delete sits after the rest"
        );
        assert!(
            matches!(&list[4], Entry::Item(item) if item.danger),
            "a destructive row is drawn in the error colour"
        );
    }

    #[test]
    fn the_arrow_keys_skip_disabled_rows_and_wrap() {
        let rows = [0, 2, 3];
        assert_eq!(step(&rows, None, true), Some(0));
        assert_eq!(step(&rows, None, false), Some(3));
        assert_eq!(
            step(&rows, Some(3), true),
            Some(0),
            "Down from the last row wraps"
        );
        assert_eq!(
            step(&rows, Some(0), false),
            Some(3),
            "Up from the first row wraps"
        );
        assert_eq!(
            step(&rows, Some(0), true),
            Some(2),
            "row 1 is disabled and skipped"
        );
    }

    #[test]
    fn typing_the_first_letters_lands_on_the_row() {
        let entries: Vec<Entry<u8>> = vec![
            Item::new(0, "Copy", "").into(),
            Item::new(1, "Cut", "").into(),
            Item::new(2, "Delete", "").enabled_if(false, "no").into(),
            Item::new(3, "Duplicate", "").into(),
        ];
        let rows = selectable_rows(&entries);
        assert_eq!(rows, [0, 1, 3]);
        assert_eq!(
            type_ahead(&entries, &rows, None, "d"),
            Some(3),
            "a disabled row is not found"
        );
        assert_eq!(
            type_ahead(&entries, &rows, Some(0), "c"),
            Some(1),
            "a repeated letter moves on"
        );
        assert_eq!(
            type_ahead(&entries, &rows, Some(1), "cu"),
            Some(1),
            "more letters narrow down"
        );
        assert_eq!(type_ahead(&entries, &rows, None, "x"), None);
    }

    #[test]
    fn a_shortcut_written_after_two_spaces_moves_to_its_own_column() {
        assert_eq!(
            split_shortcut("Hide  Space"),
            ("Hide".into(), "Space".into())
        );
        assert_eq!(split_shortcut("Zoom to"), ("Zoom to".into(), String::new()));
    }

    /// Every `.rs` file under `dir`, except the ones in `skip`.
    fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                sources(&path, out)?;
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
        Ok(())
    }

    #[test]
    fn no_context_menu_builds_its_own_rows() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files)?;
        let mut offenders = Vec::new();
        for path in files {
            let name = path
                .strip_prefix(&root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            if name == "ui_kit/menu.rs" || name.starts_with("e2e/") {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
            for banned in [
                ".context_menu(",
                "Popup::context_menu",
                "canvas_menu_button",
            ] {
                if text.contains(banned) {
                    offenders.push(format!("{name} uses {banned}"));
                }
            }
            if text.contains("secondary_clicked()")
                && !text.contains("menu::")
                && !text.contains("open_canvas_menu")
            {
                offenders.push(format!(
                    "{name} opens something on a right click without ui_kit::menu"
                ));
            }
        }
        assert!(
            offenders.is_empty(),
            "every right-click menu is drawn by ui_kit/menu.rs; found {offenders:?}"
        );
        Ok(())
    }
}
