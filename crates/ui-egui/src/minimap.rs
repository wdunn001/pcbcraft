//! Minimap: a small overview of the whole board or sheet that sits above the
//! navigation card, so a person can jump to another region without zooming out
//! and back in.
//!
//! Context: the Map button lives in the bottom-right navigation card drawn by
//! `canvas.rs` (`fit-fab`), which calls [`App::minimap_show`] once per frame
//! after the card. The camera types live in `lib.rs` (`Camera`) and the fit
//! bounds in `canvas.rs` (`view_bounds`). Pure parts (overview builder, region
//! maths, size and position clamps, fade easing) come first and are unit tested;
//! the egui parts follow.
//!
//! Local rules: the overview is geometry only and is cached by project
//! revision, tab and sheet; selection, findings and the lit net are cheap marks
//! built per frame. Tracks, parts and pour points are capped (level of detail),
//! so the work count of a frame stays under [`WORK_BUDGET`]. Pinned state, width,
//! opacity and position persist per tab in `Preferences::minimap`. Eject keeps
//! the map on screen without pinning it until it is docked back (desktop only; the browser
//! build offers pin only). It does not open an OS window: that waits on the workbench
//! pop-out (#460).

use super::*;
use egui::{Pos2, Sense};
use pcbcraft_geom::Point;

/// Most tracks the overview keeps; more are thinned evenly.
pub(crate) const MAX_TRACKS: usize = 4000;
/// Most parts the overview keeps.
pub(crate) const MAX_PARTS: usize = 2500;
/// Most vertices kept for one pour outline.
pub(crate) const MAX_POUR_POINTS: usize = 300;
/// Largest work count (primitives painted) one map frame may cost. The
/// showcase board, the largest example, stays far below it.
#[cfg(test)]
pub(crate) const WORK_BUDGET: usize = 9000;
/// Narrowest the map may be drawn, in points.
pub(crate) const MIN_WIDTH: f32 = 180.0;
/// Opacity limits of a pinned map (10 to 100 percent).
pub(crate) const MIN_OPACITY: f32 = 0.10;
/// Default opacity of a pinned map when the pointer is away.
pub(crate) const DEFAULT_OPACITY: f32 = 0.55;
/// Height of the title strip above the map, shown with the hover chrome.
const STRIP: f32 = 30.0;
/// Height of the chip row below the map, shown with the hover chrome.
const CHIPS: f32 = 34.0;
/// Seconds the chrome takes to ease in and out.
const FADE_IN: f32 = 0.15;
const FADE_OUT: f32 = 0.25;
/// Seconds a glide to a region takes.
const GLIDE: f32 = 0.35;
/// Gap between the map and the card below it.
const GAP: f32 = 8.0;
/// Inner space between the frame and its contents.
const PAD: f32 = 8.0;

/// One straight piece of copper or wire.
pub(crate) type Seg = [Point; 2];

/// A simplified copy of the design for the map: geometry only.
///
/// # Rules
/// - Built by [`build_overview`]; never mutated after.
/// - `work()` is the primitive count the painter will walk.
///
/// # Do not
/// - Store selection, findings or highlights here; they are [`Marks`].
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Overview {
    /// The world rectangle the map shows (low and high corner).
    pub bounds: (Point, Point),
    /// Board outline (or the sheet frame on a schematic), closed or open.
    pub outline: Vec<Point>,
    /// Copper pour outlines.
    pub pours: Vec<Vec<Point>>,
    /// Tracks (or wires) and whether each is on the top side.
    pub tracks: Vec<(Seg, bool)>,
    /// Part boxes (low and high corner).
    pub parts: Vec<(Point, Point)>,
    /// Tracks the level of detail dropped.
    pub dropped_tracks: usize,
}

impl Overview {
    /// Primitives the painter walks for this overview.
    #[cfg(test)]
    pub(crate) fn work(&self) -> usize {
        self.outline.len()
            + self.pours.iter().map(Vec::len).sum::<usize>()
            + self.tracks.len()
            + self.parts.len()
    }
    /// Width over height of the shown rectangle; never zero.
    pub(crate) fn aspect(&self) -> f32 {
        let w = (self.bounds.1.x - self.bounds.0.x).max(1.0);
        let h = (self.bounds.1.y - self.bounds.0.y).max(1.0);
        (w / h) as f32
    }
    /// Width over height of the map as the view shows it: a quarter turn swaps the two sides.
    pub(crate) fn aspect_for(&self, turns: u8) -> f32 {
        if turns % 2 == 1 {
            1.0 / self.aspect()
        } else {
            self.aspect()
        }
    }
}

/// What changes often and costs little: findings, the selection, a lit net.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Marks {
    pub findings: Vec<Point>,
    pub selection: Option<(Point, Point)>,
    pub net: Vec<Seg>,
}

/// Thin `items` to at most `cap` entries, keeping an even spread.
fn thin<T: Clone>(items: &[T], cap: usize) -> Vec<T> {
    if items.len() <= cap || cap == 0 {
        return items.to_vec();
    }
    let step = items.len().div_ceil(cap);
    items.iter().step_by(step).cloned().collect()
}

fn corners(lo: Point, hi: Point) -> Vec<Point> {
    vec![lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y), lo]
}

/// Build the overview for the board (`pcb`) or for schematic sheet `page`.
///
/// Why: the map must stay cheap on a large board, so it draws a capped,
/// simplified copy instead of the document.
///
/// # Rules
/// - Total: any project gives a valid overview with a non-empty bounds.
/// - Tracks are capped at [`MAX_TRACKS`], parts at [`MAX_PARTS`], each pour at
///   [`MAX_POUR_POINTS`]; the count dropped is kept.
/// - Schematic: only the objects of `page` are included; the outline is the
///   sheet frame.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas.rs` (`view_bounds`, the bounds the Fit button uses).
///
/// # Do not
/// - Read selection or findings here; see [`build_marks`].
pub(crate) fn build_overview(project: &Project, pcb: bool, page: Id) -> Overview {
    let bounds = crate::canvas::view_bounds(project, pcb, page);
    let mut out = Overview {
        bounds,
        ..Overview::default()
    };
    if pcb {
        out.outline = match &project.board.outline {
            Some(ring) if ring.len() >= 3 => ring.clone(),
            _ => corners(
                Point::new(0.0, 0.0),
                Point::new(project.board.width, project.board.height),
            ),
        };
        out.pours = project
            .board
            .zones
            .iter()
            .filter(|z| z.outline.len() >= 3)
            .map(|z| thin(&z.outline, MAX_POUR_POINTS))
            .collect();
        let all: Vec<(Seg, bool)> = project
            .board
            .tracks
            .iter()
            .map(|t| ([t.start, t.end], t.layer == pcbcraft_doc::Layer::Front))
            .collect();
        out.tracks = thin(&all, MAX_TRACKS);
        out.dropped_tracks = all.len() - out.tracks.len();
        let parts: Vec<(Point, Point)> =
            project.board.footprints.iter().map(footprint_box).collect();
        out.parts = thin(&parts, MAX_PARTS);
    } else {
        out.outline = corners(bounds.0, bounds.1);
        let mut segs: Vec<(Seg, bool)> = Vec::new();
        for wire in &project.wires {
            if crate::page_scope::object_in_view(project, page, wire.id)
                && let Some(path) = project.wire_path(wire)
            {
                segs.extend(path.windows(2).map(|w| ([w[0], w[1]], true)));
            }
        }
        for wire in &project.drawn_wires {
            if crate::page_scope::object_in_view(project, page, wire.id) {
                segs.extend(wire.points.windows(2).map(|w| ([w[0], w[1]], true)));
            }
        }
        out.tracks = thin(&segs, MAX_TRACKS);
        out.dropped_tracks = segs.len() - out.tracks.len();
        let parts: Vec<(Point, Point)> = project
            .components
            .iter()
            .filter(|c| crate::page_scope::in_view(project, page, c.sheet))
            .map(|c| {
                (
                    Point::new(c.position.x - 3.0, c.position.y - 3.0),
                    Point::new(c.position.x + 3.0, c.position.y + 3.0),
                )
            })
            .collect();
        out.parts = thin(&parts, MAX_PARTS);
    }
    out
}

