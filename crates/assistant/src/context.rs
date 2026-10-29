//! What the model is told about the open design with each message.
//!
//! Context: the pane builds a [`Context`] from the design summary (`design.summary`), the active
//! tab and the selection, and sends [`note`] before the user's words. The first message of a
//! conversation carries the whole note; later ones carry only what changed.
//! Rules: the note is plain text under [`MAX_NOTE`] characters, built from the design only (no
//! file paths, no keys); a picture is sent only to a connection that takes pictures and is at
//! most [`MAX_PICTURE_BYTES`].
//! Do not: put the whole project in the note; the model calls `inspect_design` for that.

use crate::backend::PromptPart;
use serde_json::Value;

/// Longest context note, in characters.
pub const MAX_NOTE: usize = 2000;
/// Largest picture sent, in bytes of PNG.
pub const MAX_PICTURE_BYTES: usize = 4 * 1024 * 1024;

/// The selected object.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub kind: String,
    pub id: u64,
    pub reference: String,
    pub value: String,
    pub net: String,
    pub position: Option<(f64, f64)>,
    pub layer: String,
}

/// What the pane knows when the user presses Send.
#[derive(Clone, Debug)]
pub struct Context {
    pub tab: &'static str,
    pub selection: Option<Selection>,
    /// The output of `design.summary`.
    pub summary: Value,
}

/// What an earlier message already told the model.
#[derive(Clone, Debug, PartialEq)]
pub struct Sent {
    pub revision: u64,
    pub selection: Option<u64>,
    pub tab: &'static str,
}

impl Context {
    /// What a message built from this context tells the model, for the next delta.
    pub fn sent(&self) -> Sent {
        Sent {
            revision: self.summary["revision"].as_u64().unwrap_or(0),
            selection: self.selection.as_ref().map(|s| s.id),
            tab: self.tab,
        }
    }
}

fn selection_line(s: &Selection) -> String {
    let mut line = format!("Selected: {}", s.kind);
    if !s.reference.is_empty() {
        line.push_str(&format!(" {}", s.reference));
    }
    if !s.value.is_empty() {
        line.push_str(&format!(" ({})", s.value));
    }
    line.push_str(&format!(", id {}", s.id));
    if let Some((x, y)) = s.position {
        line.push_str(&format!(", at {x:.1}, {y:.1} mm"));
    }
    if !s.net.is_empty() {
        line.push_str(&format!(", net {}", s.net));
    }
    if !s.layer.is_empty() {
        line.push_str(&format!(", layer {}", s.layer));
    }
    line.push('.');
    line
}

fn clip(mut text: String) -> String {
    if text.chars().count() > MAX_NOTE {
        text = text.chars().take(MAX_NOTE - 12).collect();
        text.push_str("\n</context>");
    }
    text
}

/// The context note for a message. With `since` it holds only what changed (and is empty when
/// nothing did). The note sits in a `<context>` fence the system note explains.
pub fn note(ctx: &Context, since: Option<&Sent>) -> String {
    let now = ctx.sent();
    let selection = ctx.selection.as_ref().map(selection_line);
    if let Some(before) = since {
        if *before == now {
            return String::new();
        }
        let mut lines = vec!["<context>".to_string()];
        if before.revision != now.revision {
            lines.push(format!(
                "The design changed: revision {} to {}.",
                before.revision, now.revision
            ));
        }
        if before.tab != now.tab {
            lines.push(format!("View: {}.", now.tab));
        }
        if before.selection != now.selection {
            lines.push(selection.unwrap_or_else(|| "Nothing is selected.".to_string()));
        }
        lines.push("</context>".to_string());
        return clip(lines.join("\n"));
    }
    let s = &ctx.summary;
    let num = |v: &Value| v.as_u64().unwrap_or(0);
    let mut lines = vec![
        "<context>".to_string(),
        format!("View: {}.", ctx.tab),
        format!(
            "Design \"{}\", revision {}. Board {} x {} mm, {} layers.",
            s["name"].as_str().unwrap_or(""),
            num(&s["revision"]),
            s["board"]["width"],
            s["board"]["height"],
            num(&s["board"]["layers"]),
        ),
        format!(
            "{} components, {} nets, {} tracks, {} vias. Findings: {} errors, {} warnings, {} notes.",
            num(&s["counts"]["components"]),
            num(&s["counts"]["nets"]),
            num(&s["counts"]["tracks"]),
            num(&s["counts"]["vias"]),
            num(&s["findings"]["errors"]),
            num(&s["findings"]["warnings"]),
            num(&s["findings"]["notes"]),
        ),
    ];
    if let Some(line) = selection {
        lines.push(line);
    }
    let parts: Vec<String> = s["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            format!(
                "{} {}",
                p["reference"].as_str().unwrap_or(""),
                p["value"].as_str().unwrap_or("")
            )
            .trim()
            .to_string()
        })
        .collect();
    let nets: Vec<String> = s["nets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| {
            format!(
                "{} ({} pins)",
                n["name"].as_str().unwrap_or(""),
                num(&n["pins"])
            )
        })
        .collect();
    let mut tail = Vec::new();
    for (label, list) in [("Parts", parts), ("Nets", nets)] {
        if !list.is_empty() {
            tail.push(format!("{label}: {}.", list.join("; ")));
        }
    }
    let mut text = lines.join("\n");
    // Keep the lists inside the budget: shorten them rather than the selection line.
    let budget = MAX_NOTE.saturating_sub(text.chars().count() + 24);
    let mut tail_text = tail.join("\n");
    if tail_text.chars().count() > budget {
        tail_text = tail_text.chars().take(budget.saturating_sub(18)).collect();
        tail_text.push_str(" ... and more.");
    }
    if !tail_text.is_empty() {
        text.push('\n');
        text.push_str(&tail_text);
    }
    text.push_str("\n</context>");
    clip(text)
}

