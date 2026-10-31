//! Gerber job file: the JSON manifest that lists every file in a fabrication
//! package.
//!
//! Field names follow the Ucamco Gerber job file specification, revision
//! 2020.08. `Header` and `GeneralSpecs` are section 4.2, `DesignRules` is 4.3,
//! `FilesAttributes` is 4.4 and `MaterialStackup` is 4.5. Each
//! `FilesAttributes` entry carries `Path`, `FileFunction` and `FilePolarity`,
//! and the file function values are the ones the specification defines:
//! `Copper,L1,Top` for a copper layer, `Profile,NP` for the board outline and
//! `Plated,1,2,PTH` for a plated drill file.
//!
//! Nothing in the manifest is typed by hand. Every entry is read back from the
//! file it describes: the `%TF.FileFunction` and `%TF.FilePolarity` attributes
//! of a Gerber file, or, for a file written before those attributes existed,
//! the description PCBCraft stamps into its `G04` comment. A layer added to the
//! package therefore appears in the manifest with no other change.
use crate::{COPPER_THICKNESS, MASK_THICKNESS, SILK_THICKNESS};
use pcbcraft_doc::Project;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Most files one fabrication package may list.
const MAX_FILES: usize = 256;

/// Gerber job file for a package, as JSON. The creation date is the current
/// UTC time; every other field is derived from the project and the files.
pub fn gerber_job(p: &Project, files: &BTreeMap<String, String>) -> Result<String, String> {
    gerber_job_at(p, files, &now_utc())
}

/// Gerber job file with an explicit creation date, so tests and callers that
/// already know the timestamp get a byte-identical manifest.
pub(crate) fn gerber_job_at(
    p: &Project,
    files: &BTreeMap<String, String>,
    creation: &str,
) -> Result<String, String> {
    p.validate()?;
    if files.len() > MAX_FILES {
        return Err(format!(
            "a fabrication package holds at most {MAX_FILES} files"
        ));
    }
    for name in files.keys() {
        if name.is_empty() || name.len() > 255 {
            return Err("fabrication file names must be 1 to 255 bytes".into());
        }
    }
    // The drill file names the copper span it drills, so the copper count is
    // needed before any file is described.
    let copper = files
        .iter()
        .filter(|(name, text)| {
            !name.ends_with(".gbrjob") && file_function(name, text, 1).0.starts_with("Copper,")
        })
        .count()
        .max(1);
    let mut attributes: Vec<Value> = Vec::new();
    for (name, text) in files {
        // The manifest never lists itself, so it stays valid when it is read
        // back out of the package it was added to.
        if name.ends_with(".gbrjob") {
            continue;
        }
        let (function, polarity) = file_function(name, text, copper);
        attributes.push(json!({
            "Path": name,
            "FileFunction": function,
            "FilePolarity": polarity,
        }));
    }
    let rules = design_rules(p);
    let job = json!({
        "Header": {
            "GenerationSoftware": {
                "Vendor": "Tangency Labs",
                "Application": "PCBCraft",
                "Version": env!("CARGO_PKG_VERSION"),
            },
            "CreationDate": creation,
        },
        "GeneralSpecs": {
            "ProjectId": { "Name": p.name },
            "Size": { "X": p.board.width, "Y": p.board.height },
            "LayerNumber": copper,
            "BoardThickness": p.board.thickness,
            "Finish": p.board.finish,
        },
        "DesignRules": rules,
        "FilesAttributes": attributes,
        "MaterialStackup": material_stackup(p, copper),
    });
    serde_json::to_string_pretty(&job).map_err(|e| e.to_string())
}

