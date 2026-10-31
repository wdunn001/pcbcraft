//! Import, Save as and Export: one dialog whose format lists come from the registry.
//!
//! Context: issue #194 and `crates/io/src/formats.rs`. The File menu has no per-format
//! items. Open reads a whole project (the start screen); Import, Save as and Export open
//! the dialog here with the formats of their direction and scope. Every entry (a
//! Recent one, a Common one, More..., a part's right-click menu, the Manufacture
//! outputs list) opens the SAME dialog, preconfigured: the format chosen and, for a
//! Recent entry, its saved options filled in.
//! Local rules: format labels, tips, extensions and options are read from the
//! registry, never written here. A bill of materials is one format with a flavour
//! option (`bom_export.rs`). Option values live in a string map keyed by the option's
//! registry label, which is also what a Recent entry stores.
//!
//! # Dependencies
//! - crates/io/src/formats.rs (`FORMATS`, `grouped`, `listed`, `for_path`)
//! - crates/ui-egui/src/bom_export.rs (rows, text, summary)
//! - crates/ui-egui/src/lib.rs (`App::tool_call`, `run`, `make_directory`)
//! - crates/ui-egui/src/editing.rs (`EditState::dialog`)
//!
//! # Do not
//! - Add a File menu item for one format; mark the registry row `common` instead.
//! - Invent packages; `packages()` is the hook a later change fills.
use crate::App;
use crate::bom_export;
use pcbcraft_doc::{BOM_EXCLUDE_FIELD, Id, Project};
use pcbcraft_io::formats::{self, Direction, FormatSpec, OptionKind, OptionSpec, Run, Scope, bom_flavour};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Option values keyed by the registry label.
pub(crate) type Values = BTreeMap<String, String>;
/// A menu pick: the format id and the options to fill in, either may be absent.
type Pick = (Option<String>, Option<Values>);

/// Most entries a Recent list keeps.
pub(crate) const RECENT_MAX: usize = 5;
/// Rows the preview shows inside the dialog.
const PREVIEW_ROWS: usize = 8;

/// Which dialog is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Export,
    Import,
    SaveAs,
}

impl Mode {
    fn title(self) -> &'static str {
        match self {
            Self::Export => "Export",
            Self::Import => "Import",
            Self::SaveAs => "Save as",
        }
    }
    fn direction(self) -> Direction {
        match self {
            Self::Import => Direction::Import,
            Self::Export | Self::SaveAs => Direction::Export,
        }
    }
    fn scope(self) -> Scope {
        match self {
            Self::Import => Scope::Parts,
            Self::Export => Scope::Output,
            Self::SaveAs => Scope::Project,
        }
    }
}

/// One remembered export or import: the format and the options it was used with.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Recent {
    pub format: String,
    pub values: BTreeMap<String, String>,
    /// What the menu shows, such as "Bill of materials (CSV, grouped)".
    pub summary: String,
}

/// The Recent lists, newest first.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Recents {
    pub exports: Vec<Recent>,
    pub imports: Vec<Recent>,
}

/// A named bundle of several formats, such as the files one fab house asks for.
pub(crate) struct Package {
    pub label: &'static str,
}

/// The packages the Export dialog lists.
///
/// Why: the hook for named multi-format bundles. A research task is defining which ones
/// ship; until then there are none and the dialog says so.
pub(crate) fn packages() -> &'static [Package] {
    &[]
}

/// What the open dialog remembers between frames.
#[derive(Default)]
pub(crate) struct DialogState {
    pub open: bool,
    pub mode: Mode,
    /// Id of the chosen format, empty for none.
    pub format: String,
    pub values: BTreeMap<String, String>,
    /// File to read or write for Import and Save as.
    pub path: String,
    /// Component ids a bill lists; empty lists every part.
    pub only: Vec<Id>,
    /// The read-only Manufacture preview window.
    pub preview_open: bool,
    /// (revision, rows) of the plain bill, for the Manufacture row count.
    pub count: Option<(u64, usize)>,
    /// The scripted clicks of a `--show` capture.
    pub script: CaptureScript,
}

/// What a `--show` capture does before it takes the picture: opens a dialog, or clicks
/// through File and Export to show the submenu.
#[derive(Default)]
pub(crate) struct CaptureScript {
/// The scene asked for: `file-export`, `export-dialog` or `export-flavour`.
    pub show: Option<String>,
    /// Where the menu buttons were drawn last frame.
    pub rects: BTreeMap<&'static str, egui::Pos2>,
    release: Option<egui::Pos2>,
    step: u8,
    wait: u32,
}

impl CaptureScript {
    /// True once the scene is on screen and has settled.
    pub(crate) fn ready(&self) -> bool {
        self.show.as_deref() != Some("file-export") || (self.step >= 2 && self.wait >= 10)
    }
}

fn spec(id: &str) -> Option<&'static FormatSpec> {
    formats::FORMATS.iter().find(|f| f.id == id)
}

