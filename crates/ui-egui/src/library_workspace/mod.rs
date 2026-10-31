//! The Library workspace: a top-level tab where a person reads, compares and edits parts.
//!
//! Context: issues #695 (Library tab), #698 (3D models), #699 (part data and compare). The left
//! aside is the existing Library browser (`library_dock.rs`), reused as is; picking a card, or
//! pressing its open button anywhere in the app, opens that part's page here. Open parts are
//! tabs along the top of the page and a Compare tab diffs them. The right aside is the part's
//! Properties. The Library panel stays in the Schematic and Layout asides for placing parts.
//!
//! Local rules: this module holds view state only. Every edit of a part in the project goes
//! through an engine command (`component.set_sourcing`, `component.set_supplier`, the
//! `library.*` commands), so it is one undo step. Prices come from cached supplier answers
//! only; with no supplier connection the page says so and shows the cached values with their
//! date. Nothing here reads the network.
//!
//! # Dependencies
//! - crates/ui-egui/src/workspace.rs (`Workspace`, `LibraryWorkspace`)
//! - crates/ui-egui/src/library_dock.rs (`Card`, the symbol and footprint painters)
//! - crates/ui-egui/src/sourcing_panel.rs (`sourcing_section`, `supplier_rows`)
//! - crates/library/src/sources.rs (`PartSource`, `FixtureSource` for recorded answers)
//!
//! # Do not
//! - Draw a second part browser or a second symbol and footprint editor here; link to the
//!   existing ones.
//! - Fetch anything from the network from a frame.
use super::*;
use crate::fonts;
use crate::theme;
use crate::ui_kit::{
    button::{Size, primary_button, secondary_button},
    form_row::{FormWidth, form_row},
    tab_bar::{TabItem, TabSize, tab_bar},
};
use library_dock::Card;
use pcbcraft_library::sources::{PartResult, PartSource};
use std::sync::Arc;

mod compare;
mod datasheet;
mod edit;
pub(crate) mod facts;
mod model;
mod overview;
mod vendors;

use facts::PartFacts;

/// Most parts open as tabs at once; opening another closes the oldest that is not showing.
pub(crate) const MAX_OPEN_PARTS: usize = 8;

/// The pages of one part.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    /// Previews, key parameters, lifecycle and compliance.
    #[default]
    Overview,
    /// The datasheet beside the details read from it.
    Datasheet,
    /// Cached prices and stock per supplier, and where the data comes from.
    Vendors,
    /// The 3D model and its alignment to the footprint.
    Model,
    /// The symbol and footprint editors.
    Edit,
}

/// What the centre shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Active {
    /// The open part at this index.
    Part(usize),
    /// The compare table of every open part.
    Compare,
}

/// One part open in a tab.
pub(crate) struct PartPage {
    /// The browser card it was opened from; its key identifies the tab.
    pub(crate) card: Card,
}

/// View state of the Library tab.
pub(crate) struct State {
    /// Parts open as tabs, in the order they were opened.
    pub(crate) parts: Vec<PartPage>,
    /// What the centre shows.
    pub(crate) active: Active,
    /// The page of the showing part.
    pub(crate) page: Page,
    /// Recorded supplier answers, when any are on this computer.
    quotes: Option<Arc<dyn PartSource>>,
    /// 3D page: the camera and mesh of the one-part view.
    pub(crate) model_view: board3d::View3d,
    /// 3D page: the one-part project the view draws, its revision and the footprint origin in it.
    pub(crate) model_project: pcbcraft_doc::Project,
    pub(crate) model_rev: u64,
    pub(crate) model_centre: pcbcraft_geom::Point,
    /// 3D page: what the project above was built from, so edits rebuild it and nothing else does.
    pub(crate) model_key: String,
    pub(crate) model_part: String,
    /// Compare page: show only the rows that differ.
    pub(crate) compare_only_diff: bool,
    /// Compare page: mark the rows that differ.
    pub(crate) compare_highlight: bool,
    /// Compare page: index into `compare::QUANTITIES`.
    pub(crate) compare_qty: usize,
    /// A read of a definition for the editor, running on a worker.
    pub(crate) draft_read: Option<edit::DraftRead>,
    /// True once the default place has been looked at for a recorded answer file.
    quotes_checked: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            parts: Vec::new(),
            active: Active::Part(0),
            page: Page::Overview,
            quotes: None,
            model_view: board3d::View3d::default(),
            model_project: pcbcraft_doc::Project::default(),
            model_rev: 0,
            model_centre: pcbcraft_geom::Point::new(0.0, 0.0),
            model_key: String::new(),
            model_part: String::new(),
            compare_only_diff: false,
            compare_highlight: true,
            compare_qty: 1,
            draft_read: None,
            quotes_checked: false,
        }
    }
}

