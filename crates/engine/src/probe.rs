//! Virtual probes: put a probe on a net or pin and read its DC voltage from the simulator.
//!
//! # Rules
//! - `probe.add` and `probe.remove` edit the design and are one undo step each; a refusal
//!   leaves the project unchanged.
//! - `probe.list` and `probe.read` only read; `probe.read` runs the same solve as `sim.dc`.
//! - A reading is the voltage of the probed net minus the voltage of its reference net
//!   (ground when none is named), in volts. When the solve cannot run, or a net is not part of
//!   the solved circuit, the answer is a plain sentence and no number is made up.
//! - A design holds at most `MAX_PROBES` probes.
//!
//! # Dependencies
//! - crates/doc/src/probe.rs
//! - crates/engine/src/sim.rs
//!
//! # Do not
//! - Touch real hardware, serial ports or USB; these probes are virtual.
//! - Store a reading in the project.
use super::*;
use pcbcraft_doc::{MAX_PROBE_LABEL, MAX_PROBES, Probe};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddParams {
    net: Option<Id>,
    pin: Option<PinRef>,
    reference: Option<Id>,
    label: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveParams {
    id: Id,
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "probe.add",
            description: "Put a virtual probe on a net or on a pin of the schematic, optionally measured against a reference net (ground by default) and with a short label. Give exactly one of net and pin. Returns the probe id. At most 64 probes per design",
            input_schema: schema(
                json!({
                    "net": id_schema(),
                    "pin": schema(
                        json!({"component": id_schema(), "pin": {"type":"string","minLength":1,"maxLength":64}, "unit": {"type":["integer","null"],"minimum":1}}),
                        &["component", "pin"],
                    ),
                    "reference": id_schema(),
                    "label": {"type":"string","minLength":1,"maxLength":MAX_PROBE_LABEL},
                }),
                &[],
            ),
        },
        CommandSpec {
            id: "probe.remove",
            description: "Remove one virtual probe by its id",
            input_schema: schema(json!({"id": id_schema()}), &["id"]),
        },
        CommandSpec {
            id: "probe.list",
            description: "List the virtual probes of the design with their point, reference and label. Read only",
            input_schema: schema(json!({}), &[]),
        },
        CommandSpec {
            id: "probe.read",
            description: "Run the DC solve of sim.dc on the current design and return each probe's voltage in volts relative to its reference net (ground by default). When the solve cannot run, solved is false and reason is a plain sentence; a probe on a net the solve does not reach has volts null and a note. Read only",
            input_schema: schema(json!({}), &[]),
        },
    ]
}

/// True for the probe commands that only read the design.
pub(super) fn handles_read(command: &str) -> bool {
    matches!(command, "probe.list" | "probe.read")
}

pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    Some(match command {
        "probe.add" => add(p, next, params),
        "probe.remove" => remove(p, params),
        _ => return None,
    })
}

pub(super) fn run_read(p: &Project, command: &str, params: &Value) -> Result<Value, Error> {
    if params.as_object().is_some_and(|o| !o.is_empty()) {
        return Err(Error::Invalid(format!("{command} takes no parameters")));
    }
    match command {
        "probe.list" => Ok(list(p)),
        _ => Ok(read(p)),
    }
}

fn add(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: AddParams = parse(params)?;
    if p.probes.len() >= MAX_PROBES {
        return Err(Error::Invalid(format!(
            "a design carries at most {MAX_PROBES} probes"
        )));
    }
    let label = a.label.map(|l| l.trim().to_string());
    let id = allocate(next)?;
    let probe = Probe {
        id,
        net: a.net,
        pin: a.pin,
        reference: a.reference,
        label,
    };
    probe.validate(p).map_err(Error::Invalid)?;
    if p.probes
        .iter()
        .any(|q| q.net == probe.net && q.pin == probe.pin && q.reference == probe.reference)
    {
        return Err(Error::Invalid(
            "a probe with that reference is already on that point".into(),
        ));
    }
    p.probes.push(probe);
    Ok(json!({"id": id}))
}

fn remove(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: RemoveParams = parse(params)?;
    let Some(at) = p.probes.iter().position(|q| q.id == a.id) else {
        return Err(Error::Invalid("no probe has that id".into()));
    };
    p.probes.remove(at);
    Ok(json!({"removed": a.id}))
}

