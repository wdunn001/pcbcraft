use super::*;
use pcbcraft_library::{FootprintDefinition, SymbolDefinition};

#[path = "host.rs"]
pub mod host;
pub use host::{MemHost, NativeHost, ProjectHost, replace_session_from_open};

#[derive(Default)]
pub(super) struct Browser {
    pub(super) editor: super::authoring::Editor,
    pub(super) preview_unit: u32,
    pub(super) catalog: Option<Value>,
    pub(super) kind: bool,
    pub(super) path: String,
    pub(super) query: String,
    pub(super) filter: String,
    pub(super) results: Option<Value>,
    pub(super) preview: Option<Value>,
    pub(super) pending: Option<std::sync::mpsc::Receiver<(String, Result<Value, String>)>>,
    pub(super) error: String,
    pub(super) sources: Option<Value>,
    pub(super) source_filter: String,
    pub(super) new_source: String,
    pub(super) notice: String,
    pub(super) quality: super::library_quality::Quality,
}
impl Browser {
    fn request(&mut self, ctx: &egui::Context, name: &str, args: Value) {
        if self.pending.is_some() {
            return;
        }
        self.error.clear();
        // Notices stay on screen until the next source change; a refresh
        // request right after an add must not erase the confirmation.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ctx.clone();
            let name = name.to_owned();
            match std::thread::Builder::new()
                .name("pcbcraft-library".into())
                .spawn(move || {
                    let result = pcbcraft_engine::library::read_tool(&name, &args);
                    let _ = tx.send((name, result));
                    ctx.request_repaint();
                }) {
                Ok(_) => self.pending = Some(rx),
                Err(e) => self.error = e.to_string(),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (ctx, name, args);
            self.error = "Installed library browsing is available in the desktop build.".into();
            self.catalog = Some(json!({}));
            if name == "library.sources" {
                self.sources = Some(json!({"sources": []}));
            }
        }
    }
    fn search(&mut self, ctx: &egui::Context) {
        self.preview = None;
        self.request(ctx,"library.search",json!({"path":self.path,"kind":if self.kind {"footprint"} else {"symbol"},"query":self.query,"limit":100}));
    }
    fn poll(&mut self) {
        let response = self.pending.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some((name, result)) = response {
            self.pending = None;
            match result {
                Ok(v) => match name.as_str() {
                    "library.catalog" => self.catalog = Some(v),
                    "library.sources" => self.sources = Some(v),
                    "library.add_folder" => {
                        self.notice = format!(
                            "Added {} · {} symbol libraries · {} footprint libraries now in the unified list",
                            v["added"].as_str().unwrap_or(""),
                            v["symbol_libraries"].as_u64().unwrap_or(0),
                            v["footprint_libraries"].as_u64().unwrap_or(0)
                        );
                        self.sources = Some(json!({"sources": v["sources"].clone()}));
                        self.catalog = None;
                        self.new_source.clear();
                    }
                    "library.remove_folder" => {
                        self.notice = format!(
                            "Removed {} from the library sources; installed folders stay untouched",
                            v["removed"].as_str().unwrap_or("")
                        );
                        self.sources = Some(json!({"sources": v["sources"].clone()}));
                        self.catalog = None;
                        self.path.clear();
                        self.results = None;
                        self.preview = None;
                    }
                    "library.search" => self.results = Some(v),
                    _ => self.preview = Some(v),
                },
                Err(e) => {
                    self.error = e;
                    if name == "library.catalog" {
                        self.catalog = Some(json!({}));
                    }
                    if name == "library.sources" {
                        self.sources = Some(json!({"sources": []}));
                    }
                }
            }
        }
    }
}
/// True when a result entry should appear under a typed filter: an empty
/// filter shows everything, otherwise the name or the description must
/// contain the filter, ignoring case.
pub(super) fn entry_matches(item: &Value, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    filter.is_empty()
        || item
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.to_lowercase().contains(&filter))
        || item
            .get("description")
            .and_then(Value::as_str)
            .is_some_and(|description| description.to_lowercase().contains(&filter))
}
/// The source a catalog entry came from; entries written before sources were
/// tagged read as the installed collections.
pub(super) fn entry_source(item: &Value) -> String {
    item.get("source")
        .and_then(Value::as_str)
        .filter(|source| !source.is_empty())
        .unwrap_or("Installed collections")
        .to_owned()
}
/// Every distinct source among the listed collections, each named once, sorted.
pub(super) fn distinct_sources(items: &[Value]) -> Vec<String> {
    let mut names: Vec<String> = items.iter().map(entry_source).collect();
    names.sort();
    names.dedup();
    names
}
impl App {
    pub(super) fn collection_browser(&mut self, ui: &mut egui::Ui) {
        self.library.poll();
        if self.library.catalog.is_none() && self.library.pending.is_none() {
            self.library.request(ui.ctx(), "library.catalog", json!({}));
        }
        if self.library.sources.is_none() && self.library.pending.is_none() {
            self.library.request(ui.ctx(), "library.sources", json!({}));
        }
        ui.heading("Component libraries");
        ui.label(
            "One library for every collection below · add any folder as a source · definitions used by a project are embedded on placement.",
        );
        let busy = self.library.pending.is_some();
        let before = self.library.kind;
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.library.kind, false, "Symbols");
                ui.selectable_value(&mut self.library.kind, true, "Footprints");
                if ui
                    .button("Refresh folders")
                    .on_hover_text(
                        "Rescan the library folders for symbol and footprint collections.",
                    )
                    .clicked()
                {
                    self.library.catalog = None;
                    self.library.sources = None;
                    self.library.source_filter.clear();
                    self.library.path.clear();
                    self.library.results = None;
                    self.library.preview = None;
                    self.library.filter.clear();
                }
            });
        });
        if before != self.library.kind {
            self.library.path.clear();
            self.library.results = None;
            self.library.preview = None;
            self.library.query.clear();
            self.library.filter.clear();
        }
        let items = self
            .library
            .catalog
            .as_ref()
            .and_then(|v| {
                v.get(if self.library.kind {
                    "footprint_libraries"
                } else {
                    "symbol_libraries"
                })
            })
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut changed = false;
        if self.library.path.is_empty() && !items.is_empty() {
            let preferred = if self.library.kind {
                "Resistor_SMD"
            } else {
                "Device"
            };
            let item = items
                .iter()
                .find(|v| v.get("name").and_then(Value::as_str) == Some(preferred))
                .or_else(|| items.first());
            if let Some(path) = item.and_then(|v| v.get("path")).and_then(Value::as_str) {
                self.library.path = path.into();
                changed = true;
            }
        }
        let source_names = distinct_sources(&items);
        if !self.library.source_filter.is_empty()
            && !source_names
                .iter()
                .any(|name| name == &self.library.source_filter)
        {
            self.library.source_filter.clear();
        }
        if source_names.len() > 1 {
            ui.horizontal(|ui| {
                ui.label("Sources");
                if ui
                    .selectable_label(self.library.source_filter.is_empty(), "All")
                    .on_hover_text("Show collections from every library source together.")
                    .clicked()
                {
                    self.library.source_filter.clear();
                }
                for name in &source_names {
                    ui.selectable_value(
                        &mut self.library.source_filter,
                        name.clone(),
                        name.as_str(),
                    )
                    .on_hover_text(format!("Show only the collections that come from {name}."));
                }
            });
        }
        let items: Vec<Value> = if self.library.source_filter.is_empty() {
            items
        } else {
            items
                .iter()
                .filter(|item| entry_source(item) == self.library.source_filter)
                .cloned()
                .collect()
        };
        ui.add_enabled_ui(!busy, |ui| {
            let label = std::path::Path::new(&self.library.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Choose library");
            egui::ComboBox::from_id_salt("kicad-library")
                .selected_text(label)
                .width(300.0)
                .show_ui(ui, |ui| {
                    let mut grouped = items.clone();
                    grouped.sort_by(|a, b| {
                        (
                            entry_source(a),
                            a.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                        )
                            .cmp(&(
                                entry_source(b),
                                b.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                            ))
                    });
                    let mut last_source: Option<String> = None;
                    for item in &grouped {
                        let source = entry_source(item);
                        if Some(&source) != last_source.as_ref() {
                            ui.label(
                                egui::RichText::new(source.as_str())
                                    .weak()
                                    .small()
                                    .strong(),
                            );
                            last_source = Some(source);
                        }
                        if let (Some(name), Some(path)) = (
                            item.get("name").and_then(Value::as_str),
                            item.get("path").and_then(Value::as_str),
                        ) {
                            changed |= ui
                                .selectable_value(&mut self.library.path, path.to_owned(), name)
                                .on_hover_text("Browse this collection.")
                                .changed();
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Library path");
                ui.add(egui::TextEdit::singleline(&mut self.library.path).desired_width(480.0))
                    .on_hover_text("The collection the search box below reads; pick any library from the list above or type a path.");
            });
            ui.horizontal(|ui| {
                let response = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.library.new_source)
                            .hint_text("Folder to add as a library source")
                            .desired_width(320.0),
                    )
                    .on_hover_text("Any folder holding symbol files or footprint collections, from any distributor or your own shelf; it joins the list above without moving your files. Press Enter to add it.");
                let selected_source = items
                    .iter()
                    .find(|item| {
                        item.get("path").and_then(Value::as_str)
                            == Some(self.library.path.as_str())
                    })
                    .map(entry_source);
                let removable = self
                    .library
                    .sources
                    .as_ref()
                    .and_then(|listing| listing.get("sources"))
                    .and_then(Value::as_array)
                    .and_then(|list| {
                        list.iter().find(|source| {
                            source.get("kind").and_then(Value::as_str) == Some("added")
                                && source.get("name").and_then(Value::as_str)
                                    == selected_source.as_deref()
                        })
                    })
                    .and_then(|source| source.get("path"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let wants_add = ui
                    .button("Add folder")
                    .on_hover_text("Register the folder named to the left as a library source; its collections appear in the list above on the next scan.")
                    .clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if wants_add {
                    let path = self.library.new_source.trim().to_owned();
                    if !path.is_empty() {
                        self.library
                            .request(ui.ctx(), "library.add_folder", json!({"path": path}));
                    }
                }
                if let Some(folder) = removable
                    && ui
                        .button("Remove source")
                        .on_hover_text("Stop listing the added source that contains the selected library; installed folders are never removed.")
                        .clicked()
                {
                    self.library
                        .request(ui.ctx(), "library.remove_folder", json!({"path": folder}));
                    self.library.source_filter.clear();
                }
            });
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.library.query)
                        .hint_text("Search names, descriptions, keywords")
                        .desired_width(360.0),
                );
                if ui
                    .button("Search")
                    .on_hover_text(
                        "Find symbols and footprints by name across the loaded collections.",
                    )
                    .clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    changed = true;
                }
            });
        });
        if changed && !busy {
            self.library.search(ui.ctx());
        }
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading library…");
            });
        }
        if !self.library.error.is_empty() {
            ui.colored_label(Color32::from_rgb(237, 135, 113), &self.library.error);
        }
        if !self.library.notice.is_empty() {
            ui.label(&self.library.notice);
        }
        let results = self.library.results.clone().unwrap_or(json!({}));
        if self
            .library
            .catalog
            .as_ref()
            .and_then(|v| v.get("scan_truncated"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            ui.label("Some library folders exceeded the scan limit; use a specific library path.");
        }
        let filter = self.library.filter.trim().to_lowercase();
        ui.columns(2,|cols| {
            let left=&mut cols[0];
            if let Some(total)=results.get("total").and_then(Value::as_u64) {left.label(format!("{total} matches in scanned entries · showing up to 100"));}
            if results.get("scan_truncated").and_then(Value::as_bool)==Some(true) {left.label("Directory scan reached its limit; more entries may exist.");}
            let listed: Vec<Value> = results.get("entries").and_then(Value::as_array).cloned().unwrap_or_default();
            let shown: Vec<&Value> = listed.iter().filter(|item| entry_matches(item, &filter)).collect();
            let response = left.add(egui::TextEdit::singleline(&mut self.library.filter).hint_text("Filter shown results").desired_width(240.0));
            let response = response.on_hover_text("Narrow the list below as you type; matches names and descriptions, ignoring case. Double-click to clear.");
            if left.button("Clear filter").on_hover_text("Show every listed result again.").clicked() || response.double_clicked() {self.library.filter.clear();}
            if !filter.is_empty() {left.label(format!("{} of {} listed entries match the filter", shown.len(), listed.len()));}
            egui::ScrollArea::vertical().id_salt("library-results").max_height(360.0).show(left,|ui| {
                for item in shown {
                    let name=item.get("name").and_then(Value::as_str).unwrap_or("");
                    let response = ui.add_enabled(!busy,egui::Button::new(name).selected(self.library.preview.as_ref().and_then(|v|v.pointer("/definition/name")).and_then(Value::as_str)==Some(name)));
                    let path=item.get("path").and_then(Value::as_str).unwrap_or(&self.library.path);
                    response.dnd_set_drag_payload(json!({"kind": item.get("kind").and_then(Value::as_str).unwrap_or("symbol"), "name": name, "path": path}));
                    if response.clicked() {
                        self.library.preview=None;
                        let path=item.get("path").and_then(Value::as_str).unwrap_or(&self.library.path);
                        let (tool,args)=if self.library.kind {("library.inspect_footprint",json!({"path":path,"include_source":true}))} else {("library.inspect_symbol",json!({"path":path,"name":name,"include_source":true}))};
                        self.library.request(ui.ctx(),tool,args);
                    }
                }
            });
            let right=&mut cols[1];
            if let Some(preview)=self.library.preview.clone() {
                let path=preview.get("path").and_then(Value::as_str).unwrap_or("").to_owned();
                if preview.get("source_truncated").and_then(Value::as_bool)==Some(true) {right.label("Original nodes exceed inline editing limits; the source file is preserved.");}
                if self.library.kind {
                    if let Some(def)=preview.get("definition").and_then(|v|serde_json::from_value::<FootprintDefinition>(v.clone()).ok()) {
                        right.heading(&def.name);right.label(&def.description);
                        if right.add_enabled(preview.get("source_truncated").and_then(Value::as_bool)!=Some(true),egui::Button::new("Edit a copy →")).clicked() {let mut copy=def.clone();copy.name.push_str("_custom");self.library.editor.notice=Some(format!("Edited copy of {} from {}. KiCad-derived data credits the KiCad community contributors; custom sources retain their original license. Official collections: https://gitlab.com/kicad/libraries/kicad-symbols and https://gitlab.com/kicad/libraries/kicad-footprints.\n{}",def.name,path,include_str!("../../library/tests/fixtures/LICENSE-kicad-libraries.md")));self.library.editor.draft=Some(super::authoring::Draft::Footprint(copy));self.library.editor.mode=2;}
                        self.library_preview(right,None,Some(&def));
                        right.label(format!("{} pads · {} graphics",def.pads.len(),def.graphics.len()));
                        let supported=pcbcraft_doc::validate_footprint_support(&def);
                        if let Err(e)=&supported {right.label(e);}
                        let component=self.selected.and_then(|id|self.project.component(id)).map(|c|(c.id,c.reference.clone()));
                        if let Some((id,reference))=component {
                            let matches=self.project.component(id).is_some_and(|c|c.pins.iter().map(|p|p.number.as_str()).collect::<std::collections::BTreeSet<_>>()==def.pads.iter().map(|p|p.number.as_str()).collect::<std::collections::BTreeSet<_>>());
                            if !matches {right.label("Pad numbers must match every pin of the selected symbol.");}
                            if right.add_enabled(supported.is_ok() && matches,egui::Button::new(format!("Assign to {reference}"))).clicked() {self.run("footprint.assign",json!({"id":id,"path":path}));}
                        } else {right.label("Select a schematic component before assigning its footprint.");}
                    }
                } else if let Some(def)=preview.get("definition").and_then(|v|serde_json::from_value::<SymbolDefinition>(v.clone()).ok()) {
                    right.heading(&def.name);right.label(&def.description);
                    if right.add_enabled(preview.get("source_truncated").and_then(Value::as_bool)!=Some(true),egui::Button::new("Edit a copy →")).clicked() {let mut copy=def.clone();copy.name.push_str("_custom");self.library.editor.notice=Some(format!("Edited copy of {} from {}. KiCad-derived data credits the KiCad community contributors; custom sources retain their original license. Official collections: https://gitlab.com/kicad/libraries/kicad-symbols and https://gitlab.com/kicad/libraries/kicad-footprints.\n{}",def.name,path,include_str!("../../library/tests/fixtures/LICENSE-kicad-libraries.md")));self.library.editor.draft=Some(super::authoring::Draft::Symbol(copy));self.library.editor.mode=2;}
                    self.library_preview(right,Some(&def),None);
                    let supported=pcbcraft_doc::library_component(def.clone(),"preview".into(),0,"preview".into(),Point::default(),None).is_ok();
                    right.label(format!("{} pins · {} units",def.pins.len(),def.pins.iter().map(|p|p.unit).max().unwrap_or(1)));
                    right.label(format!("Suggested footprint: {}",def.footprint));
                    if right.add_enabled(supported,egui::Button::new("Place in schematic →")).clicked() {
                        self.change_tab(Tab::Schematic);self.tool=Tool::LibraryPlace(path,def.name.clone(),Box::new(def));
                    }
                    if !supported {right.label("Inspection available; unsupported pin/graphic styles or repeated common pin numbers cannot be placed yet.");}
                }
            } else {right.label("Choose an entry to preview its real geometry.");}
        });
        ui.separator();
        ui.hyperlink_to(
            "Library data license: CC-BY-SA 4.0 + design exception",
            "https://www.kicad.org/libraries/license/",
        );
        ui.label("Placement: single/multi-unit symbols, matching front SMD or plated through-hole packages. Use Edit a copy to customize a definition.");
    }
    pub(super) fn library_preview(
        &mut self,
        ui: &mut egui::Ui,
        symbol: Option<&SymbolDefinition>,
        footprint: Option<&FootprintDefinition>,
    ) {
        if let Some(s) = symbol {
            self.library.preview_unit = self.library.preview_unit.max(1);
            let units = pcbcraft_doc::symbol_units(s).unwrap_or_default();
            if !units.contains(&self.library.preview_unit) {
                self.library.preview_unit = 1;
            }
            if units.len() > 1 {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Preview unit");
                    for unit in units {
                        ui.selectable_value(&mut self.library.preview_unit, unit, unit.to_string())
                            .on_hover_text("Draw this half of the part in the preview.");
                    }
                });
            }
        }
        let paths = library_geometry_paths(symbol, footprint, self.library.preview_unit.max(1));
        let (response, painter) =
            ui.allocate_painter(Vec2::new(ui.available_width(), 180.0), egui::Sense::hover());
        let (center, extent) = library_paths_extent(&paths);
        let camera = Camera {
            center,
            zoom: ((response.rect.width() - 24.0) / extent.x as f32)
                .min(150.0 / extent.y as f32)
                .clamp(0.01, 40.0),
        };
        painter.rect_filled(response.rect, 6.0, self.surface());
        paint_library_paths(&painter, response.rect, camera, &paths, self.accent());
    }
}

