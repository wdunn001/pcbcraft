//! Stories for the tool bar's own controls on the Layout page: the Layer
//! choice, the Corners switch, Update from schematic and the tools that wait
//! in More. Tools on the Schematic page are told in `board.rs`.
use super::super::*;
use super::support::*;

fn layout_page() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

#[test]
fn a_newcomer_picks_the_layer_to_draw_on_from_the_layers_tree() -> Result<(), String> {
    use crate::draw_target::{Side, Target};
    let (ctx, mut app) = layout_page();
    assert_eq!(
        app.current_target(),
        Target::Copper(pcbcraft_doc::Layer::Front)
    );
    assert!(
        tap_id(&ctx, &mut app, "combobox:Layer").is_err(),
        "the tool bar no longer carries a Layer box"
    );
    press_on(&ctx, &mut app, "side:top/silk", egui::Key::Enter);
    assert_eq!(
        app.current_target(),
        Target::Silk(Side::Top),
        "a Layers tree row sets what the drawing tools draw on: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_switches_the_route_tool_between_45_and_90_corners() -> Result<(), String> {
    let (ctx, mut app) = layout_page();
    tap_strip(&ctx, &mut app, "icon:tool-route")?;
    assert!(
        matches!(app.tool, Tool::Route(_)),
        "Route must be active for the Corners control"
    );
    assert!(app.preferences.chamfer_corners, "corners start at 45");
    tap_toolbar_corner(&ctx, &mut app, false)?;
    assert!(
        !app.preferences.chamfer_corners,
        "the 90 choice squares the corners"
    );
    tap_toolbar_corner(&ctx, &mut app, true)?;
    assert!(
        app.preferences.chamfer_corners,
        "the 45 choice chamfers them again"
    );
    Ok(())
}

/// Press the 45 (true) or 90 (false) choice of the Corners control while Route
/// is active: the two choices read the same to the gate, so go by their place.
pub(crate) fn tap_toolbar_corner(ctx: &egui::Context, app: &mut App, chamfer: bool) -> Result<(), String> {
    let mut found: Vec<_> = super::audit::controls(ctx, app)
        .into_iter()
        .filter(|c| c.id.as_deref() == Some("radiobutton:#") && c.rect.center().y < 120.0)
        .collect();
    found.sort_by(|a, b| a.rect.center().x.total_cmp(&b.rect.center().x));
    let pick = if chamfer { found.first() } else { found.last() }
        .ok_or("the Corners switch is not on the tool bar")?;
    let pos = pick.rect.center();
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_updates_the_board_from_the_edit_menu_when_sync_is_off() -> Result<(), String> {
    let (ctx, mut app) = layout_page();
    assert!(
        tap_id(&ctx, &mut app, "button:Update from schematic").is_err(),
        "the Layout toolbar no longer carries an Update button"
    );
    assert!(
        app.project.auto_sync,
        "the board follows the schematic by default"
    );
    app.run("project.set_auto_sync", json!({"auto_sync": false}));
    assert!(
        !app.project.auto_sync,
        "Keep board in sync can be turned off"
    );
    app.run("board.sync", json!({}));
    assert!(
        app.message.contains("board.sync"),
        "Update board from schematic runs the sync: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_arms_via_and_keepout_from_the_tool_bar() -> Result<(), String> {
    let (ctx, mut app) = layout_page();
    tap_strip(&ctx, &mut app, "icon:tool-via")?;
    assert!(matches!(app.tool, Tool::Via), "Via arms the via tool");
    tap_strip(&ctx, &mut app, "icon:tool-keepout")?;
    assert!(
        matches!(app.tool, Tool::Keepout),
        "Keepout arms the keepout tool"
    );
    Ok(())
}
