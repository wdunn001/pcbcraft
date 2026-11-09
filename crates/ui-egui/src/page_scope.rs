//! Which schematic items a page shows: one page is its own schematic, and only
//! the Main page also shows every other page.
//!
//! Context: `pages.rs` owns the active page (`App::schematic_page`); the
//! painters, pickers, snaps and the map ask here whether an item belongs in the
//! window. The document model keeps `Project::on_schematic_page` as the strict
//! "does this item live on that page" question; this module adds the view rule
//! on top and leaves netlist, checks and layout sync alone.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`Project::on_schematic_page`, `Project::sheets`)
//! - crates/ui-egui/src/pages.rs (`App::schematic_page`)
//!
//! # Do not
//! - Compare a page id with `==` in a painter or picker; ask here instead.
//! - Move or copy items between pages to make the Main view work.

use super::*;

/// True when an item that lives on `item_page` is shown in the window of page `view`.
///
/// Why: a page is a separate schematic, so a non-Main page shows only its own
/// items. The Main page (id 0) is the one exception: it shows all pages.
///
/// # Rules
/// - View 0 shows every page.
/// - Any other view shows only its own page.
///
/// # Dependencies
/// - crates/ui-egui/src/pages.rs (`App::schematic_page` gives the view)
///
/// # Do not
/// - Use it for the Layers tree rows; those group items by their own page.
pub(crate) fn in_view(project: &Project, view: Id, item_page: Id) -> bool {
    if view == 0 {
        // Main shows every page. Asking the page list is cheap; laying the pages out is not, and
        // this runs once per item on every paint and every hit test.
        return main_uses(project).any(|(page, _)| page == item_page);
    }
    view == item_page
}

/// One use of a page on the Main page: which page, which copy of it, and
/// where it sits on the Main sheet.
///
/// Why: Main is the end-to-end schematic on one continuous sheet. Today every
/// page is used once, but a later feature will use one page several times, so
/// Main is built from a list of these uses and never from a plain list of pages.
///
/// # Rules
/// - `instance` is None for the page's own single use today.
/// - `offset` is added to the page's own coordinates to get Main sheet
///   coordinates; it is derived from page extents, never stored in the file.
/// - The first use (Main itself) always has a zero offset.
///
/// # Dependencies
/// - crates/doc/src/sheet_instances.rs (`SheetInstance`, the stored copies of a page)
///
/// # Do not
/// - Build reuse here; it only lists uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MainInstance {
    /// The page whose items this use shows.
    pub(crate) page: Id,
    /// The stored copy this use stands for, or None for the page itself.
    pub(crate) instance: Option<Id>,
    /// Where this use sits on Main, in millimetres.
    pub(crate) offset: Point,
}

/// Gap in millimetres between two pages side by side on the Main sheet.
const MAIN_GAP: f64 = 20.0;

/// The page and instance of every use Main shows, Main's own first.
///
/// Why: membership questions ("is this page shown on Main") need no layout,
/// so they ask this cheap list and leave the placement to [`main_instances`].
///
/// # Rules
/// - Main itself, then each named page once, in tab order.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Project::sheets`)
///
/// # Do not
/// - Compute extents here; this runs once per painted item.
pub(crate) fn main_uses(project: &Project) -> impl Iterator<Item = (Id, Option<Id>)> + '_ {
    std::iter::once((0, None)).chain(project.sheets.iter().map(|s| (s.id, None)))
}

