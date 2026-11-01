//! Headless replay progress (#774): saved rows kept in preferences and the in-memory playback session.
//! The shell controller (#773) lives in `state.rs`.

use serde::{Deserialize, Serialize};

/// Saved progress for one tour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TourRecord {
    pub id: String,
    pub version: u32,
    pub state: TourState,
    pub step: usize,
}

/// Whether the user finished, skipped, or paused mid-way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TourState {
    Done,
    Skipped,
    InProgress,
}

/// All tour progress rows; stored inside [`crate::Preferences`] as `tour_replay`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TourProgress {
    #[serde(default)]
    pub tours: Vec<TourRecord>,
}

impl TourProgress {
    #[allow(dead_code)]
    pub fn record(&mut self, row: TourRecord) {
        if let Some(existing) = self.tours.iter_mut().find(|r| r.id == row.id) {
            *existing = row;
        } else {
            self.tours.push(row);
        }
    }

    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Option<&TourRecord> {
        self.tours.iter().find(|r| r.id == id)
    }
}

/// Active tour while steps are shown (not persisted until skip, Esc or finish).
#[derive(Debug, Clone)]
pub(crate) struct Playback {
    #[allow(dead_code)]
    pub tour_id: String,
    #[allow(dead_code)]
    pub version: u32,
    pub step: usize,
}

impl Playback {
    #[allow(dead_code)]
    pub fn step_index(&self) -> usize {
        self.step
    }
}
