//! Nets that share a name are one net: the merge that runs when a design opens.
//!
//! A label placed with a typed name joins the net of that name (see
//! `label.place`). A design written before that rule, or by another tool, can
//! hold several nets with one name; this module folds them into the first one
//! and reports what it did, as one undo step.

use super::*;

impl Session {
    /// Replace the design with one just read from a file, folding nets that
    /// share a name.
    ///
    /// # Rules
    /// - The file's design is one undo step and the merge a second, so undo
    ///   after opening gives back the nets exactly as the file held them
    ///   (that state cannot be saved until the merge is redone).
    /// - The merged design must pass `Project::validate`; if it does not,
    ///   nothing is replaced.
    /// - Returns the revision and the merges, so the caller can tell the user.
    ///
    /// # Dependencies
    /// - crates/doc/src/net_merge.rs (`Project::merge_same_name_nets`)
    ///
    /// # Do not
    /// - Pass a design that was not read from a file: `Session::replace`
    ///   refuses a repeated name, this folds it.
    pub fn open_project(&mut self, project: Project) -> Result<Value, Error> {
        let mut merged_project = project.clone();
        let merged = merged_project.merge_same_name_nets();
        merged_project.validate()?;
        self.next_id = self
            .next_id
            .max(project.ids().max().unwrap_or(0).saturating_add(1));
        self.commit(project);
        if !merged.is_empty() {
            self.commit(merged_project);
        }
        Ok(json!({"revision": self.revision, "merged": merged}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_wire(s: &mut Session, y: f64) -> Result<(), Error> {
        s.execute(
            "wire.draw",
            json!({"points":[{"x":0.0,"y":y},{"x":10.0,"y":y}]}),
            None,
        )?;
        Ok(())
    }

    fn net_named(s: &Session, name: &str) -> Vec<Id> {
        s.project
            .nets
            .iter()
            .filter(|n| n.name == name)
            .map(|n| n.id)
            .collect()
    }

    #[test]
    fn label_place_creates_the_net_when_a_typed_name_is_missing() -> Result<(), Error> {
        let mut s = Session::default();
        assert!(net_named(&s, "SIG").is_empty());
        let out = s.execute(
            "label.place",
            json!({"kind":"label","name":"SIG","position":{"x":5.0,"y":5.0}}),
            None,
        )?;
        assert_eq!(out["result"]["created"], json!(true));
        assert_eq!(net_named(&s, "SIG").len(), 1);
        assert_eq!(s.project.labels.len(), 1);
        assert_eq!(
            Some(s.project.labels[0].net),
            net_named(&s, "SIG").first().copied()
        );
        s.undo()?;
        assert!(net_named(&s, "SIG").is_empty(), "undo removes the made net");
        assert!(s.project.labels.is_empty());
        Ok(())
    }

    #[test]
    fn label_place_needs_a_net_or_a_name_and_trims_the_name() -> Result<(), Error> {
        let mut s = Session::default();
        assert!(
            s.execute(
                "label.place",
                json!({"kind":"label","position":{"x":1.0,"y":1.0}}),
                None
            )
            .is_err()
        );
        assert!(
            s.execute(
                "label.place",
                json!({"kind":"label","name":"   ","position":{"x":1.0,"y":1.0}}),
                None
            )
            .is_err()
        );
        s.execute(
            "label.place",
            json!({"kind":"label","name":"  CLK ","position":{"x":1.0,"y":1.0}}),
            None,
        )?;
        assert_eq!(net_named(&s, "CLK").len(), 1, "the stored name is trimmed");
        assert!(s.project.validate().is_ok());
        Ok(())
    }

    #[test]
    fn labels_of_one_name_put_two_wires_on_one_net() -> Result<(), Error> {
        let mut s = Session::default();
        bare_wire(&mut s, 0.0)?;
        bare_wire(&mut s, 20.0)?;
        for y in [0.0, 20.0] {
            s.execute(
                "label.place",
                json!({"kind":"label","name":"SIG","position":{"x":5.0,"y":y}}),
                None,
            )?;
        }
        let sig = net_named(&s, "SIG");
        assert_eq!(sig.len(), 1, "one net carries the name");
        assert!(
            s.project
                .drawn_wires
                .iter()
                .all(|w| w.net == sig.first().copied()),
            "both wires sit on that net"
        );
        assert!(
            s.project
                .labels
                .iter()
                .all(|l| Some(l.net) == sig.first().copied())
        );
        Ok(())
    }

    #[test]
    fn a_label_name_on_a_wired_net_renames_it_or_merges_into_the_named_net() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "wire.draw",
            json!({"points":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0}],"net":a}),
            None,
        )?;
        s.execute(
            "wire.draw",
            json!({"points":[{"x":0.0,"y":9.0},{"x":10.0,"y":9.0}],"net":b}),
            None,
        )?;
        let out = s.execute(
            "label.place",
            json!({"kind":"label","net":a,"name":"SIG","position":{"x":5.0,"y":0.0}}),
            None,
        )?;
        assert_eq!(out["result"]["merged"], json!(null));
        assert_eq!(s.project.net_name(a), "SIG", "a free name renames the net");
        let out = s.execute(
            "label.place",
            json!({"kind":"label","net":b,"name":"SIG","position":{"x":5.0,"y":9.0}}),
            None,
        )?;
        assert_eq!(out["result"]["merged"], json!(b));
        assert_eq!(
            net_named(&s, "SIG"),
            vec![a],
            "the older net keeps the name"
        );
        assert!(s.project.nets.iter().all(|n| n.id != b));
        assert!(s.project.drawn_wires.iter().all(|w| w.net == Some(a)));
        assert!(s.project.validate().is_ok());
        Ok(())
    }

    #[test]
    fn power_label_style_round_trips_and_only_power_takes_one() -> Result<(), Error> {
        let mut s = Session::default();
        for style in Label::POWER_STYLES {
            s.execute(
                "label.place",
                json!({"kind":"power","name":"3V3","style":style,"position":{"x":1.0,"y":1.0}}),
                None,
            )?;
        }
        assert_eq!(net_named(&s, "3V3").len(), 1, "four symbols, one rail");
        let styles: Vec<&str> = s.project.labels.iter().map(|l| l.style.as_str()).collect();
        assert_eq!(styles, ["ground", "bar", "arrow", "negative"]);
        let text = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
        let back: Project =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
        assert_eq!(
            back.labels, s.project.labels,
            "the style survives save and load"
        );
        for bad in [
            json!({"kind":"power","name":"5V","style":"pennant","position":{"x":1.0,"y":1.0}}),
            json!({"kind":"label","name":"5V","style":"bar","position":{"x":1.0,"y":1.0}}),
        ] {
            assert!(s.execute("label.place", bad, None).is_err());
        }
        let first = s.project.labels[0].id;
        s.execute("label.set", json!({"id":first,"style":"arrow"}), None)?;
        assert_eq!(s.project.labels[0].style, "arrow");
        assert!(
            s.execute("label.set", json!({"id":first,"style":"arrow"}), None)
                .is_err(),
            "setting the style it already has is refused"
        );
        Ok(())
    }

    fn two_nets_named_x() -> Result<Session, Error> {
        let mut s = Session::default();
        for (i, id) in [101, 102].into_iter().enumerate() {
            s.project.nets.push(Net {
                id,
                name: "X".into(),
                ..Default::default()
            });
            let y = 10.0 * i as f64;
            s.project.drawn_wires.push(DrawnWire {
                id: id + 100,
                net: Some(id),
                points: vec![Point::new(0.0, y), Point::new(10.0, y)],
            });
        }
        Ok(s)
    }

    #[test]
    fn opening_a_design_merges_nets_that_share_a_name_in_one_undo_step() -> Result<(), Error> {
        let source = two_nets_named_x()?;
        let mut s = Session::default();
        let out = s.open_project(source.project)?;
        let merged = out["merged"].as_array().ok_or_else(missing)?;
        assert_eq!(merged.len(), 1, "one name was repeated");
        assert_eq!(merged[0]["name"], json!("X"));
        assert_eq!(merged[0]["merged"].as_array().map(Vec::len), Some(1));
        assert_eq!(merged[0]["pins"], json!(0));
        assert!(s.project.validate().is_ok(), "the opened design is valid");
        assert_eq!(net_named(&s, "X").len(), 1);
        assert!(
            s.project
                .drawn_wires
                .iter()
                .all(|w| w.net == net_named(&s, "X").first().copied()),
            "both wires now sit on the kept net"
        );
        s.undo()?;
        assert_eq!(net_named(&s, "X").len(), 2, "one undo restores both nets");
        s.undo()?;
        assert!(
            s.project.nets.is_empty(),
            "the next undo is the load itself"
        );
        Ok(())
    }

    #[test]
    fn opening_a_design_without_repeated_names_records_no_extra_step() -> Result<(), Error> {
        let mut source = Session::default();
        source.execute("net.create", json!({"name":"A"}), None)?;
        source.execute("net.create", json!({"name":"B"}), None)?;
        let mut s = Session::default();
        let out = s.open_project(source.project.clone())?;
        assert_eq!(out["merged"], json!([]));
        s.undo()?;
        assert!(!s.can_undo(), "only the load itself was recorded");
        Ok(())
    }

    #[test]
    fn a_file_with_repeated_net_names_loads_for_open_but_not_as_valid() -> Result<(), Error> {
        let source = two_nets_named_x()?;
        let text =
            serde_json::to_string(&source.project).map_err(|e| Error::Invalid(e.to_string()))?;
        let dir = tempfile::tempdir().map_err(|e| Error::Invalid(e.to_string()))?;
        let path = dir.path().join("twins.pcbcraft");
        std::fs::write(&path, text).map_err(|e| Error::Invalid(e.to_string()))?;
        assert!(
            pcbcraft_io::load(&path).is_err(),
            "the strict reader still refuses a repeated name"
        );
        let mut s = Session::default();
        let out = s.open_project(pcbcraft_io::load_for_open(&path).map_err(Error::Invalid)?)?;
        assert_eq!(out["merged"].as_array().map(Vec::len), Some(1));
        assert_eq!(net_named(&s, "X").len(), 1);
        Ok(())
    }
}
