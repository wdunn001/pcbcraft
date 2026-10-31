//! Tour progress in preferences and the in-memory playback session.

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

/// All tour progress rows; stored inside [`crate::Preferences`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TourProgress {
    #[serde(default)]
    pub tours: Vec<TourRecord>,
}

impl TourProgress {
    pub fn record(&mut self, row: TourRecord) {
        if let Some(existing) = self.tours.iter_mut().find(|r| r.id == row.id) {
            *existing = row;
        } else {
            self.tours.push(row);
        }
    }

    pub fn get(&self, id: &str) -> Option<&TourRecord> {
        self.tours.iter().find(|r| r.id == id)
    }
}

/// Active tour while steps are shown (not persisted until skip, Esc or finish).
#[derive(Debug, Clone)]
pub(crate) struct Playback {
    pub tour_id: String,
    pub version: u32,
    pub step: usize,
}

impl Playback {
    pub fn step_index(&self) -> usize {
        self.step
    }
}
