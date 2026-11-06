//! The Analyze workspace: a top-level tab for checks, signal and power analysis, instruments and rules.
//!
//! Context: issue #1421 (SIM-3-01) approves a sixth main tab named Analyze, placed after Layout; the
//! simulation epic is #1360. The left aside lists the analyses in groups (Checks, Signal integrity,
//! Power integrity, Protocols, Instruments, Rules), the centre shows the page of the picked entry,
//! and the right aside is that page's Properties. The ten pages are Scope, Checks, Net classes,
//! PDN impedance, Reflections and levels, RF, Antenna, Firmware, Hardware link and IDE link.
//!
//! Local rules: this module holds view state only and draws nothing it cannot back. The Checks and
//! Net classes pages read the design's own findings and net classes. Every other page draws its
//! structure with a plain empty state that says what is missing and where it will come from; no
//! waveform, number or status on those pages is invented. A control with nothing behind it is
//! disabled and its tooltip says why.
//!
//! # Dependencies
//! - crates/ui-egui/src/workspace.rs (`Workspace`, `AnalyzeWorkspace`)
//! - crates/ui-egui/src/finding_set.rs (`FindingSet`, `Row`, `Loudness`)
//! - crates/ui-egui/src/analyze/checks.rs (the Checks page)
//! - crates/ui-egui/src/analyze/net_classes.rs (the Net classes page)
//! - crates/ui-egui/src/analyze/pending.rs (the pages with no data source yet)
//! - crates/ui-egui/src/ui_kit/rows.rs (`list_row`)
//!
//! # Do not
//! - Draw sample data, example values or fake status on a page; say what is missing instead.
//! - Add a control that does nothing without disabling it and giving it a tooltip that says why.
use super::*;
use crate::fonts;
use crate::theme;
use crate::ui_kit::button::{Size, ghost_button, primary_button, secondary_button};

mod checks;
mod net_classes;
mod pending;

/// The ten pages of the tab, plus the one plain page shared by entries that have no page yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    /// Oscilloscope view with channels, trigger, decoders and measurements.
    Scope,
    /// Findings of the design's checks.
    Checks,
    /// The design's net classes.
    NetClasses,
    /// Power delivery impedance and load step response.
    Pdn,
    /// Reflections and noise margin on one digital line.
    Reflections,
    /// Spectrum and a Smith chart.
    Rf,
    /// Radiation pattern, a future feature.
    Antenna,
    /// A firmware image run on a target part.
    Firmware,
    /// A hardware link compared with the simulation.
    Hitl,
    /// The link to a build tool.
    IdeLink,
    /// An entry the list names that has no page yet.
    Pending,
}

/// One row of the left aside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    /// Copper and board rule findings.
    DesignRules,
    /// Schematic and part-pairing findings.
    ElectricalChecks,
    /// Placement findings.
    Placement,
    /// Impedance targets per net class.
    ImpedanceTargets,
    /// Return path analysis.
    ReturnPath,
    /// Crosstalk analysis.
    Crosstalk,
    /// Reflections and levels.
    Reflections,
    /// Eye diagram.
    EyeDiagram,
    /// Power rails.
    Rails,
    /// Power delivery impedance.
    PdnImpedance,
    /// Load step response.
    LoadStep,
    /// UART decoder.
    Uart,
    /// SPI decoder.
    Spi,
    /// I2C decoder.
    I2c,
    /// CAN decoder.
    Can,
    /// The scope.
    Scope,
    /// Firmware run.
    Firmware,
    /// Hardware link.
    HardwareLink,
    /// IDE link.
    IdeLink,
    /// RF.
    Rf,
    /// Antenna.
    Antenna,
    /// Rule sets.
    RuleSets,
    /// Net classes.
    NetClasses,
}

/// A titled group of entries.
struct Group {
    /// Catalog key of the group title.
    title: &'static str,
    /// Entries in order.
    entries: &'static [Entry],
}