/// The box around everything page `page` owns, in the page's own coordinates.
///
/// Why: the Main sheet places pages side by side by their extents, so two
/// pages never overlap there.
///
/// # Rules
/// - Strict: only items that live on `page` count, never the Main view.
/// - None when the page owns nothing.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Project::on_schematic_page`, `Component::schematic_views`)
///
/// # Do not
/// - Use it for the frame of a non-Main page; `canvas/sheet.rs` does that.
pub(crate) fn page_extent(project: &Project, page: Id) -> Option<(Point, Point)> {
    let mut pts: Vec<Point> = Vec::new();
    for c in project
        .components
        .iter()
        .filter(|c| c.sheet == page)
        .flat_map(|c| c.schematic_views())
    {
        pts.push(c.position);
        for pin in &c.pins {
            pts.push(c.position.offset(pin.offset.rotate(-c.rotation)));
        }
        pts.extend(
            c.drawing_points()
                .iter()
                .map(|q| c.position.offset(q.rotate(-c.rotation))),
        );
    }
    for w in project.drawn_wires.iter() {
        if project.schematic_drawn_wire_page(w) == page {
            pts.extend(w.points.iter().copied());
        }
    }
    pts.extend(
        project
            .labels
            .iter()
            .filter(|l| project.schematic_label_page(l) == page)
            .map(|l| l.position),
    );
    pts.extend(
        project
            .ports
            .iter()
            .filter(|q| q.sheet == page)
            .map(|q| q.position),
    );
    if page == 0 {
        pts.extend(
            project
                .graphics
                .iter()
                .filter(|g| g.layer == "schematic")
                .flat_map(|g| g.points.iter().copied()),
        );
    }
    let first = *pts.first()?;
    let (mut lo, mut hi) = (first, first);
    for q in &pts {
        lo = Point::new(lo.x.min(q.x), lo.y.min(q.y));
        hi = Point::new(hi.x.max(q.x), hi.y.max(q.y));
    }
    Some((
        Point::new(lo.x - 6.0, lo.y - 12.0),
        Point::new(hi.x + 6.0, hi.y + 14.0),
    ))
}

/// Every use of a page on the Main sheet with its placement.
///
/// Why: the Main view, its frame, its pickers and its map all ask this one
/// list, so adding a second use of a page later changes only this function.
///
/// # Rules
/// - Pages sit side by side, left to right in tab order, top aligned with
///   Main's own top, with a gap of about 20 mm (rounded to the 0.5 mm snap grid so
///   a drag on Main lands on that grid in page coordinates).
/// - A page that owns nothing takes no room.
/// - Deterministic: the same project always gives the same placement.
///
/// # Dependencies
/// - crates/ui-egui/src/page_scope.rs (`page_extent`, `main_uses`)
///
/// # Do not
/// - Cache the list; pages are added and removed while the app runs.
pub(crate) fn main_instances(project: &Project) -> Vec<MainInstance> {
    let grid = 0.5;
    let snap = |v: f64| (v / grid).round() * grid;
    let mut cursor: Option<f64> = None;
    let mut top = 0.0;
    let mut out = Vec::new();
    for (page, instance) in main_uses(project) {
        let mut offset = Point::new(0.0, 0.0);
        if let Some((lo, hi)) = page_extent(project, page) {
            match cursor {
                None => {
                    top = lo.y;
                    cursor = Some(hi.x + MAIN_GAP);
                }
                Some(x) => {
                    offset = Point::new(snap(x - lo.x), snap(top - lo.y));
                    cursor = Some(hi.x + offset.x + MAIN_GAP);
                }
            }
        }
        out.push(MainInstance {
            page,
            instance,
            offset,
        });
    }
    out
}

