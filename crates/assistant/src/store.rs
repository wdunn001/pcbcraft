//! The conversation of one design, kept on this computer outside the design file.
//!
//! Context: one JSON-lines file per design under the user's configuration folder, next to the
//! recent-designs list. Written when a message ends, when the design is saved and when the app
//! saves its state; read when that design opens.
//! Rules: approvals, undo rows and permission questions come back as plain notes (they cannot be
//! answered any more); context notes and pictures are never written, so neither are keys; at most
//! [`MAX_ITEMS`] items and 4 MB are kept, oldest dropped first; an unsaved design has no file;
//! loading never fails (a damaged line is skipped and counted).
//! Do not: write a connection key, a context note or a picture here.

use crate::transcript::{Item, MAX_ITEMS};
use pcbcraft_engine::user_config::UserConfig;
use serde_json::{Value, json};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Largest file written, in bytes.
const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Longest line read back, in bytes.
const MAX_LINE: usize = 512 * 1024;

/// The file for a design, or `None` for a design that has no path yet.
pub fn transcript_path(config: &UserConfig, design_path: Option<&str>) -> Option<PathBuf> {
    let design = design_path.map(str::trim).filter(|p| !p.is_empty())?;
    let mut hasher = DefaultHasher::new();
    design.as_bytes().hash(&mut hasher);
    Some(
        config
            .folder()?
            .join("assistant")
            .join(format!("{:016x}.jsonl", hasher.finish())),
    )
}

fn to_line(item: &Item) -> Option<Value> {
    Some(match item {
        Item::User { text } => json!({"k": "user", "text": text}),
        Item::Assistant { text } => json!({"k": "assistant", "text": text}),
        Item::Thought { text } => json!({"k": "thought", "text": text}),
        Item::ToolCall { title, .. } => json!({"k": "tool", "text": title}),
        Item::Plan(steps) => json!({"k": "plan", "steps": steps}),
        Item::Note { text, error } => json!({"k": "note", "text": text, "error": error}),
        Item::Approval { title, answer, .. } => {
            let word = match answer {
                Some(true) => "allowed",
                Some(false) => "denied",
                None => "not answered",
            };
            json!({"k": "note", "text": format!("Asked to {title}; {word}"), "error": false})
        }
        Item::UndoStep { changes, .. } => {
            json!({"k": "note", "text": format!("{changes} changes in one undo step"), "error": false})
        }
        Item::Permission { title, answer, .. } => json!({
            "k": "note",
            "text": format!("{title}: {}", answer.as_deref().unwrap_or("not answered")),
            "error": false,
        }),
Item::Queued { .. } => return None,
        Item::Sources(links) => json!({
            "k": "sources",
            "links": links.iter().map(|l| json!({"title": l.title, "source": l.source})).collect::<Vec<_>>(),
        }),
        Item::Step { title, result, .. } => {
            json!({"k": "note", "text": format!("{title}: {result}"), "error": false})
        }
    })
}

/// Write the items, newest kept, with a temporary file and a rename. Returns how many were kept.
pub fn save(items: &[Item], path: &std::path::Path) -> Result<usize, String> {
    let start = items.len().saturating_sub(MAX_ITEMS);
    let mut lines: Vec<String> = items[start..]
        .iter()
        .filter_map(to_line)
        .map(|v| v.to_string())
        .collect();
    let mut total: usize = lines.iter().map(|l| l.len() + 1).sum();
    let mut drop = 0;
    while total > MAX_BYTES && drop < lines.len() {
        total -= lines[drop].len() + 1;
        drop += 1;
    }
    lines.drain(..drop);
    let kept = lines.len();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("tmp");
    let mut text = lines.join("\n");
    text.push('\n');
    std::fs::write(&temp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
    Ok(kept)
}

/// Read a transcript back; the second value counts the lines that could not be used.
pub fn load(path: &std::path::Path) -> (Vec<Item>, usize) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (Vec::new(), 0);
    };
    let mut items = Vec::new();
    let mut skipped = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let parsed: Option<Value> = if line.len() <= MAX_LINE {
            serde_json::from_str(line).ok()
        } else {
            None
        };
        let Some(v) = parsed else {
            skipped += 1;
            continue;
        };
        let text = v["text"].as_str().unwrap_or("").to_string();
        let item = match v["k"].as_str() {
            Some("user") => Item::User { text },
            Some("assistant") => Item::Assistant { text },
            Some("thought") => Item::Thought { text },
            Some("tool") => Item::ToolCall {
                id: String::new(),
                title: text,
                status: None,
                content: None,
            },
            Some("note") => Item::Note {
                text,
                error: v["error"].as_bool().unwrap_or(false),
            },
            Some("plan") => Item::Plan(
                v["steps"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| Some((s[0].as_str()?.to_string(), s[1].as_str()?.to_string())))
                    .collect(),
            ),
            Some("sources") => Item::Sources(
                v["links"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|l| {
                        Some(crate::backend::SourceLink {
                            title: l["title"].as_str()?.to_string(),
                            source: l["source"].as_str().unwrap_or("").to_string(),
                        })
                    })
                    .collect(),
            ),
            _ => {
                skipped += 1;
                continue;
            }
        };
        items.push(item);
    }
    let extra = items.len().saturating_sub(MAX_ITEMS);
    items.drain(..extra);
    (items, skipped)
}

