//! A small drawing of a board shape: the cut path, its cut-outs and its mounting holes.
//!
//! Context: the Files page (Board shape card, Shape of each layer row) and the Settings Board
//! tab show the outline as a picture. The board is a free-form 2D shape; a rectangle is only
//! one template of it. The numbers a person reads are corner counts and the extent of the
//! shape; there is no width or height to type.
//! Local rules: the painter takes plain points in millimetres and a screen rectangle. It
//! scales the whole shape to fit, keeps the aspect ratio, and draws y downward like the
//! canvas. Arcs are flattened to short chords for the drawing; corner counts count only the
//! real vertices.
//!
//! # Dependencies
//! - crates/doc/src/outline.rs (`flatten_outline`, `outline_extent`)
//! - crates/ui-egui/src/theme.rs (colour tokens)
//!
//! # Do not
//! - Read the document here; callers pass the points.
//! - Add a width or height edit to a caller that uses this picture.
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use pcbcraft_doc::{Board, flatten_outline, outline_extent};
use pcbcraft_geom::Point;

/// Largest distance a drawn chord may stray from a true arc, in millimetres.
const ARC_ERROR_MM: f64 = 0.1;
/// Space kept around the shape inside its picture, in points.
const MARGIN: f32 = 6.0;

/// What a shape looks like, in millimetres.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ShapeView {
    /// The closed cut path, each arc flattened into chords.
    pub(crate) path: Vec<Point>,
    /// Real vertices of the cut path; arcs add none.
    pub(crate) corners: usize,
    /// Round cut-outs inside the shape: centre and radius.
    pub(crate) cutouts: Vec<(Point, f64)>,
    /// Round mounting holes: centre and radius.
    pub(crate) holes: Vec<(Point, f64)>,
}

/// Colours and strokes of one picture.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShapeLook {
    /// Colour of the cut path.
    pub(crate) line: Color32,
    /// Colour of cut-outs and holes.
    pub(crate) detail: Color32,
}

/// The points that cut the board: its outline, or a rectangle when the outline is absent.
///
/// Why: a plain rectangle is a template. Both forms read as one list of corners.
///
/// # Rules
/// - A repeated closing point is dropped.
///
/// # Do not
/// - Return a width or height; use [`extent_size`] for the read only reach.
pub(crate) fn corners_of(board: &Board) -> Vec<Point> {
    match board.outline.as_ref() {
        Some(points) if points.len() >= 3 => {
            let mut out = points.clone();
            if out.len() > 3 && out.first() == out.last() {
                out.pop();
            }
            out
        }
        _ => vec![
            Point::new(0.0, 0.0),
            Point::new(board.width, 0.0),
            Point::new(board.width, board.height),
            Point::new(0.0, board.height),
        ],
    }
}

/// The picture data of a board: cut path, cut-outs and mounting holes.
///
/// # Rules
/// - Arcs are kept only while the corners are the stored outline; a rectangle has none.
pub(crate) fn view_of(board: &Board) -> ShapeView {
    let corners = corners_of(board);
    let count = corners.len();
    let has_outline = board.outline.as_ref().is_some_and(|p| p.len() >= 3);
    let path = if has_outline && count == board.outline.as_ref().map_or(0, Vec::len) {
        flatten_outline(&corners, &board.outline_arcs, ARC_ERROR_MM)
    } else {
        corners
    };
    ShapeView {
        path,
        corners: count,
        cutouts: board.cutouts.iter().map(|c| (c.center, c.radius)).collect(),
        holes: board
            .holes
            .iter()
            .map(|h| (h.position, h.diameter / 2.0))
            .collect(),
    }
}

/// The box around the shape: lower corner and upper corner, in millimetres.
///
/// Why: size is only a read only fact about the shape. The model owns the real answer
/// (`Board::extent`); until it lands this reads the outline or the template rectangle.
pub(crate) fn extent(board: &Board) -> (Point, Point) {
    let corners = corners_of(board);
    let arcs: &[Option<Point>] = if board.outline.is_some() {
        &board.outline_arcs
    } else {
        &[]
    };
    outline_extent(&corners, arcs).unwrap_or_default()
}

