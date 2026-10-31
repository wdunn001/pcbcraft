//! Minimal valid PDF bytes for unit tests (no downloaded files).

/// One-page PDF with visible text (standard Helvetica substitute).
pub fn one_page_text() -> Vec<u8> {
    let content = "BT /F1 18 Tf 72 720 Td (Sample Datasheet Page) Tj ET";
    build(&[(content, (612, 792))])
}

fn build(pages: &[(&str, (u32, u32))]) -> Vec<u8> {
    let mut objs: Vec<String> = Vec::new();
    objs.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids: Vec<String> = (0..pages.len())
        .map(|i| format!("{} 0 R", 4 + 2 * i))
        .collect();
    objs.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    ));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for (i, (content, (w, h))) in pages.iter().enumerate() {
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
            5 + 2 * i
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