/// The probe's point in words: a net name, or `R1.2` for a pin.
fn point_text(p: &Project, probe: &Probe) -> String {
    match (&probe.net, &probe.pin) {
        (Some(net), _) => p.net_name(*net).to_string(),
        (None, Some(pin)) => {
            let owner = p
                .component(pin.component)
                .map(|c| c.reference.clone())
                .unwrap_or_default();
            format!("{owner}.{}", pin.pin)
        }
        (None, None) => String::new(),
    }
}

fn list(p: &Project) -> Value {
    let probes: Vec<Value> = p
        .probes
        .iter()
        .map(|q| {
            json!({
                "id": q.id,
                "net": q.net,
                "pin": q.pin,
                "point": point_text(p, q),
                "reference": q.reference,
                "reference_name": q.reference.map(|r| p.net_name(r).to_string()),
                "label": q.label,
            })
        })
        .collect();
    json!({"probes": probes})
}

fn read(p: &Project) -> Value {
    let solved = if p.probes.is_empty() {
        Err("No probes are placed, so nothing was solved.".to_string())
    } else {
        super::sim::solve_volts(p).map_err(|why| format!("The DC solve could not run: {why}."))
    };
    let (volts, ground, reason) = match &solved {
        Ok(s) => (Some(&s.volts), Some(s.ground), None),
        Err(why) => (None, None, Some(why.clone())),
    };
    let rows: Vec<Value> = p
        .probes
        .iter()
        .map(|q| {
            let mut note: Option<String> = None;
            let mut value: Option<f64> = None;
            if let (Some(volts), Some(ground)) = (volts, ground) {
                let at = |net: Option<Id>, what: &str| -> Result<f64, String> {
                    let Some(net) = net else {
                        return Err(format!("The {what} is not on a net."));
                    };
                    if net == ground {
                        return Ok(0.0);
                    }
                    volts.get(&net).copied().ok_or_else(|| {
                        format!(
                            "Net {} is not part of the solved circuit; connect it to a source or a resistor.",
                            p.net_name(net)
                        )
                    })
                };
                let high = at(q.probed_net(p), "probed pin");
                let low = match q.reference {
                    Some(r) => at(Some(r), "reference"),
                    None => Ok(0.0),
                };
                match (high, low) {
                    (Ok(h), Ok(l)) => value = Some(((h - l) * 1e9).round() / 1e9),
                    (Err(why), _) | (_, Err(why)) => note = Some(why),
                }
            }
            json!({
                "id": q.id,
                "label": q.label,
                "point": point_text(p, q),
                "reference": q.reference.map(|r| p.net_name(r).to_string()),
                "volts": value,
                "note": note,
            })
        })
        .collect();
    json!({
        "solved": solved.is_ok(),
        "reason": reason,
        "ground": ground.map(|g| p.net_name(g).to_string()),
        "probes": rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    fn missing() -> Error {
        Error::Invalid("missing field".into())
    }

    fn id_of(out: &Value) -> Result<Id, Error> {
        out["result"]["id"].as_u64().ok_or_else(missing)
    }

    fn net(s: &Session, name: &str) -> Result<Id, Error> {
        s.project
            .nets
            .iter()
            .find(|n| n.name == name)
            .map(|n| n.id)
            .ok_or_else(missing)
    }

    fn part(s: &Session, reference: &str) -> Result<Id, Error> {
        s.project
            .components
            .iter()
            .find(|c| c.reference == reference)
            .map(|c| c.id)
            .ok_or_else(missing)
    }

    /// A 10 V source across two 1 k resistors; the middle net MID sits at 5 V.
    fn divider() -> Result<Session, Error> {
        let mut s = Session::default();
        for name in ["GND", "VCC", "MID"] {
            s.execute("net.create", json!({"name": name}), None)?;
        }
        for (reference, value, x) in [("V1", "", 0.0), ("R1", "1k", 5.0), ("R2", "1k", 10.0)] {
            let mut params =
                json!({"kind":"resistor","reference":reference,"position":{"x":x,"y":0.0}});
            if !value.is_empty() {
                params["value"] = json!(value);
            }
            s.execute("component.add", params, None)?;
        }
        s.execute(
            "netlist.import",
            json!({"nets":[
                {"name":"GND","pins":[{"reference":"V1","pin":"2"},{"reference":"R2","pin":"2"}]},
                {"name":"VCC","pins":[{"reference":"V1","pin":"1"},{"reference":"R1","pin":"1"}]},
                {"name":"MID","pins":[{"reference":"R1","pin":"2"},{"reference":"R2","pin":"1"}]},
            ]}),
            None,
        )?;
        for (reference, model) in [("V1", "V"), ("R1", "R"), ("R2", "R")] {
            s.execute(
                "component.field",
                json!({"reference":reference,"key":"sim.model","value":model}),
                None,
            )?;
        }
        s.execute(
            "component.field",
            json!({"reference":"V1","key":"sim.value","value":"10"}),
            None,
        )?;
        Ok(s)
    }

    fn volts_of(out: &Value, index: usize) -> Result<f64, Error> {
        out["result"]["probes"][index]["volts"]
            .as_f64()
            .ok_or_else(missing)
    }

    fn undo_depth(s: &Session) -> u64 {
        s.inspect()["history"]["undo"].as_u64().unwrap_or(0)
    }

    #[test]
    fn a_probe_goes_on_a_net_and_is_listed() -> Result<(), Error> {
        let mut s = divider()?;
        let mid = net(&s, "MID")?;
        let out = s.execute("probe.add", json!({"net": mid, "label": "P1"}), None)?;
        let id = id_of(&out)?;
        assert_eq!(s.project.probes.len(), 1);
        let listed = s.execute("probe.list", json!({}), None)?;
        assert_eq!(listed["result"]["probes"][0]["id"], id);
        assert_eq!(listed["result"]["probes"][0]["point"], "MID");
        assert_eq!(listed["result"]["probes"][0]["label"], "P1");
        Ok(())
    }

    #[test]
    fn a_probe_goes_on_a_pin() -> Result<(), Error> {
        let mut s = divider()?;
        let r1 = part(&s, "R1")?;
        s.execute(
            "probe.add",
            json!({"pin": {"component": r1, "pin": "2"}}),
            None,
        )?;
        let listed = s.execute("probe.list", json!({}), None)?;
        assert_eq!(listed["result"]["probes"][0]["point"], "R1.2");
        Ok(())
    }

    #[test]
    fn bad_probes_are_refused_and_change_nothing() -> Result<(), Error> {
        let mut s = divider()?;
        let mid = net(&s, "MID")?;
        let r1 = part(&s, "R1")?;
        s.execute("probe.add", json!({"net": mid}), None)?;
        let before = s.inspect();
        for bad in [
            json!({"net": 999_999}),
            json!({"pin": {"component": 999_999, "pin": "1"}}),
            json!({"pin": {"component": r1, "pin": "99"}}),
            json!({}),
            json!({"net": mid, "pin": {"component": r1, "pin": "1"}}),
            json!({"net": net(&s, "GND")?, "reference": 999_999}),
            json!({"net": mid, "reference": mid}),
            json!({"net": mid}),
            json!({"net": net(&s, "VCC")?, "label": ""}),
            json!({"net": net(&s, "VCC")?, "label": "x".repeat(MAX_PROBE_LABEL + 1)}),
            json!({"net": net(&s, "VCC")?, "extra": 1}),
        ] {
            assert!(
                s.execute("probe.add", bad.clone(), None).is_err(),
                "{bad} must be refused"
            );
        }
        assert_eq!(s.inspect(), before, "refusals change nothing");
        Ok(())
    }

    #[test]
    fn a_design_holds_at_most_sixty_four_probes() -> Result<(), Error> {
        let mut s = divider()?;
        for n in 0..MAX_PROBES {
            s.execute("net.create", json!({"name": format!("N{n}")}), None)?;
        }
        let ids: Vec<Id> = s
            .project
            .nets
            .iter()
            .filter(|n| n.name.starts_with('N'))
            .map(|n| n.id)
            .collect();
        assert_eq!(ids.len(), MAX_PROBES);
        for id in ids {
            s.execute("probe.add", json!({"net": id}), None)?;
        }
        assert_eq!(s.project.probes.len(), MAX_PROBES);
        let mid = net(&s, "MID")?;
        let refused = s.execute("probe.add", json!({"net": mid, "label": "late"}), None);
        assert!(refused.is_err(), "the 65th probe is refused");
        assert_eq!(s.project.probes.len(), MAX_PROBES);
        Ok(())
    }

    #[test]
    fn add_and_remove_are_one_undo_step_each() -> Result<(), Error> {
        let mut s = divider()?;
        let mid = net(&s, "MID")?;
        let depth = undo_depth(&s);
        let id = id_of(&s.execute("probe.add", json!({"net": mid}), None)?)?;
        assert_eq!(undo_depth(&s), depth + 1);
        s.execute("probe.remove", json!({"id": id}), None)?;
        assert_eq!(undo_depth(&s), depth + 2);
        assert!(s.project.probes.is_empty());
        s.undo()?;
        assert_eq!(
            s.project.probes.len(),
            1,
            "undoing the remove brings it back"
        );
        s.undo()?;
        assert!(s.project.probes.is_empty(), "undoing the add takes it away");
        assert!(
            s.execute("probe.remove", json!({"id": id}), None).is_err(),
            "an unknown probe id is refused"
        );
        Ok(())
    }

    #[test]
    fn reading_a_divider_gives_the_exact_voltages() -> Result<(), Error> {
        let mut s = divider()?;
        let mid = net(&s, "MID")?;
        let vcc = net(&s, "VCC")?;
        let gnd = net(&s, "GND")?;
        let r1 = part(&s, "R1")?;
        s.execute("probe.add", json!({"net": mid}), None)?;
        s.execute("probe.add", json!({"net": mid, "reference": vcc}), None)?;
        s.execute(
            "probe.add",
            json!({"pin": {"component": r1, "pin": "1"}}),
            None,
        )?;
        s.execute("probe.add", json!({"net": gnd}), None)?;
        let revision = s.revision;
        let out = s.execute("probe.read", json!({}), None)?;
        assert_eq!(out["revision"], revision, "reading changes nothing");
        assert_eq!(out["result"]["solved"], true);
        let want = [5.0, -5.0, 10.0, 0.0];
        for (index, want) in want.into_iter().enumerate() {
            let got = volts_of(&out, index)?;
            assert!(
                (got - want).abs() < 1e-9,
                "probe {index} read {got}, wanted {want}"
            );
        }
        Ok(())
    }

    #[test]
    fn reading_without_a_ground_says_why_and_invents_no_number() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("net.create", json!({"name": "A"}), None)?;
        let a = net(&s, "A")?;
        s.execute("probe.add", json!({"net": a}), None)?;
        let out = s.execute("probe.read", json!({}), None)?;
        assert_eq!(out["result"]["solved"], false);
        let reason = out["result"]["reason"].as_str().ok_or_else(missing)?;
        assert!(
            reason.contains("ground"),
            "the sentence names the ground: {reason}"
        );
        assert!(out["result"]["probes"][0]["volts"].is_null());
        Ok(())
    }

    #[test]
    fn a_floating_net_reads_as_a_sentence_not_a_number() -> Result<(), Error> {
        let mut s = divider()?;
        s.execute("net.create", json!({"name": "X"}), None)?;
        s.execute("net.create", json!({"name": "Y"}), None)?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R3","value":"1k","position":{"x":15.0,"y":0.0}}),
            None,
        )?;
        s.execute(
            "netlist.import",
            json!({"nets":[
                {"name":"X","pins":[{"reference":"R3","pin":"1"}]},
                {"name":"Y","pins":[{"reference":"R3","pin":"2"}]},
            ]}),
            None,
        )?;
        s.execute(
            "component.field",
            json!({"reference":"R3","key":"sim.model","value":"R"}),
            None,
        )?;
        let x = net(&s, "X")?;
        s.execute("probe.add", json!({"net": x}), None)?;
        let out = s.execute("probe.read", json!({}), None)?;
        assert_eq!(out["result"]["solved"], false);
        assert!(out["result"]["probes"][0]["volts"].is_null());
        assert!(
            out["result"]["reason"]
                .as_str()
                .is_some_and(|r| !r.is_empty())
        );
        Ok(())
    }

    #[test]
    fn deleting_the_probed_net_drops_the_probe() -> Result<(), Error> {
        let mut s = divider()?;
        let x = s
            .execute("net.create", json!({"name": "SPARE"}), None)
            .map(|o| id_of(&o))??;
        s.execute("probe.add", json!({"net": x}), None)?;
        s.execute("object.delete", json!({"id": x}), None)?;
        assert!(s.project.probes.is_empty());
        Ok(())
    }
}
