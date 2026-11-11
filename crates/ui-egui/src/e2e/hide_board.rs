//! Stories for the eyes of the Board edge and Drills and vias rows on the Layout tab.
//!
//! Context: with every layer eye off the canvas shows the empty grid. The board fill, its
//! outline, the datums and the dimension lines follow Board edge; mounting holes follow
//! Drills and vias.
use super::super::*;
use super::support::*;

/// What one frame painted of the board itself.
#[derive(Debug, Default, PartialEq)]
struct Seen {
    fill: bool,
    outline: bool,
    holes: usize,
    dimensions: usize,
}

impl Seen {
    fn nothing(&self) -> bool {
        *self == Seen::default()
    }
}

fn near(a: egui::Pos2, b: egui::Pos2) -> bool {
    (a - b).length() < 1.5
}

fn walk(app: &App, shape: &egui::Shape, hole_centres: &[egui::Pos2], seen: &mut Seen) {
    let art = app.art();
    match shape {
        egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(app, s, hole_centres, seen)),
        egui::Shape::Path(path) => {
            if path.fill == art.substrate && path.points.len() >= 3 {
                seen.fill = true;
            }
            if path.closed
                && path.stroke.width == 1.5
                && path.stroke.color == egui::epaint::ColorMode::Solid(art.edge)
            {
                seen.outline = true;
            }
        }
        egui::Shape::Mesh(mesh) => {
            if mesh.vertices.iter().any(|v| v.color == art.substrate) {
                seen.fill = true;
            }
        }
        egui::Shape::Circle(circle) => {
            if circle.fill == art.bg && hole_centres.iter().any(|c| near(*c, circle.center)) {
                seen.holes += 1;
            }
        }
        egui::Shape::Text(text) => {
            let words = text.galley.text();
            if words == "72.00 mm" || words == "60.00 mm" {
                seen.dimensions += 1;
            }
        }
        _ => {}
    }
}

fn seen(ctx: &egui::Context, app: &mut App) -> Seen {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let canvas = app.canvas_rect;
    let centres: Vec<egui::Pos2> = app
        .project
        .board
        .holes
        .iter()
        .map(|h| app.pcb.screen(h.position, canvas))
        .collect();
    let mut seen = Seen::default();
    for clipped in &output.shapes {
        walk(app, &clipped.shape, &centres, &mut seen);
    }
    seen
}

/// The showcase board on the Layout tab with the Layers tree open.
fn sample() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.home_open = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Showcase board")?;
    app.change_tab(Tab::Pcb);
    app.region().left_tab = "layers".to_owned();
    app.needs_fit = true;
    app.refresh();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app))
}

