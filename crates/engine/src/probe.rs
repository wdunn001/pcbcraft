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
//! - A probe may carry an expected range in volts (`min`, `max`, or `target` with
//!   `tolerance_percent`, stored as `min` and `max`). `probe.expect` sets or clears it as one
//!   undo step. `probe.read` gives each probe a `status`: `pass`, `fail`, or `unchecked` when
//!   there is no range or no reading.
//! - `findings` raises `probe_out_of_range` for every probe that reads outside its range;
//!   it raises nothing when the solve cannot run.
//!
//! # Dependencies
//! - crates/doc/src/probe.rs
//! - crates/engine/src/sim.rs
//!
//! # Do not
//! - Touch real hardware, serial ports or USB; these probes are virtual.
//! - Store a reading in the project.
use super::*;
use pcbcraft_doc::{MAX_PROBE_LABEL, MAX_PROBE_VOLTS, MAX_PROBES, Probe, ProbeExpect, plain_volts};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddParams {
    net: Option<Id>,
    pin: Option<PinRef>,
    reference: Option<Id>,
    label: Option<String>,
    expect: Option<ExpectParams>,
}

/// An expected range as a caller gives it: bounds, or a target with a tolerance.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectParams {
    min: Option<f64>,
    max: Option<f64>,
    target: Option<f64>,
    tolerance_percent: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetExpectParams {
    id: Id,
    min: Option<f64>,
    max: Option<f64>,
    target: Option<f64>,
    tolerance_percent: Option<f64>,
    clear: Option<bool>,
}

impl ExpectParams {
    /// The stored range these parameters mean, or the plain reason they mean none.
    fn resolve(&self) -> Result<ProbeExpect, Error> {
        let bounds = self.min.is_some() || self.max.is_some();
        let range = match (self.target, self.tolerance_percent) {
            (Some(_), _) | (_, Some(_)) if bounds => {
                return Err(Error::Invalid(
                    "give min and max, or a target with tolerance_percent, not both".into(),
                ));
            }
            (Some(target), Some(tolerance)) => {
                ProbeExpect::from_target(target, tolerance).map_err(Error::Invalid)?
            }
            (Some(_), None) => {
                return Err(Error::Invalid(
                    "a target needs a tolerance_percent beside it".into(),
                ));
            }
            (None, Some(_)) => {
                return Err(Error::Invalid(
                    "a tolerance_percent needs a target beside it".into(),
                ));
            }
            (None, None) => ProbeExpect {
                min: self.min,
                max: self.max,
            },
        };
        range.validate().map_err(Error::Invalid)?;
        Ok(range)
    }
}