/// `DesignRules` section (specification 4.3), in millimetres. The board
/// clearance is the baseline for every separation. When the board records
/// fabrication limits, `min_space` replaces that separation and `min_track`
/// becomes `MinLineWidth`. Fields the board does not record are left out, which
/// the specification allows for a partial job file.
///
/// # Rules
/// Exports what the user recorded; it never runs a check.
///
/// # Dependencies
/// `FabRules` in `crates/doc/src/fab.rs`.
///
/// # Do not
/// Invent a limit the board does not carry.
fn design_rules(p: &Project) -> Value {
    let fab = p.board.rules.as_ref();
    let space = fab.and_then(|r| r.min_space).unwrap_or(p.board.clearance);
    let mut rule = json!({
        "Layers": "All",
        "PadToPad": space,
        "PadToTrack": space,
        "TrackToTrack": space,
        "TrackToRegion": space,
        "RegionToRegion": space,
    });
    if let (Some(width), Some(map)) = (fab.and_then(|r| r.min_track), rule.as_object_mut()) {
        map.insert("MinLineWidth".into(), json!(width));
    }
    json!([rule])
}

/// The stackup in fabrication order: legend and mask on the top face, each
/// copper layer with the dielectric under it, then mask and legend on the
/// bottom face. Thicknesses come from the board and the shared layer
/// constants, so the manifest cannot disagree with the 3D view.
fn material_stackup(p: &Project, copper: usize) -> Value {
    let mut stack: Vec<Value> = vec![
        json!({ "Type": "Legend", "Name": "Top Silk Screen", "Thickness": SILK_THICKNESS }),
        json!({ "Type": "SolderMask", "Name": "Top Solder Mask", "Thickness": MASK_THICKNESS }),
    ];
    let gaps = (copper - 1).max(1) as f64;
    let dielectric = ((p.board.thickness - COPPER_THICKNESS * copper as f64) / gaps).max(0.0);
    for layer in 0..copper {
        let name = if copper == 1 || layer == 0 {
            "F.Cu".to_owned()
        } else if layer + 1 == copper {
            "B.Cu".to_owned()
        } else {
            format!("In{layer}.Cu")
        };
        stack.push(json!({
            "Type": "Copper",
            "Name": name,
            "Thickness": COPPER_THICKNESS,
        }));
        if layer + 1 < copper {
            stack.push(json!({
                "Type": "Dielectric",
                "Name": format!("{name}/{}", if layer + 2 == copper {
                    "B.Cu".to_owned()
                } else {
                    format!("In{}.Cu", layer + 1)
                }),
                "Material": "FR4",
                "Thickness": dielectric,
            }));
        }
    }
    stack.push(json!({
        "Type": "SolderMask",
        "Name": "Bottom Solder Mask",
        "Thickness": MASK_THICKNESS,
    }));
    stack.push(json!({
        "Type": "Legend",
        "Name": "Bottom Silk Screen",
        "Thickness": SILK_THICKNESS,
    }));
    Value::Array(stack)
}

