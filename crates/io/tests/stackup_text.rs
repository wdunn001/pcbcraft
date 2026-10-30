//! Stackup text export for fabrication (issue #652).
use pcbcraft_doc::{LayerRecord, Project};
use pcbcraft_io::stackup_text;

fn row(name: &str, kind: &str, thickness: f64) -> LayerRecord {
    LayerRecord {
        name: name.into(),
        kind: kind.into(),
        thickness,
        ..Default::default()
    }
}

#[test]
fn the_stackup_sheet_lists_the_new_rows_and_the_copper_they_cover() {
    let mut p = Project::default();
    p.board.layer_records = vec![
        row("F.Mask", "soldermask", 0.02),
        row("F.Silk", "silkscreen", 0.015),
        row("F.Cu", "copper", 0.035),
        row("Flex adhesive", "adhesive", 0.025),
        row("Core", "core", 1.2),
        row("B.Cu", "copper", 0.035),
        row("Cover", "coverlay", 0.03),
        row("Stiffener", "stiffener", 0.5),
        row("B.Peel", "peelable_mask", 0.01),
        row("B.Paste", "paste", 0.01),
    ];
    p.validate().expect("fixture validates");
    let text = stackup_text(&p).expect("stackup export");
    assert!(
        text.contains("Flex adhesive,adhesive,0.025,")
            && text.contains("Stiffener,stiffener,0.500,")
            && text.contains("B.Peel,peelable_mask,0.010,B.Cu"),
        "new rows appear on the sheet: {text}"
    );
    assert!(
        text.contains("F.Mask,soldermask,0.020,F.Cu")
            && text.contains("B.Paste,paste,0.010,B.Cu")
            && text.contains("F.Silk,silkscreen,0.015,F.Cu")
            && text.contains("Cover,coverlay,0.030,B.Cu"),
        "mask, paste and silk rows name the copper they sit on: {text}"
    );
    assert!(
        !text.contains("Core,core,1.200,F.") && !text.contains("Stiffener,stiffener,0.500,F."),
        "dielectric and stiffener rows leave ON_COPPER blank"
    );
}