pub(super) fn library_geometry_paths(
    symbol: Option<&SymbolDefinition>,
    footprint: Option<&FootprintDefinition>,
    preview_unit: u32,
) -> Vec<Vec<Point>> {
    let mut paths: Vec<Vec<Point>> = vec![];
    if let Some(s) = symbol {
        paths.extend(
            s.graphics
                .iter()
                .filter(|g| (g.unit == 0 || g.unit == preview_unit) && g.body_style <= 1)
                .map(|g| g.primitive.path_points()),
        );
        for p in s.pins.iter().filter(|p| {
            (p.unit == 0 && preview_unit == 1 || p.unit == preview_unit) && p.body_style <= 1
        }) {
            paths.push(vec![p.position, p.stem_end()]);
            for (center, radius) in p.decoration_bubbles_draw(false) {
                paths.push(pcbcraft_library::SymbolPin::circle_points(center, radius));
            }
            paths.extend(p.decoration_paths_draw(false));
        }
    }
    if let Some(f) = footprint {
        paths.extend(f.graphics.iter().map(|g| g.path_points()));
        paths.extend(f.pads.iter().map(|p| {
            pcbcraft_doc::Pad {
                number: p.number.clone(),
                offset: p.position,
                diameter: p.size.x.min(p.size.y),
                net: None,
                castellated: None,
                geometry: Some(p.clone()),
            }
            .outline_points()
        }));
        paths.extend(f.pads.iter().map(|p| {
            pcbcraft_doc::Pad {
                number: p.number.clone(),
                offset: p.position,
                diameter: p.size.x.min(p.size.y),
                net: None,
                castellated: None,
                geometry: Some(p.clone()),
            }
            .drill_outline_points()
        }));
    }
    paths
}

