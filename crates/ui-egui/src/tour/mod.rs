//! In-app tours: progress, triggers and offers (#773). Data files and overlay
//! drawing live in sibling modules (#769, #770).

mod offer;
pub mod state;

pub(crate) use offer::paint_offers;
pub(crate) use state::{Controller, TourMeta, Trigger, WorkspaceId};
