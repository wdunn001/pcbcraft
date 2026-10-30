//! Engine-side [`FindingSet`] for the open session (board checks plus library).

use crate::Session;
use pcbcraft_library::FindingSeverity;
use pcbcraft_rules::FindingSet;
use pcbcraft_rules::SeverityFilter;
use serde_json::Value;

impl Session {
    /// Findings of the open design: wiring and board checks plus every project
    /// library footprint check, sorted for display.
    ///
    /// # Rules
    /// The one place engine and tests derive counts from; UI should mirror this.
    pub fn finding_set(&self) -> FindingSet {
        let checks = self.checks();
        let mut library = Vec::new();
        for def in &self.project.library.footprints {
            let findings = pcbcraft_library::check_footprint(def);
            if findings.is_empty() {
                continue;
            }
            library.push((def.name.clone(), findings));
        }
        let refs: Vec<(&str, &[pcbcraft_library::FootprintFinding])> = library
            .iter()
            .map(|(name, rows)| (name.as_str(), rows.as_slice()))
            .collect();
        FindingSet::build(&checks, &refs)
    }
}

fn counts_by_hand(checks: &Value, project: &pcbcraft_doc::Project) -> (usize, usize, usize) {
    let mut c = (0usize, 0usize, 0usize);
    for key in ["erc", "drc"] {
        for f in checks
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match f.get("severity").and_then(Value::as_str) {
                Some("error") => c.0 += 1,
                Some("warning") => c.1 += 1,
                _ => c.2 += 1,
            }
        }
    }
    for def in &project.library.footprints {
        for f in pcbcraft_library::check_footprint(def) {
            match f.severity {
                FindingSeverity::Error => c.0 += 1,
                FindingSeverity::Warning => c.1 += 1,
            }
        }
    }
    c
}

/// Every aggregate agrees with the hand count and with each other filter.
pub(crate) fn assert_finding_counts_agree(session: &Session, set: &FindingSet, what: &str) {
    let (e, w, n) = counts_by_hand(&session.checks(), &session.project);
    let all = set.counts();
    assert_eq!(
        (all.errors, all.warnings, all.notes),
        (e, w, n),
        "{what}: set vs hand count"
    );
    assert_eq!(all.strip(), (e, w + n), "{what}: status strip");
    assert_eq!(all.total(), e + w + n, "{what}: badge");
    for filter in [
        SeverityFilter::All,
        SeverityFilter::Errors,
        SeverityFilter::Warnings,
        SeverityFilter::Notes,
    ] {
        let shown = set.visible_counts(filter);
        let listed = set.visible(filter).count();
        assert_eq!(shown.total(), listed, "{what}: {filter:?} list length");
        let want = match filter {
            SeverityFilter::All => (e, w, n),
            SeverityFilter::Errors => (e, 0, 0),
            SeverityFilter::Warnings => (0, w, 0),
            SeverityFilter::Notes => (0, 0, n),
        };
        assert_eq!(
            (shown.errors, shown.warnings, shown.notes),
            want,
            "{what}: {filter:?}"
        );
    }
    let mut last = 0;
    for row in set.visible(SeverityFilter::All) {
        let rank = row.loudness as u8;
        assert!(rank >= last, "{what}: errors sort above warnings");
        last = rank;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_io;
    use pcbcraft_rules::Loudness;
    use serde_json::json;
    use std::path::PathBuf;

    const FILTERS: [SeverityFilter; 4] = [
        SeverityFilter::All,
        SeverityFilter::Errors,
        SeverityFilter::Warnings,
        SeverityFilter::Notes,
    ];

    fn examples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("examples")
    }

    fn example_projects() -> Result<Vec<PathBuf>, String> {
        let dir = examples_dir();
        let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "pcbcraft"))
            .collect();
        found.sort();
        if found.is_empty() {
            return Err("no example projects found".into());
        }
        Ok(found)
    }

    fn load_session(path: &std::path::Path) -> Result<Session, String> {
        let project = pcbcraft_io::load(path).map_err(|e| e.to_string())?;
        let mut session = Session::default();
        session
            .replace(project)
            .map_err(|e| e.to_string())?;
        Ok(session)
    }

    #[test]
    fn finding_counts_agree_on_every_example() -> Result<(), String> {
        for path in example_projects()? {
            let mut s = load_session(&path)?;
            let set = s.finding_set();
            assert_finding_counts_agree(&s, &set, &path.display().to_string());
            for filter in FILTERS {
                let _ = set.visible(filter).count();
            }
            let before = set.counts().total();
            let target = ["erc", "drc"].iter().find_map(|key| {
                s.checks().get(*key).and_then(Value::as_array).and_then(|list| {
                    list.iter().find_map(|f| {
                        let ids = f.get("objects")?.as_array()?.clone();
                        let code = f.get("code")?.as_str()?.to_owned();
                        (!ids.is_empty()).then_some((code, ids))
                    })
                })
            });
            if let Some((code, ids)) = target {
                s.execute(
                    "rules.exclude",
                    json!({"code": code, "ids": ids}),
                    None,
                )
                .map_err(|e| e.to_string())?;
                let after = s.finding_set();
                assert_finding_counts_agree(
                    &s,
                    &after,
                    &format!("{} after hiding {code}", path.display()),
                );
                assert!(
                    after.counts().total() < before,
                    "{}: hiding {code} removes it from every count",
                    path.display()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_newcomer_sees_the_same_finding_counts_everywhere() -> Result<(), String> {
        let path = examples_dir().join("showcase-with-findings.pcbcraft");
        let s = load_session(&path)?;
        let set = s.finding_set();
        assert_finding_counts_agree(&s, &set, "showcase-with-findings");
        let counts = set.counts();
        assert!(counts.errors >= 1, "the sample carries an error");
        let rows: Vec<_> = set.visible(SeverityFilter::All).collect();
        assert_eq!(rows.len(), counts.total(), "the list has as many rows");
        let first = rows.first().ok_or("the list has rows")?;
        assert_eq!(
            first.loudness,
            Loudness::Error,
            "the list starts with the error"
        );
        Ok(())
    }
}
