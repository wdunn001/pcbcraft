//! Snapmaker CNC dialect: wraps plain mill G-code into a file the touchscreen accepts.
//!
//! Context: the isolation, drill and outline writers in `cam.rs` and `cam_drill.rs` emit
//! generic G-code. This module reads that text, refuses anything the machine cannot run
//! (arcs, a coordinate outside the work area, a spindle outside 50 to 100 percent, a tool
//! change inside one file) and rewrites it as a Snapmaker file: comment header with real
//! bounds, line count and time estimate, `G90 G21 G17`, `M3 P<percent>`, `M400` then `M5`.
//! The settings come from a `cnc_mill` machine profile (`crates/doc/src/machine_profile.rs`).
//!
//! Local rules: output is a pure function of its input (no timestamps); every number in
//! the header is measured from the body that follows it.
use crate::cam::{CncProfile, Dialect, Spindle, Tool};
use pcbcraft_doc::MachineProfile;
use pcbcraft_geom::Point;

const MAX_LINES: usize = 2_000_000;

/// Machine facts a Snapmaker file needs, read once from a mill profile.
///
/// Why: the writers should not each re-read the profile TOML shape.
///
/// # Rules
/// Work size already has the Quick Swap trim applied when `quick_swap` was true.
///
/// # Dependencies
/// `crates/doc/src/machine_profile.rs` (`MachineProfile`, `GcodeSection`, `SpindleSection`).
///
/// # Do not
/// Build one by hand for a real job; use `snapmaker_settings` so validation runs.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapmakerSettings {
    /// Id of the profile these values came from, quoted in refusal messages.
    pub profile_id: String,
    /// Value of the header `header_type` line the touchscreen checks, normally `cnc`.
    pub header_type: String,
    /// Tool head name written in the header; the screen refuses a file whose head does not match
    /// the module fitted.
    pub tool_head: String,
    /// Machine model name written in the header.
    pub machine: String,
    /// G-code flavour name written in the header, `marlin`.
    pub flavor: String,
    /// Reach in X; points beyond it are refused.
    pub work_x_mm: f64,
    /// Reach in Y, already shortened to the Quick Swap length when that kit is selected.
    pub work_y_mm: f64,
    /// Reach in Z; cuts deeper than this are refused.
    pub work_z_mm: f64,
    /// Cutting feed quoted in the header as `work_speed`.
    pub work_speed_mm_min: f64,
    /// Rapid feed quoted in the header as `jog_speed`.
    pub jog_speed_mm_min: f64,
    /// Travel height between cuts.
    pub safe_z_mm: f64,
    /// Spindle speed at 100 percent, used to turn RPM into `P`.
    pub max_rpm: f64,
    /// Lowest spindle percent the machine will run (50 for the 200 W module).
    pub min_percent: u8,
    /// Highest spindle percent (100).
    pub max_percent: u8,
    /// Profile lines written after the header, before the first move.
    pub preamble: Vec<String>,
    /// Profile lines written after the closing `M400` and `M5`.
    pub postamble: Vec<String>,
}

