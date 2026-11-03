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
        return main_instances(project).iter().any(|i| i.page == item_page);
    }
    view == item_page
}

/// One use of a page on the Main page: which page, and which copy of it.
///
/// Why: Main is the end-to-end schematic. Today every page is used once, but a
/// later feature will use one page several times, so Main is built from a list
/// of these uses and never from a plain list of pages.
///
/// # Rules
/// - `instance` is None for the page's own single use today.
/// - `offset` is where the use sits on Main, added to the page's own
///   coordinates; it is zero today (see the PR notes).
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

/// Every use of a page that the Main page shows, Main's own items first.
///
/// Why: the Main view, its frame, its pickers and its map all ask this one
/// list, so adding a second use of a page later changes only this function.
///
/// # Rules
/// - Main itself, then each named page once, in tab order.
/// - Total: a project with no named pages lists Main alone.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Project::sheets`)
///
/// # Do not
/// - Cache the list; pages are added and removed while the app runs.
pub(crate) fn main_instances(project: &Project) -> Vec<MainInstance> {
    std::iter::once(0)
        .chain(project.sheets.iter().map(|s| s.id))
        .map(|page| MainInstance {
            page,
            instance: None,
            offset: Point::new(0.0, 0.0),
        })
        .collect()
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

impl App {
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
}