/// The option values a format starts with.
fn defaults(project: &Project, f: &FormatSpec) -> BTreeMap<String, String> {
    if f.id == "bom" {
return bom_export::default_values(project, &formats::BOM_FLAVOURS[0]);
    }
    let mut out = BTreeMap::new();
    for o in f.options {
        let value = match o.kind {
            OptionKind::Bool { default } => default.to_string(),
            OptionKind::Choice { choices, default } => choices
                .get(default)
                .map(|(v, _)| (*v).to_owned())
                .unwrap_or_default(),
            _ => String::new(),
        };
        out.insert(o.label.to_owned(), value);
    }
    let mut out = BTreeMap::new();
    for o in f.options {
        let value = match o.kind {
            OptionKind::Bool { default } => default.to_string(),
            OptionKind::Choice { choices, default } => choices
                .get(default)
                .map(|(v, _)| (*v).to_owned())
                .unwrap_or_default(),
            _ => String::new(),
        };
        out.insert(o.label.to_owned(), value);
    }
    out
}

/// Words for a Recent entry.
fn summary(project: &Project, f: &FormatSpec, values: &BTreeMap<String, String>) -> String {
    if f.id == "bom" {
        return bom_export::summary(project, values);
    }
    let fresh = defaults(project, f);
    let mut notes: Vec<String> = vec![];
    for o in f.options {
        if let OptionKind::Choice { choices, .. } = o.kind
            && values.get(o.label) != fresh.get(o.label)
            && let Some((_, label)) = choices
                .iter()
                .find(|(v, _)| Some(*v) == values.get(o.label).map(String::as_str))
        {
            notes.push((*label).to_owned());
        }
    }
    if notes.is_empty() {
        f.label.to_owned()
    } else {
        format!("{} ({})", f.label, notes.join(", "))
    }
}

/// Put `entry` first in `list`, dropping an older identical one and anything past the limit.
fn remember(list: &mut Vec<Recent>, entry: Recent) {
    list.retain(|r| r.format != entry.format || r.values != entry.values);
    list.insert(0, entry);
    list.truncate(RECENT_MAX);
}

impl App {
    /// Open the dialog for `mode`.
    ///
    /// Why: every way into Import, Save as and Export (menu entries, the part menu, the
    /// Manufacture list) lands here, so they all show the same dialog.
    ///
    /// # Rules
    /// - `format` of `None` opens with nothing chosen (Save as picks the project).
    /// - `values` of `None` starts from the format's defaults.
    ///
    /// # Dependencies
    /// - crates/io/src/formats.rs `FORMATS`.
    ///
    /// # Do not
    /// - Run anything; the dialog's button does.
    pub(crate) fn open_dialog(
        &mut self,
        mode: Mode,
        format: Option<&str>,
        values: Option<BTreeMap<String, String>>,
        only: Vec<Id>,
    ) {
        let chosen = match (format, mode) {
            (Some(id), _) => spec(id),
            (None, Mode::SaveAs) => formats::listed(Direction::Export, Scope::Project)
                .first()
                .copied(),
            _ => None,
        };
        let path = match (mode, chosen) {
            (Mode::SaveAs, Some(f)) => self.default_save_path(f),
            _ => String::new(),
        };
        let fresh = chosen.map(|f| defaults(&self.project, f));
        let d = &mut self.edit.dialog;
        d.open = true;
        d.mode = mode;
        d.only = only;
        d.preview_open = false;
        d.format = chosen.map(|f| f.id.to_owned()).unwrap_or_default();
        d.values = values.or(fresh).unwrap_or_default();
        d.path = path;
    }

    /// The path Save as starts with for a project format.
    fn default_save_path(&self, f: &FormatSpec) -> String {
        let ext = f.extensions.first().copied().unwrap_or("pcbcraft");
        let base = std::path::Path::new(&self.path);
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("design");
        let stem = stem.strip_suffix(".pcbcraft").unwrap_or(stem);
        base.with_file_name(format!("{stem}.{ext}"))
            .to_string_lossy()
            .into_owned()
    }

    /// Component ids of the parts selected on the canvas or in the Layers tree.
    pub(crate) fn selected_part_components(&self) -> Vec<Id> {
        let mut out: Vec<Id> = vec![];
        for id in self.selected.iter().chain(self.edit.extra.iter()) {
            let component = self.project.footprint(*id).map_or(*id, |f| f.component);
            if self.project.component(component).is_some() && !out.contains(&component) {
                out.push(component);
            }
        }
        out
    }

    /// True when the part (a footprint or a component id) is left out of the bill.
    pub(crate) fn part_excluded(&self, id: Id) -> bool {
        let component = self.project.footprint(id).map_or(id, |f| f.component);
        self.project
            .component(component)
            .is_some_and(|c| !c.in_bom())
    }

    /// Leave parts out of the bill, or put them back.
    ///
    /// Why: the per-part flag is a component property (`BOM_EXCLUDE_FIELD`) set through
    /// `component.field`, so it saves with the design, undoes, and the engine's
    /// `bom.export` and the output jobs both honour it.
    ///
    /// # Do not
    /// - Clear a flag a part does not carry; the engine refuses that.
    pub(crate) fn set_bom_exclusion(&mut self, components: &[Id], exclude: bool) {
        for id in components {
            let Some(c) = self.project.component(*id) else {
                continue;
            };
            if exclude {
                self.run(
                    "component.field",
                    json!({"id": id, "key": BOM_EXCLUDE_FIELD, "value": "yes"}),
                );
            } else if c.field(BOM_EXCLUDE_FIELD).is_some() {
                self.run(
                    "component.field",
                    json!({"id": id, "key": BOM_EXCLUDE_FIELD, "value": Value::Null}),
                );
            }
        }
    }

