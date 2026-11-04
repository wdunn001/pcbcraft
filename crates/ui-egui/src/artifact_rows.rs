//! Artifact rows: the files a design makes, shown in the Layers tree under the
//! layers they come from, listed in Manufacture, and described in Properties.
//!
//! Context: an artifact record (`pcbcraft_doc::Artifact`) names one output file.
//! This file decides where its rows sit (`homes`), whether the file is out of
//! date (`TreeView::stale_ids`), how strongly a row lights for the one
//! selection value (`level_for`), and draws the Manufacture list and the
//! Properties card. A drill artifact is not a record; it comes from the drill
//! table and keeps its `drill:<from>-<to>:<micrometres>` key.
//! Local rules: a row key is `<parent path>/artifact:<kind>:<id>`; the text
//! after `artifact:` is the selection value, so every row of one artifact is
//! one thing. The logic here is pure except the `App` methods at the end.
//!
//! # Dependencies
//! - crates/ui-egui/src/selection.rs (`Selection`, `Level`, `DrillKey`)
//! - crates/ui-egui/src/layers_tree.rs (`Row`, `Env`, `TreeView`)
//! - crates/engine/src/artifacts.rs (`artifact_is_stale`, the stale test)
//! - crates/io/src/drill_table.rs (`drill_table` rows for drill artifacts)
//!
//! # Do not
//! - Generate or write a file from a pure function here; only the button in
//!   Properties runs `artifact.export`.
use super::*;
use crate::a11y::Named;
use crate::layers_tree::{Env, Row, RowKind, TreeView, row};
use crate::selection::{DrillKey, Level, Selection};
use crate::ui_kit::form_row::{FormWidth, form_row};
use pcbcraft_doc::{Artifact, Project};
use std::collections::BTreeSet;
use std::rc::Rc;

/// The out of date set with the revision and records it was worked out for.
pub(crate) type StaleCache = (u64, u64, Rc<BTreeSet<Id>>);

/// The text after `artifact:` that names a record: its kind, then its id.
///
/// # Rules
/// - Never starts with `drill:`, which is reserved for drill artifacts.
///
/// # Do not
/// - Parse this by hand; use [`record`].
pub(crate) fn rest_of(a: &Artifact) -> String {
    format!("{}:{}", a.kind, a.id)
}

/// The record a selection value names, when the design still holds it.
///
/// # Rules
/// - A drill artifact text (`drill:1-2:300`) is not a record and gives None.
pub(crate) fn record<'a>(project: &'a Project, rest: &str) -> Option<&'a Artifact> {
    let (kind, id) = rest.split_once(':')?;
    let id: Id = id.parse().ok()?;
    project
        .artifacts
        .iter()
        .find(|a| a.id == id && a.kind == kind)
}

fn key_at(i: usize, total: usize) -> String {
    if i == 0 {
        "side:top".to_owned()
    } else if i + 1 >= total {
        "side:bottom".to_owned()
    } else {
        format!("inner:{i}")
    }
}

/// The tree key of the copper layer a layer name stands for (`F.Cu`, `B.Cu`,
/// `In2.Cu`, a stackup name, or a mask or silk name of a side).
fn layer_key_for(project: &Project, name: &str) -> Option<String> {
    let coppers: Vec<_> = project
        .board
        .layer_records
        .iter()
        .filter(|r| r.kind == "copper")
        .collect();
    let total = coppers.len().max(2);
    if let Some(i) = coppers.iter().position(|r| r.name == name) {
        return Some(key_at(i, total));
    }
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("f.") {
        return Some("side:top".to_owned());
    }
    if lower.starts_with("b.") {
        return Some("side:bottom".to_owned());
    }
    let n: usize = lower
        .strip_prefix("in")?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()?;
    (n >= 1 && n + 1 < total).then(|| format!("inner:{n}"))
}