/// Remove the file; a missing file is fine.
pub fn clear(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

/// The calendar date of a count of days since 1970-01-01.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The conversation as Markdown, with the design name and the date at the top.
pub fn export_markdown(items: &[Item], design: &str, unix_seconds: u64) -> String {
    let (y, m, d) = civil(i64::try_from(unix_seconds / 86_400).unwrap_or(0));
    let mut out = format!("# {design}\n\nConversation exported {y:04}-{m:02}-{d:02}.\n");
    let mut tools: Vec<&str> = Vec::new();
    let flush = |out: &mut String, tools: &mut Vec<&str>| {
        for t in tools.drain(..) {
            out.push_str(&format!("- {t}\n"));
        }
    };
    for item in items {
        match item {
            Item::ToolCall { title, .. } => tools.push(title),
            other => {
                if !tools.is_empty() {
                    flush(&mut out, &mut tools);
                }
                match other {
                    Item::User { text } => out.push_str(&format!("\n## You\n\n{text}\n")),
                    Item::Assistant { text } => {
                        out.push_str(&format!("\n## Assistant\n\n{text}\n"))
                    }
                    Item::Note { text, .. } => out.push_str(&format!("\n_{text}_\n")),
                    Item::Approval { .. } | Item::UndoStep { .. } | Item::Step { .. } => {
                        if let Some(v) = to_line(other) {
                            out.push_str(&format!("\n_{}_\n", v["text"].as_str().unwrap_or("")));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    flush(&mut out, &mut tools);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pcbcraft-store-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_transcript_round_trips_through_jsonl_with_approvals_turned_into_notes()
    -> Result<(), String> {
        let items = vec![
            Item::User {
                text: "add R1".into(),
            },
            Item::Approval {
                id: 1,
                title: "Add resistor R1".into(),
                detail: "adds R1".into(),
                summary: None,
                answer: Some(true),
            },
            Item::UndoStep {
                changes: 1,
                undone: false,
            },
            Item::Assistant {
                text: "Done".into(),
            },
        ];
        let path = temp("round.jsonl");
        save(&items, &path)?;
        let (back, skipped) = load(&path);
        clear(&path);
        assert_eq!(skipped, 0);
        assert_eq!(back.len(), 4);
        assert!(
            matches!(&back[1], Item::Note { text, .. } if text == "Asked to Add resistor R1; allowed")
        );
        assert!(
            matches!(&back[2], Item::Note { text, .. } if text == "1 changes in one undo step")
        );
        Ok(())
    }

    #[test]
    fn a_transcript_past_the_cap_keeps_the_newest_items() -> Result<(), String> {
        let items: Vec<Item> = (0..MAX_ITEMS + 10)
            .map(|i| Item::Note {
                text: format!("n{i}"),
                error: false,
            })
            .collect();
        let path = temp("cap.jsonl");
        let kept = save(&items, &path)?;
        let (back, _) = load(&path);
        clear(&path);
        assert_eq!(kept, MAX_ITEMS);
        assert!(matches!(&back[0], Item::Note { text, .. } if text == "n10"));
        Ok(())
    }

    #[test]
    fn a_damaged_line_is_skipped_and_counted() -> Result<(), String> {
        let path = temp("bad.jsonl");
        std::fs::write(
            &path,
            "{\"k\":\"user\",\"text\":\"hi\"}\nnot json\n{\"k\":\"mystery\"}\n",
        )
        .map_err(|e| e.to_string())?;
        let (back, skipped) = load(&path);
        clear(&path);
        assert_eq!(back.len(), 1);
        assert_eq!(skipped, 2);
        assert_eq!(load(&temp("missing.jsonl")), (Vec::new(), 0));
        Ok(())
    }

    #[test]
    fn an_unsaved_design_is_not_persisted_until_it_has_a_path() {
        let config = UserConfig::in_folder("/config/pcbcraft");
        assert_eq!(transcript_path(&config, None), None);
        assert_eq!(transcript_path(&config, Some("  ")), None);
        let a = transcript_path(&config, Some("/a/b.pcbcraft"));
        assert!(a.is_some());
        assert_ne!(a, transcript_path(&config, Some("/a/c.pcbcraft")));
        assert_eq!(a, transcript_path(&config, Some("/a/b.pcbcraft")));
    }

    #[test]
    fn export_markdown_names_the_design_and_keeps_the_order() {
        let items = vec![
            Item::User {
                text: "hello".into(),
            },
            Item::ToolCall {
                id: "t".into(),
                title: "inspect design".into(),
                status: None,
                content: None,
            },
            Item::Assistant { text: "hi".into() },
        ];
        let md = export_markdown(&items, "LED indicator", 1_767_225_600);
        assert!(md.starts_with("# LED indicator"));
        assert!(md.contains("2026-01-01"), "{md}");
        let you = md.find("## You").unwrap_or(usize::MAX);
        let tool = md.find("- inspect design").unwrap_or(usize::MAX);
        let asst = md.find("## Assistant").unwrap_or(usize::MAX);
        assert!(you < tool && tool < asst);
    }

    #[test]
    fn no_key_or_context_note_appears_in_the_file() -> Result<(), String> {
        // Context notes are sent to the model but never become transcript items.
        let items = vec![Item::User {
            text: "move R1".into(),
        }];
        let path = temp("secret.jsonl");
        save(&items, &path)?;
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        clear(&path);
        assert!(!text.contains("<context>") && !text.contains("sk-secret"));
        Ok(())
    }
}