    /// The Export submenu of the File menu: Recent, Common, More...
    pub(crate) fn export_submenu(&mut self, ui: &mut egui::Ui) {
        self.entries_submenu(ui, Mode::Export);
    }

    /// The Import submenu of the File menu: Recent, Common, More...
    pub(crate) fn import_submenu(&mut self, ui: &mut egui::Ui) {
        self.entries_submenu(ui, Mode::Import);
    }

    fn entries_submenu(&mut self, ui: &mut egui::Ui, mode: Mode) {
        let (label, tip) = match mode {
            Mode::Import => (
                "Import",
                "Bring a drawing, a part or a machine profile into this design.",
            ),
            _ => (
                "Export",
                "Make something from this design, such as a bill of materials or the files a board maker needs.",
            ),
        };
        let recent = match mode {
            Mode::Import => self.preferences.recent_files.imports.clone(),
            _ => self.preferences.recent_files.exports.clone(),
        };
        let mut picked: Option<Pick> = None;
        let button = ui.menu_button(label, |ui| {
            if !recent.is_empty() {
                ui.label(crate::fonts::caption("Recent").color(self.muted()));
                for r in &recent {
                    if ui
                        .button(&r.summary)
                        .on_hover_text("Open the dialog with this format and the options you used.")
                        .clicked()
                    {
                        picked = Some((Some(r.format.clone()), Some(r.values.clone())));
                        ui.close();
                    }
                }
                ui.separator();
            }
            ui.label(crate::fonts::caption("Common").color(self.muted()));
            for f in formats::listed(mode.direction(), mode.scope())
                .into_iter()
                .filter(|f| f.common)
            {
                if recent.iter().any(|r| r.format == f.id) {
                    continue;
                }
                if ui.button(f.label).on_hover_text(f.tip).clicked() {
                    picked = Some((Some(f.id.to_owned()), None));
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .button("More…")
                .on_hover_text("Open the dialog with every format listed and none chosen.")
                .clicked()
            {
                picked = Some((None, None));
                ui.close();
            }
        });
        if self.capture.is_some() {
            self.edit
                .dialog
                .script
                .rects
                .insert(label, button.response.rect.center());
        }
        button.response.on_hover_text(tip);
        if let Some((format, values)) = picked {
            self.open_dialog(mode, format.as_deref(), values, vec![]);
        }
    }

    /// The dialog window and, when asked, the Manufacture preview window.
    pub(crate) fn file_dialog_window(&mut self, ctx: &egui::Context) {
        self.bom_preview_window(ctx);
        if !self.edit.dialog.open {
            return;
        }
        let mut d = std::mem::take(&mut self.edit.dialog);
        let mut open = true;
        let mut go = false;
        let mut close = false;
        let muted = self.muted();
        let project = self.project.clone();
        let make_dir = self.make_directory();
        let browser = self.browser;
        egui::Window::new(d.mode.title())
            .id(egui::Id::new("file-dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(780.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(220.0);
                        egui::ScrollArea::vertical()
                            .id_salt("file-dialog-formats")
                            .max_height((ctx.content_rect().height() - 170.0).max(240.0))
                            .show(ui, |ui| format_list(ui, &mut d, &project, muted));
                    });
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_width(520.0);
                        match spec(&d.format) {
                            None => {
                                ui.label(
                                    egui::RichText::new(match d.mode {
                                        Mode::Import => "Pick what you want to bring in.",
                                        _ => "Pick a format on the left.",
                                    })
                                    .color(muted),
                                );
                            }
                            Some(f) => {
                                detail(ui, &mut d, f, &project, &make_dir, muted, browser, &mut go)
                            }
                        }
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("Close")
                        .on_hover_text("Close without writing anything.")
                        .clicked()
                    {
                        close = true;
                    }
                });
                ui.add_space(6.0);
            });
        if go {
            let ok = self.run_dialog(ctx, &d);
            if ok {
                close = true;
            }
        }
        if !open || close {
            d.open = false;
        }
        let preview = self.edit.dialog.preview_open;
        self.edit.dialog = d;
        self.edit.dialog.preview_open |= preview;
    }

    /// Do what the open dialog says; true when it worked.
    fn run_dialog(&mut self, ctx: &egui::Context, d: &DialogState) -> bool {
        let Some(f) = spec(&d.format) else {
            return false;
        };
        let ok = match d.mode {
            Mode::SaveAs => self.save_as(f, &d.path),
            Mode::Import => self.import_file(f, d),
            Mode::Export => self.export_output(ctx, f, d),
        };
        if ok {
            let entry = Recent {
                format: f.id.to_owned(),
                values: d.values.clone(),
                summary: summary(&self.project, f, &d.values),
            };
            match d.mode {
                Mode::Export => remember(&mut self.preferences.recent_files.exports, entry),
                Mode::Import => remember(&mut self.preferences.recent_files.imports, entry),
                Mode::SaveAs => {}
            }
        }
        ok
    }

    fn save_as(&mut self, f: &FormatSpec, path: &str) -> bool {
        let path = path.trim();
        if path.is_empty() {
            self.message = "Type where to save.".into();
            return false;
        }
        match f.export {
            Some(Run::Tool("save")) => {
                if self.tool_call("save", json!({"path": path})) {
                    self.path = path.to_owned();
                    return true;
                }
                false
            }
            Some(Run::Engine(command)) => self.tool_call(command, json!({"path": path})),
            _ => false,
        }
    }

    fn import_file(&mut self, f: &FormatSpec, d: &DialogState) -> bool {
        let path = d.path.trim();
        if path.is_empty() {
            self.message = "Type the file to bring in.".into();
            return false;
        }
        let Some(Run::Engine(command)) = f.import else {
            return false;
        };
        let mut params = json!({"path": path});
        for o in f.options {
            if let (Some(param), OptionKind::Bool { default }) = (o.param, o.kind) {
                let on = d.values.get(o.label).map_or(default, |v| v == "true");
                params[param] = json!(on);
            }
        }
        self.tool_call(command, params)
    }

    fn export_output(&mut self, ctx: &egui::Context, f: &FormatSpec, d: &DialogState) -> bool {
        let directory = self.make_directory();
        if f.id == "bom" {
            return self.write_bom(ctx, d, &directory);
        }
        if std::fs::create_dir_all(&directory).is_err() {
            self.message = "That folder could not be made.".into();
            return false;
        }
        match f.export {
Some(Run::Engine("gerber.files")) => {
                let mut params = gerber_params(f, d);
                params["directory"] = json!(&directory);
                let ok = self.tool_call("gerber.files", params);
                if ok {
                    self.message = format!("{} saved in {directory}.", f.label);
                }
                ok
            }
            Some(Run::Kind(kind)) => {
                let job = format!("export-{kind}");
                let _ = self.tool_call("output_job.add", json!({"name": &job, "kinds": [kind]}));
                let ok = self.tool_call(
                    "output_job.run",
                    json!({"name": &job, "directory": &directory}),
                );
                if ok {
                    self.message = format!("{} saved in {directory}.", f.label);
                }
                ok
            }
            Some(Run::Kind(kind)) => {
                let job = format!("export-{kind}");
                let _ = self.tool_call("output_job.add", json!({"name": &job, "kinds": [kind]}));
                let ok = self.tool_call(
                    "output_job.run",
                    json!({"name": &job, "directory": &directory}),
                );
                if ok {
                    self.message = format!("{} saved in {directory}.", f.label);
                }
                ok
            }
            Some(Run::Tool("export")) => {
                let stem = std::path::Path::new(&self.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("design")
                    .to_owned();
                let ext = f.extensions.first().copied().unwrap_or("png");
                let path = std::path::Path::new(&directory)
                    .join(format!("{stem}.{ext}"))
                    .to_string_lossy()
                    .into_owned();
                let format = if f.id == "picture" {
                    "png"
                } else {
                    "kicad_pcb"
                };
                let mut params = json!({"path": path, "format": format});
                if f.id == "picture" {
                    params["view"] = json!(d.values.get("View").map_or("pcb", String::as_str));
                }
                let ok = self.tool_call("export", params);
                if ok {
                    self.message = format!("{} saved in {directory}.", f.label);
                }
                ok
            }
            _ => false,
        }
    }

    /// Write the bill of materials into the make folder, or copy it in the browser build.
    fn write_bom(&mut self, ctx: &egui::Context, d: &DialogState, directory: &str) -> bool {
        let o = bom_export::Options::from_values(&self.project, &d.values, &d.only);
        let table = match bom_export::table(&self.project, &o) {
            Ok(t) => t,
            Err(e) => {
                self.message = e;
                return false;
            }
        };
        let text = bom_export::render(&table, &o.file_type);
        if self.browser {
            ctx.copy_text(text);
            self.message =
                "Bill of materials copied. The browser build cannot write files yet.".into();
            return true;
        }
        if std::fs::create_dir_all(directory).is_err() {
            self.message = "That folder could not be made.".into();
            return false;
        }
        let path = std::path::Path::new(directory).join(bom_export::file_name(&o));
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.message = format!(
                    "Bill of materials saved as {} ({} rows).",
                    path.display(),
                    table.rows.len()
                );
                true
            }
            Err(e) => {
                self.message = format!("Could not write the bill of materials: {e}");
                false
            }
        }
    }

    /// The row count of the plain bill, cached per revision.
    pub(crate) fn bom_row_count(&mut self) -> usize {
        if let Some((revision, n)) = self.edit.dialog.count
            && revision == self.revision
        {
            return n;
        }
        let o = bom_export::Options::from_values(
            &self.project,
&bom_export::default_values(&self.project, &formats::BOM_FLAVOURS[0]),
            &bom_export::default_values(&self.project, formats::default_bom_flavour()),
            &[],
            &[],
        );
        let n = bom_export::table(&self.project, &o).map_or(0, |t| t.rows.len());
        self.edit.dialog.count = Some((self.revision, n));
        n
    }

    /// The Bill of materials row of the Manufacture outputs list.
    ///
    /// # Rules
    /// - Preview opens the read-only window; Export... opens the Export dialog on the
    ///   bill. Output jobs that list `bom` are named so the person sees it runs there too.
    pub(crate) fn bom_outputs_ui(&mut self, ui: &mut egui::Ui) {
        let rows = self.bom_row_count();
        let jobs: Vec<String> = self
            .project
            .output_jobs
            .iter()
            .filter(|j| j.kinds.iter().any(|k| k == "bom"))
            .map(|j| j.name.clone())
            .collect();
        let muted = self.muted();
        ui.add_space(8.0);
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 0)).show(ui, |ui| {
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Bill of materials").strong())
                        .on_hover_text(formats::FORMATS.iter().find(|f| f.id == "bom").map_or("", |f| f.tip));
                    ui.label(egui::RichText::new(format!("{rows} rows")).color(muted));
                });
                ui.label(
                    crate::fonts::caption(if jobs.is_empty() {
                        "Not in an output job yet.".to_owned()
                    } else {
                        format!("Also written by the output jobs: {}.", jobs.join(", "))
                    })
                    .color(muted),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("Preview")
                        .on_hover_text("Look at every row of the bill of materials before you write it.")
                        .clicked()
                    {
                        self.edit.dialog.preview_open = true;
                    }
                    if ui
                        .button("Export…")
                        .on_hover_text("Choose the file type, columns and build, then write the bill of materials.")
                        .clicked()
                    {
                        self.open_dialog(Mode::Export, Some("bom"), None, vec![]);
                    }
                });
            });
        });
    }

    fn bom_preview_window(&mut self, ctx: &egui::Context) {
        if !self.edit.dialog.preview_open {
            return;
        }
let values = bom_export::default_values(&self.project, formats::default_bom_flavour());
        let o = bom_export::Options::from_values(&self.project, &values, &[]);
        let table = bom_export::table(&self.project, &o).unwrap_or(bom_export::Table {
            headers: vec![],
            rows: vec![],
            header: true,
            missing: vec![],
        });
        let mut open = true;
        let mut export = false;
        let o = bom_export::Options::from_values(&self.project, &values, &[]);
        let table = bom_export::table(&self.project, &o).unwrap_or(bom_export::Table {
            headers: vec![],
            rows: vec![],
let values = bom_export::default_values(&self.project, &formats::BOM_FLAVOURS[0]);
        let o = bom_export::Options::from_values(&self.project, &values, &[]);
        let table = bom_export::table(&self.project, &o).unwrap_or(bom_export::Table {
            headers: vec![],
            rows: vec![],
            header: true,
            missing: vec![],
        });
        });
        let mut open = true;
        let mut export = false;
        egui::Window::new("Bill of materials preview")
            .id(egui::Id::new("bom-preview-window"))
            .open(&mut open)
            .collapsible(false)
            .default_width(620.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                preview_grid(ui, &table, 200, "bom-preview-all");
                ui.add_space(8.0);
                if ui
                    .button("Export…")
                    .on_hover_text("Choose the file type, columns and build, then write it.")
                    .clicked()
                {
                    export = true;
                }
                ui.add_space(6.0);
            });
        if export {
            self.edit.dialog.preview_open = false;
            self.open_dialog(Mode::Export, Some("bom"), None, vec![]);
        } else if !open {
            self.edit.dialog.preview_open = false;
        }
    }
}

