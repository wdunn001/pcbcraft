//! Explicit document format migrations.
//!
//! A design file records the format version it was written in. Reading runs the
//! migration chain on the parsed JSON, before typed deserialisation, so every
//! entry point (opening a file, reading an archive, recovering an autosave and
//! the command line) accepts the same files and refuses the same ones in the
//! same words. Each step is a pure rewrite from one version to the next, which
//! keeps old fixtures small and every step testable on its own.
//!
//! The rule for when a version bump is needed lives in `docs/architecture.md`:
//! adding an optional field with a serde default needs no bump; renaming a
//! field, changing its unit or removing it does, and each bump ships with a
//! fixture of the old version under test.
use pcbcraft_doc::Project;
use serde_json::Value;

/// The format version this build writes and reads.
pub const CURRENT_VERSION: u32 = 1;

/// One step of the chain: the version it reads, and the rewrite that turns that
/// version's JSON into the next one's.
type Step = (u32, fn(Value) -> Result<Value, String>);

/// Every released step, oldest first. Version 1 is the first format this
/// project wrote, so the chain is empty until a change needs version 2.
const STEPS: &[Step] = &[];

/// Bring a design's JSON up to [`CURRENT_VERSION`].
///
/// A file from a newer build is refused in plain words rather than failing
/// later with a missing-field message, and so is a version this chain cannot
/// reach. The returned value is ready for typed deserialisation.
pub fn migrate(value: Value) -> Result<Value, String> {
    migrate_to(value, STEPS, CURRENT_VERSION)
}

/// The chain walker. `steps` and `target` are arguments so a test can prove the
/// walk goes one version at a time and reports a gap, without inventing a
/// released format.
fn migrate_to(value: Value, steps: &[Step], target: u32) -> Result<Value, String> {
    let Some(object) = value.as_object() else {
        return Err("a design file holds a JSON object".into());
    };
    if object.get("format").and_then(Value::as_str) != Some("pcbcraft") {
        return Err("this file is not a PCBCraft design".into());
    }
    let Some(recorded) = object.get("version").and_then(Value::as_u64) else {
        return Err("this design records no format version, so it cannot be read".into());
    };
    if recorded == 0 {
        return Err("this design records format 0, which no PCBCraft wrote".into());
    }
    let Ok(mut version) = u32::try_from(recorded) else {
        return Err(format!(
            "this design records format {recorded}, which is not a format version"
        ));
    };
    if version > target {
        return Err(format!(
            "this design was made by a newer PCBCraft (format {version}); this build reads up to format {target}"
        ));
    }
    let mut value = value;
    while version < target {
        let Some((_, step)) = steps.iter().find(|(from, _)| *from == version) else {
            return Err(format!(
                "no migration is known from format {version} to {}",
                version + 1
            ));
        };
        value = step(value)?;
        version += 1;
        let Some(object) = value.as_object_mut() else {
            return Err("a migration step must keep the design a JSON object".into());
        };
        object.insert("version".into(), Value::from(version));
    }
    Ok(value)
}

/// Read a design from its file text: migrate, deserialise, then validate.
///
/// This is the one place that turns text into a `Project`, so a new entry point
/// gets the migration chain by calling it.
pub fn read_project(text: &str) -> Result<Project, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let project: Project = serde_json::from_value(migrate(value)?).map_err(|e| e.to_string())?;
    project.validate()?;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal() -> Value {
match serde_json::to_value(Project::default()) {
            Ok(value) => value,
            Err(_) => Value::Null,
        }
    }

    fn with_version(version: u32) -> Value {
        let mut value = minimal();
        if let Some(object) = value.as_object_mut() {
            object.insert("version".into(), json!(version));
        }
        value
    }

    fn second(mut value: Value) -> Result<Value, String> {
        let object = value
            .as_object_mut()
            .ok_or_else(|| "an object".to_string())?;
        object.insert("second".into(), json!(true));
        Ok(value)
    }

    fn third(mut value: Value) -> Result<Value, String> {
        let object = value
            .as_object_mut()
            .ok_or_else(|| "an object".to_string())?;
        let seen = object.get("second").and_then(Value::as_bool) == Some(true);
        object.insert("third".into(), json!(seen));
        Ok(value)
    }

    #[test]
    fn loading_a_version_1_file_migrates_to_current() -> Result<(), String> {
        let text = serde_json::to_string(&Project::default()).map_err(|e| e.to_string())?;
        let project = read_project(&text)?;
        assert_eq!(project.version, CURRENT_VERSION);
        assert_eq!(project.format, "pcbcraft");
        Ok(())
    }

    #[test]
    fn a_newer_version_is_refused_in_plain_words() -> Result<(), String> {
        let value = with_version(CURRENT_VERSION + 7);
        let text = serde_json::to_string(&value).map_err(|e| e.to_string())?;
        let error = match read_project(&text) {
            Ok(_) => return Err("a newer design should be refused".into()),
            Err(error) => error,
        };
        assert!(error.contains("made by a newer PCBCraft"), "{error}");
        assert!(
            error.contains(&format!("format {}", CURRENT_VERSION + 7)),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn a_file_that_is_not_a_design_is_refused() -> Result<(), String> {
        let mut value = minimal();
        if let Some(object) = value.as_object_mut() {
            object.insert("format".into(), json!("other"));
        }
        let text = serde_json::to_string(&value).map_err(|e| e.to_string())?;
        let error = match read_project(&text) {
            Ok(_) => return Err("another format tag should be refused".into()),
            Err(error) => error,
        };
        assert!(error.contains("not a PCBCraft design"), "{error}");
        assert!(read_project("{\"name\":\"no format\"}").is_err());
        let error = match read_project("{\"format\":\"pcbcraft\",\"name\":\"no version\"}") {
            Ok(_) => return Err("a design without a version should be refused".into()),
            Err(error) => error,
        };
        assert!(error.contains("no format version"), "{error}");
        assert!(read_project("not json").is_err());
        Ok(())
    }

    /// A two-step chain from format 1 to format 3: version 3 is not released,
    /// so this runs the walker alone.
    #[test]
    fn the_chain_walks_one_version_at_a_time() -> Result<(), String> {
        let steps: &[Step] = &[(1, second), (2, third)];
        let walked = migrate_to(with_version(1), steps, 3)?;
        assert_eq!(walked.get("version").and_then(Value::as_u64), Some(3));
        assert_eq!(walked.get("third").and_then(Value::as_bool), Some(true));
        // Already there: every field comes back as it went in.
        let current = with_version(3);
        assert_eq!(migrate_to(current.clone(), steps, 3)?, current);
        Ok(())
    }

    #[test]
    fn a_gap_in_the_chain_is_reported() -> Result<(), String> {
        let steps: &[Step] = &[(1, second)];
        let error = match migrate_to(with_version(1), steps, 3) {
            Ok(_) => return Err("format 2 has no step, so the walk should fail".into()),
            Err(error) => error,
        };
        assert!(
            error.contains("no migration is known from format 2 to 3"),
            "{error}"
        );
        Ok(())
    }
}
