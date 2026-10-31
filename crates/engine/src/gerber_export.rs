//! `gerber.files`: the fabrication files under a naming preset and drill options.
//!
//! Context: issue #225. The Export dialog and assistants name a preset and
//! drill style; this runs `pcbcraft_io::gerber_preset::gerber_files_with` and,
//! when a directory is given, writes the files there. Without a directory it
//! only lists the names, which is what the dialog preview shows.
//!
//! # Rules
//! - Reads the project only; the directory is the one thing it writes.
//! - Unknown preset or option values are refused with the allowed words.
//!
//! # Dependencies
//! - crates/io/src/gerber_preset.rs `GerberOptions`, `gerber_files_with`
//!
//! # Do not
//! - Add options here that `GerberOptions` lacks.
use super::Error;
use pcbcraft_doc::Project;
use pcbcraft_io::gerber_preset::{
    DrillUnits, GerberOptions, NamingPreset, ZeroSuppression, gerber_files_with,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    #[serde(default)]
    naming: Option<String>,
    #[serde(default)]
    drill_files: Option<String>,
    #[serde(default)]
    drill_units: Option<String>,
    #[serde(default)]
    zero_suppression: Option<String>,
    #[serde(default)]
    integer_digits: Option<u8>,
    #[serde(default)]
    decimal_digits: Option<u8>,
    #[serde(default = "yes")]
    drill_map: bool,
    #[serde(default = "yes")]
    job_file: bool,
    #[serde(default = "yes")]
    readme: bool,
    #[serde(default)]
    directory: Option<String>,
}

fn yes() -> bool {
    true
}

/// Options from command parameters.
fn options(a: &Params) -> Result<GerberOptions, Error> {
    let bad =
        |what: &str, allowed: &str| Error::Invalid(format!("{what} must be one of {allowed}"));
    let naming = match a.naming.as_deref() {
        None => NamingPreset::Project,
        Some(id) => {
            NamingPreset::from_id(id).ok_or_else(|| bad("naming", "project, plain, protel"))?
        }
    };
    let merge_drills = match a.drill_files.as_deref() {
        None | Some("split") => false,
        Some("merged") => true,
        Some(_) => return Err(bad("drill_files", "split, merged")),
    };
    let units = match a.drill_units.as_deref() {
        None | Some("project") => DrillUnits::Project,
        Some("mm") => DrillUnits::Mm,
        Some("inch") => DrillUnits::Inch,
        Some(_) => return Err(bad("drill_units", "project, mm, inch")),
    };
    let zeros = match a.zero_suppression.as_deref() {
        None => ZeroSuppression::Decimal,
        Some(id) => ZeroSuppression::from_id(id)
            .ok_or_else(|| bad("zero_suppression", "decimal, leading, trailing"))?,
    };
    let format = match (a.integer_digits, a.decimal_digits) {
        (None, None) => None,
        (Some(i), Some(f)) => Some((i, f)),
        _ => {
            return Err(Error::Invalid(
                "give both integer_digits and decimal_digits, or neither".into(),
            ));
        }
    };
    let options = GerberOptions {
        naming,
        merge_drills,
        units,
        zeros,
        format,
        drill_map: a.drill_map,
        job_file: a.job_file,
        readme: a.readme,
        ..GerberOptions::default()
    };
    options.validate().map_err(Error::Invalid)?;
    Ok(options)
}

/// Run `gerber.files`.
///
/// # Rules
/// - `mutating` is false for a read-only call; a directory then refuses.
///
/// # Dependencies
/// - `pcbcraft_io::atomic_write` for the files.
///
/// # Do not
/// - Return file contents; the names and sizes are the preview.
pub(crate) fn run(p: &Project, params: &Value, mutating: bool) -> Result<Value, Error> {
    let a: Params = super::parse(params)?;
    let files = gerber_files_with(p, &options(&a)?).map_err(Error::Invalid)?;
    let mut written = false;
    if let Some(directory) = a.directory.as_deref() {
        if !mutating {
            return Err(Error::Invalid(
                "gerber.files cannot write a directory as a read-only command".into(),
            ));
        }
        if directory.is_empty() || directory.len() > 4096 {
            return Err(Error::Invalid("output directory is out of range".into()));
        }
        let dir = std::path::Path::new(directory);
        std::fs::create_dir_all(dir).map_err(|e| Error::Invalid(e.to_string()))?;
        for (name, text) in &files {
            pcbcraft_io::atomic_write(&dir.join(name), text.as_bytes()).map_err(Error::Invalid)?;
        }
        written = true;
    }
    let list: Vec<Value> = files
        .iter()
        .map(|(name, text)| json!({"file": name, "bytes": text.len()}))
        .collect();
    Ok(json!({"files": list, "written": written}))
}
