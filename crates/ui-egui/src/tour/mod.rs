//! In-app tours: data (#769), anchors (#772), progress and triggers (#773), replay in CI (#774),
//! overlay (#770).

pub mod anchor;
mod app;
pub mod data;
pub(crate) mod layout;
mod offer;
pub mod playback;
pub mod replay;
pub mod state;

pub(crate) use offer::paint_offers;
pub(crate) use state::{Controller, WorkspaceId};
#[cfg(test)]
pub(crate) use layout::step_layout;
#[cfg(test)]
pub(crate) use state::Trigger;