fn sublayer_homes(
    project: &Project,
    names: &[String],
    needle: &str,
    sub: &str,
    default: &[&str],
) -> Vec<String> {
    let matching: Vec<&String> = names
        .iter()
        .filter(|n| n.to_ascii_lowercase().contains(needle))
        .collect();
    let chosen: Vec<&String> = if matching.is_empty() {
        names.iter().collect()
    } else {
        matching
    };
    let mut keys: Vec<String> = if chosen.is_empty() {
        default.iter().map(|k| (*k).to_owned()).collect()
    } else {
        chosen
            .iter()
            .filter_map(|n| layer_key_for(project, n))
            .collect()
    };
    keys.dedup();
    let mut out = Vec::new();
    for k in keys {
        if k.starts_with("inner:") && sub != "copper" {
            continue;
        }
        let home = format!("{k}/{sub}");
        if !out.contains(&home) {
            out.push(home);
        }
    }
    if out.is_empty() {
        out.push("outputs".to_owned());
    }
    out
}

fn placement_homes(a: &Artifact) -> Vec<String> {
    let side = a.settings.get("side").and_then(|v| v.as_str());
    let keys: &[&str] = match side {
        Some("top") => &["side:top"],
        Some("bottom") => &["side:bottom"],
        _ => &["side:top", "side:bottom"],
    };
    keys.iter().map(|k| format!("{k}/components")).collect()
}

/// The tree rows an artifact record is listed under, as parent row keys.
///
/// Why: a copper plot belongs under the copper of the layer it plots, a
/// placement file under the parts of its side, a drill file under Drills.
///
/// # Rules
/// - Copper, mask, silk and paste plots go under that sublayer of each layer
///   the record names; an empty list means the kind's usual layers.
/// - Placement files go under Pads of the sides their settings name.
/// - Drill files go under `drills`, outline files under `edge`.
/// - Anything else, or a layer the board does not have, goes under `outputs`.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Artifact`, `Project`)
///
/// # Do not
/// - Return the same parent twice.
pub(crate) fn homes(project: &Project, a: &Artifact) -> Vec<String> {
    let both = ["side:top", "side:bottom"];
    match a.kind.as_str() {
        "copper_plot" => sublayer_homes(project, &a.layers, "cu", "copper", &["side:top"]),
        "isolation_gcode" => sublayer_homes(project, &a.layers, "cu", "copper", &both),
        "mask_plot" => sublayer_homes(project, &a.layers, "mask", "mask", &both),
        "silk_plot" => sublayer_homes(project, &a.layers, "silk", "silk", &both),
        "paste_plot" => sublayer_homes(project, &a.layers, "paste", "paste", &both),
        "placement" | "placement_job" | "placement_yy1" => placement_homes(a),
        "drill_plated" | "drill_unplated" | "drill_map" | "drill_gcode" => {
            vec!["drills".to_owned()]
        }
        "profile_plot" | "outline_gcode" => vec!["edge".to_owned()],
        _ => vec!["outputs".to_owned()],
    }
}

/// What a selected artifact record touches on the board.
pub(crate) struct Reach {
    /// Copper layer indexes of the layers it comes from.
    pub layers: Option<BTreeSet<u8>>,
    /// Parts it lists, when it is a placement file.
    pub objects: BTreeSet<Id>,
}

/// The layers and parts a record reaches, for the weak highlight.
///
/// # Rules
/// - None when `rest` is not a record of this design.
/// - `layers` is None for a record that sits on no layer (a drill file, a bill
///   of materials), so no layer row dims.
///
/// # Dependencies
/// - crates/ui-egui/src/selection.rs (`relations` calls this)
///
/// # Do not
/// - Store the answer; ask again after an edit.
pub(crate) fn reach(project: &Project, rest: &str) -> Option<Reach> {
    let a = record(project, rest)?;
    let mut layers = BTreeSet::new();
    let mut objects = BTreeSet::new();
    for home in homes(project, a) {
        if let Some(i) = Selection::layer_index(project, &home) {
            layers.insert(i);
        }
        if home.ends_with("/components") {
            let top = home.starts_with("side:top");
            for f in &project.board.footprints {
                if f.side.is_none_or(|l| l == pcbcraft_doc::Layer::Front) == top {
                    objects.insert(f.id);
                }
            }
        }
    }
    Some(Reach {
        layers: (!layers.is_empty()).then_some(layers),
        objects,
    })
}

