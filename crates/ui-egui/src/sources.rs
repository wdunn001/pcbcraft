//! Part sources in the library list: remote results appear beside local parts
//! as cards with a source badge, stock, a price break and the source's credit.
use super::*;
use pcbcraft_library::sources::{FixtureSource, PartResult, PartSource, ResponseCache};

const SAMPLE: &str = include_str!("../../library/tests/fixtures/sources/sample.json");

#[derive(Default)]
pub(super) struct PartSources {
    source: Option<Box<dyn PartSource>>,
    cache: ResponseCache,
    notice: String,
}

fn now_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// One plain line about a result's stock.
fn stock_line(part: &PartResult) -> String {
    let stock = if part.stock == 0 {
        "out of stock".to_string()
    } else {
        format!("{} in stock", part.stock)
    };
    let price = part.price_breaks.first().map(|b| {
        format!(
            "from {:.2} {} each at {}",
            b.unit_price, b.currency, b.quantity
        )
    });
    match price {
        Some(price) => format!("{stock} · {price}"),
        None => stock,
    }
}

impl App {
    /// The remote half of the part list. Local parts are drawn by the caller.
    pub(super) fn source_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("From part sources")
                .size(10.0)
                .color(self.muted()),
        );
        #[cfg(target_arch = "wasm32")]
        {
            ui.label(
                egui::RichText::new("Part sources are available in the desktop app.")
                    .small()
                    .color(self.muted()),
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.part_sources.source.is_none() {
                ui.label(
                    egui::RichText::new(
                        "Part sources are off. Only parts on this computer are listed.",
                    )
                    .small()
                    .color(self.muted()),
                );
                if ui
                    .button("Try sample source")
                    .on_hover_text(
                        "Turn on a small built-in set of sample parts, to see how a part source looks. No account needed.",
                    )
                    .clicked()
                {
                    match FixtureSource::from_json(SAMPLE) {
                        Ok(source) => self.part_sources.source = Some(Box::new(source)),
                        Err(e) => self.part_sources.notice = e.to_string(),
                    }
                }
                if !self.part_sources.notice.is_empty() {
                    ui.label(egui::RichText::new(&self.part_sources.notice).small());
                }
                return;
            }
            let query = self.part_search.trim().to_owned();
            if query.is_empty() {
                ui.label(
                    egui::RichText::new("Type a part number or keyword to search the sources.")
                        .small()
                        .color(self.muted()),
                );
                return;
            }
            let Some(source) = self.part_sources.source.as_deref() else {
                return;
            };
            let before = self.part_sources.cache.misses;
            let found = self.part_sources.cache.search(source, &query, now_secs());
            if self.part_sources.cache.misses != before {
                eprintln!(
                    "part search source={} results={}",
                    source.id(),
                    found.as_ref().map(Vec::len).unwrap_or(0)
                );
            }
            let name = source.name().to_owned();
            let credit = source.attribution();
            match found {
                Err(e) => {
                    ui.label(egui::RichText::new(e.to_string()).small());
                }
                Ok(parts) if parts.is_empty() => {
                    ui.label(
                        egui::RichText::new(format!("{name} has nothing for \"{query}\"."))
                            .small()
                            .color(self.muted()),
                    );
                }
                Ok(parts) => {
                    for part in parts {
                        self.source_card(ui, &part, &credit.text);
                    }
                }
            }
            if ui
                .small_button("Forget saved answers")
                .on_hover_text(
                    "Clear the answers kept for a day so the next search asks the source again.",
                )
                .clicked()
            {
                self.part_sources.cache.purge();
            }
        }
    }
    fn source_card(&mut self, ui: &mut egui::Ui, part: &PartResult, credit: &str) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button(&part.mpn)
                .on_hover_text("Pick this part; the next click on the schematic places it with its maker and part number filled in.")
                .clicked()
            {
                let kind = TEMPLATES
                    .iter()
                    .find(|t| t.kind == part.kind)
                    .map(|t| t.kind)
                    .unwrap_or("resistor");
                self.change_tab(Tab::Schematic);
                self.tool = Tool::Place(kind.into());
                self.pending_source = Some(part.clone());
                self.palette = false;
                self.part_search.clear();
            }
            ui.label(egui::RichText::new(&part.source).small().strong())
                .on_hover_text(format!("This result came from {}.", part.source));
        });
        let facts = format!("{} · {}", part.manufacturer, part.description);
        ui.label(egui::RichText::new(facts).small().color(self.muted()));
        ui.label(egui::RichText::new(stock_line(part)).small());
        if !part.lifecycle.is_empty() {
            ui.label(
                egui::RichText::new(format!("Status: {}", part.lifecycle))
                    .small()
                    .color(self.muted()),
            );
        }
        if !credit.is_empty() {
            ui.label(egui::RichText::new(credit).size(9.0).color(self.muted()));
        }
    }
    /// After a source part is placed, write its maker and numbers onto the
    /// new component in one step.
    pub(super) fn attach_pending_source(&mut self, component: Id) {
        let Some(part) = self.pending_source.take() else {
            return;
        };
        if !matches!(self.tool, Tool::Place(_)) {
            return;
        }
        self.run(
            "part.attach_source",
            json!({"id":component,"manufacturer":part.manufacturer,"mpn":part.mpn,
                   "datasheet":part.datasheet_url,"distributor":part.distributor,"sku":part.sku}),
        );
    }
}
