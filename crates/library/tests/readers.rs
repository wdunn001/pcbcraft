use pcbcraft_library::{Graphic, SymbolPin, read_footprint, read_symbols};

#[test]
fn real_device_symbols_preserve_pin_types_and_graphics() -> Result<(), Box<dyn std::error::Error>> {
    let symbols = read_symbols(include_str!("fixtures/device.kicad_sym"))?;
    let resistor = symbols.iter().find(|s| s.name == "R").ok_or("R missing")?;
    assert_eq!(resistor.reference, "R");
    assert_eq!(resistor.pins.len(), 2);
    assert!(resistor.unsupported.is_empty());
    assert!(resistor.pins.iter().all(|p| p.electrical_type == "passive"));
    assert!(resistor.pins.iter().any(|p| p.position.y < 0.0));
    assert!(
        resistor
            .graphics
            .iter()
            .any(|g| matches!(g.primitive, Graphic::Rectangle { .. }))
    );
    let led = symbols
        .iter()
        .find(|s| s.name == "LED")
        .ok_or("LED missing")?;
    assert_eq!(led.pins.len(), 2);
    assert!(led.pins.iter().any(|p| p.name == "A"));
    assert!(!led.graphics.is_empty());
    assert!(!led.raw_source.starts_with("(kicad_symbol_lib"));
    Ok(())
}
#[test]
fn real_ic_resolves_inheritance_and_units() -> Result<(), Box<dyn std::error::Error>> {
    let symbols = read_symbols(include_str!("fixtures/lm358.kicad_sym"))?;
    let ic = symbols
        .iter()
        .find(|s| s.name == "LM358")
        .ok_or("LM358 missing")?;
    assert_eq!(ic.extends.as_deref(), Some("LM2904"));
    assert_eq!(ic.value, "LM358");
    assert_eq!(ic.pins.len(), 8);
    assert!(
        ic.pins
            .iter()
            .any(|p| p.unit == 3 && p.electrical_type == "power_in")
    );
    assert!(ic.graphics.iter().any(|g| g.unit == 2));
    Ok(())
}
#[test]
fn real_0805_preserves_roundrect_pads_and_layers() -> Result<(), Box<dyn std::error::Error>> {
    let fp = read_footprint(include_str!("fixtures/R_0805_2012Metric.kicad_mod"))?;
    assert_eq!(fp.pads.len(), 2);
    assert!(fp.pads.iter().all(|p| p.shape == "roundrect"
        && p.pad_type == "smd"
        && p.layers.iter().any(|l| l == "F.Cu")));
    assert!(fp.pads[0].position.x < 0.0);
    assert!(fp.pads[0].roundrect_ratio.is_some());
    assert!(
        fp.graphics
            .iter()
            .any(|g| matches!(g,Graphic::Rectangle{layer,..}if layer=="F.CrtYd"))
    );
    assert!(fp.unsupported.is_empty());
    assert!(fp.raw_source.contains("(model"));
    Ok(())
}

#[test]
fn real_dip8_preserves_through_holes_and_reference_marker() -> Result<(), Box<dyn std::error::Error>>
{
    let fp = read_footprint(include_str!("fixtures/DIP-8_W7.62mm.kicad_mod"))?;
    assert_eq!(fp.pads.len(), 8);
    assert!(fp.unsupported.is_empty());
    assert!(fp.pads.iter().all(|p| {
        p.pad_type == "thru_hole"
            && p.layers.iter().any(|l| l == "*.Cu")
            && p.drill
                .as_ref()
                .is_some_and(|d| d.size.x == 0.8 && d.size.y == 0.8)
    }));
    assert_eq!(fp.pads[0].shape, "roundrect");
    assert!(fp.raw_source.contains("(point"));
    assert!(read_footprint("(footprint X (point (at NaN 0) (size 1) (layer F.Fab)))").is_err());
    Ok(())
}

