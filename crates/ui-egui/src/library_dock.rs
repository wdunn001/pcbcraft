//! The library browser in the Library tab of the left aside: one search box
//! over built-in parts, this project's library, installed libraries and the
//! online part source, filter toggles, a Symbols | Footprints | Both switch
//! that follows the tab, and a card with a small drawing for every part.
//!
//! # Rules
//! - Installed libraries are read on a worker thread through `library.list`;
//!   a frame never waits for them. Typing is debounced and every request
//!   carries a sequence number, so an older answer never replaces a newer one.
//! - An empty search shows built-in and project parts only; installed
//!   libraries are listed once the person types or picks a category.
//! - Filter state lives in `App::dock`, not in the design.
//! - Hiding a set hides its parts from this list only; placed parts stay.
//! - A card dragged onto the drawing places the symbol on the Schematic and
//!   the footprint on the Layout (`part.place`, one undo step); installed
//!   parts are read on a worker first and copied into the project library.
//! - Footprint pictures are drawn once into a cached texture, top view, in
//!   theme colours; the cache is dropped when the theme changes.
//! - Favourites live in the preferences, by card key.
//!
//! # Dependencies
//! - crates/engine/src/library.rs#list_over
//! - crates/library/src/listing.rs
//! - crates/engine/src/library.rs#resolve_footprint_over
//! - crates/engine/src/authoring.rs (`part.place`, `library.import_part`)
//! - crates/ui-egui/src/library.rs#library_geometry_paths
//! - crates/ui-egui/src/icons.rs
//! - crates/ui-egui/src/sources.rs#online_parts
//! - crates/text/strings/en.txt

use super::*;
use crate::library_builtin;
use pcbcraft_library::sources::PartResult;
use pcbcraft_library::{FootprintDefinition, SymbolDefinition};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{Receiver, channel};

/// Seconds the search box must rest before installed libraries are asked.
const DEBOUNCE_SECONDS: f64 = 0.2;
/// Requests allowed in flight; older ones are dropped, never applied.
const MAX_INFLIGHT: usize = 4;
/// Most keyword chips shown.
const MAX_TAGS: usize = 12;
/// Installed results kept per request (the engine caps a page at 100).
const PAGE: usize = 100;

/// Kind toggles: the name kept in state, its label and tip catalog keys, and
/// the listing categories it stands for.
const KINDS: [(&str, &str, &str, &[&str]); 4] = [
    (
        "passive",
        "library.dock.kind.passive",
        "library.dock.kind.passive.tip",
        &[
            "resistor",
            "capacitor",
            "inductor",
            "diode",
            "led",
            "crystal",
        ],
    ),
    (
        "ic",
        "library.dock.kind.ic",
        "library.dock.kind.ic.tip",
        &[
            "ic_regulator",
            "ic_mcu",
            "ic_logic",
            "ic_analog",
            "transistor",
            "mosfet",
        ],
    ),
    (
        "connector",
        "library.dock.kind.connector",
        "library.dock.kind.connector.tip",
        &["connector"],
    ),
    (
        "mechanical",
        "library.dock.kind.mechanical",
        "library.dock.kind.mechanical.tip",
        &["mechanical", "module"],
    ),
];

/// Height of one card row, thumbnail included.
const ROW_HEIGHT: f32 = 96.0;
/// Side of the thumbnail on a card.
const THUMB: f32 = 72.0;

pub(crate) type Reply = Receiver<Result<Value, String>>;

/// One installed part as the listing reports it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Installed {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) package: String,
    pub(crate) category: String,
}

/// Browser state: the filters, the last installed answer, and the drawings.
pub(crate) struct Dock {
    /// Seconds of silence before a request goes out; tests lower it.
    pub(crate) debounce: f64,
    /// Requests started so far.
    pub(crate) requests_sent: u64,
    /// Highest request number whose answer is on screen.
    pub(crate) applied: u64,
    pub(crate) items: Vec<Installed>,
    pub(crate) total: usize,
    pub(crate) error: String,
    /// Source chips switched off.
    pub(crate) hide_built_in: bool,
    pub(crate) hide_project: bool,
    pub(crate) hide_installed: bool,
    pub(crate) hide_online: bool,
    /// Which drawing the cards show; follows the tab until the person picks.
    pub(crate) view: ViewKind,
    view_tab: Option<Tab>,
    /// A `--capture` image asks for the first online card to be placed once it is listed;
    /// holds the frame the placement was started on.
    pub(crate) capture_place: Option<Option<u32>>,
    /// Only starred parts.
    pub(crate) favourites_only: bool,
    /// Kind toggles switched on (`passive`, `ic`, `connector`, `mechanical`).
    pub(crate) kinds: BTreeSet<String>,
    /// Keyword chip switched on.
    pub(crate) tag: Option<String>,
    /// Folders to read instead of the user's installed ones (tests).
    pub(crate) roots: Option<Vec<std::path::PathBuf>>,
    sent: String,
    wanted: String,
    changed_at: f64,
    inflight: Vec<(u64, Reply)>,
    drawings: BTreeMap<String, Vec<Vec<Point>>>,
    drawing_request: Option<(String, Reply)>,
    /// Online parts read for their thumbnail: symbol and footprint by sku; `None` when unreadable.
    online: BTreeMap<String, Option<(SymbolDefinition, Option<FootprintDefinition>)>>,
    online_request: Option<(String, Reply)>,
    placing: Option<(String, String, Reply)>,
    /// Installed footprints by package text; `None` when none was found.
    footprints: BTreeMap<String, Option<FootprintDefinition>>,
    footprint_request: Option<(String, Reply)>,
    thumbs: BTreeMap<String, egui::TextureHandle>,
    thumb_theme: [Color32; 3],
    /// The card being dragged toward the drawing.
    dragged: Option<Card>,
    pending: Option<Pending>,
}

/// What the cards draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewKind {
    #[default]
    Symbols,
    Footprints,
    Both,
}

/// What to do with a part once its definitions are read.
#[derive(Clone)]
enum Intent {
    Place {
        target: String,
        at: Point,
    },
    Import,
    /// Place an online part with `library.place_online`; the worker's answer is its parameters.
    Online {
        target: String,
        at: Point,
    },
}

/// A read of an installed part's definitions on a worker.
struct Pending {
    intent: Intent,
    name: String,
    rx: Reply,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            debounce: DEBOUNCE_SECONDS,
            requests_sent: 0,
            applied: 0,
            items: Vec::new(),
            total: 0,
            error: String::new(),
            hide_built_in: false,
            hide_project: false,
            hide_installed: false,
            hide_online: false,
            view: ViewKind::Symbols,
            view_tab: None,
            capture_place: None,
            favourites_only: false,
            kinds: BTreeSet::new(),
            tag: None,
            roots: None,
            sent: String::new(),
            wanted: String::new(),
            changed_at: 0.0,
            inflight: Vec::new(),
            drawings: BTreeMap::new(),
            drawing_request: None,
            online: BTreeMap::new(),
            online_request: None,
            placing: None,
            footprints: BTreeMap::new(),
            footprint_request: None,
            thumbs: BTreeMap::new(),
            thumb_theme: [Color32::TRANSPARENT; 3],
            dragged: None,
            pending: None,
        }
    }
}

/// A row of the visible list.
#[derive(Clone)]
pub(crate) enum Card {
    BuiltIn(&'static str, &'static str),
    Project(Box<SymbolDefinition>),
    Installed(Installed),
    Online(Box<PartResult>, String),
}

impl Card {
    /// A stable name for favourites and caches.
    pub(crate) fn key(&self) -> String {
        match self {
            Card::BuiltIn(kind, _) => format!("built-in/{kind}"),
            Card::Project(d) => format!("project/{}", d.name),
            Card::Installed(i) => format!("installed/{}/{}", i.path, i.name),
            Card::Online(p, _) => format!("online/{}/{}", p.source, p.mpn),
        }
    }

    pub(crate) fn name(&self) -> String {
        match self {
            Card::BuiltIn(_, label) => (*label).to_owned(),
            Card::Project(d) => d.name.clone(),
            Card::Installed(i) => i.name.clone(),
            Card::Online(p, _) => p.mpn.clone(),
        }
    }
}

