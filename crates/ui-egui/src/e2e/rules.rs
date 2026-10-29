use super::super::*;
use crate::test_support::{frame, paints_text, scrolled_to, scrolled_to_at, self_names, tap};
use serde_json::json;

    #[test]
    fn a_newcomer_adds_a_rule_set_and_tunes_its_numbers() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Rules")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "No rule sets yet")?,
            "a design without rule sets says so"
        );
        tap(&ctx, &mut app, "New class")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.project.net_classes.len(),
            1,
            "one click adds one rule set: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "Gap 0.200 mm")?,
            "the new rule set shows its starting numbers"
        );
        let id = app.project.net_classes[0].id;
        tap(&ctx, &mut app, "Tune")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.rule_draft = Some((id, 0.5, 0.6));
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Apply")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let class = &app.project.net_classes[0];
        assert!(
            (class.clearance - 0.5).abs() < 1e-9 && (class.width - 0.6).abs() < 1e-9,
            "the tuned numbers stick: {} {} · {}",
            class.clearance,
            class.width,
            app.message
        );
        tap(&ctx, &mut app, "Remove")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.net_classes.is_empty(),
            "the rule set is removed: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_puts_a_connection_under_a_rule_set_and_lets_it_go() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Rules")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "New class")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let vcc = app
            .project
            .nets
            .iter()
            .find(|n| n.name == "VCC")
            .map(|n| n.id)
            .ok_or("the sample should have a VCC connection")?;
        tap(&ctx, &mut app, "choose a connection to put here")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "VCC")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Assign")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.net_classes[0].nets.contains(&vcc),
            "the connection joins the rule set: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "1 nets")?,
            "the row counts its connections"
        );
        tap(&ctx, &mut app, "\u{d7}")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.net_classes[0].nets.is_empty(),
            "the connection is released to the defaults: {}",
            app.message
        );
        Ok(())
    }
