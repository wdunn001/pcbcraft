//! Plain-English text for what a batch would change.
//!
//! Context: the approval card shows [`summary_text`] before the user allows an edit.
//! Rules: counts and names, no icons; the pane maps the text through its catalog.

use pcbcraft_engine::ChangeSummary;

fn group(verb: &str, list: &[(String, String)]) -> Option<String> {
    if list.is_empty() {
        return None;
    }
    if list.len() <= 2 {
        let names: Vec<&str> = list.iter().map(|(_, n)| n.as_str()).collect();
        return Some(format!("{verb} {}", names.join(" and ")));
    }
    let kind = list.first().map_or("objects", |(k, _)| k.as_str());
    Some(format!("{verb} {} {kind}s", list.len()))
}

/// For example "adds R1, moves 2 tracks, revision 7 to 8".
pub fn summary_text(s: &ChangeSummary, revision: u64) -> String {
    let mut parts: Vec<String> = [
        group("adds", &s.added),
        group("removes", &s.removed),
        group("moves", &s.moved),
        group("changes", &s.changed),
    ]
    .into_iter()
    .flatten()
    .collect();
    if s.nets_changed > 0 {
        parts.push(format!("changes {} nets", s.nets_changed));
    }
    if s.board_changed {
        parts.push("changes the board".into());
    }
    if s.more > 0 {
        parts.push(format!("and {} more", s.more));
    }
    if parts.is_empty() {
        parts.push("changes nothing".into());
    }
    parts.push(format!("revision {revision} to {}", revision + 1));
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_reads_as_one_plain_line() {
        let s = ChangeSummary {
            added: vec![("component".into(), "R1".into())],
            moved: vec![
                ("track".into(), "1".into()),
                ("track".into(), "2".into()),
                ("track".into(), "3".into()),
            ],
            ..ChangeSummary::default()
        };
        assert_eq!(
            summary_text(&s, 7),
            "adds R1, moves 3 tracks, revision 7 to 8"
        );
    }
}
