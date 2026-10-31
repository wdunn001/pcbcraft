//! The "Board is behind the schematic" notice on the Layout canvas.
//!
//! Context: `project.auto_sync` (engine command `project.set_auto_sync`) is on by
//! default, so the board follows the schematic by itself. With it off, this notice
//! says when a manual `board.sync` would change something and offers to run it.
//! Local rules: the check is a dry run of `board.sync` on a copy, cached by
//! revision; it never changes the session.
//!
//! # Dependencies
//! - crates/engine/src/dry_run.rs (`Session::dry_run`)
//! - crates/ui-egui/src/workspace.rs draws it next to the view controls.
//!
//! # Do not
//! - Run the dry run every frame; it clones the project.
use super::*;

impl App {
    /// Whether a manual sync would change the board right now.
    ///
    /// # Rules
    /// - Always false while automatic sync is on.
    /// - Cached per revision.
    pub(crate) fn board_behind(&mut self) -> bool {
        if self.project.auto_sync || self.project.board_only {
            return false;
        }
        if let Some((rev, behind)) = self.sync_behind
            && rev == self.revision
        {
            return behind;
        }
        let command = pcbcraft_engine::Command {
            command: "board.sync".into(),
            params: json!({}),
        };
        let behind = self
            .session
            .lock()
            .ok()
            .and_then(|s| s.dry_run(&[command], None).ok())
            .is_some_and(|d| {
                let c = d.summary;
                !(c.added.is_empty()
                    && c.removed.is_empty()
                    && c.moved.is_empty()
                    && c.changed.is_empty()
                    && c.nets_changed == 0
                    && !c.board_changed)
            });
        self.sync_behind = Some((self.revision, behind));
        behind
    }
}
