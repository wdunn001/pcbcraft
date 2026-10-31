//! Read-only publication privacy and licence scan command.
//!
//! # Rules
//! - `publication.scan` does not change the project or revision.
//!
//! # Dependencies
//! - crates/publication/src/scan.rs
//! - crates/engine/src/lib.rs command registry
//!
//! # Do not
//! - Upload bundles from here; callers act on the scan result.

use crate::CommandSpec;
use pcbcraft_publication::{ScanRequest, scan};
use pcbcraft_doc::Project;
use serde_json::{Value, json};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "publication.scan",
        description: "Run privacy, licence, datasheet and bundle file-type checks on a publication upload payload without changing the open project; hard fails block upload and list a remove-and-retry action",
        input_schema: json!({
            "type":"object",
            "properties":{
                "design_licence":{"type":"string","maxLength":64},
                "author_emails":{"type":"array","items":{"type":"string","maxLength":256}},
                "parts":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string","maxLength":128},"licence":{"type":"string","maxLength":64},"source":{"type":"string","maxLength":256}},"required":["id","licence","source"],"additionalProperties":false}},
                "datasheets":{"type":"array","items":{"type":"object","properties":{"url":{"type":"string","maxLength":2048},"title":{"type":"string","maxLength":256},"sha256":{"type":"string","minLength":64,"maxLength":64}},"required":["url","title","sha256"],"additionalProperties":false}},
                "compliance":{"type":"object","properties":{"claims":{"type":"array","items":{"type":"string","maxLength":512}},"export_review_requested":{"type":"boolean"},"mains_power":{"type":"boolean"},"battery_power":{"type":"boolean"},"high_voltage":{"type":"boolean"}},"additionalProperties":false},
                "files":{"type":"array","items":{"type":"object","properties":{"path":{"type":"string","maxLength":512},"role":{"type":"string","maxLength":64},"text":{"type":"string","maxLength":1048576},"bytes_base64":{"type":"string","maxLength":4194304}},"required":["path"],"additionalProperties":false}}
            },
            "required":["design_licence"],
            "additionalProperties":false
        }),
    }]
}

pub(super) fn handles(id: &str) -> bool {
    id == "publication.scan"
}

pub(super) fn run(_p: &Project, params: &Value) -> Result<Value, String> {
    let req: ScanRequest = serde_json::from_value(params.clone())
        .map_err(|e| format!("invalid publication.scan params: {e}"))?;
    let result = scan(&req.into_input());
    let map_outcome = |o: pcbcraft_publication::ScanOutcome| -> Value {
        match o {
            pcbcraft_publication::ScanOutcome::HardFail {
                rule,
                path,
                line,
                column,
                detail,
                action,
            } => json!({"kind":"hard_fail","rule":rule,"path":path,"line":line,"column":column,"detail":detail,"action":action}),
            pcbcraft_publication::ScanOutcome::HumanQueue {
                rule,
                path,
                detail,
            } => json!({"kind":"human_queue","rule":rule,"path":path,"detail":detail}),
            pcbcraft_publication::ScanOutcome::Note { rule, detail } => {
                json!({"kind":"note","rule":rule,"detail":detail})
            }
        }
    };
    Ok(json!({
        "ok_to_upload": result.ok_to_upload(),
        "hard_fails": result.hard_fails.into_iter().map(map_outcome).collect::<Vec<_>>(),
        "human_queue": result.human_queue.into_iter().map(map_outcome).collect::<Vec<_>>(),
        "notes": result.notes.into_iter().map(map_outcome).collect::<Vec<_>>(),
    }))
}
