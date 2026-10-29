//! Panel grid settings and layout math shared by preview and fabrication export.

use crate::{Board, Project};
use serde::{Deserialize, Serialize};

/// How neighbouring copies are separated on the manufacturing panel.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PanelSeparation {
    #[default]
    Tab,
    VScore,
}

/// Panel grid stored on the project; disabled until the designer turns it on.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PanelSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_panel_columns")]
    pub columns: usize,
    #[serde(default = "default_panel_rows")]
    pub rows: usize,
    #[serde(default)]
    pub pitch_x: f64,
    #[serde(default)]
    pub pitch_y: f64,
    #[serde(default = "default_panel_rail")]
    pub rail: f64,
    #[serde(default)]
    pub separation: PanelSeparation,
    #[serde(default = "default_tab_channel")]
    pub tab_channel: f64,
}

fn default_panel_columns() -> usize {
    1
}
fn default_panel_rows() -> usize {
    1
}
fn default_panel_rail() -> f64 {
    5.0
}
fn default_tab_channel() -> f64 {
    1.8
}

impl Default for PanelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            columns: 1,
            rows: 1,
            pitch_x: 0.0,
            pitch_y: 0.0,
            rail: default_panel_rail(),
            separation: PanelSeparation::Tab,
            tab_channel: default_tab_channel(),
        }
    }
}

impl PanelSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_fields()
    }

    pub fn validate_fields(&self) -> Result<(), String> {
        if self.columns < 1
            || self.rows < 1
            || self.columns > 32
            || self.rows > 32
            || self.columns * self.rows > 1024
        {
            return Err("panels run 1 to 32 copies per side and at most 1024 in total".into());
        }
        if !self.pitch_x.is_finite()
            || !self.pitch_y.is_finite()
            || self.pitch_x < 0.0
            || self.pitch_y < 0.0
        {
            return Err("gaps between copies cannot be negative".into());
        }
        if !(0.0..=25.0).contains(&self.rail) {
            return Err("rails must be 0 to 25 millimetres".into());
        }
        if !(1.6..=2.0).contains(&self.tab_channel) {
            return Err("tab channel width must be 1.6 to 2.0 millimetres".into());
        }
        Ok(())
    }
}

/// One copy's placement on the panel, origins in millimetres from the panel corner.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelCopy {
    pub row: usize,
    pub column: usize,
    /// Row-major copy index starting at 1.
    pub index: usize,
    pub x: f64,
    pub y: f64,
    pub center_x: f64,
    pub center_y: f64,
}

/// Full panel dimensions and every copy origin.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelLayout {
    pub columns: usize,
    pub rows: usize,
    pub board_width: f64,
    pub board_height: f64,
    pub pitch_x: f64,
    pub pitch_y: f64,
    pub rail: f64,
    pub panel_width: f64,
    pub panel_height: f64,
    pub copies: Vec<PanelCopy>,
}

