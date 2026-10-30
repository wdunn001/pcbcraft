//! The glyphs menu rows lead with: copy, paste, rotate, trash, wire and the rest.
//!
//! Context: `./menu.rs` shows one 16 px glyph per row. The shared icon set of
//! ticket #612 is not on main, so these are drawn in code on the same 16 by 16
//! grid and 1.4 px stroke as `./icons.rs`, and plug into `menu::Item::icon`.
//! Local rules: generic shapes only; every glyph is one `Icon` constant that
//! paints in the colour it is given.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/icons.rs (grid helpers `pt` and `line`, `Icon`).
//!
//! # Do not
//! - Add a product or brand mark; swap this file for the shared set when #612 lands.
use super::icons::{ICON_STROKE, Icon, line, pt};
use egui::{Color32, Painter, Rect, Shape, Stroke, StrokeKind, epaint::PathShape};

fn stroke(c: Color32) -> Stroke {
    Stroke::new(ICON_STROKE, c)
}

fn outline(p: &Painter, r: Rect, c: Color32, a: (f32, f32), b: (f32, f32)) {
    p.rect_stroke(
        Rect::from_min_max(pt(r, a.0, a.1), pt(r, b.0, b.1)),
        egui::CornerRadius::same(1),
        stroke(c),
        StrokeKind::Inside,
    );
}

fn arc(p: &Painter, r: Rect, c: Color32, centre: (f32, f32), radius: f32, from: f32, to: f32) {
    let steps = 14;
    let points: Vec<_> = (0..=steps)
        .map(|k| {
            let a = (from + (to - from) * k as f32 / steps as f32).to_radians();
            pt(r, centre.0 + radius * a.cos(), centre.1 + radius * a.sin())
        })
        .collect();
    p.add(Shape::line(points, stroke(c)));
}

/// Two sheets, for copy.
pub const COPY: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[
            (5.5, 5.0),
            (5.5, 2.5),
            (13.5, 2.5),
            (13.5, 10.5),
            (11.0, 10.5),
        ],
    );
    outline(p, r, c, (2.5, 5.5), (10.5, 13.5));
});

/// A clipboard, for paste.
pub const PASTE: Icon = Icon(|p, r, c| {
    outline(p, r, c, (3.0, 3.5), (13.0, 14.0));
    p.rect_filled(
        Rect::from_min_max(pt(r, 5.8, 1.8), pt(r, 10.2, 5.0)),
        egui::CornerRadius::same(1),
        c,
    );
    line(p, r, c, &[(5.5, 8.5), (10.5, 8.5)]);
    line(p, r, c, &[(5.5, 11.0), (9.0, 11.0)]);
});

/// A sheet with a plus, for duplicate.
pub const DUPLICATE: Icon = Icon(|p, r, c| {
    outline(p, r, c, (2.5, 5.5), (10.0, 13.5));
    line(p, r, c, &[(12.0, 2.0), (12.0, 7.0)]);
    line(p, r, c, &[(9.5, 4.5), (14.5, 4.5)]);
});

/// A circular arrow, for rotate.
pub const ROTATE: Icon = Icon(|p, r, c| {
    arc(p, r, c, (8.0, 8.0), 5.2, -50.0, 250.0);
    line(p, r, c, &[(10.5, 2.6), (12.9, 3.6), (11.9, 6.0)]);
});

/// Two triangles across an axis, for flip.
pub const FLIP: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(8.0, 2.0), (8.0, 14.0)]);
    line(
        p,
        r,
        c,
        &[(2.5, 12.5), (6.2, 12.5), (6.2, 4.5), (2.5, 12.5)],
    );
    line(
        p,
        r,
        c,
        &[(13.5, 12.5), (9.8, 12.5), (9.8, 4.5), (13.5, 12.5)],
    );
});

