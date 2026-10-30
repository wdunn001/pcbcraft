//! Collect MCP tool output as named byte blobs for browser download instead of disk writes.
use pcbcraft_doc::Project;
use serde_json::{Value, json};
use std::path::Path;

pub fn write_tool_files(
    project: &Project,
    name: &str,
    args: &Value,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    match name {
        "library.save" => {
            project.library.validate()?;
            let path = string(args, "path")?;
            let bytes = serde_json::to_vec_pretty(
                &json!({"format":"pcbcraft-library","version":1,"library":project.library}),
            )
            .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > pcbcraft_io::MAX_FILE_BYTES {
                return Err("library collection exceeds 8 MiB".into());
            }
            Ok(vec![(file_name(path)?, bytes)])
        }
        "library.export_footprint"
        | "library.export_symbol"
        | "library.export_footprint_svg"
        | "library.export_symbol_svg" => {
            project.library.validate()?;
            let path = string(args, "path")?;
            let definition = string(args, "name")?;
            let footprint =
                name == "library.export_footprint" || name == "library.export_footprint_svg";
            let suffix = match name {
                "library.export_footprint" => "kicad_mod",
                "library.export_symbol" => "kicad_sym",
                _ => "svg",
            };
            if Path::new(path).extension().and_then(|e| e.to_str()) != Some(suffix) {
                return Err(format!("path must end in .{suffix}"));
            }
            let text = export_library_item(project, footprint, suffix, definition)?;
            Ok(vec![(file_name(path)?, text.into_bytes())])
        }
        "library.export_footprints" => {
            let mut files = Vec::new();
            for item in &project.library.footprints {
                let text = pcbcraft_io::footprint_library_text(item)?;
                files.push((format!("{}.kicad_mod", item.name), text.into_bytes()));
            }
            Ok(files)
        }
        "library.export_symbols" => {
            let mut files = Vec::new();
            for item in &project.library.symbols {
                let text = pcbcraft_io::symbol_library_text(item)?;
                files.push((format!("{}.kicad_sym", item.name), text.into_bytes()));
            }
            Ok(files)
        }
        "gerber.export" => gerber_files(project),
        "dsn.write" => {
            let text = pcbcraft_io::specctra_dsn(project)?;
            Ok(vec![("pcb.dsn".into(), text.into_bytes())])
        }
        "cmp.write" => {
            let path = string(args, "path")?;
            if Path::new(path).extension().and_then(|e| e.to_str()) != Some("cmp") {
                return Err("path must end in .cmp".into());
            }
            let text = pcbcraft_io::cmp_text(project)?;
            Ok(vec![(file_name(path)?, text.into_bytes())])
        }
        "report.write" => {
            let path = string(args, "path")?;
            if Path::new(path).extension().and_then(|e| e.to_str()) != Some("txt") {
                return Err("path must end in .txt".into());
            }
            let text = pcbcraft_io::report_text(project)?;
            Ok(vec![(file_name(path)?, text.into_bytes())])
        }
        "ipc2581.write" => {
            let path = string(args, "path")?;
            if Path::new(path).extension().and_then(|e| e.to_str()) != Some("xml") {
                return Err("path must end in .xml".into());
            }
            let text = pcbcraft_io::ipc2581_text(project)?;
            Ok(vec![(file_name(path)?, text.into_bytes())])
        }
        "idf.write" => {
            let text = pcbcraft_io::idf_board(project)?;
            Ok(vec![("board.idf".into(), text.into_bytes())])
        }
        "step.write" => {
            let text = pcbcraft_io::step_board(project)?;
            Ok(vec![("board.step".into(), text.into_bytes())])
        }
        "wrl.write" => {
            let text = pcbcraft_io::vrml_board(project)?;
            Ok(vec![("board.wrl".into(), text.into_bytes())])
        }
        "hyperlynx.write" => {
            let text = pcbcraft_io::hyperlynx_csv(project)?;
            Ok(vec![("netlist.csv".into(), text.into_bytes())])
        }
        "gencad.write" => {
            let text = pcbcraft_io::gencad_text(project)?;
            Ok(vec![("board.gencad".into(), text.into_bytes())])
        }
        "ps.write" => {
            let view = view_arg(args);
            let text = pcbcraft_io::postscript(project, view)?;
            Ok(vec![(format!("pcb-{view}.ps"), text.into_bytes())])
        }
        "stats.write" => {
            let text = pcbcraft_io::stats_text(project)?;
            Ok(vec![("board-stats.csv".into(), text.into_bytes())])
        }
        "drill_chart.write" => {
            let text = pcbcraft_io::drill_chart_csv(project)?;
            Ok(vec![("board-drill-tools.csv".into(), text.into_bytes())])
        }
        "stackup.write" => {
            let text = pcbcraft_io::stackup_text(project)?;
            Ok(vec![("board-stackup.txt".into(), text.into_bytes())])
        }
        "hpgl.write" => {
            let view = view_arg(args);
            let text = pcbcraft_io::hpgl_plot(project, view)?;
            Ok(vec![(format!("pcb-{view}.hpgl"), text.into_bytes())])
        }
        "odb.write" => {
            let bytes = pcbcraft_io::odb_package(project)?;
            Ok(vec![("board.odb.zip".into(), bytes)])
        }
        "pdf.write" => {
            let view = view_arg(args);
            let bytes = pcbcraft_io::pdf_document(project, view)?;
            Ok(vec![(format!("pcb-{view}.pdf"), bytes)])
        }
        "ipc356.write" => {
            let text = pcbcraft_io::ipc356_netlist(project)?;
            Ok(vec![("test.brd".into(), text.into_bytes())])
        }
        "netlist.write" => {
            let text = pcbcraft_io::netlist_text(project)?;
            Ok(vec![("pcb-netlist.net".into(), text.into_bytes())])
        }
        "bom.write" => {
            let fields = strings(args, "fields")?;
            let variant = args
                .get("variant")
                .map(|v| v.as_str().ok_or("variant must be a string"))
                .transpose()?;
            let text = pcbcraft_io::bom_csv(project, &fields, variant)?;
            Ok(vec![("bom.csv".into(), text.into_bytes())])
        }
        "dxf.export" => {
            let text = pcbcraft_io::dxf_board(project)?;
            Ok(vec![("pcb.dxf".into(), text.into_bytes())])
        }
        "placement.write" => {
            let text = pcbcraft_io::placement_text(project)?;
            Ok(vec![("placement.txt".into(), text.into_bytes())])
        }
        "plot_csv.write" => {
            let text = pcbcraft_io::plot_csv(project)?;
            Ok(vec![("plot-report.csv".into(), text.into_bytes())])
        }
        "export" => export_files(project, args),
        _ => Err(format!("tool {name} does not produce downloadable files")),
    }
}

