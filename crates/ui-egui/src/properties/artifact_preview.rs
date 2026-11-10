//! What an artifact file would hold, counted from the file the design makes
//! now, so the Properties card can say it without leaving the app.
//!
//! Context: Properties shows an artifact's kind, layers, settings, file and
//! state; this adds one line saying how much the file contains. Only plot
//! kinds are Gerber text with flashes and draws; every other kind or any
//! failure has no preview line.
//! Local rules: the preview never writes a file and never reads the live
//! clock; it pins the clock to zero so the count cannot drift between frames.
//!
//! # Dependencies
//! - crates/io/src/generate.rs (`generate`, `GeneratedOutput`)
//! - crates/io/src/plot_read.rs (`count_flashes_and_draws`)
//! - crates/io/src/gerber_job.rs (`with_fixed_clock`)
//!
//! # Do not
//! - Cache the count; the design changes every revision.
//! - Preview kinds that are not Gerber plots; their files are not counted.

use pcbcraft_doc::{Artifact, Project};
use pcbcraft_io::GeneratedOutput;

/// One line saying what an artifact file would draw, counted from the text
/// the design would make now, or None when the kind is not a plot or the
/// text cannot be made or counted.
///
/// # Rules
/// - Only the kinds copper_plot, mask_plot, silk_plot, paste_plot and
///   profile_plot get a line; every other kind is None.
/// - The clock is pinned to zero while generating, so two frames of the
///   same design give the same line.
/// - Any generation or counting error is None, never a message.
///
/// # Dependencies
/// - crates/io/src/generate.rs (`generate` returns the text)
/// - crates/io/src/plot_read.rs (`count_flashes_and_draws` counts it)
/// - crates/text/strings/en.txt (`artifact.preview`)
///
/// # Do not
/// - Write a file or touch the file system here.
pub(crate) fn preview_line(project: &Project, a: &Artifact) -> Option<String> {
    if !matches!(
        a.kind.as_str(),
        "copper_plot" | "mask_plot" | "silk_plot" | "paste_plot" | "profile_plot"
    ) {
        return None;
    }
    let text = match pcbcraft_io::with_fixed_clock(0, || pcbcraft_io::generate(project, a)).ok()? {
        GeneratedOutput::Text(text) => text,
        _ => return None,
    };
    let (flashes, draws) = pcbcraft_io::count_flashes_and_draws(&text).ok()?;
    Some(pcbcraft_text::lookup(
        "artifact.preview",
        &[
            ("pads", &flashes.to_string()),
            ("lines", &draws.to_string()),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: &str, path: &str) -> Artifact {
        Artifact {
            id: pcbcraft_doc::Id::default(),
            name: "Preview test".into(),
            kind: kind.into(),
            layers: vec!["F.Cu".into()],
            settings: serde_json::Value::Null,
            machine: None,
            path: path.into(),
            generated_at_revision: None,
            generated_hash: None,
            job: None,
        }
    }

    fn sample_project() -> Project {
        let mut project = Project::default();
        project.board.width = 40.0;
        project.board.height = 30.0;
        project
    }

    #[test]
    fn a_copper_plot_counts_its_flashes_and_draws() -> Result<(), String> {
        let project = sample_project();
        let a = record("copper_plot", "pcb-F.Cu.gbr");
        let text = match pcbcraft_io::with_fixed_clock(0, || pcbcraft_io::generate(&project, &a))? {
            GeneratedOutput::Text(text) => text,
            _ => return Err("a copper plot generates text".into()),
        };
        let flashes = text
            .lines()
            .filter(|l| l.trim_end().ends_with("D03*"))
            .count();
        let draws = text
            .lines()
            .filter(|l| l.trim_end().ends_with("D01*"))
            .count();
        let line = preview_line(&project, &a).ok_or("a copper plot has a preview line")?;
        let want = pcbcraft_text::lookup(
            "artifact.preview",
            &[
                ("pads", &flashes.to_string()),
                ("lines", &draws.to_string()),
            ],
        );
        if line != want {
            return Err(format!("the preview says {line}, expected {want}"));
        }
        if !line.contains("flashed") || !line.contains("drawn") {
            return Err(format!("the preview should name both counts: {line}"));
        }
        Ok(())
    }

    #[test]
    fn a_bill_of_materials_has_no_preview() -> Result<(), String> {
        let project = sample_project();
        let a = record("bom", "bom.csv");
        if preview_line(&project, &a).is_some() {
            return Err("a bill of materials has no preview line".into());
        }
        Ok(())
    }
}
