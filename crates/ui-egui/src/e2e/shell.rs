use super::super::*;
use crate::test_support::{frame, paints_text, scrolled_to, scrolled_to_at, self_names, tap};
use serde_json::json;

    #[test]
    fn a_newcomer_reads_the_status_strip() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Tool: Pick")?,
            "the status strip should say what a click does: {}",
            app.message
        );
        let pct = (app.schematic.zoom / 15.0 * 100.0).round() as i32;
        let zoom_label = format!("{pct}%");
        assert!(
            paints_text(&ctx, &mut app, &zoom_label)?,
            "the status strip should show the zoom level ({zoom_label}): {}",
            app.message
        );
        let mut mind = 0usize;
        for key in ["erc", "drc"] {
            if let Some(findings) = app.checks.get(key).and_then(serde_json::Value::as_array) {
                for finding in findings {
                    let code = finding
                        .get("code")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("finding");
                    if app.project.rule_severity(code, "warning") != "error" {
                        mind += 1;
                    }
                }
            }
        }
        assert!(
            mind > 0,
            "the practice board should leave something to mind"
        );
        let mind_label = format!("{mind} to mind");
        assert!(
            paints_text(&ctx, &mut app, &mind_label)?,
            "the status strip should count the warnings ({mind_label}): {}",
            app.message
        );
        tap(&ctx, &mut app, &mind_label)?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.tab,
            Tab::Checks,
            "tapping the warning count should open the checks list: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_adds_assembly_marks_for_the_build_camera() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.tab = Tab::Pcb;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Assembly mark")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.canvas_click(Point::new(-27.0, -17.0), 1.0);
        assert_eq!(
            app.project.board.fiducials.len(),
            1,
            "one click lays one mark: {}",
            app.message
        );
        app.canvas_click(Point::new(27.0, 17.0), 1.0);
        assert_eq!(
            app.project.board.fiducials.len(),
            2,
            "the tool stays ready for the next corner: {}",
            app.message
        );
        assert!(
            app.project
                .board
                .fiducials
                .iter()
                .all(|fid| (fid.diameter - 1.0).abs() < 1e-9),
            "both marks are the usual 1 mm disc: {}",
            app.message
        );
        app.tool = Tool::Select;
        app.tab = Tab::Schematic;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Layers")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            scrolled_to(&ctx, &mut app, "1.00 mm on Top")?,
            "the marks are listed with their size and side"
        );
        let last = app.project.board.fiducials[1].id;
        tap(&ctx, &mut app, "Tune")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.fiducial_edit = Some((last, 1.2, false));
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Apply")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.board.fiducials.iter().any(|fid| {
                fid.id == last
                    && (fid.diameter - 1.2).abs() < 1e-9
                    && fid.layer == pcbcraft_doc::Layer::Back
            }),
            "the mark is resized and moved to the bottom: {}",
            app.message
        );
        tap(&ctx, &mut app, "\u{d7}")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.project.board.fiducials.len(),
            1,
            "the other mark comes off the board: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_opens_rules_and_settings_from_the_top() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Rules")?;
        assert!(
            paints_text(&ctx, &mut app, "Design rules travel with the design")?,
            "the Rules tab explains what rules are for: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "New class")?,
            "the Rules tab offers a way to add a rule set"
        );
        tap(&ctx, &mut app, "Settings")?;
        assert!(
            paints_text(&ctx, &mut app, "APPEARANCE")?,
            "the Settings tab opens with the look of the app"
        );
        assert!(
            paints_text(&ctx, &mut app, "BOARD GEOMETRY")?,
            "the board settings that used to crowd the aside now live on the Settings tab"
        );
        assert!(
            paints_text(&ctx, &mut app, "Nothing selected")?,
            "the aside no longer holds the project settings"
        );
        tap(&ctx, &mut app, "Dark")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(app.preferences.dark, "the theme switch on the tab works");
        Ok(())
    }
