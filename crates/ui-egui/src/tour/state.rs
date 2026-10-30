//! Tour progress, triggers and the in-session state machine (#773).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

const TAB_QUIET: Duration = Duration::from_secs(3);

/// Where a tour expects the person to be working.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkspaceId {
    Files,
    Schematic,
    Layout,
    Manufacture,
}

impl WorkspaceId {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "files" => Some(Self::Files),
            "schematic" => Some(Self::Schematic),
            "layout" => Some(Self::Layout),
            "manufacture" => Some(Self::Manufacture),
            _ => None,
        }
    }

    pub fn from_tab(tab: crate::Tab) -> Option<Self> {
        match tab {
            crate::Tab::Files => Some(Self::Files),
            crate::Tab::Schematic => Some(Self::Schematic),
            crate::Tab::Pcb | crate::Tab::Board3d => Some(Self::Layout),
            crate::Tab::PlotFiles => Some(Self::Manufacture),
            _ => None,
        }
    }
}

/// When an unprompted offer may appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    FirstLaunch,
    FirstVisit,
    Manual,
    Hint,
}

impl Trigger {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "first_launch" => Some(Self::FirstLaunch),
            "first_visit" => Some(Self::FirstVisit),
            "manual" => Some(Self::Manual),
            "hint" => Some(Self::Hint),
            _ => None,
        }
    }
}

/// Static definition the controller keeps for one tour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TourMeta {
    pub id: String,
    pub version: u32,
    pub trigger: Trigger,
    pub workspace: WorkspaceId,
    pub step_count: usize,
}

impl TourMeta {
    pub fn list_key(&self) -> String {
        format!("tour.{}.list", self.id)
    }

    pub fn chip_label_key(&self) -> String {
        format!("tour.{}.chip", self.id)
    }
}

