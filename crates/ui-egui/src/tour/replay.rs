//! Catalog gates and preferences round-trip for tour progress (#774).

use super::data::Tour;
use super::state::{TourProgress, TourRecord, TourState};
use pcbcraft_text::lookup;

/// Gate: every step title and body key resolves in the English catalog.
pub fn gate_catalog_labels(tours: &[Tour]) -> Result<(), String> {
    let keys = catalog_keys();
    for tour in tours {
        require_label(&keys, &tour.list_key(), &format!("tour `{}` list", tour.id))?;
        for step in &tour.steps {
            let ctx = format!("tour `{}` step `{}`", tour.id, step.id);
            require_label(&keys, &step.title, &format!("{ctx} title"))?;
            require_label(&keys, &step.body, &format!("{ctx} body"))?;
        }
    }
    Ok(())
}

fn catalog_keys() -> std::collections::BTreeSet<String> {
    pcbcraft_text::parse(pcbcraft_text::ENGLISH)
        .0
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

fn require_label(
    keys: &std::collections::BTreeSet<String>,
    key: &str,
    label: &str,
) -> Result<(), String> {
    if !keys.contains(key) {
        return Err(format!("{label}: catalog has no key `{key}`"));
    }
    let text = lookup(key, &[]);
    if text == key {
        return Err(format!("{label}: `{key}` does not resolve to user text"));
    }
    Ok(())
}

/// Tour progress serializes with preferences (browser storage uses the same JSON).
pub fn tour_progress_round_trip() -> Result<(), String> {
    let mut progress = TourProgress::default();
    progress.record(TourRecord {
        id: "demo".into(),
        version: 2,
        state: TourState::InProgress,
        step: 2,
    });
    let text = serde_json::to_string(&progress).map_err(|e| e.to_string())?;
    let back: TourProgress = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.tours.len(), 1);
    assert_eq!(back.tours[0].step, 2);
    Ok(())
}
