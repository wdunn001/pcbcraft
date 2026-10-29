use super::super::*;
use crate::test_support::{frame, paints_text, scrolled_to, scrolled_to_at, self_names, tap};
use serde_json::json;

    #[test]
    fn a_newcomer_copies_a_part_with_one_click() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let resistor = app
            .project
            .components
            .iter()
            .find(|c| c.reference.starts_with('R'))
            .map(|c| c.id)
            .ok_or("the practice design should carry a resistor")?;
        app.select(Some(resistor));
        app.refresh();
        let refs: Vec<String> = app
            .project
            .components
            .iter()
            .map(|c| c.reference.clone())
            .collect();
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Duplicate")?,
            "a chosen part offers to be copied: {}",
            app.message
        );
        tap(&ctx, &mut app, "Duplicate")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        let after: Vec<String> = app
            .project
            .components
            .iter()
            .map(|c| c.reference.clone())
            .collect();
        assert_eq!(
            after.len(),
            refs.len() + 1,
            "one more part sits on the board: {refs:?} -> {after:?}"
        );
        assert_eq!(
            after.iter().filter(|r| r.starts_with('R')).count(),
            refs.iter().filter(|r| r.starts_with('R')).count() + 1,
            "the copy got its own new reference: {after:?}"
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_freezes_a_part_so_nothing_moves_it() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.select(Some(10));
        app.refresh();
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Lock in place")?,
            "a chosen footprint offers to be frozen: {}",
            app.message
        );
        tap(&ctx, &mut app, "Lock in place")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        let frozen = |app: &App| {
            app.project
                .board
                .footprints
                .iter()
                .find(|f| f.id == 10)
                .and_then(|f| f.locked)
                == Some(true)
        };
        assert!(frozen(&app), "one click freezes the part: {}", app.message);
        assert!(
            !app.tool_call(
                "footprint.move",
                json!({"id":10,"position":{"x":20.0,"y":20.0}})
            ),
            "a frozen part refuses to be shoved: {}",
            app.message
        );
        tap(&ctx, &mut app, "Unlock")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert!(
            !frozen(&app),
            "unlocking lets the part move freely again: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_docks_a_part_flush_to_the_board_edge() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.select(Some(10));
        app.refresh();
        let before = app
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.id == 10)
            .map(|f| f.position.x)
            .ok_or("the practice board should carry J1's footprint")?;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Dock left")?,
            "a chosen footprint offers to sit flush to an edge: {}",
            app.message
        );
        tap(&ctx, &mut app, "Dock left")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        let after = app
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.id == 10)
            .map(|f| f.position.x)
            .ok_or("the practice board should still carry J1's footprint")?;
        assert!(
            after < before && after > 0.0,
            "the part slid in to kiss the left edge: {before} -> {after}"
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_smooths_a_corner_for_the_etcher() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.tool_call(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let (id, middle, before) = app
            .project
            .board
            .tracks
            .first()
            .map(|t| {
                (
                    t.id,
                    Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0),
                    app.project.board.tracks.len(),
                )
            })
            .ok_or("routing should leave a corner to smooth")?;
        app.change_tab(Tab::Pcb);
        app.canvas_click(middle, 15.0);
        assert_eq!(
            app.selected,
            Some(id),
            "the first leg of the route is picked up: {}",
            app.message
        );
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Cut corner")?,
            "a wire that meets another offers a corner cut: {}",
            app.message
        );
        tap(&ctx, &mut app, "Cut corner")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert!(
            app.project.board.tracks.len() > before,
            "the cut adds a diagonal so the run turns gently: {}",
            app.message
        );
        Ok(())
    }
