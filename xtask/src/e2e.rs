//! The e2e gate (issue #393): compare the controls the app paints with the
//! controls the `a_newcomer_*` stories touch.
//!
//! The UI crate writes two plain-text logs (see
//! `crates/ui-egui/src/e2e/audit.rs`); this module only reads text, so the
//! rules are unit tested without a window.
//!
//! Walk log lines: `P<TAB>id` painted, `T<TAB>id<TAB>page` painted with no
//! tooltip, `N<TAB>description` painted with no usable id. Touch log lines:
//! one control id each.
//!
//! Exemptions live in `e2e-exemptions.txt` at the repository root, one per
//! line: `kind | id | reason`, kind being `tooltip`, `story` or `name`. An id
//! ending in `*` matches by prefix. Blank lines and lines starting with `#`
//! are ignored. Every entry needs a reason.
use std::collections::BTreeSet;

#[derive(Debug, Default, PartialEq)]
pub struct Exemptions {
    entries: Vec<(String, String)>,
}

impl Exemptions {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut entries = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(3, '|').map(str::trim).collect();
            let [kind, id, reason] = parts[..] else {
                return Err(format!(
                    "e2e-exemptions.txt line {}: expected `kind | id | reason`",
                    number + 1
                ));
            };
            if !matches!(kind, "tooltip" | "story" | "name") {
                return Err(format!(
                    "e2e-exemptions.txt line {}: kind must be tooltip, story or name",
                    number + 1
                ));
            }
            if id.is_empty() || reason.is_empty() {
                return Err(format!(
                    "e2e-exemptions.txt line {}: id and reason are required",
                    number + 1
                ));
            }
            entries.push((kind.to_owned(), id.to_owned()));
        }
        Ok(Self { entries })
    }

    pub fn skips(&self, kind: &str, id: &str) -> bool {
        self.entries.iter().any(|(k, pattern)| {
            k == kind
                && match pattern.strip_suffix('*') {
                    Some(prefix) => id.starts_with(prefix),
                    None => pattern == id,
                }
        })
    }

    /// Entries that match nothing, so the list can only shrink.
    pub fn stale(&self, seen: &[(&str, &str)]) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(kind, pattern)| {
                !seen.iter().any(|(k, id)| {
                    k == kind
                        && match pattern.strip_suffix('*') {
                            Some(prefix) => id.starts_with(prefix),
                            None => pattern == id,
                        }
                })
            })
            .map(|(kind, pattern)| format!("{kind} | {pattern}"))
            .collect()
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Painted controls with no tooltip, as `id (page)`.
    pub no_tooltip: Vec<String>,
    /// Painted controls no story pressed or typed into.
    pub no_story: Vec<String>,
    /// Painted widgets with no label or icon id to name them.
    pub no_name: Vec<String>,
    /// Exemption entries that match nothing painted any more.
    pub stale: Vec<String>,
    pub painted: usize,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.no_tooltip.is_empty()
            && self.no_story.is_empty()
            && self.no_name.is_empty()
            && self.stale.is_empty()
    }

    /// One line per offending control, naming its id verbatim.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        out.extend(self.no_tooltip.iter().map(|c| format!("no tooltip: {c}")));
        out.extend(
            self.no_story
                .iter()
                .map(|c| format!("no a_newcomer story: {c}")),
        );
        out.extend(self.no_name.iter().map(|c| format!("no control id: {c}")));
        out.extend(
            self.stale
                .iter()
                .map(|c| format!("stale exemption (remove it): {c}")),
        );
        out
    }
}

pub fn diff(walk: &str, touched: &str, exempt: &Exemptions) -> Report {
    let touched: BTreeSet<&str> = touched
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut report = Report::default();
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for line in walk.lines() {
        let mut cols = line.split('\t');
        match (cols.next(), cols.next(), cols.next()) {
            (Some("P"), Some(id), _) => {
                report.painted += 1;
                seen.push(("story", id));
                if !touched.contains(id) && !exempt.skips("story", id) {
                    report.no_story.push(id.to_owned());
                }
            }
            (Some("T"), Some(id), page) => {
                seen.push(("tooltip", id));
                if !exempt.skips("tooltip", id) {
                    report
                        .no_tooltip
                        .push(format!("{id} (page {})", page.unwrap_or("?")));
                }
            }
            (Some("N"), Some(what), _) => {
                seen.push(("name", what));
                if !exempt.skips("name", what) {
                    report.no_name.push(what.to_owned());
                }
            }
            _ => {}
        }
    }
    report.stale = exempt.stale(&seen);
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALK: &str =
        "P\tbutton:Save\nP\tbutton:Route\nT\tbutton:Route\tpcb\nN\tsmall on pcb at 4,9\n";

    #[test]
    fn missing_tooltip_lists_control_id() {
        let report = diff(WALK, "button:Save\nbutton:Route\n", &Exemptions::default());
        assert_eq!(report.no_tooltip, vec!["button:Route (page pcb)"]);
        assert!(report.lines().iter().any(|l| l.contains("button:Route")));
    }

    #[test]
    fn missing_story_lists_control_id() {
        let report = diff(WALK, "button:Save\n", &Exemptions::default());
        assert_eq!(report.no_story, vec!["button:Route"]);
        assert!(
            report
                .lines()
                .iter()
                .any(|l| l == "no a_newcomer story: button:Route")
        );
    }

    #[test]
    fn exemption_file_skips_listed_control_ids() -> Result<(), String> {
        let exempt = Exemptions::parse(
            "# gaps tracked in the issue\nstory | button:Route | no story yet\ntooltip | button:R* | prefix\nname | small on pcb* | icon\n",
        )?;
        let report = diff(WALK, "button:Save\n", &exempt);
        assert!(report.is_clean(), "{:?}", report.lines());
        Ok(())
    }

    #[test]
    fn exemption_without_reason_is_rejected_and_stale_entries_reported() -> Result<(), String> {
        assert!(Exemptions::parse("story | button:Route |").is_err());
        let exempt = Exemptions::parse("story | button:Gone | was removed")?;
        let report = diff("P\tbutton:Save\n", "button:Save\n", &exempt);
        assert_eq!(report.stale, vec!["story | button:Gone"]);
        Ok(())
    }
}
