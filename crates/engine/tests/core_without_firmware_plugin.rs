//! Issue #980: the core engine builds and runs without the firmware target plugin.
//!
//! When compiled without the `plugin-firmware-target` feature, firmware commands
//! are not registered, executing them is refused, and output jobs with the
//! firmware_target kind fail with the expected message.

#![cfg(not(feature = "plugin-firmware-target"))]

use pcbcraft_engine::{CommandSpec, Session};
use serde_json::json;
use tempfile::tempdir;

fn has_command(commands: &[CommandSpec], id: &str) -> bool {
    commands.iter().any(|c| c.id == id)
}

#[test]
fn core_builds_without_firmware_plugin() -> Result<(), String> {
    let commands = pcbcraft_engine::commands();

    assert!(
        !has_command(&commands, "firmware.config.compare"),
        "firmware.config.compare must not be registered without the plugin"
    );
    assert!(
        !has_command(&commands, "firmware.target.export"),
        "firmware.target.export must not be registered without the plugin"
    );

    let mut session = Session::sample().map_err(|e| e.to_string())?;

    let result = session.execute("firmware.config.compare", json!({"config": "x"}), None);
    match result {
        Err(e) => {
            let err = e.to_string();
            assert!(
                err.contains("unknown command"),
                "expected unknown command error, got: {err}"
            );
        }
        Ok(_) => {
            return Err("expected firmware.config.compare to fail without the plugin".into());
        }
    }

    let dir = tempdir().map_err(|e| e.to_string())?;
    let dir_str = dir.path().to_string_lossy().to_string();

    let add_result = session.execute(
        "output_job.add",
        json!({"name": "fw", "kinds": ["firmware_target"]}),
        None,
    );
    if let Err(e) = add_result {
        let add_err = e.to_string();
        assert!(
            add_err.contains("firmware_target"),
            "output_job.add failed with unexpected error: {add_err}"
        );
        return Ok(());
    }

    let run_result = session.execute(
        "output_job.run",
        json!({"name": "fw", "directory": dir_str}),
        None,
    );
    match run_result {
        Err(e) => {
            let run_err = e.to_string();
            assert!(
                run_err.contains("unknown export kind"),
                "expected unknown export kind error, got: {run_err}"
            );
        }
        Ok(_) => {
            return Err("expected output_job.run to fail without the plugin".into());
        }
    }

    Ok(())
}
