//! A small painted icon set for the kit and its gallery.
//!
//! Context: the shared icon set of ticket #612 is not on main yet. Kit buttons take
//! an `Icon`, a function that paints into a 16 px box, so the real set plugs in
//! without changing a component.
//! Local rules: icons are 16 px, 1.4 px stroke, drawn in the colour they are given.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/button.rs takes an `Icon`.
//!
//! # Do not
//! - Add a product or brand mark here; kit icons are generic shapes.
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2};

/// Icon size in points.
pub const ICON_SIZE: f32 = 16.0;
/// Icon stroke width in points.
pub const ICON_STROKE: f32 = 1.4;

/// A paint routine for one icon in a rectangle, in one colour.
#[derive(Clone, Copy)]
pub struct Icon(pub fn(&Painter, Rect, Color32));

impl std::fmt::Debug for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Icon")
    }
}

fn pt(rect: Rect, x: f32, y: f32) -> Pos2 {
    // 16 by 16 design grid mapped into the rect.
    pos2(
        rect.min.x + rect.width() * x / 16.0,
        rect.min.y + rect.height() * y / 16.0,
    )
}

fn line(p: &Painter, rect: Rect, c: Color32, pts: &[(f32, f32)]) {
    let points: Vec<Pos2> = pts.iter().map(|&(x, y)| pt(rect, x, y)).collect();
    p.add(Shape::line(points, Stroke::new(ICON_STROKE, c)));
}

/// A plus sign.
pub const PLUS: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(8.0, 3.0), (8.0, 13.0)]);
    line(p, r, c, &[(3.0, 8.0), (13.0, 8.0)]);
});

/// A minus sign.
pub const MINUS: Icon = Icon(|p, r, c| line(p, r, c, &[(3.0, 8.0), (13.0, 8.0)]));

/// Three dots in a row.
pub const MORE: Icon = Icon(|p, r, c| {
    for x in [3.5, 8.0, 12.5] {
        p.circle_filled(pt(r, x, 8.0), 1.2, c);
    }
});

/// A cross.
pub const CLOSE: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(4.0, 4.0), (12.0, 12.0)]);
    line(p, r, c, &[(12.0, 4.0), (4.0, 12.0)]);
});

/// A chevron pointing down.
pub const CHEVRON_DOWN: Icon =
    Icon(|p, r, c| line(p, r, c, &[(4.0, 6.0), (8.0, 10.0), (12.0, 6.0)]));

/// A chevron pointing up.
pub const CHEVRON_UP: Icon =
    Icon(|p, r, c| line(p, r, c, &[(4.0, 10.0), (8.0, 6.0), (12.0, 10.0)]));

/// A chevron pointing right.
pub const CHEVRON_RIGHT: Icon =
    Icon(|p, r, c| line(p, r, c, &[(6.0, 4.0), (10.0, 8.0), (6.0, 12.0)]));

/// A magnifier.
pub const SEARCH: Icon = Icon(|p, r, c| {
    p.circle_stroke(
        pt(r, 7.0, 7.0),
        4.0 * r.width() / 16.0,
        Stroke::new(ICON_STROKE, c),
    );
    line(p, r, c, &[(10.0, 10.0), (13.5, 13.5)]);
});

/// A magnifier with a plus, for zoom in.
pub const ZOOM_IN: Icon = Icon(|p, r, c| {
    p.circle_stroke(
        pt(r, 7.0, 7.0),
        4.5 * r.width() / 16.0,
        Stroke::new(ICON_STROKE, c),
    );
    line(p, r, c, &[(10.5, 10.5), (13.5, 13.5)]);
    line(p, r, c, &[(5.0, 7.0), (9.0, 7.0)]);
    line(p, r, c, &[(7.0, 5.0), (7.0, 9.0)]);
});

