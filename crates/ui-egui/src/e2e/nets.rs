use super::super::*;
use crate::test_support::{frame, paints_text, scrolled_to, scrolled_to_at, self_names, tap};
use serde_json::json;

    #[test]
    fn a_newcomer_puts_a_pin_on_a_connection_and_takes_it_back_off() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call(
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"330","position":{"x":40.0,"y":14.0}}),
        );
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Layers")?;
        assert!(
            scrolled_to(&ctx, &mut app, "New net")?,
            "the layers tree offers a way to add a connection"
        );
        tap(&ctx, &mut app, "New net")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(app.project.nets.len(), 1, "one click adds one connection");
        assert!(
            paints_text(&ctx, &mut app, "No pins on this connection yet.")?,
            "a fresh connection says so plainly: {}",
            app.message
        );
        assert!(
            scrolled_to(&ctx, &mut app, "choose a spare pin")?,
            "the new connection offers a pin picker: {}",
            app.message
        );
        let net_id = app.project.nets[0].id;
        let on_net = |app: &App| -> usize {
            app.project
                .components
                .iter()
                .flat_map(|c| c.pins.iter())
                .filter(|p| p.net == Some(net_id))
                .count()
        };
        assert_eq!(on_net(&app), 0, "the resistor starts unconnected");
        tap(&ctx, &mut app, "choose a spare pin")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "R1 pin 1")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Connect")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            on_net(&app),
            1,
            "the chosen pin joins the connection: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "R1 pin 1")?,
            "the pin is listed on the connection"
        );
        tap(&ctx, &mut app, "\u{d7}")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            on_net(&app),
            0,
            "one click takes the pin back off: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_clears_the_wiring_on_one_connection_then_the_whole_board() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.tool_call(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let routed = app.project.board.tracks.len();
        assert!(
            routed > 0,
            "routing a pad to pad should draw copper: {}",
            app.message
        );
        let vcc = app
            .project
            .nets
            .iter()
            .find(|n| n.name == "VCC")
            .map(|n| n.id)
            .ok_or("the sample should have a VCC connection")?;
        app.highlight_net = Some(vcc);
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Edit")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Clear wiring on VCC")?,
            "the lit connection gets its own clear item"
        );
        tap(&ctx, &mut app, "Clear wiring on VCC")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.board.tracks.len() < routed,
            "the connection's copper lifts: {}",
            app.message
        );
        app.tool_call(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let routed = app.project.board.tracks.len();
        assert!(
            routed > 0,
            "the copper should be back for the whole-board clear"
        );
        tap(&ctx, &mut app, "Edit")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Clear all wiring")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.project.board.tracks.len(),
            0,
            "the board starts over clean: {}",
            app.message
        );
        assert_eq!(app.project.nets.len(), 3, "the connections themselves stay");
        Ok(())
    }

    #[test]
    fn a_newcomer_thickens_a_wire_with_one_click() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.tool_call(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let (id, middle) = app
            .project
            .board
            .tracks
            .first()
            .map(|t| {
                (
                    t.id,
                    Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0),
                )
            })
            .ok_or("routing should have left a wire on the board")?;
        app.change_tab(Tab::Pcb);
        app.canvas_click(middle, 15.0);
        assert_eq!(
            app.selected,
            Some(id),
            "clicking the middle of a wire picks it up: {}",
            app.message
        );
        app.track_width = 0.6;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Wire width")?,
            "the picked wire's width is editable in the inspector: {}",
            app.message
        );
        tap(&ctx, &mut app, "Set width")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert!(
            app.project
                .board
                .tracks
                .iter()
                .any(|t| t.id == id && (t.width - 0.6).abs() < 1e-9),
            "the wire is now half a millimetre across: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_hardens_the_junctions_for_the_etcher() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.run(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        app.refresh();
        let teardrops = |app: &App| -> usize {
            app.project
                .board
                .zones
                .iter()
                .filter(|zone| zone.kind.as_deref() == Some("teardrop"))
                .count()
        };
        assert_eq!(
            teardrops(&app),
            0,
            "a fresh route has no teardrops yet: {}",
            app.message
        );
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Rules")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Harden junctions")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert!(
            teardrops(&app) > 0,
            "one click grows teardrop copper at every joint: {}",
            app.message
        );
        tap(&ctx, &mut app, "Remove teardrops")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert_eq!(
            teardrops(&app),
            0,
            "removing teardrops leaves the plain wires exactly as they were: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_learns_where_a_wire_s_size_comes_from() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.tool_call(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let (track, net, width) = app
            .project
            .board
            .tracks
            .first()
            .map(|t| (t.id, t.net, t.width))
            .ok_or("the practice board should have copper to inspect")?;
        app.tool_call(
            "netclass.add",
            json!({"name": "Power", "clearance": 0.2, "width": width, "nets": [net]}),
        );
        app.select(Some(track));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let usual = format!("{width:.3} mm · the usual width for Power");
        assert!(
            paints_text(&ctx, &mut app, &usual)?,
            "the properties panel should say the width came from the net class ({usual}): {}",
            app.message
        );
        app.tool_call(
            "track.set_width",
            json!({"id": track, "width": width + 0.1}),
        );
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let own = format!(
            "{:.3} mm · chosen for this wire, not the usual {:.3} mm for Power",
            width + 0.1,
            width
        );
        assert!(
            paints_text(&ctx, &mut app, &own)?,
            "after widening one wire the panel should say it is no longer the class default ({own}): {}",
            app.message
        );
        let equal = "0.200 mm · the board's usual gap, Power asks no more".to_string();
        assert!(
            paints_text(&ctx, &mut app, &equal)?,
            "a class that asks no more than the board should say the gap is the board's ({equal}): {}",
            app.message
        );
        let class_id = app
            .project
            .net_classes
            .iter()
            .find(|c| c.name == "Power")
            .map(|c| c.id)
            .ok_or("the class should still exist")?;
        app.tool_call(
            "netclass.clearance",
            json!({"class": class_id, "clearance": 0.35}),
        );
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let usual_gap = "0.350 mm · the usual gap for Power".to_string();
        assert!(
            paints_text(&ctx, &mut app, &usual_gap)?,
            "a stricter class should own the gap note ({usual_gap}): {}",
            app.message
        );
        let (other_id, other_name) = app
            .project
            .nets
            .iter()
            .find(|n| n.id != net)
            .map(|n| (n.id, n.name.clone()))
            .ok_or("the practice board should have another net")?;
        app.tool_call(
            "board.set_pair_rule",
            json!({"net_a": net, "net_b": other_id, "clearance": 0.5}),
        );
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let special = format!("0.500 mm away from {other_name}");
        assert!(
            paints_text(&ctx, &mut app, &special)?,
            "a rule between two named nets should be listed beside the wire ({special}): {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_declares_a_pair_of_connections() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Rules")?;
        for _ in 0..18 {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(egui::pos2(150.0, 500.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, -4.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        assert!(
            scrolled_to_at(&ctx, &mut app, "DIFFERENTIAL PAIRS", 300.0)?,
            "the rules page should offer team lines: {}",
            app.message
        );
        app.dp_gap = 0.3;
        tap(&ctx, &mut app, "Declare pair")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert!(
            app.project
                .board
                .diff_pairs
                .iter()
                .any(|d| d.name == "USB" && (d.gap - 0.3).abs() < 1e-9 && d.net_p != d.net_n),
            "one click declares the pair with the typed gap: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "Pairs you have declared")?,
            "the pair is listed so it can be taken away: {}",
            app.message
        );
        tap(&ctx, &mut app, "Remove")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert!(
            app.project.board.diff_pairs.is_empty(),
            "removing stops the checks watching the pair: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_keeps_two_connections_apart() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        assert!(
            app.project.nets.len() >= 2,
            "the practice design has more than one connection"
        );
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Rules")?;
        for _ in 0..14 {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(egui::pos2(150.0, 500.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, -4.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        assert!(
            paints_text(&ctx, &mut app, "PAIR CLEARANCES")?,
            "the rules dock should offer pair spacing: {}",
            app.message
        );
        app.pair_clearance = 0.9;
        tap(&ctx, &mut app, "Set spacing")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert!(
            app.project
                .board
                .pair_rules
                .iter()
                .any(|r| r.clearance > 0.89 && r.a != r.b),
            "one click sets a spacing rule between two different connections: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "away from")?,
            "the rule is listed in plain words: {}",
            app.message
        );
        tap(&ctx, &mut app, "Clear")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert!(
            app.project.board.pair_rules.is_empty(),
            "clearing hands the pair back to the normal rules: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_thickens_a_whole_connection_at_once() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.tool_call(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let (id, net, middle) = app
            .project
            .board
            .tracks
            .first()
            .map(|t| {
                (
                    t.id,
                    t.net,
                    Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0),
                )
            })
            .ok_or("routing should leave a connection to thicken")?;
        app.change_tab(Tab::Pcb);
        app.canvas_click(middle, 15.0);
        assert_eq!(
            app.selected,
            Some(id),
            "one leg is picked up: {}",
            app.message
        );
        app.track_width = 0.6;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Thicken whole connection")?,
            "the inspector offers to widen the entire run, not just the picked leg: {}",
            app.message
        );
        tap(&ctx, &mut app, "Thicken whole connection")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        let legs: Vec<f64> = app
            .project
            .board
            .tracks
            .iter()
            .filter(|t| t.net == net)
            .map(|t| t.width)
            .collect();
        assert!(
            legs.len() > 1 && legs.iter().all(|w| (w - 0.6).abs() < 1e-9),
            "every stretch of the connection is now half a millimetre across: {:?}",
            legs
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_cuts_a_wire_in_half() -> Result<(), String> {
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
            .ok_or("routing should leave a wire to cut")?;
        app.select(Some(id));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Cut in half")?,
            "a chosen wire offers to be cut in half: {}",
            app.message
        );
        tap(&ctx, &mut app, "Cut in half")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.refresh();
        assert_eq!(
            app.project.board.tracks.len(),
            before + 1,
            "one wire becomes two: {}",
            app.message
        );
        let on_mid = |p: Point| (p.x - middle.x).abs() < 1e-6 && (p.y - middle.y).abs() < 1e-6;
        assert!(
            app.project
                .board
                .tracks
                .iter()
                .any(|t| on_mid(t.start) || on_mid(t.end)),
            "the two halves meet exactly at the middle of the original wire"
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_marks_an_unused_pin_so_no_one_asks_again() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.run(
            "component.add",
            json!({"kind":"resistor","reference":"R9","value":"1 k","position":{"x":20.0,"y":40.0}}),
        );
        app.refresh();
        app.tab = Tab::Schematic;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "No connect")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let spare = app
            .project
            .components
            .iter()
            .find(|c| c.reference == "R9")
            .and_then(|c| c.pin_position("1"))
            .ok_or("the spare resistor should have a pin 1")?;
        let before = app.project.no_connects.len();
        app.canvas_click(spare, 1.0);
        assert_eq!(
            app.project.no_connects.len(),
            before + 1,
            "clicking the pin marks it unused: {}",
            app.message
        );
        assert!(
            app.project.no_connects.iter().any(|f| {
                f.pin.component
                    == app
                        .project
                        .components
                        .iter()
                        .find(|c| c.reference == "R9")
                        .map(|c| c.id)
                        .unwrap_or_default()
                    && f.pin.pin == "1"
            }),
            "the mark lands on the pin that was clicked: {}",
            app.message
        );
        app.canvas_click(spare, 1.0);
        assert_eq!(
            app.project.no_connects.len(),
            before,
            "clicking the mark again clears it: {}",
            app.message
        );
        app.canvas_click(spare, 1.0);
        assert_eq!(
            app.project.no_connects.len(),
            before + 1,
            "the tool stays ready for the next unused pin: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_renumbers_a_messy_pile_of_parts_in_one_step() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.run(
            "component.add",
            json!({"kind":"resistor","reference":"R7","value":"1 k","position":{"x":50.0,"y":40.0}}),
        );
        app.run(
            "component.add",
            json!({"kind":"resistor","reference":"R9","value":"10 k","position":{"x":10.0,"y":40.0}}),
        );
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Preview")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "R7  →  R3")?,
            "the plan spells out a new number before anything changes: {}",
            app.message
        );
        let untouched = app.project.components.iter().any(|c| c.reference == "R9");
        assert!(untouched, "a preview never touches the design");
        tap(&ctx, &mut app, "Renumber")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let mut references: Vec<String> = app
            .project
            .components
            .iter()
            .map(|c| c.reference.clone())
            .collect();
        references.sort();
        assert_eq!(
            references,
            ["D1", "J1", "R1", "R2", "R3"],
            "the numbers follow placement order: {}",
            app.message
        );
        Ok(())
    }