#[test]
#[ignore = "requires installed KiCad 10 libraries in Linux/WSL"]
fn installed_library_catalogs() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["Device", "Amplifier_Operational"] {
        let text = std::fs::read_to_string(format!("/usr/share/kicad/symbols/{name}.kicad_sym"))?;
        let symbols = read_symbols(&text)?;
        assert!(symbols.len() > 100);
    }
    Ok(())
}
#[test]
fn hostile_inputs_and_native_dto_validation() -> Result<(), Box<dyn std::error::Error>> {
    for source in [
        "(",
        "(kicad_symbol_lib))",
        "(kicad_symbol_lib (symbol A (extends B)) (symbol B (extends A)))",
        "(kicad_symbol_lib (symbol A (extends Missing)))",
    ] {
        assert!(read_symbols(source).is_err());
    }
    let deep = format!("{}{}", "(".repeat(70), ")".repeat(70));
    assert!(read_symbols(&deep).is_err());
    assert!(
        read_footprint("(footprint X (pad 1 smd circle (at NaN 0) (size 1 1) (layers F.Cu)))")
            .is_err()
    );
    let mut fp = read_footprint(include_str!("fixtures/R_0805_2012Metric.kicad_mod"))?;
    fp.pads[0].size.x = f64::INFINITY;
    assert!(fp.validate().is_err());
    let mut symbols = read_symbols(include_str!("fixtures/device.kicad_sym"))?;
    let s = symbols.first_mut().ok_or("no symbols")?;
    s.pins.push(s.pins[0].clone());
    assert!(s.validate().is_err());
    Ok(())
}
#[test]
fn arbitrary_pin_numbers_unknown_geometry_and_drills() -> Result<(), Box<dyn std::error::Error>> {
    let s = read_symbols(
        "(kicad_symbol_lib (symbol X (symbol X_1_1 (pin input line (at 1 2 90) (length 2) (name CLK) (number A12)))))",
    )?;
    assert_eq!(s[0].pins[0].number, "A12");
    assert_eq!(s[0].pins[0].position.y, -2.0);
    let fp = read_footprint(
        "(footprint X (pad A12 thru_hole custom (at 1 2 90) (size 2 3) (drill oval 1 2 (offset 0.1 0.2)) (layers *.Cu *.Mask) (primitives (gr_poly))))",
    )?;
    assert_eq!(fp.pads[0].position.y, 2.0);
    assert_eq!(fp.pads[0].rotation, 90.0);
    assert_eq!(fp.pads[0].drill.as_ref().ok_or("no drill")?.size.y, 2.0);
    assert!(!fp.unsupported.is_empty());
    Ok(())
}

#[test]
fn inheritance_overrides_properties_and_replaces_local_units()
-> Result<(), Box<dyn std::error::Error>> {
    let symbols = read_symbols(
        r#"(kicad_symbol_lib
        (symbol Parent (property "Reference" "U") (property "Description" "base")
          (symbol Parent_1_1 (pin passive line (at 0 0) (length 1) (name OLD) (number 1)))
          (symbol Parent_0_1 (rectangle (start -1 -1) (end 1 1))))
        (symbol Child (extends "Parent") (property "Description" "override")
          (symbol Child_1_1 (pin input line (at 1 2 90) (length 2) (name NEW) (number A))))
    )"#,
    )?;
    let child = symbols
        .iter()
        .find(|s| s.name == "Child")
        .ok_or("missing child")?;
    assert_eq!(child.reference, "U");
    assert_eq!(child.description, "override");
    assert_eq!(child.pins.len(), 1);
    assert_eq!(child.pins[0].number, "A");
    assert_eq!(child.graphics[0].unit, 0);
    assert!(read_symbols("(kicad_symbol_lib (symbol X (symbol X_1_1) (symbol X_1_1)))").is_err());
    Ok(())
}