pub fn board_copy_size(board: &Board) -> Result<(f64, f64), String> {
    match board.outline.as_deref() {
        Some(outline) if outline.len() >= 3 => {
            let min_x = outline.iter().map(|v| v.x).fold(f64::INFINITY, f64::min);
            let max_x = outline
                .iter()
                .map(|v| v.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = outline.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
            let max_y = outline
                .iter()
                .map(|v| v.y)
                .fold(f64::NEG_INFINITY, f64::max);
            Ok((max_x - min_x, max_y - min_y))
        }
        _ => Ok((board.width, board.height)),
    }
}

/// Grid parameters shared by stored settings and the read-only preview command.
#[derive(Clone, Debug)]
pub struct PanelGrid {
    pub columns: usize,
    pub rows: usize,
    pub pitch_x: f64,
    pub pitch_y: f64,
    pub rail: f64,
}

impl PanelGrid {
    pub fn validate(&self) -> Result<(), String> {
        if self.columns < 1
            || self.rows < 1
            || self.columns > 32
            || self.rows > 32
            || self.columns * self.rows > 1024
        {
            return Err("panels run 1 to 32 copies per side and at most 1024 in total".into());
        }
        if !self.pitch_x.is_finite()
            || !self.pitch_y.is_finite()
            || self.pitch_x < 0.0
            || self.pitch_y < 0.0
        {
            return Err("gaps between copies cannot be negative".into());
        }
        if !(0.0..=25.0).contains(&self.rail) {
            return Err("rails must be 0 to 25 millimetres".into());
        }
        Ok(())
    }
}

impl From<&PanelSettings> for PanelGrid {
    fn from(s: &PanelSettings) -> Self {
        Self {
            columns: s.columns,
            rows: s.rows,
            pitch_x: s.pitch_x,
            pitch_y: s.pitch_y,
            rail: s.rail,
        }
    }
}

pub fn panel_layout_from_grid(board: &Board, grid: &PanelGrid) -> Result<PanelLayout, String> {
    grid.validate()?;
    let (board_w, board_h) = board_copy_size(board)?;
    if !board_w.is_finite() || !board_h.is_finite() || board_w <= 0.0 || board_h <= 0.0 {
        return Err("the board has no size to panelise".into());
    }
    let panel_w =
        grid.columns as f64 * board_w + (grid.columns - 1) as f64 * grid.pitch_x + 2.0 * grid.rail;
    let panel_h =
        grid.rows as f64 * board_h + (grid.rows - 1) as f64 * grid.pitch_y + 2.0 * grid.rail;
    let mut copies = Vec::with_capacity(grid.columns * grid.rows);
    let mut index = 0usize;
    for row in 0..grid.rows {
        for column in 0..grid.columns {
            index += 1;
            let x = grid.rail + column as f64 * (board_w + grid.pitch_x);
            let y = grid.rail + row as f64 * (board_h + grid.pitch_y);
            copies.push(PanelCopy {
                row,
                column,
                index,
                x,
                y,
                center_x: x + board_w / 2.0,
                center_y: y + board_h / 2.0,
            });
        }
    }
    Ok(PanelLayout {
        columns: grid.columns,
        rows: grid.rows,
        board_width: board_w,
        board_height: board_h,
        pitch_x: grid.pitch_x,
        pitch_y: grid.pitch_y,
        rail: grid.rail,
        panel_width: panel_w,
        panel_height: panel_h,
        copies,
    })
}

/// Layout for the panel settings stored on the project; requires `enabled`.
pub fn panel_layout_with_settings(
    board: &Board,
    settings: &PanelSettings,
) -> Result<PanelLayout, String> {
    let grid = PanelGrid::from(settings);
    grid.validate()?;
    panel_layout_from_grid(board, &grid)
}

pub fn panel_layout(p: &Project) -> Result<PanelLayout, String> {
    if !p.panel.enabled {
        return Err("panel layout is disabled".into());
    }
    panel_layout_with_settings(&p.board, &p.panel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Project;

    #[test]
    fn panel_layout_matches_board_panel_json() -> Result<(), String> {
        let mut p = Project::default();
        p.panel.enabled = true;
        p.panel.columns = 2;
        p.panel.rows = 2;
        p.panel.pitch_x = 60.0;
        p.panel.pitch_y = 40.0;
        p.panel.rail = 3.0;
        let layout = panel_layout(&p)?;
        assert_eq!(layout.copies.len(), 4);
        assert_eq!(layout.board_width, 60.0);
        assert_eq!(layout.panel_width, 186.0);
        assert_eq!(layout.panel_height, 126.0);
        assert_eq!(layout.copies[0].x, 3.0);
        assert_eq!(layout.copies[3].x, 123.0);
        assert_eq!(layout.copies[3].y, 83.0);
        assert_eq!(layout.copies[3].center_x, 153.0);
        Ok(())
    }
}