/// The left aside as the approved design lists it.
const GROUPS: [Group; 6] = [
    Group {
        title: "analyze.group.checks",
        entries: &[
            Entry::DesignRules,
            Entry::ElectricalChecks,
            Entry::Placement,
        ],
    },
    Group {
        title: "analyze.group.signal",
        entries: &[
            Entry::ImpedanceTargets,
            Entry::ReturnPath,
            Entry::Crosstalk,
            Entry::Reflections,
            Entry::EyeDiagram,
        ],
    },
    Group {
        title: "analyze.group.power",
        entries: &[Entry::Rails, Entry::PdnImpedance, Entry::LoadStep],
    },
    Group {
        title: "analyze.group.protocols",
        entries: &[Entry::Uart, Entry::Spi, Entry::I2c, Entry::Can],
    },
    Group {
        title: "analyze.group.instruments",
        entries: &[
            Entry::Scope,
            Entry::Firmware,
            Entry::HardwareLink,
            Entry::IdeLink,
            Entry::Rf,
            Entry::Antenna,
        ],
    },
    Group {
        title: "analyze.group.rules",
        entries: &[Entry::RuleSets, Entry::NetClasses],
    },
];

impl Entry {
    /// Catalog key of the row's label; its tooltip is the same key with `.tip`.
    fn key(self) -> &'static str {
        match self {
            Entry::DesignRules => "analyze.entry.design_rules",
            Entry::ElectricalChecks => "analyze.entry.electrical_checks",
            Entry::Placement => "analyze.entry.placement",
            Entry::ImpedanceTargets => "analyze.entry.impedance_targets",
            Entry::ReturnPath => "analyze.entry.return_path",
            Entry::Crosstalk => "analyze.entry.crosstalk",
            Entry::Reflections => "analyze.entry.reflections",
            Entry::EyeDiagram => "analyze.entry.eye_diagram",
            Entry::Rails => "analyze.entry.rails",
            Entry::PdnImpedance => "analyze.entry.pdn_impedance",
            Entry::LoadStep => "analyze.entry.load_step",
            Entry::Uart => "analyze.entry.uart",
            Entry::Spi => "analyze.entry.spi",
            Entry::I2c => "analyze.entry.i2c",
            Entry::Can => "analyze.entry.can",
            Entry::Scope => "analyze.entry.scope",
            Entry::Firmware => "analyze.entry.firmware",
            Entry::HardwareLink => "analyze.entry.hardware_link",
            Entry::IdeLink => "analyze.entry.ide_link",
            Entry::Rf => "analyze.entry.rf",
            Entry::Antenna => "analyze.entry.antenna",
            Entry::RuleSets => "analyze.entry.rule_sets",
            Entry::NetClasses => "analyze.entry.net_classes",
        }
    }

    /// The row's label and tooltip.
    fn words(self) -> (String, String) {
        (
            pcbcraft_text::lookup(self.key(), &[]),
            pcbcraft_text::lookup(&format!("{}.tip", self.key()), &[]),
        )
    }

    /// The entry a capture names by the last word of its catalog key, such as `design_rules`.
    ///
    /// Why: `--select analyze:<name>` shows one page in a `--capture` image.
    pub(crate) fn from_slug(slug: &str) -> Option<Entry> {
        GROUPS
            .iter()
            .flat_map(|g| g.entries.iter().copied())
            .find(|e| e.key().rsplit('.').next() == Some(slug))
    }

    /// The page this entry opens.
    pub(crate) fn page(self) -> Page {
        match self {
            Entry::DesignRules | Entry::ElectricalChecks => Page::Checks,
            Entry::NetClasses => Page::NetClasses,
            Entry::Reflections => Page::Reflections,
            Entry::PdnImpedance | Entry::LoadStep => Page::Pdn,
            Entry::Uart | Entry::Spi | Entry::I2c | Entry::Can | Entry::Scope => Page::Scope,
            Entry::Firmware => Page::Firmware,
            Entry::HardwareLink => Page::Hitl,
            Entry::IdeLink => Page::IdeLink,
            Entry::Rf => Page::Rf,
            Entry::Antenna => Page::Antenna,
            Entry::Placement
            | Entry::ImpedanceTargets
            | Entry::ReturnPath
            | Entry::Crosstalk
            | Entry::EyeDiagram
            | Entry::Rails
            | Entry::RuleSets => Page::Pending,
        }
    }
}

/// What the Analyze tab remembers while it is open.
#[derive(Clone, Debug)]
pub(crate) struct State {
    /// The picked row of the left aside.
    pub(crate) entry: Entry,
    /// Text in the filter box.
    pub(crate) filter: String,
    /// Index of the severity segment on the Checks page: all, errors, warnings, notes.
    pub(crate) severity: usize,
    /// The finding last shown on the sheet or board, read in Properties.
    pub(crate) shown: Option<finding_set::Row>,
    /// Index of the picked class on the Net classes page.
    pub(crate) class: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            entry: Entry::Scope,
            filter: String::new(),
            severity: 0,
            shown: None,
            class: 0,
        }
    }
}

