//! Folding nets together: one net into another, and every set of nets that
//! share a name into the first of them.
//!
//! The engine uses `merge_net` for its net commands and `merge_same_name_nets`
//! when a design opens, because the file reader hands over a design that may
//! still repeat a net name, which `Project::validate` refuses.

use crate::{Id, Project};
use serde::Serialize;

/// What folding one shared name did.
///
/// # Rules
/// - `kept` is the first net of that name in the design; `merged` lists the
///   ids of the later nets that were folded into it, in design order.
/// - `pins` counts the component pins that moved onto the kept net.
///
/// # Dependencies
/// - crates/doc/src/net_merge.rs (`Project::merge_same_name_nets` builds these)
/// - crates/ui-egui/src/files_home.rs (shows them in the open notice)
///
/// # Do not
/// - Keep a merged id around: those nets no longer exist.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NetMerge {
    /// The shared name, trimmed.
    pub name: String,
    /// Id of the net that stayed.
    pub kept: Id,
    /// Ids of the nets folded into it.
    pub merged: Vec<Id>,
    /// Component pins that moved onto the kept net.
    pub pins: usize,
}

impl Project {
    /// Move everything one net owns onto another net and remove the first.
    ///
    /// # Rules
    /// - Pins, wires, labels, ports, tracks, vias, zones, net classes, pair
    ///   rules and length groups that named `source` name `target` after.
    /// - A pair rule or length group left with one net is dropped.
    /// - Does nothing when either net is missing or the two are the same.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`merge_net_into` and `net.tie` call it)
    ///
    /// # Do not
    /// - Call it to rename a net; use `net.rename`.
    pub fn merge_net(&mut self, source: Id, target: Id) {
        if source == target
            || !self.nets.iter().any(|n| n.id == source)
            || !self.nets.iter().any(|n| n.id == target)
        {
            return;
        }
        for c in self.components.iter_mut() {
            for pin in c.pins.iter_mut().filter(|pin| pin.net == Some(source)) {
                pin.net = Some(target);
            }
        }
        for w in self
            .drawn_wires
            .iter_mut()
            .filter(|w| w.net == Some(source))
        {
            w.net = Some(target);
        }
        for w in self.wires.iter_mut().filter(|w| w.net == source) {
            w.net = target;
        }
        for l in self.labels.iter_mut().filter(|l| l.net == source) {
            l.net = target;
        }
        for port in self.ports.iter_mut().filter(|port| port.net == source) {
            port.net = target;
        }
        for t in self.board.tracks.iter_mut().filter(|t| t.net == source) {
            t.net = target;
        }
        for v in self.board.vias.iter_mut().filter(|v| v.net == source) {
            v.net = target;
        }
        for z in self.board.zones.iter_mut().filter(|z| z.net == source) {
            z.net = target;
        }
        for class in self.net_classes.iter_mut() {
            if class.nets.contains(&source) {
                class.nets.retain(|n| *n != source && *n != target);
                class.nets.push(target);
                class.nets.sort_unstable();
                class.nets.dedup();
            }
        }
        for rule in self.board.pair_rules.iter_mut() {
            if rule.a == source {
                rule.a = target;
            }
            if rule.b == source {
                rule.b = target;
            }
        }
        self.board.pair_rules.retain(|r| r.a != r.b);
        for group in self.board.length_groups.iter_mut() {
            if group.nets.contains(&source) {
                group.nets.retain(|n| *n != source);
                group.nets.push(target);
                group.nets.sort_unstable();
                group.nets.dedup();
            }
        }
        self.board.length_groups.retain(|g| g.nets.len() >= 2);
        self.nets.retain(|n| n.id != source);
    }

    /// Fold every set of nets that share a name into the first net of that name.
    ///
    /// # Rules
    /// - Names are compared after trimming; an empty name is left alone.
    /// - A design with no repeated name is left untouched and gives an empty list.
    /// - The kept net is the first of its name in design order.
    ///
    /// # Dependencies
    /// - crates/doc/src/net_merge.rs (`Project::merge_net` moves what a net owns)
    ///
    /// # Do not
    /// - Call it mid-edit without telling the user; it is for the moment a
    ///   design opens, before `Project::validate` runs.
    pub fn merge_same_name_nets(&mut self) -> Vec<NetMerge> {
        let mut seen: Vec<(String, Id)> = Vec::new();
        let mut folds: Vec<(Id, Id)> = Vec::new();
        for net in &self.nets {
            let name = net.name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            match seen.iter().find(|(n, _)| *n == name) {
                Some((_, kept)) => folds.push((net.id, *kept)),
                None => seen.push((name, net.id)),
            }
        }
        let mut report: Vec<NetMerge> = Vec::new();
        for (source, kept) in folds {
            let pins: usize = self
                .components
                .iter()
                .map(|c| c.pins.iter().filter(|pin| pin.net == Some(source)).count())
                .sum();
            self.merge_net(source, kept);
            match report.iter_mut().find(|m| m.kept == kept) {
                Some(m) => {
                    m.merged.push(source);
                    m.pins += pins;
                }
                None => report.push(NetMerge {
                    name: self.net_name(kept).trim().to_string(),
                    kept,
                    merged: vec![source],
                    pins,
                }),
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Net, Point};

    fn net(id: Id, name: &str) -> Net {
        Net {
            id,
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn same_name_nets_fold_into_the_first_and_report_what_moved() {
        let mut p = Project::default();
        p.nets
            .extend([net(1, "X"), net(2, "Y"), net(3, "X"), net(4, "X")]);
        p.labels.push(crate::Label {
            id: 9,
            kind: "label".into(),
            net: 3,
            position: Point::default(),
            rotation: 0.0,
            text: String::new(),
            style: String::new(),
        });
        let report = p.merge_same_name_nets();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].name, "X");
        assert_eq!(report[0].kept, 1);
        assert_eq!(report[0].merged, vec![3, 4]);
        let names: Vec<_> = p.nets.iter().map(|n| n.id).collect();
        assert_eq!(names, vec![1, 2], "later same-name nets are gone");
        assert_eq!(p.labels[0].net, 1, "the label follows its net");
        assert!(p.validate().is_ok(), "the folded design is valid again");
    }

    #[test]
    fn a_design_without_repeated_names_is_left_alone() {
        let mut p = Project::default();
        p.nets.extend([net(1, "A"), net(2, "B")]);
        let before = p.clone();
        assert!(p.merge_same_name_nets().is_empty());
        assert_eq!(p, before);
    }
}
