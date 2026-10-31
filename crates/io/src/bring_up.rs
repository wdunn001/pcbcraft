//! Generated bring-up checklist: power rails, headers, continuity and gaps.
use pcbcraft_doc::{Component, Id, NetTie, Project};
use pcbcraft_rules::testpoint::{self, CoverageReport, ProbeSettings};
use std::collections::BTreeMap;

fn csv_cell(text: &str) -> String {
    if text.contains(',') || text.contains('"') || text.contains('\n') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PowerKind {
    Ground,
    Rail,
}

fn power_kind(name: &str) -> Option<PowerKind> {
    let n = name.trim().to_ascii_uppercase();
    let n = n.trim_start_matches('+');
    if n.is_empty() {
        return None;
    }
    if n.starts_with("GND") || matches!(n, "VSS" | "0V" | "EARTH" | "AGND" | "DGND" | "PGND") {
        return Some(PowerKind::Ground);
    }
    let digit_v = n.chars().next().is_some_and(|c| c.is_ascii_digit())
        && n.chars().any(|c| c == 'V')
        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
    if n.starts_with("VCC")
        || n.starts_with("VDD")
        || matches!(n, "VBUS" | "VIN" | "VBAT" | "V")
        || digit_v
    {
        return Some(PowerKind::Rail);
    }
    None
}

fn power_nets(project: &Project) -> BTreeMap<Id, PowerKind> {
    let mut found = BTreeMap::new();
    for net in &project.nets {
        if let Some(kind) = power_kind(&net.name) {
            found.insert(net.id, kind);
        }
    }
    for label in project.labels.iter().filter(|l| l.kind == "power") {
        found
            .entry(label.net)
            .or_insert_with(|| power_kind(project.net_name(label.net)).unwrap_or(PowerKind::Rail));
    }
    for pin in project.components.iter().flat_map(|c| c.pins.iter()) {
        if pin.electrical_type == "power_in" || pin.electrical_type == "power_out" {
            if let Some(net) = pin.net.filter(|n| *n != 0) {
                found
                    .entry(net)
                    .or_insert_with(|| power_kind(project.net_name(net)).unwrap_or(PowerKind::Rail));
            }
        }
    }
    found
}

fn expected_voltage_v(name: &str, kind: PowerKind) -> String {
    if kind == PowerKind::Ground {
        return "0".into();
    }
    let n = name.trim().to_ascii_uppercase();
    let n = n.trim_start_matches('+');
    if let Some(stripped) = n.strip_suffix('V') {
        let digits: String = stripped.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        if digits.contains('.') {
            return digits;
        }
        if digits.len() >= 2 {
            let whole = &digits[..digits.len() - 1];
            let frac = &digits[digits.len() - 1..];
            if !whole.is_empty() {
                return format!("{whole}.{frac}");
            }
        }
    }
    match n {
        "VBUS" => "5".into(),
        "VIN" | "VBAT" => "see supply".into(),
        _ => "measure".into(),
    }
}

fn is_header_component(c: &Component) -> bool {
    let reference = c.reference.to_ascii_uppercase();
    if reference.starts_with('J') && c.pins.len() >= 2 {
        return true;
    }
    let mut text = String::new();
    if let Some(sym) = &c.library_symbol {
        text.push_str(&sym.definition.description);
        text.push(' ');
        text.push_str(&sym.definition.name);
    }
    if let Some(fp) = &c.library_footprint {
        text.push(' ');
        text.push_str(&fp.definition.name);
    }
    let upper = text.to_ascii_uppercase();
    ["HEADER", "CONN", "SWD", "TAG-CONNECT", "DEBUG", "JTAG"].iter().any(|k| upper.contains(k))
}

fn sensor_note(c: &Component) -> Option<String> {
    let blob = format!("{} {}", c.value, c.reference).to_ascii_uppercase();
    const KEYS: &[&str] = &[
        "IMU", "MPU", "BMI", "ICM", "BMP", "MS5611", "LIS3", "LSM", "QMC", "MAG", "BARO",
        "SENSOR",
    ];
    if KEYS.iter().any(|k| blob.contains(k)) {
        Some(format!(
            "Read WHO_AM_I or chip ID over the bus for {}",
            c.reference
        ))
    } else {
        None
    }
}

struct HeaderPin {
    reference: String,
    pin: String,
    net: String,
}

fn header_pins(project: &Project) -> Vec<HeaderPin> {
    let mut rows = Vec::new();
    for c in &project.components {
        if !is_header_component(c) {
            continue;
        }
        for pin in &c.pins {
            let net = pin
                .net
                .map(|id| project.net_name(id))
                .unwrap_or_else(|| "unassigned".into());
            rows.push(HeaderPin {
                reference: c.reference.clone(),
                pin: if pin.name.is_empty() {
                    pin.number.clone()
                } else {
                    format!("{} ({})", pin.number, pin.name)
                },
                net,
            });
        }
    }
    rows.sort_by(|a, b| {
        a.reference
            .cmp(&b.reference)
            .then(a.pin.cmp(&b.pin))
            .then(a.net.cmp(&b.net))
    });
    rows
}

fn continuity_pairs(project: &Project) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for NetTie { net_a, net_b, .. } in &project.board.netties {
        pairs.push((project.net_name(*net_a), project.net_name(*net_b)));
    }
    pairs.sort();
    pairs
}

