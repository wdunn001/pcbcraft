//! The cards of the Files page: Parts, Nets, Board shape, Checks, Shape of each layer and Next.
//!
//! Context: the approved mockup `files`. Every card has its own link button that goes to the
//! tab where the card belongs. The Board shape card draws the cut path with its cut-outs and
//! mounting holes and prints the real corner count and the read only reach. The board is a
//! free-form 2D shape; a rectangle is only a template and has no width or height field here.
//! Local rules: each layer row draws the shape that layer follows. Until the model gives every
//! copper layer its own shape (`Board::layer_shapes_in_order`) every layer follows the board.
//! Only copper layers have a shape; silkscreen, mask, paste, pads, tracks and zones follow
//! their copper layer.
//!
//! # Dependencies
//! - crates/ui-egui/src/shape_preview.rs (`paint_shape`, `view_of`, `extent_size`)
//! - crates/ui-egui/src/ui_kit/card.rs, button.rs, rows.rs
//! - crates/ui-egui/src/files_home.rs (state, commands)
//!
//! # Do not
//! - Add a width or height field or print the board as "W x H" outside the Reaches line.
//! - Draw a button by hand; use the kit.
use super::*;
use crate::shape_preview::{self, ShapeLook, ShapeView};
use crate::ui_kit::card::card;

/// Height of every card of the top row in points.
const CARD_HEIGHT: f32 = 216.0;
/// Height of the link strip at the foot of a card in points.
const LINK_STRIP: f32 = 40.0;
/// Width and height of the Board shape picture in points.
const BOARD_PICTURE: egui::Vec2 = egui::vec2(176.0, 148.0);
/// Width and height of one layer picture in points.
const LAYER_PICTURE: egui::Vec2 = egui::vec2(120.0, 100.0);
/// How many parts or net names a card lists before "+N more".
const NAMES: usize = 4;
/// Narrowest centre that still puts the four cards in one row.
const CARDS_ONE_ROW_FROM: f32 = 900.0;
/// Narrowest centre that still puts two cards side by side.
const CARDS_TWO_ROWS_FROM: f32 = 560.0;
/// Narrowest card that still puts the Board shape picture beside its words.
const PICTURE_BESIDE_FROM: f32 = 330.0;
/// Narrowest Next card that still puts a step's button beside its words.
const STEP_BUTTON_BESIDE_FROM: f32 = 440.0;
/// Narrowest centre that still puts Next beside Recent projects.
const TWO_COLUMNS_FROM: f32 = 640.0;

/// What one copper layer follows, and what it is called.
struct LayerRow {
    name: String,
    colour: egui::Color32,
    own: bool,
}

/// Up to [`NAMES`] names joined with commas, then "+N more".
fn name_list(names: &[String]) -> String {
    if names.is_empty() {
        return t!("files.stat.none");
    }
    let shown = names
        .iter()
        .take(NAMES)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > NAMES {
        format!(
            "{shown} {}",
            t!("files.stat.more", count = names.len() - NAMES)
        )
    } else {
        shown
    }
}

