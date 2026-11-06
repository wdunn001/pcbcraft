//! The Analyze pages that have no data source yet: structure and plain empty states.
//!
//! Context: issue #1421 and the simulation epic #1360. Scope, PDN impedance, Reflections and levels,
//! RF, Antenna, Firmware, Hardware link and IDE link are laid out as the approved mockups show, card
//! by card and Properties section by Properties section. Nothing in the design or the engine feeds
//! them yet, so each card says what is missing and where it will come from.
//!
//! Local rules: no waveform, curve, number, state or example value is drawn. A Properties row shows
//! its label and the words "Not set". A button with nothing behind it is disabled and its tooltip
//! says why. The one real button on a card opens the tab that will produce the data.
//!
//! # Dependencies
//! - crates/ui-egui/src/analyze/mod.rs (`Page`, `Entry`)
//! - crates/ui-egui/src/ui_kit/card.rs (`card`)
//! - crates/ui-egui/src/ui_kit/empty_state.rs (`empty_state`)
//! - crates/ui-egui/src/ui_kit/form_row.rs (`form_row`)
//!
//! # Do not
//! - Add a value, a sample or a status line; add the data source first.
use super::*;
use crate::ui_kit::{
    card::card,
    empty_state::empty_state,
    form_row::{FormWidth, form_row},
    icons::{self, Icon},
};

/// The tab a card's one real button opens.
#[derive(Clone, Copy)]
enum Go {
    /// The Schematic tab.
    Schematic,
    /// The Layout tab.
    Layout,
    /// The Rules page.
    Rules,
}

impl Go {
    /// Catalog key of the button label; its tooltip is the same key with `.tip`.
    fn key(self) -> &'static str {
        match self {
            Go::Schematic => "analyze.go.schematic",
            Go::Layout => "analyze.go.layout",
            Go::Rules => "analyze.go.rules",
        }
    }
}

/// One card of a page: its words and the empty state in its body.
struct CardSpec {
    /// Catalog key of the card title.
    title: &'static str,
    /// Catalog key of the muted caption beside the title.
    caption: &'static str,
    /// Catalog key of the empty state title.
    empty: &'static str,
    /// Catalog key of the one sentence that says what is missing.
    message: &'static str,
    /// Icon in the empty state.
    icon: Icon,
    /// The tab that will produce the data, when there is one to open.
    go: Option<Go>,
}

/// One Properties section: a caption, rows that read Not set, and disabled buttons.
struct SectionSpec {
    /// Catalog key of the section caption.
    title: &'static str,
    /// Catalog keys of the row labels.
    rows: &'static [&'static str],
    /// Catalog keys of the disabled buttons; each tooltip is the same key with `.tip`.
    buttons: &'static [&'static str],
}

/// A page's cards and Properties sections.
struct PageSpec {
    /// The cards in order.
    cards: &'static [CardSpec],
    /// The Properties sections in order.
    sections: &'static [SectionSpec],
}

