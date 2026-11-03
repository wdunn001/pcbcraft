//! The left end of the status footer: the project name, a copper dot when it has unsaved
//! changes, and the plain sentence the footer shows after a command.
//!
//! Context: the app keeps one project open, so the footer is the place that names it. The
//! name rule is `display_title`; the unsaved rule is `App::project_has_unsaved_changes`
//! (the same one the close warning asks), so the dot and the warning cannot disagree.
//! Rules: the dot is a painted shape, never a text glyph; a command that succeeds says a
//! plain sentence or nothing, and never its tool name.
//!
//! # Dependencies
//! - crates/engine/src/design_document.rs (display_title, the name rule)
//! - crates/ui-egui/src/files_home.rs (project_has_unsaved_changes, the unsaved rule)
//! - crates/ui-egui/src/ui_kit/status_item.rs (the look of the neighbouring footer items)
//! - crates/text/strings/en.txt (every sentence)
//!
//! # Do not
//! - Show a tool name or a raw command in the footer.
//! - Add a tab or a second open project.

use super::*;
use std::path::Path;

/// Width of the slot that holds the unsaved dot, in points.
const DOT_SLOT: f32 = 14.0;
/// Radius of the unsaved dot, in points.
const DOT_RADIUS: f32 = 3.5;

/// The last part of a path as shown to a person, or the whole text when it has none.
fn leaf(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// The footer sentence for a command that succeeded, or `None` to stay quiet.
///
/// Why: the footer used to print "{command} · complete", which is a tool name; only the
/// commands a person asked for by name get a sentence.
///
/// # Rules
/// - Quiet is the answer for every command not listed here.
/// - Paths show as file or folder names the person typed or chose.
///
/// # Dependencies
/// - crates/text/strings/en.txt (the `status.done.*` sentences)
///
/// # Do not
/// - Return the command name or an id.
pub(crate) fn done_sentence(command: &str, args: &Value) -> Option<String> {
    let text = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    match command {
        "new_design" => Some(t!("status.done.new_design")),
        "sample_design" => Some(t!("status.done.sample_design")),
        "open" => text("path").map(|p| t!("status.done.open", name = leaf(p))),
        "save" => text("path").map(|p| t!("status.done.save", file = leaf(p))),
        "undo" => Some(t!("status.done.undo")),
        "redo" => Some(t!("status.done.redo")),
        "export" | "gerber.files" | "package.export" => {
            let folder = text("directory").map(str::to_owned).or_else(|| {
                text("path").map(|p| {
                    Path::new(p)
                        .parent()
                        .map_or_else(|| p.to_owned(), |d| d.to_string_lossy().into_owned())
                })
            })?;
            Some(t!("status.done.files_made", folder = folder))
        }
        _ => None,
    }
}

impl App {
    /// The name the footer shows for the open project.
    pub(crate) fn footer_project_name(&self) -> String {
        pcbcraft_engine::design_document::display_title(&self.project.name, &self.path)
    }

    /// Paint the project name and, when there are unsaved changes, the copper dot.
    ///
    /// Why: the person should see which project is open and whether closing it would lose
    /// work, without opening the Files tab.
    ///
    /// # Rules
    /// - The whole indicator has the id `footer-project`; the dot adds a tooltip.
    ///
    /// # Do not
    /// - Draw the dot as a text character.
    pub(crate) fn footer_project(&self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let name = self.footer_project_name();
        let dirty = self.project_has_unsaved_changes();
        ui.push_id("footer-project", |ui| {
            ui.add_space(10.0);
            ui.add(egui::Label::new(fonts::caption(&name).color(tokens.text)).selectable(false))
                .on_hover_text(t!("status.project.name.tip"));
            if dirty {
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(DOT_SLOT, theme::STATUS_HEIGHT),
                    egui::Sense::hover(),
                );
                ui.painter()
                    .circle_filled(rect.center(), DOT_RADIUS, tokens.copper);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Label,
                        true,
                        t!("status.project.unsaved"),
                    )
                });
                response.on_hover_text(t!("status.project.unsaved.tip"));
            } else {
                ui.add_space(DOT_SLOT);
            }
        });
    }
}