/// The schema of an expected range: bounds in volts, or a target and a tolerance.
fn expect_properties() -> Value {
    let volts = |what: &str| json!({"type":"number","minimum":-MAX_PROBE_VOLTS,"maximum":MAX_PROBE_VOLTS,"description":what});
    json!({
        "min": volts("the lowest acceptable voltage in volts"),
        "max": volts("the highest acceptable voltage in volts"),
        "target": volts("the voltage the probe should read, used with tolerance_percent instead of min and max"),
        "tolerance_percent": {"type":"number","minimum":0,"maximum":100,"description":"how far from the target is still acceptable, as a percentage of the target; stored as min and max"},
    })
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
            description: "Put a virtual probe on a net or on a pin of the schematic, optionally measured against a reference net (ground by default) and with a short label. Give exactly one of net and pin. Optionally give the voltage it should read as expect (min and max in volts, or target with tolerance_percent). Returns the probe id. At most 64 probes per design",
            input_schema: schema(
                json!({
                    "expect": schema(expect_properties(), &[]),
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
            id: "probe.expect",
            description: "Set or clear the voltage range one probe should read, as one undo step. Give min, max or both in volts, or a target with tolerance_percent (stored as min and max), or clear true to remove the range. Setting replaces any earlier range. The design then reports a probe_out_of_range finding when the simulated reading is outside it",
            input_schema: schema(
                {
                    let mut props = expect_properties();
                    if let Some(map) = props.as_object_mut() {
                        map.insert("id".into(), id_schema());
                        map.insert("clear".into(), json!({"type":"boolean","description":"true removes the expected range"}));
                    }
                    props
                },
                &["id"],
            ),
        },
        CommandSpec {
            id: "probe.list",
            description: "List the virtual probes of the design with their point, reference, label and expected range. Read only",
            input_schema: schema(json!({}), &[]),
        },
        CommandSpec {
            id: "probe.read",
            description: "Run the DC solve of sim.dc on the current design and return each probe's voltage in volts relative to its reference net (ground by default) and a status of pass, fail or unchecked against its expected range, with a plain reason. When the solve cannot run, solved is false, reason is a plain sentence and every status is unchecked; a probe on a net the solve does not reach has volts null and a note. Read only",
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
        "probe.expect" => expect(p, params),
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
    let expect = a.expect.as_ref().map(ExpectParams::resolve).transpose()?;
    let id = allocate(next)?;
    let probe = Probe {
        id,
        net: a.net,
        pin: a.pin,
        reference: a.reference,
        label,
        expect,
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

fn expect(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: SetExpectParams = parse(params)?;
    let Some(probe) = p.probes.iter_mut().find(|q| q.id == a.id) else {
        return Err(Error::Invalid("no probe has that id".into()));
    };
    let bounds = a.min.is_some() || a.max.is_some();
    let target = a.target.is_some() || a.tolerance_percent.is_some();
    let range = if a.clear == Some(true) {
        if bounds || target {
            return Err(Error::Invalid(
                "clear removes the range; give no min, max or target with it".into(),
            ));
        }
        if probe.expect.is_none() {
            return Err(Error::Invalid(
                "that probe has no expected range to clear".into(),
            ));
        }
        None
    } else {
        if !bounds && !target {
            return Err(Error::Invalid(
                "give min, max or both, or a target with tolerance_percent, or clear true".into(),
            ));
        }
        let given = ExpectParams {
            min: a.min,
            max: a.max,
            target: a.target,
            tolerance_percent: a.tolerance_percent,
        };
        Some(given.resolve()?)
    };
    probe.expect = range;
    Ok(json!({"id": a.id, "expect": range}))
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
                "expect": q.expect,
            })
        })
        .collect();
    json!({"probes": probes})
}

/// One probe's reading in volts against its reference, or the plain sentence that says why
/// there is none.
fn reading(p: &Project, q: &Probe, solved: &super::sim::DcVolts) -> Result<f64, String> {
    let at = |net: Option<Id>, what: &str| -> Result<f64, String> {
        let Some(net) = net else {
            return Err(format!("The {what} is not on a net."));
        };
        if net == solved.ground {
            return Ok(0.0);
        }
        solved.volts.get(&net).copied().ok_or_else(|| {
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
        (Ok(h), Ok(l)) => Ok(((h - l) * 1e9).round() / 1e9),
        (Err(why), _) | (_, Err(why)) => Err(why),
    }
}

/// The name a probe goes by in sentences: its label, or its point.
fn probe_name(p: &Project, q: &Probe) -> String {
    q.label.clone().unwrap_or_else(|| point_text(p, q))
}

fn read(p: &Project) -> Value {
    let solved = if p.probes.is_empty() {
        Err("No probes are placed, so nothing was solved.".to_string())
    } else {
        super::sim::solve_volts(p).map_err(|why| format!("The DC solve could not run: {why}."))
    };
    let (ground, reason) = match &solved {
        Ok(s) => (Some(s.ground), None),
        Err(why) => (None, Some(why.clone())),
    };
    let rows: Vec<Value> = p
        .probes
        .iter()
        .map(|q| {
            let mut note: Option<String> = None;
            let mut value: Option<f64> = None;
            if let Ok(s) = &solved {
                match reading(p, q, s) {
                    Ok(v) => value = Some(v),
                    Err(why) => note = Some(why),
                }
            }
            let (status, why) = match (q.expect, value) {
                (None, _) => (
                    "unchecked",
                    "No expected range is set for this probe.".to_string(),
                ),
                (Some(_), None) => (
                    "unchecked",
                    note.clone()
                        .or_else(|| reason.clone())
                        .unwrap_or_else(|| "There is no reading to check.".to_string()),
                ),
                (Some(range), Some(v)) if range.contains(v) => (
                    "pass",
                    format!("Reads {} V, inside {}.", plain_volts(v), range.range_text()),
                ),
                (Some(range), Some(v)) => (
                    "fail",
                    format!(
                        "Reads {} V, outside {}.",
                        plain_volts(v),
                        range.range_text()
                    ),
                ),
            };
            json!({
                "id": q.id,
                "label": q.label,
                "point": point_text(p, q),
                "reference": q.reference.map(|r| p.net_name(r).to_string()),
                "volts": value,
                "note": note,
                "expect": q.expect,
                "status": status,
                "status_reason": why,
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

/// The `probe_out_of_range` findings: one for every probe whose solved reading is outside the
/// range it should have.
///
/// # Rules
/// - A probe with no range, or with no reading, raises nothing; a failed solve raises nothing.
/// - The severity is the project's override for the code, warning by default; `off` hides it.
/// - Runs with the board checks, so the findings panel and the Checks page list it.
pub(super) fn findings(p: &Project) -> Vec<Value> {
    const CODE: &str = "probe_out_of_range";
    if p.rule_disabled(CODE) || !p.probes.iter().any(|q| q.expect.is_some()) {
        return vec![];
    }
    let Ok(solved) = super::sim::solve_volts(p) else {
        return vec![];
    };
    let severity = p.rule_severity(CODE, "warning");
    let mut out = vec![];
    for q in &p.probes {
        let Some(range) = q.expect else {
            continue;
        };
        let Ok(volts) = reading(p, q, &solved) else {
            continue;
        };
        if range.contains(volts) {
            continue;
        }
        let objects: Vec<Id> = q.probed_net(p).into_iter().collect();
        if p.rule_excluded(CODE, &objects) || p.rule_region_excluded(CODE, &objects) {
            continue;
        }
        out.push(super::check_finding_json(
            CODE,
            &severity,
            &format!(
                "Probe {} reads {} V, outside the expected {}",
                probe_name(p, q),
                plain_volts(volts),
                range.range_text()
            ),
            objects,
            Some("Check the circuit, or change the expected range of this probe in the Analyze Scope table."),
        ));
    }
    out
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

    fn probe_on_mid(s: &mut Session, extra: Value) -> Result<Id, Error> {
        let mid = net(s, "MID")?;
        let mut params = json!({"net": mid, "label": "P1"});
        if let (Some(into), Some(from)) = (params.as_object_mut(), extra.as_object()) {
            for (k, v) in from {
                into.insert(k.clone(), v.clone());
            }
        }
        id_of(&s.execute("probe.add", params, None)?)
    }

    fn status_of(s: &mut Session) -> Result<(String, Value), Error> {
        let out = s.execute("probe.read", json!({}), None)?;
        let row = &out["result"]["probes"][0];
        let status = row["status"].as_str().ok_or_else(missing)?.to_string();
        Ok((status, row.clone()))
    }

    fn range_findings(s: &Session) -> Vec<Value> {
        s.checks()["drc"]
            .as_array()
            .map(|all| {
                all.iter()
                    .filter(|f| f["code"] == "probe_out_of_range")
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn an_expected_range_is_set_replaced_and_cleared_one_undo_step_each() -> Result<(), Error> {
        let mut s = divider()?;
        let id = probe_on_mid(&mut s, json!({}))?;
        let depth = undo_depth(&s);
        s.execute(
            "probe.expect",
            json!({"id": id, "min": 4.9, "max": 5.1}),
            None,
        )?;
        assert_eq!(undo_depth(&s), depth + 1);
        let listed = s.execute("probe.list", json!({}), None)?;
        assert_eq!(listed["result"]["probes"][0]["expect"]["min"], 4.9);
        assert_eq!(listed["result"]["probes"][0]["expect"]["max"], 5.1);
        s.execute("probe.expect", json!({"id": id, "min": 3.0}), None)?;
        let listed = s.execute("probe.list", json!({}), None)?;
        assert_eq!(listed["result"]["probes"][0]["expect"]["min"], 3.0);
        assert!(
            listed["result"]["probes"][0]["expect"]["max"].is_null(),
            "setting replaces the earlier range"
        );
        s.execute("probe.expect", json!({"id": id, "clear": true}), None)?;
        assert!(s.project.probes[0].expect.is_none());
        assert_eq!(undo_depth(&s), depth + 3);
        s.undo()?;
        assert_eq!(s.project.probes[0].expect.and_then(|e| e.min), Some(3.0));
        s.undo()?;
        assert_eq!(s.project.probes[0].expect.and_then(|e| e.max), Some(5.1));
        s.undo()?;
        assert!(
            s.project.probes[0].expect.is_none(),
            "undo takes the range away"
        );
        Ok(())
    }

    #[test]
    fn a_target_with_a_tolerance_is_stored_as_min_and_max() -> Result<(), Error> {
        let mut s = divider()?;
        let id = probe_on_mid(&mut s, json!({}))?;
        s.execute(
            "probe.expect",
            json!({"id": id, "target": 5.0, "tolerance_percent": 5.0}),
            None,
        )?;
        let range = s.project.probes[0].expect.ok_or_else(missing)?;
        assert_eq!((range.min, range.max), (Some(4.75), Some(5.25)));
        Ok(())
    }

    #[test]
    fn bad_ranges_are_refused_and_change_nothing() -> Result<(), Error> {
        let mut s = divider()?;
        let id = probe_on_mid(&mut s, json!({"expect": {"min": 4.0, "max": 6.0}}))?;
        let before = s.inspect();
        for bad in [
            json!({"id": id}),
            json!({"id": id, "min": 6.0, "max": 4.0}),
            json!({"id": id, "min": 1001.0}),
            json!({"id": id, "max": -1001.0}),
            json!({"id": id, "target": 5.0}),
            json!({"id": id, "tolerance_percent": 5.0}),
            json!({"id": id, "target": 5.0, "tolerance_percent": 101.0}),
            json!({"id": id, "target": 5.0, "tolerance_percent": -1.0}),
            json!({"id": id, "target": 5.0, "tolerance_percent": 5.0, "min": 1.0}),
            json!({"id": id, "clear": true, "min": 1.0}),
            json!({"id": id, "clear": false}),
            json!({"id": 999_999, "min": 1.0}),
            json!({"id": 999_999, "clear": true}),
            json!({"id": id, "min": "five"}),
            json!({"id": id, "min": 1.0, "extra": 1}),
        ] {
            assert!(
                s.execute("probe.expect", bad.clone(), None).is_err(),
                "{bad} must be refused"
            );
        }
        assert_eq!(s.inspect(), before, "refusals change nothing");
        let vcc = net(&s, "VCC")?;
        let plain = probe_on_mid(&mut s, json!({"net": vcc}))?;
        let before = s.inspect();
        assert!(
            s.execute("probe.expect", json!({"id": plain, "clear": true}), None)
                .is_err(),
            "there is nothing to clear"
        );
        assert_eq!(s.inspect(), before);
        Ok(())
    }

    #[test]
    fn a_probe_can_be_added_with_its_range_and_a_bad_range_adds_nothing() -> Result<(), Error> {
        let mut s = divider()?;
        let mid = net(&s, "MID")?;
        probe_on_mid(
            &mut s,
            json!({"expect": {"target": 5.0, "tolerance_percent": 2.0}}),
        )?;
        let range = s.project.probes[0].expect.ok_or_else(missing)?;
        assert_eq!((range.min, range.max), (Some(4.9), Some(5.1)));
        s.undo()?;
        assert!(
            s.project.probes.is_empty(),
            "one undo removes the probe and its range"
        );
        let before = s.inspect();
        for bad in [
            json!({"net": mid, "expect": {}}),
            json!({"net": mid, "expect": {"min": 9.0, "max": 1.0}}),
            json!({"net": mid, "expect": {"target": 5.0}}),
            json!({"net": mid, "expect": {"min": 2000.0}}),
            json!({"net": mid, "expect": {"other": 1}}),
        ] {
            assert!(
                s.execute("probe.add", bad.clone(), None).is_err(),
                "{bad} must be refused"
            );
        }
        assert_eq!(s.inspect(), before);
        Ok(())
    }

    #[test]
    fn the_divider_passes_around_five_volts_and_fails_away_from_it() -> Result<(), Error> {
        let mut s = divider()?;
        let id = probe_on_mid(&mut s, json!({}))?;
        let (status, _) = status_of(&mut s)?;
        assert_eq!(status, "unchecked", "no range, no verdict");
        s.execute(
            "probe.expect",
            json!({"id": id, "min": 4.9, "max": 5.1}),
            None,
        )?;
        let (status, row) = status_of(&mut s)?;
        assert_eq!(status, "pass");
        assert_eq!(row["status_reason"], "Reads 5 V, inside 4.9 to 5.1 V.");
        assert_eq!(row["volts"], 5.0);
        s.execute(
            "probe.expect",
            json!({"id": id, "min": 5.2, "max": 5.4}),
            None,
        )?;
        let (status, row) = status_of(&mut s)?;
        assert_eq!(status, "fail");
        assert_eq!(row["status_reason"], "Reads 5 V, outside 5.2 to 5.4 V.");
        s.execute("probe.expect", json!({"id": id, "min": 5.0}), None)?;
        assert_eq!(
            status_of(&mut s)?.0,
            "pass",
            "a reading on the bound passes"
        );
        Ok(())
    }

    #[test]
    fn with_no_source_every_probe_is_unchecked_and_says_why() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("net.create", json!({"name": "A"}), None)?;
        let a = net(&s, "A")?;
        s.execute(
            "probe.add",
            json!({"net": a, "expect": {"min": 1.0, "max": 2.0}}),
            None,
        )?;
        let (status, row) = status_of(&mut s)?;
        assert_eq!(status, "unchecked");
        let why = row["status_reason"].as_str().ok_or_else(missing)?;
        assert!(
            why.contains("ground"),
            "the sentence names the ground: {why}"
        );
        assert!(row["volts"].is_null());
        assert!(range_findings(&s).is_empty(), "no finding without a solve");
        Ok(())
    }

    #[test]
    fn the_finding_follows_the_range_and_the_severity_setting() -> Result<(), Error> {
        let mut s = divider()?;
        let id = probe_on_mid(&mut s, json!({}))?;
        assert!(range_findings(&s).is_empty(), "no range, no finding");
        s.execute(
            "probe.expect",
            json!({"id": id, "min": 4.9, "max": 5.1}),
            None,
        )?;
        assert!(
            range_findings(&s).is_empty(),
            "inside the range, no finding"
        );
        s.execute(
            "probe.expect",
            json!({"id": id, "min": 5.2, "max": 5.4}),
            None,
        )?;
        let found = range_findings(&s);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["severity"], "warning", "warning by default");
        let message = found[0]["message"].as_str().ok_or_else(missing)?;
        assert!(
            message.contains("P1") && message.contains("5 V") && message.contains("5.2 to 5.4 V"),
            "the message names the probe, the reading and the range: {message}"
        );
        s.execute(
            "rules.severity",
            json!({"code": "probe_out_of_range", "severity": "error"}),
            None,
        )?;
        assert_eq!(range_findings(&s)[0]["severity"], "error");
        s.execute(
            "rules.severity",
            json!({"code": "probe_out_of_range", "severity": "off"}),
            None,
        )?;
        assert!(range_findings(&s).is_empty(), "off hides the finding");
        s.execute(
            "rules.severity",
            json!({"code": "probe_out_of_range"}),
            None,
        )?;
        assert_eq!(
            range_findings(&s).len(),
            1,
            "removing the override brings it back"
        );
        s.execute(
            "probe.expect",
            json!({"id": id, "min": 4.0, "max": 6.0}),
            None,
        )?;
        assert!(
            range_findings(&s).is_empty(),
            "back in range, the finding goes"
        );
        s.execute("probe.expect", json!({"id": id, "min": 6.0}), None)?;
        assert_eq!(range_findings(&s).len(), 1);
        s.execute("probe.expect", json!({"id": id, "clear": true}), None)?;
        assert!(range_findings(&s).is_empty(), "clearing the range ends it");
        Ok(())
    }

    #[test]
    fn the_rules_list_publishes_the_code() {
        assert!(
            pcbcraft_rules::CHECK_CODES
                .iter()
                .any(|(code, level)| *code == "probe_out_of_range" && *level == "warning")
        );
    }
}