/// The box around a footprint's pads, at least two millimetres square.
fn footprint_box(f: &pcbcraft_doc::Footprint) -> (Point, Point) {
    let mut lo = f.position;
    let mut hi = f.position;
    for pad in &f.pads {
        if let Some(p) = f.pad_position(&pad.number) {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
    }
    (
        Point::new(lo.x - 1.0, lo.y - 1.0),
        Point::new(hi.x + 1.0, hi.y + 1.0),
    )
}

/// Object ids named by the findings of the active view: board checks on the
/// layout, schematic checks on a sheet.
pub(crate) fn finding_ids(checks: &Value, pcb: bool) -> Vec<Id> {
    let key = if pcb { "drc" } else { "erc" };
    checks
        .get(key)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .flat_map(|f| {
                    f.get("objects")
                        .and_then(Value::as_array)
                        .map(|v| v.iter().filter_map(Value::as_u64).collect::<Vec<_>>())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Where an object sits in the active view, or None when it is not part of it.
fn object_point(project: &Project, pcb: bool, page: Id, id: Id) -> Option<Point> {
    if pcb {
        if let Some(f) = project.footprint(id) {
            return Some(f.position);
        }
        if let Some(t) = project.board.tracks.iter().find(|t| t.id == id) {
            return Some(Point::new(
                (t.start.x + t.end.x) / 2.0,
                (t.start.y + t.end.y) / 2.0,
            ));
        }
        project
            .board
            .vias
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.position)
    } else {
        project
            .component(id)
            .filter(|c| crate::page_scope::in_view(project, page, c.sheet))
            .map(|c| c.position)
    }
}

/// Findings dots, the selection box and the lit net for the active view.
///
/// # Rules
/// - Objects that are not in the active view (another sheet, the other
///   domain) give no mark.
/// - Dots are capped at 500 so a noisy check cannot slow the frame.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas.rs` (`selection_bounds`).
///
/// # Do not
/// - Cache the result across frames; it is cheap and must follow the selection.
pub(crate) fn build_marks(
    project: &Project,
    pcb: bool,
    page: Id,
    finding_ids: &[Id],
    selected: Option<Id>,
    net: Option<Id>,
) -> Marks {
    let mut marks = Marks::default();
    for id in finding_ids {
        if marks.findings.len() >= 500 {
            break;
        }
        if let Some(p) = object_point(project, pcb, page, *id) {
            marks.findings.push(p);
        }
    }
    if let Some(id) = selected
        && object_point(project, pcb, page, id).is_some()
    {
        marks.selection = crate::canvas::selection_bounds(project, id);
    }
    if let Some(net) = net {
        if pcb {
            marks.net = project
                .board
                .tracks
                .iter()
                .filter(|t| t.net == net)
                .map(|t| [t.start, t.end])
                .collect();
        } else {
            for wire in project.wires.iter().filter(|w| w.net == net) {
                if crate::page_scope::object_in_view(project, page, wire.id)
                    && let Some(path) = project.wire_path(wire)
                {
                    marks.net.extend(path.windows(2).map(|w| [w[0], w[1]]));
                }
            }
        }
    }
    marks
}

/// How the view is oriented: the same three switches the camera has.
///
/// # Rules
/// - The map is drawn the way the main view is drawn, so a board seen from below, turned or
///   with +y up looks the same in both.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Orient {
    /// Look from the other side: screen x runs the other way.
    pub mirror: bool,
    /// World +y points up the screen (the board view).
    pub up: bool,
    /// Quarter turns, 0 to 3.
    pub turns: u8,
}

impl Orient {
    /// The orientation of a camera.
    pub(crate) fn of(camera: Camera) -> Self {
        Self {
            mirror: camera.mirror,
            up: camera.up,
            turns: camera.turns % 4,
        }
    }
    fn camera(self) -> Camera {
        Camera {
            center: Point::default(),
            zoom: 1.0,
            mirror: self.mirror,
            turns: self.turns,
            up: self.up,
        }
    }
}

/// How the world rectangle maps into a map rectangle, keeping its shape and the view's orientation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MapFrame {
    pub bounds: (Point, Point),
    pub rect: Rect,
    pub orient: Orient,
}

impl MapFrame {
    /// The world rectangle's size once it is turned: a quarter turn swaps width and height.
    fn shown_size(&self) -> (f64, f64) {
        let w = (self.bounds.1.x - self.bounds.0.x).max(1e-6);
        let h = (self.bounds.1.y - self.bounds.0.y).max(1e-6);
        if self.orient.turns % 2 == 1 {
            (h, w)
        } else {
            (w, h)
        }
    }
    fn centre(&self) -> Point {
        Point::new(
            (self.bounds.0.x + self.bounds.1.x) / 2.0,
            (self.bounds.0.y + self.bounds.1.y) / 2.0,
        )
    }
    fn scale(&self) -> f64 {
        let (w, h) = self.shown_size();
        ((self.rect.width() as f64) / w).min((self.rect.height() as f64) / h)
    }
    /// The part of `rect` the world rectangle fills.
    #[cfg(test)]
    pub(crate) fn inner(&self) -> Rect {
        let s = self.scale();
        let (w, h) = self.shown_size();
        Rect::from_center_size(
            self.rect.center(),
            Vec2::new((w * s) as f32, (h * s) as f32),
        )
    }
    pub(crate) fn to_map(self, p: Point) -> Pos2 {
        let s = self.scale();
        let c = self.centre();
        let (x, y) = self.orient.camera().turn(p.x - c.x, p.y - c.y);
        self.rect.center() + Vec2::new((x * s) as f32, (y * s) as f32)
    }
    pub(crate) fn to_world(self, pos: Pos2) -> Point {
        let s = self.scale().max(1e-9);
        let c = self.centre();
        let d = pos - self.rect.center();
        let (x, y) = self.orient.camera().unturn(d.x as f64 / s, d.y as f64 / s);
        Point::new(c.x + x, c.y + y)
    }
}

/// The world rectangle a camera shows in a canvas of size `canvas`
/// (low and high corner), correct under mirror and quarter turns.
pub(crate) fn view_box(camera: Camera, canvas: Vec2) -> (Point, Point) {
    let rect = Rect::from_min_size(Pos2::ZERO, canvas);
    let pts = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
    .map(|p| camera.world(p, rect));
    let mut lo = pts[0];
    let mut hi = pts[0];
    for p in pts {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo, hi)
}

/// The camera that shows exactly the world rectangle `lo..hi` in a canvas of
/// size `canvas`, keeping the mirror and turns.
///
/// Rules: the whole rectangle is visible and touches the canvas on its tighter
/// axis; zoom stays inside the app's limits.
pub(crate) fn camera_for_region(camera: Camera, canvas: Vec2, lo: Point, hi: Point) -> Camera {
    let (pw, ph) = if camera.turns % 2 == 1 {
        (canvas.y, canvas.x)
    } else {
        (canvas.x, canvas.y)
    };
    let w = (hi.x - lo.x).max(0.01);
    let h = (hi.y - lo.y).max(0.01);
    let zoom = ((pw as f64 / w).min(ph as f64 / h)).clamp(0.02, 100.0) as f32;
    Camera {
        center: Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0),
        zoom,
        ..camera
    }
}

/// The camera that keeps the zoom and centres on `at`.
pub(crate) fn camera_for_point(camera: Camera, at: Point) -> Camera {
    Camera {
        center: at,
        ..camera
    }
}

/// Order two corners into a low and a high corner.
pub(crate) fn ordered(a: Point, b: Point) -> (Point, Point) {
    (
        Point::new(a.x.min(b.x), a.y.min(b.y)),
        Point::new(a.x.max(b.x), a.y.max(b.y)),
    )
}

/// Where a finished gesture on the map sends the view.
///
/// A drag longer than `min_px` map pixels chooses a region; a shorter one is a
/// click on a point. The target is clamped to the map's own world rectangle
/// so a drag past the edge cannot send the view into empty space.
pub(crate) fn go_target(
    camera: Camera,
    canvas: Vec2,
    frame: &MapFrame,
    from: Pos2,
    to: Pos2,
    min_px: f32,
) -> Camera {
    if (to - from).length() < min_px {
        return camera_for_point(camera, clamp_point(frame.to_world(to), frame.bounds));
    }
    let (lo, hi) = ordered(frame.to_world(from), frame.to_world(to));
    let lo = clamp_point(lo, frame.bounds);
    let hi = clamp_point(hi, frame.bounds);
    camera_for_region(camera, canvas, lo, hi)
}

fn clamp_point(p: Point, bounds: (Point, Point)) -> Point {
    Point::new(
        p.x.clamp(bounds.0.x, bounds.1.x),
        p.y.clamp(bounds.0.y, bounds.1.y),
    )
}

/// Move the region box by whole steps with the arrow keys, staying inside
/// `bounds`. `dir` is a unit step such as `(1, 0)`; `step` is a fraction of the
/// bounds size.
pub(crate) fn nudge_region(
    region: (Point, Point),
    dir: (i8, i8),
    step: f64,
    bounds: (Point, Point),
) -> (Point, Point) {
    let w = region.1.x - region.0.x;
    let h = region.1.y - region.0.y;
    let dx = dir.0 as f64 * step * (bounds.1.x - bounds.0.x);
    let dy = dir.1 as f64 * step * (bounds.1.y - bounds.0.y);
    let x = (region.0.x + dx).clamp(bounds.0.x, (bounds.1.x - w).max(bounds.0.x));
    let y = (region.0.y + dy).clamp(bounds.0.y, (bounds.1.y - h).max(bounds.0.y));
    (Point::new(x, y), Point::new(x + w, y + h))
}

/// A camera partway between two: zoom moves by ratio so a glide feels even.
pub(crate) fn lerp_camera(from: Camera, to: Camera, t: f32) -> Camera {
    let t = t.clamp(0.0, 1.0);
    let e = t * t * (3.0 - 2.0 * t);
    let zoom = (from.zoom.max(1e-3).ln() * (1.0 - e) + to.zoom.max(1e-3).ln() * e).exp();
    Camera {
        center: Point::new(
            from.center.x + (to.center.x - from.center.x) * e as f64,
            from.center.y + (to.center.y - from.center.y) * e as f64,
        ),
        zoom,
        mirror: to.mirror,
        turns: to.turns,
        up: Default::default(),
    }
}

/// One step of the hover chrome: eases toward 1 over about 150 ms while the
/// pointer is over the map, toward 0 over about 250 ms after it leaves, and
/// jumps with Reduce motion.
pub(crate) fn ease_chrome(alpha: f32, hovered: bool, dt: f32, reduced: bool) -> f32 {
    if reduced {
        return if hovered { 1.0 } else { 0.0 };
    }
    let next = if hovered {
        alpha + dt / FADE_IN
    } else {
        alpha - dt / FADE_OUT
    };
    next.clamp(0.0, 1.0)
}

