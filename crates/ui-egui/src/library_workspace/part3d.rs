//! The load sentence of one library part's 3D picture: source, triangles,
//! bounding box and licence.
//!
//! Context: issue #359, part of #357. The Library tab's 3D page shows the
//! part's model file when it has one and the generated body otherwise, in a
//! viewport of its own: the board's software renderer with a per-part camera
//! (`board3d::viewport` over the one-part project), the footprint pads and
//! courtyard drawn on the board plane, and angle buttons that include the
//! plain top view. This module carries the plain-sentence load result shown
//! beside that viewport. A failure shows its reason beside the part, never
//! in a popup.
//!
//! Local rules: every control carries a tooltip; the sentence is one plain
//! label that wraps when long; numbers show one decimal and name
//! millimetres once per axis; nothing here is written to the project
//! document.
//!
//! # Dependencies
//! - crates/io/src/board_mesh.rs (`BoardMesh::triangle_count`, `BoardMesh::bounds`)
//! - crates/ui-egui/src/library_workspace/model.rs (the viewport this sits under)
//! - crates/ui-egui/src/theme.rs (`tokens`, for the caption colour)
//! - crates/ui-egui/src/fonts.rs (`caption`, the label style)
//!
//! # Do not
//! - Fetch model files here; the viewport's model cache already owns them.
//! - Add a second viewport; this page has exactly one.
use super::*;

/// Where the part's picture came from.
///
/// # Rules
/// - `Generated` is the fallback whenever a model file is absent or failed.
/// - `None` is only for a mesh that could not be built at all.
///
/// # Dependencies
/// - crates/io/src/board_mesh.rs (`ModelStatus`, which decides the variant)
///
/// # Do not
/// - Invent a source the loader cannot actually produce.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum ModelSource {
    /// A model file named by the footprint, read from disk.
    File,
    /// The body generated from the footprint's pads and courtyard.
    #[default]
    Generated,
    /// Nothing was available, and the reason sits next to it.
    None,
}

/// What the last 3D load of the showing part produced, kept to be read out
/// in one plain sentence beside the viewport.
///
/// # Rules
/// - `bounds` is the mesh bounding box in board millimetres, all zero when
///   nothing loaded.
/// - `failure` carries words only when a load failed; a success clears it.
///
/// # Dependencies
/// - crates/io/src/board_mesh.rs (the mesh the counts and box came from)
///
/// # Do not
/// - Keep the note across part switches without recording the new load.
#[derive(Clone, Debug, Default)]
pub(crate) struct Part3d {
    source: ModelSource,
    triangles: usize,
    bounds: [[f64; 3]; 2],
    licence: String,
    failure: Option<String>,
}

impl Part3d {
    /// Record what the last load produced, in the words the page shows.
    ///
    /// # Rules
    /// - `low` and `high` are the mesh bounding box in board millimetres.
    /// - `failure` is a plain sentence naming why a file load failed, or why
    ///   nothing is shown; it is kept only alongside a failing load.
    ///
    /// # Dependencies
    /// - crates/io/src/board_mesh.rs (`BoardMesh::triangle_count`, `BoardMesh::bounds`)
    ///
    /// # Do not
    /// - Keep the previous failure once a load succeeded.
    pub(crate) fn set_load_result(
        &mut self,
        source: ModelSource,
        triangles: usize,
        low: [f64; 3],
        high: [f64; 3],
        licence: &str,
        failure: Option<&str>,
    ) {
        self.source = source;
        self.triangles = triangles;
        self.bounds = [low, high];
        self.licence = licence.trim().to_owned();
        self.failure = failure.map(str::to_owned);
    }

    /// The plain-sentence load result: source, triangle count, bounding box
    /// and licence line; or, with nothing to show, the failure reason.
    ///
    /// # Rules
    /// - Numbers show one decimal and millimetres named once per axis.
    /// - An unstated licence says so in plain words.
    ///
    /// # Dependencies
    /// - crates/io/src/board_mesh.rs (the mesh the counts came from)
    ///
    /// # Do not
    /// - Wrap the sentence in markup; a screen reader reads it as it comes.
    pub(crate) fn load_line(&self) -> String {
        if self.source == ModelSource::None {
            let why = self.failure.clone().unwrap_or_else(|| {
                "this part has no model file and no body could be generated".to_owned()
            });
            return format!("No model: {why}.");
        }
        let from = match (&self.source, &self.failure) {
            (ModelSource::File, _) => "Model file loaded".to_owned(),
            (ModelSource::Generated, Some(why)) => {
                format!("Model file did not load: {why}; generated body shown")
            }
            _ => "Generated body shown".to_owned(),
        };
        let [lo, hi] = self.bounds;
        let licence = if self.licence.is_empty() {
            "Licence: none stated.".to_owned()
        } else {
            format!("Licence: {}.", self.licence)
        };
        format!(
            "{from}. {} triangles. Bounding box x {:.1} to {:.1} mm, y {:.1} to {:.1} mm, z {:.1} to {:.1} mm. {licence}",
            self.triangles, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
        )
    }

    /// Paint the load sentence as one selectable label beside the viewport,
    /// with a tooltip explaining what it lists.
    ///
    /// # Rules
    /// - The sentence wraps when long; it is never truncated or hidden.
    /// - The label is caption-styled and muted, matching the rest of the page.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/theme.rs (`tokens`, for the caption colour)
    /// - crates/ui-egui/src/fonts.rs (`caption`, the label style)
    ///
    /// # Do not
    /// - Put the sentence in a popup or a collapsing box.
    pub(crate) fn under_viewport(&self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        ui.label(fonts::caption(self.load_line()).color(t.muted))
            .on_hover_text("How the picture was made: source, triangles, size and licence.");
    }
}
