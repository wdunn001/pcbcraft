//! Widening a symbol whose pin names meet in the middle.
//!
//! A 40-pin header draws names like GPIO4_C3/SPI3_MOSI on both sides of a
//! body too narrow to hold them, and the two columns overrun each other. The
//! canvas paints a handle on each corner of a selected symbol, which reads as
//! an invitation to drag, and nothing reads those handles. Properties is
//! where the size can actually be changed.
use super::super::*;
use super::support::*;

/// A few frames so the panel has drawn and any edit has landed.
fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

#[test]
fn a_newcomer_widens_a_symbol_whose_pin_names_collide() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    // A part drawn from a library symbol. The built-in kinds carry no symbol
    // definition of their own, so the body size does not apply to them.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../library/tests/fixtures/device.kicad_sym");
    app.tool_call(
        "component.place_library",
        json!({
            "path": fixture.to_string_lossy(),
            "symbol": "R",
            "position": {"x": 50.0, "y": 40.0}
        }),
    );
    settle(&ctx, &mut app);

    let part = app
        .project
        .components
        .iter()
        .find(|c| c.library_symbol.is_some())
        .map(|c| c.id)
        .ok_or("a part placed from a library symbol")?;
    app.selected = Some(part);
    app.change_tab(Tab::Schematic);
    settle(&ctx, &mut app);

    assert!(
        paints_text(&ctx, &mut app, "Reference")?,
        "Properties is open on the selected part"
    );
    // The section headings sit in collapsed bodies in this harness, so the
    // panel's own text is not what is read here. What the row does when it
    // changes is, and that is the same call the row makes.

    let body_of = |app: &App, id: Id| -> Option<(f64, f64)> {
        let c = app.project.component(id)?;
        let symbol = c.library_symbol.as_ref()?;
        symbol
            .definition
            .graphics
            .iter()
            .filter_map(|g| match &g.primitive {
                pcbcraft_library::Graphic::Rectangle { start, end, .. } => {
                    Some(((end.x - start.x).abs(), (end.y - start.y).abs()))
                }
                _ => None,
            })
            .max_by(|a, b| (a.0 * a.1).total_cmp(&(b.0 * b.1)))
    };

    let before = body_of(&app, part).ok_or("the part draws a body")?;

    assert!(
        app.tool_call(
            "component.symbol_body_resize",
            json!({"id": part, "width": 40.0, "height": 20.0}),
        ),
        "the part takes a new body size"
    );
    settle(&ctx, &mut app);

    let widened = body_of(&app, part).ok_or("the part still draws a body")?;
    assert!(
        (widened.0 - 40.0).abs() < 1e-6 && (widened.1 - 20.0).abs() < 1e-6,
        "the body took the width it was given, got {widened:?}, was {before:?}"
    );
    Ok(())
}
