//! Acceptance story: a_newcomer_sees_a_warning_on_a_sharp_corner_and_fixes_it_by_rounding
//!
//! A newcomer routes a track with a sharp corner, runs the board checks, sees a
//! warning about the acute corner, rounds the corner, and sees the warning go away.

use super::super::*;
use super::support::*;

fn board() -> Result<App, String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run("board.resize", json!({"width":40.0,"height":30.0}));
    app.run("net.create", json!({"name":"N0"}));
    let net = app.project.nets.last().ok_or("net id")?.id;
    app.run(
        "component.add",
        json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
    );
    let part = app.project.components.last().ok_or("part id")?.id;
    for pin in ["1", "2"] {
        app.run(
            "net.connect",
            json!({"net":net,"pins":[{"component":part,"pin":pin}]}),
        );
    }
    app.run("board.sync", json!({}));
    {
        let mut s = app.session.lock().map_err(|_| "session lock")?;
        for f in s.project.board.footprints.iter_mut() {
            f.position = pcbcraft_geom::Point::new(0.0, 0.0);
            for (k, pad) in f.pads.iter_mut().enumerate() {
                pad.offset = if k == 0 {
                    pcbcraft_geom::Point::new(5.0, 10.0)
                } else {
                    pcbcraft_geom::Point::new(30.0, 22.0)
                };
                pad.diameter = 1.0;
