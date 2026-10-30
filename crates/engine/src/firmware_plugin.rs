//! Optional firmware target export plugin (issue #980).
use super::*;
use pcbcraft_plugin_firmware_target::{
    EXPORT_KIND, FirmwareTargetPlugin, firmware_config_compare, firmware_target_files,
};
use pcbcraft_plugin_sdk::ExportPlugin;

const COMMANDS: [&str; 2] = ["firmware.config.compare", "firmware.target.export"];

static PLUGIN: FirmwareTargetPlugin = FirmwareTargetPlugin;

pub(super) fn handles(command: &str) -> bool {
    COMMANDS.contains(&command)
}

pub(super) fn export_kind(kind: &str) -> bool {
    kind == EXPORT_KIND
}

pub(super) fn export_kind_files(p: &Project, kind: &str) -> Result<Vec<(String, Vec<u8>)>, Error> {
    PLUGIN.export_files(kind, p).map_err(Error::Invalid)
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "firmware.target.export",
            description: "Write flight-controller board config.h and manifest from the firmware design; changes nothing in the project",
            input_schema: schema(json!({}), &[]),
        },
        CommandSpec {
            id: "firmware.config.compare",
            description: "Compare an existing flight-controller config.h pin map with the firmware design attached to the project; read-only",
            input_schema: schema(
                json!({"config":{"type":"string","minLength":1,"maxLength":262144,"description":"full config.h text to compare"}}),
                &["config"],
            ),
        },
    ]
}

pub(super) fn run(p: &Project, command: &str, params: &Value) -> Result<Value, String> {
    match command {
        "firmware.target.export" => {
            let files = firmware_target_files(p)?;
            let mut out = Vec::with_capacity(files.len());
            for (name, body) in files {
                let body = String::from_utf8(body).map_err(|e| e.to_string())?;
                out.push(json!({"name": name, "body": body}));
            }
            Ok(json!({"files": out, "plugin": pcbcraft_plugin_firmware_target::PLUGIN_ID}))
        }
        "firmware.config.compare" => {
            let text = params
                .get("config")
                .and_then(Value::as_str)
                .ok_or_else(|| "config is required".to_string())?;
            if text.len() > 262_144 {
                return Err("config text is too long".into());
            }
            let mismatches = firmware_config_compare(p, text)?;
            Ok(json!({"mismatches": mismatches, "count": mismatches.len()}))
        }
        _ => Err(format!("unknown firmware plugin command: {command}")),
    }
}