/// How strongly a record lights for `selection`, as listed under `home`.
///
/// # Rules
/// - Strong when this very record is selected, wherever its row sits.
/// - Weak when a layer or sublayer it belongs to is selected.
///
/// # Dependencies
/// - crates/ui-egui/src/selection.rs (`Selection`, `Level`)
///
/// # Do not
/// - Light a record for a layer it does not belong to.
pub(crate) fn level_for(
    project: &Project,
    selection: &Selection,
    a: &Artifact,
    home: Option<&str>,
) -> Level {
    match selection {
        Selection::Artifact(rest) if *rest == rest_of(a) => Level::Strong,
        Selection::Layer(k) | Selection::Sublayer(k) => {
            let mine = match home {
                Some(h) => vec![h.to_owned()],
                None => homes(project, a),
            };
            if mine
                .iter()
                .any(|h| h == k || h.split('/').next() == Some(k.as_str()))
            {
                Level::Weak
            } else {
                Level::None
            }
        }
        _ => Level::None,
    }
}

fn artifact_of_key(key: &str) -> Option<&str> {
    key.rsplit('/').next()?.strip_prefix("artifact:")
}

fn parent_of(key: &str) -> &str {
    key.rsplit_once('/').map_or("", |(p, _)| p)
}

fn row_level(env: &Env, r: &Row) -> Level {
    let (Some(rest), Some(sel)) = (artifact_of_key(&r.key), env.subject) else {
        return Level::None;
    };
    if let Some(d) = DrillKey::parse(rest) {
        let rel = crate::selection::relations(env.project, sel);
        return crate::selection::drill_level(env.project, sel, &rel, d);
    }
    match record(env.project, rest) {
        Some(a) => level_for(env.project, sel, a, Some(parent_of(&r.key))),
        None => Level::None,
    }
}

/// True when this tree row is an artifact row of the selected artifact.
///
/// # Rules
/// - Every row of the selected artifact is selected, not only the clicked one.
///
/// # Do not
/// - Use for rows that are not artifact rows; they answer false.
pub(crate) fn row_is_selected(env: &Env, r: &Row) -> bool {
    row_level(env, r) == Level::Strong
}

/// True when this tree row is related to the selected artifact: a layer or
/// sublayer row it comes from, or an artifact row of the selected layer.
///
/// # Rules
/// - A strong row answers false here; the caller draws it as selected.
pub(crate) fn row_is_lit_weakly(env: &Env, r: &Row) -> bool {
    if artifact_of_key(&r.key).is_some() {
        return row_level(env, r) == Level::Weak;
    }
    let Some(Selection::Artifact(rest)) = env.subject else {
        return false;
    };
    record(env.project, rest).is_some_and(|a| homes(env.project, a).contains(&r.key))
        || (DrillKey::parse(rest).is_some() && r.key == "drills")
}

fn stale_fingerprint(project: &Project) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for a in &project.artifacts {
        (&a.id, &a.kind, &a.layers, &a.path, &a.generated_hash).hash(&mut h);
        a.settings.to_string().hash(&mut h);
    }
    h.finish()
}

impl TreeView {
    /// The ids of artifact records whose file is out of date.
    ///
    /// Why: finding out means making the file again, which costs, so it is
    /// done once per document change and shared by the tree, Manufacture and
    /// Properties.
    ///
    /// # Rules
    /// - A file that cannot be made counts as out of date.
    /// - Nothing is made when the design has no artifact records.
    ///
    /// # Dependencies
    /// - crates/engine/src/artifacts.rs (`artifact_is_stale`)
    ///
    /// # Do not
    /// - Call it for a design other than the one `revision` belongs to.
    pub(crate) fn stale_ids(&mut self, project: &Project, revision: u64) -> Rc<BTreeSet<Id>> {
        let fp = stale_fingerprint(project);
        if let Some((r, f, set)) = &self.stale
            && *r == revision
            && *f == fp
        {
            return Rc::clone(set);
        }
        let set: BTreeSet<Id> = project
            .artifacts
            .iter()
            .filter(|a| pcbcraft_engine::artifact_is_stale(project, a).unwrap_or(true))
            .map(|a| a.id)
            .collect();
        let set = Rc::new(set);
        self.stale = Some((revision, fp, Rc::clone(&set)));
        set
    }
}

fn record_row(env: &Env, parent: &str, a: &Artifact) -> Row {
    let mut r = row(
        parent,
        &format!("artifact:{}", rest_of(a)),
        RowKind::Leaf,
        a.name.clone(),
    );
    let stale = env.stale.is_some_and(|s| s.contains(&a.id));
    r.sub = if stale {
        t!("artifact.row.stale")
    } else {
        a.path.clone()
    };
    r.marks = false;
    r
}