/// File function and polarity of one package file. Attributes written into the
/// file win; a file without them falls back to what its name and its `G04`
/// comment say it is.
pub(crate) fn file_function(name: &str, text: &str, copper: usize) -> (String, String) {
    let mut function: Option<String> = None;
    let mut polarity: Option<String> = None;
    let mut comment = "";
    for line in text.lines() {
        // Gerber carries attributes as `%TF.Name,value*%`; a drill file carries
        // the same names as `; #@! TF.Name,value` comments.
        let line = line.strip_prefix("; #@! ").unwrap_or(line);
        let line = line.strip_prefix('%').unwrap_or(line);
        if name.ends_with(".drl")
            && function.is_none()
            && (line.starts_with("Plated,") || line.starts_with("NonPlated,"))
        {
            // A drill file may name its plating on a bare header line.
            function = Some(line.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("TF.FileFunction,") {
            function = Some(attribute_value(value));
        } else if let Some(value) = line.strip_prefix("TF.FilePolarity,") {
            polarity = Some(attribute_value(value));
        } else if comment.is_empty()
            && let Some(value) = line.strip_prefix("G04 ")
        {
            comment = value;
        }
    }
    let function = function.unwrap_or_else(|| {
        if name.ends_with(".drl") {
            // Drill files written before the split into plated and unplated
            // files hold every hole, and every hole is plated today.
            format!("Plated,1,{copper},PTH")
        } else if comment.to_ascii_lowercase().contains("outline") {
            "Profile,NP".into()
        } else {
            "Other,Other".into()
        }
    });
    (function, polarity.unwrap_or_else(|| "Positive".into()))
}

/// Value of one `%TF.Name,value*%` attribute, without its closing `*%`.
fn attribute_value(value: &str) -> String {
    value
        .trim_end()
        .trim_end_matches("*%")
        .trim_end_matches('*')
        .trim_end()
        .to_owned()
}

std::thread_local! {
    /// Clock override for the current thread, in Unix seconds. Set only by
    /// `with_fixed_clock`; a thread-local keeps parallel tests apart.
    static FIXED_CLOCK: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Run `body` with `CreationDate` stamped from `seconds` (Unix time) instead
/// of the real clock, on this thread only. Tests use it to make two exports
/// byte-identical without touching process-wide state.
///
/// # Rules
/// The override ends when `body` returns, even if it panics.
///
/// # Dependencies
/// `now_utc` in this file reads it.
///
/// # Do not
/// Use it in application code; real exports must stamp the real time.
pub fn with_fixed_clock<R>(seconds: u64, body: impl FnOnce() -> R) -> R {
    struct Restore(Option<u64>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FIXED_CLOCK.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(FIXED_CLOCK.with(|slot| slot.replace(Some(seconds))));
    body()
}

/// Unix seconds from the standard `SOURCE_DATE_EPOCH` setting, when it holds
/// a plain non-negative integer.
fn source_date_epoch() -> Option<u64> {
    std::env::var("SOURCE_DATE_EPOCH").ok()?.trim().parse().ok()
}

/// Value of one `%TF.Name,value*%` attribute, without its closing `*%`.
fn attribute_value(value: &str) -> String {
    value
        .trim_end()
        .trim_end_matches("*%")
        .trim_end_matches('*')
        .trim_end()
        .to_owned()
}

/// Current time as an ISO 8601 UTC timestamp, the form the specification uses
/// for `CreationDate`, for example `2026-10-28T22:05:11+00:00`. The clock comes
/// from `crates/clock`, which also works in the browser.
pub(crate) fn now_utc() -> String {
    iso8601_utc(pcbcraft_clock::unix_secs())
}

/// Civil date from Unix seconds, by the days-to-civil algorithm the calendar
/// literature uses (shift the epoch to 0000-03-01, then 400-year eras).
fn iso8601_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rem = seconds % 86_400;
}
/// Current time as an ISO 8601 UTC timestamp, the form the specification uses
/// for `CreationDate`, for example `2026-10-28T22:05:11+00:00`. The clock comes
/// from `crates/clock`, which also works in the browser.
pub(crate) fn now_utc() -> String {
    iso8601_utc(pcbcraft_clock::unix_secs())
}

/// Civil date from Unix seconds, by the days-to-civil algorithm the calendar
/// literature uses (shift the epoch to 0000-03-01, then 400-year eras).
fn iso8601_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rem = seconds % 86_400;
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    if month <= 2 {
        year += 1;
    }
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00",
        rem / 3_600,
        (rem / 60) % 60,
        rem % 60
    )
}

/// The whole fabrication package as a stored ZIP with no subfolders: every
/// Gerber layer, the drill file and the job file that lists them.
pub fn gerber_zip(p: &Project) -> Result<Vec<u8>, String> {
    let files = crate::gerber_files(p)?;
    let entries: Vec<(String, Vec<u8>)> = files
        .into_iter()
        .map(|(name, text)| (name, text.into_bytes()))
        .collect();
    crate::zip_entries(&entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Layer, Net, Track};
    use pcbcraft_geom::Point;

    fn board() -> Project {
        let mut project = Project::default();
        project.nets.push(Net {
            id: 8,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.tracks.push(Track {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: 3,
            net: 8,
            start: Point::new(10.0, 10.0),
            end: Point::new(40.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        // A plated via puts a plated drill file in the package.
        project.board.vias.push(pcbcraft_doc::Via {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: 4,
            net: 8,
            position: Point::new(40.0, 10.0),
            diameter: 0.6,
            drill: 0.3,
            from_layer: None,
            to_layer: None,
            covering: None,
covering_scope: None,
            covering_bottom: None,
            backdrill: None,
        });
        project
    }

    #[test]
    fn job_lists_every_package_file_with_its_own_attributes() -> Result<(), String> {
        let project = board();
        let files = crate::gerber_files(&project)?;
        let job: Value =
            serde_json::from_str(&files["pcb-job.gbrjob"]).map_err(|e| e.to_string())?;
        let listed = job["FilesAttributes"]
            .as_array()
            .ok_or("FilesAttributes must be an array")?;
        assert_eq!(
            listed.len(),
            files.len() - 2,
            "every file but the manifest and the readme is listed"
        );
        for entry in listed {
            let path = entry["Path"].as_str().ok_or("every entry has a Path")?;
            assert_ne!(path, "pcb-job.gbrjob", "the manifest does not list itself");
            let text = files
                .get(path)
                .ok_or_else(|| format!("{path} is missing"))?;
            let function = entry["FileFunction"].as_str().ok_or("a FileFunction")?;
            let polarity = entry["FilePolarity"].as_str().ok_or("a FilePolarity")?;
            if let Some(line) = text.lines().find(|l| l.starts_with("%TF.FileFunction,")) {
                assert_eq!(
                    function,
                    line.trim_start_matches("%TF.FileFunction,")
                        .trim_end_matches("*%"),
                    "{path} keeps the function it declares"
                );
            }
            if let Some(line) = text.lines().find(|l| l.starts_with("%TF.FilePolarity,")) {
                assert_eq!(
                    polarity,
                    line.trim_start_matches("%TF.FilePolarity,")
                        .trim_end_matches("*%"),
                    "{path} keeps the polarity it declares"
                );
            }
        }
        let functions: Vec<&str> = listed
            .iter()
            .filter_map(|e| e["FileFunction"].as_str())
            .collect();
        assert!(
            functions.contains(&"Copper,L1,Top"),
            "front copper is listed"
        );
        assert!(
            functions.contains(&"Copper,L2,Bot"),
            "back copper is listed"
        );
        assert!(functions.contains(&"Profile,NP"), "the outline is listed");
        assert!(
            functions.contains(&"Plated,1,2,PTH"),
            "the drill file is listed as plated through holes"
        );
        assert_eq!(
            job["GeneralSpecs"]["LayerNumber"], 2,
            "the copper count comes from the listed layers"
        );
        assert_eq!(
            job["Header"]["GenerationSoftware"]["Application"], "PCBCraft",
            "the manifest says what wrote it"
        );
        Ok(())
    }

    #[test]
    fn design_rules_carry_the_recorded_fabrication_limits() -> Result<(), String> {
        let mut project = board();
        project.board.rules = Some(pcbcraft_doc::FabRules {
            min_track: Some(0.127),
            min_space: Some(0.15),
            ..Default::default()
        });
        let files = crate::gerber_files(&project)?;
        let job: Value =
            serde_json::from_str(&files["pcb-job.gbrjob"]).map_err(|e| e.to_string())?;
        let rule = &job["DesignRules"][0];
        assert_eq!(
            rule["MinLineWidth"], 0.127,
            "min_track becomes the line width"
        );
        assert_eq!(
            rule["TrackToTrack"], 0.15,
            "min_space becomes the separation"
        );
        assert_eq!(rule["PadToPad"], 0.15);
        Ok(())
    }

    #[test]
    fn design_rules_without_fabrication_limits_keep_the_board_clearance() -> Result<(), String> {
        let project = board();
        let files = crate::gerber_files(&project)?;
        let job: Value =
            serde_json::from_str(&files["pcb-job.gbrjob"]).map_err(|e| e.to_string())?;
        let rule = &job["DesignRules"][0];
        assert_eq!(rule["PadToPad"], project.board.clearance);
        assert!(rule.get("MinLineWidth").is_none(), "no invented line width");
        Ok(())
    }

    #[test]
    fn job_is_deterministic_apart_from_the_creation_date() -> Result<(), String> {
        let project = board();
        let files = crate::gerber_files(&project)?;
        let early = gerber_job_at(&project, &files, "2026-01-02T03:04:05+00:00")?;
        let late = gerber_job_at(&project, &files, "2026-11-12T13:14:15+00:00")?;
        assert_ne!(early, late, "the creation date is recorded");
        assert_eq!(
            early.replace("2026-01-02T03:04:05+00:00", "STAMP"),
            late.replace("2026-11-12T13:14:15+00:00", "STAMP"),
            "two runs differ only in the creation date"
        );
        let stamped = gerber_job(&project, &files)?;
        let job: Value = serde_json::from_str(&stamped).map_err(|e| e.to_string())?;
        let date = job["Header"]["CreationDate"]
            .as_str()
            .ok_or("a CreationDate")?;
        assert_eq!(date.len(), 25, "an ISO 8601 timestamp with an offset");
        assert!(date.ends_with("+00:00"), "the stamp is UTC");
        assert_eq!(&date[4..5], "-", "the year comes first");
        Ok(())
    }

    #[test]
    fn a_fixed_clock_makes_two_jobs_byte_identical_and_ends_with_its_scope() -> Result<(), String> {
        let project = board();
        let files = crate::gerber_files(&project)?;
        let (first, second) = with_fixed_clock(1_800_000_000, || {
            let first = gerber_job(&project, &files)?;
            std::thread::sleep(std::time::Duration::from_millis(1_100));
            Ok::<_, String>((first, gerber_job(&project, &files)?))
        })?;
        assert_eq!(first, second, "a pinned clock does not tick");
        assert!(first.contains("2027-01-15T08:00:00+00:00"));
        let after = gerber_job(&project, &files)?;
        assert!(!after.contains("2027-01-15"), "the override is gone");
        Ok(())
    }

    #[test]
    fn a_new_layer_file_appears_in_the_job_without_other_edits() -> Result<(), String> {
        let project = board();
        let mut files = crate::gerber_files(&project)?;
        files.insert(
            "pcb-F.Mask.gbr".into(),
            "%TF.FileFunction,SolderMask,Top*%\n%TF.FilePolarity,Negative*%\n%FSLAX46Y46*%\nM02*\n"
                .into(),
        );
        let job: Value =
            serde_json::from_str(&gerber_job(&project, &files)?).map_err(|e| e.to_string())?;
        let mask = job["FilesAttributes"]
            .as_array()
            .ok_or("FilesAttributes must be an array")?
            .iter()
            .find(|e| e["Path"] == "pcb-F.Mask.gbr")
            .ok_or("the new mask file is listed")?;
        assert_eq!(mask["FileFunction"], "SolderMask,Top");
        assert_eq!(mask["FilePolarity"], "Negative");
        Ok(())
    }

    #[test]
    fn package_zip_holds_the_job_and_every_layer_without_folders() -> Result<(), String> {
        let project = board();
        let files = crate::gerber_files(&project)?;
        let zip = gerber_zip(&project)?;
        assert!(zip.starts_with(b"PK\x03\x04"), "a stored ZIP");
        let text = String::from_utf8_lossy(&zip);
        for name in files.keys() {
            assert!(text.contains(name.as_str()), "the archive holds {name}");
            assert!(!name.contains('/'), "{name} sits at the top level");
        }
        Ok(())
    }
}