/// A bin, for delete and remove.
pub const TRASH: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(2.5, 4.5), (13.5, 4.5)]);
    line(p, r, c, &[(6.0, 4.5), (6.0, 2.5), (10.0, 2.5), (10.0, 4.5)]);
    line(
        p,
        r,
        c,
        &[(4.0, 4.5), (4.8, 13.5), (11.2, 13.5), (12.0, 4.5)],
    );
    line(p, r, c, &[(6.6, 7.0), (6.6, 11.0)]);
    line(p, r, c, &[(9.4, 7.0), (9.4, 11.0)]);
});

/// Three sliders, for properties.
pub const PROPERTIES: Icon = Icon(|p, r, c| {
    for (y, x) in [(4.0, 10.5), (8.0, 5.5), (12.0, 9.0)] {
        line(p, r, c, &[(2.5, y), (13.5, y)]);
        p.circle_filled(pt(r, x, y), 1.9, c);
    }
});

/// A track between two pads, for wire and route.
pub const WIRE: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[(3.0, 12.0), (6.0, 12.0), (10.0, 4.0), (13.0, 4.0)],
    );
    p.circle_stroke(pt(r, 3.0, 12.0), 1.7, stroke(c));
    p.circle_stroke(pt(r, 13.0, 4.0), 1.7, stroke(c));
});

/// A track with a gap, for cut.
pub const SPLIT: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(1.5, 8.0), (5.5, 8.0)]);
    line(p, r, c, &[(10.5, 8.0), (14.5, 8.0)]);
    line(p, r, c, &[(5.5, 4.5), (5.5, 11.5)]);
    line(p, r, c, &[(10.5, 4.5), (10.5, 11.5)]);
});

/// Two rails with arrows between, for width.
pub const WIDTH: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(2.5, 3.0), (13.5, 3.0)]);
    line(p, r, c, &[(2.5, 13.0), (13.5, 13.0)]);
    line(p, r, c, &[(8.0, 5.0), (8.0, 11.0)]);
    line(p, r, c, &[(6.0, 7.0), (8.0, 5.0), (10.0, 7.0)]);
    line(p, r, c, &[(6.0, 9.0), (8.0, 11.0), (10.0, 9.0)]);
});

/// Stacked sheets, for layers.
pub const LAYERS: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[(8.0, 2.5), (14.0, 5.5), (8.0, 8.5), (2.0, 5.5), (8.0, 2.5)],
    );
    line(p, r, c, &[(2.0, 8.5), (8.0, 11.5), (14.0, 8.5)]);
    line(p, r, c, &[(2.0, 11.0), (8.0, 14.0), (14.0, 11.0)]);
});

/// A pointer arrow, for select.
pub const SELECT: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[
            (4.0, 2.5),
            (4.0, 12.5),
            (6.7, 10.0),
            (8.7, 13.8),
            (10.5, 13.0),
            (8.6, 9.3),
            (12.0, 9.2),
            (4.0, 2.5),
        ],
    );
});

/// A target, for focus.
pub const FOCUS: Icon = Icon(|p, r, c| {
    p.circle_stroke(pt(r, 8.0, 8.0), 4.2, stroke(c));
    p.circle_filled(pt(r, 8.0, 8.0), 1.2, c);
    for (a, b) in [
        ((8.0, 1.5), (8.0, 3.2)),
        ((8.0, 12.8), (8.0, 14.5)),
        ((1.5, 8.0), (3.2, 8.0)),
        ((12.8, 8.0), (14.5, 8.0)),
    ] {
        line(p, r, c, &[a, b]);
    }
});

fn star_points() -> Vec<(f32, f32)> {
    (0..10)
        .map(|k| {
            let radius = if k % 2 == 0 { 6.4 } else { 2.7 };
            let a = (-90.0 + 36.0 * k as f32).to_radians();
            (8.0 + radius * a.cos(), 8.4 + radius * a.sin())
        })
        .collect()
}

/// A star outline, for favourite.
pub const STAR: Icon = Icon(|p, r, c| {
    let mut points = star_points();
    points.push(points[0]);
    line(p, r, c, &points);
});

