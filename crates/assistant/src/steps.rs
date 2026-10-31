//! Plain-language steps and next-step suggestions of one assistant run (no UI, no text catalog).
//!
//! Context: every tool call the model makes becomes one `Item::Step` row in the conversation.
//! The pane names it from [`StepKind`], a catalog keyed by engine command, and summarises the
//! result with [`digest`]. At the end of a message the model may offer next steps; they are
//! read with [`split_suggestions`] from the lines the system note asks for.
//! Rules: classification looks at the command id only (`execute` and `batch` are looked through);
//! a digest is numbers and short names, never the model's raw output; parsing is total and
//! bounded (at most [`MAX_SUGGESTIONS`] cards, titles and details cut to fixed lengths).
//! Do not: put user-facing words here; the pane maps a [`StepKind`] id to the text catalog.

use serde_json::Value;

/// Most next-step cards kept from one message.
pub const MAX_SUGGESTIONS: usize = 4;
/// Longest card title, in characters.
const MAX_TITLE: usize = 60;
/// Longest card detail, in characters.
const MAX_DETAIL: usize = 140;

/// What a step is for, in the words a newcomer uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    /// Searching libraries and part lists.
    Parts,
    /// Adding or moving parts.
    Placing,
    /// Making nets and wires.
    Connecting,
    /// Tracks, vias and zones.
    Routing,
    /// Board outline, holes and size.
    Board,
    /// Running the rule checks.
    Checking,
    /// Looking at the design without changing it.
    Looking,
    /// Taking a picture of the view.
    Picture,
    /// Saving, opening or exporting files.
    Files,
    /// Anything else.
    Other,
}

impl StepKind {
    /// The short id that ends the catalog key `assistant.step.<id>`.
    pub fn id(self) -> &'static str {
        match self {
            StepKind::Parts => "parts",
            StepKind::Placing => "placing",
            StepKind::Connecting => "connecting",
            StepKind::Routing => "routing",
            StepKind::Board => "board",
            StepKind::Checking => "checking",
            StepKind::Looking => "looking",
            StepKind::Picture => "picture",
            StepKind::Files => "files",
            StepKind::Other => "other",
        }
    }
}

/// Where a step is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    /// Waiting for the user's answer.
    Waiting,
    /// Running now.
    Running,
    /// Finished.
    Done,
    /// Refused, declined or failed.
    Failed,
}

/// The kind for one engine command or tool id.
pub fn command_kind(command: &str) -> StepKind {
    match command {
        "library.search"
        | "library.list"
        | "library.catalog"
        | "library.sources"
        | "library.compare"
        | "library.inspect_symbol"
        | "library.inspect_footprint" => StepKind::Parts,
        "checks" => StepKind::Checking,
        "inspect_design" | "design.summary" | "list_commands" | "board_mesh" | "design.recents"
        | "dry_run" => StepKind::Looking,
        "screenshot" => StepKind::Picture,
        "save" | "open" | "export" | "new_design" | "sample_design" | "undo" | "redo" => {
            StepKind::Files
        }
        "net.connect" | "net.create" | "net.rename" | "net.tie" | "net.swap" | "net.disconnect" => {
            StepKind::Connecting
        }
        c if c.starts_with("wire.") || c.starts_with("label.") || c.starts_with("netclass.") => {
            StepKind::Connecting
        }
        c if c.starts_with("track.")
            || c.starts_with("via.")
            || c.starts_with("zone.")
            || c.starts_with("teardrop.")
            || c == "board.unroute"
            || c == "net.unroute" =>
        {
            StepKind::Routing
        }
        c if c.starts_with("board.") || c.starts_with("hole.") => StepKind::Board,
        c if c.starts_with("component.")
            || c.starts_with("footprint.")
            || c.starts_with("schematic.")
            || c.starts_with("pattern.")
            || c.starts_with("object.") =>
        {
            StepKind::Placing
        }
        c if c.ends_with(".write") => StepKind::Files,
        _ => StepKind::Other,
    }
}

/// The engine commands one tool call carries, in order (`execute` has one, `batch` many; any
/// other tool is its own command).
pub fn commands_of(tool: &str, args: &Value) -> Vec<String> {
    match tool {
        "execute" => vec![
            args.get("command")
                .and_then(Value::as_str)
                .unwrap_or("execute")
                .to_string(),
        ],
        "batch" | "dry_run" => args
            .get("commands")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| c.get("command").and_then(Value::as_str))
            .map(str::to_string)
            .collect(),
        other => vec![other.to_string()],
    }
}

/// The kind of a whole tool call: the commonest kind among its commands (the earliest wins a tie).
pub fn step_kind(tool: &str, args: &Value) -> StepKind {
    let commands = commands_of(tool, args);
    let mut best = (StepKind::Other, 0usize);
    for c in &commands {
        let kind = command_kind(c);
        let n = commands.iter().filter(|x| command_kind(x) == kind).count();
        if n > best.1 {
            best = (kind, n);
        }
    }
    best.0
}

