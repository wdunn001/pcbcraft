//! Pins the format registry (`crates/io/src/formats.rs`) to the commands and tools it names.
//!
//! Context: issue #194. A row that names a command, tool, output kind or parameter that
//! does not exist would make a dialog fail only when someone picks it, so these tests
//! fail the build first. They sit in the mcp crate because it sees both the engine
//! commands and the tool list.
use super::*;
use pcbcraft_io::formats::{FORMATS, OptionSpec, Run};

/// The property names of a schema, or none.
fn properties(schema: &Value) -> Vec<String> {
    schema
        .pointer("/properties")
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// The input schema of a tool the server lists, by name.
fn tool_schema(name: &str) -> Option<Value> {
    tool_list()["tools"]
        .as_array()?
        .iter()
        .find(|t| t["name"] == name)
        .map(|t| t["inputSchema"].clone())
}

fn check(id: &str, run: Run, options: &[OptionSpec]) -> Result<(), String> {
    let schema = match run {
        Run::Engine(command) | Run::Tool(command) => {
            tool_schema(command).ok_or(format!("{id}: no command or tool named {command}"))?
        }
        Run::Kind(kind) => {
            let mut s = Session::default();
            s.execute(
                "output_job.add",
                json!({"name": "pin", "kinds": [kind]}),
                None,
            )
            .map_err(|e| format!("{id}: {kind} is not an output kind: {e}"))?;
            return Ok(());
        }
    };
    let known = properties(&schema);
    for option in options {
        if let Some(param) = option.param
            && !known.iter().any(|k| k == param)
        {
            return Err(format!(
                "{id}: `{param}` is not a documented parameter ({known:?})"
            ));
        }
    }
    Ok(())
}

#[test]
fn every_format_names_a_real_command_and_documented_parameters() -> Result<(), String> {
    for f in FORMATS {
        for run in [f.open, f.import, f.export].into_iter().flatten() {
            check(f.id, run, f.options)?;
        }
    }
    Ok(())
}

#[test]
fn the_bill_of_materials_row_maps_onto_the_export_command() -> Result<(), String> {
    let bom = FORMATS
        .iter()
        .find(|f| f.id == "bom")
        .ok_or("no bill of materials row")?;
    assert_eq!(bom.export, Some(Run::Engine("bom.export")));
    let known = properties(&tool_schema("bom.export").ok_or("no bom.export")?);
    for param in ["variant", "fields", "group", "ids"] {
        assert!(known.iter().any(|k| k == param), "bom.export lacks {param}");
    }
    Ok(())
}