#[test]
fn footprint_models_are_recorded_and_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let fp = read_footprint(
        "(footprint M (at 0 0 0) (layer F.Cu)\
         (model \"pkg.wrl\" (offset (xyz 0 0 1)) (scale (xyz 2 2 2)) (rotate (xyz 0 0 90)))\
         (pad 1 smd circle (at 0 0) (size 1 1) (layers F.Cu)))",
    )?;
    let m = fp.model.as_ref().ok_or("model missing")?;
    assert_eq!(m.path, "pkg.wrl");
    assert_eq!(m.offset, [0.0, 0.0, 1.0]);
    assert_eq!(m.scale, [2.0, 2.0, 2.0]);
    assert_eq!(m.rotate, [0.0, 0.0, 90.0]);
    assert!(
        fp.unsupported.iter().all(|u| !u.contains("model")),
        "a recorded model is no longer unsupported"
    );
    let modern = read_footprint(
        "(footprint M (at 0 0 0) (layer F.Cu)\
         (model \"pkg.step\" (at (xyz 1 2 3)) (rotate (xyz 0 90 0)))\
         (pad 1 smd circle (at 0 0) (size 1 1) (layers F.Cu)))",
    )?;
    let m = modern.model.as_ref().ok_or("at-form model missing")?;
    assert_eq!(
        m.offset,
        [25.4, 50.8, 76.2],
        "the legacy at form is inches, stored as millimetres"
    );
    let same = read_footprint(
        "(footprint M (at 0 0 0) (layer F.Cu)\
         (model \"pkg.step\" (offset (xyz 25.4 50.8 76.2)))\
         (pad 1 smd circle (at 0 0) (size 1 1) (layers F.Cu)))",
    )?;
    assert_eq!(
        same.model
            .as_ref()
            .ok_or("offset-form model missing")?
            .offset,
        m.offset,
        "the legacy and modern forms read to the same millimetres"
    );
    let both = read_footprint(
        "(footprint M (at 0 0 0) (layer F.Cu)\
         (model \"pkg.step\" (at (xyz 1 1 1)) (offset (xyz 2 3 4)))\
         (pad 1 smd circle (at 0 0) (size 1 1) (layers F.Cu)))",
    )?;
    assert_eq!(
        both.model.as_ref().ok_or("both-form model missing")?.offset,
        [2.0, 3.0, 4.0],
        "offset wins over at"
    );
    assert!(
        read_footprint(
            "(footprint M (at 0 0 0) (layer F.Cu) (model \"a.wrl\" (at (xyz 99999 0 0))))"
        )
        .is_err(),
        "a legacy offset that converts out of bounds is rejected"
    );
    assert_eq!(m.scale, [1.0, 1.0, 1.0], "an absent scale is unit size");
    let plain = read_footprint(
        "(footprint M (at 0 0 0) (layer F.Cu) (model \"pkg.wrl\")\
         (pad 1 smd circle (at 0 0) (size 1 1) (layers F.Cu)))",
    )?;
    let m = plain.model.as_ref().ok_or("bare model missing")?;
    assert_eq!(m.offset, [0.0, 0.0, 0.0]);
    assert!(
        read_footprint(
            "(footprint M (at 0 0 0) (layer F.Cu) (model \"a.wrl\" (offset (xyz 0 0 NaN))))"
        )
        .is_err(),
        "a non-finite model offset is rejected"
    );
    assert!(
        read_footprint(
            "(footprint M (at 0 0 0) (layer F.Cu) (model \"a.wrl\" (scale (xyz 1 0 1))))"
        )
        .is_err(),
        "a zero model scale is rejected"
    );
    assert!(
        read_footprint("(footprint M (at 0 0 0) (layer F.Cu) (model \"\"))").is_err(),
        "an empty model path is rejected"
    );
    let doubled = read_footprint(
        "(footprint M (at 0 0 0) (layer F.Cu) (model \"a.wrl\") (model \"b.wrl\"))",
    )?;
    assert_eq!(
        doubled.model.as_ref().ok_or("first model missing")?.path,
        "a.wrl",
        "the first model wins"
    );
    assert!(
        doubled
            .unsupported
            .iter()
            .any(|u| u.contains("more than one 3D model")),
        "extra models are reported"
    );
    Ok(())
}