/// Draw the first `limit` rows of `table`.
fn preview_grid(ui: &mut egui::Ui, table: &bom_export::Table, limit: usize, id: &str) {
    ui.label(format!(
        "{} rows{}",
        table.rows.len(),
        if table.rows.len() > limit {
            format!(", showing the first {limit}")
        } else {
            String::new()
        }
    ));
    egui::ScrollArea::both()
        .id_salt(id)
        .max_height(240.0)
        .show(ui, |ui| {
            egui::Grid::new(id)
                .striped(true)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for h in &table.headers {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for row in table.rows.iter().take(limit) {
                        for cell in row {
                            ui.label(egui::RichText::new(cell).monospace());
                        }
                        ui.end_row();
                    }
                });
        });
}

/// The left column: formats grouped under headings, then Packages for Export.
fn format_list(ui: &mut egui::Ui, d: &mut DialogState, project: &Project, muted: egui::Color32) {
    let groups: Vec<(String, Vec<&'static FormatSpec>)> = if d.mode == Mode::SaveAs {
        vec![(
            "Project".to_owned(),
            formats::listed(Direction::Export, Scope::Project),
        )]
    } else {
        formats::grouped(d.mode.direction(), d.mode.scope())
            .into_iter()
            .map(|(g, list)| (g.label().to_owned(), list))
            .collect()
    };
    for (heading, list) in groups {
        ui.label(crate::fonts::caption(heading).color(muted));
        for f in list {
            let chosen = d.format == f.id;
            if ui
                .selectable_label(chosen, f.label)
                .on_hover_text(f.tip)
                .clicked()
                && !chosen
            {
                d.format = f.id.to_owned();
                d.values = defaults(project, f);
                if d.mode == Mode::SaveAs {
                    d.path = String::new();
                }
            }
        }
        ui.add_space(8.0);
    }
    if d.mode == Mode::Export {
        ui.label(crate::fonts::caption("Packages").color(muted));
        if packages().is_empty() {
            ui.label(egui::RichText::new("No packages yet.").color(muted))
                .on_hover_text(
                    "A package is a named bundle of several formats, such as the files one board maker asks for.",
                );
        }
        for p in packages() {
            let _ = ui
                .selectable_label(false, p.label)
                .on_hover_text("A named bundle of formats.");
        }
    }
}