fn plan_title(project: &Project) -> &str {
    if project.name.trim().is_empty() {
        "untitled board"
    } else {
        project.name.trim()
    }
}

fn write_rails(out: &mut String, project: &Project, rails: &BTreeMap<Id, PowerKind>) {
    out.push_str("## Power rails\n\n");
    out.push_str("| Net | Expected (V) | Notes |\n");
    out.push_str("| --- | --- | --- |\n");
    for (id, kind) in rails {
        let name = project.net_name(*id);
        let expected = expected_voltage_v(&name, *kind);
        let note = if *kind == PowerKind::Ground {
            "common return"
        } else {
            "verify before load"
        };
        out.push_str(&format!("| {name} | {expected} | {note} |\n"));
    }
    if rails.is_empty() {
        out.push_str("| (none detected) | | |\n");
    }
    out.push('\n');
}

fn write_headers(out: &mut String, pins: &[HeaderPin]) {
    out.push_str("## Programming and debug headers\n\n");
    if pins.is_empty() {
        out.push_str("No connector headers were detected on the schematic.\n\n");
        return;
    }
    out.push_str("| Reference | Pin | Net |\n");
    out.push_str("| --- | --- | --- |\n");
    for row in pins {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            row.reference, row.pin, row.net
        ));
    }
    out.push('\n');
}

fn write_continuity(out: &mut String, pairs: &[(String, String)]) {
    out.push_str("## Continuity pairs\n\n");
    if pairs.is_empty() {
        out.push_str("No net ties were recorded on the board.\n\n");
        return;
    }
    out.push_str("| Net A | Net B |\n");
    out.push_str("| --- | --- |\n");
    for (a, b) in pairs {
        out.push_str(&format!("| {a} | {b} |\n"));
    }
    out.push('\n');
}

fn write_sensors(out: &mut String, notes: &[String]) {
    out.push_str("## Sensor identity reads\n\n");
    if notes.is_empty() {
        out.push_str("No sensor parts were detected by value or reference.\n\n");
        return;
    }
    for note in notes {
        out.push_str("- ");
        out.push_str(note);
        out.push('\n');
    }
    out.push('\n');
}

fn write_gaps(
    out: &mut String,
    project: &Project,
    report: &CoverageReport,
    settings: ProbeSettings,
) {
    out.push_str("## Nets without a probe pad\n\n");
    out.push_str(&format!(
        "Probe minimums: {:.3} mm pad, {:.3} mm spacing between sites on different nets.\n\n",
        settings.min_pad_mm, settings.min_spacing_mm
    ));
    if report.uncovered.is_empty() {
        out.push_str("Every routed net has at least one qualifying probe site.\n\n");
        return;
    }
    out.push_str("| Net |\n");
    out.push_str("| --- |\n");
    for net in &report.uncovered {
        out.push_str(&format!("| {} |\n", project.net_name(*net)));
    }
    out.push('\n');
}

