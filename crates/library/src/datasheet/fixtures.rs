//! Small PDF fixtures generated in code for tests (no downloaded files, no vendor datasheets).
//!
//! Context: spike #723 and the reader tests. Real datasheets are never committed, so tests need their own inputs. Each fixture
//! is a hand-written, valid PDF with a correct xref table, built from page content strings.
//!
//! Local rules: only the 14 standard fonts, so rendering depends on hayro's embedded substitutes.
//!
//! # Do not
//! - Grow this into a PDF writer; add fixtures as new `Fixture` entries instead.
//! - Commit a downloaded datasheet.

/// One generated fixture: a file name, the PDF bytes and strings the text layer must contain.
pub struct Fixture {
    /// File name.
    pub name: &'static str,
    /// PDF bytes.
    pub bytes: Vec<u8>,
    /// Strings that search must find on page 1 (or the page given in `.1`).
    pub expect: Vec<(usize, &'static str)>,
    /// Whether page 1 should contain visible ink after rendering.
    pub has_ink: bool,
}

/// Build a PDF from page content streams. `rotate` applies to every page.
fn build(pages: &[(&str, (u32, u32))], rotate: u32) -> Vec<u8> {
    let mut objs: Vec<String> = Vec::new();
    // 1 catalog, 2 pages, 3 Helvetica, 4 Courier, then (page, content) pairs.
    objs.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids: Vec<String> = (0..pages.len())
        .map(|i| format!("{} 0 R", 5 + 2 * i))
        .collect();
    objs.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    ));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into());
    for (i, (content, (w, h))) in pages.iter().enumerate() {
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Rotate {rotate} /Contents {} 0 R /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> >>",
            6 + 2 * i
        ));
        objs.push(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            content.len(),
            content
        ));
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// All fixtures.
pub fn all() -> Vec<Fixture> {
    let text = "BT /F1 18 Tf 72 720 Td (LM317 Adjustable Regulator) Tj ET\n\
                BT /F1 11 Tf 72 690 Td (Vin 3 V to 40 V, Iout 1.5 A, TO-220) Tj ET\n\
                BT /F2 10 Tf 72 660 Td (Pin 1 ADJ  Pin 2 OUT  Pin 3 IN) Tj ET";
    let shapes = "0.2 0.4 0.8 rg 100 500 200 100 re f\n1 0 0 RG 4 w 100 400 m 300 450 l S\n\
                  0 0.6 0 RG 2 w 100 300 m 150 380 250 220 300 300 c S\n\
                  0.9 0.9 0.2 rg 350 500 m 450 500 l 400 600 l f\nBT /F1 12 Tf 100 250 Td (Package outline) Tj ET";
    let p2 = "BT /F1 14 Tf 72 700 Td (Absolute Maximum Ratings) Tj ET\nBT /F1 10 Tf 72 680 Td (Storage -65 to 150 C) Tj ET";
    let sheet = "BT /F1 20 Tf 50 740 Td (NR1206 series thick film chip resistors) Tj ET\n\
                BT /F1 10 Tf 50 722 Td (Revision D - General purpose - RoHS compliant) Tj ET\n\
                0.7 g 50 700 512 1 re f 0 g\n\
                BT /F1 13 Tf 50 650 Td (Electrical characteristics) Tj ET\n\
                BT /F1 11 Tf 50 620 Td (Resistance range) Tj ET BT /F2 11 Tf 300 620 Td (10 ohm to 10 Mohm) Tj ET\n\
                BT /F1 11 Tf 50 590 Td (Tolerance) Tj ET BT /F2 11 Tf 300 590 Td (1 %) Tj ET\n\
                BT /F1 11 Tf 50 560 Td (Power rating at 70 C) Tj ET BT /F2 11 Tf 300 560 Td (0.25 W) Tj ET\n\
                BT /F1 11 Tf 50 530 Td (Working voltage) Tj ET BT /F2 11 Tf 300 530 Td (200 V) Tj ET\n\
                BT /F1 11 Tf 50 500 Td (Temperature coefficient) Tj ET BT /F2 11 Tf 300 500 Td (100 ppm/C) Tj ET\n\
                BT /F1 13 Tf 50 440 Td (Dimensions) Tj ET\n\
                0.4 g 150 330 150 80 re S 0.75 g 150 330 34 80 re f 266 330 34 80 re f 0 g\n\
                BT /F2 10 Tf 200 310 Td (3.2 mm) Tj ET";
    vec![
        Fixture {
            name: "resistor-datasheet.pdf",
            bytes: build(&[(sheet, (612, 792))], 0),
            expect: vec![(0, "NR1206"), (0, "Power rating at 70"), (0, "0.25 W")],
            has_ink: true,
        },
        Fixture {
            name: "text-one-page.pdf",
            bytes: build(&[(text, (612, 792))], 0),
            expect: vec![(0, "LM317"), (0, "TO-220"), (0, "1.5 A"), (0, "Pin 3 IN")],
            has_ink: true,
        },
        Fixture {
            name: "vector-shapes.pdf",
            bytes: build(&[(shapes, (612, 792))], 0),
            expect: vec![(0, "Package outline")],
            has_ink: true,
        },
        Fixture {
            name: "two-pages.pdf",
            bytes: build(&[(text, (612, 792)), (p2, (595, 842))], 0),
            expect: vec![
                (0, "LM317"),
                (1, "Absolute Maximum Ratings"),
                (1, "-65 to 150"),
            ],
            has_ink: true,
        },
        Fixture {
            name: "rotated-90.pdf",
            bytes: build(&[(text, (612, 792))], 90),
            expect: vec![(0, "LM317")],
            has_ink: true,
        },
    ]
}