/// The right column: tip, options, destination, preview and the action button.
#[allow(clippy::too_many_arguments)]
fn detail(
    ui: &mut egui::Ui,
    d: &mut DialogState,
    f: &'static FormatSpec,
    project: &Project,
    make_dir: &str,
    muted: egui::Color32,
    browser: bool,
    go: &mut bool,
) {
    ui.label(crate::fonts::title(f.label));
    ui.label(egui::RichText::new(f.tip).color(muted));
    ui.add_space(8.0);
    for o in f.options {
        option_row(ui, d, f, o, project);
    }
    if f.id == "bom" && !d.only.is_empty() {
        ui.label(format!("Listing the {} selected parts only.", d.only.len()));
    }
    ui.add_space(8.0);
    match d.mode {
        Mode::Export => {
            ui.label(crate::fonts::caption(format!("Saves into {make_dir}")).color(muted))
                .on_hover_text("The make folder. Change it in Settings under Make files.");
        }
        Mode::SaveAs | Mode::Import => {
            let before = d.path.clone();
            ui.horizontal(|ui| {
                ui.label(if d.mode == Mode::Import {
                    "File"
                } else {
                    "Save to"
                });
                ui.add(crate::targets::text_field(&mut d.path).desired_width(340.0))
                    .on_hover_text(match d.mode {
                        Mode::Import => format!(
                            "The file to bring in. Accepts {}.",
                            formats::accept_list(Direction::Import)
                        ),
                        _ => "Where to write the whole project.".to_owned(),
                    });
            });
            if d.mode == Mode::Import
                && d.path != before
                && let Some(found) = formats::for_path(&d.path, Direction::Import)
                && found.id != d.format
            {
                d.format = found.id.to_owned();
                d.values = defaults(project, found);
            }
        }
    }
    if f.id == "bom" && d.mode == Mode::Export {
        ui.add_space(8.0);
        let o = bom_export::Options::from_values(project, &d.values, &d.only);
        match bom_export::table(project, &o) {
Ok(table) => {
                bom_needs_ui(ui, &table, &o);
                preview_grid_limited(ui, &table);
            }
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        }
    }
if f.id == "bom" && d.mode == Mode::Export {
        ui.add_space(8.0);
        let o = bom_export::Options::from_values(project, &d.values, &d.only);
        match bom_export::table(project, &o) {
            Ok(table) => preview_grid_limited(ui, &table),
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        }
    }
    if f.id == "gerbers" && d.mode == Mode::Export {
        ui.add_space(8.0);
        gerber_preview(ui, d, f, project);
    }
    ui.add_space(10.0);
    let (label, tip) = match d.mode {
        Mode::Export if f.id == "bom" && browser => (
            "Copy",
            "The browser build cannot write files yet: copy the list to the clipboard.",
        ),
        Mode::Export => ("Export", "Write this file now."),
        Mode::Import => (
            "Import",
            "Bring the file into this design. One undo takes it out.",
        ),
        Mode::SaveAs => ("Save", "Write the whole project to this file."),
    };
    if ui.button(label).on_hover_text(tip).clicked() {
        *go = true;
    }
}