/// Markdown bring-up checklist built from the layout and schematic.
pub fn bring_up_plan_markdown(project: &Project) -> Result<String, String> {
    project.validate()?;
    let report = testpoint::coverage_report(project)?;
    let settings = report.settings;
    let rails = power_nets(project);
    let pins = header_pins(project);
    let pairs = continuity_pairs(project);
    let sensors: Vec<String> = project
        .components
        .iter()
        .filter_map(sensor_note)
        .collect();
    let mut out = format!("# Bring-up plan for {}\n\n", plan_title(project));
    write_rails(&mut out, project, &rails);
    write_headers(&mut out, &pins);
    write_continuity(&mut out, &pairs);
    write_sensors(&mut out, &sensors);
    write_gaps(&mut out, project, &report, settings);
    Ok(out)
}

/// CSV rows mirroring the markdown sections for spreadsheets and fixtures.
pub fn bring_up_plan_csv(project: &Project) -> Result<String, String> {
    project.validate()?;
    let report = testpoint::coverage_report(project)?;
    let settings = report.settings;
    let rails = power_nets(project);
    let pins = header_pins(project);
    let pairs = continuity_pairs(project);
    let mut out = String::from("SECTION,FIELD1,FIELD2,FIELD3,FIELD4\n");
    out.push_str(&format!(
        "meta,probe_min_pad_mm,{:.3},probe_min_spacing_mm,{:.3}\n",
        settings.min_pad_mm, settings.min_spacing_mm
    ));
    for (id, kind) in &rails {
        let name = project.net_name(*id);
        let expected = expected_voltage_v(&name, *kind);
        out.push_str(&format!(
            "rail,{},{},,\n",
            csv_cell(&name),
            csv_cell(&expected)
        ));
    }
    for row in &pins {
        out.push_str(&format!(
            "header,{},{},{},\n",
            csv_cell(&row.reference),
            csv_cell(&row.pin),
            csv_cell(&row.net)
        ));
    }
    for (a, b) in &pairs {
        out.push_str(&format!(
            "continuity,{},{},,\n",
            csv_cell(a),
            csv_cell(b)
        ));
    }
    for c in &project.components {
        if let Some(note) = sensor_note(c) {
            out.push_str(&format!("sensor,{},{},,\n", csv_cell(&c.reference), csv_cell(&note)));
        }
    }
    for net in &report.uncovered {
        out.push_str(&format!(
            "missing_testpoint,{},{},,\n",
            csv_cell(&project.net_name(*net)),
            csv_cell(&format!(
                "min pad {:.3} mm, spacing {:.3} mm",
                settings.min_pad_mm, settings.min_spacing_mm
            ))
        ));
    }
    Ok(out)
}

/// Both bring-up files for a fabrication export job.
pub fn bring_up_export_files(project: &Project) -> Result<BTreeMap<String, String>, String> {
    let mut files = BTreeMap::new();
    files.insert(
        "bring-up-plan.md".into(),
        bring_up_plan_markdown(project)?,
    );
    files.insert("bring-up-plan.csv".into(), bring_up_plan_csv(project)?);
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Footprint, Net, Pin};
    use pcbcraft_geom::Point;

    #[test]
    fn markdown_lists_an_uncovered_net_and_rail() -> Result<(), String> {
        let mut p = Project::default();
        p.name = "demo".into();
        p.nets.push(Net {
            id: 1,
            name: "3V3".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: 2,
            name: "SIG".into(),
            ..Default::default()
        });
        p.components.push(Component {
            id: 10,
            reference: "J1".into(),
            value: "DEBUG".into(),
            kind: "library".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![Pin {
                number: "1".into(),
                name: "SWDIO".into(),
                offset: pcbcraft_geom::Point::default(),
                net: Some(2),
                electrical_type: "bidirectional".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sheet: 0,
        });
        p.board.footprints.push(Footprint {
            id: 20,
            component: 10,
            position: Point::default(),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![],
        });
        p.validate()?;
        let md = bring_up_plan_markdown(&p)?;
        assert!(md.contains("3V3"), "{md}");
        assert!(md.contains("SIG"), "{md}");
        assert!(md.contains("SWDIO"), "{md}");
        let csv = bring_up_plan_csv(&p)?;
        assert!(csv.contains("missing_testpoint"));
        Ok(())
    }
}