/// How an entry's status dot is painted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dot {
    /// There are errors.
    Error,
    /// There are warnings or notes.
    Warning,
    /// The check ran and found nothing.
    Clear,
    /// Nothing backs the entry yet.
    Idle,
}

/// What the aside shows beside an entry: its count and dot, only when the design backs them.
struct Backing {
    /// The count at the right of the row.
    count: Option<usize>,
    /// The dot colour.
    dot: Dot,
}

impl App {
    /// Count findings of the entries that read the design's checks.
    ///
    /// # Rules
    /// - Design rules covers drawing and library findings; Electrical checks covers wiring and
    ///   part-pairing findings. Other entries have no findings of their own.
    fn analyze_backing(&self, set: &finding_set::FindingSet, entry: Entry) -> Backing {
        let idle = Backing {
            count: None,
            dot: Dot::Idle,
        };
        let keys: &[&str] = match entry {
            Entry::DesignRules => &["drc", "lib"],
            Entry::ElectricalChecks => &["erc", "peripheral"],
            Entry::NetClasses => {
                return Backing {
                    count: Some(self.project.net_classes.len()),
                    dot: Dot::Idle,
                };
            }
            _ => return idle,
        };
        let rows: Vec<&finding_set::Row> = set
            .visible(bottom_sheet::SeverityFilter::All)
            .filter(|r| keys.contains(&r.key))
            .collect();
        let dot = if rows
            .iter()
            .any(|r| r.loudness == finding_set::Loudness::Error)
        {
            Dot::Error
        } else if rows.is_empty() {
            Dot::Clear
        } else {
            Dot::Warning
        };
        Backing {
            count: Some(rows.len()),
            dot,
        }
    }

    /// The tool bar of the Analyze tab, one row that follows the showing page.
    ///
    /// # Rules
    /// - Only Run on the Checks pages does anything; every other button is disabled and its
    ///   tooltip names what is missing.
    pub(crate) fn analyze_toolbar(&mut self, ui: &mut egui::Ui) {
        let page = self.analyze.entry.page();
        let checks = page == Page::Checks;
        ui.horizontal_centered(|ui| {
            ui.add_space(Self::PAGE_GUTTER);
            ui.spacing_mut().item_spacing.x = 8.0;
            let sets: &[&[(&str, bool)]] = match page {
                Page::Scope => &[
                    &[("run", false), ("stop", false), ("single", false)],
                    &[("probe", false), ("cursors", false), ("fft", false)],
                    &[("export_csv", false), ("export_image", false)],
                ],
                Page::Firmware => &[&[("run", false), ("stop", false)], &[("export_csv", false)]],
                _ => &[
                    &[("run_selected", checks), ("stop", false)],
                    &[("export_csv", false)],
                ],
            };
            for (n, set) in sets.iter().enumerate() {
                if n > 0 {
                    ui.separator();
                }
                for (name, enabled) in *set {
                    let label = pcbcraft_text::lookup(&format!("analyze.tool.{name}"), &[]);
                    let tip_key = if *enabled {
                        format!("analyze.tool.{name}.tip")
                    } else if checks {
                        format!("analyze.tool.{name}.checks_tip")
                    } else {
                        format!("analyze.tool.{name}.off_tip")
                    };
                    let tip = pcbcraft_text::lookup(&tip_key, &[]);
                    if secondary_button(ui, None, &label, &tip, Size::Normal, *enabled).clicked()
                        && *name == "run_selected"
                    {
                        self.analyze_run_checks();
                    }
                }
            }
        });
    }

    /// Run the design's checks again, as the Findings tab does.
    fn analyze_run_checks(&mut self) {
        // Checks are recomputed from the session whenever its revision differs from ours.
        self.revision = self.revision.wrapping_add(1);
        self.message = t!("sheet.checks_ran");
    }