/// A filled star, for a part that is a favourite.
pub const STAR_FILLED: Icon = Icon(|p, r, c| {
    let points = star_points();
    let inner: Vec<_> = points
        .iter()
        .skip(1)
        .step_by(2)
        .map(|&(x, y)| pt(r, x, y))
        .collect();
    p.add(Shape::convex_polygon(inner, c, Stroke::NONE));
    for k in (0..10).step_by(2) {
        let tip = points[k];
        let left = points[(k + 9) % 10];
        let right = points[(k + 1) % 10];
        p.add(Shape::Path(PathShape::convex_polygon(
            vec![
                pt(r, left.0, left.1),
                pt(r, tip.0, tip.1),
                pt(r, right.0, right.1),
            ],
            c,
            stroke(c),
        )));
    }
});

/// A square with a plus, for place.
pub const PLACE: Icon = Icon(|p, r, c| {
    outline(p, r, c, (2.5, 2.5), (13.5, 13.5));
    line(p, r, c, &[(8.0, 5.0), (8.0, 11.0)]);
    line(p, r, c, &[(5.0, 8.0), (11.0, 8.0)]);
});

/// An arrow into a tray, for add to project.
pub const IMPORT: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(8.0, 2.0), (8.0, 9.5)]);
    line(p, r, c, &[(4.8, 6.5), (8.0, 9.8), (11.2, 6.5)]);
    line(
        p,
        r,
        c,
        &[(2.5, 10.5), (2.5, 13.5), (13.5, 13.5), (13.5, 10.5)],
    );
});

/// A page with a folded corner, for datasheet.
pub const DATASHEET: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[
            (4.0, 2.0),
            (9.5, 2.0),
            (12.5, 5.0),
            (12.5, 14.0),
            (4.0, 14.0),
            (4.0, 2.0),
        ],
    );
    line(p, r, c, &[(9.5, 2.0), (9.5, 5.0), (12.5, 5.0)]);
    line(p, r, c, &[(6.2, 8.2), (10.3, 8.2)]);
    line(p, r, c, &[(6.2, 10.8), (10.3, 10.8)]);
});

/// A circled letter i, for source and information.
pub const INFO: Icon = Icon(|p, r, c| {
    p.circle_stroke(pt(r, 8.0, 8.0), 6.0, stroke(c));
    p.circle_filled(pt(r, 8.0, 5.2), 0.9, c);
    line(p, r, c, &[(8.0, 7.4), (8.0, 11.2)]);
});

/// A pencil, for rename.
pub const RENAME: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[
            (2.8, 13.2),
            (3.6, 10.0),
            (10.8, 2.8),
            (13.2, 5.2),
            (6.0, 12.4),
            (2.8, 13.2),
        ],
    );
    line(p, r, c, &[(9.2, 4.4), (11.6, 6.8)]);
});

/// An arrow up, for move up.
pub const MOVE_UP: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(8.0, 13.0), (8.0, 3.5)]);
    line(p, r, c, &[(4.5, 7.0), (8.0, 3.5), (11.5, 7.0)]);
});

/// An arrow down, for move down.
pub const MOVE_DOWN: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(8.0, 3.0), (8.0, 12.5)]);
    line(p, r, c, &[(4.5, 9.0), (8.0, 12.5), (11.5, 9.0)]);
});

/// A sun, for highlight.
pub const HIGHLIGHT: Icon = Icon(|p, r, c| {
    p.circle_stroke(pt(r, 8.0, 8.0), 2.6, stroke(c));
    for k in 0..8 {
        let a = (45.0 * k as f32).to_radians();
        let (s, e) = (4.8, 6.6);
        line(
            p,
            r,
            c,
            &[
                (8.0 + s * a.cos(), 8.0 + s * a.sin()),
                (8.0 + e * a.cos(), 8.0 + e * a.sin()),
            ],
        );
    }
});

/// A circle with a bar across, for mute.
pub const MUTE: Icon = Icon(|p, r, c| {
    p.circle_stroke(pt(r, 8.0, 8.0), 5.8, stroke(c));
    line(p, r, c, &[(3.9, 12.1), (12.1, 3.9)]);
});
