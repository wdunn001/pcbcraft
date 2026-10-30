//! Read-only Snapmaker CNC artifacts: isolation, drill and outline G-code for a
//! `cnc_mill` machine profile.
//!
//! Context: each command takes a `profile_id` (a profile stored on the project, else a
//! built-in one) and a tool id from that profile's tool list, validates the numbers, calls
//! the writers in `crates/io/src/cam.rs` and `crates/io/src/cam_drill.rs`, and finishes the
//! text with `crates/io/src/snapmaker.rs`. Nothing here edits the project or its history.
//!
//! Local rules: every refusal is a plain sentence naming the value and its limit.
use super::*;
use pcbcraft_io::{
    DrillDialect, DrillMillTool, DrillOptions, DrillOrder, OutlineCompensation, OutlineOptions,
    Side, SnapmakerSettings, Spindle, Tabs, ToolChange,
};

const COMMANDS: [&str; 3] = [
    "cam.snapmaker_isolation",
    "cam.snapmaker_drill",
    "cam.snapmaker_outline",
];

/// True for the read-only Snapmaker artifact commands.
pub(super) fn handles(command: &str) -> bool {
    COMMANDS.contains(&command)
}

pub(super) fn commands() -> Vec<CommandSpec> {
    let common = json!({
        "profile_id":{"type":"string","minLength":1,"description":"a cnc_mill machine profile, stored on the project or built in"},
        "quick_swap":{"type":"boolean","default":false,"description":"the Quick Swap kit is fitted, so the Y work area is shorter"},
        "feed_mm_min":{"type":"number","exclusiveMinimum":0,"description":"cut feed overriding the profile tool"},
        "plunge_mm_min":{"type":"number","exclusiveMinimum":0,"description":"plunge feed overriding the profile tool"},
        "spindle_percent":{"type":"number","minimum":50,"maximum":100,"description":"spindle speed overriding the profile tool"}
    });
    let with = |extra: Value| {
        let mut props = common.clone();
        if let (Some(a), Some(b)) = (props.as_object_mut(), extra.as_object()) {
            for (k, v) in b {
                a.insert(k.clone(), v.clone());
            }
        }
        props
    };
    vec![
        CommandSpec {
            id: "cam.snapmaker_isolation",
            description: "Isolation milling G-code for one copper side as a Snapmaker CNC file with header, M3 P percent and bounds checked against the profile; changes nothing in the project",
            input_schema: schema(
                with(json!({
                    "tool_id":{"type":"string","minLength":1},
                    "side":{"type":"string","enum":["top","bottom"]},
                    "depth_mm":{"type":"number","exclusiveMinimum":0,"description":"cut depth below the copper top, at most the board thickness"},
                    "passes":{"type":"integer","minimum":1,"maximum":8,"default":1},
                    "overlap_pct":{"type":"integer","minimum":0,"maximum":90,"default":50}
                })),
                &["profile_id", "tool_id", "side", "depth_mm"],
            ),
        },
        CommandSpec {
            id: "cam.snapmaker_drill",
            description: "Drill G-code as one Snapmaker CNC file per drill bit, each with its own header and bounds; changes nothing in the project",
            input_schema: schema(
                with(json!({
                    "tool_ids":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"string","minLength":1},"description":"drill bits from the profile tool list"}
                })),
                &["profile_id", "tool_ids"],
            ),
        },
        CommandSpec {
            id: "cam.snapmaker_outline",
            description: "Board outline routing G-code with tabs as a Snapmaker CNC file; changes nothing in the project",
            input_schema: schema(
                with(json!({
                    "tool_id":{"type":"string","minLength":1},
                    "pass_depth_mm":{"type":"number","exclusiveMinimum":0,"default":0.5,"description":"depth per pass, at most the board thickness"},
                    "tabs":{"type":"integer","minimum":0,"maximum":16,"default":4}
                })),
                &["profile_id", "tool_id"],
            ),
        },
    ]
}

