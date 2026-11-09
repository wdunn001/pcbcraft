//! The Overview page of a part: previews, key parameters, lifecycle and compliance.
//!
//! Context: issue #695. The previews reuse the browser's own symbol and footprint drawing
//! (`library_dock.rs`), so a card and its page always show the same picture.
//!
//! Local rules: a value no source gave reads "Not listed"; it is never guessed.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_workspace/facts.rs
//! - crates/ui-egui/src/library_dock.rs (`dock_paint_symbol`, `dock_paint_footprint`)
//!
//! # Do not
//! - Read a file or the network here.
use super::facts::{PartFacts, date_text};
use super::*;
use crate::ui_kit::card::card;
use pcbcraft_library::listing::{Lifecycle, Unit};

/// A number with an SI prefix and its unit symbol, such as `330 Ω` or `4.7 µF`.
fn si_text(value: f64, unit: Unit) -> String {
    let symbol = match unit {
        Unit::Ohm => "\u{3a9}",
        Unit::Farad => "F",
        Unit::Henry => "H",
        Unit::Hertz => "Hz",
        _ => "",
    };
    let (scaled, prefix) = [
        (1e9, "G"),
        (1e6, "M"),
        (1e3, "k"),
        (1.0, ""),
        (1e-3, "m"),
        (1e-6, "\u{b5}"),
        (1e-9, "n"),
        (1e-12, "p"),
    ]
    .iter()
    .find(|(f, _)| value.abs() >= *f)
    .map_or((value, ""), |(f, p)| (value / f, *p));
    let digits = format!("{scaled:.3}");
    let digits = digits.trim_end_matches('0').trim_end_matches('.');
    format!("{digits} {prefix}{symbol}").trim().to_owned()
}

/// The key parameters of a part as label and value, `None` for a value no source gave.
///
/// # Rules
/// - Order is fixed so the Overview and the Compare table read alike.
pub(crate) fn key_parameters(f: &PartFacts) -> Vec<(String, Option<String>)> {
    let l = f.listing.as_ref();
    let range = l.and_then(|l| match (l.temp_min_c, l.temp_max_c) {
        (Some(a), Some(b)) => Some(format!("{a} to {b} \u{b0}C")),
        _ => None,
    });
    vec![
        (
            t!("libws.param.package"),
            f.package.clone().or_else(|| {
                l.filter(|l| !l.package_text.is_empty())
                    .map(|l| l.package_text.clone())
            }),
        ),
        (t!("libws.param.pins"), f.pins.map(|n| n.to_string())),
        (
            t!("libws.param.value"),
            l.and_then(|l| l.value.zip(l.value_unit))
                .map(|(v, u)| si_text(v, u)),
        ),
        (
            t!("libws.param.tolerance"),
            l.and_then(|l| l.tolerance_pct).map(|(lo, hi)| {
                if (lo - hi).abs() < f32::EPSILON {
                    format!("{lo} %")
                } else {
                    format!("-{lo} / +{hi} %")
                }
            }),
        ),
        (
            t!("libws.param.voltage"),
            l.and_then(|l| l.rated_voltage_v).map(|v| format!("{v} V")),
        ),
        (
            t!("libws.param.power"),
            l.and_then(|l| l.power_w).map(|w| format!("{w} W")),
        ),
        (t!("libws.param.temperature"), range),
    ]
}

/// The lifecycle in words, from the typed record first, then the listing.
pub(crate) fn lifecycle_words(f: &PartFacts) -> Option<String> {
    let l = f
        .lifecycle
        .or_else(|| f.listing.as_ref().map(|l| l.lifecycle))
        .filter(|l| *l != Lifecycle::Unknown)?;
    Some(pcbcraft_doc::lifecycle_text(l).to_owned())
}

/// Yes, no or nothing for a typed flag.
pub(crate) fn flag_words(v: Option<bool>) -> Option<String> {
    v.map(|b| {
        if b {
            t!("props.sourcing.yes")
        } else {
            t!("props.sourcing.no")
        }
    })
}