/// Move every schematic item of `project` to its place on the Main sheet.
///
/// Why: the Main view is one sheet with one coordinate space. Painters and
/// pickers run on this copy; wires follow because they are drawn between pins.
///
/// # Rules
/// - Shifts parts (and each unit), labels, ports and drawn wires by the offset
///   of the page they live on. Nothing else changes, ids stay the same.
/// - The page of a label or drawn wire is worked out before any item moves.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Project::schematic_label_page`, `Project::schematic_drawn_wire_page`)
///
/// # Do not
/// - Save this copy or send it to the engine.
pub(crate) fn apply_main_layout(project: &mut Project) {
    let placed = main_instances(project);
    let offset_of = |page: Id| {
        placed
            .iter()
            .find(|i| i.page == page)
            .map_or(Point::new(0.0, 0.0), |i| i.offset)
    };
    let label_pages: Vec<Id> = project
        .labels
        .iter()
        .map(|l| project.schematic_label_page(l))
        .collect();
    let wire_pages: Vec<Id> = project
        .drawn_wires
        .iter()
        .map(|w| project.schematic_drawn_wire_page(w))
        .collect();
    for c in &mut project.components {
        let o = offset_of(c.sheet);
        c.position = c.position.offset(o);
        for u in &mut c.units {
            u.position = u.position.offset(o);
        }
    }
    for (l, page) in project.labels.iter_mut().zip(label_pages) {
        l.position = l.position.offset(offset_of(page));
    }
    for q in &mut project.ports {
        q.position = q.position.offset(offset_of(q.sheet));
    }
    for (w, page) in project.drawn_wires.iter_mut().zip(wire_pages) {
        let o = offset_of(page);
        for pt in &mut w.points {
            *pt = pt.offset(o);
        }
    }
}

/// The page whose region of the Main sheet contains `at`, or Main.
///
/// Why: a click on the Main sheet that adds a label or a wire bend must land
/// on the page drawn under the pointer, in that page's own coordinates.
///
/// # Rules
/// - Regions are the page extents moved by their offsets; the first match wins.
///
/// # Dependencies
/// - crates/ui-egui/src/page_scope.rs (`main_instances`, `page_extent`)
///
/// # Do not
/// - Use it off the Main page.
pub(crate) fn page_at(project: &Project, at: Point) -> Id {
    for i in main_instances(project) {
        if let Some((lo, hi)) = page_extent(project, i.page) {
            let (lo, hi) = (lo.offset(i.offset), hi.offset(i.offset));
            if at.x >= lo.x && at.x <= hi.x && at.y >= lo.y && at.y <= hi.y {
                return i.page;
            }
        }
    }
    0
}

/// True when schematic object `object` is shown in the window of page `view`.
///
/// Why: wires, labels, drawn wires and graphics have no page field of their
/// own; the project works their page out from the parts and ports they touch.
///
/// # Rules
/// - View 0 shows every object, including a wire that joins parts of two pages.
/// - Any other view shows the object only when it lives on that page.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Project::on_schematic_page`)
///
/// # Do not
/// - Call it for board objects.
pub(crate) fn object_in_view(project: &Project, view: Id, object: Id) -> bool {
    if view == 0 {
        return true;
    }
    project.on_schematic_page(view, object)
}

/// The page schematic object `object` lives on, or None when it is on none.
///
/// Why: selecting an item from a list (the Layers tree) must open the page the
/// item belongs to, so the item is never selected out of sight.
///
/// # Rules
/// - Checks the Main page first, then each named page in order.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Project::on_schematic_page`, `Project::sheets`)
///
/// # Do not
/// - Cache the answer; moving a part changes it.
pub(crate) fn page_of_object(project: &Project, object: Id) -> Option<Id> {
    std::iter::once(0)
        .chain(project.sheets.iter().map(|s| s.id))
        .find(|page| project.on_schematic_page(*page, object))
}

/// The project as Main lays it out, with what it was built from.
#[derive(Default)]
pub(crate) struct MainViewCache {
    /// The engine revision, the part count and the page count the view was built from.
    key: Option<(u64, usize, usize)>,
    /// The laid out project, shared with every caller until the project changes.
    view: Option<std::sync::Arc<Project>>,
}

/// The project a window should read: the real one, or Main's placed copy of it.
pub(crate) enum SheetView<'a> {
    /// The real project, for any page but Main.
    Own(&'a Project),
    /// Main's laid out copy, shared from the cache.
    Placed(std::sync::Arc<Project>),
}

impl std::ops::Deref for SheetView<'_> {
    type Target = Project;
    fn deref(&self) -> &Project {
        match self {
            Self::Own(p) => p,
            Self::Placed(p) => p,
        }
    }
}