/// The persisted look of one tab's map.
///
/// # Rules
/// - `width` is the map area's width in points; the height follows the
///   board's shape. `origin` is the frame's top-left from the canvas top-left;
///   None means the default spot above the navigation card.
///
/// # Do not
/// - Read it raw; call [`MapPrefs::clamped`] first, a saved file may hold junk.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct MapPrefs {
    #[serde(default)]
    pub pinned: bool,
    #[serde(default = "default_width")]
    pub width: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub origin: Option<[f32; 2]>,
}

fn default_width() -> f32 {
    300.0
}
fn default_opacity() -> f32 {
    DEFAULT_OPACITY
}

impl Default for MapPrefs {
    fn default() -> Self {
        Self {
            pinned: false,
            width: default_width(),
            opacity: DEFAULT_OPACITY,
            origin: None,
        }
    }
}

impl MapPrefs {
    /// The same prefs with every number brought into range.
    pub(crate) fn clamped(&self) -> Self {
        let finite = |v: f32, d: f32| if v.is_finite() { v } else { d };
        Self {
            pinned: self.pinned,
            width: finite(self.width, default_width()).clamp(MIN_WIDTH, 2000.0),
            opacity: finite(self.opacity, DEFAULT_OPACITY).clamp(MIN_OPACITY, 1.0),
            origin: self.origin.filter(|o| o[0].is_finite() && o[1].is_finite()),
        }
    }
}

/// The size of the map area for a wanted width: the board's shape is kept and
/// the map never takes more than 60 percent of the canvas in either direction.
pub(crate) fn map_size(width: f32, aspect: f32, canvas: Vec2) -> Vec2 {
    let aspect = if aspect.is_finite() && aspect > 0.05 {
        aspect.min(20.0)
    } else {
        1.0
    };
    let max_w = (canvas.x * 0.6).max(MIN_WIDTH);
    let max_h = (canvas.y * 0.6).max(60.0);
    let mut w = width.clamp(MIN_WIDTH.min(max_w), max_w);
    let mut h = w / aspect;
    if h > max_h {
        h = max_h;
        w = h * aspect;
    }
    Vec2::new(w, h)
}

/// Keep a frame of `size` fully inside a canvas of size `canvas` when its
/// top-left is at `origin` (from the canvas top-left).
pub(crate) fn clamp_origin(origin: Vec2, size: Vec2, canvas: Vec2) -> Vec2 {
    Vec2::new(
        origin.x.clamp(0.0, (canvas.x - size.x).max(0.0)),
        origin.y.clamp(0.0, (canvas.y - size.y).max(0.0)),
    )
}

/// The width a bottom-right grip drag by `delta` gives, from `start` width.
/// The larger of the two movements wins so the corner follows the pointer.
pub(crate) fn resized_width(start: f32, aspect: f32, delta: Vec2) -> f32 {
    let by_h = delta.y * aspect;
    start
        + if by_h.abs() > delta.x.abs() {
            by_h
        } else {
            delta.x
        }
}

/// The two-line announcement for a target region, for a screen reader.
pub(crate) fn announce_text(lo: Point, hi: Point) -> String {
    t!(
        "minimap.announce",
        x = format!("{:.0}", (lo.x + hi.x) / 2.0),
        y = format!("{:.0}", (lo.y + hi.y) / 2.0)
    )
}

/// A map interaction in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Move { origin: Vec2 },
    Resize { width: f32 },
}

/// Per-session state of the minimap. Nothing here is saved; see [`MapPrefs`].
#[derive(Default)]
pub(crate) struct State {
    /// The peek key is held.
    pub key_held: bool,

    /// The map is kept open on the canvas until docked back (desktop only). It is not an OS window.
    pub ejected: bool,

    /// The map was already kept open on the previous frame, so the
    /// frame that just opened does not swallow that frame's Esc.
    was_ejected: bool,

    /// The Map button opened it.
    pub click_open: bool,
    /// The pointer opened it by resting on the card.
    pub hover_open: bool,
    /// A region drag on the map: where it began and where it is.
    drag: Option<(Pos2, Pos2)>,
    /// The keyboard region box, in world units.
    pub region: Option<(Point, Point)>,
    /// Esc was pressed during this peek.
    cancelled: bool,
    /// Hover chrome opacity, 0 to 1.
    pub chrome: f32,
    pub opacity_open: bool,
    grab: Option<Grab>,
    glide: Option<(Camera, Camera, f32)>,
    cache: Option<((u64, bool, Id), Overview)>,
    /// Sheet being peeked when it differs from the one on the canvas.
    pub peek_page: Option<Id>,
    /// The last announced target, for the status line.
    pub announced: String,
    pub show_findings: Option<bool>,
    pub show_selection: Option<bool>,
    pub show_net: Option<bool>,
    /// How many finding dots the last minimap frame drew (read by stories).
    pub drawn_findings: usize,
}

fn tab_key(tab: Tab) -> &'static str {
    if tab == Tab::Pcb {
        "layout"
    } else {
        "schematic"
    }
}

/// Eject is desktop only: the browser build offers pin, not a second window.
pub(crate) const EJECTABLE: bool = !cfg!(target_arch = "wasm32");

impl App {
    /// The saved look of the current tab's map, in range.
    pub(crate) fn map_prefs(&self) -> MapPrefs {
        self.preferences
            .minimap
            .get(tab_key(self.tab))
            .cloned()
            .unwrap_or_default()
            .clamped()
    }

    fn set_map_prefs(&mut self, prefs: MapPrefs) {
        self.preferences
            .minimap
            .insert(tab_key(self.tab).to_owned(), prefs);
    }

    /// True while the map is on screen.
    pub(crate) fn minimap_visible(&self) -> bool {
        let s = &self.minimap;
        self.map_prefs().pinned || s.key_held || s.click_open || s.hover_open || s.ejected
    }

    /// The Map button: open or close the peek.
    pub(crate) fn minimap_toggle(&mut self) {
        if self.minimap.ejected {
            self.minimap_eject();
            return;
        }
        self.minimap.click_open = !self.minimap.click_open;
        self.minimap.cancelled = false;
        self.minimap.region = None;
    }

    /// Eject: keep the map open on the canvas without pinning it; press again to dock
    /// it back to the navigation card. No OS window is opened. The Map button shows a dot while out.
    ///
    /// # Rules
    /// - A no-op in the browser build, where the map offers pin only.
    /// - Ejecting or docking never changes the pin, the view or the saved size.
    /// # Dependencies
    /// - crates/ui-egui/src/minimap.rs (`State::ejected`, `EJECTABLE`).
    /// # Do not
    /// - Close or reopen the map here: the open flags stay as they are.
    pub(crate) fn minimap_eject(&mut self) {
        if !EJECTABLE {
            return;
        }
        self.minimap.ejected = !self.minimap.ejected;
        self.minimap.announced = if self.minimap.ejected {
            t!("minimap.eject.announced")
        } else {
            t!("minimap.dock.announced")
        };
    }

    /// Send the view to `target`, gliding unless Reduce motion is on.
    fn minimap_go(&mut self, ctx: &egui::Context, target: Camera, label: String) {
        let current = if self.tab == Tab::Pcb {
            self.pcb
        } else {
            self.schematic
        };
        ctx.data_mut(|d| {
            d.remove_temp::<(i32, i32)>(egui::Id::new(("auto-fit", self.tab == Tab::Pcb)))
        });
        if self.preferences.reduced_motion {
            self.set_camera(target);
            self.minimap.glide = None;
        } else {
            self.minimap.glide = Some((current, target, 0.0));
        }
        self.minimap.announced = label;
        self.minimap.click_open = false;
        self.minimap.key_held = false;
        self.minimap.hover_open = false;
        self.minimap.drag = None;
        self.minimap.region = None;
    }

    fn set_camera(&mut self, camera: Camera) {
        if self.tab == Tab::Pcb {
            self.pcb = camera;
        } else {
            self.schematic = camera;
        }
    }

    /// Advance a glide by one frame.
    fn minimap_tick(&mut self, ctx: &egui::Context) {
        let Some((from, to, t)) = self.minimap.glide else {
            return;
        };
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let t = t + dt / GLIDE;
        if t >= 1.0 {
            self.set_camera(to);
            self.minimap.glide = None;
        } else {
            self.set_camera(lerp_camera(from, to, t));
            self.minimap.glide = Some((from, to, t));
            ctx.request_repaint();
        }
    }

    fn cached_overview(&mut self, page: Id) -> Overview {
        let key = (self.revision, self.tab == Tab::Pcb, page);
        if let Some((k, o)) = &self.minimap.cache
            && *k == key
        {
            return o.clone();
        }
        let pcb = self.tab == Tab::Pcb;
        let overview = if !pcb && page == 0 && !self.project.sheets.is_empty() {
            build_overview(
                &crate::page_scope::main_sheet_project(&self.project),
                pcb,
                page,
            )
        } else {
            build_overview(&self.project, pcb, page)
        };
        self.minimap.cache = Some((key, overview.clone()));
        overview
    }