/// Reads a Snapmaker settings block from a `cnc_mill` profile.
///
/// Why: refuse early, with a plain reason, when the profile is not a Snapmaker mill.
///
/// # Rules
/// Arcs are never allowed here whatever `arcs_allowed` says, and `tool_change` must be
/// `new_file` because the machine has no tool change command.
///
/// # Dependencies
/// `crates/doc/src/machine_profile.rs`.
///
/// # Do not
/// Skip `profile.validate()`; it holds the 50 to 100 percent spindle rule.
pub fn snapmaker_settings(
    profile: &MachineProfile,
    quick_swap: bool,
) -> Result<SnapmakerSettings, String> {
    profile.validate()?;
    if profile.kind != "cnc_mill" || profile.output.dialect != "snapmaker" {
        return Err(format!(
            "profile {} is not a Snapmaker CNC mill profile",
            profile.id
        ));
    }
    let gcode = profile
        .mill
        .gcode
        .as_ref()
        .ok_or_else(|| format!("profile {} has no G-code settings", profile.id))?;
    let spindle = profile
        .mill
        .spindle
        .as_ref()
        .ok_or_else(|| format!("profile {} has no spindle settings", profile.id))?;
    if gcode.tool_change != "new_file" {
        return Err(format!(
            "profile {} must write one file per tool (tool_change = new_file)",
            profile.id
        ));
    }
    if spindle.max_rpm <= 0.0 || !spindle.max_rpm.is_finite() {
        return Err(format!("profile {} needs a positive max_rpm", profile.id));
    }
    let work = &profile.work_area;
    let y = match (quick_swap, work.y_with_quick_swap) {
        (true, Some(trimmed)) => trimmed,
        (true, None) => {
            return Err(format!(
                "profile {} has no Quick Swap work area",
                profile.id
            ));
        }
        (false, _) => work.y,
    };
    let preamble = if gcode.preamble.is_empty() {
        vec!["G90".to_string(), "G21".into(), "G17".into()]
    } else {
        gcode.preamble.clone()
    };
    Ok(SnapmakerSettings {
        profile_id: profile.id.clone(),
        header_type: if gcode.header_type.is_empty() {
            "cnc".into()
        } else {
            gcode.header_type.clone()
        },
        tool_head: gcode.tool_head.clone(),
        machine: gcode.machine_label.clone(),
        flavor: if gcode.flavor.is_empty() {
            "marlin".into()
        } else {
            gcode.flavor.clone()
        },
        work_x_mm: work.x,
        work_y_mm: y,
        work_z_mm: work.z.unwrap_or(work.x.min(y)),
        work_speed_mm_min: if gcode.work_speed_mm_min > 0.0 {
            gcode.work_speed_mm_min
        } else {
            300.0
        },
        jog_speed_mm_min: if gcode.jog_speed_mm_min > 0.0 {
            gcode.jog_speed_mm_min
        } else {
            1500.0
        },
        safe_z_mm: if gcode.safe_z_mm > 0.0 {
            gcode.safe_z_mm
        } else {
            5.0
        },
        max_rpm: spindle.max_rpm,
        min_percent: spindle.min_percent.round().clamp(50.0, 100.0) as u8,
        max_percent: spindle.max_percent.round().clamp(50.0, 100.0) as u8,
        preamble,
        postamble: if gcode.postamble.is_empty() {
            vec!["M400".to_string(), "M5".into()]
        } else {
            gcode.postamble.clone()
        },
    })
}

/// Builds the isolation writer profile for a Snapmaker mill.
///
/// Why: isolation checks every point against the work area while it writes.
///
/// # Dependencies
/// `crates/io/src/cam.rs` (`CncProfile`).
///
/// # Do not
/// Use it for drill or outline; those take `CamProfile` from `cam_drill.rs`.
pub fn snapmaker_cnc_profile(s: &SnapmakerSettings) -> CncProfile {
    CncProfile {
        dialect: Dialect::Snapmaker,
        work_min: Point::new(0.0, 0.0),
        work_max: Point::new(s.work_x_mm, s.work_y_mm),
        safe_z_mm: s.safe_z_mm,
        jog_mm_min: s.jog_speed_mm_min,
    }
}

/// Converts a spindle setting to the `P` percent a Snapmaker reads.
///
/// Why: the 200 W module takes percent of 18000 RPM and clamps an `S` word above 100
/// itself, so RPM must never reach the file.
///
/// # Rules
/// RPM becomes `round(rpm * 100 / max_rpm)` clamped to 100. A result under the profile
/// minimum is refused, not raised.
///
/// # Do not
/// Write an `S` word on this dialect.
pub fn spindle_percent(spindle: Spindle, s: &SnapmakerSettings) -> Result<u8, String> {
    let pct = match spindle {
        Spindle::Percent(p) => f64::from(p),
        Spindle::Rpm(r) => (f64::from(r) * 100.0 / s.max_rpm).round().min(100.0),
    };
    if pct < f64::from(s.min_percent) || pct > f64::from(s.max_percent) {
        return Err(format!(
            "spindle {:.0} percent is outside {} to {} percent for profile {}",
            pct, s.min_percent, s.max_percent, s.profile_id
        ));
    }
    Ok(pct as u8)
}