/// A reach number: whole numbers without decimals, others to one place.
pub(super) fn reach_text(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

impl App {
    /// The words for a shape: its name and its counts, one line each.
    pub(super) fn shape_words(&self, view: &ShapeView) -> (String, String) {
        let name = if shape_preview::is_plain_rectangle(view) {
            t!("files.shape.rectangle")
        } else {
            t!("files.shape.free_form")
        };
        let detail = t!(
            "files.shape.detail",
            corners = self
                .catalog
                .plural("files.shape.corners", view.corners as u64),
            cutouts = self
                .catalog
                .plural("files.shape.cutouts", view.cutouts.len() as u64),
            holes = self
                .catalog
                .plural("files.shape.holes", view.holes.len() as u64)
        );
        (name, detail)
    }

    /// The copper layers in stackup order and what each follows.
    ///
    /// Why: the shape is per copper layer; the model will say which layers have their own.
    fn layer_rows(&self, ui: &egui::Ui) -> Vec<LayerRow> {
        let tokens = theme::tokens(ui.ctx());
        let count = usize::from(self.project.copper_layer_count());
        (0..count)
            .map(|i| {
                let (name, colour) = if i == 0 {
                    (t!("files.layer.top"), tokens.copper)
                } else if i + 1 == count {
                    (t!("files.layer.bottom"), tokens.cyan)
                } else {
                    (t!("files.layer.inner", number = i), tokens.gold)
                };
                LayerRow {
                    name,
                    colour,
                    own: false,
                }
            })
            .collect()
    }

    /// One card with a body and a link strip at its foot; true when the link was pressed.
    fn link_card(
        &self,
        ui: &mut egui::Ui,
        width: f32,
        label: &str,
        link: (icons::Icon, String, String),
        body: impl FnOnce(&mut egui::Ui),
    ) -> bool {
        let tokens = theme::tokens(ui.ctx());
        let mut pressed = false;
        let inner = width - 2.0 * theme::BORDER;
        egui::Frame::NONE
            .fill(tokens.panel)
            .stroke(egui::Stroke::new(theme::BORDER, tokens.line))
            .corner_radius(egui::CornerRadius::same(theme::RADIUS_CARD))
            .show(ui, |ui| {
                ui.set_width(inner);
                ui.set_min_height(CARD_HEIGHT - 2.0 * theme::BORDER);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::same(theme::SPACE[3] as i8))
                        .show(ui, |ui| {
                            ui.set_width(inner - 2.0 * theme::SPACE[3]);
                            ui.set_min_height(
                                CARD_HEIGHT - LINK_STRIP - 2.0 * theme::BORDER - 32.0,
                            );
                            ui.add(
                                egui::Label::new(fonts::section_caption(label).color(tokens.muted))
                                    .selectable(false),
                            );
                            body(ui);
                        });
                    ui.painter().hline(
                        ui.max_rect().x_range(),
                        ui.cursor().min.y,
                        egui::Stroke::new(theme::BORDER, tokens.line),
                    );
                    ui.horizontal(|ui| {
                        ui.add_space(theme::SPACE[1]);
                        pressed = button::ghost_button(
                            ui,
                            Some(link.0),
                            &link.1,
                            &link.2,
                            Size::Normal,
                            true,
                        )
                        .clicked();
                    });
                });
            });
        pressed
    }

    /// Parts, Nets, Board shape and Checks, each with the link to where it belongs.
    pub(super) fn files_cards(&mut self, ui: &mut egui::Ui) {
        let gap = theme::SPACE[3];
        let total = ui.available_width();
        let weights: [f32; 4] = [1.0, 1.0, 1.7, 1.0];
        let rows: Vec<Vec<usize>> = if total >= CARDS_ONE_ROW_FROM {
            vec![vec![0, 1, 2, 3]]
        } else if total >= CARDS_TWO_ROWS_FROM {
            vec![vec![0, 1], vec![2, 3]]
        } else {
            vec![vec![0], vec![1], vec![2], vec![3]]
        };
        let tokens = theme::tokens(ui.ctx());
        let refs: Vec<String> = self
            .project
            .components
            .iter()
            .map(|c| c.reference.clone())
            .collect();
        let nets: Vec<String> = self.project.nets.iter().map(|n| n.name.clone()).collect();
        let view = shape_preview::view_of(&self.project.board);
        let (reach_w, reach_h) = shape_preview::extent_size(&self.project.board);
        let (errors, warnings) = self.findings().counts().strip();
        let (checks, checks_detail) = if self.checking() {
            ("…".to_owned(), t!("files.stat.checks.running"))
        } else if errors > 0 {
            (
                errors.to_string(),
                t!("files.stat.checks.errors", warnings = warnings),
            )
        } else if warnings > 0 {
            (warnings.to_string(), t!("files.stat.checks.review"))
        } else {
            ("0".to_owned(), t!("files.stat.checks.clear"))
        };
        let copper = self.project.copper_layer_count();
        let (shape_name, shape_detail) = self.shape_words(&view);
        let mut go: Option<usize> = None;
        for row in rows {
            let sum: f32 = row.iter().map(|i| weights[*i]).sum();
            let room = total - gap * (row.len() as f32 - 1.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for i in row {
                    let width = (room * weights[i] / sum).max(0.0);
                    let big = |ui: &mut egui::Ui, value: &str, detail: &str| {
                        ui.add(
                            egui::Label::new(fonts::display(value).color(tokens.text))
                                .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(fonts::caption(detail).color(tokens.muted))
                                .selectable(false)
                                .truncate(),
                        );
                    };
                    let pressed = match i {
                        0 => self.link_card(
                            ui,
                            width,
                            &t!("files.stat.parts"),
                            (
                                icons::CHIP,
                                t!("files.card.parts.link"),
                                t!("files.card.parts.link.tip"),
                            ),
                            |ui| big(ui, &refs.len().to_string(), &name_list(&refs)),
                        ),
                        1 => self.link_card(
                            ui,
                            width,
                            &t!("files.stat.nets"),
                            (
                                icons::PATH,
                                t!("files.card.nets.link"),
                                t!("files.card.nets.link.tip"),
                            ),
                            |ui| big(ui, &nets.len().to_string(), &name_list(&nets)),
                        ),
                        2 => self.link_card(
                            ui,
                            width,
                            &t!("files.card.shape"),
                            (
                                icons::PATH,
                                t!("files.shape.edit"),
                                t!("files.shape.edit.tip"),
                            ),
                            |ui| {
                                let beside = ui.available_width() >= PICTURE_BESIDE_FROM;
                                let layout = if beside {
                                    egui::Layout::left_to_right(egui::Align::Min)
                                } else {
                                    egui::Layout::top_down(egui::Align::Min)
                                };
                                ui.with_layout(layout, |ui| {
                                    ui.vertical(|ui| {
                                        let (rect, response) = ui.allocate_exact_size(
                                            BOARD_PICTURE,
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            rect,
                                            egui::CornerRadius::same(theme::RADIUS_CARD),
                                            tokens.ground,
                                        );
                                        shape_preview::paint_shape(
                                            ui.painter(),
                                            rect,
                                            &view,
                                            &ShapeLook {
                                                line: tokens.copper,
                                                detail: tokens.text,
                                            },
                                        );
                                        let name = format!("{shape_name}, {shape_detail}");
                                        response.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Label,
                                                true,
                                                &name,
                                            )
                                        });
                                        response.on_hover_text(t!("files.shape.picture.tip"));
                                        ui.add(
                                            egui::Label::new(
                                                fonts::caption(t!("files.shape.picture.caption"))
                                                    .color(tokens.muted),
                                            )
                                            .selectable(false),
                                        );
                                    });
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = theme::SPACE[1];
                                        ui.add(
                                            egui::Label::new(
                                                fonts::strong(&shape_name).color(tokens.text),
                                            )
                                            .selectable(false),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                fonts::caption(&shape_detail).color(tokens.muted),
                                            )
                                            .selectable(false)
                                            .wrap(),
                                        );
                                        let reach = t!(
                                            "files.shape.reaches",
                                            width = reach_text(reach_w),
                                            height = reach_text(reach_h)
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                fonts::caption(reach).color(tokens.muted),
                                            )
                                            .selectable(false),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                fonts::caption(t!(
                                                    "files.stat.board.layers",
                                                    count = copper
                                                ))
                                                .color(tokens.muted),
                                            )
                                            .selectable(false),
                                        );
                                    });
                                });
                            },
                        ),
                        _ => self.link_card(
                            ui,
                            width,
                            &t!("files.stat.checks"),
                            (
                                icons::WARNING,
                                t!("files.card.checks.link"),
                                t!("files.card.checks.link.tip"),
                            ),
                            |ui| big(ui, &checks, &checks_detail),
                        ),
                    };
                    if pressed {
                        go = Some(i);
                    }
                }
            });
            ui.add_space(gap);
        }
        match go {
            Some(0) => self.change_tab(Tab::Schematic),
            Some(1) => self.change_tab(Tab::Schematic),
            Some(2) => self.files_edit_shape(),
            Some(_) => self.change_tab(Tab::Checks),
            None => {}
        }
    }

    /// Go to Layout with the board shape selected, ready to edit.
    ///
    /// # Rules
    /// - A free-form outline selects its first edge; the Layers tree opens the Board edge
    ///   group so the edge is in view.
    /// - A board with no outline yet opens Layout with nothing selected.
    pub(crate) fn files_edit_shape(&mut self) {
        self.change_tab(Tab::Pcb);
        self.tree.expanded.insert("edge".to_owned());
        if let Some(points) = self.project.board.outline.as_deref() {
            self.outline_ui.pick(
                outline_arcs::OutlineHit::Segment(0),
                points,
                &self.project.board.outline_arcs,
            );
        }
    }

    /// The Shape of each layer row: one small drawing per copper layer.
    pub(super) fn files_layer_shapes(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let rows = self.layer_rows(ui);
        let view = shape_preview::view_of(&self.project.board);
        card(
            ui,
            &t!("files.layers.title"),
            Some(&t!("files.layers.caption")),
            |_| {},
            |ui| {
                let fit = ((ui.available_width() / (LAYER_PICTURE.x + theme::SPACE[3])) as usize)
                    .clamp(1, rows.len().max(1));
                let cell = ui.available_width() / fit as f32;
                for chunk in rows.chunks(fit) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for row in chunk {
                            ui.allocate_ui_with_layout(
                                egui::vec2(cell, 0.0),
                                egui::Layout::top_down(egui::Align::Center),
                                |ui| {
                                    ui.set_width(cell);
                                    ui.spacing_mut().item_spacing.y = theme::SPACE[1];
                                    let (rect, response) =
                                        ui.allocate_exact_size(LAYER_PICTURE, egui::Sense::hover());
                                    ui.painter().rect_filled(
                                        rect,
                                        egui::CornerRadius::same(theme::RADIUS_CARD),
                                        tokens.ground,
                                    );
                                    shape_preview::paint_shape(
                                        ui.painter(),
                                        rect,
                                        &view,
                                        &ShapeLook {
                                            line: row.colour,
                                            detail: tokens.muted,
                                        },
                                    );
                                    let status = if row.own {
                                        t!("files.layers.own")
                                    } else {
                                        t!("files.layers.follows")
                                    };
                                    let name = format!("{} {status}", row.name);
                                    response.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Label,
                                            true,
                                            &name,
                                        )
                                    });
                                    response.on_hover_text(t!("files.layers.picture.tip"));
                                    ui.add(
                                        egui::Label::new(
                                            fonts::strong(&row.name).color(tokens.text),
                                        )
                                        .selectable(false),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            fonts::caption(&status).color(tokens.muted),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                        }
                    });
                }
            },
        );
    }

    /// The Next card: fix errors, make files, save a copy to share.
    fn files_next_card(&mut self, ui: &mut egui::Ui) {
        let (errors, _) = self.findings().counts().strip();
        let mut go: Option<usize> = None;
        let steps: [(String, String, String, String, icons::Icon); 3] = [
            (
                if errors > 0 {
                    t!("files.next.fix", count = errors)
                } else {
                    t!("files.next.fix.none")
                },
                t!("files.next.fix.detail"),
                t!("files.card.checks.link"),
                t!("files.card.checks.link.tip"),
                icons::WARNING,
            ),
            (
                t!("files.next.make"),
                t!("files.next.make.detail"),
                t!("files.next.make.link"),
                t!("files.next.make.link.tip"),
                icons::UPLOAD,
            ),
            (
                t!("files.next.share"),
                t!("files.next.share.detail"),
                t!("files.tool.export"),
                t!("files.tool.export.tip"),
                icons::SAVE,
            ),
        ];
        crate::ui_kit::card::card_flush(
            ui,
            &t!("files.next.title"),
            None,
            |_| {},
            |ui| {
                let tokens = theme::tokens(ui.ctx());
                let beside = ui.available_width() >= STEP_BUTTON_BESIDE_FROM;
                for (i, (title, detail, link, tip, icon)) in steps.iter().enumerate() {
                    let words = |ui: &mut egui::Ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add(
                            egui::Label::new(fonts::strong(title).color(tokens.text))
                                .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(fonts::caption(detail).color(tokens.muted))
                                .selectable(false),
                        );
                    };
                    ui.horizontal_top(|ui| {
                        ui.add_space(theme::SPACE[3]);
                        let (rect, _) = ui.allocate_exact_size(
                            egui::Vec2::splat(icons::ICON_SIZE),
                            egui::Sense::hover(),
                        );
                        (icon.0)(ui.painter(), rect, tokens.muted);
                        ui.add_space(theme::SPACE[1]);
                        ui.vertical(|ui| {
                            if beside {
                                ui.horizontal(|ui| {
                                    ui.vertical(words);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add_space(theme::SPACE[3]);
                                            if button::ghost_button(
                                                ui,
                                                None,
                                                link,
                                                tip,
                                                Size::Normal,
                                                true,
                                            )
                                            .clicked()
                                            {
                                                go = Some(i);
                                            }
                                        },
                                    );
                                });
                            } else {
                                ui.vertical(words);
                                if button::ghost_button(ui, None, link, tip, Size::Normal, true)
                                    .clicked()
                                {
                                    go = Some(i);
                                }
                            }
                        });
                    });
                    ui.add_space(theme::SPACE[2]);
                }
            },
        );
        match go {
            Some(0) => self.change_tab(Tab::Checks),
            Some(1) => self.change_tab(Tab::PlotFiles),
            Some(_) => self.files_run(Command::Export),
            None => {}
        }
    }

    /// The home page: header, four cards, Shape of each layer, then Next beside Recent projects.
    ///
    /// # Rules
    /// - Below [`TWO_COLUMNS_FROM`] Next and Recent projects stack.
    pub(crate) fn files_home_page(&mut self, ui: &mut egui::Ui) {
        let gap = theme::SPACE[3];
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        if !self.files_home.error.is_empty() {
            let red = theme::tokens(ui.ctx()).red;
            ui.label(fonts::text(fonts::Role::Body, &self.files_home.error).color(red));
        }
        self.files_header(ui);
        self.files_cards(ui);
        self.files_layer_shapes(ui);
        let total = ui.available_width();
        if total < TWO_COLUMNS_FROM {
            self.files_next_card(ui);
            self.files_recent_card(ui);
        } else {
            let half = (total - gap) / 2.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.allocate_ui_with_layout(
                    egui::vec2(half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(half);
                        self.files_next_card(ui);
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(half);
                        self.files_recent_card(ui);
                    },
                );
            });
        }
        self.files_rectangle_dialog(ui.ctx());
    }
}
