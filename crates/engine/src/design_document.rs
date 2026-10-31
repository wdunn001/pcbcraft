//! Rules shared by the desktop shell and MCP bridge for open design tabs.
use pcbcraft_doc::{Id, Project};
use std::path::Path;

/// Same cap as recent designs: enough boards for compare-and-copy workflows.
pub const MAX_OPEN_DOCUMENTS: usize = 8;

const UNTITLED_TAB: &str = "Untitled design";
const DEFAULT_PROJECT_NAME: &str = "Untitled circuit";

/// Tab label: prefer a renamed project, then the file name, then a plain untitled label.
pub fn display_title(project_name: &str, path: &str) -> String {
    let trimmed = project_name.trim();
    if !trimmed.is_empty() && trimmed != DEFAULT_PROJECT_NAME {
        return trimmed.to_owned();
    }
    if !path.is_empty()
        && let Some(name) = Path::new(path).file_name().and_then(|s| s.to_str())
    {
        return name.to_owned();
    }
    UNTITLED_TAB.to_owned()
}

/// True when the project has schematic or board content beyond a blank start.
pub fn project_has_content(project: &Project) -> bool {
    !project.components.is_empty()
        || !project.nets.is_empty()
        || !project.board.footprints.is_empty()
}

/// Whether edits since the last successful save marker should show as unsaved.
pub fn is_dirty(
    saved_revision: Option<u64>,
    revision: u64,
    path: &str,
    project_nonempty: bool,
) -> bool {
    if saved_revision != Some(revision) {
        return true;
    }
    path.is_empty()
        && saved_revision.is_none()
        && (revision > 0 || project_nonempty)
}

/// Workbench view tab for one open design (mirrors the desktop `Tab` without UI deps).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WorkbenchTab {
    Files,
    #[default]
    Schematic,
    Pcb,
    Board3d,
    Rules,
    Checks,
    Settings,
    Libraries,
    PlotFiles,
}

impl WorkbenchTab {
    /// Stable snake-case name for agents and the control bridge.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Schematic => "schematic",
            Self::Pcb => "pcb",
            Self::Board3d => "board3d",
            Self::Rules => "rules",
            Self::Checks => "checks",
            Self::Settings => "settings",
            Self::Libraries => "libraries",
            Self::PlotFiles => "plot_files",
        }
    }
}

/// Selection and workbench view that must not leak when switching design tabs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DocumentShellState {
    pub workbench_tab: WorkbenchTab,
    pub selected: Option<Id>,
    pub selected_unit: u32,
}

/// One bounded design slot: path, save marker, and per-document shell state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignDocument {
    pub path: String,
    pub saved_revision: Option<u64>,
    pub shell: DocumentShellState,
}

/// Up to [`MAX_OPEN_DOCUMENTS`] designs with a single active index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSlots {
    pub documents: Vec<DesignDocument>,
    pub active: usize,
}

impl DocumentSlots {
    /// Boot with one empty untitled design, matching a fresh desktop session.
    pub fn boot_untitled(saved_revision: Option<u64>) -> Self {
        Self {
            documents: vec![DesignDocument {
                path: String::new(),
                saved_revision,
                shell: DocumentShellState::default(),
            }],
            active: 0,
        }
    }

    /// Persist the live shell into the outgoing slot and load the target slot.
    pub fn activate_document(
        &mut self,
        index: usize,
        live: &mut DocumentShellState,
    ) -> Result<(), String> {
        if self.documents.is_empty() {
            return Err("no open documents".into());
        }
        if index >= self.documents.len() {
            return Err("document index out of range".into());
        }
        self.documents[self.active].shell = live.clone();
        self.active = index;
        *live = self.documents[index].shell.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Project;

    #[test]
    fn document_dirty_flag_clears_after_successful_save() {
        let path = "bench.pcbcraft";
        let mut saved = Some(0_u64);
        let revision = 1_u64;
        assert!(is_dirty(saved, revision, path, false));
        saved = Some(revision);
        assert!(!is_dirty(saved, revision, path, false));
    }

    #[test]
    fn document_title_prefers_project_name_over_file_name() {
        assert_eq!(
            display_title("Bench supply", "ignored.pcbcraft"),
            "Bench supply"
        );
    }

    #[test]
    fn document_title_uses_file_name_for_default_project_name() {
        assert_eq!(
            display_title(DEFAULT_PROJECT_NAME, "/tmp/filter.pcbcraft"),
            "filter.pcbcraft"
        );
    }

    #[test]
    fn untitled_new_design_is_dirty_after_first_edit() {
        assert!(!is_dirty(Some(0), 0, "", false));
        assert!(is_dirty(Some(0), 1, "", false));
    }

    #[test]
    fn untitled_new_design_with_content_is_dirty_before_save_marker() {
        let mut project = Project::default();
        assert!(!project_has_content(&project));
        project.nets.push(pcbcraft_doc::Net {
            id: pcbcraft_doc::Id(1),
            name: "GND".into(),
            target_length: None,
            length_tolerance: None,
        });
        assert!(project_has_content(&project));
        assert!(is_dirty(None, 0, "", true));
    }

    #[test]
    fn document_switch_restores_workbench_tab_and_selection() {
        let mut slots = DocumentSlots {
            documents: vec![
                DesignDocument {
                    path: "first.pcbcraft".into(),
                    saved_revision: Some(0),
                    shell: DocumentShellState {
                        workbench_tab: WorkbenchTab::Schematic,
                        selected: Some(Id(1)),
                        selected_unit: 1,
                    },
                },
                DesignDocument {
                    path: "second.pcbcraft".into(),
                    saved_revision: Some(0),
                    shell: DocumentShellState {
                        workbench_tab: WorkbenchTab::Pcb,
                        selected: Some(Id(2)),
                        selected_unit: 2,
                    },
                },
            ],
            active: 0,
        };
        let mut live = slots.documents[0].shell.clone();
        slots.activate_document(1, &mut live).unwrap();
        assert_eq!(live.workbench_tab, WorkbenchTab::Pcb);
        assert_eq!(live.selected, Some(Id(2)));
        assert_eq!(live.selected_unit, 2);

        live.workbench_tab = WorkbenchTab::Libraries;
        live.selected = Some(Id(99));
        slots.activate_document(0, &mut live).unwrap();
        assert_eq!(live.workbench_tab, WorkbenchTab::Schematic);
        assert_eq!(live.selected, Some(Id(1)));
        assert_eq!(
            slots.documents[1].shell.workbench_tab,
            WorkbenchTab::Libraries
        );
        assert_eq!(slots.documents[1].shell.selected, Some(Id(99)));
    }
}