    /// The left aside: filter, Run all, Mute all and the groups of entries.
    pub(crate) fn analyze_left(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let set = self.findings();
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            let width = ui.available_width() - 28.0 - 8.0 - 8.0;
            let hint = t!("analyze.filter");
            let field = ui.add(
                crate::targets::text_field(&mut self.analyze.filter)
                    .hint_text(&hint)
                    .desired_width(width),
            );
            {
                use crate::a11y::Named;
                field.named(&hint).on_hover_text(t!("analyze.filter.tip"));
            }
            crate::ui_kit::button::icon_button(
                ui,
                crate::ui_kit::icons::PLUS,
                &t!("analyze.add"),
                &t!("analyze.add.tip"),
                Size::Normal,
                false,
            );
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            if primary_button(
                ui,
                None,
                &t!("analyze.run_all"),
                &t!("analyze.run_all.tip"),
                Size::Small,
                true,
            )
            .clicked()
            {
                self.analyze_run_checks();
            }
            ghost_button(
                ui,
                None,
                &t!("analyze.mute_all"),
                &t!("analyze.mute_all.tip"),
                Size::Small,
                false,
            );
        });
        ui.add_space(8.0);
        let needle = self.analyze.filter.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for group in &GROUPS {
                    let shown: Vec<Entry> = group
                        .entries
                        .iter()
                        .copied()
                        .filter(|e| {
                            needle.is_empty() || e.words().0.to_lowercase().contains(&needle)
                        })
                        .collect();
                    if shown.is_empty() {
                        continue;
                    }
                    let total: Option<usize> = {
                        let counts: Vec<usize> = group
                            .entries
                            .iter()
                            .filter_map(|e| self.analyze_backing(&set, *e).count)
                            .collect();
                        if counts.is_empty() {
                            None
                        } else {
                            Some(counts.iter().sum())
                        }
                    };
                    ui.add_space(theme::SPACE[1]);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            fonts::section_caption(pcbcraft_text::lookup(group.title, &[]))
                                .color(tokens.muted),
                        );
                        if let Some(total) = total {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_space(12.0);
                                    ui.label(fonts::mono(total.to_string()).color(tokens.muted));
                                },
                            );
                        }
                    });
                    for entry in shown {
                        self.analyze_entry_row(ui, &set, entry);
                    }
                }
            });
    }

    /// One row of the aside: a dot, the label and a count when the design backs one.
    fn analyze_entry_row(
        &mut self,
        ui: &mut egui::Ui,
        set: &finding_set::FindingSet,
        entry: Entry,
    ) {
        let tokens = theme::tokens(ui.ctx());
        let (label, tip) = entry.words();
        let backing = self.analyze_backing(set, entry);
        let name = match backing.count {
            Some(n) => t!("analyze.entry.name_count", name = label, count = n),
            None => label.clone(),
        };
        let selected = self.analyze.entry == entry;
        let response = crate::ui_kit::rows::list_row_high(ui, 28.0, selected, &name, &tip, |ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            let colour = match backing.dot {
                Dot::Error => tokens.red,
                Dot::Warning => tokens.gold,
                Dot::Clear => tokens.mask,
                Dot::Idle => tokens.muted,
            };
            ui.painter().circle_filled(dot.center(), 4.0, colour);
            ui.add_space(8.0);
            // Unselectable text, so a press on the words reaches the row under them.
            ui.add(
                egui::Label::new(fonts::text(fonts::Role::Body, &label).color(tokens.text))
                    .selectable(false),
            );
            if let Some(n) = backing.count {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(fonts::mono(n.to_string()).color(tokens.muted))
                            .selectable(false),
                    );
                });
            }
        });
        if response.clicked() {
            self.analyze.entry = entry;
        }
    }

    /// The centre of the Analyze tab: the page of the picked entry.
    pub(crate) fn analyze_center(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(28, 20))
                    .show(ui, |ui| match self.analyze.entry.page() {
                        Page::Checks => self.analyze_checks_center(ui),
                        Page::NetClasses => self.analyze_classes_center(ui),
                        page => self.analyze_pending_center(ui, page),
                    });
            });
    }

    /// The right aside of the Analyze tab: Properties of the showing page.
    pub(crate) fn analyze_properties(&mut self, ui: &mut egui::Ui) {
        egui::Frame::NONE
            .inner_margin(theme::aside_margin())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme::ASIDE_ROW_GAP;
                match self.analyze.entry.page() {
                    Page::Checks => self.analyze_checks_properties(ui),
                    Page::NetClasses => self.analyze_classes_properties(ui),
                    page => self.analyze_pending_properties(ui, page),
                }
            });
    }
}
