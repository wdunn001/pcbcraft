//! Schematic sheet text placement tool.
//!
//! Allows users to add free-standing text notes to the schematic sheet.
//! The tool arms via the Add menu, waits for a click on the canvas,
//! then shows a text input field where the user types the text.
//! On Enter, the text is placed via the graphic.add engine command.

use super::*;
use crate::canvas::snap;

impl App {
    /// Handle a click while the sheet text tool is armed.
    ///
    /// # Rules
    /// - First click stores the position and opens the text input.
    /// - Empty text places nothing and disarms the tool.
    /// - Esc cancels and disarms.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (graphic.add command)
    /// - crates/ui-egui/src/canvas.rs (snap)
    ///
    /// # Do not
    /// - Use this on the board tab; it only places on the schematic.
    pub(crate) fn place_sheet_text(&mut self, pos: Point) {
        let at = snap(pos);
        // Store the position and prepare for text input.
        self.sheet_text_position = Some(at);
        self.sheet_text_input = String::new();
        // Focus the text input in the hint strip.
        // The actual input is handled in the hint strip UI.
    }

    /// Submit the sheet text at the stored position.
    ///
    /// # Rules
    /// - If no position is stored, do nothing.
    /// - If text is empty, place nothing and disarm.
    /// - Otherwise, run graphic.add with the text.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (graphic.add command)
    ///
    /// # Do not
    /// - Validate the text length here; the engine does that.
    pub(crate) fn submit_sheet_text(&mut self) {
        let Some(pos) = self.sheet_text_position else {
            return;
        };
        let text = self.sheet_text_input.trim().to_string();
        if text.is_empty() {
            // Empty text: place nothing, disarm.
            self.tool = Tool::Select;
            return;
        }
        self.run(
            "graphic.add",
            json!({
                "layer": "schematic",
                "shape": "text",
                "points": [pos],
                "text": text,
            }),
        );
        // Stay armed for another placement.
        self.sheet_text_position = None;
        self.sheet_text_input = String::new();
    }

    /// Cancel sheet text placement and disarm the tool.
    ///
    /// # Rules
    /// - Clears the stored position and input.
    /// - Sets the tool back to Select.
    pub(crate) fn cancel_sheet_text(&mut self) {
        self.sheet_text_position = None;
        self.sheet_text_input = String::new();
        self.tool = Tool::Select;
    }
}
