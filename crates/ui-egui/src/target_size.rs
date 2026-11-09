//! The 24 by 24 CSS pixel pointer-target check (WCAG 2.2, criterion 2.5.8).
//!
//! Context: `e2e/shell.rs` walks every tab, collects the interactive rectangles
//! of a frame through `e2e/support.rs` `interactive_rects`, and asks this file
//! which are too small. Test-only; the app itself keeps the rules by sizing
//! controls with `App::MIN_TARGET`.
//! Local rules: an undersized control is fixed in the UI or listed with a
//! reason in `crates/ui-egui/target-size-exemptions.txt`; the minimum is never lowered.
use egui::Rect;

/// Names of the rectangles with a side under `minimum` logical pixels.
///
/// Why: a missed tap on a tiny control is the failure criterion 2.5.8 exists to
/// stop, and only a walk of real frames finds controls nobody listed.
///
/// # Rules
/// - Output is sorted and deduplicated so the failure message is stable.
///
/// # Dependencies
/// - `crates/ui-egui/src/e2e/support.rs` `interactive_rects` supplies the input.
///
/// # Do not
/// - Pass the painted size of a control whose hit area is larger; pass the hit area.
pub(crate) fn undersized_targets(rects: &[(String, Rect)], minimum: f32) -> Vec<String> {
    let mut out: Vec<String> = rects
        .iter()
        .filter(|(_, r)| r.width() < minimum - 0.01 || r.height() < minimum - 0.01)
        .map(|(name, r)| format!("{name} ({:.0} by {:.0})", r.width(), r.height()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Control names that are allowed to stay small, each with its reason.
///
/// # Rules
/// - A line is `control id = reason`; a missing reason is an error.
///
/// # Dependencies
/// - `crates/ui-egui/target-size-exemptions.txt` holds the list.
///
/// # Do not
/// - Match by prefix; an exemption names one control.
pub(crate) fn exemptions() -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for line in include_str!("../target-size-exemptions.txt").lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, reason) = line
            .split_once('=')
            .ok_or_else(|| format!("exemption without a reason: {line}"))?;
        if reason.trim().is_empty() {
            return Err(format!("exemption without a reason: {line}"));
        }
        out.push((name.trim().to_string(), reason.trim().to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    #[test]
    fn a_target_exactly_24_by_24_passes_and_23_fails() {
        let ok = Rect::from_min_size(pos2(0.0, 0.0), vec2(24.0, 24.0));
        let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(23.0, 40.0));
        let short = Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 23.0));
        let list = vec![
            ("ok".to_string(), ok),
            ("narrow".to_string(), narrow),
            ("short".to_string(), short),
        ];
        let bad = undersized_targets(&list, 24.0);
        assert_eq!(bad, vec!["narrow (23 by 40)", "short (40 by 23)"]);
    }

    #[test]
    fn exemptions_all_carry_a_reason() -> Result<(), String> {
        exemptions().map(|_| ())
    }
}