/// The result payload of a tool call: its structured part when it has one.
fn payload(result: &Value) -> &Value {
    result.get("structuredContent").unwrap_or(result)
}

/// The first text of an error result, one line, cut to 120 characters.
pub fn error_line(result: &Value) -> String {
    let text = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or("");
    let line = text.lines().next().unwrap_or("");
    line.chars().take(120).collect()
}

/// A number that makes a one-line result, with the catalog id of its sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digest {
    /// Ends the catalog key `assistant.digest.<id>`.
    pub id: &'static str,
    /// The numbers the sentence mentions, in order (`{a}`, `{b}`).
    pub numbers: [usize; 2],
}

/// Summarise a finished read-only call as numbers; `None` when there is nothing to count.
///
/// # Rules
/// - Searches count their listed results; checks count problems and unrouted connections.
pub fn digest(tool: &str, result: &Value) -> Option<Digest> {
    let data = payload(result);
    let count = |value: &Value| -> usize {
        match value {
            Value::Array(a) => a.len(),
            Value::Object(o) => o
                .values()
                .find_map(|v| v.as_array().map(Vec::len))
                .unwrap_or(0),
            _ => 0,
        }
    };
    let len = |key: &str| data.get(key).and_then(Value::as_array).map_or(0, Vec::len);
    match tool {
        "library.search" | "library.list" => Some(Digest {
            id: "found",
            numbers: [count(data), 0],
        }),
        "checks" => {
            let problems = len("erc") + len("drc");
            let open = len("airwires");
            Some(match (problems, open) {
                (0, 0) => Digest {
                    id: "checks_clean",
                    numbers: [0, 0],
                },
                (0, n) => Digest {
                    id: "checks_unrouted",
                    numbers: [n, 0],
                },
                (p, n) => Digest {
                    id: "checks_found",
                    numbers: [p, n],
                },
            })
        }
        _ => None,
    }
}

/// One card offered at the end of a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// What pressing it asks for.
    pub title: String,
    /// One line on why.
    pub detail: String,
    /// The model's best pick (at most one is kept).
    pub recommended: bool,
}

/// Split a message into its text and the next-step lines it ends with.
///
/// A line is `Next: <title> | <detail>` or `Next (recommended): <title> | <detail>`; the
/// detail is optional. Those lines are removed from the text. Only the first
/// [`MAX_SUGGESTIONS`] are kept and only the first recommended one stays marked.
pub fn split_suggestions(text: &str) -> (String, Vec<Suggestion>) {
    let mut body: Vec<&str> = Vec::new();
    let mut cards: Vec<Suggestion> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let (recommended, rest) = if let Some(r) = trimmed.strip_prefix("Next (recommended):") {
            (true, r)
        } else if let Some(r) = trimmed.strip_prefix("Next:") {
            (false, r)
        } else {
            body.push(line);
            continue;
        };
        let (title, detail) = rest.split_once('|').unwrap_or((rest, ""));
        let title: String = title.trim().chars().take(MAX_TITLE).collect();
        if title.is_empty() || cards.len() >= MAX_SUGGESTIONS {
            continue;
        }
        cards.push(Suggestion {
            title,
            detail: detail.trim().chars().take(MAX_DETAIL).collect(),
            recommended: recommended && !cards.iter().any(|c| c.recommended),
        });
    }
    (body.join("\n").trim_end().to_string(), cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn commands_are_named_by_what_a_newcomer_would_call_them() {
        assert_eq!(command_kind("library.search"), StepKind::Parts);
        assert_eq!(command_kind("component.add"), StepKind::Placing);
        assert_eq!(command_kind("track.cleanup"), StepKind::Routing);
        assert_eq!(command_kind("checks"), StepKind::Checking);
        assert_eq!(command_kind("no.such"), StepKind::Other);
    }

    #[test]
    fn a_batch_takes_the_kind_of_most_of_its_commands() {
        let args = json!({"commands":[
            {"command":"component.add","params":{}},
            {"command":"component.add","params":{}},
            {"command":"net.create","params":{}}]});
        assert_eq!(step_kind("batch", &args), StepKind::Placing);
        assert_eq!(commands_of("batch", &args).len(), 3);
    }

    #[test]
    fn checks_are_summarised_as_counts() {
        let r = json!({"structuredContent":{"erc":[],"drc":[1],"airwires":[1,2]}});
        let d = digest("checks", &r);
        assert_eq!(
            d,
            Some(Digest {
                id: "checks_found",
                numbers: [1, 2]
            })
        );
    }

    #[test]
    fn next_step_lines_become_cards_and_leave_the_text() {
        let text = "Done.\nNext (recommended): Make shop files | Gerber files\nNext: Mark polarity\nNext (recommended): Another";
        let (body, cards) = split_suggestions(text);
        assert_eq!(body, "Done.");
        assert_eq!(cards.len(), 3);
        assert!(cards[0].recommended && !cards[2].recommended);
        assert_eq!(cards[1].detail, "");
    }
}