fn text<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or_else(|| format!("{key} must be a non-empty name"))
}

fn positive_number(params: &Value, key: &str) -> Result<Option<f64>, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.0)
            .map(Some)
            .ok_or_else(|| format!("{key} must be greater than zero")),
    }
}

struct Pick {
    diameter_mm: f64,
    feed_mm_min: f64,
    plunge_mm_min: f64,
    spindle_percent: f64,
}

fn pick_tool(profile: &MachineProfile, id: &str, params: &Value) -> Result<Pick, String> {
    let tool = profile
        .mill
        .tool
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("profile {} has no tool named {id}", profile.id))?;
    let feed = positive_number(params, "feed_mm_min")?.unwrap_or(tool.feed_mm_min);
    let plunge = positive_number(params, "plunge_mm_min")?.unwrap_or(tool.plunge_mm_min);
    let percent = match params.get("spindle_percent") {
        None | Some(Value::Null) => tool.spindle_percent,
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| "spindle_percent must be a number".to_string())?,
    };
    if feed <= 0.0 || plunge <= 0.0 {
        return Err("feed and plunge must be greater than zero".into());
    }
    Ok(Pick {
        diameter_mm: tool.diameter_mm,
        feed_mm_min: feed,
        plunge_mm_min: plunge,
        spindle_percent: percent,
    })
}

fn lookup(p: &Project, params: &Value) -> Result<(MachineProfile, SnapmakerSettings), String> {
    let id = text(params, "profile_id")?;
    let profile = match p.machine_profiles.iter().find(|m| m.id == id) {
        Some(found) => found.clone(),
        None => pcbcraft_io::builtin_machine_profile(id)
            .map_err(|_| format!("no machine profile named {id}"))?,
    };
    let quick = params
        .get("quick_swap")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let settings = pcbcraft_io::snapmaker_settings(&profile, quick)?;
    Ok((profile, settings))
}

fn rpm_for(percent: f64, s: &SnapmakerSettings) -> u32 {
    (percent * s.max_rpm / 100.0).round().clamp(100.0, 60_000.0) as u32
}

fn percent_checked(percent: f64, s: &SnapmakerSettings) -> Result<u8, String> {
    if !percent.is_finite() {
        return Err("spindle percent must be a number".into());
    }
    pcbcraft_io::spindle_percent(Spindle::Percent(percent.round().clamp(0.0, 255.0) as u8), s)
}

fn files_json(files: Vec<(String, String)>, profile: &str) -> Value {
    let list: Vec<Value> = files
        .into_iter()
        .map(|(name, body)| json!({"name":name,"body":body}))
        .collect();
    json!({"profile_id":profile,"count":list.len(),"files":list})
}