#[test]
fn specctra_session_net_maps_are_split_bounded_and_skipped_when_empty()
-> Result<(), Box<dyn std::error::Error>> {
    let session = r#"(session
        (parser
            (library 0)
        )
        (net_map_out
            (net
                (number 0)
                (name UNCONNECTED)
            )
            (net
                (number 1)
                (name GND)
                (pins
                    (pin R1-1)
                    (pin C1-2)
                    (pin J1-A)
                )
            )
            (net
                (number 2)
                (name "5V")
                (pins
                    (pin U1-8)
                    (pin C1-1)
                )
            )
        )
    )"#;
    let nets = pcbcraft_library::read_specctra_session(session)?;
    assert_eq!(nets.len(), 2, "the pin-less net is skipped");
    assert_eq!(nets[0].name, "GND");
    assert_eq!(nets[0].pins[0], ("R1".into(), "1".into()));
    assert_eq!(
        nets[0].pins[2],
        ("J1".into(), "A".into()),
        "lettered pins split at the last dash"
    );
    assert_eq!(nets[1].name, "5V", "quoted names are unquoted");
    assert_eq!(nets[1].pins[1], ("C1".into(), "1".into()));
    assert!(pcbcraft_library::read_specctra_session("(board X)").is_err());
    assert!(pcbcraft_library::read_specctra_session("(session (parser (library 0)))").is_err());
    assert!(
        pcbcraft_library::read_specctra_session(
            "(session (net_map_out (net (name GND) (pins (pin R1)))))"
        )
        .is_err(),
        "a pin without a dash is refused"
    );
    assert!(
        pcbcraft_library::read_specctra_session(
            "(session (net_map_out (net (name \"\") (pins (pin R1-1)))))"
        )
        .is_err(),
        "an empty net name is refused"
    );
    let many = (0..257)
        .map(|i| format!("(net (number {i}) (name N{i}) (pins (pin R1-1)))"))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        pcbcraft_library::read_specctra_session(&format!("(session (net_map_out {many}))"))
            .is_err(),
        "more than 256 nets are refused"
    );
    Ok(())
}

#[test]
fn footprint_rule_overrides_are_recorded_bounded_and_deduplicated()
-> Result<(), Box<dyn std::error::Error>> {
    let base = r#"(footprint "R_X" (version 20240101) (layer F.Cu) (at 0 0)
        (pad "1" smd rect (at -1 0) (size 1 1) (layers "F.Cu" "F.Paste" "F.Mask"))
    )"#;
    let plain = pcbcraft_library::read_footprint(base)?;
    assert!(
        plain.rules.is_none(),
        "a plain footprint carries no overrides"
    );
    let tuned = pcbcraft_library::read_footprint(&base.replace(
        "(at 0 0)",
        "(at 0 0) (clearance 0.15) (solder_mask_margin 0.05) (solder_paste_margin -0.02)",
    ))?;
    let rules = tuned.rules.ok_or("overrides should be recorded")?;
    assert_eq!(rules.clearance, Some(0.15));
    assert_eq!(rules.mask_margin, Some(0.05));
    assert_eq!(
        rules.paste_margin,
        Some(-0.02),
        "a negative paste margin shrinks the opening"
    );
    let doubled = pcbcraft_library::read_footprint(
        &base.replace("(at 0 0)", "(at 0 0) (clearance 0.15) (clearance 0.2)"),
    )?;
    assert!(
        doubled
            .unsupported
            .iter()
            .any(|u| u.contains("duplicate local clearance")),
        "a second clearance is reported, not silently merged"
    );
    assert!(
        pcbcraft_library::read_footprint(&base.replace("(at 0 0)", "(at 0 0) (clearance 150)"))
            .is_err(),
        "an override beyond 100 mm is refused"
    );
    Ok(())
}

/// An original footprint written for this test: a chamfered polarity pad, a
/// concave custom pad, and unsupported variants that must be reported rather
/// than silently simplified.
const SHAPED_PADS: &str = r#"(footprint "ShapedPads" (version 20240108) (generator "test")
  (layer "F.Cu")
  (pad "1" smd roundrect (at -1.5 0) (size 1.2 1.6) (layers "F.Cu" "F.Mask")
    (roundrect_rratio 0) (chamfer_ratio 0.3) (chamfer top_left bottom_left))
  (pad "2" smd custom (at 1.5 0) (size 0.4 0.4) (layers "F.Cu" "F.Mask")
    (options (clearance outline) (anchor rect))
    (primitives
      (gr_poly (pts (xy -0.6 -0.8) (xy 0.6 -0.8) (xy 0.6 0.8) (xy 0 0.4) (xy -0.6 0.8)) (width 0) (fill yes))))
  (pad "3" smd rect (at 0 2) (size 1 1) (layers "F.Cu") (chamfer_ratio 0.2) (chamfer bottom_right))
  (pad "4" smd roundrect (at 0 -2) (size 1 1) (layers "F.Cu") (roundrect_rratio 0.25) (chamfer_ratio 0.2) (chamfer top_right))
  (pad "5" smd custom (at 3 2) (size 0.4 0.4) (layers "F.Cu")
    (primitives (gr_poly (pts (xy -1 -1) (xy 1 1) (xy 1 -1) (xy -1 1)) (width 0) (fill yes))))
  (pad "6" smd custom (at 3 -2) (size 0.4 0.4) (layers "F.Cu")
    (primitives (gr_circle (center 0 0) (end 0.5 0) (width 0) (fill yes))))
)"#;

