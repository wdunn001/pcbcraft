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
fn a_newcomer_picks_the_layer_to_draw_on_from_the_tool_bar() -> Result<(), String> {
    use crate::draw_target::{Side, Target};
    let (ctx, mut app) = layout_page();
    // The status bar names the layer too; pressing its name changes nothing.
    tap_id(&ctx, &mut app, "button:Layer: Top copper")?;
    assert_eq!(
        app.current_target(),
        Target::Copper(pcbcraft_doc::Layer::Front)
    );
    tap_id(&ctx, &mut app, "combobox:Layer")?;
    for choice in ["Top silkscreen", "Bottom silkscreen", "Board edge"] {
        assert!(
            paints_text(&ctx, &mut app, choice)?,
            "the Layer box lists {choice}"
        );
    }
    // The choices drop down under the box: press the one called Top silkscreen.
    let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let at = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "Top silkscreen" => {
                Some(t.pos + t.galley.size() / 2.0)
            }
            _ => None,
        })
        .next_back()
        .ok_or("the Layer box should list Top silkscreen")?;
    tap_at(&ctx, &mut app, at)?;
    assert_eq!(
        app.current_target(),
        Target::Silk(Side::Top),
        "the Layer choice sets what the drawing tools draw on: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_corners_between_45_and_90_degrees() -> Result<(), String> {
    let (ctx, mut app) = layout_page();
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

/// Press the 45 (true) or 90 (false) choice of the Corners switch: the two
/// choices read the same to the gate, so go by their place in the row.
fn tap_toolbar_corner(ctx: &egui::Context, app: &mut App, chamfer: bool) -> Result<(), String> {
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
fn a_newcomer_updates_the_layout_from_the_schematic() -> Result<(), String> {
    let (ctx, mut app) = layout_page();
    tap_id(&ctx, &mut app, "button:Update from schematic")?;
    assert!(
        app.message.contains("board.sync"),
        "the button runs the sync and says so: {}",
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
