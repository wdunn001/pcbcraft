//! Vendor quote handoff commands (compile-time optional, no network I/O).
use pcbcraft_doc::{
    VendorAdapterDescriptor, VendorHandoffModuleSettings, VendorQuoteHandoffManifest,
    fixture_upload_adapter, fixture_upload_manifest,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{CommandSpec, Error, Project, schema};

pub fn commands() -> impl Iterator<Item = CommandSpec> {
    [
        CommandSpec {
            id: "vendor_handoff.capabilities",
            description: "Return the instance capability fragment for the optional vendor quote handoff module; every flag is false when the module is off or was not compiled in",
            input_schema: schema(
                json!({
                    "settings":{"type":"object","description":"optional VendorHandoffModuleSettings JSON; omitted means all defaults off"}
                }),
                &[],
            ),
        },
        CommandSpec {
            id: "vendor_handoff.settings.validate",
            description: "Validate vendor handoff module settings without changing the design",
            input_schema: schema(
                json!({"settings":{"type":"object","description":"VendorHandoffModuleSettings JSON"}}),
                &["settings"],
            ),
        },
        CommandSpec {
            id: "vendor_handoff.quote.validate",
            description: "Validate a vendor quote handoff manifest (file list, hashes and adapter metadata) before the user approves sending manufacturing outputs",
            input_schema: schema(
                json!({"manifest":{"type":"object","description":"VendorQuoteHandoffManifest JSON"}}),
                &["manifest"],
            ),
        },
        CommandSpec {
            id: "vendor_handoff.adapters.list",
            description: "List built-in vendor adapter descriptors when vendor quotes are enabled on this instance; otherwise returns an empty list",
            input_schema: schema(
                json!({
                    "settings":{"type":"object","description":"optional VendorHandoffModuleSettings JSON; omitted means all defaults off"}
                }),
                &[],
            ),
        },
    ]
    .into_iter()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsParams {
    #[serde(default)]
    settings: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateSettingsParams {
    settings: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateManifestParams {
    manifest: Value,
}

fn parse_settings(v: Option<Value>) -> Result<VendorHandoffModuleSettings, Error> {
    let raw = match v {
        Some(value) => value,
        None => json!({}),
    };
    serde_json::from_value(raw).map_err(|e| Error::Invalid(format!("invalid settings: {e}")))
}

pub fn run(
    _p: &mut Project,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    match command {
        "vendor_handoff.capabilities" => Some((|| {
            let a: SettingsParams = parse(params)?;
            let settings = parse_settings(a.settings)?;
            Ok(settings.capability_json(true))
        })()),
        "vendor_handoff.settings.validate" => Some((|| {
            let a: ValidateSettingsParams = parse(params)?;
            let settings: VendorHandoffModuleSettings = serde_json::from_value(a.settings)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(if settings.valid() {
                json!({"valid": true})
            } else {
                json!({"valid": false, "reason": "sub-settings require the master module switch"})
            })
        })()),
        "vendor_handoff.quote.validate" => Some((|| {
            let a: ValidateManifestParams = parse(params)?;
            let manifest: VendorQuoteHandoffManifest = serde_json::from_value(a.manifest)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(if manifest.valid() {
                json!({"valid": true, "files": manifest.files.len()})
            } else {
                json!({"valid": false, "reason": "manifest failed structural checks"})
            })
        })()),
        "vendor_handoff.adapters.list" => Some((|| {
            let a: SettingsParams = parse(params)?;
            let settings = parse_settings(a.settings)?;
            let caps = settings.instance_capabilities(true);
            let adapters: Vec<VendorAdapterDescriptor> = if caps.vendor_quotes {
                vec![fixture_upload_adapter()]
            } else {
                vec![]
            };
            Ok(json!({"adapters": adapters}))
        })()),
        _ => None,
    }
}

fn parse<T: for<'de> Deserialize<'de>>(params: &Value) -> Result<T, Error> {
    serde_json::from_value(params.clone()).map_err(|e| Error::Invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::VendorHandoffModuleSettings;

    #[test]
    fn off_settings_yield_empty_adapters() -> Result<(), String> {
        let mut project = Project::default();
        let result = run(
            &mut project,
            "vendor_handoff.adapters.list",
            &json!({}),
        )
        .ok_or("handler missing")??;
        let list = result["adapters"]
            .as_array()
            .ok_or("adapters array")?;
        assert!(list.is_empty());
        Ok(())
    }

    #[test]
    fn enabled_settings_list_fixture_adapter() -> Result<(), String> {
        let mut project = Project::default();
        let settings = json!({
            "enabled": true,
            "vendor_quotes": {"enabled": true, "show_in_app": true}
        });
        let result = run(
            &mut project,
            "vendor_handoff.adapters.list",
            &json!({"settings": settings}),
        )
        .ok_or("handler missing")??;
        let list = result["adapters"]
            .as_array()
            .ok_or("adapters array")?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["id"], "fixture-upload");
        Ok(())
    }

    #[test]
    fn quote_validate_accepts_fixture_manifest() -> Result<(), String> {
        let mut project = Project::default();
        let manifest = fixture_upload_manifest();
        let value = serde_json::to_value(&manifest).map_err(|e| e.to_string())?;
        let result = run(
            &mut project,
            "vendor_handoff.quote.validate",
            &json!({"manifest": value}),
        )
        .ok_or("handler missing")??;
        assert_eq!(result["valid"], true);
        assert_eq!(result["files"], 2);
        Ok(())
    }

    #[test]
    fn capabilities_hide_flags_when_off() -> Result<(), String> {
        let mut project = Project::default();
        let result = run(
            &mut project,
            "vendor_handoff.capabilities",
            &json!({}),
        )
        .ok_or("handler missing")??;
        assert_eq!(result["compiled"], true);
        assert!(result.get("vendor_quotes").is_none());
        let settings = VendorHandoffModuleSettings {
            enabled: true,
            vendor_quotes: pcbcraft_doc::VendorQuoteSettings {
                enabled: true,
                show_in_app: true,
            },
            ..Default::default()
        };
        let on = run(
            &mut project,
            "vendor_handoff.capabilities",
            &json!({"settings": settings}),
        )
        .ok_or("handler missing")??;
        assert_eq!(on["vendor_quotes"], true);
        assert_eq!(on["vendor_quotes_ui"], true);
        Ok(())
    }
}