fn drill_rows(env: &Env, parent: &str) -> Vec<Row> {
    let rows = pcbcraft_io::drill_table(env.project).unwrap_or_default();
    rows.iter()
        .map(|d| {
            let key = DrillKey::new(d.from_layer, d.to_layer, d.diameter_mm);
            let mut r = row(
                parent,
                &format!("artifact:{}", key.text()),
                RowKind::Leaf,
                t!(
                    "artifact.row.drill",
                    from = d.from_layer,
                    to = d.to_layer,
                    kind = d.kind.label().to_lowercase()
                ),
            );
            r.sub = format!("{:.3} mm", d.diameter_mm);
            r.count = d.count.to_string();
            r.marks = false;
            r
        })
        .collect()
}

/// Add the artifact rows to the rows built for the Layout tree under `key`.
///
/// Why: the tree builder stays as it was; this adds the files a layer makes at
/// the top of its sublayer, marks a sublayer that now has rows as openable, and
/// adds the Output files row for records that sit on no layer.
///
/// # Rules
/// - Artifact rows come first under their parent, before nets and parts.
/// - A parent with no artifact rows is left exactly as built.
///
/// # Dependencies
/// - crates/ui-egui/src/layers_tree.rs (`children` calls this)
///
/// # Do not
/// - Add rows to the Schematic tree.
pub(crate) fn with_artifact_rows(env: &Env, key: &str, mut built: Vec<Row>) -> Vec<Row> {
    let project = env.project;
    if key.is_empty() {
        let has_outputs = project
            .artifacts
            .iter()
            .any(|a| homes(project, a).iter().any(|h| h == "outputs"));
        if has_outputs {
            let at = built
                .iter()
                .position(|r| r.key == "components")
                .unwrap_or(built.len());
            let mut r = row("", "outputs", RowKind::Layer, t!("artifact.outputs.title"));
            r.expandable = true;
            r.swatch = Some(env.colours.link);
            built.insert(at, r);
        }
    }
    let is_layer = key.starts_with("side:") || key.starts_with("inner:");
    if key.is_empty() || is_layer {
        for r in &mut built {
            if !r.expandable && has_record(project, &r.key) {
                r.expandable = true;
            }
        }
    }
    let mut mine: Vec<Row> = Vec::new();
    if key == "drills" {
        mine.extend(drill_rows(env, key));
    }
    for a in &project.artifacts {
        if homes(project, a).iter().any(|h| h == key) {
            mine.push(record_row(env, key, a));
        }
    }
    if mine.is_empty() {
        return built;
    }
    mine.extend(built);
    mine
}

fn has_record(project: &Project, key: &str) -> bool {
    project
        .artifacts
        .iter()
        .any(|a| homes(project, a).iter().any(|h| h == key))
}

fn fact(ui: &mut egui::Ui, label: String, value: String, tip: String) {
    form_row(ui, FormWidth::Aside, &label, None, None, |ui| {
        ui.label(fonts::text(fonts::Role::Mono, &value))
            .on_hover_text(tip);
    });
}

fn settings_text(a: &Artifact) -> String {
    match &a.settings {
        serde_json::Value::Null => t!("artifact.settings.defaults"),
        serde_json::Value::Object(m) if m.is_empty() => t!("artifact.settings.defaults"),
        other => other.to_string(),
    }
}

fn kind_label(kind: &str) -> String {
    pcbcraft_text::lookup(&format!("artifact.kind.{kind}"), &[])
}