/// Spindle percent for one isolation tool.
///
/// # Dependencies
/// `crates/io/src/cam.rs` (`Tool`, `Spindle`).
pub fn spindle_percent_for_tool(tool: &Tool, s: &SnapmakerSettings) -> Result<u8, String> {
    spindle_percent(tool.spindle, s)
}

fn num(v: f64) -> String {
    let t = format!("{v:.3}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-0" {
        "0".into()
    } else {
        t.into()
    }
}

fn word(line: &str, letter: char) -> Option<f64> {
    line.split_whitespace().find_map(|w| {
        let mut chars = w.chars();
        let first = chars.next()?;
        if first.eq_ignore_ascii_case(&letter) {
            chars.as_str().parse::<f64>().ok()
        } else {
            None
        }
    })
}

/// Per-axis extremes of the coordinates a program actually visits.
struct Bounds {
    min: [Option<f64>; 3],
    max: [Option<f64>; 3],
}

impl Bounds {
    fn note(&mut self, axis: usize, v: f64) {
        self.min[axis] = Some(self.min[axis].map_or(v, |m| m.min(v)));
        self.max[axis] = Some(self.max[axis].map_or(v, |m| m.max(v)));
    }
}

fn check_axis(
    name: &str,
    value: f64,
    low: f64,
    high: f64,
    s: &SnapmakerSettings,
    line_no: usize,
) -> Result<(), String> {
    if !value.is_finite() || value < low - 1e-9 || value > high + 1e-9 {
        return Err(format!(
            "{name} {} mm on line {line_no} is outside the {} to {} mm work area of profile {}",
            num(value),
            num(low),
            num(high),
            s.profile_id
        ));
    }
    Ok(())
}

/// Rewrites one generic G-code file as a Snapmaker file.
///
/// Why: the machine rejects files without the header and spins at the wrong speed when
/// handed RPM; the header numbers must come from the body that follows.
///
/// # Rules
/// Drops generator preamble, `M2`, `M5` and `M400` lines and adds the profile ones.
/// Refuses `G2`/`G3`, `G91`, `M0`, `M6`, an `M3` without a speed, a coordinate outside
/// `[0, work]` on X and Y or deeper than the work height on Z, and spindle percent outside
/// the profile range. `file_total_lines` counts every line of the returned text.
///
/// # Dependencies
/// `crates/io/src/cam.rs`, `crates/io/src/cam_drill.rs` produce the input.
///
/// # Do not
/// Add timestamps or anything else that changes between runs.
pub fn snapmaker_finalize(input: &str, s: &SnapmakerSettings) -> Result<String, String> {
    let mut body: Vec<String> = Vec::new();
    let mut bounds = Bounds {
        min: [None; 3],
        max: [None; 3],
    };
    let mut at: [Option<f64>; 3] = [None; 3];
    let mut feed = s.work_speed_mm_min;
    let mut seconds = 0.0_f64;
    for (i, raw) in input.lines().enumerate() {
        if i >= MAX_LINES {
            return Err("G-code is too long".into());
        }
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix(';') {
            let rest = rest.trim();
            if rest.starts_with("estimate_s=") || rest.starts_with("Snapmaker CNC") {
                continue;
            }
            body.push(format!(";{rest}"));
            continue;
        }
        if let Some(rest) = line.strip_prefix('(') {
            body.push(format!(";{}", rest.trim_end_matches(')').trim()));
            continue;
        }
        let code = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        match code.as_str() {
            "G90" | "G21" | "G17" | "M2" | "M30" | "M5" | "M400" => {}
            "G91" => return Err(format!("relative moves (line {line_no}) are not supported")),
            "G2" | "G02" | "G3" | "G03" => {
                return Err(format!(
                    "arc on line {line_no}: this machine takes straight moves only"
                ));
            }
            "M0" | "M6" => {
                return Err(format!(
                    "tool change on line {line_no}: write one file per tool"
                ));
            }
            "M3" => {
                let spindle = if let Some(p) = word(line, 'P') {
                    Spindle::Percent(p.round().clamp(0.0, 255.0) as u8)
                } else if let Some(r) = word(line, 'S') {
                    Spindle::Rpm(r.round().clamp(0.0, 4_000_000.0) as u32)
                } else {
                    return Err(format!("spindle start on line {line_no} has no speed"));
                };
                body.push(format!("M3 P{}", spindle_percent(spindle, s)?));
            }
            "G0" | "G00" | "G1" | "G01" => {
                let rapid = code == "G0" || code == "G00";
                let target = [word(line, 'X'), word(line, 'Y'), word(line, 'Z')];
                if let Some(v) = target[0] {
                    check_axis("X", v, 0.0, s.work_x_mm, s, line_no)?;
                }
                if let Some(v) = target[1] {
                    check_axis("Y", v, 0.0, s.work_y_mm, s, line_no)?;
                }
                if let Some(v) = target[2] {
                    check_axis("Z", v, -s.work_z_mm, s.work_z_mm, s, line_no)?;
                }
                let given = word(line, 'F');
                if rapid {
                    feed = given.unwrap_or(s.jog_speed_mm_min);
                } else if let Some(f) = given {
                    feed = f;
                }
                if !(feed.is_finite() && feed > 0.0) {
                    return Err(format!("feed on line {line_no} must be positive"));
                }
                let mut squared = 0.0;
                for axis in 0..3 {
                    if let Some(v) = target[axis] {
                        if let Some(old) = at[axis] {
                            squared += (v - old).powi(2);
                        }
                        at[axis] = Some(v);
                        bounds.note(axis, v);
                    }
                }
                seconds += squared.sqrt() / feed * 60.0;
                let mut out = format!("{} ", if rapid { "G0" } else { "G1" });
                let mut parts = Vec::new();
                for (letter, v) in [
                    ('X', word(line, 'X')),
                    ('Y', word(line, 'Y')),
                    ('Z', word(line, 'Z')),
                ] {
                    if let Some(v) = v {
                        parts.push(format!("{letter}{}", num(v)));
                    }
                }
                parts.push(format!("F{}", num(feed)));
                out.push_str(&parts.join(" "));
                body.push(out);
            }
            _ => body.push(line.to_string()),
        }
    }
    let mut lines: Vec<String> = Vec::new();
    lines.push(";Header Start".into());
    lines.push(format!(";header_type: {}", s.header_type));
    lines.push(format!(";tool_head: {}", s.tool_head));
    lines.push(format!(";machine: {}", s.machine));
    lines.push(format!(";gcode_flavor: {}", s.flavor));
    lines.push(";renderMethod: line".into());
    let total_index = lines.len();
    lines.push(String::new());
    let time_index = lines.len();
    lines.push(String::new());
    lines.push(";is_rotate: false".into());
    for (i, a) in ["x", "y", "z"].iter().enumerate() {
        lines.push(format!(
            ";max_{a}(mm): {}",
            num(bounds.max[i].unwrap_or(0.0))
        ));
    }
    for (i, a) in ["x", "y", "z"].iter().enumerate() {
        lines.push(format!(
            ";min_{a}(mm): {}",
            num(bounds.min[i].unwrap_or(0.0))
        ));
    }
    lines.push(format!(
        ";work_speed(mm/minute): {}",
        num(s.work_speed_mm_min)
    ));
    lines.push(format!(
        ";jog_speed(mm/minute): {}",
        num(s.jog_speed_mm_min)
    ));
    lines.push(format!(";work_size_x: {}", num(s.work_x_mm)));
    lines.push(format!(";work_size_y: {}", num(s.work_y_mm)));
    lines.push(";origin: lower-left".into());
    lines.push(";Header End".into());
    lines.extend(s.preamble.iter().cloned());
    lines.extend(body);
    lines.extend(s.postamble.iter().cloned());
    lines[total_index] = format!(";file_total_lines: {}", lines.len());
    lines[time_index] = format!(";estimated_time(s): {}", seconds.round() as u64);
    Ok(lines.join("\n") + "\n")
}

/// Finalises every file of one job; one input file per tool gives one output file.
///
/// # Dependencies
/// `snapmaker_finalize` in this file.
///
/// # Do not
/// Merge files: the machine has no tool change, so each tool keeps its own file.
pub fn snapmaker_files(
    files: Vec<(String, String)>,
    s: &SnapmakerSettings,
) -> Result<Vec<(String, String)>, String> {
    files
        .into_iter()
        .map(|(name, text)| Ok((name, snapmaker_finalize(&text, s)?)))
        .collect()
}
