//! Tour playback hooks on [`crate::App`] (headless replay and future overlay).

use super::playback::{Playback, TourRecord, TourState};

impl crate::App {
    #[allow(dead_code)]
    pub(crate) fn tour_playback(&self) -> Option<&Playback> {
        self.tour_playback.as_ref()
    }

    #[allow(dead_code)]
    pub(crate) fn tour_start(&mut self, tour_id: String, version: u32) {
        let step = self
            .preferences
            .tour_replay
            .get(&tour_id)
            .filter(|r| r.version == version && r.state == TourState::InProgress)
            .map(|r| r.step)
            .unwrap_or(0);
        self.tour_playback = Some(Playback {
            tour_id,
            version,
            step,
        });
    }

    #[allow(dead_code)]
    pub(crate) fn tour_next(&mut self) {
        let Some(pb) = self.tour_playback.as_mut() else {
            return;
        };
        pb.step += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn tour_complete(&mut self) {
        let Some(pb) = self.tour_playback.take() else {
            return;
        };
        self.preferences.tour_replay.record(TourRecord {
            id: pb.tour_id,
            version: pb.version,
            state: TourState::Done,
            step: pb.step,
        });
    }

    #[allow(dead_code)]
    pub(crate) fn tour_escape(&mut self) {
        let Some(pb) = self.tour_playback.take() else {
            return;
        };
        self.preferences.tour_replay.record(TourRecord {
            id: pb.tour_id,
            version: pb.version,
            state: TourState::InProgress,
            step: pb.step,
        });
    }

    #[allow(dead_code)]
    pub(crate) fn tour_open_panel(&mut self, panel: &str) -> Result<(), String> {
        use crate::bottom_sheet::SheetTab;
        let tab = match panel {
            "findings" => SheetTab::Findings,
            "output" => SheetTab::Output,
            "console" => SheetTab::Console,
            other => return Err(format!("unknown region tab `{other}`")),
        };
        self.open_sheet(tab);
        Ok(())
    }
}
