use pcbcraft_doc::{
    Net, Point, Project, Via, ViaCovering, ViaCoveringScope,
};
use pcbcraft_io::gerber_files;
fn via_project(
    covering: ViaCovering,
    scope: Option<ViaCoveringScope>,
) -> Result<Project, String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "GND".into(),
        ..Default::default()
    });
    p.board.via_covering_default = ViaCovering::Open;
    p.board.vias.push(Via {
        id: 2,
        net: 1,
        position: Point::new(10.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: Some(covering),
        covering_scope: scope,
    });
    p.validate()?;
    Ok(p)
}

/// The tool attribute lines of one drill file, in file order.
fn via_drill_attributes(drl: &str) -> Vec<String> {
    drl.lines()
        .filter(|l| l.contains("TA.AperFunction,Plated,PTH,ViaDrill"))
        .map(str::to_string)
        .collect()
}

/// Number of filled mask openings in one mask file.
fn openings(mask: &str) -> usize {
    mask.matches("G36*").count()
}

#[test]
fn drill_file_writes_the_via_protection_type_as_a_tool_attribute() -> Result<(), String> {
    let files = gerber_files(&via_project(ViaCovering::Tented, None)?)?;
    let drill = files
        .get("pcb-Plated.drl")
        .ok_or("plated drill missing")?;
    assert!(
        drill.contains("; #@! TA.AperFunction,Plated,PTH,ViaDrill,Ib\n"),
        "{drill}"
    );
    assert!(
        !drill.contains("via protection:"),
        "comment line must be gone: {drill}"
    );
    Ok(())
}

#[test]
fn via_pads_carry_no_specifier() -> Result<(), String> {
    let files = gerber_files(&via_project(ViaCovering::Open, None)?)?;
    let copper = files.get("pcb-F.Cu.gbr").ok_or("front copper missing")?;
    assert!(
        copper.contains("%TA.AperFunction,ViaPad*%\n"),
        "{copper}"
    );
    for (name, text) in &files {
        assert!(
            !text.contains("ViaPad,"),
            "{name}: the specification gives ViaPad no specifier"
        );
    }
    Ok(())
}

#[test]
fn a_via_open_on_the_bottom_keeps_its_bottom_mask_opening_only() -> Result<(), String> {
    let mut p = via_project(ViaCovering::Tented, None)?;
    p.board.vias[0].covering = Some(ViaCovering::Open);
    p.board.vias[0].covering_bottom = Some(ViaCovering::Plugged);
    let files = gerber_files(&p)?;
    assert_eq!(openings(&files["pcb-F.Mask.gbr"]), 1, "open on top");
    assert_eq!(openings(&files["pcb-B.Mask.gbr"]), 0, "plugged underneath");
    Ok(())
}
    Ok(())
}

#[test]
Ok(())
}

#[test]
fn via_pads_carry_no_specifier() -> Result<(), String> {
    let files = gerber_files(&via_project(ViaCovering::Open, None)?)?;
    let copper = files.get("pcb-F.Cu.gbr").ok_or("front copper missing")?;
    assert!(
        copper.contains("%TA.AperFunction,ViaPad*%\n"),
        "{copper}"
    );
    assert!(
        !copper.contains("ViaPad,CuDef"),
        "via pads carry no CuDef specifier: {copper}"
    );
    Ok(())
}

#[test]
fn a_via_open_on_the_bottom_keeps_its_bottom_mask_opening_only() -> Result<(), String> {
    let files = gerber_files(&via_project(
        ViaCovering::Tented,
        Some(ViaCoveringScope::Top),
    )?)?;
    let front = files.get("pcb-F.Mask.gbr").ok_or("front mask missing")?;
    let back = files.get("pcb-B.Mask.gbr").ok_or("back mask missing")?;
    assert_eq!(
        front.matches("G36*").count(),
        0,
        "top stays tented: {front}"
    );
    assert_eq!(
        back.matches("G36*").count(),
        1,
        "bottom keeps its opening: {back}"
    );
    Ok(())
}