fn export_files(project: &Project, args: &Value) -> Result<Vec<(String, Vec<u8>)>, String> {
    let path = string(args, "path")?;
    let format = string(args, "format")?;
    if format == "png" {
        let bytes = pcbcraft_io::preview_png(project, view_arg(args))?;
        return Ok(vec![(file_name(path)?, bytes)]);
    }
    let text = match format {
        "kicad_pcb" => pcbcraft_io::kicad_board(project)?,
        "gerber_f_cu" => pcbcraft_io::gerber_copper(project, pcbcraft_io::Layer::Front)?,
        "gerber_b_cu" => pcbcraft_io::gerber_copper(project, pcbcraft_io::Layer::Back)?,
        "svg" => pcbcraft_io::svg(project, view_arg(args))?,
        _ => return Err("unsupported export format".into()),
    };
    Ok(vec![(file_name(path)?, text.into_bytes())])
}

fn gerber_files(project: &Project) -> Result<Vec<(String, Vec<u8>)>, String> {
    let files = pcbcraft_io::gerber_files(project)?;
    Ok(files
        .into_iter()
        .map(|(name, text)| (name, text.into_bytes()))
        .collect())
}

fn export_library_item(
    project: &Project,
    footprint: bool,
    suffix: &str,
    definition: &str,
) -> Result<String, String> {
    if footprint {
        let item = project
            .library
            .footprints
            .iter()
            .find(|f| f.name == definition)
            .ok_or("unknown project-library footprint")?;
        if suffix == "svg" {
            pcbcraft_io::footprint_svg(item)
        } else {
            pcbcraft_io::footprint_library_text(item)
        }
    } else {
        let item = project
            .library
            .symbols
            .iter()
            .find(|f| f.name == definition)
            .ok_or("unknown project-library symbol")?;
        if suffix == "svg" {
            pcbcraft_io::symbol_svg(item)
        } else {
            pcbcraft_io::symbol_library_text(item)
        }
    }
}

fn view_arg(args: &Value) -> &str {
    match args.get("view").and_then(Value::as_str) {
        Some(view) => view,
        None => "pcb",
    }
}

fn file_name(path: &str) -> Result<String, String> {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| "path must include a file name".into())
}

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string parameter: {key}"))
}

fn strings(args: &Value, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("{key} entries must be strings"))
            })
            .collect(),
        _ => Err(format!("{key} must be an array of strings")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Project;

    #[test]
    fn gerber_export_collector_matches_gerber_file_names() -> Result<(), String> {
        let project = Project::default();
        let files = write_tool_files(&project, "gerber.export", &json!({"path":"/tmp/out"}))?;
        assert!(!files.is_empty());
        assert!(files.iter().all(|(name, _)| !name.is_empty()));
        Ok(())
    }
}
