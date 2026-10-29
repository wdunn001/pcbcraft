//! Artifact commands, stale detection and export helpers.
use crate::Error;
use base64::Engine as _;
use pcbcraft_doc::{Artifact, Id, Project};
use pcbcraft_io::{generate, output_hash, write_generated, GeneratedOutput};
use serde_json::{json, Value};
use std::path::Path;

pub fn command_is_read_only(id: &str) -> bool {
    matches!(id, "artifact.generate" | "artifact.stale")
}

pub fn artifact_is_stale(p: &Project, artifact: &Artifact) -> Result<bool, Error> {
    if artifact.generated_at_revision.is_none() {
        return Ok(true);
    }
    let Some(stored) = artifact.generated_hash.as_ref() else {
        return Ok(true);
    };
    let live = output_hash(&generate(p, artifact).map_err(Error::Invalid)?);
    Ok(stored != &live)
}

pub fn stale_artifact_ids(p: &Project, filter: Option<&[Id]>) -> Result<Vec<Id>, Error> {
    let mut stale = Vec::new();
    for artifact in &p.artifacts {
        if filter.is_some_and(|ids| !ids.contains(&artifact.id)) {
            continue;
        }
        if artifact_is_stale(p, artifact)? {
            stale.push(artifact.id);
        }
    }
    stale.sort_unstable();
    Ok(stale)
}

pub fn format_generate_payload(output: &GeneratedOutput) -> Value {
    match output {
        GeneratedOutput::Text(text) => json!({
            "encoding": "text",
            "payload": text,
        }),
        GeneratedOutput::Bytes(bytes) => json!({
            "encoding": "base64",
            "payload": base64::engine::general_purpose::STANDARD.encode(bytes),
        }),
        GeneratedOutput::Files(map) => {
            let files: Vec<Value> = map
                .iter()
                .map(|(name, body)| {
                    json!({
                        "name": name,
                        "encoding": "text",
                        "payload": body,
                    })
                })
                .collect();
            json!({ "files": files })
        }
    }
}

pub fn export_artifact_record(
    p: &mut Project,
    id: Id,
    directory: &Path,
    record_revision: u64,
) -> Result<(Vec<String>, String), Error> {
    let index = p
        .artifacts
        .iter()
        .position(|a| a.id == id)
        .ok_or_else(|| Error::Invalid("object not found".into()))?;
    let artifact = p.artifacts[index].clone();
    let output = generate(p, &artifact).map_err(Error::Invalid)?;
    let hash = output_hash(&output);
    let written = write_generated(directory, &artifact, &output).map_err(Error::Invalid)?;
    let artifact = p
        .artifacts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| Error::Invalid("object not found".into()))?;
    artifact.generated_at_revision = Some(record_revision);
    artifact.generated_hash = Some(hash.clone());
    Ok((written, hash))
}

pub fn write_legacy_output_kind(
    p: &Project,
    kind: &str,
    directory: &Path,
) -> Result<Vec<String>, Error> {
    let mut written: Vec<String> = vec![];
    match kind {
        "gerbers" => {
            let files = pcbcraft_io::gerber_files(p).map_err(Error::Invalid)?;
            for (name, text) in &files {
                pcbcraft_io::atomic_write(&directory.join(name), text.as_bytes())
                    .map_err(Error::Invalid)?;
                written.push(name.clone());
            }
        }
        "netlist" => {
            let text = pcbcraft_io::netlist_text(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("pcb-netlist.net"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("pcb-netlist.net".into());
        }
        "bom" => {
            let text = pcbcraft_io::bom_csv(p, &[], None).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("bom.csv"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("bom.csv".into());
        }
        "placement" => {
            let text = pcbcraft_io::placement_text(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("placement.txt"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("placement.txt".into());
        }
        "dxf" => {
            let text = pcbcraft_io::dxf_board(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("pcb.dxf"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("pcb.dxf".into());
        }
        "cmp" => {
            let text = pcbcraft_io::cmp_text(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("pcb.cmp"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("pcb.cmp".into());
        }
        "idf" => {
            let text = pcbcraft_io::idf_board(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.idf"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board.idf".into());
        }
        "vrml" => {
            let text = pcbcraft_io::vrml_board(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.wrl"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board.wrl".into());
        }
        "step" => {
            let text = pcbcraft_io::step_board(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.step"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board.step".into());
        }
        "pdf" => {
            let bytes = pcbcraft_io::pdf_document(p, "pcb").map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.pdf"), &bytes)
                .map_err(Error::Invalid)?;
            written.push("board.pdf".into());
        }
        "svg" => {
            let text = pcbcraft_io::svg(p, "pcb").map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.svg"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board.svg".into());
        }
        "ipc2581" => {
            let text = pcbcraft_io::ipc2581_text(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.xml"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board.xml".into());
        }
        "dsn" => {
            let text = pcbcraft_io::specctra_dsn(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("pcb.dsn"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("pcb.dsn".into());
        }
        "ipc356" => {
            let text = pcbcraft_io::ipc356_netlist(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("test.brd"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("test.brd".into());
        }
        "gencad" => {
            let text = pcbcraft_io::gencad_text(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.gencad"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board.gencad".into());
        }
        "hyperlynx" => {
            let text = pcbcraft_io::hyperlynx_csv(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("netlist.csv"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("netlist.csv".into());
        }
        "plot" => {
            let text = pcbcraft_io::plot_csv(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("plot-report.csv"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("plot-report.csv".into());
        }
        "report" => {
            let text = pcbcraft_io::report_text(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board-report.txt"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board-report.txt".into());
        }
        "ps" => {
            let text = pcbcraft_io::postscript(p, "pcb").map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("pcb.ps"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("pcb.ps".into());
        }
        "odb" => {
            let bytes = pcbcraft_io::odb_package(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.odb.zip"), &bytes)
                .map_err(Error::Invalid)?;
            written.push("board.odb.zip".into());
        }
        "hpgl" => {
            let text = pcbcraft_io::hpgl_plot(p, "pcb").map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board.hpgl"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board.hpgl".into());
        }
        "stackup" => {
            let text = pcbcraft_io::stackup_text(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board-stackup.txt"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board-stackup.txt".into());
        }
        "drill_chart" => {
            let text = pcbcraft_io::drill_chart_csv(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board-drill-tools.csv"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board-drill-tools.csv".into());
        }
        "stats" => {
            let text = pcbcraft_io::stats_text(p).map_err(Error::Invalid)?;
            pcbcraft_io::atomic_write(&directory.join("board-stats.csv"), text.as_bytes())
                .map_err(Error::Invalid)?;
            written.push("board-stats.csv".into());
        }
        other => {
            return Err(Error::Invalid(format!("unknown export kind {other}")));
        }
    }
    written.sort();
    Ok(written)
}