/// Press the eye of every drawn layer row that is on.
fn hide_every_layer(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    // A short tree keeps every layer row on screen.
    app.tree.expanded.retain(|k| !k.starts_with("side:"));
    app.tree.expanded.insert("user".into());
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let mut eyes: Vec<String> = ["Layer # \u{b7} Top", "Layer # \u{b7} Bottom"]
        .iter()
        .map(|n| format!("button:Hide {n}"))
        .collect();
    let coppers = usize::from(app.project.board.copper_count());
    for i in 1..coppers.saturating_sub(1) {
        let name = app
            .project
            .board
            .copper_key(u8::try_from(i).map_err(|e| e.to_string())?);
        let name: String = name
            .chars()
            .map(|c| if c.is_ascii_digit() { '#' } else { c })
            .collect();
        eyes.push(format!("button:Hide Layer # \u{b7} {name}"));
    }
    eyes.push("button:Hide Drills and vias".into());
    eyes.push("button:Hide Board edge".into());
    for art in app.project.board.user_layers.clone() {
        eyes.push(format!("button:Hide {}", art.name));
    }
    for id in eyes {
        tap_id(ctx, app, &id).map_err(|e| {
            let names: Vec<String> = interactive_rects(ctx, app)
                .into_iter()
                .map(|(n, _)| n)
                .filter(|n| n.contains("Layer"))
                .collect();
            format!("{e}: {names:?}")
        })?;
    }
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

fn show_only(ctx: &egui::Context, app: &mut App, row: &str) -> Result<(), String> {
    hide_every_layer(ctx, app)?;
    tap_id(ctx, app, &format!("button:Show {row}"))?;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_sees_the_whole_board_before_hiding_anything() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    let all = seen(&ctx, &mut app);
    assert!(all.fill && all.outline, "{all:?}");
    assert_eq!(all.holes, app.project.board.holes.len(), "{all:?}");
    assert!(app.project.board.holes.len() >= 4, "the sample has holes");
    assert_eq!(all.dimensions, 2, "{all:?}");
    Ok(())
}

#[test]
fn a_newcomer_hides_every_layer_and_no_board_hole_or_dimension_stays() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    hide_every_layer(&ctx, &mut app)?;
    let left = seen(&ctx, &mut app);
    assert!(left.nothing(), "nothing of the design is painted: {left:?}");
    for i in 0..app.project.copper_layer_count() {
        let layer = pcbcraft_doc::Layer::from_index(i)?;
        assert_eq!(
            app.layer_counts(layer).1,
            crate::board_visibility::LayerCount::default(),
            "copper layer {i} paints nothing"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_shows_board_edge_alone_and_gets_fill_outline_and_dimensions() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    show_only(&ctx, &mut app, "Board edge")?;
    let now = seen(&ctx, &mut app);
    assert!(now.fill && now.outline, "{now:?}");
    assert_eq!(now.dimensions, 2, "{now:?}");
    assert_eq!(now.holes, 0, "holes stay with Drills and vias: {now:?}");
    Ok(())
}

#[test]
fn a_newcomer_shows_drills_alone_and_gets_the_holes_only() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    show_only(&ctx, &mut app, "Drills and vias")?;
    let now = seen(&ctx, &mut app);
    assert_eq!(now.holes, app.project.board.holes.len(), "{now:?}");
    assert!(
        !now.fill && !now.outline && now.dimensions == 0,
        "the board stays with Board edge: {now:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_presses_show_all_and_everything_comes_back() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    let before = seen(&ctx, &mut app);
    hide_every_layer(&ctx, &mut app)?;
    assert!(seen(&ctx, &mut app).nothing());
    tap_id(&ctx, &mut app, "button:Show all")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(seen(&ctx, &mut app), before);
    assert!(
        app.project.board.user_layers.iter().all(|l| l.visible),
        "drawn layers are back"
    );
    Ok(())
}

#[test]
fn a_newcomer_presses_hide_others_and_only_the_picked_row_stays() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    tap_id(&ctx, &mut app, "button:Hide others")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let now = seen(&ctx, &mut app);
    assert!(now.nothing(), "Top stays, the board and holes go: {now:?}");
    for i in 1..app.project.copper_layer_count() {
        let layer = pcbcraft_doc::Layer::from_index(i)?;
        assert_eq!(
            app.layer_counts(layer).1,
            crate::board_visibility::LayerCount::default(),
            "copper layer {i} goes with the others"
        );
    }
    assert!(
        app.project.board.user_layers.iter().all(|l| !l.visible),
        "drawn layers go with the others"
    );
    Ok(())
}

/// A track on Top whose middle is a clean hit on the track itself (no pad, via or part over it).
fn clean_top_track(app: &App) -> Result<(Id, Point), String> {
    app.project
        .board
        .tracks
        .iter()
        .filter(|t| t.layer == pcbcraft_doc::Layer::Front)
        .map(|t| {
            (
                t.id,
                Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0),
            )
        })
        .find(|(id, at)| app.hit_at(*at, 10.0) == canvas::gestures::Hit::TrackSegment(*id))
        .ok_or_else(|| "a clean Top track in the showcase".to_owned())
}

#[test]
fn a_newcomer_cannot_click_or_hover_a_track_on_a_hidden_layer() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    let (track, at) = clean_top_track(&app)?;
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_ne!(
        app.hit_at(at, 10.0),
        canvas::gestures::Hit::TrackSegment(track),
        "the pointer finds nothing of a hidden track"
    );
    assert_ne!(app.pick_object(at, 10.0), Some(track));
    app.select_at(at, 10.0);
    assert_ne!(app.selected, Some(track), "a click selects nothing");
    tap_id(&ctx, &mut app, "button:Show Layer # \u{b7} Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.hit_at(at, 10.0),
        canvas::gestures::Hit::TrackSegment(track),
        "showing the layer makes the track pickable again"
    );
    Ok(())
}