/// Persisted per tour in Preferences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TourRecord {
    pub version: u32,
    pub state: PersistedState,
    #[serde(default)]
    pub step: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersistedState {
    Done,
    Skipped,
    InProgress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TourEvent {
    Started { id: String, step: u32 },
    StepShown { id: String, step: u32 },
    Advanced { id: String, step: u32 },
    Skipped { id: String, step: u32 },
    Completed { id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Start,
    Resume,
    Restart,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuRow {
    pub id: String,
    pub list_key: String,
    pub action: MenuAction,
    pub detail_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferKind {
    FirstLaunchCard,
    TabChip,
    VersionChip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingOffer {
    pub tour_id: String,
    pub kind: OfferKind,
}

#[derive(Clone, Debug, Default)]
pub struct Session {
    pub tab_changed_at: Option<Instant>,
    pub visits: HashSet<WorkspaceId>,
    pub unprompted_offered: HashSet<String>,
    pub chip_dismissed: HashSet<String>,
    pub version_chip_shown: HashSet<String>,
    pub first_launch_card_seen: bool,
    pub pending: Option<PendingOffer>,
    pub active_id: Option<String>,
    pub active_step: u32,
    pub events: Vec<TourEvent>,
}

/// Shell facts the trigger logic needs without importing all of `App`.
pub struct ShellView<'a> {
    pub offer_tours: bool,
    pub modal_open: bool,
    pub file_busy: bool,
    pub tab: crate::Tab,
    pub workspace: Option<WorkspaceId>,
    pub progress: &'a BTreeMap<String, TourRecord>,
    pub first_launch_done: bool,
    pub tab_changed_at: Option<Instant>,
}

pub struct Controller {
    catalog: Vec<TourMeta>,
    session: Session,
}

impl Controller {
    pub fn new(catalog: Vec<TourMeta>) -> Self {
        Self {
            catalog,
            session: Session::default(),
        }
    }

    pub(crate) fn meta(&self, id: &str) -> Option<&TourMeta> {
        self.catalog.iter().find(|t| t.id == id)
    }

    pub(crate) fn hint_tours(&self) -> impl Iterator<Item = &TourMeta> {
        self.catalog
            .iter()
            .filter(|t| t.trigger == Trigger::Hint)
    }

    #[cfg(test)]
    pub fn catalog(&self) -> &[TourMeta] {
        &self.catalog
    }

    pub fn take_events(&mut self) -> Vec<TourEvent> {
        std::mem::take(&mut self.session.events)
    }

    pub fn active(&self) -> Option<(&TourMeta, u32)> {
        let id = self.session.active_id.as_ref()?;
        let meta = self.catalog.iter().find(|t| &t.id == id)?;
        Some((meta, self.session.active_step))
    }

    pub fn pending_offer(&self) -> Option<&PendingOffer> {
        self.session.pending.as_ref()
    }

    pub fn dismiss_chip(&mut self, tour_id: &str) {
        self.session.chip_dismissed.insert(tour_id.to_owned());
        self.session.pending = None;
    }

    pub fn dismiss_first_launch_card(&mut self) {
        self.session.first_launch_card_seen = true;
        self.session.pending = None;
    }

    pub(crate) fn first_launch_handled(&self) -> bool {
        self.session.first_launch_card_seen
    }

    #[cfg(test)]
    pub fn test_quiet_tab_changes(&mut self) {
        self.session.tab_changed_at =
            Some(Instant::now() - TAB_QUIET - Duration::from_secs(1));
    }

    pub fn note_tab_change(&mut self, now: Instant) {
        self.session.tab_changed_at = Some(now);
        self.session.chip_dismissed.clear();
    }

    pub(crate) fn tab_changed_at(&self) -> Option<Instant> {
        self.session.tab_changed_at
    }

    pub fn note_workspace_visit(&mut self, workspace: WorkspaceId) {
        self.session.visits.insert(workspace);
    }

    pub fn menu_rows(&self, progress: &BTreeMap<String, TourRecord>) -> Vec<MenuRow> {
        let mut rows = Vec::new();
        for meta in &self.catalog {
            let (action, detail_key) = match progress.get(&meta.id) {
                Some(rec) if rec.state == PersistedState::InProgress => {
                    (MenuAction::Resume, "tour.menu.resume_at".into())
                }
                Some(rec)
                    if rec.state == PersistedState::Done || rec.state == PersistedState::Skipped =>
                {
                    (MenuAction::Restart, "tour.menu.restart".into())
                }
                _ => (MenuAction::Start, "tour.menu.start".into()),
            };
            rows.push(MenuRow {
                id: meta.id.clone(),
                list_key: meta.list_key(),
                action,
                detail_key,
            });
        }
        rows
    }

    pub fn command_entries(&self) -> Vec<(String, String)> {
        self.catalog
            .iter()
            .filter(|t| t.trigger == Trigger::Manual || t.trigger == Trigger::Hint)
            .map(|t| (format!("tour:{}", t.id), t.list_key()))
            .collect()
    }

    pub fn sync_triggers(&mut self, view: ShellView<'_>, now: Instant) {
        if self.session.active_id.is_some() || self.session.pending.is_some() {
            return;
        }
        if !view.offer_tours || !unprompted_allowed(view, now) {
            return;
        }
        for meta in self.catalog.clone() {
            if self.session.unprompted_offered.contains(&meta.id) {
                continue;
            }
            if version_needs_chip(meta.version, view.progress.get(&meta.id))
                && !self.session.version_chip_shown.contains(&meta.id)
            {
                self.session.pending = Some(PendingOffer {
                    tour_id: meta.id.clone(),
                    kind: OfferKind::VersionChip,
                });
                return;
            }
            match meta.trigger {
                Trigger::FirstLaunch if !view.first_launch_done && !self.session.first_launch_card_seen => {
                    self.session.pending = Some(PendingOffer {
                        tour_id: meta.id.clone(),
                        kind: OfferKind::FirstLaunchCard,
                    });
                    return;
                }
                Trigger::FirstVisit => {
                    let Some(ws) = view.workspace else {
                        continue;
                    };
                    if meta.workspace != ws || !self.session.visits.contains(&ws) {
                        continue;
                    }
                    if self.session.chip_dismissed.contains(&meta.id) {
                        continue;
                    }
                    if tour_finished(view.progress.get(&meta.id), meta.version) {
                        continue;
                    }
                    self.session.pending = Some(PendingOffer {
                        tour_id: meta.id.clone(),
                        kind: OfferKind::TabChip,
                    });
                    return;
                }
                Trigger::FirstLaunch | Trigger::Manual | Trigger::Hint => {}
            }
        }
    }

    pub fn accept_offer(&mut self, progress: &mut BTreeMap<String, TourRecord>, tour_id: &str) {
        let Some(meta) = self.catalog.iter().find(|t| t.id == tour_id).cloned() else {
            return;
        };
        let kind = self.session.pending.as_ref().map(|p| p.kind);
        self.session.pending = None;
        if matches!(kind, Some(OfferKind::FirstLaunchCard)) {
            self.session.first_launch_card_seen = true;
        }
        if matches!(kind, Some(OfferKind::VersionChip)) {
            self.session.version_chip_shown.insert(tour_id.to_owned());
        }
        if matches!(
            kind,
            Some(OfferKind::TabChip | OfferKind::VersionChip | OfferKind::FirstLaunchCard)
        ) {
            self.session.unprompted_offered.insert(tour_id.to_owned());
        }
        self.start_tour(progress, &meta, 0);
    }

    pub fn start_from_menu(
        &mut self,
        progress: &mut BTreeMap<String, TourRecord>,
        tour_id: &str,
        action: MenuAction,
    ) {
        let Some(meta) = self.catalog.iter().find(|t| t.id == tour_id).cloned() else {
            return;
        };
        let step = match action {
            MenuAction::Resume => progress
                .get(tour_id)
                .filter(|r| r.state == PersistedState::InProgress)
                .map(|r| r.step)
                .unwrap_or(0),
            MenuAction::Start | MenuAction::Restart => 0,
        };
        self.start_tour(progress, &meta, step);
    }

    /// Leave the tour for now and keep the current step for Help > Tours resume.
    pub fn skip(&mut self, progress: &mut BTreeMap<String, TourRecord>) {
        let Some(id) = self.session.active_id.clone() else {
            return;
        };
        let Some(meta) = self.catalog.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        let step = self.session.active_step;
        write_record(
            progress,
            &meta,
            PersistedState::InProgress,
            step,
        );
        self.session.events.push(TourEvent::Skipped {
            id: id.clone(),
            step,
        });
        self.session.active_id = None;
        self.session.active_step = 0;
    }

    pub fn advance(&mut self, progress: &mut BTreeMap<String, TourRecord>) {
        let Some(id) = self.session.active_id.clone() else {
            return;
        };
        let Some(meta) = self.catalog.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        let step = self.session.active_step;
        if step + 1 >= meta.step_count as u32 {
            write_record(progress, &meta, PersistedState::Done, step);
            self.session.events.push(TourEvent::Completed { id });
            self.session.active_id = None;
            self.session.active_step = 0;
            return;
        }
        let next = step + 1;
        write_record(
            progress,
            &meta,
            PersistedState::InProgress,
            next,
        );
        self.session.events.push(TourEvent::Advanced {
            id,
            step: next,
        });
        self.session.active_step = next;
        self.session.events.push(TourEvent::StepShown {
            id: self.session.active_id.clone().unwrap_or_default(),
            step: next,
        });
    }

    pub fn back(&mut self, progress: &mut BTreeMap<String, TourRecord>) {
        let Some(id) = self.session.active_id.clone() else {
            return;
        };
        let Some(meta) = self.catalog.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        let step = self.session.active_step.saturating_sub(1);
        write_record(
            progress,
            &meta,
            PersistedState::InProgress,
            step,
        );
        self.session.active_step = step;
        self.session.events.push(TourEvent::StepShown {
            id,
            step,
        });
    }

    fn start_tour(
        &mut self,
        progress: &mut BTreeMap<String, TourRecord>,
        meta: &TourMeta,
        step: u32,
    ) {
        let step = step.min(meta.step_count.saturating_sub(1) as u32);
        write_record(
            progress,
            meta,
            PersistedState::InProgress,
            step,
        );
        self.session.active_id = Some(meta.id.clone());
        self.session.active_step = step;
        self.session.events.push(TourEvent::Started {
            id: meta.id.clone(),
            step,
        });
        self.session.events.push(TourEvent::StepShown {
            id: meta.id.clone(),
            step,
        });
    }
}

fn unprompted_allowed(view: ShellView<'_>, now: Instant) -> bool {
    if view.modal_open || view.file_busy {
        return false;
    }
    match view.tab_changed_at {
        Some(at) if now.saturating_duration_since(at) < TAB_QUIET => false,
        _ => true,
    }
}

fn version_needs_chip(live: u32, stored: Option<&TourRecord>) -> bool {
    match stored {
        None => false,
        Some(rec) => rec.version < live && rec.state != PersistedState::InProgress,
    }
}

fn tour_finished(stored: Option<&TourRecord>, live_version: u32) -> bool {
    match stored {
        None => false,
        Some(rec) if rec.version < live_version => false,
        Some(rec) => matches!(rec.state, PersistedState::Done | PersistedState::Skipped),
    }
}

fn write_record(
    progress: &mut BTreeMap<String, TourRecord>,
    meta: &TourMeta,
    state: PersistedState,
    step: u32,
) {
    progress.insert(
        meta.id.clone(),
        TourRecord {
            version: meta.version,
            state,
            step,
        },
    );
}

#[cfg(test)]
pub fn e2e_fixture_catalog() -> Vec<TourMeta> {
    vec![TourMeta {
        id: "e2e_resume".into(),
        version: 1,
        trigger: Trigger::Manual,
        workspace: WorkspaceId::Layout,
        step_count: 4,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell<'a>(
        progress: &'a BTreeMap<String, TourRecord>,
        offer: bool,
    ) -> ShellView<'a> {
        ShellView {
            offer_tours: offer,
            modal_open: false,
            file_busy: false,
            tab: crate::Tab::Pcb,
            workspace: Some(WorkspaceId::Layout),
            progress,
            first_launch_done: false,
            tab_changed_at: Some(Instant::now() - TAB_QUIET - Duration::from_millis(1)),
        }
    }

    #[test]
    fn skip_at_step_three_resumes_from_help() -> Result<(), String> {
        let catalog = e2e_fixture_catalog();
        let mut progress = BTreeMap::new();
        let mut ctrl = Controller::new(catalog);
        ctrl.start_from_menu(&mut progress, "e2e_resume", MenuAction::Start);
        ctrl.session.active_step = 2;
        write_record(
            &mut progress,
            ctrl.catalog().first().unwrap(),
            PersistedState::InProgress,
            2,
        );
        ctrl.skip(&mut progress);
        let rec = progress.get("e2e_resume").ok_or("missing record")?;
        assert_eq!(rec.state, PersistedState::InProgress);
        assert_eq!(rec.step, 2);
        let rows = ctrl.menu_rows(&progress);
        assert_eq!(rows[0].action, MenuAction::Resume);
        Ok(())
    }

    #[test]
    fn offer_tours_off_blocks_unprompted_offers() -> Result<(), String> {
        let catalog = vec![TourMeta {
            id: "visit".into(),
            version: 1,
            trigger: Trigger::FirstVisit,
            workspace: WorkspaceId::Layout,
            step_count: 3,
        }];
        let progress = BTreeMap::new();
        let mut ctrl = Controller::new(catalog);
        ctrl.session.visits.insert(WorkspaceId::Layout);
        let mut view = shell(&progress, false);
        view.tab_changed_at = None;
        ctrl.sync_triggers(view, Instant::now());
        assert!(ctrl.pending_offer().is_none());
        view.offer_tours = true;
        ctrl.sync_triggers(view, Instant::now());
        assert!(ctrl.pending_offer().is_some());
        Ok(())
    }

    #[test]
    fn bumped_version_shows_chip_once_per_session() -> Result<(), String> {
        let mut catalog = e2e_fixture_catalog();
        catalog[0].version = 2;
        catalog[0].trigger = Trigger::FirstVisit;
        let mut progress = BTreeMap::new();
        progress.insert(
            "e2e_resume".into(),
            TourRecord {
                version: 1,
                state: PersistedState::Done,
                step: 3,
            },
        );
        let mut ctrl = Controller::new(catalog);
        ctrl.session.visits.insert(WorkspaceId::Layout);
        let view = ShellView {
            offer_tours: true,
            modal_open: false,
            file_busy: false,
            tab: crate::Tab::Pcb,
            workspace: Some(WorkspaceId::Layout),
            progress: &progress,
            first_launch_done: true,
            tab_changed_at: Some(Instant::now() - TAB_QUIET - Duration::from_millis(1)),
        };
        ctrl.sync_triggers(view, Instant::now());
        let offer = ctrl.pending_offer().ok_or("expected version chip")?;
        assert_eq!(offer.kind, OfferKind::VersionChip);
        ctrl.session.version_chip_shown.insert("e2e_resume".into());
        ctrl.session.pending = None;
        ctrl.sync_triggers(view, Instant::now());
        assert!(ctrl.pending_offer().is_none());
        Ok(())
    }

    #[test]
    fn tour_progress_survives_preferences_round_trip() -> Result<(), String> {
        let mut prefs = crate::Preferences::default();
        prefs.tour_progress.insert(
            "e2e_resume".into(),
            TourRecord {
                version: 1,
                state: PersistedState::InProgress,
                step: 2,
            },
        );
        prefs.offer_tours = false;
        let text = serde_json::to_string(&prefs).map_err(|e| e.to_string())?;
        let back: crate::Preferences = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert!(!back.offer_tours);
        assert_eq!(
            back.tour_progress.get("e2e_resume").map(|r| r.step),
            Some(2)
        );
        Ok(())
    }
}