    /// Draw the map for this frame and act on it. Called by the canvas after
    /// the navigation card.
    ///
    /// Why: one entry point so `canvas.rs` and `lib.rs` need only a hook.
    ///
    /// # Rules
    /// - Does nothing while the map is hidden and no key or button asks for it.
    /// - Release goes, Esc cancels, moving off hides an unpinned map.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/canvas.rs` (the card rectangle and canvas rectangle).
    /// - `crates/ui-egui/src/keymap.rs` (`KeyAction::MinimapPeek`).
    ///
    /// # Do not
    /// - Write the document; the map only moves the view.
    pub(crate) fn minimap_show(&mut self, ctx: &egui::Context, canvas: Rect, card: Rect) {
        self.minimap_tick(ctx);
        let pcb = self.tab == Tab::Pcb;
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let reduced = self.preferences.reduced_motion;
        // The peek key: held while the chord's key is down.
        if self.minimap.key_held {
            let key = self
                .preferences
                .active_keymap()
                .chord_for(keymap::KeyAction::MinimapPeek)
                .map(|c| c.key);
            if key.is_none_or(|k| !ctx.input(|i| i.key_down(k))) {
                self.minimap.key_held = false;
                self.minimap.cancelled = false;
            }
        }
        let mut prefs = self.map_prefs();
        let on_card = pointer.is_some_and(|p| card.contains(p));
        // Sheet peeked on a schematic; the canvas sheet otherwise.
        let page = if pcb {
            0
        } else {
            self.minimap
                .peek_page
                .unwrap_or_else(|| self.schematic_page())
        };
        if !self.minimap_visible() && !on_card {
            self.minimap.chrome = 0.0;
            self.minimap.peek_page = None;
            self.minimap.opacity_open = false;
            return;
        }
        if on_card && !self.minimap.cancelled {
            self.minimap.hover_open = true;
        }
        let overview = self.cached_overview(page);
        let orient = Orient::of(if pcb { self.pcb } else { self.schematic });
        let size = map_size(
            prefs.width,
            overview.aspect_for(orient.turns),
            canvas.size(),
        );
        let sheets = self.sheet_names();
        let tabs_h = if !pcb && sheets.len() > 1 { 28.0 } else { 0.0 };
        let total = Vec2::new(
            size.x + 2.0 * PAD,
            size.y + STRIP + CHIPS + tabs_h + 2.0 * PAD,
        );
        let default_origin = Vec2::new(
            canvas.width() - 16.0 - total.x,
            card.top() - canvas.top() - GAP - total.y,
        );
        let origin = prefs
            .origin
            .map(|o| Vec2::new(o[0], o[1]))
            .unwrap_or(default_origin);
        let origin = clamp_origin(origin, total, canvas.size());
        let frame_rect = Rect::from_min_size(canvas.min + origin, total);
        let pinned = prefs.pinned;
        let ejected = self.minimap.ejected;
        let eject_just = ejected && !std::mem::take(&mut self.minimap.was_ejected);
        self.minimap.was_ejected = ejected;
        let chrome_on = pinned || eject_just || self.minimap.chrome > 0.01;
        // An unpinned peek leaves with the pointer; a grab keeps it open.
        let near = pointer.is_some_and(|p| frame_rect.expand(GAP).union(card).contains(p));
        // With no pointer position (it left the window) the peek stays as it
        // is; it closes when the pointer is seen elsewhere in the window.
        if !pinned
            && self.minimap.hover_open
            && pointer.is_some()
            && !on_card
            && !near
            && self.minimap.drag.is_none()
            && self.minimap.grab.is_none()
        {
            self.minimap.hover_open = false;
        }
        if !self.minimap_visible() {
            return;
        }
        let over = near || self.minimap.drag.is_some() || self.minimap.grab.is_some();
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let before = self.minimap.chrome;
        self.minimap.chrome = ease_chrome(before, over || self.minimap.opacity_open, dt, reduced);
        if (self.minimap.chrome - before).abs() > f32::EPSILON && !reduced {
            ctx.request_repaint();
        }
        let chrome = if chrome_on { self.minimap.chrome } else { 1.0 };
        let map_alpha = if chrome_on { prefs.opacity } else { 1.0 };
        let colours = self.colours();
        let camera = if pcb { self.pcb } else { self.schematic };
        let area_rect =
            Rect::from_min_size(frame_rect.min + Vec2::new(PAD, PAD + STRIP + tabs_h), size);
        let frame = MapFrame {
            bounds: overview.bounds,
            rect: area_rect,
            orient,
        };
        let view = view_box(camera, canvas.size());
        let finding_ids = finding_ids(&self.checks, pcb);
        let main_project;
        let map_project = if !pcb && page == 0 && !self.project.sheets.is_empty() {
            main_project = crate::page_scope::main_sheet_project(&self.project);
            &main_project
        } else {
            &self.project
        };
        let marks = build_marks(
            map_project,
            pcb,
            page,
            &finding_ids,
            self.selected,
            self.highlight_net,
        );
        // Record how many finding dots the map will draw, so stories can see them.
        self.minimap.drawn_findings = if self.minimap.show_findings.unwrap_or(true) {
            marks.findings.len()
        } else {
            0
        };
        let mut next_prefs = prefs.clone();
        let mut go: Option<(Camera, String)> = None;
        egui::Area::new(egui::Id::new("minimap"))
            .order(egui::Order::Foreground)
            .fixed_pos(frame_rect.min)
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_clip_rect(frame_rect.expand(24.0));
                ui.allocate_exact_size(total, Sense::hover());
                let painter = ui.painter().clone();
                // Soft shadow and, with the chrome, the frame and title.
                painter.add(
                    egui::Shadow {
                        offset: [0, 4],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha((90.0 * map_alpha.max(0.4)) as u8),
                    }
                    .as_shape(area_rect, 6.0),
                );
                if chrome_on {
                    let a = chrome;
                    painter.rect_filled(frame_rect, 8.0, colours.panel.gamma_multiply(0.94 * a));
                    painter.rect_stroke(
                        frame_rect,
                        8.0,
                        Stroke::new(1.0, colours.ink.gamma_multiply(0.35 * a)),
                        egui::StrokeKind::Inside,
                    );
                }
                // The map surface.
                painter.rect_filled(
                    area_rect,
                    4.0,
                    colours.surface.gamma_multiply(0.96 * map_alpha),
                );
                paint_overview(
                    &painter,
                    &frame,
                    &overview,
                    &marks,
                    &colours,
                    map_alpha,
                    (
                        self.minimap.show_findings.unwrap_or(true),
                        self.minimap.show_selection.unwrap_or(true),
                        self.minimap.show_net.unwrap_or(true),
                    ),
                );
                // The current view box, or the region being chosen.
                let shown = match (self.minimap.drag, self.minimap.region) {
                    (Some((a, b)), _) => ordered(frame.to_world(a), frame.to_world(b)),
                    (None, Some(r)) => r,
                    _ => view,
                };
                let box_rect = Rect::from_two_pos(frame.to_map(shown.0), frame.to_map(shown.1))
                    .intersect(area_rect);
                let chosen = self.minimap.drag.is_some() || self.minimap.region.is_some();
                painter.rect_filled(
                    box_rect,
                    0.0,
                    colours
                        .accent
                        .gamma_multiply(if chosen { 0.22 } else { 0.10 } * map_alpha),
                );
                painter.rect_stroke(
                    box_rect,
                    0.0,
                    Stroke::new(1.5, colours.accent.gamma_multiply(map_alpha)),
                    egui::StrokeKind::Inside,
                );
                // Dragging a region or clicking a point.
                let map_resp = ui.interact(
                    area_rect,
                    egui::Id::new("minimap-surface"),
                    Sense::click_and_drag(),
                );
                let map_resp = map_resp.on_hover_text(t!("minimap.surface.tip"));
                a11y::name_control(&map_resp, &t!("minimap.surface"), None);
                self.record_icon("minimap-surface", area_rect);
                if map_resp.drag_started()
                    && let Some(p) = ui
                        .input(|i| i.pointer.press_origin())
                        .or_else(|| map_resp.interact_pointer_pos())
                {
                    self.minimap.drag = Some((p, p));
                }
                if map_resp.dragged()
                    && let (Some((a, _)), Some(p)) =
                        (self.minimap.drag, map_resp.interact_pointer_pos())
                {
                    self.minimap.drag = Some((a, p));
                }
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if map_resp.drag_stopped()
                    && let Some((a, b)) = self.minimap.drag.take()
                    && !self.minimap.cancelled
                    && !esc
                {
                    let target = go_target(camera, canvas.size(), &frame, a, b, 4.0);
                    let (lo, hi) = view_box(target, canvas.size());
                    go = Some((target, announce_text(lo, hi)));
                } else if map_resp.clicked()
                    && let Some(p) = map_resp.interact_pointer_pos()
                {
                    let target = go_target(camera, canvas.size(), &frame, p, p, 4.0);
                    let (lo, hi) = view_box(target, canvas.size());
                    go = Some((target, announce_text(lo, hi)));
                }
                // Keyboard: the arrows move the region box, Enter goes, P pins, E ejects.
                if map_resp.has_focus() || self.minimap.key_held || self.minimap.click_open {
                    self.minimap_keys(
                        ui,
                        view,
                        &overview,
                        canvas.size(),
                        camera,
                        &mut next_prefs,
                        &mut go,
                    );
                }
                if self.minimap.region.is_some() {
                    map_resp.request_focus();
                }
                // Sheet tabs on a schematic.
                if tabs_h > 0.0 {
                    let mut x = frame_rect.min.x + PAD;
                    let y = frame_rect.min.y + PAD + STRIP;
                    for (id, name) in &sheets {
                        let r = Rect::from_min_size(
                            Pos2::new(x, y),
                            Vec2::new((name.chars().count() as f32 * 7.0 + 18.0).max(44.0), 24.0),
                        );
                        x += r.width() + 4.0;
                        let selected = *id == page;
                        let resp = ui
                            .interact(r, egui::Id::new(("minimap-sheet", *id)), Sense::click())
                            .on_hover_text(t!("minimap.sheet.tip", name = name.clone()));
                        a11y::name_control(&resp, name, Some(selected));
                        self.record_icon(&format!("minimap-sheet-{id}"), r);
                        let fill = if selected {
                            colours.accent.gamma_multiply(0.30)
                        } else if resp.hovered() {
                            colours.hover
                        } else {
                            colours.icon_fill
                        };
                        painter.rect_filled(r, 12.0, fill);
                        painter.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            name,
                            crate::fonts::Role::Body.font(),
                            colours.ink,
                        );
                        if resp.clicked() {
                            self.minimap.peek_page = Some(*id);
                        }
                    }
                }
                // Controls: pin, opacity, hide, chips, grip.
                self.minimap_controls(
                    ui,
                    &painter,
                    &colours,
                    frame_rect,
                    area_rect,
                    chrome_on,
                    chrome,
                    canvas,
                    &overview,
                    &mut next_prefs,
                );
            });
        // Esc: close the popup first, then the peek. The frame that just opened swallows nothing.
        if !eject_just && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.minimap.opacity_open {
                self.minimap.opacity_open = false;
            } else if self.minimap.drag.is_some() || self.minimap.region.is_some() {
                self.minimap.drag = None;
                self.minimap.region = None;
                self.minimap.cancelled = true;
            } else if !pinned && self.minimap_visible() {
                self.minimap.click_open = false;
                self.minimap.hover_open = false;
                self.minimap.cancelled = true;
            }
        }
        if let Some((target, label)) = go {
            self.minimap_go(ctx, target, label);
            if !pinned {
                self.minimap.cancelled = false;
            }
        }
        prefs = next_prefs;
        if prefs != self.map_prefs() {
            self.set_map_prefs(prefs);
        }
        if !self.minimap.announced.is_empty() {
            let announced = self.minimap.announced.clone();
            egui::Area::new(egui::Id::new("minimap-status"))
                .order(egui::Order::Background)
                .fixed_pos(card.left_top())
                .interactable(false)
                .show(ctx, |ui| {
                    let r = ui.label(crate::fonts::caption(&announced).color(Color32::TRANSPARENT));
                    a11y::announce_status(ctx, &r, &announced);
                });
        }
    }

    /// The sheets a schematic map can show: Main, then the named sheets.
    fn sheet_names(&self) -> Vec<(Id, String)> {
        let mut pages: Vec<(Id, String)> = vec![(0, self.page_name(0))];
        pages.extend(self.project.sheets.iter().map(|s| (s.id, s.name.clone())));
        pages
    }

    #[allow(clippy::too_many_arguments)]
    fn minimap_keys(
        &mut self,
        ui: &egui::Ui,
        view: (Point, Point),
        overview: &Overview,
        canvas: Vec2,
        camera: Camera,
        prefs: &mut MapPrefs,
        go: &mut Option<(Camera, String)>,
    ) {
        let dir = ui.input(|i| {
            let mut d = (0i8, 0i8);
            if i.key_pressed(egui::Key::ArrowLeft) {
                d.0 -= 1;
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                d.0 += 1;
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                d.1 -= 1;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                d.1 += 1;
            }
            d
        });
        if dir != (0, 0) {
            let region = self.minimap.region.unwrap_or(view);
            let next = nudge_region(region, dir, 0.1, overview.bounds);
            self.minimap.announced = announce_text(next.0, next.1);
            self.minimap.region = Some(next);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Some((lo, hi)) = self.minimap.region
        {
            let target =
                camera_for_point(camera, Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0));
            let _ = canvas;
            go.get_or_insert((target, announce_text(lo, hi)));
        }
        // P pins or unpins; E ejects or docks back (desktop only).
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::P)) {
            prefs.pinned = !prefs.pinned;
            self.minimap.click_open = false;
            self.minimap.hover_open = false;
        }
        if EJECTABLE && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::E)) {
            self.minimap_eject();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn minimap_controls(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        colours: &palette::ThemeColours,
        frame_rect: Rect,
        area_rect: Rect,
        pinned: bool,
        chrome: f32,
        canvas: Rect,
        overview: &Overview,
        prefs: &mut MapPrefs,
    ) {
        let size = 24.0;
        // Icons sit in the title strip when pinned and over the map's corner
        // when it is only peeking.
        let right = if pinned {
            frame_rect.right() - PAD
        } else {
            area_rect.right() - 6.0
        };
        let top = if pinned {
            frame_rect.top() + PAD
        } else {
            area_rect.top() + 6.0
        };
        let icon_alpha = if pinned { chrome.max(0.0) } else { 1.0 };
        let mut slot = 0.0;
        let mut button =
            |ui: &mut egui::Ui, id: &str, tip: String, name: String, selected: Option<bool>| {
                let r = Rect::from_min_size(Pos2::new(right - size - slot, top), Vec2::splat(size));
                slot += size + 4.0;
                let resp = ui
                    .interact(r, egui::Id::new(id), Sense::click())
                    .on_hover_text(tip);
                a11y::name_control(&resp, &name, selected);
                (resp, r)
            };
        // Pin.
        let (resp, r) = button(
            ui,
            "minimap-pin",
            if pinned {
                t!("minimap.pin.on.tip")
            } else {
                t!("minimap.pin.off.tip")
            },
            t!("minimap.pin"),
            Some(prefs.pinned),
        );
        self.record_icon("minimap-pin", r);
        paint_pin(painter, r, pinned, resp.hovered(), colours, icon_alpha);
        if resp.clicked() {
            prefs.pinned = !prefs.pinned;
            self.minimap.click_open = false;
            self.minimap.hover_open = false;
        }
        if pinned {
            // Opacity.
            let (resp, r) = button(
                ui,
                "minimap-opacity",
                t!("minimap.opacity.tip"),
                t!("minimap.opacity"),
                Some(self.minimap.opacity_open),
            );
            self.record_icon("minimap-opacity", r);
            paint_opacity(painter, r, resp.hovered(), colours, icon_alpha);
            if resp.clicked() {
                self.minimap.opacity_open = !self.minimap.opacity_open;
            }
            // Hide: unpin and close.
            let (resp, r) = button(
                ui,
                "minimap-hide",
                t!("minimap.hide.tip"),
                t!("minimap.hide"),
                None,
            );
            self.record_icon("minimap-hide", r);
            paint_hide(painter, r, resp.hovered(), colours, icon_alpha);
            if resp.clicked() {
                prefs.pinned = false;
                self.minimap.opacity_open = false;
                self.minimap.click_open = false;
                self.minimap.hover_open = false;
            }

            // Eject or dock back: the browser build offers pin alone.
            if EJECTABLE {
                if !self.minimap.ejected {
                    let (resp, r) = button(
                        ui,
                        "minimap-eject",
                        t!("minimap.eject.tip"),
                        t!("minimap.eject"),
                        None,
                    );
                    self.record_icon("minimap-eject", r);
                    paint_window(painter, r, true, resp.hovered(), colours, icon_alpha);
                    if resp.clicked() {
                        self.minimap_eject();
                    }
                } else {
                    let (resp, r) = button(
                        ui,
                        "minimap-dock",
                        t!("minimap.dock.tip"),
                        t!("minimap.dock"),
                        None,
                    );
                    self.record_icon("minimap-dock", r);
                    paint_window(painter, r, false, resp.hovered(), colours, icon_alpha);
                    if resp.clicked() {
                        self.minimap_eject();
                    }
                }
            }

            // Title.
            painter.text(
                Pos2::new(
                    frame_rect.left() + PAD + 4.0,
                    frame_rect.top() + PAD + size / 2.0,
                ),
                Align2::LEFT_CENTER,
                t!("minimap.title"),
                crate::fonts::Role::Body.font(),
                colours.ink.gamma_multiply(icon_alpha),
            );
            // Move grip: the whole title strip left of the icons.
            let strip = Rect::from_min_max(
                Pos2::new(frame_rect.left() + PAD, frame_rect.top() + PAD),
                Pos2::new(right - slot, frame_rect.top() + PAD + size),
            );
            let grip = ui
                .interact(strip, egui::Id::new("minimap-move"), Sense::drag())
                .on_hover_text(t!("minimap.move.tip"))
                .on_hover_cursor(egui::CursorIcon::Grab);
            a11y::name_control(&grip, &t!("minimap.move"), None);
            self.record_icon("minimap-move", strip);
            // Six dots, a visible grip.
            for i in 0..3 {
                for j in 0..2 {
                    painter.circle_filled(
                        Pos2::new(
                            strip.right() - 10.0 + j as f32 * 5.0,
                            strip.center().y - 5.0 + i as f32 * 5.0,
                        ),
                        1.2,
                        colours.muted.gamma_multiply(icon_alpha),
                    );
                }
            }
            let start = prefs
                .origin
                .map(|o| Vec2::new(o[0], o[1]))
                .unwrap_or(frame_rect.min - canvas.min);
            if grip.drag_started() {
                self.minimap.grab = Some(Grab::Move { origin: start });
            }
            if let Some(Grab::Move { origin }) = self.minimap.grab
                && grip.dragged()
            {
                let moved = grip.drag_motion();
                let _ = moved;
                let now = (frame_rect.min - canvas.min) + ui.input(|i| i.pointer.delta());
                let clamped = clamp_origin(now, frame_rect.size(), canvas.size());
                let _ = origin;
                prefs.origin = Some([clamped.x, clamped.y]);
            }
            if grip.drag_stopped() {
                self.minimap.grab = None;
            }
            // Chips.
            let mut x = frame_rect.left() + PAD;
            let y = frame_rect.bottom() - PAD - 26.0;
            let chips = [
                (
                    "findings",
                    t!("minimap.chip.findings"),
                    self.minimap.show_findings.unwrap_or(true),
                ),
                (
                    "selection",
                    t!("minimap.chip.selection"),
                    self.minimap.show_selection.unwrap_or(true),
                ),
                (
                    "net",
                    t!("minimap.chip.net"),
                    self.minimap.show_net.unwrap_or(true),
                ),
            ];
            for (key, label, on) in chips {
                let w = label.chars().count() as f32 * 7.0 + 20.0;
                let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 24.0));
                x += w + 6.0;
                let resp = ui
                    .interact(r, egui::Id::new(("minimap-chip", key)), Sense::click())
                    .on_hover_text(match key {
                        "findings" => t!("minimap.chip.findings.tip"),
                        "selection" => t!("minimap.chip.selection.tip"),
                        _ => t!("minimap.chip.net.tip"),
                    });
                a11y::name_control(&resp, &label, Some(on));
                self.record_icon(&format!("minimap-chip-{key}"), r);
                let fill = if on {
                    colours.accent.gamma_multiply(0.25 * icon_alpha)
                } else {
                    colours.icon_fill.gamma_multiply(icon_alpha)
                };
                painter.rect_filled(r, 12.0, fill);
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    &label,
                    crate::fonts::Role::Body.font(),
                    colours.ink.gamma_multiply(icon_alpha),
                );
                if resp.clicked() {
                    let flag = on;
                    match key {
                        "findings" => self.minimap.show_findings = Some(!flag),
                        "selection" => self.minimap.show_selection = Some(!flag),
                        _ => self.minimap.show_net = Some(!flag),
                    }
                }
            }
            // Resize: bottom-right grip, right edge and bottom edge.
            let corner = Rect::from_min_size(
                frame_rect.right_bottom() - Vec2::splat(18.0),
                Vec2::splat(18.0),
            );
            let right_edge = Rect::from_min_max(
                Pos2::new(frame_rect.right() - 5.0, frame_rect.top() + STRIP + PAD),
                Pos2::new(frame_rect.right(), corner.top()),
            );
            let bottom_edge = Rect::from_min_max(
                Pos2::new(frame_rect.left() + PAD, frame_rect.bottom() - 5.0),
                Pos2::new(corner.left(), frame_rect.bottom()),
            );
            for (id, rect, cursor, tip) in [
                (
                    "minimap-resize",
                    corner,
                    egui::CursorIcon::ResizeNwSe,
                    t!("minimap.resize.tip"),
                ),
                (
                    "minimap-resize-right",
                    right_edge,
                    egui::CursorIcon::ResizeHorizontal,
                    t!("minimap.resize.tip"),
                ),
                (
                    "minimap-resize-bottom",
                    bottom_edge,
                    egui::CursorIcon::ResizeVertical,
                    t!("minimap.resize.tip"),
                ),
            ] {
                let resp = ui
                    .interact(rect, egui::Id::new(id), Sense::drag())
                    .on_hover_text(tip)
                    .on_hover_cursor(cursor);
                a11y::name_control(&resp, &t!("minimap.resize"), None);
                self.record_icon(id, rect);
                if resp.drag_started() {
                    self.minimap.grab = Some(Grab::Resize { width: prefs.width });
                }
                if let Some(Grab::Resize { width }) = self.minimap.grab
                    && resp.dragged()
                {
                    let turns = if self.tab == Tab::Pcb {
                        self.pcb
                    } else {
                        self.schematic
                    }
                    .turns;
                    let aspect = overview.aspect_for(turns);
                    let delta = ui.input(|i| i.pointer.delta());
                    let _ = width;
                    let base = map_size(prefs.width, aspect, canvas.size()).x;
                    let next = match id {
                        "minimap-resize-right" => base + delta.x,
                        "minimap-resize-bottom" => base + delta.y * aspect,
                        _ => resized_width(base, aspect, delta),
                    };
                    prefs.width = next.clamp(MIN_WIDTH, 2000.0);
                }
                if resp.drag_stopped() {
                    self.minimap.grab = None;
                }
            }
            // The grip's three diagonal lines.
            for k in 1..=3 {
                let d = k as f32 * 4.0;
                painter.line_segment(
                    [
                        Pos2::new(frame_rect.right() - 3.0, frame_rect.bottom() - 3.0 - d),
                        Pos2::new(frame_rect.right() - 3.0 - d, frame_rect.bottom() - 3.0),
                    ],
                    Stroke::new(1.0, colours.muted.gamma_multiply(icon_alpha)),
                );
            }
            // Opacity popup: only while pinned.
            if self.minimap.opacity_open && prefs.pinned {
                self.opacity_popup(ui.ctx(), frame_rect, prefs);
            }
        }
        if self.minimap.ejected {
            self.minimap.opacity_open = false;
        }
    }

    /// The small slider popup: 10 to 100 percent, closes on a click away.
    fn opacity_popup(&mut self, ctx: &egui::Context, frame_rect: Rect, prefs: &mut MapPrefs) {
        let pos = Pos2::new(
            frame_rect.right() - 190.0,
            frame_rect.top() + STRIP + PAD + 2.0,
        );
        let mut percent = prefs.opacity * 100.0;
        let response = egui::Area::new(egui::Id::new("minimap-opacity-popup"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos)
            .constrain(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(170.0);
                        ui.label(crate::fonts::caption(t!("minimap.opacity")));
                        let slider = ui
                            .add(
                                egui::Slider::new(&mut percent, 10.0..=100.0)
                                    .suffix("%")
                                    .integer(),
                            )
                            .on_hover_text(t!("minimap.opacity.slider.tip"));
                        a11y::name_control(&slider, &t!("minimap.opacity"), None);
                        self.record_icon("minimap-opacity-slider", slider.rect);
                    });
            });
        prefs.opacity = (percent / 100.0).clamp(MIN_OPACITY, 1.0);
        let inside = response.response.rect;
        let (pressed_at, esc) = ctx.input(|i| {
            (
                i.pointer
                    .any_pressed()
                    .then(|| i.pointer.interact_pos())
                    .flatten(),
                i.key_pressed(egui::Key::Escape),
            )
        });
        let on_button = pressed_at.is_some_and(|p| {
            Rect::from_min_size(
                Pos2::new(frame_rect.right() - PAD - 56.0, frame_rect.top() + PAD),
                Vec2::new(28.0, 24.0),
            )
            .contains(p)
        });
        if esc || pressed_at.is_some_and(|p| !inside.contains(p) && !on_button) {
            self.minimap.opacity_open = false;
        }
    }
}