impl App {
    /// True when this window is Main and the project has named pages.
    ///
    /// Why: Main shows every page as one sheet and also holds parts of its own. The other pages'
    /// items are read-only there. A project with no named page has only Main, which is editable.
    ///
    /// # Rules
    /// - Only the Schematic tab; the board is never a grouping.
    pub(crate) fn main_is_grouping(&self) -> bool {
        self.tab == Tab::Schematic && self.schematic_page() == 0 && !self.project.sheets.is_empty()
    }

    /// Refuse an edit of another page's item from Main, and say why.
    ///
    /// Why: Main shows every page as one sheet and keeps parts of its own. The other pages' items
    /// are shown there but belong to their pages, so they are changed on their own page.
    ///
    /// # Rules
    /// - Refuses a command that names, by `id` or `ids`, a part, port, label, wire or pad of a named page.
    /// - Lets through everything about Main's own items, new parts placed on Main, nets, pages and settings.
    ///
    /// # Do not
    /// - Call it for selection or view changes; those stay allowed on Main.
    pub(crate) fn main_refuses(&mut self, _command: &str, params: &serde_json::Value) -> bool {
        if !self.main_is_grouping() {
            return false;
        }
        let ids = params
            .get("id")
            .into_iter()
            .chain(
                params
                    .get("ids")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten(),
            )
            .filter_map(|v| v.as_u64());
        let on_another_page = ids
            .into_iter()
            .any(|id| page_of_object(&self.project, id).is_some_and(|page| page != 0));
        if on_another_page {
            self.message = t!("schematic.main.read_only");
            return true;
        }
        false
    }

    /// Open the page of object `id` when the current page cannot show it.
    ///
    /// Why: a list click on a part of another page must not select something
    /// the window hides. On the Main page every item is visible, so nothing moves.
    ///
    /// # Rules
    /// - Does nothing off the Schematic tab or when the object is in view.
    /// - Keeps the selection; only the active page changes.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/layers_tree.rs (`Action::Select` calls it)
    ///
    /// # Do not
    /// - Call `show_page`; it clears the selection.
    pub(crate) fn follow_object_page(&mut self, id: Id) {
        if self.tab != Tab::Schematic {
            return;
        }
        let view = self.schematic_page();
        if object_in_view(&self.project, view, id) {
            return;
        }
        if let Some(page) = page_of_object(&self.project, id) {
            self.active_page = page;
        }
    }

    /// The project as the Schematic canvas sees it: on the Main page every page
    /// is moved to its place on the one Main sheet, elsewhere it is the project.
    ///
    /// Why: readers of schematic geometry (pickers, snaps, box select, hit
    /// tests, the map) must see the same positions the painter draws.
    ///
    /// # Rules
    /// - Borrowed, free, on every other page and tab.
    /// - Owned on Main: a light copy with the schematic items placed.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/page_scope.rs (`apply_main_layout`)
    ///
    /// # Do not
    /// - Write through it; edits go to `self.project` in page coordinates.
    pub(crate) fn sheet_view(&self) -> SheetView<'_> {
        if self.tab != Tab::Schematic
            || self.schematic_page() != 0
            || self.project.sheets.is_empty()
        {
            return SheetView::Own(&self.project);
        }
        let key = (
            self.revision,
            self.project.components.len(),
            self.project.sheets.len(),
        );
        let mut cache = self
            .main_view
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.key != Some(key) || cache.view.is_none() {
            cache.view = Some(std::sync::Arc::new(main_sheet_project(&self.project)));
            cache.key = Some(key);
        }
        match &cache.view {
            Some(view) => SheetView::Placed(std::sync::Arc::clone(view)),
            None => SheetView::Own(&self.project),
        }
    }
}