/// Runs one Snapmaker artifact command.
///
/// Why: validation happens here, before any G-code is written, so a bad request leaves
/// the project and its history untouched.
///
/// # Rules
/// Cut depth may not pass the board thickness; feed and plunge must be positive; the
/// spindle percent must sit in the profile range (50 to 100 on the shipped profiles);
/// the tool must exist in the profile.
///
/// # Dependencies
/// `crates/io/src/snapmaker.rs`, `crates/io/src/cam.rs`, `crates/io/src/cam_drill.rs`.
///
/// # Do not
/// Mutate the project; this is a read command.
pub(super) fn run(p: &Project, command: &str, params: &Value) -> Result<Value, String> {
    let (profile, s) = lookup(p, params)?;
    match command {
        "cam.snapmaker_isolation" => {
            let tool = pick_tool(&profile, text(params, "tool_id")?, params)?;
            let depth = positive_number(params, "depth_mm")?
                .ok_or_else(|| "depth_mm must be greater than zero".to_string())?;
            if depth > p.board.thickness {
                return Err(format!(
                    "cut depth {depth} mm is deeper than the {} mm board",
                    p.board.thickness
                ));
            }
            let side = match params.get("side").and_then(Value::as_str) {
                Some("top") => Side::Top,
                Some("bottom") => Side::Bottom,
                _ => return Err("side must be top or bottom".into()),
            };
            let passes = params.get("passes").and_then(Value::as_u64).unwrap_or(1);
            if !(1..=8).contains(&passes) {
                return Err("passes must be 1 to 8".into());
            }
            let overlap = params
                .get("overlap_pct")
                .and_then(Value::as_u64)
                .unwrap_or(50);
            let percent = percent_checked(tool.spindle_percent, &s)?;
            let opts = pcbcraft_io::IsolationOptions {
                side,
                tool: pcbcraft_io::Tool {
                    diameter_mm: tool.diameter_mm,
                    feed_mm_min: tool.feed_mm_min,
                    plunge_mm_min: tool.plunge_mm_min,
                    depth_mm: depth,
                    passes: passes as u8,
                    spindle: Spindle::Percent(percent),
                },
                overlap_pct: overlap.min(255) as u8,
                rubout: None,
                mirror_axis: pcbcraft_io::CamMirrorAxis::Y,
                alignment_holes: false,
            };
            let out =
                pcbcraft_io::isolation_gcode(p, &opts, &pcbcraft_io::snapmaker_cnc_profile(&s))?;
            Ok(files_json(
                pcbcraft_io::snapmaker_files(out.files, &s)?,
                &s.profile_id,
            ))
        }
        "cam.snapmaker_drill" => {
            let ids = params
                .get("tool_ids")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty() && a.len() <= 16)
                .ok_or_else(|| "tool_ids must list 1 to 16 drill bits".to_string())?;
            let mut tools = Vec::new();
            for (i, id) in ids.iter().enumerate() {
                let id = id
                    .as_str()
                    .ok_or_else(|| "tool_ids must be names".to_string())?;
                let t = pick_tool(&profile, id, params)?;
                percent_checked(t.spindle_percent, &s)?;
                tools.push(DrillMillTool {
                    number: (i + 1) as u8,
                    diameter_mm: t.diameter_mm,
                    spindle_rpm: rpm_for(t.spindle_percent, &s),
                    cut_feed_mm_min: t.feed_mm_min,
                    plunge_feed_mm_min: t.plunge_mm_min,
                });
            }
            let options = DrillOptions {
                tools,
                peck_mm: None,
                order: DrillOrder::Nearest,
                alignment_holes: vec![],
                tool_change: ToolChange::NewFile,
            };
            let files = pcbcraft_io::drill_gcode(p, &options, &mill_profile(&s))?;
            Ok(files_json(
                pcbcraft_io::snapmaker_files(files, &s)?,
                &s.profile_id,
            ))
        }
        _ => {
            let tool = pick_tool(&profile, text(params, "tool_id")?, params)?;
            let pass = positive_number(params, "pass_depth_mm")?.unwrap_or(0.5);
            if pass > p.board.thickness {
                return Err(format!(
                    "pass depth {pass} mm is deeper than the {} mm board",
                    p.board.thickness
                ));
            }
            let percent = percent_checked(tool.spindle_percent, &s)?;
            let tabs = params.get("tabs").and_then(Value::as_u64).unwrap_or(4);
            if tabs > 16 {
                return Err("tabs must be 0 to 16".into());
            }
            let options = OutlineOptions {
                tool: DrillMillTool {
                    number: 1,
                    diameter_mm: tool.diameter_mm,
                    spindle_rpm: rpm_for(f64::from(percent), &s),
                    cut_feed_mm_min: tool.feed_mm_min,
                    plunge_feed_mm_min: tool.plunge_mm_min,
                },
                pass_depth_mm: pass,
                tabs: Tabs {
                    count: tabs as u8,
                    width_mm: 3.0,
                    height_mm: 0.6_f64.min(p.board.thickness),
                },
                compensation: OutlineCompensation::Outside,
                cutouts: false,
            };
            let text = pcbcraft_io::outline_gcode(p, &options, &mill_profile(&s))?;
            Ok(files_json(
                pcbcraft_io::snapmaker_files(vec![("board-outline.nc".into(), text)], &s)?,
                &s.profile_id,
            ))
        }
    }
}