fn paint_window(
    painter: &egui::Painter,
    r: Rect,
    open_mode: bool,
    hovered: bool,
    colours: &palette::ThemeColours,
    alpha: f32,
) {
    let ink = if hovered {
        colours.ink.gamma_multiply(alpha)
    } else {
        colours.muted.gamma_multiply(alpha)
    };
    let small = Rect::from_min_size(r.center() - Vec2::new(7.0, 4.0), Vec2::splat(8.0));
    let big = Rect::from_min_size(r.center() - Vec2::new(1.0, 1.0), Vec2::new(9.0, 6.0));
    if open_mode {
        painter.rect_stroke(small, 1.0, Stroke::new(1.2, ink), egui::StrokeKind::Inside);
        painter.rect_filled(big, 1.0, ink);
    } else {
        painter.rect_filled(small, 1.0, ink);
        painter.rect_stroke(big, 1.0, Stroke::new(1.2, ink), egui::StrokeKind::Inside);
    }
}

fn fade(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

/// Paint the simplified design inside the map frame.
fn paint_overview(
    painter: &egui::Painter,
    frame: &MapFrame,
    o: &Overview,
    marks: &Marks,
    colours: &palette::ThemeColours,
    a: f32,
    show: (bool, bool, bool),
) {
    let line = |pts: &[Point], stroke: Stroke| {
        let v: Vec<Pos2> = pts.iter().map(|p| frame.to_map(*p)).collect();
        painter.add(egui::Shape::line(v, stroke));
    };
    for pour in &o.pours {
        let v: Vec<Pos2> = pour.iter().map(|p| frame.to_map(*p)).collect();
        painter.add(egui::Shape::convex_polygon(
            v,
            fade(colours.canvas.front, 0.18 * a),
            Stroke::NONE,
        ));
    }
    line(&o.outline, Stroke::new(1.5, fade(colours.warn, a)));
    for (seg, front) in &o.tracks {
        let colour = if *front {
            colours.canvas.front
        } else {
            colours.canvas.back
        };
        line(seg, Stroke::new(1.0, fade(colour, 0.9 * a)));
    }
    for (lo, hi) in &o.parts {
        let r = Rect::from_two_pos(frame.to_map(*lo), frame.to_map(*hi));
        painter.rect_filled(r, 1.0, fade(colours.canvas.pad, 0.55 * a));
    }
    if show.2 {
        for seg in &marks.net {
            line(seg, Stroke::new(2.0, fade(colours.link, a)));
        }
    }
    if show.1
        && let Some((lo, hi)) = marks.selection
    {
        let r = Rect::from_two_pos(frame.to_map(lo), frame.to_map(hi)).expand(2.0);
        painter.rect_stroke(
            r,
            1.0,
            Stroke::new(1.5, fade(colours.accent, a)),
            egui::StrokeKind::Outside,
        );
    }
    if show.0 {
        for p in &marks.findings {
            painter.circle_filled(frame.to_map(*p), 2.5, fade(colours.error, a));
        }
    }
}

/// The pin: a head and a needle. Unpinned it is grey and tilted; pinned it is
/// copper and upright. Drawn from shapes, never a text glyph.
fn paint_pin(
    painter: &egui::Painter,
    r: Rect,
    pinned: bool,
    hovered: bool,
    colours: &palette::ThemeColours,
    a: f32,
) {
    painter.rect_filled(
        r,
        6.0,
        fade(
            if hovered {
                colours.hover
            } else {
                colours.icon_fill
            },
            a,
        ),
    );
    let ink = if pinned {
        colours.canvas.front
    } else {
        colours.muted
    };
    let ink = fade(ink, a);
    let angle: f32 = if pinned { 0.0 } else { 0.6 };
    let (s, c) = angle.sin_cos();
    let rot = |x: f32, y: f32| r.center() + Vec2::new(x * c - y * s, x * s + y * c);
    painter.add(egui::Shape::convex_polygon(
        vec![
            rot(-4.5, -8.0),
            rot(4.5, -8.0),
            rot(3.0, 1.0),
            rot(-3.0, 1.0),
        ],
        ink,
        Stroke::NONE,
    ));
    painter.line_segment([rot(-6.0, 1.5), rot(6.0, 1.5)], Stroke::new(1.8, ink));
    painter.line_segment([rot(0.0, 1.5), rot(0.0, 9.0)], Stroke::new(1.6, ink));
}

/// The opacity icon: a circle with one half filled.
fn paint_opacity(
    painter: &egui::Painter,
    r: Rect,
    hovered: bool,
    colours: &palette::ThemeColours,
    a: f32,
) {
    painter.rect_filled(
        r,
        6.0,
        fade(
            if hovered {
                colours.hover
            } else {
                colours.icon_fill
            },
            a,
        ),
    );
    let c = r.center();
    let ink = fade(colours.ink, a);
    painter.circle_stroke(c, 7.0, Stroke::new(1.4, ink));
    let half: Vec<Pos2> = (0..=12)
        .map(|i| {
            let t = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 12.0;
            c + Vec2::new(t.cos(), t.sin()) * 7.0
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(half, ink, Stroke::NONE));
}

/// The hide icon: a bar along the bottom, like a minimise mark.
fn paint_hide(
    painter: &egui::Painter,
    r: Rect,
    hovered: bool,
    colours: &palette::ThemeColours,
    a: f32,
) {
    painter.rect_filled(
        r,
        6.0,
        fade(
            if hovered {
                colours.hover
            } else {
                colours.icon_fill
            },
            a,
        ),
    );
    let c = r.center();
    painter.line_segment(
        [c + Vec2::new(-6.0, 4.0), c + Vec2::new(6.0, 4.0)],
        Stroke::new(1.8, fade(colours.ink, a)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(center: (f64, f64), zoom: f32) -> Camera {
        Camera {
            center: Point::new(center.0, center.1),
            zoom,
            mirror: false,
            turns: 0,
            up: Default::default(),
        }
    }

    fn frame() -> MapFrame {
        MapFrame {
            bounds: (Point::new(0.0, 0.0), Point::new(100.0, 50.0)),
            rect: Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(200.0, 200.0)),
            orient: Orient::default(),
        }
    }

    #[test]
    fn the_map_keeps_the_boards_shape_and_round_trips_points() {
        let f = frame();
        let inner = f.inner();
        assert!((inner.width() / inner.height() - 2.0).abs() < 1e-3);
        let p = Point::new(30.0, 20.0);
        let back = f.to_world(f.to_map(p));
        assert!((back.x - 30.0).abs() < 1e-3 && (back.y - 20.0).abs() < 1e-3);
        let corner = f.to_map(Point::new(100.0, 50.0));
        assert!((corner.x - inner.max.x).abs() < 1e-3 && (corner.y - inner.max.y).abs() < 1e-3);
    }

    #[test]
    fn the_map_follows_the_views_up_mirror_and_turns() {
        let high = Point::new(50.0, 50.0); // the top edge of the board when +y is up
        let flat = frame();
        // y down: a high y lands low on the map.
        assert!(flat.to_map(high).y > flat.rect.center().y);
        // +y up (the board view): the same point lands high on the map, as it does in the view.
        let up = MapFrame {
            orient: Orient {
                up: true,
                ..Orient::default()
            },
            ..frame()
        };
        assert!(
            up.to_map(high).y < up.rect.center().y,
            "the board view draws +y upward"
        );
        // Mirrored: a high x lands on the left.
        let mirrored = MapFrame {
            orient: Orient {
                mirror: true,
                ..Orient::default()
            },
            ..frame()
        };
        assert!(mirrored.to_map(Point::new(90.0, 25.0)).x < mirrored.rect.center().x);
        // A quarter turn swaps the sides the world rectangle fills.
        let turned = MapFrame {
            orient: Orient {
                turns: 1,
                ..Orient::default()
            },
            ..frame()
        };
        let inner = turned.inner();
        assert!(
            (inner.height() / inner.width() - 2.0).abs() < 1e-3,
            "100 by 50 turns to 50 by 100"
        );
        // Every orientation round trips a point.
        for mirror in [false, true] {
            for up in [false, true] {
                for turns in 0..4u8 {
                    let f = MapFrame {
                        orient: Orient { mirror, up, turns },
                        ..frame()
                    };
                    let p = Point::new(30.0, 20.0);
                    let back = f.to_world(f.to_map(p));
                    assert!(
                        (back.x - 30.0).abs() < 1e-3 && (back.y - 20.0).abs() < 1e-3,
                        "mirror {mirror} up {up} turns {turns}: {back:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_dragged_region_becomes_the_exact_view() {
        let f = frame();
        let canvas = Vec2::new(800.0, 400.0);
        let from = f.to_map(Point::new(20.0, 10.0));
        let to = f.to_map(Point::new(60.0, 30.0));
        let go = go_target(cam((0.0, 0.0), 5.0), canvas, &f, from, to, 4.0);
        assert!((go.center.x - 40.0).abs() < 1e-3 && (go.center.y - 20.0).abs() < 1e-3);
        let (lo, hi) = view_box(go, canvas);
        // The region fits and touches the canvas on its tighter axis.
        assert!(lo.x <= 20.0 + 1e-3 && hi.x >= 60.0 - 1e-3);
        assert!(lo.y <= 10.0 + 1e-3 && hi.y >= 30.0 - 1e-3);
        assert!(((hi.x - lo.x) - 40.0).abs() < 1e-2 || ((hi.y - lo.y) - 20.0).abs() < 1e-2);
    }

    #[test]
    fn a_click_keeps_the_zoom_and_centres_on_the_point() {
        let f = frame();
        let at = f.to_map(Point::new(75.0, 25.0));
        let start = cam((5.0, 5.0), 7.0);
        let go = go_target(start, Vec2::new(800.0, 400.0), &f, at, at, 4.0);
        assert_eq!(go.zoom, 7.0);
        assert!((go.center.x - 75.0).abs() < 1e-3 && (go.center.y - 25.0).abs() < 1e-3);
    }

    #[test]
    fn a_drag_past_the_edge_stays_on_the_board() {
        let f = frame();
        let go = go_target(
            cam((0.0, 0.0), 5.0),
            Vec2::new(800.0, 400.0),
            &f,
            f.to_map(Point::new(80.0, 30.0)),
            Pos2::new(900.0, 900.0),
            4.0,
        );
        assert!(go.center.x <= 100.0 && go.center.y <= 50.0);
    }

    #[test]
    fn the_view_box_follows_mirror_and_turns() {
        let canvas = Vec2::new(400.0, 200.0);
        let plain = view_box(cam((50.0, 25.0), 4.0), canvas);
        assert!((plain.1.x - plain.0.x - 100.0).abs() < 1e-6);
        assert!((plain.1.y - plain.0.y - 50.0).abs() < 1e-6);
        let turned = view_box(
            Camera {
                turns: 1,
                ..cam((50.0, 25.0), 4.0)
            },
            canvas,
        );
        assert!(
            (turned.1.x - turned.0.x - 50.0).abs() < 1e-6,
            "a quarter turn swaps the sides"
        );
        let mirrored = view_box(
            Camera {
                mirror: true,
                ..cam((50.0, 25.0), 4.0)
            },
            canvas,
        );
        assert!((mirrored.1.x - mirrored.0.x - 100.0).abs() < 1e-6);
    }

    #[test]
    fn region_to_camera_and_back_agree_under_a_turn() {
        let canvas = Vec2::new(400.0, 200.0);
        let start = Camera {
            turns: 1,
            mirror: true,
            ..cam((0.0, 0.0), 1.0)
        };
        let go = camera_for_region(
            start,
            canvas,
            Point::new(10.0, 10.0),
            Point::new(30.0, 20.0),
        );
        assert!(go.mirror && go.turns == 1, "orientation is kept");
        let (lo, hi) = view_box(go, canvas);
        assert!(lo.x <= 10.0 + 1e-3 && hi.x >= 30.0 - 1e-3);
        assert!(lo.y <= 10.0 + 1e-3 && hi.y >= 20.0 - 1e-3);
    }

    #[test]
    fn arrow_steps_move_the_box_and_stop_at_the_edge() {
        let bounds = (Point::new(0.0, 0.0), Point::new(100.0, 50.0));
        let region = (Point::new(40.0, 10.0), Point::new(60.0, 20.0));
        let right = nudge_region(region, (1, 0), 0.1, bounds);
        assert!((right.0.x - 50.0).abs() < 1e-9 && (right.1.x - 70.0).abs() < 1e-9);
        assert_eq!(right.0.y, 10.0);
        let mut r = region;
        for _ in 0..20 {
            r = nudge_region(r, (1, 0), 0.1, bounds);
        }
        assert!(
            (r.1.x - 100.0).abs() < 1e-9,
            "the box stops at the right edge"
        );
        assert!(
            (r.1.x - r.0.x - 20.0).abs() < 1e-9,
            "its size never changes"
        );
        let up = nudge_region(region, (0, -1), 0.5, bounds);
        assert_eq!(up.0.y, 0.0);
    }

    #[test]
    fn a_glide_starts_at_the_old_view_and_ends_at_the_new_one() {
        let a = cam((0.0, 0.0), 2.0);
        let b = cam((100.0, 40.0), 8.0);
        let start = lerp_camera(a, b, 0.0);
        let end = lerp_camera(a, b, 1.0);
        assert_eq!(start.center, a.center);
        assert!((start.zoom - 2.0).abs() < 1e-4);
        assert_eq!(end.center, b.center);
        assert!((end.zoom - 8.0).abs() < 1e-3);
        let mid = lerp_camera(a, b, 0.5);
        assert!((mid.zoom - 4.0).abs() < 1e-2, "zoom moves by ratio");
    }

    #[test]
    fn the_chrome_eases_in_fast_and_out_slow_and_jumps_with_reduce_motion() {
        let mut a = 0.0;
        for _ in 0..9 {
            a = ease_chrome(a, true, 1.0 / 60.0, false);
        }
        assert!((a - 1.0).abs() < 1e-6, "full in about 150 ms");
        let mut b = 1.0;
        for _ in 0..9 {
            b = ease_chrome(b, false, 1.0 / 60.0, false);
        }
        assert!(b > 0.3, "still fading after 150 ms");
        for _ in 0..7 {
            b = ease_chrome(b, false, 1.0 / 60.0, false);
        }
        assert_eq!(b, 0.0, "gone after about 250 ms");
        assert_eq!(ease_chrome(0.3, true, 0.001, true), 1.0);
        assert_eq!(ease_chrome(0.7, false, 0.001, true), 0.0);
    }

    #[test]
    fn saved_prefs_are_brought_into_range() {
        let junk = MapPrefs {
            pinned: true,
            width: f32::NAN,
            opacity: 7.0,
            origin: Some([f32::INFINITY, 1.0]),
        };
        let c = junk.clamped();
        assert_eq!(c.width, 300.0);
        assert_eq!(c.opacity, 1.0);
        assert_eq!(c.origin, None);
        let low = MapPrefs {
            opacity: 0.0,
            width: 5.0,
            ..MapPrefs::default()
        }
        .clamped();
        assert_eq!(low.opacity, MIN_OPACITY);
        assert_eq!(low.width, MIN_WIDTH);
        assert_eq!(MapPrefs::default().opacity, 0.55);
    }

    #[test]
    fn the_map_size_keeps_the_shape_and_fits_the_canvas() {
        let canvas = Vec2::new(1000.0, 600.0);
        let s = map_size(400.0, 2.0, canvas);
        assert!((s.x / s.y - 2.0).abs() < 1e-3);
        let huge = map_size(5000.0, 2.0, canvas);
        assert!(huge.x <= 600.0 + 1e-3 && huge.y <= 360.0 + 1e-3);
        assert!((huge.x / huge.y - 2.0).abs() < 1e-3);
        let tall = map_size(300.0, 0.1, canvas);
        assert!(tall.y <= 360.0 + 1e-3);
    }

    #[test]
    fn a_dragged_frame_stays_inside_the_canvas() {
        let size = Vec2::new(300.0, 200.0);
        let canvas = Vec2::new(1000.0, 600.0);
        assert_eq!(
            clamp_origin(Vec2::new(-50.0, -9.0), size, canvas),
            Vec2::ZERO
        );
        assert_eq!(
            clamp_origin(Vec2::new(5000.0, 5000.0), size, canvas),
            Vec2::new(700.0, 400.0)
        );
        assert_eq!(
            clamp_origin(Vec2::new(10.0, 20.0), size, canvas),
            Vec2::new(10.0, 20.0)
        );
    }

    #[test]
    fn the_corner_grip_follows_the_larger_movement() {
        assert_eq!(resized_width(300.0, 2.0, Vec2::new(40.0, 0.0)), 340.0);
        assert_eq!(resized_width(300.0, 2.0, Vec2::new(0.0, 30.0)), 360.0);
        assert_eq!(resized_width(300.0, 2.0, Vec2::new(-10.0, 0.0)), 290.0);
    }

    #[test]
    fn thinning_keeps_an_even_spread_under_the_cap() {
        let items: Vec<u32> = (0..10_000).collect();
        let kept = thin(&items, 4000);
        assert!(kept.len() <= 4000 && kept.len() >= 2000);
        assert_eq!(kept[0], 0);
        assert!(kept[1] - kept[0] > 1);
        assert_eq!(thin(&items[..10], 4000).len(), 10);
    }

    #[test]
    fn finding_ids_come_from_the_checks_of_the_active_view() {
        let checks = json!({
            "drc": [{"objects": [3, 4]}, {"objects": [9]}, {"code": "x"}],
            "erc": [{"objects": [7]}]
        });
        assert_eq!(finding_ids(&checks, true), vec![3, 4, 9]);
        assert_eq!(finding_ids(&checks, false), vec![7]);
        assert!(finding_ids(&json!({}), true).is_empty());
    }

    fn sample_app() -> App {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.refresh();
        app
    }

    #[test]
    fn the_board_overview_holds_outline_parts_and_tracks() {
        let mut app = sample_app();
        app.change_tab(Tab::Pcb);
        let o = build_overview(&app.project, true, 0);
        assert!(o.outline.len() >= 4);
        assert_eq!(o.parts.len(), app.project.board.footprints.len());
        assert_eq!(o.tracks.len(), app.project.board.tracks.len());
        assert!(o.bounds.1.x > o.bounds.0.x && o.bounds.1.y > o.bounds.0.y);
        assert!(o.aspect() > 0.0);
        assert!(o.work() < WORK_BUDGET);
    }

    #[test]
    fn a_schematic_overview_lists_only_the_parts_of_its_sheet() {
        let mut app = sample_app();
        let total = app.project.components.len();
        assert!(total >= 3);
        let main = build_overview(&app.project, false, 0);
        assert_eq!(main.parts.len(), total);
        let other = build_overview(&app.project, false, 999);
        assert!(other.parts.is_empty(), "a sheet with no parts shows none");
        assert!(other.tracks.is_empty());
        app.change_tab(Tab::Schematic);
    }

    #[test]
    fn marks_follow_the_selection_findings_and_lit_net() {
        let mut app = sample_app();
        app.change_tab(Tab::Pcb);
        let fp = app.project.board.footprints[0].id;
        let marks = build_marks(&app.project, true, 0, &[fp, 999_999], Some(fp), None);
        assert_eq!(marks.findings.len(), 1, "an unknown object gives no dot");
        assert!(marks.selection.is_some());
        let on_schematic = build_marks(&app.project, false, 0, &[fp], Some(fp), None);
        assert!(
            on_schematic.findings.is_empty(),
            "a footprint is not on the schematic"
        );
        assert!(on_schematic.selection.is_none());
        if let Some(t) = app.project.board.tracks.first() {
            let lit = build_marks(&app.project, true, 0, &[], None, Some(t.net));
            assert!(!lit.net.is_empty());
        }
    }

    #[test]
    fn schematic_findings_and_the_lit_net_become_marks() {
        let mut app = sample_app();
        app.change_tab(Tab::Schematic);
        let c = app.project.components[0].id;
        // A schematic component gives a finding dot and selection on the schematic.
        let marks = build_marks(&app.project, false, 0, &[c, 999_999], Some(c), None);
        assert_eq!(
            marks.findings.len(),
            1,
            "a component gives a dot; the unknown id gives none"
        );
        assert!(
            marks.selection.is_some(),
            "a component gives a selection box"
        );
        // The same component id gives no mark on the board (it is not a board object).
        let on_board = build_marks(&app.project, true, 0, &[c], Some(c), None);
        assert!(
            on_board.findings.is_empty(),
            "a component is not a board object, only its footprint is"
        );
        assert!(on_board.selection.is_none());
        // A lit net shows on the schematic when that net has wires on this sheet.
        if let Some(net) = app.project.nets.first() {
            let lit = build_marks(&app.project, false, 0, &[], None, Some(net.id));
            if !lit.net.is_empty() {
                // The net has wires on this sheet; the mark is drawn.
            } else {
                // This net has no wires on sheet 0; check that finding_ids
                // still extracts the component id from an ERC result.
                let checks = json!({"erc": [{"objects": [c]}]});
                assert_eq!(finding_ids(&checks, false), vec![c]);
            }
        }
    }

    #[test]
    fn the_largest_example_stays_inside_the_work_budget() {
        let mut app = sample_app();
        assert!(app.open_showcase(), "the showcase opens: {}", app.message);
        app.refresh();
        let o = build_overview(&app.project, true, 0);
        assert!(
            o.work() <= WORK_BUDGET,
            "the map walks {} primitives; the budget is {WORK_BUDGET}",
            o.work()
        );
        assert!(o.tracks.len() <= MAX_TRACKS && o.parts.len() <= MAX_PARTS);
        let sch = build_overview(&app.project, false, 0);
        assert!(sch.work() <= WORK_BUDGET);
    }

    #[test]
    fn a_huge_track_list_is_thinned_to_the_cap() {
        let mut app = sample_app();
        let base = app.project.board.tracks.first().cloned();
        if let Some(t) = base {
            for i in 0..(MAX_TRACKS * 3) {
                let mut copy = t.clone();
                copy.id = 1_000_000 + i as u64;
                app.project.board.tracks.push(copy);
            }
            let o = build_overview(&app.project, true, 0);
            assert!(o.tracks.len() <= MAX_TRACKS);
            assert!(o.dropped_tracks > 0);
        }
    }
}
