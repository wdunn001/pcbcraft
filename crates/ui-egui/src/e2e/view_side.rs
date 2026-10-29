//! Story for the Top | Bottom control on the Layout canvas.
use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_flips_the_view_to_the_bottom_with_the_toggle() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.seen_from_below(), "the board opens from the top");
    assert_eq!(app.route_layer, pcbcraft_doc::Layer::Front);

    authoring::tests::tap(&ctx, &mut app, "Bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.seen_from_below(), "Bottom mirrors the view");
    assert_eq!(
        app.route_layer,
        pcbcraft_doc::Layer::Back,
        "bottom copper is now the drawing target"
    );

    app.set_target(crate::draw_target::Target::Silk(
        crate::draw_target::Side::Top,
    ));
    authoring::tests::tap(&ctx, &mut app, "Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.seen_from_below(), "Top is the normal view again");
    authoring::tests::tap(&ctx, &mut app, "Bottom")?;
    assert_eq!(
        app.target,
        crate::draw_target::Target::Silk(crate::draw_target::Side::Bottom),
        "bottom silkscreen becomes the drawing target"
    );
    Ok(())
}
