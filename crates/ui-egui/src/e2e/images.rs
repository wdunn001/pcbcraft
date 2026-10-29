use super::super::*;
use super::support::*;

const LOGO: &[u8] = include_bytes!("../../../io/tests/fixtures/logo-64x64-two-color.png");

#[test]
fn a_newcomer_puts_a_logo_on_the_silk() -> Result<(), String> {
    let file = std::env::temp_dir().join(format!("pcbcraft-logo-{}.png", std::process::id()));
    std::fs::write(&file, LOGO).map_err(|e| e.to_string())?;
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    go_to_tab(&ctx, &mut app, Tab::Settings)?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "REFERENCE IMAGE", 700.0)?,
        "the settings page has a plainly named reference image section"
    );
    assert!(
        paints_text(&ctx, &mut app, "PNG file path")?,
        "the box says what to type"
    );
    authoring::tests::tap(&ctx, &mut app, "PNG file path")?;
    for letter in file.to_string_lossy().chars() {
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::Text(letter.to_string())],
            egui::Modifiers::NONE,
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Attach PNG")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.reference_images.len(),
        1,
        "the picture is attached and shown under the board: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Trace to layer")?
            || scrolled_to_at(&ctx, &mut app, "Trace to layer", 700.0)?,
        "once a picture is attached the page offers to trace it"
    );
    authoring::tests::tap(&ctx, &mut app, "Trace to layer")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let graphic = app
        .project
        .graphics
        .first()
        .cloned()
        .ok_or_else(|| format!("tracing adds an outline: {}", app.message))?;
    assert_eq!(
        graphic.sublayer, "F.SilkS",
        "the default target is front silk"
    );
    assert_eq!(graphic.layer, "board");
    assert!(
        graphic.points.len() >= 3,
        "the outline has corners: {:?}",
        graphic.points
    );
    app.change_tab(Tab::Pcb);
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let closed: Vec<usize> = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Path(path) if path.closed => Some(path.points.len()),
            _ => None,
        })
        .collect();
    assert!(
        closed.iter().any(|n| *n >= graphic.points.len()),
        "the traced outline is drawn on the board: {} corners, closed paths {closed:?}, tab {:?}",
        graphic.points.len(),
        app.tab
    );
    let silk = pcbcraft_io::gerber_silk(&app.project, pcbcraft_doc::Layer::Front)?;
    assert!(
        silk.contains("D01"),
        "the front silk file carries the traced outline"
    );
    let _ = std::fs::remove_file(&file);
    Ok(())
}