fn bom_needs_ui(ui: &mut egui::Ui, table: &bom_export::Table, o: &bom_export::Options) {
    for m in &table.missing {
        let shown: Vec<&str> = m.references.iter().take(12).map(String::as_str).collect();
        let more = m.references.len().saturating_sub(shown.len());
        let mut text = format!(
            "{} part{} lack{} a {}: {}",
            m.references.len(),
            if m.references.len() == 1 { "" } else { "s" },
            if m.references.len() == 1 { "s" } else { "" },
            m.what,
            shown.join(", ")
        );
        if more > 0 {
            text.push_str(&format!(" and {more} more"));
        }
        ui.colored_label(ui.visuals().warn_fg_color, text).on_hover_text(
            "The file is still written, with these cells blank. Add the field in the part's Properties first if the site needs it.",
        );
    }
    for note in bom_export::limit_notes(table, o) {
        ui.colored_label(ui.visuals().warn_fg_color, note)
            .on_hover_text("The receiving site says it will refuse or cut a larger upload.");
    }
}

fn preview_grid_limited(ui: &mut egui::Ui, table: &bom_export::Table) {
    ui.label(egui::RichText::new("Preview").strong());
    let shown = bom_export::Table {
        headers: table.headers.clone(),
        rows: table.rows.iter().take(PREVIEW_ROWS).cloned().collect(),
        header: table.header,
        missing: vec![],
    };
    ui.label(format!(
fn preview_grid_limited(ui: &mut egui::Ui, table: &bom_export::Table) {
    ui.label(egui::RichText::new("Preview").strong());
    let shown = bom_export::Table {
        headers: table.headers.clone(),
        rows: table.rows.iter().take(PREVIEW_ROWS).cloned().collect(),
header: table.header,
        missing: vec![],
        rows: table.rows.iter().take(PREVIEW_ROWS).cloned().collect(),
    };
    ui.label(format!(
        "First {} of {} rows",
        shown.rows.len(),
        table.rows.len()
    ));
    egui::ScrollArea::horizontal()
        .id_salt("bom-dialog-preview")
        .show(ui, |ui| {
            egui::Grid::new("bom-dialog-grid")
                .striped(true)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for h in &shown.headers {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for row in &shown.rows {
                        for cell in row {
                            ui.label(egui::RichText::new(cell).monospace());
                        }
                        ui.end_row();
                    }
                });
        });
}

/// One option of the chosen format, bound to the dialog's value map.
fn option_row(
    ui: &mut egui::Ui,
    d: &mut DialogState,
    f: &FormatSpec,
    o: &OptionSpec,
    project: &Project,
) {
    match o.kind {
        OptionKind::Bool { default } => {
            let mut on = d.values.get(o.label).map_or(default, |v| v == "true");
            if ui.checkbox(&mut on, o.label).on_hover_text(o.tip).changed() {
                d.values.insert(o.label.to_owned(), on.to_string());
            }
        }
        OptionKind::Choice { choices, default } => {
            let current = d
                .values
                .get(o.label)
                .cloned()
                .or_else(|| choices.get(default).map(|(v, _)| (*v).to_owned()))
                .unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                ui.label(o.label);
                for (value, label) in choices {
                    if ui
                        .selectable_label(current == *value, *label)
                        .on_hover_text(o.tip)
                        .clicked()
                    {
                        d.values.insert(o.label.to_owned(), (*value).to_owned());
                    }
                }
            });
        }
        OptionKind::Flavour => {
            let current = bom_export::flavour_of(&d.values);
ui.horizontal_wrapped(|ui| {
                ui.label(o.label);
                egui::ComboBox::from_id_salt("bom_flavour")
                    .selected_text(&current.label)
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for flavour in formats::bom_flavours() {
                            if ui
                                .selectable_label(current.id == flavour.id, &flavour.label)
                                .on_hover_text(&flavour.tip)
                                .clicked()
                                && current.id != flavour.id
                            {
                                let build =
                                    d.values.get(bom_export::BUILD).cloned().unwrap_or_default();
                                d.values = bom_export::default_values(project, flavour);
                                d.values.insert(bom_export::BUILD.to_owned(), build);
                            }
                        }
                    })
                    .response
                    .on_hover_text(o.tip);
            });
            let current = bom_export::flavour_of(&d.values);
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&current.tip).weak());
            if !current.header {
                ui.label(
                    egui::RichText::new(
                        "This file has no header row. The preview names the columns for reading only.",
                    )
                    .weak(),
                );
            }
        }
        OptionKind::Variant => {
            if project.variants.is_empty() {
                return;
            }
            let current = d.values.get(o.label).cloned().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                ui.label(o.label);
                if ui
                    .selectable_label(current.is_empty(), "All parts")
                    .on_hover_text(o.tip)
                    .clicked()
                {
                    d.values.insert(o.label.to_owned(), String::new());
                }
                for v in &project.variants {
                    if ui
                        .selectable_label(current == v.name, &v.name)
                        .on_hover_text(o.tip)
                        .clicked()
                    {
                        d.values.insert(o.label.to_owned(), v.name.clone());
                    }
                }
            });
        }
        OptionKind::Columns => {
            let flavour = bom_export::flavour_of(&d.values);
            let mut chosen: Vec<String> = d
                .values
                .get(o.label)
                .map(|c| {
                    c.split(',')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let mut changed = false;
            ui.label(o.label);
            ui.horizontal_wrapped(|ui| {
                let mut toggle = |ui: &mut egui::Ui, token: String, label: String| {
                    let mut on = chosen.contains(&token);
                    if ui.checkbox(&mut on, label).on_hover_text(o.tip).changed() {
                        if on {
                            chosen.push(token);
                        } else {
                            chosen.retain(|c| *c != token);
                        }
                        changed = true;
                    }
                };
for (token, header) in flavour.columns {
                    toggle(ui, token.to_owned(), header.to_owned());
                }
                }
                for field in &project.bom_fields {
                    let label = if field.label.is_empty() {
                        &field.key
                    } else {
                        &field.label
                    };
                    toggle(ui, format!("field:{}", field.key), label.clone());
                }
            });
            if changed {
                d.values.insert(o.label.to_owned(), chosen.join(","));
            }
        }
    }
    let _ = f;
}