#[test]
fn chamfered_and_custom_kicad_pads_import_with_exact_outlines()
-> Result<(), Box<dyn std::error::Error>> {
    use pcbcraft_library::{ChamferCorners, PadShape};
    let f = read_footprint(SHAPED_PADS)?;
    let pad = |n: &str| f.pads.iter().find(|p| p.number == n).ok_or("missing pad");
    let one = pad("1")?;
    assert_eq!(one.shape, "chamfered");
    assert_eq!(one.roundrect_ratio, None);
    assert_eq!(
        one.shape_kind(),
        PadShape::Chamfered(pcbcraft_library::PadChamfer {
            ratio: 0.3,
            corners: ChamferCorners {
                top_left: true,
                bottom_left: true,
                ..Default::default()
            }
        })
    );
    let two = pad("2")?;
    assert_eq!(two.shape, "custom");
    assert_eq!(two.polygon.len(), 5);
    assert!(matches!(two.shape_kind(), PadShape::Custom { polygon } if polygon.len() == 5));
    // A plain rect with a chamfer is a chamfered pad as well.
    assert_eq!(pad("3")?.shape, "chamfered");
    // Rounded and chamfered together, self-intersecting polygons and
    // non-polygon primitives are kept as data but flagged unsupported.
    assert_eq!(pad("4")?.shape, "roundrect");
    assert!(pad("4")?.chamfer.is_none());
    assert!(pad("5")?.polygon.is_empty());
    assert!(pad("6")?.polygon.is_empty());
    for n in [
        "pad 4 chamfer",
        "pad 5:",
        "pad 6 custom primitive gr_circle",
    ] {
        assert!(
            f.unsupported.iter().any(|u| u.starts_with(n)),
            "{n} not reported in {:?}",
            f.unsupported
        );
    }
    assert_eq!(f.unsupported.len(), 3);
    Ok(())
}

#[test]
fn reader_keeps_inverted_clock_pin_style_off_unsupported_list() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"(kicad_symbol_lib (version 20260206) (generator pcbcraft)
  (symbol "INVCLK"
    (property "Reference" "U" (at 0 0 0)
      (effects (font (size 1.27 1.27) (thickness 0.15))))
    (symbol "INVCLK_1_1"
      (pin input inverted_clock (at 0 0 180) (length 2.54)
        (name "CLK" (effects (font (size 1.27 1.27))))
        (number "1" (effects (font (size 1.27 1.27)))))
    )
  )
)"#;
    let symbols = read_symbols(source)?;
    let symbol = symbols
        .iter()
        .find(|s| s.name == "INVCLK")
        .ok_or("fixture missing")?;
    assert_eq!(symbol.pins.len(), 1);
    assert_eq!(symbol.pins[0].graphical_style, "inverted_clock");
    assert!(symbol.pins[0].visible);
    assert!(
        symbol
            .unsupported
            .iter()
            .all(|u| !u.contains("graphical style")),
        "known styles must not land in unsupported: {:?}",
        symbol.unsupported
    );
    Ok(())
}

#[test]
fn symbol_pin_graphical_style_rejects_unknown_token() {
    let pin = SymbolPin {
        number: "1".into(),
        name: "~".into(),
        position: pcbcraft_geom::Point::new(0.0, 0.0),
        angle: 0.0,
        length: 2.54,
        electrical_type: "passive".into(),
        unit: 1,
        body_style: 1,
        graphical_style: "line".into(),
        visible: true,
    };
    let mut symbol = pcbcraft_library::SymbolDefinition {
        name: "X".into(),
        reference: "U".into(),
        value: "X".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: String::new(),
        set: String::new(),
        pins: vec![pin.clone()],
        graphics: vec![],
        extends: None,
        raw_source: String::new(),
        unsupported: vec![],
    };
    assert!(symbol.validate().is_ok());
    symbol.pins[0].graphical_style = "mystery_shape".into();
    assert!(symbol.validate().is_err());
}