impl App {
    /// Properties of an artifact record: what it is, which layers it comes
    /// from, its settings, its file and whether the file is out of date.
    ///
    /// # Rules
    /// - Shows a button to make the file again and one to open Manufacture.
    /// - A record that is gone reads "no longer in the design".
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`artifact.export` makes the file and stamps it)
    ///
    /// # Do not
    /// - Edit the record here; creating and editing records is another change.
    pub(crate) fn record_artifact_properties(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(a) = self.project.artifacts.iter().find(|a| a.id == id).cloned() else {
            ui.label(t!("artifact.unknown"));
            return;
        };
        let stale = self
            .tree
            .stale_ids(&self.project, self.revision)
            .contains(&a.id);
        ui.label(fonts::title(a.name.clone()));
        fact(
            ui,
            t!("artifact.kind"),
            kind_label(&a.kind),
            t!("artifact.kind.tip"),
        );
        fact(
            ui,
            t!("artifact.layers"),
            if a.layers.is_empty() {
                t!("artifact.layers.usual")
            } else {
                a.layers.join(", ")
            },
            t!("artifact.layers.tip"),
        );
        let facts = crate::properties::artifact_kind::kind_facts(&a);
        if facts.is_empty() {
            fact(
                ui,
                t!("artifact.settings"),
                settings_text(&a),
                t!("artifact.settings.tip"),
            );
        } else {
            for f in facts {
                let label = pcbcraft_text::lookup(f.label_key, &[]);
                let tip_key = format!("{}.tip", f.label_key);
                let tip = pcbcraft_text::lookup(&tip_key, &[]);
                let value = if f.value.starts_with("artifact.fact.") {
                    pcbcraft_text::lookup(&f.value, &[])
                } else {
                    f.value.clone()
                };
                fact(ui, label, value, tip);
            }
        }
        fact(
            ui,
            t!("artifact.path"),
            a.path.clone(),
            t!("artifact.path.tip"),
        );
        fact(
            ui,
            t!("artifact.state"),
            if stale {
                t!("artifact.state.stale")
            } else {
                t!("artifact.state.current")
            },
            t!("artifact.state.tip"),
        );
        ui.add_space(8.0);
        let make = ui
            .button(t!("artifact.make"))
            .on_hover_text(t!("artifact.make.tip"));
        if make.clicked() {
            let directory = self.make_directory();
            if std::fs::create_dir_all(&directory).is_err() {
                self.message = t!("status.write_make_files.that_folder_could_not_be");
            } else {
                self.run(
                    "artifact.export",
                    serde_json::json!({"id": a.id, "directory": directory}),
                );
                // The command reports the record id, which `run` takes for an object.
                self.select_subject(Selection::Artifact(rest_of(&a)));
            }
        }
        let show = ui
            .button(t!("artifact.show"))
            .on_hover_text(t!("artifact.show.tip"));
        if show.clicked() {
            self.change_tab(Tab::PlotFiles);
        }
    }

    /// The Output files list in Manufacture: one row per artifact record.
    ///
    /// # Rules
    /// - Draws nothing when the design has no artifact records.
    /// - The selected record is lit strongly; one from the selected layer weakly.
    /// - Pressing a row selects the artifact, the same value the Layers tree sets.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/workspace.rs (the Manufacture centre calls this)
    ///
    /// # Do not
    /// - Make a file from here; the button lives in Properties.
    pub(crate) fn artifact_outputs_section(&mut self, ui: &mut egui::Ui) {
        if self.project.artifacts.is_empty() {
            return;
        }
        let stale = self.tree.stale_ids(&self.project, self.revision);
        let accent = self.accent();
        let muted = self.muted();
        let heading = t!("artifact.outputs.title");
        let mut picked: Option<String> = None;
        let header = egui::CollapsingHeader::new(heading.clone())
            .id_salt("artifact-outputs")
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                for a in &self.project.artifacts {
                    let level = level_for(&self.project, &self.subject, a, None);
                    let mut text = egui::RichText::new(a.name.clone());
                    if level == Level::Weak {
                        text = text.color(accent);
                    }
                    ui.horizontal(|ui| {
                        let resp = ui
                            .selectable_label(level == Level::Strong, text)
                            .on_hover_text(t!("artifact.outputs.row.tip"))
                            .named(&format!("Output {}", a.name));
                        if resp.clicked() {
                            picked = Some(rest_of(a));
                        }
                        let state = if stale.contains(&a.id) {
                            t!("artifact.row.stale")
                        } else {
                            a.path.clone()
                        };
                        ui.label(egui::RichText::new(state).color(muted));
                    });
                }
                ui.add_space(8.0);
            });
        header
            .header_response
            .named(&heading)
            .on_hover_text(t!("artifact.outputs.title.tip"));
        if let Some(rest) = picked {
            self.select_subject(Selection::Artifact(rest));
        }
    }
}