impl App {
    /// Capture option: stage a scene for the screenshot (`file-export`, `export-dialog`, `export-flavour`).
    pub fn with_capture_show(mut self, scene: Option<String>) -> Self {
        self.edit.dialog.script.show = scene;
        self
    }

    /// Frame 3 of a `--show` capture: seed a Recent export or open the dialog.
    pub(crate) fn capture_scene(&mut self) {
        match self.edit.dialog.script.show.clone().as_deref() {
            Some("file-export") => {
                let mut values =
                    bom_export::default_values(&self.project, formats::default_bom_flavour());
    pub fn with_capture_show(mut self, scene: Option<String>) -> Self {
        self.edit.dialog.script.show = scene;
        self
    }

    /// Frame 3 of a `--show` capture: seed a Recent export or open the dialog.
    pub(crate) fn capture_scene(&mut self) {
        match self.edit.dialog.script.show.clone().as_deref() {
            Some("file-export") => {
                let mut values =
bom_export::default_values(&self.project, &formats::BOM_FLAVOURS[0]);
                values.insert(bom_export::FILE_TYPE.into(), "tsv".into());
                values.insert(bom_export::GROUP.into(), "false".into());
                let summary = bom_export::summary(&self.project, &values);
                remember(
                    &mut self.preferences.recent_files.exports,
                    Recent {
                        format: "bom".into(),
                        values,
                        summary,
                    },
                );
            }
            Some("export-dialog") => self.open_dialog(Mode::Export, Some("bom"), None, vec![]),
Some("export-flavour") => {
                self.open_dialog(Mode::Export, Some("bom"), None, vec![]);
                if let Some(flavour) = formats::bom_flavour("asm_proto_a") {
                    self.edit.dialog.values = bom_export::default_values(&self.project, flavour);
                }
            }
            Some("export-dialog") => {
                self.open_dialog(Mode::Export, Some("bom"), None, vec![]);
                let mut values = self.edit.dialog.values.clone();
                values.insert(bom_export::GROUP.into(), "false".into());
                let summary = bom_export::summary(&self.project, &values);
                remember(
                    &mut self.preferences.recent_files.exports,
                    Recent {
                        format: "bom".into(),
                        values,
                        summary,
                    },
                );
            }
            Some(scene) if scene.starts_with("library-") => self.capture_library_scene(scene),
            Some(scene) if scene.starts_with("files-") => self.files_capture_scene(scene),
            Some(scene) => {
                self.capture_project_settings(scene);
            }
            None => {}
        }
    }
            _ => {}
        }
    }

    /// Feed the scripted clicks of a `--show file-export` capture into the next frame.
    pub(crate) fn capture_inputs(&mut self, raw: &mut egui::RawInput) {
        if self.capture.is_none() {
            return;
        }
        let frames = self.frames;
        let script = &mut self.edit.dialog.script;
        if script.show.as_deref() != Some("file-export") {
            return;
        }
        let button = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        if let Some(pos) = script.release.take() {
            raw.events.push(button(pos, false));
            return;
        }
        if frames < 8 {
            return;
        }
        script.wait += 1;
        let target = match script.step {
            0 => "File",
            1 if script.wait >= 6 => "Export",
            _ => return,
        };
        if let Some(pos) = script.rects.get(target).copied() {
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(button(pos, true));
            script.release = Some(pos);
            script.step += 1;
            script.wait = 0;
        }
    }
}
1 if script.wait >= 6 => "Export",
            _ => return,
        };
        if let Some(pos) = script.rects.get(target).copied() {
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(button(pos, true));
            script.release = Some(pos);
            script.step += 1;
            script.wait = 0;
        }
    }
}

