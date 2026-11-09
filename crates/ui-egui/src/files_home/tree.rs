//! The project files tree: the left aside of the Files tab.
//!
//! Context: the approved mockup `files`. The aside lists what the open project holds on one
//! tree: the design file, the project library (symbols, footprints, 3D models), the attached
//! files (datasheets, notes, images), made files and history. It never shows the PCB layout
//! layers; those belong to Schematic and Layout.
//! Local rules: every count and name comes from the open project. A section with no data
//! source yet (made files, history) says so in one honest row. Which sections the person
//! folded is state in `files_home::State`. A filter box narrows the rows by name.
//!
//! # Dependencies
//! - crates/ui-egui/src/files_home.rs (state, probes)
//! - crates/ui-egui/src/ui_kit/icons.rs
//! - crates/ui-egui/src/ui_kit/button.rs
//! - crates/doc/src/lib.rs (`EmbeddedFile`, `ProjectLibrary`)
//!
//! # Do not
//! - Invent a file; list only what the project holds.
//! - Draw a layer row here.
use super::home::file_name;
use super::*;
use crate::a11y::Named;
use pcbcraft_doc::EmbeddedFile;

/// The kinds of attached file the tree groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Attached {
    /// A datasheet or other document.
    Datasheet,
    /// A text note.
    Note,
    /// A picture.
    Image,
}

/// Which group an attached file belongs to, from how it is stored and what it is called.
pub(super) fn attached_kind(file: &EmbeddedFile) -> Attached {
    let lower = file.name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    if matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp") {
        Attached::Image
    } else if ext == "pdf" || lower.contains("datasheet") {
        Attached::Datasheet
    } else {
        Attached::Note
    }
}

/// Height of one tree row in points, also its smallest target.
const ROW_HEIGHT: f32 = 28.0;
/// Horizontal step of one nesting level in points.
const INDENT: f32 = 16.0;
/// Largest share of a row the note at its right may take.
const NOTE_SHARE: f32 = 0.45;

/// What one tree row shows.
struct Line<'a> {
    depth: usize,
    /// `Some(open)` draws a chevron; the whole row is the control that folds it.
    chevron: Option<bool>,
    icon: Option<icons::Icon>,
    name: &'a str,
    /// Words or a count at the right edge, in the muted colour.
    note: Option<String>,
    selected: bool,
    dimmed: bool,
}