impl App {
    /// The Overview page.
    pub(super) fn library_overview_page(&mut self, ui: &mut egui::Ui, card: &Card, f: &PartFacts) {
        let t = theme::tokens(ui.ctx());
        ui.columns(3, |cols| {
            for (i, col) in cols.iter_mut().enumerate() {
                let (title, caption) = match i {
                    0 => (
                        t!("libws.preview.symbol"),
                        f.pins.map(|n| t!("library.dock.pins", count = n)),
                    ),
                    1 => (
                        t!("libws.preview.footprint"),
                        self.dock_card_footprint(card)
                            .map(|d| t!("library.dock.pads", count = d.pads.len())),
                    ),
                    _ => (t!("libws.preview.model"), None),
                };
                card_fn(col, &title, caption.as_deref(), |ui| {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 170.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, 4.0, self.surface());
                    let drawn = match i {
                        0 => self.dock_paint_symbol(ui, rect, card),
                        1 => self.dock_paint_footprint(ui, rect, card),
                        _ => false,
                    };
                    if i == 2 {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            t!("libws.preview.model_hint"),
                            fonts::Role::Body.font(),
                            t.muted,
                        );
                    } else if !drawn {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            t!("libws.preview.none"),
                            fonts::Role::Body.font(),
                            t.muted,
                        );
                    }
                });
            }
        });
        ui.columns(2, |cols| {
            card_fn(&mut cols[0], &t!("libws.card.parameters"), None, |ui| {
                for (label, value) in key_parameters(f) {
                    value_row(ui, &label, value);
                }
            });
            card_fn(&mut cols[1], &t!("libws.card.lifecycle"), None, |ui| {
                value_row(ui, &t!("libws.life.lifecycle"), lifecycle_words(f));
                value_row(
                    ui,
                    &t!("props.sourcing.rohs"),
                    flag_words(f.rohs.or(f.listing.as_ref().and_then(|l| l.rohs))),
                );
                value_row(ui, &t!("props.sourcing.reach"), flag_words(f.reach));
                value_row(
                    ui,
                    &t!("props.sourcing.moisture"),
                    f.moisture
                        .map(|n| t!("props.sourcing.moisture.level", level = n)),
                );
                let checked = f.quotes.iter().map(|q| q.retrieved_at).max();
                value_row(ui, &t!("libws.life.checked"), checked.map(date_text));
            });
        });
        card_fn(ui, &t!("libws.card.description"), None, |ui| {
            let text = if f.description.trim().is_empty() {
                t!("libws.not_listed")
            } else {
                f.description.clone()
            };
            ui.label(fonts::text(fonts::Role::Body, text).color(t.text));
        });
    }
}

/// A card with a title, an optional caption and a body.
fn card_fn(
    ui: &mut egui::Ui,
    title: &str,
    caption: Option<&str>,
    body: impl FnOnce(&mut egui::Ui),
) {
    card(ui, title, caption, |_| {}, body);
}

/// One label and value row of a card; a missing value is said plainly.
pub(super) fn value_row(ui: &mut egui::Ui, label: &str, value: Option<String>) {
    let t = theme::tokens(ui.ctx());
    form_row(ui, FormWidth::Page, label, None, None, |ui| match value {
        Some(v) => {
            ui.label(fonts::mono(v).color(t.text));
        }
        None => {
            ui.label(fonts::text(fonts::Role::Body, t!("libws.not_listed")).color(t.muted));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn si_values_read_with_their_prefix() {
        assert_eq!(si_text(330.0, Unit::Ohm), "330 \u{3a9}");
        assert_eq!(si_text(4700.0, Unit::Ohm), "4.7 k\u{3a9}");
        assert_eq!(si_text(1e-7, Unit::Farad), "100 nF");
    }
}