/// Command parameters from the option values of a format whose options all name a parameter.
///
/// # Rules
/// - A choice sends its value, a checkbox sends a boolean; an option the person never
///   touched sends its default.
///
/// # Dependencies
/// - crates/io/src/formats.rs `OptionSpec`.
///
/// # Do not
/// - Add a directory here; the caller owns where files go.
fn gerber_params(f: &FormatSpec, d: &DialogState) -> serde_json::Value {
    let mut params = json!({});
    for o in f.options {
        let Some(param) = o.param else { continue };
        match o.kind {
            OptionKind::Bool { default } => {
                let on = d.values.get(o.label).map_or(default, |v| v == "true");
                params[param] = json!(on);
            }
            OptionKind::Choice { choices, default } => {
                let value = d
                    .values
                    .get(o.label)
                    .cloned()
                    .or_else(|| choices.get(default).map(|(v, _)| (*v).to_owned()));
                if let Some(value) = value {
                    params[param] = json!(value);
                }
            }
            _ => {}
        }
    }
    params
}

/// The file names the Export button would write, so the naming choice is visible first.
fn gerber_preview(ui: &mut egui::Ui, d: &DialogState, f: &FormatSpec, project: &Project) {
    use pcbcraft_io::gerber_preset::{NamingPreset, preview_names};
    let params = gerber_params(f, d);
    let preset = params["naming"]
        .as_str()
        .and_then(NamingPreset::from_id)
        .unwrap_or_default();
    let stem = pcbcraft_io::gerber_preset::file_stem(project);
    ui.label("Preview");
    let merged = params["drill_files"].as_str() == Some("merged");
    let names = preview_names(preset, &stem, project.copper_layer_count(), merged);
    egui::Frame::new()
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            for name in names {
                ui.label(name);
            }
        });
}