pub(super) fn library_paths_extent(paths: &[Vec<Point>]) -> (Point, Point) {
    let min = paths
        .iter()
        .flatten()
        .fold(Point::new(f64::INFINITY, f64::INFINITY), |m, p| {
            Point::new(m.x.min(p.x), m.y.min(p.y))
        });
    let max = paths
        .iter()
        .flatten()
        .fold(Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY), |m, p| {
            Point::new(m.x.max(p.x), m.y.max(p.y))
        });
    if min.x.is_finite() && min.y.is_finite() {
        (
            Point::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0),
            Point::new((max.x - min.x).max(1.0), (max.y - min.y).max(1.0)),
        )
    } else {
        (Point::default(), Point::new(2.0, 2.0))
    }
}

pub(super) fn library_paths_bounds(paths: &[Vec<Point>]) -> (Point, Point) {
    let min = paths
        .iter()
        .flatten()
        .fold(Point::new(f64::INFINITY, f64::INFINITY), |m, p| {
            Point::new(m.x.min(p.x), m.y.min(p.y))
        });
    let max = paths
        .iter()
        .flatten()
        .fold(Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY), |m, p| {
            Point::new(m.x.max(p.x), m.y.max(p.y))
        });
    if min.x.is_finite() && min.y.is_finite() {
        let pad = 1.0;
        (
            Point::new(min.x - pad, min.y - pad),
            Point::new(max.x + pad, max.y + pad),
        )
    } else {
        (Point::new(-4.0, -4.0), Point::new(4.0, 4.0))
    }
}

fn paint_library_paths(
    painter: &egui::Painter,
    rect: Rect,
    camera: Camera,
    paths: &[Vec<Point>],
    color: Color32,
) {
    for path in paths.iter().filter(|p| p.len() > 1) {
        painter.add(egui::Shape::line(
            path.iter().map(|p| camera.screen(*p, rect)).collect(),
            Stroke::new(1.5, color),
        ));
    }
}