const SCOPE: PageSpec = PageSpec {
    cards: &[
        CardSpec {
            title: "analyze.scope.plot",
            caption: "analyze.scope.plot.caption",
            empty: "analyze.scope.plot.empty",
            message: "analyze.scope.plot.message",
            icon: icons::PATH,
            go: Some(Go::Schematic),
        },
        CardSpec {
            title: "analyze.scope.lanes",
            caption: "analyze.scope.lanes.caption",
            empty: "analyze.scope.lanes.empty",
            message: "analyze.scope.lanes.message",
            icon: icons::CHIP,
            go: None,
        },
        CardSpec {
            title: "analyze.scope.measure",
            caption: "analyze.scope.measure.caption",
            empty: "analyze.scope.measure.empty",
            message: "analyze.scope.measure.message",
            icon: icons::BOLT,
            go: None,
        },
    ],
    sections: &[
        SectionSpec {
            title: "analyze.scope.probes",
            rows: &[
                "analyze.row.ch1",
                "analyze.row.ch2",
                "analyze.row.ch3",
                "analyze.row.ch4",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.scope.channel",
            rows: &[
                "analyze.row.volts_div",
                "analyze.row.offset",
                "analyze.row.coupling",
                "analyze.row.probe_ratio",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.scope.timebase",
            rows: &["analyze.row.time_div", "analyze.row.position"],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.scope.trigger",
            rows: &[
                "analyze.row.source",
                "analyze.row.slope",
                "analyze.row.level",
                "analyze.row.mode",
                "analyze.row.holdoff",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.scope.decoders",
            rows: &[
                "analyze.row.uart",
                "analyze.row.spi",
                "analyze.row.i2c",
                "analyze.row.can",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.scope.display",
            rows: &[
                "analyze.row.persistence",
                "analyze.row.math",
                "analyze.row.fft",
            ],
            buttons: &[],
        },
    ],
};

const PDN: PageSpec = PageSpec {
    cards: &[
        CardSpec {
            title: "analyze.pdn.curve",
            caption: "analyze.pdn.curve.caption",
            empty: "analyze.pdn.curve.empty",
            message: "analyze.pdn.curve.message",
            icon: icons::BOLT,
            go: Some(Go::Schematic),
        },
        CardSpec {
            title: "analyze.pdn.step",
            caption: "analyze.pdn.step.caption",
            empty: "analyze.pdn.step.empty",
            message: "analyze.pdn.step.message",
            icon: icons::PATH,
            go: None,
        },
    ],
    sections: &[
        SectionSpec {
            title: "analyze.pdn.rail",
            rows: &[
                "analyze.row.voltage",
                "analyze.row.step_current",
                "analyze.row.ripple",
                "analyze.row.target",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.pdn.caps",
            rows: &["analyze.row.capacitors"],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.pdn.model",
            rows: &["analyze.row.esr_esl", "analyze.row.regulator"],
            buttons: &[],
        },
    ],
};

const REFLECTIONS: PageSpec = PageSpec {
    cards: &[
        CardSpec {
            title: "analyze.refl.line",
            caption: "analyze.refl.line.caption",
            empty: "analyze.refl.line.empty",
            message: "analyze.refl.line.message",
            icon: icons::PATH,
            go: Some(Go::Layout),
        },
        CardSpec {
            title: "analyze.refl.margin",
            caption: "analyze.refl.margin.caption",
            empty: "analyze.refl.margin.empty",
            message: "analyze.refl.margin.message",
            icon: icons::CHECK,
            go: None,
        },
    ],
    sections: &[
        SectionSpec {
            title: "analyze.refl.finding",
            rows: &["analyze.row.state", "analyze.row.where", "analyze.row.fix"],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.refl.line_section",
            rows: &[
                "analyze.row.z0",
                "analyze.row.rout",
                "analyze.row.load",
                "analyze.row.length",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.refl.thresholds",
            rows: &[
                "analyze.row.family",
                "analyze.row.vil",
                "analyze.row.vih",
                "analyze.row.input_max",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.refl.rule",
            rows: &["analyze.row.severity", "analyze.row.overshoot"],
            buttons: &[],
        },
    ],
};

const RF: PageSpec = PageSpec {
    cards: &[
        CardSpec {
            title: "analyze.rf.spectrum",
            caption: "analyze.rf.spectrum.caption",
            empty: "analyze.rf.spectrum.empty",
            message: "analyze.rf.spectrum.message",
            icon: icons::BOLT,
            go: None,
        },
        CardSpec {
            title: "analyze.rf.smith",
            caption: "analyze.rf.smith.caption",
            empty: "analyze.rf.smith.empty",
            message: "analyze.rf.smith.message",
            icon: icons::PATH,
            go: None,
        },
    ],
    sections: &[
        SectionSpec {
            title: "analyze.rf.spectrum_section",
            rows: &["analyze.row.center", "analyze.row.span", "analyze.row.rbw"],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.rf.budget",
            rows: &[
                "analyze.row.frequency",
                "analyze.row.distance",
                "analyze.row.tx_power",
                "analyze.row.gains",
                "analyze.row.noise_figure",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.rf.result",
            rows: &[
                "analyze.row.path_loss",
                "analyze.row.received",
                "analyze.row.noise_floor",
                "analyze.row.margin",
            ],
            buttons: &[],
        },
    ],
};

const ANTENNA: PageSpec = PageSpec {
    cards: &[CardSpec {
        title: "analyze.antenna.pattern",
        caption: "analyze.antenna.pattern.caption",
        empty: "analyze.antenna.pattern.empty",
        message: "analyze.antenna.pattern.message",
        icon: icons::PATH,
        go: None,
    }],
    sections: &[
        SectionSpec {
            title: "analyze.antenna.antenna",
            rows: &[
                "analyze.row.type",
                "analyze.row.frequency",
                "analyze.row.plane",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.antenna.result",
            rows: &[
                "analyze.row.peak",
                "analyze.row.beamwidth",
                "analyze.row.directivity",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.antenna.array",
            rows: &[
                "analyze.row.elements",
                "analyze.row.spacing",
                "analyze.row.phase_step",
            ],
            buttons: &[],
        },
    ],
};

const FIRMWARE: PageSpec = PageSpec {
    cards: &[
        CardSpec {
            title: "analyze.fw.signals",
            caption: "analyze.fw.signals.caption",
            empty: "analyze.fw.signals.empty",
            message: "analyze.fw.signals.message",
            icon: icons::PATH,
            go: Some(Go::Schematic),
        },
        CardSpec {
            title: "analyze.fw.console",
            caption: "analyze.fw.console.caption",
            empty: "analyze.fw.console.empty",
            message: "analyze.fw.console.message",
            icon: icons::FILE,
            go: None,
        },
    ],
    sections: &[
        SectionSpec {
            title: "analyze.fw.image",
            rows: &["analyze.row.file", "analyze.row.size", "analyze.row.kind"],
            buttons: &["analyze.button.choose_image"],
        },
        SectionSpec {
            title: "analyze.fw.target",
            rows: &["analyze.row.part", "analyze.row.core", "analyze.row.entry"],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.fw.run",
            rows: &[
                "analyze.row.time_to_run",
                "analyze.row.step_budget",
                "analyze.row.used",
            ],
            buttons: &[],
        },
    ],
};

const HITL: PageSpec = PageSpec {
    cards: &[CardSpec {
        title: "analyze.hitl.compare",
        caption: "analyze.hitl.compare.caption",
        empty: "analyze.hitl.compare.empty",
        message: "analyze.hitl.compare.message",
        icon: icons::PATH,
        go: None,
    }],
    sections: &[
        SectionSpec {
            title: "analyze.hitl.connection",
            rows: &[
                "analyze.row.port",
                "analyze.row.baud",
                "analyze.row.framing",
                "analyze.row.status",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.hitl.counters",
            rows: &["analyze.row.bytes_in", "analyze.row.bytes_out"],
            buttons: &["analyze.button.connect", "analyze.button.disconnect"],
        },
    ],
};

const IDE_LINK: PageSpec = PageSpec {
    cards: &[
        CardSpec {
            title: "analyze.ide.how",
            caption: "analyze.ide.how.caption",
            empty: "analyze.ide.how.empty",
            message: "analyze.ide.how.message",
            icon: icons::FILE,
            go: None,
        },
        CardSpec {
            title: "analyze.ide.states",
            caption: "analyze.ide.states.caption",
            empty: "analyze.ide.states.empty",
            message: "analyze.ide.states.message",
            icon: icons::LAYERS,
            go: None,
        },
    ],
    sections: &[
        SectionSpec {
            title: "analyze.ide.link",
            rows: &[
                "analyze.row.state_link",
                "analyze.row.peer",
                "analyze.row.token",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.ide.last",
            rows: &[
                "analyze.row.name",
                "analyze.row.size",
                "analyze.row.hash",
                "analyze.row.pushed",
            ],
            buttons: &[],
        },
        SectionSpec {
            title: "analyze.ide.folder",
            rows: &["analyze.row.watching"],
            buttons: &["analyze.button.run_pushed", "analyze.button.disconnect"],
        },
    ],
};

/// The plain page for an entry the list names that has no page yet.
const PLAIN: PageSpec = PageSpec {
    cards: &[],
    sections: &[],
};

/// The spec of a page.
fn spec(page: Page) -> &'static PageSpec {
    match page {
        Page::Scope => &SCOPE,
        Page::Pdn => &PDN,
        Page::Reflections => &REFLECTIONS,
        Page::Rf => &RF,
        Page::Antenna => &ANTENNA,
        Page::Firmware => &FIRMWARE,
        Page::Hitl => &HITL,
        Page::IdeLink => &IDE_LINK,
        Page::Checks | Page::NetClasses | Page::Pending => &PLAIN,
    }
}

/// Look a catalog key up.
fn words(key: &str) -> String {
    pcbcraft_text::lookup(key, &[])
}

impl App {
    /// The cards of a page, each with an empty state.
    pub(super) fn analyze_pending_center(&mut self, ui: &mut egui::Ui, page: Page) {
        if page == Page::Pending {
            let entry = self.analyze.entry;
            let rules = entry == Entry::RuleSets;
            let title = entry.words().0;
            let mut go = false;
            card(
                ui,
                &title,
                None,
                |_| {},
                |ui| {
                    ui.add_space(8.0);
                    if rules {
                        go = empty_state(
                            ui,
                            icons::GEAR,
                            &t!("analyze.pending.empty"),
                            &t!("analyze.pending.rules_message"),
                            &words(Go::Rules.key()),
                            &words(&format!("{}.tip", Go::Rules.key())),
                        )
                        .clicked();
                    } else {
                        plain_empty(
                            ui,
                            &t!("analyze.pending.empty"),
                            &t!("analyze.pending.message"),
                        );
                    }
                    ui.add_space(8.0);
                },
            );
            if go {
                self.change_tab(Tab::Rules);
            }
            return;
        }
        let mut target: Option<Go> = None;
        for card_spec in spec(page).cards {
            let title = words(card_spec.title);
            let caption = words(card_spec.caption);
            card(
                ui,
                &title,
                Some(&caption),
                |_| {},
                |ui| {
                    ui.add_space(8.0);
                    match card_spec.go {
                        Some(go) => {
                            if empty_state(
                                ui,
                                card_spec.icon,
                                &words(card_spec.empty),
                                &words(card_spec.message),
                                &words(go.key()),
                                &words(&format!("{}.tip", go.key())),
                            )
                            .clicked()
                            {
                                target = Some(go);
                            }
                        }
                        None => plain_empty(ui, &words(card_spec.empty), &words(card_spec.message)),
                    }
                    ui.add_space(8.0);
                },
            );
            ui.add_space(12.0);
        }
        match target {
            Some(Go::Schematic) => self.change_tab(Tab::Schematic),
            Some(Go::Layout) => self.change_tab(Tab::Pcb),
            Some(Go::Rules) => self.change_tab(Tab::Rules),
            None => {}
        }
    }

    /// The Properties sections of a page: labels with Not set, and disabled buttons.
    pub(super) fn analyze_pending_properties(&mut self, ui: &mut egui::Ui, page: Page) {
        let tokens = theme::tokens(ui.ctx());
        let sections = spec(page).sections;
        if sections.is_empty() {
            ui.label(
                fonts::text(fonts::Role::Body, t!("analyze.pending.props")).color(tokens.muted),
            );
            return;
        }
        ui.label(fonts::text(fonts::Role::Body, t!("analyze.props.unbacked")).color(tokens.muted));
        for section in sections {
            ui.label(fonts::section_caption(words(section.title)).color(tokens.muted));
            for row in section.rows {
                form_row(ui, FormWidth::Aside, &words(row), None, None, |ui| {
                    ui.label(
                        fonts::text(fonts::Role::Body, t!("analyze.not_set")).color(tokens.muted),
                    );
                });
            }
            if !section.buttons.is_empty() {
                ui.horizontal(|ui| {
                    for button in section.buttons {
                        secondary_button(
                            ui,
                            None,
                            &words(button),
                            &words(&format!("{button}.tip")),
                            Size::Normal,
                            false,
                        );
                    }
                });
            }
        }
    }
}

/// An empty state with no button: a title and the one sentence that says what is missing.
fn plain_empty(ui: &mut egui::Ui, title: &str, message: &str) {
    let tokens = theme::tokens(ui.ctx());
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.label(fonts::strong(title).color(tokens.text));
        ui.label(fonts::text(fonts::Role::Body, message).color(tokens.muted));
    });
}