#[test]
fn golden_drill_files_and_masks_for_the_four_layer_board() -> Result<(), String> {
    let files = pcbcraft_io::gerber_files(&golden_board()?)?;
    for (name, golden_name) in [
        ("pcb-Plated.drl", "via_protection_plated.drl"),
        ("pcb-Drill-L1-L2.drl", "via_protection_blind.drl"),
        ("pcb-Drill-L2-L3.drl", "via_protection_buried.drl"),
        ("pcb-NPTH.drl", "via_protection_npth.drl"),
        ("pcb-F.Mask.gbr", "via_protection_top_mask.gbr"),
        ("pcb-B.Mask.gbr", "via_protection_bottom_mask.gbr"),
    ] {
        let text = files.get(name).ok_or_else(|| format!("no {name}"))?;
        golden(golden_name, text)?;
    }
    Ok(())
}

/// What the mask and the drill file should show for a via, worked out by hand.
fn expected(
    layers: u8,
    span: Option<(u8, u8)>,
    top: ViaCovering,
    bottom: ViaCovering,
) -> (bool, bool, &'static str) {
    let last = layers - 1;
    let (a, b) = span.unwrap_or((0, last));
    let (top_face, bottom_face) = (a == 0, b == last);
    let open = |c: ViaCovering| c == ViaCovering::Open;
    let rank = |c: ViaCovering| ViaCovering::ALL.iter().position(|x| *x == c).unwrap_or(0);
    let token = |strong: ViaCovering, one: bool| -> &'static str {
        match (strong, one) {
            (ViaCovering::Open, _) => "None",
            (ViaCovering::Tented, true) => "Ia",
            (ViaCovering::Tented, false) => "Ib",
            (ViaCovering::TentedCovered, true) => "IIa",
            (ViaCovering::TentedCovered, false) => "IIb",
            (ViaCovering::Plugged, true) => "IIIa",
            (ViaCovering::Plugged, false) => "IIIb",
            (ViaCovering::PluggedCovered, true) => "IVa",
            (ViaCovering::PluggedCovered, false) => "IVb",
            (ViaCovering::Filled, _) => "V",
            (ViaCovering::FilledCovered, _) => "VI",
            (ViaCovering::FilledCapped, _) => "VII",
        }
    };
    let ty = match (top_face, bottom_face) {
        (true, true) => {
            let (strong, weak) = if rank(top) >= rank(bottom) {
                (top, bottom)
            } else {
                (bottom, top)
            };
            token(strong, open(weak))
        }
        (true, false) => token(top, true),
        (false, true) => token(bottom, true),
        (false, false) => "None",
    };
    (top_face && open(top), bottom_face && open(bottom), ty)
}

#[test]
fn every_protection_on_two_four_and_six_layers_reaches_the_files_as_worked_out_by_hand() -> Result<(), String> {
    for layers in [2u8, 4, 6] {
        let mut spans: Vec<Option<(u8, u8)>> = vec![None];
        if layers > 2 {
            spans.push(Some((0, 1)));
            spans.push(Some((layers - 2, layers - 1)));
            spans.push(Some((1, layers - 2)));
        }
        for span in spans {
            for top in ViaCovering::ALL {
                for bottom in ViaCovering::ALL {
                    let mut p = board(layers);
                    p.board.vias = vec![via(10, 10.0, 0.3, span, Some(top), Some(bottom))?];
                    p.validate()?;
                    let files = pcbcraft_io::gerber_files(&p)?;
                    let (open_top, open_bottom, ty) = expected(layers, span, top, bottom);
                    let note =
                        format!("{layers} layers, span {span:?}, top {top:?}, bottom {bottom:?}");
                    assert_eq!(
                        openings(&files["pcb-F.Mask.gbr"]),
                        usize::from(open_top),
                        "{note}"
                    );
                    assert_eq!(
                        openings(&files["pcb-B.Mask.gbr"]),
                        usize::from(open_bottom),
                        "{note}"
                    );
                    let name = match span {
                        Some((a, b)) if !(a == 0 && b == layers - 1) => {
                            format!("pcb-Drill-L{}-L{}.drl", a + 1, b + 1)
                        }
                        _ => "pcb-Plated.drl".to_string(),
                    };
                    let drill = files.get(&name).ok_or_else(|| format!("no {name}"))?;
                    assert_eq!(
                        via_drill_attributes(drill),
                        vec![format!("; #@! TA.AperFunction,Plated,PTH,ViaDrill,{ty}")],
                        "{note}"
                    );
                }
            }
        }
    }
    Ok(())
}
    Ok(())
}