impl App {
    /// Open the part behind `card` in the Library tab and show its Overview.
    ///
    /// Why: the browser's open button, a click on a card in the Library tab and the part
    /// menu all land on the same page of the same part.
    ///
    /// # Rules
    /// - A part already open is shown again, not opened twice.
    /// - At most `MAX_OPEN_PARTS` tabs; the oldest tab that is not showing closes first.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library_dock.rs (`Card`)
    pub(crate) fn library_open_part(&mut self, _ctx: &egui::Context, card: &Card) {
        let key = card.key();
        let at = match self.libws.parts.iter().position(|p| p.card.key() == key) {
            Some(i) => i,
            None => {
                if self.libws.parts.len() >= MAX_OPEN_PARTS {
                    let showing = match self.libws.active {
                        Active::Part(i) => i,
                        Active::Compare => usize::MAX,
                    };
                    let oldest = if showing == 0 { 1 } else { 0 };
                    self.libws.parts.remove(oldest);
                }
                self.libws.parts.push(PartPage { card: card.clone() });
                self.libws.parts.len() - 1
            }
        };
        self.libws.active = Active::Part(at);
        self.libws.page = Page::Overview;
        if self.tab != Tab::Library {
            self.change_tab(Tab::Library);
        }
    }

    /// Stage a Library tab scene for a `--capture` image: `library-overview`, `library-datasheet`,
    /// `library-vendors`, `library-3d`, `library-edit` or `library-compare`.
    ///
    /// # Rules
    /// - Opens the built-in resistor and LED, so the compare scene has two parts.
    /// - Frame 3 of a capture only; it changes view state, never the design.
    pub(crate) fn capture_library_scene(&mut self, scene: &str) {
        let ctx = egui::Context::default();
        self.change_tab(Tab::Library);
        let demo = scene == "library-3d" || scene == "library-edit";
        if demo {
            self.capture_demo_part();
        }
        for kind in ["resistor", "led"] {
            if let Some(t) = pcbcraft_doc::TEMPLATES.iter().find(|t| t.kind == kind) {
                self.library_open_part(&ctx, &Card::BuiltIn(t.kind, t.label));
            }
        }
        if demo && let Some(sym) = self.project.library.symbols.first().cloned() {
            self.library_open_part(&ctx, &Card::Project(Box::new(sym)));
        } else {
            self.libws.active = Active::Part(0);
        }
        if scene == "library-compare" {
            self.libws.active = Active::Compare;
            return;
        }
        self.libws.page = match scene {
            "library-datasheet" => Page::Datasheet,
            "library-vendors" => Page::Vendors,
            "library-3d" => Page::Model,
            "library-edit" => Page::Edit,
            _ => Page::Overview,
        };
    }