/// How far the shape reaches across and down, in millimetres.
pub(crate) fn extent_size(board: &Board) -> (f64, f64) {
    let (lo, hi) = extent(board);
    (hi.x - lo.x, hi.y - lo.y)
}

/// True when the shape is exactly four corners at right angles to the axes.
///
/// Why: the page may call it "Rectangle" only as a name for what it is.
pub(crate) fn is_plain_rectangle(view: &ShapeView) -> bool {
    if view.corners != 4 || view.path.len() != 4 {
        return false;
    }
    let p = &view.path;
    let flat = |a: Point, b: Point| (a.x - b.x).abs() < 1e-9 || (a.y - b.y).abs() < 1e-9;
    let same_x = |a: Point, b: Point| (a.x - b.x).abs() < 1e-9;
    (0..4).all(|i| flat(p[i], p[(i + 1) % 4])) && same_x(p[0], p[1]) != same_x(p[1], p[2])
}

/// Draw `view` scaled to fit inside `rect`.
///
/// Why: one picture for every place that shows a shape, so they never disagree.
///
/// # Rules
/// - The aspect ratio is kept and the shape is centred; y grows downward.
/// - Draws nothing for an empty path.
/// - The caller allocates `rect` and owns the tooltip and accessible name.
///
/// # Dependencies
/// - crates/ui-egui/src/theme.rs
///
/// # Do not
/// - Call it with a rectangle smaller than the margin on a side.
pub(crate) fn paint_shape(painter: &Painter, rect: Rect, view: &ShapeView, look: &ShapeLook) {
    let Some(first) = view.path.first() else {
        return;
    };
    let (mut lo, mut hi) = (*first, *first);
    for p in &view.path {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let (w, h) = ((hi.x - lo.x).max(1e-6), (hi.y - lo.y).max(1e-6));
    let room = rect.shrink(MARGIN);
    let scale = (f64::from(room.width()) / w).min(f64::from(room.height()) / h) as f32;
    let used = Vec2::new(w as f32 * scale, h as f32 * scale);
    let origin = room.center() - used / 2.0;
    let to_screen = |p: Point| -> Pos2 {
        origin + Vec2::new((p.x - lo.x) as f32 * scale, (p.y - lo.y) as f32 * scale)
    };
    let points: Vec<Pos2> = view.path.iter().map(|p| to_screen(*p)).collect();
    let mut closed = points;
    if let Some(start) = closed.first().copied() {
        closed.push(start);
    }
    painter.add(Shape::line(closed, Stroke::new(1.5, look.line)));
    for (centre, radius) in view.cutouts.iter().chain(view.holes.iter()) {
        let r = ((*radius as f32) * scale).max(1.5);
        painter.circle_stroke(to_screen(*centre), r, Stroke::new(1.0, look.detail));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_with(points: Option<Vec<Point>>) -> Board {
        let mut board = pcbcraft_doc::Project::default().board;
        board.width = 72.0;
        board.height = 60.0;
        board.outline = points;
        board.outline_arcs.clear();
        board
    }

    #[test]
    fn a_missing_outline_reads_as_a_four_corner_rectangle() -> Result<(), String> {
        let view = view_of(&board_with(None));
        assert_eq!(view.corners, 4);
        assert!(is_plain_rectangle(&view));
        let (w, h) = extent_size(&board_with(None));
        assert!((w - 72.0).abs() < 1e-9 && (h - 60.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn an_l_shape_has_six_corners_and_is_not_a_rectangle() -> Result<(), String> {
        let l = vec![
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(40.0, 20.0),
            Point::new(20.0, 20.0),
            Point::new(20.0, 50.0),
            Point::new(0.0, 50.0),
        ];
        let board = board_with(Some(l));
        let view = view_of(&board);
        assert_eq!(view.corners, 6);
        assert!(!is_plain_rectangle(&view));
        let (w, h) = extent_size(&board);
        assert!((w - 40.0).abs() < 1e-9 && (h - 50.0).abs() < 1e-9);
        Ok(())
    }
}