/// A picture of the view as a message part, or a note saying why it was left out.
pub fn picture(takes_pictures: bool, png_base64: Option<&str>) -> Result<PromptPart, String> {
    if !takes_pictures {
        return Err(
            "The picture was left out: this connection is not set to take pictures.".into(),
        );
    }
    let data = png_base64.ok_or("The picture could not be made.")?;
    // Base64 is 4 bytes for every 3.
    if data.len() / 4 * 3 > MAX_PICTURE_BYTES {
        return Err("The picture was left out: it is larger than 4 MB.".into());
    }
    Ok(PromptPart::Picture {
        mime: "image/png".into(),
        base64: data.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(revision: u64, selected: Option<u64>) -> Context {
        Context {
            tab: "schematic",
            selection: selected.map(|id| Selection {
                kind: "resistor".into(),
                id,
                reference: "R1".into(),
                value: "330".into(),
                net: "LED".into(),
                position: Some((40.0, 14.0)),
                layer: String::new(),
            }),
            summary: json!({"name":"LED indicator","revision":revision,
                "board":{"width":80,"height":50,"layers":2},
                "counts":{"components":3,"nets":3,"tracks":0,"vias":0},
                "findings":{"errors":0,"warnings":1,"notes":0},
                "parts":[{"reference":"R1","value":"330"},{"reference":"D1","value":"LED"}],
                "nets":[{"name":"LED","pins":2}]}),
        }
    }

    #[test]
    fn the_first_message_of_a_turn_carries_the_context_note_and_the_second_carries_only_the_delta()
    {
        let first = note(&ctx(3, Some(2)), None);
        assert!(first.starts_with("<context>") && first.ends_with("</context>"));
        assert!(first.contains("revision 3") && first.contains("Parts: R1 330; D1 LED"));
        assert!(first.chars().count() <= MAX_NOTE);
        let c = ctx(5, Some(2));
        let delta = note(&c, Some(&ctx(3, Some(2)).sent()));
        assert!(delta.contains("revision 3 to 5"));
        assert!(
            !delta.contains("Parts:"),
            "the delta does not repeat the design"
        );
        assert_eq!(note(&c, Some(&c.sent())), "", "nothing new, nothing sent");
    }

    #[test]
    fn a_selected_part_is_named_in_the_note_with_its_net_and_position() {
        let n = note(&ctx(1, Some(7)), None);
        assert!(
            n.contains("Selected: resistor R1 (330), id 7, at 40.0, 14.0 mm, net LED."),
            "{n}"
        );
    }

    #[test]
    fn attach_view_is_left_out_with_a_note_for_a_connection_that_takes_no_pictures() {
        assert!(
            picture(false, Some("AA==")).is_err_and(|e| e.contains("not set to take pictures"))
        );
        assert!(matches!(
            picture(true, Some("AA==")),
            Ok(PromptPart::Picture { .. })
        ));
        let huge = "A".repeat(MAX_PICTURE_BYTES * 2);
        assert!(picture(true, Some(&huge)).is_err_and(|e| e.contains("4 MB")));
    }
}