    /// Put a demonstration chip part in the project library for a capture: two pads, a symbol
    /// that names them and the box model kept with the test fixtures.
    ///
    /// # Rules
    /// - Capture only; the part is real project data, so a normal run never calls it.
    fn capture_demo_part(&mut self) {
        let mut fp = authoring::new_footprint();
        fp.name = "R_1206_demo".into();
        fp.pads = vec![authoring::new_pad("1", -1.5), authoring::new_pad("2", 1.5)];
        for pad in &mut fp.pads {
            pad.pad_type = "smd".into();
            pad.shape = "roundrect".into();
            pad.size = pcbcraft_geom::Point::new(1.2, 1.6);
            pad.drill = None;
            pad.layers = vec!["F.Cu".into(), "F.Paste".into(), "F.Mask".into()];
        }
        self.tool_call("library.footprint.upsert", json!({"definition": fp}));
        let model = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/e2e/fixtures/chip-body.gltf"
        );
        self.tool_call(
            "library.footprint.set_model",
            json!({"name": "R_1206_demo", "path": model}),
        );
        let mut sym = authoring::new_symbol();
        sym.name = "Chip resistor".into();
        sym.value = "330".into();
        sym.footprint = "R_1206_demo".into();
        self.tool_call("library.symbol.upsert", json!({"definition": sym}));
        self.refresh();
    }

    /// Hand the Library tab recorded supplier answers (tests, or a cache file on this computer).
    ///
    /// Contract: crates/library/src/sources.rs (`PartSource`)
    #[cfg(test)]
    pub(crate) fn use_supplier_quotes(&mut self, source: Box<dyn PartSource>) {
        self.libws.quotes = Some(Arc::from(source));
        self.libws.quotes_checked = true;
    }

    /// The cached supplier answers for a part number, read from the recorded file the first
    /// time they are needed. No supplier connection exists yet, so this is the only source.
    fn library_quotes(&mut self, mpn: &str) -> Vec<PartResult> {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.libws.quotes_checked {
            self.libws.quotes_checked = true;
            if let Some(path) = pcbcraft_library::cad_cache::default_cache_dir()
                .and_then(|d| d.parent().map(|p| p.join("supplier-quotes.json")))
                && let Ok(text) = std::fs::read_to_string(path)
                && let Ok(source) = pcbcraft_library::sources::FixtureSource::from_json(&text)
            {
                self.libws.quotes = Some(Arc::new(source));
            }
        }
        let mpn = mpn.trim();
        if mpn.is_empty() {
            return Vec::new();
        }
        self.libws
            .quotes
            .as_ref()
            .and_then(|q| q.search(mpn).ok())
            .map(|all| {
                all.into_iter()
                    .filter(|p| p.mpn.eq_ignore_ascii_case(mpn))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether a live supplier connection is set up. None exists yet: the online transports
    /// that would make one are separate work, so prices are always cached values.
    pub(crate) fn supplier_connected(&self) -> bool {
        false
    }

    /// The placed part in this project that a card stands for, if one is placed.
    fn library_component(&self, card: &Card) -> Option<&pcbcraft_doc::Component> {
        self.project.components.iter().find(|c| match card {
            Card::BuiltIn(kind, _) => c.kind == *kind && c.library_symbol.is_none(),
            Card::Project(d) => c
                .library_symbol
                .as_ref()
                .is_some_and(|s| s.definition.name == d.name),
            Card::Installed(i) => c
                .library_symbol
                .as_ref()
                .is_some_and(|s| s.definition.name == i.name),
            Card::Online(p, _) => !p.mpn.is_empty() && c.sourcing.mpn == p.mpn,
        })
    }

    /// Everything the pages show about one open part, gathered once per frame.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library_workspace/facts.rs
    pub(crate) fn library_facts(&mut self, ui: &egui::Ui, card: &Card) -> PartFacts {
        let (name, what, source, meta) = self.dock_summary(ui, card);
        let mut f = PartFacts {
            name,
            origin: source,
            description: what,
            ..Default::default()
        };
        match card {
            Card::BuiltIn(kind, _) => {
                f.package = library_builtin::footprint(kind).map(|d| d.name);
                f.pins = pcbcraft_doc::component_template(
                    kind,
                    0,
                    String::new(),
                    pcbcraft_geom::Point::new(0.0, 0.0),
                )
                .map(|p| p.pins.len());
            }
            Card::Project(d) => {
                if !d.footprint.trim().is_empty() {
                    f.package = Some(d.footprint.trim().to_owned());
                }
                f.pins = Some(d.pins.len());
            }
            Card::Installed(i) => {
                if !i.package.trim().is_empty() {
                    f.package = Some(i.package.clone());
                }
            }
            Card::Online(p, _) => {
                f.manufacturer = p.manufacturer.clone();
                f.mpn = p.mpn.clone();
                if !p.datasheet_url.is_empty() {
                    f.datasheet = Some(p.datasheet_url.clone());
                }
                f.listing = Some(pcbcraft_library::listing::listing_from_result(
                    p,
                    &std::collections::BTreeMap::new(),
                ));
                f.quotes.push((**p).clone());
                let _ = &meta;
            }
        }
        if let Some(c) = self.library_component(card) {
            f.component = Some(c.id);
            f.manufacturer = c.sourcing.manufacturer.clone();
            f.mpn = c.sourcing.mpn.clone();
            if !c.sourcing.description.is_empty() {
                f.description = c.sourcing.description.clone();
            }
            f.suppliers = c.sourcing.suppliers.clone();
            f.lifecycle = c.sourcing.lifecycle;
            f.rohs = c.sourcing.rohs;
            f.reach = c.sourcing.reach;
            f.moisture = c.sourcing.moisture_level;
            if let Some(url) = c.field("Datasheet") {
                f.datasheet = Some(url.to_owned());
            }
        }
        let mpn = f.mpn.clone();
        for q in self.library_quotes(&mpn) {
            if !f
                .quotes
                .iter()
                .any(|e| e.source == q.source && e.sku == q.sku)
            {
                f.quotes.push(q);
            }
        }
        f
    }

    /// The tool bar of the Library tab: New part, Edit symbol, Edit footprint, Import, Refresh prices.
    ///
    /// # Rules
    /// - Every button has a tip; one that cannot act says why in its tip.
    /// - Refresh prices with no supplier connection changes nothing and says so in the status line.
    pub(crate) fn library_toolbar(&mut self, ui: &mut egui::Ui) {
        let showing = self.library_showing();
        ui.horizontal_centered(|ui| {
            ui.add_space(Self::PAGE_GUTTER);
            ui.spacing_mut().item_spacing.x = 8.0;
            if secondary_button(
                ui,
                None,
                &t!("libws.new_part"),
                &t!("libws.new_part.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                self.library_new_part();
            }
            let has_part = showing.is_some();
            if secondary_button(
                ui,
                None,
                &t!("libws.edit_symbol"),
                &t!("libws.edit_symbol.tip"),
                Size::Normal,
                has_part,
            )
            .clicked()
            {
                self.libws.page = Page::Edit;
                self.library_load_symbol_draft(ui.ctx());
            }
            if secondary_button(
                ui,
                None,
                &t!("libws.edit_footprint"),
                &t!("libws.edit_footprint.tip"),
                Size::Normal,
                has_part,
            )
            .clicked()
            {
                self.libws.page = Page::Edit;
                self.library_load_footprint_draft(ui.ctx());
            }
            let installed = matches!(showing, Some(Card::Installed(_)));
            if secondary_button(
                ui,
                None,
                &t!("libws.import"),
                &t!("libws.import.tip"),
                Size::Normal,
                installed,
            )
            .clicked()
                && let Some(card) = showing.clone()
            {
                self.dock_import_card(ui.ctx(), &card);
            }
            if secondary_button(
                ui,
                None,
                &t!("libws.refresh_prices"),
                &t!("libws.refresh_prices.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                self.message = if self.supplier_connected() {
                    t!("libws.refresh_started")
                } else {
                    t!("libws.refresh_no_connection")
                };
            }
        });
    }

    /// Start a new symbol in the editor on the Edit page.
    pub(crate) fn library_new_part(&mut self) {
        self.library.editor.draft = Some(authoring::Draft::Symbol(authoring::new_symbol()));
        self.library.editor.notice = None;
        self.library.editor.needs_fit = true;
        self.libws.page = Page::Edit;
        if self.libws.parts.is_empty() {
            self.libws.active = Active::Part(0);
        }
    }

    /// The card of the part the centre is showing, if one is.
    pub(crate) fn library_showing(&self) -> Option<Card> {
        match self.libws.active {
            Active::Part(i) => self.libws.parts.get(i).map(|p| p.card.clone()),
            Active::Compare => None,
        }
    }

    /// The centre of the Library tab: the open-part tabs, the part header, the page tabs and
    /// the page; or the empty state, or the compare table.
    pub(crate) fn library_center(&mut self, ui: &mut egui::Ui) {
        self.library_poll_draft();
        if self.libws.parts.is_empty() && self.libws.page != Page::Edit {
            ui.add_space(96.0);
            if crate::ui_kit::empty_state::empty_state(
                ui,
                crate::ui_kit::icons::SEARCH,
                &t!("libws.empty.title"),
                &t!("libws.empty.message"),
                &t!("libws.empty.button"),
                &t!("libws.empty.button.tip"),
            )
            .clicked()
            {
                self.library_new_part();
            }
            return;
        }
        self.library_part_tabs(ui);
        let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(24, 16));
        egui::ScrollArea::vertical()
            .id_salt("library-page-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE[3];
                    match self.libws.active {
                        Active::Compare => self.library_compare_page(ui),
                        Active::Part(_) => self.library_part_page(ui),
                    }
                });
            });
    }

    /// The strip of open parts: one tab each, plus Compare when two or more are open, and a
    /// button that closes the showing tab.
    fn library_part_tabs(&mut self, ui: &mut egui::Ui) {
        let labels: Vec<(String, String)> = self
            .libws
            .parts
            .iter()
            .map(|p| {
                (
                    p.card.name(),
                    t!("libws.part_tab.tip", name = p.card.name()),
                )
            })
            .collect();
        let compare = (t!("libws.compare_tab"), t!("libws.compare_tab.tip"));
        let mut items: Vec<TabItem<'_>> =
            labels.iter().map(|(l, tip)| TabItem::new(l, tip)).collect();
        let compare_at = items.len();
        if labels.len() >= 2 {
            items.push(TabItem::new(&compare.0, &compare.1));
        }
        let mut current = match self.libws.active {
            Active::Part(i) => i,
            Active::Compare => compare_at,
        };
        let was = current;
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let width = (ui.available_width() - 44.0).max(120.0);
            ui.allocate_ui(egui::vec2(width, TabSize::Aside.height()), |ui| {
                let r = tab_bar(
                    ui,
                    "library-part-tabs",
                    &mut current,
                    &items,
                    TabSize::Aside,
                );
                let _ = r;
            });
            if crate::ui_kit::button::icon_button(
                ui,
                crate::ui_kit::icons::CLOSE,
                &t!("libws.close_part"),
                &t!("libws.close_part.tip"),
                Size::Normal,
                !self.libws.parts.is_empty(),
            )
            .clicked()
            {
                self.library_close_showing();
            }
        });
        if current != was {
            self.libws.active = if current == compare_at && labels.len() >= 2 {
                Active::Compare
            } else {
                Active::Part(current)
            };
            self.libws.page = Page::Overview;
        }
        let tokens = theme::tokens(ui.ctx());
        let y = ui.min_rect().bottom();
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            egui::Stroke::new(theme::BORDER, tokens.line),
        );
    }

    /// Close the tab that is showing and show its neighbour.
    fn library_close_showing(&mut self) {
        match self.libws.active {
            Active::Part(i) if i < self.libws.parts.len() => {
                self.libws.parts.remove(i);
                self.libws.active = Active::Part(i.saturating_sub(1));
            }
            Active::Compare => self.libws.active = Active::Part(0),
            _ => {}
        }
        if self.libws.parts.len() < 2 && self.libws.active == Active::Compare {
            self.libws.active = Active::Part(0);
        }
        self.libws.page = Page::Overview;
    }

    /// The header of the showing part, its page tabs and the page.
    fn library_part_page(&mut self, ui: &mut egui::Ui) {
        let Some(card) = self.library_showing() else {
            // A new part with nothing open: only the editor makes sense.
            self.library_edit_page(ui);
            return;
        };
        let facts = self.library_facts(ui, &card);
        let t = theme::tokens(ui.ctx());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(fonts::section_caption(&facts.origin).color(t.muted));
                ui.label(fonts::display(&facts.name).color(t.text));
                let sub = match (facts.manufacturer.is_empty(), facts.mpn.is_empty()) {
                    (false, false) => format!("{} \u{b7} {}", facts.manufacturer, facts.mpn),
                    (true, false) => facts.mpn.clone(),
                    (false, true) => facts.manufacturer.clone(),
                    (true, true) => t!("libws.no_maker"),
                };
                ui.label(fonts::text(fonts::Role::Body, sub).color(t.muted));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_button(
                    ui,
                    None,
                    &t!("libws.compare"),
                    &t!("libws.compare.tip"),
                    Size::Large,
                    self.libws.parts.len() >= 2,
                )
                .clicked()
                {
                    self.libws.active = Active::Compare;
                }
                if matches!(card, Card::Installed(_))
                    && secondary_button(
                        ui,
                        None,
                        &t!("library.dock.add_to_project"),
                        &t!("library.dock.add_to_project.tip"),
                        Size::Large,
                        true,
                    )
                    .clicked()
                {
                    self.dock_import_card(ui.ctx(), &card);
                }
                if primary_button(
                    ui,
                    None,
                    &t!("libws.place"),
                    &t!("libws.place.tip"),
                    Size::Large,
                    true,
                )
                .clicked()
                {
                    self.dock_place_card(ui.ctx(), &card);
                }
            });
        });
        let pages = [
            (
                Page::Overview,
                t!("libws.page.overview"),
                t!("libws.page.overview.tip"),
            ),
            (
                Page::Datasheet,
                t!("libws.page.datasheet"),
                t!("libws.page.datasheet.tip"),
            ),
            (
                Page::Vendors,
                t!("libws.page.vendors"),
                t!("libws.page.vendors.tip"),
            ),
            (
                Page::Model,
                t!("libws.page.model"),
                t!("libws.page.model.tip"),
            ),
            (Page::Edit, t!("libws.page.edit"), t!("libws.page.edit.tip")),
        ];
        let items: Vec<TabItem<'_>> = pages
            .iter()
            .map(|(_, l, tip)| TabItem::new(l, tip))
            .collect();
        let mut current = pages
            .iter()
            .position(|(p, ..)| *p == self.libws.page)
            .unwrap_or(0);
        let was = current;
        tab_bar(
            ui,
            "library-page-tabs",
            &mut current,
            &items,
            TabSize::Aside,
        );
        if current != was {
            self.libws.page = pages[current].0;
        }
        match self.libws.page {
            Page::Overview => self.library_overview_page(ui, &card, &facts),
            Page::Vendors => self.library_vendors_page(ui, &card, &facts),
            Page::Edit => self.library_edit_page(ui),
            Page::Model => self.library_model_page(ui, &card),
            Page::Datasheet => self.library_datasheet_page(ui, &facts),
        }
    }

    /// The Edit page: the existing symbol and footprint editor, in this tab.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/authoring.rs (`library_editor`, the one editor of definitions)
    ///
    /// # Do not
    /// - Copy the editor; it is called, so a fix in one place fixes both.
    fn library_edit_page(&mut self, ui: &mut egui::Ui) {
        if self.library.editor.draft.is_none() {
            self.library_load_symbol_draft(ui.ctx());
        }
        self.library_editor_page(ui);
    }

    /// The right aside of the Library tab: the showing part's properties.
    ///
    /// # Rules
    /// - A part placed in this project edits through the Sourcing section, one undo step per edit.
    /// - Any other part is read-only here and says how to make it editable.
    pub(crate) fn library_properties(&mut self, ui: &mut egui::Ui) {
        let Some(card) = self.library_showing() else {
            return;
        };
        let facts = self.library_facts(ui, &card);
        let t = theme::tokens(ui.ctx());
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.label(fonts::title(&facts.name).color(t.text));
                ui.label(fonts::caption(&facts.origin).color(t.muted));
                ui.add_space(8.0);
                match facts
                    .component
                    .and_then(|id| self.project.component(id).cloned())
                {
                    Some(component) => self.sourcing_section(ui, &component),
                    None => {
                        ui.label(
                            fonts::text(fonts::Role::Body, t!("libws.props.read_only"))
                                .color(t.muted),
                        );
                        ui.add_space(8.0);
                        for (label, value) in [
                            (
                                t!("props.sourcing.manufacturer"),
                                facts.manufacturer.clone(),
                            ),
                            (t!("props.sourcing.mpn"), facts.mpn.clone()),
                            (t!("props.sourcing.description"), facts.description.clone()),
                        ] {
                            form_row(ui, FormWidth::Aside, &label, None, None, |ui| {
                                let shown = if value.is_empty() {
                                    t!("libws.not_listed")
                                } else {
                                    value.clone()
                                };
                                ui.label(fonts::text(fonts::Role::Body, shown).color(t.muted));
                            });
                        }
                        ui.add_space(8.0);
                        if matches!(card, Card::Installed(_))
                            && secondary_button(
                                ui,
                                None,
                                &t!("library.dock.add_to_project"),
                                &t!("library.dock.add_to_project.tip"),
                                Size::Normal,
                                true,
                            )
                            .clicked()
                        {
                            self.dock_import_card(ui.ctx(), &card);
                        }
                    }
                }
            });
    }
}
