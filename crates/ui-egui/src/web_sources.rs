//! What the browser build can do with each distributor source.
//!
//! The browser page may only read hosts on its transport's allow list. A
//! distributor source whose host is on that list can be asked; any other gets
//! one plain line in the Library dock saying why it is missing.
//!
//! # Rules
//! - The outcome comes from the transport's allow list, never from a guess.
//! - The line text comes from the catalog (`library.dock.web_source.unreachable`).
//! - The desktop build never calls this.
//!
//! # Dependencies
//! - crates/library/src/source_hosts.rs
//! - crates/library/src/web_transport.rs
use pcbcraft_library::web_transport::WebTransport;

/// The distributor sources, each with its shown name and the host of its documented API.
pub(crate) const DISTRIBUTORS: &[(&str, &str)] = &[
    ("Digi-Key", "api.digikey.com"),
    ("Mouser", "api.mouser.com"),
    ("element14", "api.element14.com"),
    ("LCSC", "www.lcsc.com"),
    ("JLCPCB", "open.jlcpcb.com"),
];

/// What the browser build shows for one distributor source.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The host is on the allow list, so the source can be asked.
    Works,
    /// The host is not reachable; this is the one line to show.
    Explained(String),
}

/// The outcome for each distributor source on `transport`, in list order.
pub(crate) fn outcomes(transport: &WebTransport) -> Vec<(&'static str, Outcome)> {
    DISTRIBUTORS
        .iter()
        .map(|(name, host)| {
            let outcome = if transport.allows_host(host) {
                Outcome::Works
            } else {
                Outcome::Explained(pcbcraft_text::t!(
                    "library.dock.web_source.unreachable",
                    source = *name,
                    host = *host
                ))
            };
            (*name, outcome)
        })
        .collect()
}

/// Draw the one line for every distributor source the browser cannot reach.
pub(crate) fn draw_unreachable(ui: &mut egui::Ui, transport: &WebTransport, muted: egui::Color32) {
    for (_, outcome) in outcomes(transport) {
        if let Outcome::Explained(line) = outcome {
            ui.label(crate::fonts::caption(line).color(muted));
        }
    }
}