/// What a card's menu or buttons asked for.
#[derive(Clone, Debug)]
enum CardAction {
    Place(&'static str),
    Import,
    Favourite,
    Datasheet(String),
    Source,
    /// Open the part's page in the Library tab.
    OpenInLibrary,
}

/// Run `work` off the frame and hand back where its answer will arrive. In
/// the browser build there are no threads, so it runs at once.
pub(crate) fn spawn_read(
    ctx: &egui::Context,
    work: impl FnOnce() -> Result<Value, String> + Send + 'static,
) -> Reply {
    let (tx, rx) = channel();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let ctx = ctx.clone();
        let failed = tx.clone();
        if std::thread::Builder::new()
            .name("pcbcraft-dock".into())
            .spawn(move || {
                let _ = tx.send(work());
                ctx.request_repaint();
            })
            .is_err()
        {
            let _ = failed.send(Err("the reader could not start".into()));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        let _ = tx.send(work());
    }
    rx
}

/// The words of keyword lists across `definitions`, most frequent first.
fn top_tags(definitions: &[SymbolDefinition]) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for d in definitions {
        for word in d.keywords.split_whitespace() {
            if word.len() > 1 {
                *counts.entry(word.to_lowercase()).or_default() += 1;
            }
        }
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.into_iter().take(MAX_TAGS).map(|(w, _)| w).collect()
}

/// True when the query finds this text: a plain substring or letters in order.
fn finds(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    text.contains(query) || fuzzy_match(query, &text)
}

/// The listing category name the library's own table gives this text.
fn category_name(source: &str, text: &str) -> String {
    serde_json::to_value(pcbcraft_library::listing::category_from(source, text))
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

impl App {
    /// The whole left-dock browser body: search, chips, sets, cards, sources.
    ///
    /// # Rules
    /// - Never waits for a library; see the file header.
    /// - Picking a card arms the placement tool and shows the schematic.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library.rs#library_geometry_paths
    ///
    /// # Do not
    /// - Call `pcbcraft_engine::library` on the frame thread.
    pub(crate) fn library_dock_body(&mut self, ui: &mut egui::Ui, viewport: Rect) {
        let now = ui.input(|i| i.time);
        self.dock_poll(ui.ctx());
        self.dock_follow_tab();
        let field = egui::Margin {
            left: 32,
            right: 8,
            top: 6,
            bottom: 6,
        };
        let search = ui
            .add(
                crate::targets::text_field(&mut self.part_search)
                    .hint_text(t!("library.browser.search_parts_by_name"))
                    .margin(field)
                    .desired_width(f32::INFINITY),
            )
            .on_hover_text(t!("library.browser.search_parts_by_name.tip"));
        icons::icon_at(
            ui,
            Rect::from_center_size(
                search.rect.left_center() + egui::vec2(16.0, 0.0),
                egui::Vec2::splat(16.0),
            ),
            "search",
            self.muted(),
        );
        let enabled = search.enabled();
        search.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::TextEdit,
                enabled,
                t!("library.browser.search_parts_by_name"),
            )
        });
        if self.part_search_focus {
            search.request_focus();
            self.part_search_focus = false;
        }
        ui.add_space(6.0);
        self.dock_view_switch(ui);
        ui.add_space(6.0);
        self.dock_chips(ui);
        ui.add_space(4.0);
        let query = self.part_search.trim().to_lowercase();
        let want_installed =
            !self.dock.hide_installed && (!query.is_empty() || !self.dock.kinds.is_empty());
        let signature = if want_installed {
            format!("{query}|{:?}", self.dock.kinds)
        } else {
            String::new()
        };
        if signature != self.dock.wanted {
            self.dock.wanted = signature.clone();
            self.dock.changed_at = now;
        }
        if signature != self.dock.sent {
            if now - self.dock.changed_at >= self.dock.debounce {
                self.dock_request(ui.ctx(), &query, want_installed);
                self.dock.sent = signature;
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(self.dock.debounce));
            }
        }
        let project_parts: Vec<SymbolDefinition> = if self.dock.hide_project {
            Vec::new()
        } else {
            self.project.library.symbols.clone()
        };
        let tags = top_tags(&project_parts);
        if !tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for tag in &tags {
                    let on = self.dock.tag.as_deref() == Some(tag.as_str());
                    if ui
                        .add(
                            egui::Button::new(tag)
                                .selected(on)
                                .min_size(egui::Vec2::splat(crate::targets::MIN_TARGET)),
                        )
                        .on_hover_text(t!("library.dock.tag.tip", tag = tag))
                        .clicked()
                    {
                        self.dock.tag = if on { None } else { Some(tag.clone()) };
                    }
                }
            });
        }
        let mut cards: Vec<Card> = Vec::new();
        let mut sets: BTreeMap<String, usize> = BTreeMap::new();
        for d in &project_parts {
            let matches = query.is_empty()
                || finds(&query, &d.name)
                || d.keywords.to_lowercase().contains(query.as_str())
                || d.set.to_lowercase().contains(query.as_str());
            let tagged = self
                .dock
                .tag
                .as_ref()
                .is_none_or(|t| d.keywords.to_lowercase().split_whitespace().any(|w| w == t));
            let kind_ok =
                self.kind_allows(&category_name("", &format!("{} {}", d.keywords, d.name)));
            if matches && tagged && kind_ok {
                let set = d.set.trim().to_owned();
                if !set.is_empty() {
                    *sets.entry(set.clone()).or_default() += 1;
                }
                if set.is_empty() || !self.hidden_sets.contains(&set) {
                    cards.push(Card::Project(Box::new(d.clone())));
                }
            }
        }
        if !self.dock.hide_installed && self.dock.tag.is_none() {
            for item in &self.dock.items {
                cards.push(Card::Installed(item.clone()));
            }
        }
        if !self.dock.hide_built_in && self.dock.tag.is_none() {
            for t in TEMPLATES {
                let kind_ok = self.kind_allows(&category_name("", t.kind));
                if kind_ok
                    && (query.is_empty()
                        || finds(&query, t.label)
                        || t.kind.contains(query.as_str()))
                {
                    cards.push(Card::BuiltIn(t.kind, t.label));
                }
            }
        }
        if !self.dock.hide_online && self.dock.tag.is_none() {
            for (part, credit) in self.online_parts(ui) {
                cards.push(Card::Online(Box::new(part), credit));
            }
        }
        let favourites = self.preferences.library_favourites.clone();
        if self.dock.favourites_only {
            cards.retain(|c| favourites.contains(&c.key()));
        }
        if self.dock.view == ViewKind::Footprints {
            cards.retain(|c| match c {
                Card::BuiltIn(..) => true,
                Card::Project(d) => !d.footprint.trim().is_empty(),
                Card::Installed(i) => !i.package.trim().is_empty(),
                Card::Online(..) => true,
            });
        }
        if self.dock.capture_place == Some(None)
            && self.dock.pending.is_none()
            && let Some(card) = cards
                .iter()
                .find(|c| matches!(c, Card::Online(..)))
                .cloned()
        {
            let (target, at) = if self.tab == Tab::Pcb {
                ("board", self.pcb.center)
            } else {
                ("schematic", self.schematic.center)
            };
            self.dock_place(ui.ctx(), &card, target, at);
            self.dock.capture_place = Some(Some(self.frames));
        }
        self.dock_sets(ui, &sets);
        if want_installed && !self.dock.inflight.is_empty() {
            ui.label(
                egui::RichText::new(t!("library.dock.searching"))
                    .font(crate::fonts::Role::Caption.font())
                    .color(self.muted()),
            );
        } else if !self.dock.error.is_empty() {
            ui.label(
                egui::RichText::new(self.dock.error.clone())
                    .font(crate::fonts::Role::Caption.font())
                    .color(self.muted()),
            );
        } else if self.dock.total > self.dock.items.len() && want_installed {
            ui.label(
                egui::RichText::new(t!(
                    "library.dock.more",
                    shown = self.dock.items.len(),
                    total = self.dock.total
                ))
                .font(crate::fonts::Role::Caption.font())
                .color(self.muted()),
            );
        }
        let top = ui.cursor().min.y - ui.min_rect().min.y;
        let first =
            (((viewport.min.y - top) / ROW_HEIGHT).floor().max(0.0) as usize).min(cards.len());
        let last =
            (((viewport.max.y - top) / ROW_HEIGHT).ceil().max(0.0) as usize + 1).min(cards.len());
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(first as f32 * ROW_HEIGHT);
        for card in &cards[first..last] {
            self.dock_card(ui, card);
        }
        ui.add_space((cards.len() - last) as f32 * ROW_HEIGHT);
        ui.spacing_mut().item_spacing.y = 4.0;
        let hint = if cards.is_empty() {
            Some(t!(
                "library.browser.nothing_here_matches_trim_yet",
                trim = self.part_search.trim()
            ))
        } else if !want_installed && !self.dock.hide_installed {
            Some(t!("library.dock.type_for_installed"))
        } else {
            None
        };
        if let Some(text) = hint {
            self.dock_empty_state(ui, &text);
        }
        ui.add_space(6.0);
        if ui
            .link(t!("library.browser.browse_component_libraries"))
            .on_hover_text(t!("library.browser.browse_component_libraries.tip"))
            .clicked()
        {
            self.change_tab(Tab::Library);
        }
    }

    /// The message under or instead of the list: centred, wrapped, in the
    /// list area with real padding so it reads as the state of the list.
    fn dock_empty_state(&self, ui: &mut egui::Ui, text: &str) {
        ui.add_space(14.0);
        ui.vertical_centered(|ui| {
            ui.add(egui::Label::new(crate::fonts::caption(text).color(self.muted())).wrap());
        });
        ui.add_space(8.0);
    }

    /// True when no kind toggle is on, or this listing category is in one.
    fn kind_allows(&self, category: &str) -> bool {
        self.dock.kinds.is_empty()
            || KINDS
                .iter()
                .any(|(k, .., cats)| self.dock.kinds.contains(*k) && cats.contains(&category))
    }

    /// Follow the tab: symbols on the Schematic, footprints on the Layout.
    /// A pick from the switch holds until the tab changes again.
    fn dock_follow_tab(&mut self) {
        if self.dock.view_tab != Some(self.tab) {
            self.dock.view_tab = Some(self.tab);
            match self.tab {
                Tab::Schematic => self.dock.view = ViewKind::Symbols,
                Tab::Pcb => self.dock.view = ViewKind::Footprints,
                Tab::Library => self.dock.view = ViewKind::Both,
                _ => {}
            }
        }
    }

    /// The small Symbols | Footprints | Both control under the search.
    fn dock_view_switch(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (view, label, tip) in [
                (
                    ViewKind::Symbols,
                    "library.dock.view.symbols",
                    "library.dock.view.symbols.tip",
                ),
                (
                    ViewKind::Footprints,
                    "library.dock.view.footprints",
                    "library.dock.view.footprints.tip",
                ),
                (
                    ViewKind::Both,
                    "library.dock.view.both",
                    "library.dock.view.both.tip",
                ),
            ] {
                let name = pcbcraft_text::lookup(label, &[]);
                let on = self.dock.view == view;
                let response = ui
                    .add(
                        egui::Button::new(name.clone())
                            .selected(on)
                            .min_size(egui::vec2(72.0, crate::targets::MIN_TARGET)),
                    )
                    .on_hover_text(pcbcraft_text::lookup(tip, &[]));
                crate::a11y::name_control(&response, &name, Some(on));
                if response.clicked() {
                    self.dock.view = view;
                }
            }
        });
    }

    /// One row of small icon toggles: four sources, three kinds, favourites,
    /// and a "..." menu for the rest (mechanical parts, clear all).
    fn dock_chips(&mut self, ui: &mut egui::Ui) {
        const SIDE: f32 = 24.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.spacing_mut().button_padding = egui::vec2(2.0, 2.0);
            let sources = [
                ("box", self.dock.hide_built_in, "library.dock.built_in.tip"),
                ("folder", self.dock.hide_project, "library.dock.project.tip"),
                (
                    "library",
                    self.dock.hide_installed,
                    "library.dock.installed.tip",
                ),
                ("globe", self.dock.hide_online, "library.dock.online.tip"),
            ];
            for (i, (icon, hidden, tip)) in sources.into_iter().enumerate() {
                if icons::icon_button_sized(
                    ui,
                    icon,
                    &pcbcraft_text::lookup(tip, &[]),
                    !hidden,
                    SIDE,
                )
                .clicked()
                {
                    match i {
                        0 => self.dock.hide_built_in = !hidden,
                        1 => self.dock.hide_project = !hidden,
                        2 => self.dock.hide_installed = !hidden,
                        _ => self.dock.hide_online = !hidden,
                    }
                }
            }
            let icon_names = ["omega", "cpu", "plug", "settings"];
            for ((name, _, tip, _), icon) in KINDS.iter().zip(icon_names).take(3) {
                let on = self.dock.kinds.contains(*name);
                if icons::icon_button_sized(ui, icon, &pcbcraft_text::lookup(tip, &[]), on, SIDE)
                    .clicked()
                {
                    if on {
                        self.dock.kinds.remove(*name);
                    } else {
                        self.dock.kinds.insert((*name).to_owned());
                    }
                }
            }
            if icons::icon_button_sized(
                ui,
                "star",
                &pcbcraft_text::lookup("library.dock.favourites.tip", &[]),
                self.dock.favourites_only,
                SIDE,
            )
            .clicked()
            {
                self.dock.favourites_only = !self.dock.favourites_only;
            }
            // A full 24 px target, like the icon buttons beside it.
            let more_button = egui::Button::new(t!("library.dock.more_filters.label"))
                .min_size(egui::Vec2::splat(crate::targets::MIN_TARGET));
            let more = egui::containers::menu::MenuButton::from_button(more_button)
                .ui(ui, |ui| {
                    if let Some((name, _, tip, _)) = KINDS.get(3) {
                        let on = self.dock.kinds.contains(*name);
                        let label = pcbcraft_text::lookup("library.dock.kind.mechanical", &[]);
                        if ui
                            .selectable_label(on, label)
                            .on_hover_text(pcbcraft_text::lookup(tip, &[]))
                            .clicked()
                        {
                            if on {
                                self.dock.kinds.remove(*name);
                            } else {
                                self.dock.kinds.insert((*name).to_owned());
                            }
                            ui.close();
                        }
                    }
                    self.online_menu(ui);
                    if ui
                        .button(t!("library.dock.clear_filters"))
                        .on_hover_text(t!("library.dock.clear_filters.tip"))
                        .clicked()
                    {
                        self.dock.kinds.clear();
                        self.dock.favourites_only = false;
                        self.dock.tag = None;
                        self.dock.hide_built_in = false;
                        self.dock.hide_project = false;
                        self.dock.hide_installed = false;
                        self.dock.hide_online = false;
                        ui.close();
                    }
                })
                .0
                .on_hover_text(t!("library.dock.more_filters.tip"));
            crate::a11y::name_control(&more, &t!("library.dock.more_filters"), None);
        });
    }

    /// One tickbox per set of the project's parts.
    fn dock_sets(&mut self, ui: &mut egui::Ui, sets: &BTreeMap<String, usize>) {
        for (set, count) in sets {
            let mut shown = !self.hidden_sets.contains(set);
            let row = ui
                .checkbox(
                    &mut shown,
                    egui::RichText::new(t!(
                        "library.browser.set_len_part_v1",
                        set = set,
                        len = count,
                        v1 = if *count == 1 { "" } else { "s" }
                    ))
                    .strong(),
                )
                .on_hover_text(t!("library.browser.set_len_part_v1.tip"));
            if row.changed() {
                if shown {
                    self.hidden_sets.retain(|h| h != set);
                } else {
                    self.hidden_sets.push(set.clone());
                }
            }
        }
    }

    /// Start a `library.list` read for installed parts, or clear the answer
    /// when installed parts are not wanted any more.
    fn dock_request(&mut self, ctx: &egui::Context, query: &str, want_installed: bool) {
        self.dock.requests_sent += 1;
        let seq = self.dock.requests_sent;
        if !want_installed {
            self.dock.items.clear();
            self.dock.total = 0;
            self.dock.error.clear();
            self.dock.applied = seq;
            self.dock.inflight.clear();
            return;
        }
        let mut filter = json!({"sources": ["installed"]});
        if !query.is_empty() {
            filter["text"] = json!(query);
        }
        if !self.dock.kinds.is_empty() {
            let wanted: Vec<&str> = KINDS
                .iter()
                .filter(|(k, ..)| self.dock.kinds.contains(*k))
                .flat_map(|(.., cats)| cats.iter().copied())
                .collect();
            filter["categories"] = json!(wanted);
        }
        let args = json!({"filter": filter, "limit": PAGE});
        let project = self.project.clone();
        let roots = self.dock.roots.clone();
        let reply = spawn_read(ctx, move || match roots {
            Some(roots) => pcbcraft_engine::library::list_over(&args, Some(&project), roots),
            None => pcbcraft_engine::library::list_with_project(&args, Some(&project)),
        });
        self.dock.inflight.push((seq, reply));
        while self.dock.inflight.len() > MAX_INFLIGHT {
            self.dock.inflight.remove(0);
        }
    }

    /// Take in finished reads: a list answer only when it is newer than what
    /// is shown, a drawing for the cache, a part ready to place.
    fn dock_poll(&mut self, ctx: &egui::Context) {
        let mut done: Vec<(u64, Result<Value, String>)> = Vec::new();
        self.dock.inflight.retain(|(seq, rx)| match rx.try_recv() {
            Ok(result) => {
                done.push((*seq, result));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        done.sort_by_key(|(seq, _)| *seq);
        for (seq, result) in done {
            if seq <= self.dock.applied {
                continue;
            }
            self.dock.applied = seq;
            match result {
                Ok(value) => {
                    self.dock.error.clear();
                    self.dock.total = value
                        .get("total_known")
                        .and_then(Value::as_u64)
                        .and_then(|n| usize::try_from(n).ok())
                        .unwrap_or(0);
                    self.dock.items = value
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|all| {
                            all.iter()
                                .filter_map(|i| {
                                    Some(Installed {
                                        path: i.get("source_id")?.as_str()?.to_owned(),
                                        name: i.get("source_ref")?.as_str()?.to_owned(),
                                        description: i
                                            .get("description")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_owned(),
                                        package: i
                                            .get("package_text")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_owned(),
                                        category: i
                                            .get("category")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_owned(),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                }
                Err(e) => {
                    self.dock.items.clear();
                    self.dock.total = 0;
                    self.dock.error = e;
                }
            }
        }
        if let Some((key, rx)) = &self.dock.drawing_request {
            match rx.try_recv() {
                Ok(result) => {
                    let paths = result
                        .ok()
                        .and_then(|v| v.get("definition").cloned())
                        .and_then(|d| serde_json::from_value::<SymbolDefinition>(d).ok())
                        .map(|d| library::library_geometry_paths(Some(&d), None, 1))
                        .unwrap_or_default();
                    self.dock.drawings.insert(key.clone(), paths);
                    self.dock.drawing_request = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.dock.drawing_request = None;
                }
            }
        }
        if let Some((sku, rx)) = &self.dock.online_request {
            let sku = sku.clone();
            match rx.try_recv() {
                Ok(result) => {
                    let found = result.ok().and_then(|v| {
                        let symbol =
                            serde_json::from_value::<SymbolDefinition>(v.get("symbol")?.clone())
                                .ok()?;
                        let footprint = v.get("footprint").and_then(|f| {
                            serde_json::from_value::<FootprintDefinition>(f.clone()).ok()
                        });
                        Some((symbol, footprint))
                    });
                    if let Some((symbol, _)) = &found {
                        self.dock.drawings.insert(
                            format!("online/{sku}"),
                            library::library_geometry_paths(Some(symbol), None, 1),
                        );
                    }
                    self.dock.online.insert(sku, found);
                    self.dock.online_request = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.dock.online.insert(sku, None);
                    self.dock.online_request = None;
                }
            }
        }
        if let Some((package, rx)) = &self.dock.footprint_request {
            let package = package.clone();
            match rx.try_recv() {
                Ok(result) => {
                    let found = result
                        .ok()
                        .and_then(|v| serde_json::from_value::<FootprintDefinition>(v).ok());
                    self.dock.footprints.insert(package, found);
                    self.dock.footprint_request = None;
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.dock.footprints.insert(package, None);
                    self.dock.footprint_request = None;
                }
            }
        }
        if let Some(pending) = self.dock.pending.take() {
            match pending.rx.try_recv() {
                Ok(result) => self.dock_finish(pending.intent, &pending.name, result),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    self.dock.pending = Some(pending);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        if let Some((path, name, rx)) = &self.dock.placing {
            match rx.try_recv() {
                Ok(result) => {
                    let (path, name) = (path.clone(), name.clone());
                    self.dock.placing = None;
                    match result
                        .ok()
                        .and_then(|v| v.get("definition").cloned())
                        .and_then(|d| serde_json::from_value::<SymbolDefinition>(d).ok())
                    {
                        Some(def) => {
                            self.change_tab(Tab::Schematic);
                            self.tool = Tool::LibraryPlace(path, name, Box::new(def));
                        }
                        None => self.message = t!("library.dock.could_not_read", name = name),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.dock.placing = None,
            }
        }
    }

    /// Facts a card shows, gathered before it is drawn.
    fn card_facts(&mut self, ui: &egui::Ui, card: &Card) -> CardFacts {
        match card {
            Card::BuiltIn(kind, label) => CardFacts {
                name: (*label).to_owned(),
                what: library_builtin::describe(kind),
                source: t!(
                    "library.dock.note.built_in_licence",
                    licence = env!("CARGO_PKG_LICENSE")
                ),
                meta: t!("library.dock.builtin.meta"),
                licence: None,
                outside: false,
                armed: matches!(&self.tool, Tool::Place(k) if k == kind),
                symbol: None,
            },
            Card::Project(d) => {
                let key = format!("project/{}/{}", d.name, d.graphics.len() + d.pins.len());
                if !self.dock.drawings.contains_key(&key) {
                    let paths = library::library_geometry_paths(Some(d), None, 1);
                    self.dock.drawings.insert(key.clone(), paths);
                }
                let mut facts: Vec<String> = Vec::new();
                if !d.footprint.trim().is_empty() {
                    facts.push(d.footprint.trim().to_owned());
                }
                if !d.pins.is_empty() {
                    facts.push(t!("library.dock.pins", count = d.pins.len()));
                }
                let source = if d.set.trim().is_empty() {
                    t!("library.dock.note.project")
                } else {
                    d.set.trim().to_owned()
                };
                CardFacts {
                    name: d.name.clone(),
                    what: d.description.trim().to_owned(),
                    source,
                    meta: facts.join(" \u{b7} "),
                    licence: None,
                    outside: false,
                    armed: matches!(&self.tool, Tool::CustomPlace(n, _) if *n == d.name),
                    symbol: Some(key),
                }
            }
            Card::Installed(i) => {
                let library = i
                    .path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&i.path)
                    .trim_end_matches(".kicad_sym")
                    .to_owned();
                let key = format!("installed/{}/{}", i.path, i.name);
                if !self.dock.drawings.contains_key(&key) && self.dock.drawing_request.is_none() {
                    let (path, name) = (i.path.clone(), i.name.clone());
                    let reply = spawn_read(ui.ctx(), move || {
                        pcbcraft_engine::library::read_tool(
                            "library.inspect_symbol",
                            &json!({"path": path, "name": name}),
                        )
                    });
                    self.dock.drawing_request = Some((key.clone(), reply));
                }
                CardFacts {
                    name: i.name.clone(),
                    what: i.description.trim().to_owned(),
                    licence: Some(t!("library.dock.licence.installed", library = library)),
                    source: library,
                    meta: i.package.clone(),
                    outside: true,
                    armed: matches!(&self.tool, Tool::LibraryPlace(p, n, _) if *p == i.path && *n == i.name),
                    symbol: Some(key),
                }
            }
            Card::Online(p, credit) => {
                if !self.dock.online.contains_key(&p.sku)
                    && self.dock.online_request.is_none()
                    && let Some(source) = self.part_sources.open()
                {
                    let sku = p.sku.clone();
                    let reply = spawn_read(ui.ctx(), {
                        let sku = sku.clone();
                        move || library_online::peek_open(&source, &sku)
                    });
                    self.dock.online_request = Some((sku, reply));
                }
                let def = self.dock.online.get(&p.sku).and_then(Option::as_ref);
                let own = def
                    .map(|(d, _)| {
                        let words = d.keywords.trim();
                        match (d.description.trim(), words.is_empty()) {
                            ("", true) => String::new(),
                            ("", false) => words.to_owned(),
                            (text, true) => text.to_owned(),
                            (text, false) => format!("{text} \u{b7} {words}"),
                        }
                    })
                    .filter(|t| !t.is_empty());
                let alone = p
                    .sku
                    .strip_prefix(pcbcraft_library::open_library::FOOTPRINT_SKU_PREFIX);
                let mut meta = if let Some(rest) = alone {
                    rest.split('/').next().unwrap_or_default().to_owned()
                } else if p.distributor.is_empty() && p.price_breaks.is_empty() {
                    p.sku.clone()
                } else {
                    sources::stock_line(p)
                };
                if let (Some(_), Some((_, Some(fp)))) = (alone, def) {
                    meta = format!(
                        "{meta} \u{b7} {}",
                        t!("library.dock.pads", count = fp.pads.len())
                    );
                } else if let Some((d, _)) = def {
                    let extra = if d.footprint.trim().is_empty() {
                        t!("library.dock.pins", count = d.pins.len())
                    } else {
                        d.footprint.trim().to_owned()
                    };
                    meta = format!("{meta} \u{b7} {extra}");
                }
                let licence_name = credit.split(". ").next().unwrap_or_default();
                CardFacts {
                    name: p.mpn.clone(),
                    what: own.unwrap_or_else(|| {
                        if def.is_some() || self.dock.online.contains_key(&p.sku) {
                            String::new()
                        } else {
                            t!("library.dock.loading")
                        }
                    }),
                    source: if licence_name.is_empty() {
                        p.source.clone()
                    } else {
                        format!("{} \u{b7} {licence_name}", p.source)
                    },
                    meta,
                    licence: None,
                    outside: true,
                    armed: false,
                    symbol: def.map(|_| format!("online/{}", p.sku)),
                }
            }
        }
    }

    /// The footprint picture of a card as a texture, drawing and caching it
    /// on first sight; installed packages are looked up on a worker.
    fn footprint_art(&mut self, ctx: &egui::Context, card: &Card) -> Option<egui::TextureId> {
        let theme = self.colours();
        let tints = [theme.canvas.pad, theme.muted, theme.surface];
        if self.dock.thumb_theme != tints {
            self.dock.thumbs.clear();
            self.dock.thumb_theme = tints;
        }
        let (key, found) = match card {
            Card::Project(d) if !d.footprint.trim().is_empty() => {
                let def = self
                    .project
                    .library
                    .footprints
                    .iter()
                    .find(|f| f.name == d.footprint)?;
                (
                    format!(
                        "project/{}/{}/{}",
                        def.name,
                        def.pads.len(),
                        def.graphics.len()
                    ),
                    Some(def.clone()),
                )
            }
            Card::Installed(i) if !i.package.trim().is_empty() => {
                let package = i.package.trim().to_owned();
                if let Some(found) = self.dock.footprints.get(&package) {
                    (format!("installed/{package}"), found.clone())
                } else {
                    if self.dock.footprint_request.is_none() {
                        let (want, roots) = (package.clone(), self.dock.roots.clone());
                        let reply = spawn_read(ctx, move || {
                            match roots {
                                Some(roots) => {
                                    pcbcraft_engine::library::resolve_footprint_over(roots, &want)
                                }
                                None => pcbcraft_engine::library::resolve_footprint(&want),
                            }
                            .and_then(|d| serde_json::to_value(d).map_err(|e| e.to_string()))
                        });
                        self.dock.footprint_request = Some((package, reply));
                    }
                    return None;
                }
            }
            Card::Online(p, _) => {
                let def = self
                    .dock
                    .online
                    .get(&p.sku)
                    .and_then(Option::as_ref)
                    .and_then(|(_, f)| f.clone())?;
                (format!("online/{}", p.sku), Some(def))
            }
            Card::BuiltIn(kind, _) => {
                (format!("built-in/{kind}"), library_builtin::footprint(kind))
            }
            _ => return None,
        };
        if let Some(texture) = self.dock.thumbs.get(&key) {
            return Some(texture.id());
        }
        let definition = found?;
        let image = footprint_image(&definition, tints[0], tints[1], tints[2]);
        let texture = ctx.load_texture(
            format!("footprint:{key}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        let id = texture.id();
        self.dock.thumbs.insert(key, texture);
        Some(id)
    }

    /// Draw the card's symbol large inside `rect`, as the card's own thumbnail
    /// does. Returns false when there is nothing to draw yet (an online or
    /// installed symbol still being read).
    ///
    /// # Rules
    /// - Reads the same drawing cache the cards fill; it never starts a second read.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library.rs (`paint_library_paths`)
    pub(crate) fn dock_paint_symbol(&mut self, ui: &mut egui::Ui, rect: Rect, card: &Card) -> bool {
        let facts = self.card_facts(ui, card);
        let painter = ui.painter_at(rect);
        let drawn = facts
            .symbol
            .as_ref()
            .and_then(|k| self.dock.drawings.get(k))
            .filter(|p| !p.is_empty())
            .cloned();
        if let Some(paths) = drawn {
            let (center, extent) = library::library_paths_extent(&paths);
            let camera = Camera {
                center,
                zoom: ((rect.width() - 24.0) / extent.x as f32)
                    .min((rect.height() - 24.0) / extent.y as f32)
                    .clamp(0.01, 60.0),
                mirror: false,
            };
            library::paint_library_paths(&painter, rect, camera, &paths, self.accent());
            return true;
        }
        if let Card::BuiltIn(kind, _) = card
            && let Some(part) =
                pcbcraft_doc::component_template(kind, 0, String::new(), Point::new(0.0, 0.0))
        {
            let camera = Camera {
                center: Point::new(0.0, 0.0),
                zoom: (rect.height() / 14.0).clamp(2.0, 12.0),
                mirror: false,
            };
            for view in part.schematic_views() {
                self.symbol(
                    &painter.with_clip_rect(rect),
                    rect,
                    camera,
                    &view,
                    false,
                    true,
                );
            }
            return true;
        }
        false
    }

    /// Draw the card's footprint picture inside `rect`. Returns false while
    /// the footprint is unknown or still being looked up.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library_dock.rs (`footprint_art`, which caches the picture)
    pub(crate) fn dock_paint_footprint(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        card: &Card,
    ) -> bool {
        match self.footprint_art(ui.ctx(), card) {
            Some(texture) => {
                let side = rect.width().min(rect.height());
                let at = Rect::from_center_size(rect.center(), egui::Vec2::splat(side));
                ui.painter_at(rect).image(
                    texture,
                    at,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                true
            }
            None => false,
        }
    }

    /// The words a card shows: name, what it is, where it is from and its
    /// line of facts, for the Library tab's page of the same part.
    pub(crate) fn dock_summary(
        &mut self,
        ui: &egui::Ui,
        card: &Card,
    ) -> (String, String, String, String) {
        let f = self.card_facts(ui, card);
        (f.name, f.what, f.source, f.meta)
    }

    /// One part card: a thumbnail, the name, a two-line description, the
    /// source in the accent colour, a muted line of facts and, for a part
    /// not in the project, a badge and its licence. The whole card picks the
    /// part, drags onto the drawing, opens a menu on right click, and shows
    /// a star and an Add to project button on hover.
    fn dock_card(&mut self, ui: &mut egui::Ui, card: &Card) {
        let facts = self.card_facts(ui, card);
        let key = card.key();
        let starred = self.preferences.library_favourites.contains(&key);
        let width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::click_and_drag());
        let enabled = response.enabled();
        let label = format!("{}, {}", facts.name, facts.source);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                enabled,
                facts.armed,
                label.clone(),
            )
        });
        let response = match card {
            Card::Online(_, credit) if !credit.is_empty() => response.on_hover_text(format!(
                "{}\n{}",
                t!("library.dock.pick.tip"),
                t!("library.dock.licence.online", credit = credit)
            )),
            _ => response.on_hover_text(t!("library.dock.pick.tip")),
        };
        let hovered = ui.rect_contains_pointer(rect);
        let accent = self.accent();
        let painter = ui.painter_at(rect);
        if facts.armed {
            painter.rect_filled(rect, 0.0, accent.gamma_multiply(0.16));
            painter.rect_filled(
                Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                0.0,
                accent,
            );
        } else if hovered {
            painter.rect_filled(rect, 0.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        painter.hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        let thumb = Rect::from_min_size(
            rect.min + egui::vec2(12.0, (ROW_HEIGHT - THUMB) / 2.0),
            egui::Vec2::splat(THUMB),
        );
        painter.rect_filled(thumb, 6.0, self.surface());
        let want_symbol = self.dock.view != ViewKind::Footprints;
        let want_footprint = self.dock.view != ViewKind::Symbols;
        let art = if want_footprint {
            self.footprint_art(ui.ctx(), card)
        } else {
            None
        };
        let drawn = if want_symbol {
            facts
                .symbol
                .as_ref()
                .and_then(|k| self.dock.drawings.get(k))
                .filter(|p| !p.is_empty())
                .cloned()
        } else {
            None
        };
        let both = want_symbol && want_footprint;
        let picture = if both {
            Rect::from_min_size(thumb.min, egui::Vec2::splat(THUMB))
        } else {
            thumb
        };
        if let Some(paths) = &drawn {
            let (center, extent) = library::library_paths_extent(paths);
            let camera = Camera {
                center,
                zoom: ((picture.width() - 12.0) / extent.x as f32)
                    .min((picture.height() - 12.0) / extent.y as f32)
                    .clamp(0.01, 40.0),
                mirror: false,
            };
            library::paint_library_paths(&painter, picture, camera, paths, accent);
        } else if want_symbol
            && let Card::BuiltIn(kind, _) = card
            && let Some(part) =
                pcbcraft_doc::component_template(kind, 0, String::new(), Point::new(0.0, 0.0))
        {
            let camera = Camera {
                center: Point::new(0.0, 0.0),
                zoom: 3.6,
                mirror: false,
            };
            for view in part.schematic_views() {
                self.symbol(
                    &painter.with_clip_rect(picture),
                    picture,
                    camera,
                    &view,
                    false,
                    true,
                );
            }
        } else if want_symbol && !want_footprint {
            painter.text(
                thumb.center(),
                egui::Align2::CENTER_CENTER,
                facts
                    .name
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string(),
                crate::fonts::Role::Display.font(),
                self.muted(),
            );
        }
        if want_footprint {
            let spot = if both {
                Rect::from_min_size(
                    thumb.right_bottom() - egui::Vec2::splat(THUMB * 0.5 + 2.0),
                    egui::Vec2::splat(THUMB * 0.5),
                )
            } else {
                thumb
            };
            if both {
                painter.rect_filled(spot, 4.0, self.surface());
                painter.rect_stroke(
                    spot,
                    4.0,
                    Stroke::new(1.0, self.muted().gamma_multiply(0.5)),
                    egui::StrokeKind::Inside,
                );
            }
            match art {
                Some(texture) => {
                    painter.image(
                        texture,
                        spot.shrink(2.0),
                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    // A part with no footprint of its own shows its symbol,
                    // never the generic chip.
                    let symbol = facts
                        .symbol
                        .as_ref()
                        .and_then(|k| self.dock.drawings.get(k))
                        .filter(|p| !p.is_empty())
                        .cloned();
                    if let (Card::Online(..), Some(paths)) = (card, symbol) {
                        let (center, extent) = library::library_paths_extent(&paths);
                        let camera = Camera {
                            center,
                            zoom: ((spot.width() - 12.0) / extent.x as f32)
                                .min((spot.height() - 12.0) / extent.y as f32)
                                .clamp(0.01, 40.0),
                            mirror: false,
                        };
                        library::paint_library_paths(&painter, spot, camera, &paths, accent);
                    } else {
                        icons::icon_at(
                            ui,
                            Rect::from_center_size(spot.center(), spot.size() * 0.45),
                            "footprint",
                            self.muted(),
                        );
                    }
                }
            }
        }
        if let Card::Online(p, _) = card
            && !self.dock.online.contains_key(&p.sku)
        {
            let spot = Rect::from_center_size(
                thumb.right_bottom() - egui::vec2(10.0, 10.0),
                egui::Vec2::splat(12.0),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(spot));
            child.add(egui::Spinner::new().size(12.0).color(self.muted()));
        }
        let text_left = thumb.right() + 12.0;
        let text_width = (rect.right() - 12.0 - text_left).max(40.0);
        let mut y = rect.top() + 9.0;
        let title = painter.layout(
            facts.name.clone(),
            crate::fonts::Role::Body.font(),
            ui.visuals().strong_text_color(),
            (text_width - 26.0).max(40.0),
        );
        let title_height = title.size().y;
        painter.galley(egui::pos2(text_left, y), title, Color32::WHITE);
        y += title_height + 2.0;
        if !facts.what.is_empty() {
            let mut job = egui::text::LayoutJob::simple(
                facts.what.clone(),
                crate::fonts::Role::Body.font(),
                self.muted(),
                text_width,
            );
            job.wrap.max_rows = 2;
            job.wrap.break_anywhere = false;
            job.wrap.overflow_character = Some('\u{2026}');
            let galley = painter.layout_job(job);
            let h = galley.size().y;
            painter.galley(egui::pos2(text_left, y), galley, Color32::WHITE);
            y += h + 2.0;
        }
        let source_galley = painter.layout(
            facts.source.clone(),
            crate::fonts::Role::Caption.font(),
            accent,
            text_width,
        );
        let h = source_galley.size().y;
        let source_width = source_galley.size().x;
        painter.galley(egui::pos2(text_left, y), source_galley, Color32::WHITE);
        if facts.outside {
            let badge = painter.layout_no_wrap(
                t!("library.dock.not_in_project"),
                crate::fonts::Role::Caption.font(),
                self.muted(),
            );
            let left = text_left + source_width + 6.0;
            if left + badge.size().x + 8.0 < rect.right() - 6.0 {
                let pill = Rect::from_min_size(
                    egui::pos2(left, y - 1.0),
                    badge.size() + egui::vec2(8.0, 2.0),
                );
                painter.rect_stroke(
                    pill,
                    7.0,
                    Stroke::new(1.0, self.muted().gamma_multiply(0.7)),
                    egui::StrokeKind::Inside,
                );
                painter.galley(pill.min + egui::vec2(4.0, 1.0), badge, Color32::WHITE);
            }
        }
        y += h + 1.0;
        for line in [facts.meta.clone()]
            .into_iter()
            .chain(facts.licence.clone())
            .filter(|l| !l.is_empty())
        {
            if y + 12.0 < rect.bottom() {
                let mut job = egui::text::LayoutJob::simple(
                    line,
                    crate::fonts::Role::Caption.font(),
                    self.muted(),
                    text_width,
                );
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                job.wrap.overflow_character = Some('\u{2026}');
                let galley = painter.layout_job(job);
                let h = galley.size().y;
                painter.galley(egui::pos2(text_left, y), galley, Color32::WHITE);
                y += h + 1.0;
            }
        }
        let mut action: Option<CardAction> = None;
        if hovered || starred {
            let spot = Rect::from_min_size(
                egui::pos2(rect.right() - 12.0 - 26.0, rect.top() + 4.0),
                egui::Vec2::splat(26.0),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(spot));
            let (icon, tip) = if starred {
                ("star-filled", t!("library.dock.unstar.tip"))
            } else {
                ("star", t!("library.dock.star.tip"))
            };
            if icons::icon_button(&mut child, icon, &tip, starred).clicked() {
                action = Some(CardAction::Favourite);
            }
        }
        if hovered && self.tab != Tab::Library {
            let spot = Rect::from_min_size(
                egui::pos2(rect.right() - 12.0 - 26.0 - 28.0, rect.top() + 4.0),
                egui::Vec2::splat(26.0),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(spot));
            if icons::icon_button(
                &mut child,
                "open",
                &t!("library.dock.open_in_library.tip"),
                false,
            )
            .clicked()
            {
                action = Some(CardAction::OpenInLibrary);
            }
        }
        if hovered && facts.outside && matches!(card, Card::Installed(_)) {
            let spot = Rect::from_min_size(
                egui::pos2(rect.right() - 12.0 - 132.0, rect.bottom() - 30.0),
                egui::vec2(132.0, 24.0),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(spot));
            if icons::icon_label_button(
                &mut child,
                "add",
                &t!("library.dock.add_to_project"),
                &t!("library.dock.add_to_project.tip"),
                false,
            )
            .clicked()
            {
                action = Some(CardAction::Import);
            }
        }
        if let Some(picked) = crate::ui_kit::menu::context(&response, "library-card", || {
            use crate::ui_kit::menu::{Item, Menu};
            use crate::ui_kit::menu_icons as glyph;
            let mut place = vec![
                Item::new(
                    CardAction::Place("schematic"),
                    t!("library.dock.place_schematic"),
                    t!("library.dock.place_schematic.tip"),
                )
                .icon(glyph::PLACE)
                .into(),
            ];
            if self.tab == Tab::Pcb {
                place.push(
                    Item::new(
                        CardAction::Place("board"),
                        t!("library.dock.place_board"),
                        t!("library.dock.place_board.tip"),
                    )
                    .icon(glyph::PLACE)
                    .into(),
                );
            }
            let mut edit = Vec::new();
            if self.tab != Tab::Library {
                edit.push(
                    Item::new(
                        CardAction::OpenInLibrary,
                        t!("library.dock.open_in_library"),
                        t!("library.dock.open_in_library.tip"),
                    )
                    .icon(glyph::PROPERTIES)
                    .into(),
                );
            }
            if matches!(card, Card::Installed(_)) {
                edit.push(
                    Item::new(
                        CardAction::Import,
                        t!("library.dock.add_to_project"),
                        t!("library.dock.add_to_project.tip"),
                    )
                    .icon(glyph::IMPORT)
                    .into(),
                );
            }
            let (star, tip) = if starred {
                (t!("library.dock.unstar"), t!("library.dock.unstar.tip"))
            } else {
                (t!("library.dock.star"), t!("library.dock.star.tip"))
            };
            edit.push(
                Item::new(CardAction::Favourite, star, tip)
                    .icon(if starred {
                        glyph::STAR_FILLED
                    } else {
                        glyph::STAR
                    })
                    .into(),
            );
            let mut about = Vec::new();
            if let Card::Online(p, _) = card
                && !p.datasheet_url.is_empty()
            {
                about.push(
                    Item::new(
                        CardAction::Datasheet(p.datasheet_url.clone()),
                        t!("library.dock.datasheet"),
                        t!("library.dock.datasheet.tip"),
                    )
                    .icon(glyph::DATASHEET)
                    .into(),
                );
            }
            about.push(
                Item::new(
                    CardAction::Source,
                    t!("library.dock.show_source"),
                    t!("library.dock.show_source.tip"),
                )
                .icon(glyph::INFO)
                .into(),
            );
            Menu::new().group(place).group(edit).group(about).entries()
        }) {
            action = Some(picked);
        }
        if response.drag_started() {
            self.dock.dragged = Some(card.clone());
        }
        if response.dragged() {
            response.dnd_set_drag_payload(json!({"kind": "dock_part", "name": facts.name}));
            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                let layer = egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("library-dock-drag-ghost"),
                );
                let ghost = ui.ctx().layer_painter(layer);
                let galley = ghost.layout_no_wrap(
                    facts.name.clone(),
                    crate::fonts::Role::Body.font(),
                    ui.visuals().strong_text_color(),
                );
                let pill = Rect::from_min_size(
                    pointer + egui::vec2(12.0, 12.0),
                    galley.size() + egui::vec2(16.0, 8.0),
                );
                ghost.rect_filled(pill, 6.0, self.surface());
                ghost.rect_stroke(
                    pill,
                    6.0,
                    Stroke::new(1.0, accent),
                    egui::StrokeKind::Inside,
                );
                ghost.galley(pill.min + egui::vec2(8.0, 4.0), galley, Color32::WHITE);
            }
        }
        if action.is_none() && response.double_clicked() && self.tab == Tab::Library {
            self.library_open_part(ui.ctx(), card);
        } else if action.is_none() && response.double_clicked() {
            let tab_target = if self.tab == Tab::Pcb {
                "board"
            } else {
                "schematic"
            };
            action = Some(CardAction::Place(tab_target));
        } else if action.is_none() && response.clicked() {
            self.dock_pick(ui, card);
        }
        if let Some(action) = action {
            self.dock_act(ui.ctx(), card, action);
        }
    }

    /// A click on a card: arm the placement tool, as before.
    fn dock_pick(&mut self, ui: &egui::Ui, card: &Card) {
        if self.tab == Tab::Library {
            self.library_open_part(ui.ctx(), card);
            return;
        }
        match card {
            Card::BuiltIn(kind, _) => {
                self.change_tab(Tab::Schematic);
                self.tool = Tool::Place((*kind).into());
            }
            Card::Project(d) => {
                self.change_tab(Tab::Schematic);
                self.tool = Tool::CustomPlace(d.name.clone(), d.clone());
            }
            Card::Installed(i) => {
                let (path, name) = (i.path.clone(), i.name.clone());
                let (p, n) = (path.clone(), name.clone());
                let reply = spawn_read(ui.ctx(), move || {
                    pcbcraft_engine::library::read_tool(
                        "library.inspect_symbol",
                        &json!({"path": p, "name": n}),
                    )
                });
                self.dock.placing = Some((path, name, reply));
            }
            Card::Online(..) => {
                let (target, at) = if self.tab == Tab::Pcb {
                    ("board", self.pcb.center)
                } else {
                    ("schematic", self.schematic.center)
                };
                self.dock_place(ui.ctx(), card, target, at);
            }
        }
    }

    /// Carry out what a card's button or menu asked for.
    fn dock_act(&mut self, ctx: &egui::Context, card: &Card, action: CardAction) {
        match action {
            CardAction::Place(target) => {
                if target == "schematic" && self.tab != Tab::Schematic {
                    self.change_tab(Tab::Schematic);
                }
                let at = if target == "board" {
                    self.pcb.center
                } else {
                    self.schematic.center
                };
                self.dock_place(ctx, card, target, at);
            }
            CardAction::Import => {
                if let Card::Installed(i) = card {
                    self.dock_fetch(ctx, i, Intent::Import);
                }
            }
            CardAction::Favourite => {
                let key = card.key();
                let all = &mut self.preferences.library_favourites;
                if let Some(at) = all.iter().position(|k| *k == key) {
                    all.remove(at);
                } else {
                    all.push(key);
                }
            }
            CardAction::Datasheet(url) => {
                ctx.open_url(egui::OpenUrl::new_tab(url));
            }
            CardAction::OpenInLibrary => self.library_open_part(ctx, card),
            CardAction::Source => {
                let (source, licence) = match card {
                    Card::BuiltIn(..) => (
                        t!("library.dock.note.built_in"),
                        t!("library.dock.licence.built_in"),
                    ),
                    Card::Project(_) => (
                        t!("library.dock.note.project"),
                        t!("library.dock.licence.project"),
                    ),
                    Card::Installed(i) => {
                        let library = i
                            .path
                            .rsplit(['/', '\\'])
                            .next()
                            .unwrap_or(&i.path)
                            .trim_end_matches(".kicad_sym")
                            .to_owned();
                        (
                            library.clone(),
                            t!("library.dock.licence.installed", library = library),
                        )
                    }
                    Card::Online(p, credit) => (
                        p.source.clone(),
                        if credit.is_empty() {
                            t!("library.dock.licence.online_plain", source = p.source)
                        } else {
                            t!("library.dock.licence.online", credit = credit)
                        },
                    ),
                };
                self.message = t!(
                    "library.dock.source_line",
                    name = card.name(),
                    source = source,
                    licence = licence
                );
            }
        }
    }

    /// Place a card's part on the schematic, as its Place button does, from the Library tab.
    pub(crate) fn dock_place_card(&mut self, ctx: &egui::Context, card: &Card) {
        self.dock_act(ctx, card, CardAction::Place("schematic"));
    }

    /// Copy an installed card's symbol and footprint into the project's own library.
    pub(crate) fn dock_import_card(&mut self, ctx: &egui::Context, card: &Card) {
        self.dock_act(ctx, card, CardAction::Import);
    }

    /// The symbol read for an online part, when it has been read.
    pub(crate) fn dock_online_symbol(&self, sku: &str) -> Option<SymbolDefinition> {
        self.dock
            .online
            .get(sku)
            .and_then(Option::as_ref)
            .map(|(d, _)| d.clone())
    }

    /// The footprint of a card when it is known now: built in, the project's, one already
    /// looked up for an installed package, or one read for an online part.
    pub(crate) fn dock_card_footprint(&self, card: &Card) -> Option<FootprintDefinition> {
        match card {
            Card::BuiltIn(kind, _) => library_builtin::footprint(kind),
            Card::Project(d) => self
                .project
                .library
                .footprints
                .iter()
                .find(|f| f.name == d.footprint)
                .cloned(),
            Card::Installed(i) => self
                .dock
                .footprints
                .get(i.package.trim())
                .and_then(Clone::clone),
            Card::Online(p, _) => self
                .dock
                .online
                .get(&p.sku)
                .and_then(Option::as_ref)
                .and_then(|(_, f)| f.clone()),
        }
    }

    /// A card let go over the drawing: place it where the pointer is.
    pub(crate) fn dock_drop(&mut self, ctx: &egui::Context, payload: &Value, world: Point) -> bool {
        if payload.get("kind").and_then(Value::as_str) != Some("dock_part") {
            return false;
        }
        let Some(card) = self.dock.dragged.take() else {
            return false;
        };
        let target = if self.tab == Tab::Pcb {
            "board"
        } else {
            "schematic"
        };
        self.dock_place(ctx, &card, target, world);
        true
    }

    /// Put a part on the drawing: the symbol on the schematic, or the
    /// footprint on the board with the part added to the schematic.
    ///
    /// # Rules
    /// - One undo step per part (`part.place`).
    /// - An installed part is read on a worker first, then placed.
    fn dock_place(&mut self, ctx: &egui::Context, card: &Card, target: &str, at: Point) {
        let at = canvas::snap(at);
        match card {
            Card::Project(d) => {
                self.run(
                    "part.place",
                    json!({"target": target, "position": at, "name": d.name}),
                );
                self.dock_after_place(target);
            }
            Card::BuiltIn(kind, _) => {
                self.run(
                    "part.place",
                    json!({"target": target, "position": at, "kind": kind}),
                );
                self.dock_after_place(target);
            }
            Card::Online(part, _) => self.dock_fetch_online(ctx, part, target, at),
            Card::Installed(i) => self.dock_fetch(
                ctx,
                i,
                Intent::Place {
                    target: target.to_owned(),
                    at,
                },
            ),
        }
    }

    /// After a board placement the selection is the new footprint, so the
    /// inspector shows it.
    fn dock_after_place(&mut self, target: &str) {
        if target != "board" {
            return;
        }
        if let Some(id) = self.selected
            && let Some(f) = self
                .project
                .board
                .footprints
                .iter()
                .find(|f| f.component == id)
        {
            self.selected = Some(f.id);
        }
    }

    /// Read an installed part's symbol and footprint on a worker, then do
    /// `intent` with them.
    fn dock_fetch(&mut self, ctx: &egui::Context, part: &Installed, intent: Intent) {
        let (path, name, package) = (part.path.clone(), part.name.clone(), part.package.clone());
        let roots = self.dock.roots.clone();
        let library = path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&path)
            .trim_end_matches(".kicad_sym")
            .to_owned();
        let notice = t!(
            "library.dock.imported_notice",
            name = name,
            library = library
        );
        let reply = spawn_read(ctx, move || {
            let read = pcbcraft_engine::library::read_tool(
                "library.inspect_symbol",
                &json!({"path": path, "name": name}),
            )?;
            let symbol = read
                .get("definition")
                .cloned()
                .ok_or("the library gave no definition")?;
            let footprint = if package.trim().is_empty() {
                Value::Null
            } else {
                match roots {
                    Some(roots) => {
                        pcbcraft_engine::library::resolve_footprint_over(roots, package.trim())
                    }
                    None => pcbcraft_engine::library::resolve_footprint(package.trim()),
                }
                .ok()
                .and_then(|d| serde_json::to_value(d).ok())
                .unwrap_or(Value::Null)
            };
            Ok(json!({"symbol": symbol, "footprint": footprint, "notice": notice}))
        });
        self.dock.pending = Some(Pending {
            intent,
            name: part.name.clone(),
            rx: reply,
        });
    }

    /// Read an online part on a worker, then place it with
    /// `library.place_online` (one undo step; a board drop also adds the
    /// symbol to the schematic).
    ///
    /// # Rules
    /// - Needs the open source switched on; otherwise says so and does nothing.
    fn dock_fetch_online(
        &mut self,
        ctx: &egui::Context,
        part: &PartResult,
        target: &str,
        at: Point,
    ) {
        let Some(source) = self.part_sources.open() else {
            self.message = t!("library.dock.online_off");
            return;
        };
        let sku = part.sku.clone();
        let reply = spawn_read(ctx, move || library_online::import_open(&source, &sku));
        self.dock.pending = Some(Pending {
            intent: Intent::Online {
                target: target.to_owned(),
                at,
            },
            name: part.mpn.clone(),
            rx: reply,
        });
    }

    /// Ask a `--capture` image to place the first online card once it is listed,
    /// the way a person picking it would.
    pub fn with_capture_place(mut self, on: bool) -> Self {
        self.dock.capture_place = on.then_some(None);
        self
    }

    /// True while a requested capture placement has not been drawn yet.
    pub(crate) fn dock_capture_placing(&self) -> bool {
        match self.dock.capture_place {
            None => false,
            Some(None) => true,
            Some(Some(at)) => self.dock.pending.is_some() || self.frames < at + 30,
        }
    }

    /// True when enough online cards have been read for a capture image, or
    /// the wait has gone on long enough.
    pub(crate) fn dock_online_read(&self) -> bool {
        self.dock.online.len() >= 8 || self.frames >= 900
    }

    /// Test hook: place an online part the way a card pick does.
    #[cfg(test)]
    pub(crate) fn place_online_for_test(
        &mut self,
        ctx: &egui::Context,
        part: &PartResult,
        target: &str,
        at: Point,
    ) {
        self.dock_fetch_online(ctx, part, target, at);
    }

    /// A worker read finished: import or place the part.
    fn dock_finish(&mut self, intent: Intent, name: &str, result: Result<Value, String>) {
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                self.message = e;
                return;
            }
        };
        if let Intent::Online { target, at } = intent {
            let mut params = value;
            params["target"] = json!(if target == "board" {
                "layout"
            } else {
                "schematic"
            });
            params["position"] = json!(at);
            self.run("library.place_online", params);
            self.dock_after_place(&target);
            return;
        }
        let mut params = json!({"symbol": value["symbol"], "notice": value["notice"]});
        if !value["footprint"].is_null() {
            params["footprint"] = value["footprint"].clone();
        }
        match intent {
            Intent::Place { target, at } => {
                params["target"] = json!(target);
                params["position"] = json!(at);
                self.run("part.place", params);
                self.dock_after_place(&target);
            }
            Intent::Online { .. } => {}
            Intent::Import => {
                self.run("library.import_part", params);
                self.message = t!("library.dock.added", name = name);
            }
        }
    }
}

/// What one card shows.
struct CardFacts {
    name: String,
    what: String,
    source: String,
    meta: String,
    licence: Option<String>,
    /// Not in the project yet: shows the badge and the licence line.
    outside: bool,
    armed: bool,
    /// Key of the symbol drawing in the cache.
    symbol: Option<String>,
}

/// Side of a footprint picture in pixels; shown smaller, so edges stay clean.
const THUMB_PX: usize = 160;

/// A footprint seen from above as a small image: copper pads, drill holes,
/// outline and silk lines, in the colours given. Pure; no UI state.
///
/// # Rules
/// - Scaled to fit with a margin; a part is never drawn bigger than 120 px/mm.
/// - Pads come from the same outlines the board draws, so shapes agree.
fn footprint_image(
    def: &FootprintDefinition,
    copper: Color32,
    ink: Color32,
    hole: Color32,
) -> egui::ColorImage {
    let n = THUMB_PX;
    let mut pixels = vec![Color32::TRANSPARENT; n * n];
    let all = library::library_geometry_paths(None, Some(def), 1);
    let (center, extent) = library::library_paths_extent(&all);
    let scale = ((n as f64 - 20.0) / extent.x.max(extent.y)).min(120.0);
    let to_px = |p: Point| {
        (
            ((p.x - center.x) * scale + n as f64 / 2.0) as f32,
            ((p.y - center.y) * scale + n as f64 / 2.0) as f32,
        )
    };
    let stamp = |pixels: &mut Vec<Color32>, x: f32, y: f32, r: f32, color: Color32| {
        let (x0, x1) = ((x - r).floor().max(0.0) as usize, (x + r).ceil() as usize);
        let (y0, y1) = ((y - r).floor().max(0.0) as usize, (y + r).ceil() as usize);
        for py in y0..=y1.min(n - 1) {
            for px in x0..=x1.min(n - 1) {
                let (dx, dy) = (px as f32 + 0.5 - x, py as f32 + 0.5 - y);
                if dx * dx + dy * dy <= r * r {
                    pixels[py * n + px] = color;
                }
            }
        }
    };
    let line = |pixels: &mut Vec<Color32>, path: &[Point], color: Color32| {
        for pair in path.windows(2) {
            let (a, b) = (to_px(pair[0]), to_px(pair[1]));
            let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let steps = (length * 2.0).ceil().max(1.0) as usize;
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                stamp(
                    pixels,
                    a.0 + (b.0 - a.0) * t,
                    a.1 + (b.1 - a.1) * t,
                    1.5,
                    color,
                );
            }
        }
    };
    let fill = |pixels: &mut Vec<Color32>, path: &[Point], color: Color32| {
        let pts: Vec<(f32, f32)> = path.iter().map(|p| to_px(*p)).collect();
        if pts.len() < 3 {
            return;
        }
        for row in 0..n {
            let y = row as f32 + 0.5;
            let mut crossings: Vec<f32> = Vec::new();
            for (i, a) in pts.iter().enumerate() {
                let b = pts[(i + 1) % pts.len()];
                if (a.1 <= y && b.1 > y) || (b.1 <= y && a.1 > y) {
                    crossings.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            crossings.sort_by(f32::total_cmp);
            for pair in crossings.as_chunks::<2>().0 {
                let from = (pair[0] - 0.5).ceil().max(0.0) as usize;
                let to = (pair[1] - 0.5).floor().min(n as f32 - 1.0);
                if to >= 0.0 {
                    for px in from..=(to as usize) {
                        pixels[row * n + px] = color;
                    }
                }
            }
        }
    };
    for graphic in &def.graphics {
        line(&mut pixels, &graphic.path_points(), ink);
    }
    let pads: Vec<pcbcraft_doc::Pad> = def
        .pads
        .iter()
        .map(|p| pcbcraft_doc::Pad {
            number: p.number.clone(),
            offset: p.position,
            diameter: p.size.x.min(p.size.y),
            net: None,
            castellated: None,
            geometry: Some(p.clone()),
        })
        .collect();
    for pad in &pads {
        fill(&mut pixels, &pad.outline_points(), copper);
    }
    for pad in &pads {
        fill(&mut pixels, &pad.drill_outline_points(), hole);
    }
    egui::ColorImage::new([n, n], pixels)
}