/// Draw one row: chevron, icon, name and note. Long names end in an ellipsis.
///
/// Why: the whole row is one 28 point target; a 16 point chevron cell would be too small.
fn tree_line(ui: &mut egui::Ui, salt: impl std::hash::Hash, line: &Line<'_>) -> egui::Response {
    let tokens = theme::tokens(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::click(),
    );
    let spoken = match line.chevron {
        Some(true) => format!("{} {}", t!("files.tree.fold"), line.name),
        Some(false) => format!("{} {}", t!("files.tree.unfold"), line.name),
        None => line.name.to_owned(),
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, line.selected, &spoken)
    });
    let _ = salt;
    if line.selected {
        ui.painter().rect_filled(rect, 0.0, tokens.copper_tint);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
            0.0,
            tokens.copper,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, tokens.raised);
    }
    let fade = if line.dimmed { 0.45 } else { 1.0 };
    let ink = tokens.text.gamma_multiply(fade);
    let mut x = rect.min.x + 8.0 + INDENT * line.depth as f32;
    let cy = rect.center().y;
    if let Some(open) = line.chevron {
        let icon = if open {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        };
        let cell = egui::Rect::from_min_size(
            egui::pos2(x, cy - icons::ICON_SIZE / 2.0),
            egui::Vec2::splat(icons::ICON_SIZE),
        );
        (icon.0)(ui.painter(), cell, tokens.muted);
    }
    x += icons::ICON_SIZE + 4.0;
    if let Some(icon) = line.icon {
        let cell = egui::Rect::from_min_size(
            egui::pos2(x, cy - icons::ICON_SIZE / 2.0),
            egui::Vec2::splat(icons::ICON_SIZE),
        );
        (icon.0)(ui.painter(), cell, tokens.muted.gamma_multiply(fade));
        x += icons::ICON_SIZE + 8.0;
    }
    let right = rect.max.x - 8.0;
    let mut name_right = right;
    if let Some(note) = &line.note {
        let galley =
            ui.painter()
                .layout_no_wrap(note.clone(), fonts::Role::Mono.font(), tokens.muted);
        let width = galley.size().x.min(rect.width() * NOTE_SHARE);
        let note_rect = egui::Rect::from_min_max(
            egui::pos2(right - width, rect.min.y),
            egui::pos2(right, rect.max.y),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(note_rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        child.add(
            egui::Label::new(fonts::mono(note).color(tokens.muted))
                .selectable(false)
                .truncate(),
        );
        name_right = right - width - 8.0;
    }
    let name_rect = egui::Rect::from_min_max(
        egui::pos2(x, rect.min.y),
        egui::pos2(name_right.max(x), rect.max.y),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(name_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let rich =
        if line.depth == 0 && line.chevron.is_some() || line.depth == 0 && line.note.is_some() {
            fonts::section_caption(line.name)
        } else if line.selected {
            fonts::strong(line.name)
        } else {
            fonts::text(fonts::Role::Body, line.name)
        };
    child.add(
        egui::Label::new(rich.color(ink))
            .selectable(false)
            .truncate(),
    );
    crate::ui_kit::focus::focus_ring(ui, &response, 0);
    response
}

/// One group of names under a section: its key, words and the names it lists.
struct Group {
    key: &'static str,
    title: String,
    tip: String,
    icon: icons::Icon,
    names: Vec<String>,
}

impl App {
    /// True when `name` passes the tree filter.
    fn tree_filter_matches(&self, name: &str) -> bool {
        let filter = self.files_home.filter.trim().to_lowercase();
        filter.is_empty() || name.to_lowercase().contains(&filter)
    }

    /// One folding row; returns whether its children show.
    ///
    /// # Rules
    /// - A row with no children has no chevron.
    /// - While a filter is typed every row with a match stays open.
    fn tree_head(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        title: &str,
        tip: &str,
        count: usize,
        depth: usize,
    ) -> bool {
        let filtering = !self.files_home.filter.trim().is_empty();
        let default_open = depth == 0;
        let flipped = self.files_home.tree_flipped.contains(key);
        let open = filtering || (default_open != flipped);
        let line = Line {
            depth,
            chevron: (count > 0).then_some(open),
            icon: None,
            name: title,
            note: Some(count.to_string()),
            selected: false,
            dimmed: false,
        };
        let row = tree_line(ui, ("files-tree-head", key), &line).on_hover_text(tip);
        if row.clicked() && count > 0 {
            if flipped {
                self.files_home.tree_flipped.remove(key);
            } else {
                self.files_home.tree_flipped.insert(key.to_owned());
            }
        }
        count > 0 && open
    }

    /// One name row under a head.
    fn tree_leaf(
        ui: &mut egui::Ui,
        key: (&str, usize),
        depth: usize,
        icon: icons::Icon,
        name: &str,
        note: Option<String>,
        dimmed: bool,
    ) -> egui::Response {
        let line = Line {
            depth,
            chevron: None,
            icon: Some(icon),
            name,
            note,
            selected: false,
            dimmed,
        };
        tree_line(ui, ("files-tree-leaf", key.0, key.1), &line)
    }

    /// The groups under Project library and Attached files, from the open project.
    fn tree_groups(&self) -> (Vec<Group>, Vec<Group>) {
        let lib = &self.project.library;
        let library = vec![
            Group {
                key: "symbols",
                title: t!("files.tree.symbols"),
                tip: t!("files.tree.symbols.tip"),
                icon: icons::CHIP,
                names: lib.symbols.iter().map(|s| s.name.clone()).collect(),
            },
            Group {
                key: "footprints",
                title: t!("files.tree.footprints"),
                tip: t!("files.tree.footprints.tip"),
                icon: icons::LAYERS,
                names: lib.footprints.iter().map(|f| f.name.clone()).collect(),
            },
            Group {
                key: "models",
                title: t!("files.tree.models"),
                tip: t!("files.tree.models.tip"),
                icon: icons::FILE,
                names: lib
                    .footprints
                    .iter()
                    .filter_map(|f| f.model.as_ref().map(|m| m.path.clone()))
                    .collect(),
            },
        ];
        let named = |kind: Attached| -> Vec<String> {
            self.project
                .files
                .iter()
                .filter(|f| attached_kind(f) == kind)
                .map(|f| f.name.clone())
                .collect()
        };
        let mut images = named(Attached::Image);
        images.extend(self.project.reference_images.iter().map(|r| r.file.clone()));
        images.sort();
        images.dedup();
        let attached = vec![
            Group {
                key: "datasheets",
                title: t!("files.tree.datasheets"),
                tip: t!("files.tree.datasheets.tip"),
                icon: icons::FILE,
                names: named(Attached::Datasheet),
            },
            Group {
                key: "notes",
                title: t!("files.tree.notes"),
                tip: t!("files.tree.notes.tip"),
                icon: icons::FILE,
                names: named(Attached::Note),
            },
            Group {
                key: "images",
                title: t!("files.tree.images"),
                tip: t!("files.tree.images.tip"),
                icon: icons::FILE,
                names: images,
            },
        ];
        (library, attached)
    }

    /// Draw one section whose children are groups of names.
    fn tree_group_section(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        title: &str,
        tip: &str,
        groups: &[Group],
    ) {
        let filtering = !self.files_home.filter.trim().is_empty();
        let total: usize = groups.iter().map(|g| g.names.len()).sum();
        if filtering
            && !groups
                .iter()
                .any(|g| g.names.iter().any(|n| self.tree_filter_matches(n)))
        {
            return;
        }
        if !self.tree_head(ui, key, title, tip, total, 0) {
            return;
        }
        for group in groups {
            let shown: Vec<&String> = group
                .names
                .iter()
                .filter(|n| self.tree_filter_matches(n))
                .collect();
            if filtering && shown.is_empty() {
                continue;
            }
            if self.tree_head(
                ui,
                group.key,
                &group.title,
                &group.tip,
                group.names.len(),
                1,
            ) {
                for (i, name) in shown.into_iter().enumerate() {
                    Self::tree_leaf(ui, (group.key, i), 2, group.icon, name, None, false)
                        .on_hover_text(&group.tip);
                }
            }
        }
    }

    /// The Made files section: nothing records what was made yet.
    fn tree_made_section(&mut self, ui: &mut egui::Ui) {
        if !self.files_home.filter.trim().is_empty() {
            return;
        }
        let (title, tip) = (t!("files.tree.made"), t!("files.tree.made.tip"));
        self.tree_head(ui, "made", &title, &tip, 0, 0);
        Self::tree_leaf(
            ui,
            ("made", 0),
            1,
            icons::UPLOAD,
            &t!("files.tree.made.none"),
            None,
            true,
        )
        .on_hover_text(&tip);
    }

    /// The History section: the last save of the design file, the only history recorded.
    fn tree_history_section(&mut self, ui: &mut egui::Ui) {
        if !self.files_home.filter.trim().is_empty() {
            return;
        }
        let (title, tip) = (t!("files.tree.history"), t!("files.tree.history.tip"));
        let saved = self
            .files_home
            .probes
            .get(&self.path)
            .filter(|p| p.exists)
            .and_then(|p| p.modified);
        let count = usize::from(saved.is_some());
        let open = self.tree_head(ui, "history", &title, &tip, count, 0);
        match saved {
            Some(m) if open => {
                let label = t!("files.tree.history.saved", when = self.files_ago(m));
                Self::tree_leaf(ui, ("history", 0), 1, icons::FOLDER, &label, None, false)
                    .on_hover_text(&tip);
            }
            Some(_) => {}
            None => {
                Self::tree_leaf(
                    ui,
                    ("history", 0),
                    1,
                    icons::FOLDER,
                    &t!("files.tree.history.none"),
                    None,
                    true,
                )
                .on_hover_text(&tip);
            }
        }
    }

    /// The Files tab of the left aside: the filter, then the tree.
    ///
    /// # Rules
    /// - The design row says "Unsaved changes" while the project has changes not on disk.
    /// - Adding a file and opening a folder live in the tool bar, not here.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/rows.rs, button.rs
    ///
    /// # Do not
    /// - Show the Layers tree here.
    pub(crate) fn files_tree(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 0))
            .show(ui, |ui| {
                let hint = t!("files.tree.filter.hint");
                let response = ui
                    .add(
                        crate::targets::text_field(&mut self.files_home.filter)
                            .id(egui::Id::new("files-tree-filter"))
                            .hint_text(hint)
                            .desired_width(ui.available_width()),
                    )
                    .named(&t!("files.tree.filter.name"))
                    .on_hover_text(t!("files.tree.filter.tip"));
                drop(response);
            });
        ui.add_space(theme::SPACE[1]);
        ui.painter().hline(
            ui.max_rect().x_range(),
            ui.cursor().min.y,
            egui::Stroke::new(theme::BORDER, tokens.line),
        );
        egui::ScrollArea::vertical()
            .id_salt("files-tree-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let file = file_name(&self.path);
                if self.tree_filter_matches(&file) {
                    let (title, tip) = (t!("files.tree.design"), t!("files.files.design.tip"));
                    if self.tree_head(ui, "design", &title, &tip, 1, 0) {
                        let note = self
                            .project_has_unsaved_changes()
                            .then(|| t!("files.home.unsaved"));
                        let line = Line {
                            depth: 1,
                            chevron: None,
                            icon: Some(icons::FILE),
                            name: &file,
                            note,
                            selected: true,
                            dimmed: false,
                        };
                        let row = tree_line(ui, "files-tree-design", &line);
                        let size = match self.files_home.probes.get(&self.path) {
                            Some(p) if p.exists && p.bytes > 0 => {
                                format!("{tip} ({})", size_text(p.bytes))
                            }
                            _ => tip,
                        };
                        if row.on_hover_text(size).clicked() {
                            self.change_tab(Tab::Schematic);
                        }
                    }
                }
                let (library, attached) = self.tree_groups();
                let (title, tip) = (t!("files.tree.library"), t!("files.tree.library.tip"));
                self.tree_group_section(ui, "library", &title, &tip, &library);
                let (title, tip) = (t!("files.tree.attached"), t!("files.tree.attached.tip"));
                self.tree_group_section(ui, "attached", &title, &tip, &attached);
                self.tree_made_section(ui);
                self.tree_history_section(ui);
            });
    }

    /// The History tab of the left aside: what is recorded about earlier saves.
    pub(crate) fn files_history(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("files-history-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.tree_history_section(ui);
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> EmbeddedFile {
        EmbeddedFile {
            name: name.to_owned(),
            data: String::new(),
            encoding: String::new(),
        }
    }

    #[test]
    fn attachments_group_by_what_they_are() -> Result<(), String> {
        assert_eq!(attached_kind(&file("board.PNG")), Attached::Image);
        assert_eq!(attached_kind(&file("stm32.pdf")), Attached::Datasheet);
        assert_eq!(attached_kind(&file("notes.md")), Attached::Note);
        Ok(())
    }
}
