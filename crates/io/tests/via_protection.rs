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
        !drill.contains("; via protection:"),
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