fn mill_profile(s: &SnapmakerSettings) -> pcbcraft_io::CamProfile {
    pcbcraft_io::CamProfile {
        dialect: DrillDialect::Grbl,
        work_x_mm: s.work_x_mm,
        work_y_mm: s.work_y_mm,
        safe_z_mm: s.safe_z_mm,
        retract_z_mm: (s.safe_z_mm / 2.5).max(0.1),
        direction: pcbcraft_io::CutDirection::Conventional,
        plating_allowance_mm: 0.0,
        fab_plated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_track() -> Result<Session, Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("no net id".into()))?;
        s.execute(
            "track.add",
            json!({"net":net,"start":[5.0,5.0],"end":[15.0,5.0],"width":0.3,"layer":"F.Cu"}),
            None,
        )?;
        Ok(s)
    }

    const A350T: &str = "snapmaker-2-a350t-200w-cnc";

    #[test]
    fn cam_snapmaker_artifact_refuses_depth_past_board_thickness() -> Result<(), Error> {
        let mut s = session_with_track()?;
        let thickness = s.project.board.thickness;
        let before = s.revision;
        let refused = s.execute(
            "cam.snapmaker_isolation",
            json!({"profile_id":A350T,"tool_id":"iso-0.2","side":"top","depth_mm":thickness + 0.5}),
            None,
        );
        assert!(refused.is_err_and(|e| e.to_string().contains("deeper than")));
        assert_eq!(s.revision, before);
        Ok(())
    }

    #[test]
    fn cam_snapmaker_artifact_refuses_zero_feed() -> Result<(), Error> {
        let mut s = session_with_track()?;
        let refused = s.execute(
            "cam.snapmaker_isolation",
            json!({"profile_id":A350T,"tool_id":"iso-0.2","side":"top","depth_mm":0.08,"feed_mm_min":0.0}),
            None,
        );
        assert!(refused.is_err());
        Ok(())
    }

    #[test]
    fn cam_snapmaker_artifact_refuses_spindle_percent_40() -> Result<(), Error> {
        let mut s = session_with_track()?;
        let refused = s.execute(
            "cam.snapmaker_isolation",
            json!({"profile_id":A350T,"tool_id":"iso-0.2","side":"top","depth_mm":0.08,"spindle_percent":40.0}),
            None,
        );
        assert!(refused.is_err_and(|e| e.to_string().contains("50 to 100")));
        Ok(())
    }

    #[test]
    fn cam_snapmaker_artifact_refuses_unknown_tool_and_profile() -> Result<(), Error> {
        let mut s = session_with_track()?;
        let tool = s.execute(
            "cam.snapmaker_isolation",
            json!({"profile_id":A350T,"tool_id":"nope","side":"top","depth_mm":0.08}),
            None,
        );
        assert!(tool.is_err_and(|e| e.to_string().contains("no tool named nope")));
        let profile = s.execute(
            "cam.snapmaker_outline",
            json!({"profile_id":"missing","tool_id":"iso-0.2"}),
            None,
        );
        assert!(profile.is_err());
        Ok(())
    }

    #[test]
    fn cam_snapmaker_isolation_returns_header_file_without_editing_history() -> Result<(), Error> {
        let mut s = session_with_track()?;
        let before = s.revision;
        let out = s.execute(
            "cam.snapmaker_isolation",
            json!({"profile_id":A350T,"tool_id":"iso-0.2","side":"top","depth_mm":0.08}),
            None,
        )?;
        assert_eq!(s.revision, before);
        let body = out["result"]["files"][0]["body"]
            .as_str()
            .ok_or_else(|| Error::Invalid("no file".into()))?;
        assert!(body.starts_with(";Header Start\n"));
        assert!(body.contains("M3 P100\n"));
        assert!(body.ends_with("M400\nM5\n"));
        Ok(())
    }
}