/// An eye, for visibility.
pub const EYE: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[
            (1.5, 8.0),
            (4.5, 4.5),
            (8.0, 3.5),
            (11.5, 4.5),
            (14.5, 8.0),
            (11.5, 11.5),
            (8.0, 12.5),
            (4.5, 11.5),
            (1.5, 8.0),
        ],
    );
    p.circle_filled(pt(r, 8.0, 8.0), 1.8, c);
});

/// Four corner brackets, for fit to view.
pub const FIT: Icon = Icon(|p, r, c| {
    line(p, r, c, &[(2.5, 6.0), (2.5, 2.5), (6.0, 2.5)]);
    line(p, r, c, &[(10.0, 2.5), (13.5, 2.5), (13.5, 6.0)]);
    line(p, r, c, &[(13.5, 10.0), (13.5, 13.5), (10.0, 13.5)]);
    line(p, r, c, &[(6.0, 13.5), (2.5, 13.5), (2.5, 10.0)]);
});

/// An eye crossed out, for hidden items.
pub const EYE_OFF: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[
            (1.5, 8.0),
            (4.5, 4.5),
            (8.0, 3.5),
            (11.5, 4.5),
            (14.5, 8.0),
            (11.5, 11.5),
            (8.0, 12.5),
            (4.5, 11.5),
            (1.5, 8.0),
        ],
    );
    line(p, r, c, &[(3.0, 13.0), (13.0, 3.0)]);
});

/// A closed padlock.
pub const LOCK: Icon = Icon(|p, r, c| {
    p.rect_stroke(
        Rect::from_min_max(pt(r, 3.5, 7.0), pt(r, 12.5, 14.0)),
        egui::CornerRadius::same(2),
        Stroke::new(ICON_STROKE, c),
        egui::StrokeKind::Inside,
    );
    line(
        p,
        r,
        c,
        &[(5.5, 7.0), (5.5, 5.0), (8.0, 2.5), (10.5, 5.0), (10.5, 7.0)],
    );
});

/// An open padlock.
pub const LOCK_OPEN: Icon = Icon(|p, r, c| {
    p.rect_stroke(
        Rect::from_min_max(pt(r, 3.5, 7.0), pt(r, 12.5, 14.0)),
        egui::CornerRadius::same(2),
        Stroke::new(ICON_STROKE, c),
        egui::StrokeKind::Inside,
    );
    line(p, r, c, &[(5.5, 7.0), (5.5, 5.0), (8.0, 2.5), (10.5, 5.0)]);
});

/// A tick, for checked boxes.
pub const CHECK: Icon = Icon(|p, r, c| line(p, r, c, &[(3.5, 8.5), (6.8, 11.5), (12.5, 4.8)]));

/// A triangle with a bar, for warnings.
pub const WARNING: Icon = Icon(|p, r, c| {
    line(
        p,
        r,
        c,
        &[(8.0, 2.5), (14.0, 13.0), (2.0, 13.0), (8.0, 2.5)],
    );
    line(p, r, c, &[(8.0, 6.5), (8.0, 9.5)]);
    p.circle_filled(pt(r, 8.0, 11.3), 0.8, c);
});

/// A circle with a bar, for errors.
pub const ERROR: Icon = Icon(|p, r, c| {
    p.circle_stroke(
        pt(r, 8.0, 8.0),
        6.0 * r.width() / 16.0,
        Stroke::new(ICON_STROKE, c),
    );
    line(p, r, c, &[(8.0, 4.8), (8.0, 8.6)]);
    p.circle_filled(pt(r, 8.0, 10.8), 0.8, c);
});

/// A circle with a tick, for success.
pub const OK: Icon = Icon(|p, r, c| {
    p.circle_stroke(
        pt(r, 8.0, 8.0),
        6.0 * r.width() / 16.0,
        Stroke::new(ICON_STROKE, c),
    );
    line(p, r, c, &[(5.0, 8.2), (7.2, 10.4), (11.0, 5.8)]);
});

/// The icon rectangle centred in `outer`.
pub fn icon_rect(outer: Rect) -> Rect {
    Rect::from_center_size(outer.center(), Vec2::splat(ICON_SIZE))
}