/// A light copy of `p` with every page placed on the one Main sheet.
///
/// Why: readers of Main geometry (the canvas, the pickers, the map) work on
/// this copy; edits never go through it.
///
/// # Rules
/// - Keeps only what the schematic needs; the board is left out.
///
/// # Dependencies
/// - crates/ui-egui/src/page_scope.rs (`apply_main_layout`)
///
/// # Do not
/// - Send it to the engine or save it.
pub(crate) fn main_sheet_project(p: &Project) -> Project {
    let mut v = Project {
        name: p.name.clone(),
        meta: p.meta.clone(),
        components: p.components.clone(),
        nets: p.nets.clone(),
        wires: p.wires.clone(),
        labels: p.labels.clone(),
        ports: p.ports.clone(),
        drawn_wires: p.drawn_wires.clone(),
        graphics: p.graphics.clone(),
        no_connects: p.no_connects.clone(),
        sheets: p.sheets.clone(),
        page_settings: p.page_settings.clone(),
        root_sheet_name: p.root_sheet_name.clone(),
        ..Project::default()
    };
    apply_main_layout(&mut v);
    v
}

impl App {
    /// Where the Main sheet places page `page`, as an offset to add to page coordinates.
    ///
    /// Why: edits made on Main arrive in sheet coordinates and must be stored
    /// in the coordinates of the page the item lives on.
    ///
    /// # Rules
    /// - Zero off the Main page and for Main itself.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/page_scope.rs (`main_instances`)
    ///
    /// # Do not
    /// - Apply it to a distance; only to a position.
    pub(crate) fn main_offset(&self, page: Id) -> Point {
        if self.tab != Tab::Schematic || self.schematic_page() != 0 {
            return Point::new(0.0, 0.0);
        }
        main_instances(&self.project)
            .into_iter()
            .find(|i| i.page == page)
            .map_or(Point::new(0.0, 0.0), |i| i.offset)
    }

    /// A position taken on the Main sheet, turned into the page that owns the
    /// spot and the position inside that page.
    ///
    /// # Rules
    /// - Off the Main page the spot is on the current page, unchanged.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/page_scope.rs (`page_at`, `main_offset`)
    ///
    /// # Do not
    /// - Use it for a drag of an existing item; use `item_to_page`.
    pub(crate) fn spot_to_page(&self, at: Point) -> (Id, Point) {
        if self.tab != Tab::Schematic || self.schematic_page() != 0 {
            return (self.schematic_page(), at);
        }
        // Main keeps its own parts beside the other pages. What is drawn on Main belongs to Main,
        // wherever the pointer is; another page's items are not edited from here.
        if self.main_is_grouping() {
            return (0, at);
        }
        let page = page_at(&self.project, at);
        let o = self.main_offset(page);
        (page, Point::new(at.x - o.x, at.y - o.y))
    }

    /// A sheet position of an existing item, turned into its own page's coordinates.
    ///
    /// # Rules
    /// - `object` is the id of a part, label, port or drawn wire; unknown ids stay unchanged.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/page_scope.rs (`page_of_object`, `main_offset`)
    ///
    /// # Do not
    /// - Use it for a new item; use `spot_to_page`.
    pub(crate) fn item_to_page(&self, object: Id, at: Point) -> Point {
        let Some(page) = page_of_object(&self.project, object) else {
            return at;
        };
        let o = self.main_offset(page);
        Point::new(at.x - o.x, at.y - o.y)
    }

    /// A run of Main sheet positions, turned into the coordinates of `page`.
    ///
    /// # Rules
    /// - Off the Main page, or for Main itself, the points are unchanged.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/page_scope.rs (`main_offset`)
    ///
    /// # Do not
    /// - Mix points of two pages in one call.
    pub(crate) fn points_to_page(&self, page: Id, points: &[Point]) -> Vec<Point> {
        let o = self.main_offset(page);
        points
            .iter()
            .map(|p| Point::new(p.x - o.x, p.y - o.y))
            .collect()
    }
}