#[test]
fn a_newcomer_box_selects_no_hidden_part_hole_or_cutout() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    let hole = app.project.board.holes.first().ok_or("a hole")?;
    let (hole_id, c) = (hole.id, hole.position);
    let cutout = app.project.board.cutouts.first().ok_or("a cutout")?;
    let (cutout_id, d) = (cutout.id, cutout.center);
    let part = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.board_side() == pcbcraft_doc::Layer::Front)
        .ok_or("a Top part")?;
    let (part_id, e) = (part.id, part.position);
    let around = |p: Point| {
        (
            p.offset(Point::new(-0.5, -0.5)),
            p.offset(Point::new(0.5, 0.5)),
        )
    };
    let boxed = |app: &App, p: Point, id: Id| {
        let (lo, hi) = around(p);
        app.ids_in_rect(lo, hi).contains(&id)
    };
    for (p, id) in [(c, hole_id), (d, cutout_id), (e, part_id)] {
        assert!(boxed(&app, p, id), "{id} is boxed while shown");
    }
    hide_every_layer(&ctx, &mut app)?;
    for (p, id) in [(c, hole_id), (d, cutout_id), (e, part_id)] {
        assert!(!boxed(&app, p, id), "{id} is not boxed while hidden");
    }
    tap_id(&ctx, &mut app, "button:Show all")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for (p, id) in [(c, hole_id), (d, cutout_id), (e, part_id)] {
        assert!(boxed(&app, p, id), "{id} is boxed again");
    }
    Ok(())
}

#[test]
fn a_newcomer_routes_from_a_pad_only_while_its_layer_is_shown() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    let part = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.board_side() == pcbcraft_doc::Layer::Front && !f.pads.is_empty())
        .ok_or("a Top part")?;
    let number = part.pads.first().ok_or("a pad")?.number.clone();
    let at = part.pad_position(&number).ok_or("pad position")?;
    let part_id = part.id;
    assert!(app.pick_pad(at, 10.0).is_some(), "the pad is a target");
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.pick_pad(at, 10.0).is_none_or(|(id, _)| id != part_id),
        "a hidden pad is not a route or snap target"
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_wires_on_the_schematic_and_cannot_pick_them() -> Result<(), String> {
    let (ctx, mut app) = sample()?;
    app.change_tab(Tab::Schematic);
    app.main_open.all = true;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let sheet = app.sheet_view();
    let page = app.schematic_page();
    let wire = sheet
        .drawn_wires
        .iter()
        .filter(|w| crate::page_scope::in_view(&sheet, page, sheet.schematic_drawn_wire_page(w)))
        .find_map(|w| {
            let a = *w.points.first()?;
            let b = *w.points.get(1)?;
            let at = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            matches!(app.hit_at(at, 10.0), canvas::gestures::Hit::WireSegment(id, _) if id == w.id)
                .then_some((w.id, at))
        })
        .ok_or("a wire to click")?;
    drop(sheet);
    app.preferences.hidden_layers.push("Wires".to_owned());
    assert!(
        !matches!(app.hit_at(wire.1, 10.0), canvas::gestures::Hit::WireSegment(id, _) if id == wire.0),
        "a hidden wire is not under the pointer"
    );
    app.preferences.hidden_layers.retain(|h| h != "Wires");
    assert!(
        matches!(app.hit_at(wire.1, 10.0), canvas::gestures::Hit::WireSegment(id, _) if id == wire.0),
        "showing Wires makes it pickable again"
    );
    app.preferences.hidden_layers.push("Symbols".to_owned());
    let part = app.project.components.first().ok_or("a symbol")?;
    let spot = part.position;
    assert_ne!(
        app.pick_object(spot, 10.0),
        Some(part.id),
        "hidden symbols are not picked"
    );
    Ok(())
}
