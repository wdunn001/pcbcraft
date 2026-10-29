//! Impedance estimates, length-match groups and the meander length tuner.
use super::*;
use pcbcraft_geom::impedance as imp;
use pcbcraft_geom::meander::{self, Corner, Run, Seg};
use std::collections::BTreeSet;

/// Copper thickness used when the stackup records none for a layer.
const DEFAULT_COPPER_MM: f64 = 0.035;
const VIA_NOTE: &str =
    "Lengths count copper tracks only; via barrels and pad interiors are not included.";

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "net.impedance",
            description: "Report the estimated single-ended microstrip impedance of each routed net from its widest track, the recorded stackup and the recorded copper thickness, with effective permittivity and delay per millimetre; a net in a differential pair also reports its differential impedance at the pair gap. With target (ohms) it also reports the width that would meet it by bisection on the same model, and target_diff does the same for the differential impedance",
            input_schema: schema(
                json!({"net":id_schema(),"target":{"type":"number","minimum":1,"maximum":1000},"target_diff":{"type":"number","minimum":1,"maximum":2000}}),
                &[],
            ),
        },
        CommandSpec {
            id: "impedance.calc",
            description: "Pure impedance calculator for one geometry, touching nothing in the design: microstrip, coupled (edge-coupled pair, reports odd, even and differential), or stripline (strip centred between two planes, height is the plane to plane spacing); height and er default to the board stackup; with target it also solves the width by bisection",
            input_schema: schema(
                json!({"geometry":{"type":"string","enum":["microstrip","coupled","stripline"]},"width":{"type":"number","exclusiveMinimum":0,"maximum":100},"gap":{"type":"number","exclusiveMinimum":0,"maximum":100,"description":"edge to edge gap of a coupled pair, millimetres"},"height":{"type":"number","exclusiveMinimum":0,"maximum":100},"thickness":{"type":"number","minimum":0,"maximum":1},"er":{"type":"number","minimum":1,"maximum":100},"target":{"type":"number","minimum":1,"maximum":2000,"description":"ohms; differential for a coupled pair"}}),
                &["geometry", "width"],
            ),
        },
        CommandSpec {
            id: "board.set_length_group",
            description: "Declare a named length-match group of two to 32 nets whose routed lengths must stay within a tolerance of each other; the same name updates the group, and DRC reports a length_group_skew finding when the longest and shortest net differ by more than the tolerance",
            input_schema: schema(
                json!({"name":{"type":"string","maxLength":64},"nets":{"type":"array","minItems":2,"maxItems":32,"items":id_schema()},"tolerance":{"type":"number","minimum":0,"maximum":100,"description":"allowed spread between longest and shortest net, millimetres"},"id":id_schema()}),
                &["name", "nets", "tolerance"],
            ),
        },
        CommandSpec {
            id: "board.remove_length_group",
            description: "Remove a length-match group by id; the nets and their copper stay",
            input_schema: schema(json!({"id":id_schema()}), &["id"]),
        },
        CommandSpec {
            id: "net.length_groups",
            description: "Report each length-match group: every net's routed length and delay, how far each falls short of the longest net, the spread, and whether the group is inside its tolerance; read only",
            input_schema: schema(json!({"id":id_schema()}), &[]),
        },
        CommandSpec {
            id: "track.tune_length",
            description: "Replace one straight track with a serpentine so the net's total routed length reaches a target within a tolerance. Bumps rise on whichever side has room, legs are spaced at least three dielectric heights apart by default and no longer than fifteen heights, corners are 45 degree mitres or rounded. Refuses curved tracks, differential pair nets, nets already at or beyond the target, and runs with no clear room, leaving the design untouched. Reports before and after length and delay",
            input_schema: schema(
                json!({"track":id_schema(),"target":{"type":"number","minimum":0.1,"maximum":10000,"description":"wanted total routed length of the net, millimetres"},"tolerance":{"type":"number","minimum":0.01,"maximum":10,"description":"allowed miss, millimetres; default 0.1"},"amplitude_max":{"type":"number","minimum":0.1,"maximum":10,"description":"tallest bump, millimetres; default 10, capped at fifteen dielectric heights"},"spacing":{"type":"number","exclusiveMinimum":0,"maximum":50,"description":"edge to edge gap between legs, millimetres; default the larger of three dielectric heights and clearance plus width; a value under three heights is accepted with a warning"},"style":{"type":"string","enum":["trombone","rounded"]}}),
                &["track", "target"],
            ),
        },
    ]
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}
/// Copper thickness for a layer and where it came from.
fn copper_thickness(p: &Project, layer: Layer) -> (f64, String) {
    let name = p.layer_name(layer);
    match p
        .board
        .layer_records
        .iter()
        .find(|r| r.name == name && r.kind == "copper" && r.thickness > 0.0)
    {
        Some(record) => (record.thickness, format!("recorded for {name}")),
        None => (
            DEFAULT_COPPER_MM,
            format!("default {DEFAULT_COPPER_MM} mm, none recorded for {name}"),
        ),
    }
}
fn net_length(p: &Project, net: Id) -> f64 {
    p.board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(Track::length)
        .sum()
}
fn widest(p: &Project, net: Id) -> Option<(f64, Layer)> {
    p.board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .fold(None, |best: Option<(f64, Layer)>, t| match best {
            Some((w, _)) if w >= t.width => best,
            _ => Some((t.width, t.layer)),
        })
}
fn refuse(text: &str) -> Error {
    Error::Invalid(text.into())
}
fn single(width: f64, height: f64, thickness: f64, er: f64) -> Result<imp::Microstrip, Error> {
    imp::microstrip(width, height, thickness, er).ok_or_else(|| {
        refuse("the stackup or track width is outside what the impedance model accepts")
    })
}
fn width_for(target: f64, height: f64, thickness: f64, er: f64) -> Option<f64> {
    imp::width_for_ohms(target, |w| {
        imp::microstrip(w, height, thickness, er).map(|m| m.ohms)
    })
}
fn coupled_validity(width: f64, gap: f64, height: f64) -> Option<String> {
    let (u, g) = (width / height, gap / height);
    if !(0.1..=10.0).contains(&u) || g < 0.01 {
        Some(format!(
            "width/height {u:.2} and gap/height {g:.2} fall outside the coupled model's stated range (0.1 to 10 and at least 0.01)"
        ))
    } else {
        None
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImpedanceParams {
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    target: Option<f64>,
    #[serde(default)]
    target_diff: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalcParams {
    geometry: String,
    width: f64,
    #[serde(default)]
    gap: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    thickness: Option<f64>,
    #[serde(default)]
    er: Option<f64>,
    #[serde(default)]
    target: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupParams {
    name: String,
    nets: Vec<Id>,
    tolerance: f64,
    #[serde(default)]
    id: Option<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupQuery {
    #[serde(default)]
    id: Option<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TuneParams {
    track: Id,
    target: f64,
    #[serde(default)]
    tolerance: Option<f64>,
    #[serde(default)]
    amplitude_max: Option<f64>,
    #[serde(default)]
    spacing: Option<f64>,
    #[serde(default)]
    style: Option<String>,
}

pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    Some(match command {
        "net.impedance" => net_impedance(p, params),
        "impedance.calc" => calc(p, params),
        "board.set_length_group" => set_group(p, next, params),
        "board.remove_length_group" => remove_group(p, params),
        "net.length_groups" => group_report(p, params),
        "track.tune_length" => tune(p, next, params),
        _ => return None,
    })
}

fn net_impedance(p: &Project, params: &Value) -> Result<Value, Error> {
    let a: ImpedanceParams = parse(params)?;
    if a.net.is_some_and(|id| !p.nets.iter().any(|n| n.id == id)) {
        return Err(missing());
    }
    if a.target.is_some_and(|v| !(1.0..=1000.0).contains(&v)) {
        return Err(refuse("target must be 1-1000 ohms"));
    }
    if a.target_diff.is_some_and(|v| !(1.0..=2000.0).contains(&v)) {
        return Err(refuse("target_diff must be 1-2000 ohms"));
    }
    let (height, er) = (p.board.thickness, p.board.er);
    let mut rows: Vec<Value> = vec![];
    for net in &p.nets {
        if a.net.is_some_and(|id| id != net.id) {
            continue;
        }
        let Some((width, layer)) = widest(p, net.id) else {
            continue;
        };
        let (copper, source) = copper_thickness(p, layer);
        let m = single(width, height, copper, er)?;
        let mut row = json!({
            "net": net.id,
            "net_name": net.name,
            "width": width,
            "layer": p.layer_name(layer),
            "impedance_ohm": r2(m.ohms),
            "eeff": r3(m.eeff),
            "delay_ps_per_mm": r3(imp::delay_ps(1.0, m.eeff)),
            "copper_mm": copper,
            "copper_source": source,
        });
        if let Some(note) = imp::validity(width, height) {
            row["validity"] = json!(note);
        }
        if let Some(target) = a.target {
            row["delta_ohm"] = json!(r2(m.ohms - target));
            match width_for(target, height, copper, er) {
                Some(w) => row["recommended_width"] = json!(r3(w)),
                None => {
                    row["recommended_width"] = Value::Null;
                    row["advice"] = json!(
                        "no width from 0.01 to 100 mm reaches that impedance on this stackup"
                    );
                }
            }
        }
        if let Some(pair) = p
            .board
            .diff_pairs
            .iter()
            .find(|d| d.net_p == net.id || d.net_n == net.id)
            && let Some(c) = imp::coupled_microstrip(width, pair.gap, height, er)
        {
            let mut diff = json!({
                "pair": pair.name,
                "gap": pair.gap,
                "z_diff_ohm": r2(c.z_diff()),
                "z_odd_ohm": r2(c.z_odd),
                "z_even_ohm": r2(c.z_even),
                "model": "edge-coupled microstrip, zero thickness",
            });
            if let Some(note) = coupled_validity(width, pair.gap, height) {
                diff["validity"] = json!(note);
            }
            if let Some(target) = a.target_diff {
                diff["delta_ohm"] = json!(r2(c.z_diff() - target));
                let solved = imp::width_for_ohms(target / 2.0, |w| {
                    imp::coupled_microstrip(w, pair.gap, height, er).map(|c| c.z_odd)
                });
                diff["recommended_width"] = solved.map_or(Value::Null, |w| json!(r3(w)));
            }
            row["diff"] = diff;
        }
        rows.push(row);
    }
    let count = rows.len();
    Ok(json!({
        "rows": rows,
        "count": count,
        "dielectric": height,
        "er": er,
        "target_ohm": a.target,
        "model": "Hammerstad and Jensen 1980 microstrip with thickness correction",
        "notes": ["Surface microstrip over the reference plane one dielectric away; finish does not change the estimate."],
    }))
}

fn calc(p: &Project, params: &Value) -> Result<Value, Error> {
    let a: CalcParams = parse(params)?;
    let height = a.height.unwrap_or(p.board.thickness);
    let er = a.er.unwrap_or(p.board.er);
    let t = a.thickness.unwrap_or(DEFAULT_COPPER_MM);
    for v in [Some(a.width), a.gap, Some(height), Some(er)]
        .into_iter()
        .flatten()
    {
        if !v.is_finite() || v <= 0.0 || v > 100.0 {
            return Err(refuse(
                "width, gap, height and er must be positive and at most 100",
            ));
        }
    }
    if !t.is_finite() || !(0.0..=1.0).contains(&t) {
        return Err(refuse("thickness must be 0 to 1 millimetre"));
    }
    if a.target.is_some_and(|v| !(1.0..=2000.0).contains(&v)) {
        return Err(refuse("target must be 1-2000 ohms"));
    }
    match a.geometry.as_str() {
        "microstrip" => {
            let m = single(a.width, height, t, er)?;
            let mut out = json!({"geometry":"microstrip","impedance_ohm":r2(m.ohms),"eeff":r3(m.eeff),"delay_ps_per_mm":r3(imp::delay_ps(1.0, m.eeff))});
            if let Some(note) = imp::validity(a.width, height) {
                out["validity"] = json!(note);
            }
            if let Some(target) = a.target {
                out["recommended_width"] =
                    width_for(target, height, t, er).map_or(Value::Null, |w| json!(r3(w)));
            }
            Ok(out)
        }
        "coupled" => {
            let gap = a.gap.ok_or_else(|| refuse("a coupled pair needs a gap"))?;
            let c = imp::coupled_microstrip(a.width, gap, height, er)
                .ok_or_else(|| refuse("the coupled model refused those dimensions"))?;
            let mut out = json!({"geometry":"coupled","z_diff_ohm":r2(c.z_diff()),"z_odd_ohm":r2(c.z_odd),"z_even_ohm":r2(c.z_even),"z_common_ohm":r2(c.z_common()),"ee_odd":r3(c.ee_odd),"ee_even":r3(c.ee_even)});
            if let Some(note) = coupled_validity(a.width, gap, height) {
                out["validity"] = json!(note);
            }
            if let Some(target) = a.target {
                let solved = imp::width_for_ohms(target / 2.0, |w| {
                    imp::coupled_microstrip(w, gap, height, er).map(|c| c.z_odd)
                });
                out["recommended_width"] = solved.map_or(Value::Null, |w| json!(r3(w)));
            }
            Ok(out)
        }
        "stripline" => {
            let z = imp::stripline(a.width, height, t, er)
                .ok_or_else(|| refuse("the stripline model refused those dimensions"))?;
            let mut out = json!({"geometry":"stripline","impedance_ohm":r2(z),"delay_ps_per_mm":r3(imp::delay_ps(1.0, er))});
            if let Some(target) = a.target {
                let solved = imp::width_for_ohms(target, |w| imp::stripline(w, height, t, er));
                out["recommended_width"] = solved.map_or(Value::Null, |w| json!(r3(w)));
            }
            Ok(out)
        }
        _ => Err(refuse("geometry must be microstrip, coupled or stripline")),
    }
}

fn set_group(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: GroupParams = parse(params)?;
    let name = a.name.trim().to_string();
    if name.is_empty() || name.len() > 64 {
        return Err(refuse("a length-match group needs a name of 1 to 64 bytes"));
    }
    let distinct: BTreeSet<Id> = a.nets.iter().copied().collect();
    if !(2..=32).contains(&a.nets.len()) || distinct.len() != a.nets.len() {
        return Err(refuse("a group holds two to 32 different nets"));
    }
    for net in &a.nets {
        if !p.nets.iter().any(|n| n.id == *net) {
            return Err(missing());
        }
    }
    if !a.tolerance.is_finite() || !(0.0..=100.0).contains(&a.tolerance) {
        return Err(refuse("the tolerance must be 0 to 100 millimetres"));
    }
    let groups = &mut p.board.length_groups;
    let target = match a.id {
        Some(id) => {
            if !groups.iter().any(|g| g.id == id) {
                return Err(missing());
            }
            if groups.iter().any(|g| g.id != id && g.name == name) {
                return Err(Error::Invalid(format!(
                    "a group named {name} already exists"
                )));
            }
            groups.iter().position(|g| g.id == id)
        }
        None => groups.iter().position(|g| g.name == name),
    };
    if let Some(index) = target {
        let group = &mut groups[index];
        group.name = name.clone();
        group.nets = a.nets;
        group.tolerance = a.tolerance;
        return Ok(json!({"id": group.id, "name": name}));
    }
    if groups.len() >= 64 {
        return Err(refuse("a board carries at most 64 length-match groups"));
    }
    let id = allocate(next)?;
    groups.push(LengthGroup {
        id,
        name: name.clone(),
        nets: a.nets,
        tolerance: a.tolerance,
    });
    Ok(json!({"id": id, "name": name}))
}

fn remove_group(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: IdParams = parse(params)?;
    let before = p.board.length_groups.len();
    p.board.length_groups.retain(|g| g.id != a.id);
    if p.board.length_groups.len() == before {
        return Err(missing());
    }
    Ok(json!({"id": a.id}))
}

fn group_report(p: &Project, params: &Value) -> Result<Value, Error> {
    let a: GroupQuery = parse(params)?;
    if a.id
        .is_some_and(|id| !p.board.length_groups.iter().any(|g| g.id == id))
    {
        return Err(missing());
    }
    let (height, er) = (p.board.thickness, p.board.er);
    let mut rows = vec![];
    for group in &p.board.length_groups {
        if a.id.is_some_and(|id| id != group.id) {
            continue;
        }
        let lengths: Vec<(Id, f64)> = group.nets.iter().map(|n| (*n, net_length(p, *n))).collect();
        let longest = lengths.iter().map(|l| l.1).fold(0.0f64, f64::max);
        let routed: Vec<f64> = lengths.iter().map(|l| l.1).filter(|l| *l > 0.0).collect();
        let shortest = routed.iter().copied().fold(f64::INFINITY, f64::min);
        let spread = if routed.len() >= 2 {
            longest - shortest
        } else {
            0.0
        };
        let nets: Vec<Value> = lengths
            .iter()
            .map(|(net, length)| {
                let delay = widest(p, *net).and_then(|(w, layer)| {
                    let (copper, _) = copper_thickness(p, layer);
                    imp::microstrip(w, height, copper, er).map(|m| imp::delay_ps(*length, m.eeff))
                });
                json!({
                    "net": net,
                    "net_name": p.net_name(*net),
                    "length_mm": r3(*length),
                    "delay_ps": delay.map(r2),
                    "short_by_mm": r3(longest - length),
                })
            })
            .collect();
        rows.push(json!({
            "id": group.id,
            "name": group.name,
            "tolerance": group.tolerance,
            "nets": nets,
            "longest_mm": r3(longest),
            "spread_mm": r3(spread),
            "unrouted_nets": lengths.iter().filter(|l| l.1 <= 0.0).count(),
            "within_tolerance": spread <= group.tolerance + 1e-9,
        }));
    }
    let count = rows.len();
    Ok(
        json!({"groups": rows, "count": count, "notes": [VIA_NOTE, "Delay assumes outer-layer microstrip."]}),
    )
}

/// Remove a net from every length group, or swap it for another net, and
/// drop groups left with fewer than two nets.
pub(super) fn forget_net(p: &mut Project, net: Id, replacement: Option<Id>) {
    for group in p.board.length_groups.iter_mut() {
        if !group.nets.contains(&net) {
            continue;
        }
        group.nets.retain(|n| *n != net);
        if let Some(other) = replacement {
            group.nets.push(other);
        }
        group.nets.sort_unstable();
        group.nets.dedup();
    }
    p.board.length_groups.retain(|g| g.nets.len() >= 2);
}

fn build_tracks(path: &[Seg], original: &Track, probe: &mut Id) -> Result<Vec<Track>, Error> {
    let mut out = Vec::new();
    for seg in path {
        if seg.length() < 1e-6 {
            continue;
        }
        let (start, end, arc) = match *seg {
            Seg::Line(a, b) => (a, b, None),
            Seg::Arc(a, m, b) => (a, b, Some(m)),
        };
        out.push(Track {
            id: allocate(probe)?,
            net: original.net,
            start,
            end,
            width: original.width,
            layer: original.layer,
            arc,
        });
    }
    Ok(out)
}

/// Findings the new copper causes that the original track did not already
/// cause with the same neighbour.
fn new_conflicts(
    trial: &Project,
    tracks: &[Track],
    inherited: &BTreeSet<Id>,
) -> Vec<pcbcraft_rules::Finding> {
    let ids: Vec<Id> = tracks.iter().map(|t| t.id).collect();
    pcbcraft_rules::copper_conflicts(trial, &ids)
        .into_iter()
        .filter(|f| {
            !f.objects
                .iter()
                .any(|o| !ids.contains(o) && inherited.contains(o))
        })
        .collect()
}

fn tune(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: TuneParams = parse(params)?;
    if !a.target.is_finite() || !(0.1..=10_000.0).contains(&a.target) {
        return Err(refuse("target must be 0.1 to 10000 millimetres"));
    }
    let tolerance = a.tolerance.unwrap_or(0.1);
    if !tolerance.is_finite() || !(0.01..=10.0).contains(&tolerance) {
        return Err(refuse("tolerance must be 0.01 to 10 millimetres"));
    }
    if a.amplitude_max
        .is_some_and(|v| !v.is_finite() || !(0.1..=10.0).contains(&v))
    {
        return Err(refuse("amplitude_max must be 0.1 to 10 millimetres"));
    }
    if a.spacing
        .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 50.0)
    {
        return Err(refuse("spacing must be above 0 and at most 50 millimetres"));
    }
    let corner = match a.style.as_deref() {
        None | Some("trombone") => Corner::Mitre,
        Some("rounded") => Corner::Round,
        Some(_) => return Err(refuse("style must be trombone or rounded")),
    };
    let index = p
        .board
        .tracks
        .iter()
        .position(|t| t.id == a.track)
        .ok_or_else(missing)?;
    let track = p.board.tracks[index].clone();
    if track.arc.is_some() {
        return Err(refuse(
            "curved tracks cannot be tuned; straighten them first",
        ));
    }
    if track.net == 0 {
        return Err(refuse(
            "the track belongs to no net, so it has no length target",
        ));
    }
    if p.board
        .diff_pairs
        .iter()
        .any(|d| d.net_p == track.net || d.net_n == track.net)
    {
        return Err(refuse(
            "this net belongs to a differential pair; tuning one side alone would not keep the coupling gap, so pair tuning is not supported yet",
        ));
    }
    let before = net_length(p, track.net);
    let extra = a.target - before;
    if extra < -tolerance {
        return Err(Error::Invalid(format!(
            "this net is already {:.3} mm longer than its target; shorten the route instead",
            -extra
        )));
    }
    if extra.abs() <= tolerance {
        return Err(refuse(
            "this net is already within the tolerance of its target",
        ));
    }
    let (height, er) = (p.board.thickness, p.board.er);
    let (copper, _) = copper_thickness(p, track.layer);
    let clearance = p
        .net_class(track.net)
        .map_or(p.board.clearance, |c| c.clearance.max(p.board.clearance));
    let default_spacing = (3.0 * height).max(clearance + track.width);
    let floor = (3.0 * track.width).max(clearance);
    let spacing = a.spacing.unwrap_or(default_spacing);
    if spacing + 1e-9 < floor {
        return Err(Error::Invalid(format!(
            "spacing must be at least {floor:.3} mm here: three track widths and the required clearance"
        )));
    }
    let amp_max = a.amplitude_max.unwrap_or(10.0).min(15.0 * height);
    if amp_max < meander::MIN_AMPLITUDE {
        return Err(refuse(
            "the dielectric is too thin to allow any bump height",
        ));
    }
    let mut warnings: Vec<String> = vec![];
    if spacing + 1e-9 < 3.0 * height {
        warnings.push(format!(
            "spacing {spacing:.2} mm is under three dielectric heights ({:.2} mm); neighbouring legs couple, so the real delay is shorter than the copper length suggests",
            3.0 * height
        ));
    }
    let base = Run {
        start: track.start,
        end: track.end,
        width: track.width,
        spacing,
        corner,
        side: 1,
    };
    let run_len = track.start.distance(track.end);
    let nmax = base.max_bumps(meander::MIN_AMPLITUDE);
    if nmax == 0 {
        return Err(Error::Invalid(format!(
            "the track is {run_len:.2} mm long, too short to hold a meander at {spacing:.2} mm spacing"
        )));
    }
    let inherited: BTreeSet<Id> = pcbcraft_rules::copper_conflicts(p, &[track.id])
        .iter()
        .flat_map(|f| f.objects.clone())
        .filter(|o| *o != track.id)
        .collect();
    let pick = |run: &Run, n: usize| -> Option<f64> {
        meander::amplitude_for(run, n, extra, amp_max).or_else(|| {
            let low = meander::added_length(run, n, meander::MIN_AMPLITUDE)?;
            (low > extra && low - extra <= tolerance).then_some(meander::MIN_AMPLITUDE)
        })
    };
    for n in 1..=nmax {
        for side in [1i8, -1i8] {
            let run = Run { side, ..base };
            let Some(amplitude) = pick(&run, n) else {
                continue;
            };
            let Some(path) = meander::shape(&run, n, amplitude) else {
                continue;
            };
            let added = meander::path_length(&path) - run_len;
            if (before + added - a.target).abs() > tolerance + 1e-9 {
                continue;
            }
            let mut probe = *next;
            let tracks = build_tracks(&path, &track, &mut probe)?;
            let mut trial = p.clone();
            trial.board.tracks.remove(index);
            trial.board.tracks.extend(tracks.iter().cloned());
            if !new_conflicts(&trial, &tracks, &inherited).is_empty() {
                continue;
            }
            *p = trial;
            *next = probe;
            let after = net_length(p, track.net);
            let eeff = imp::microstrip(track.width, height, copper, er).map(|m| m.eeff);
            let ids: Vec<Id> = tracks.iter().map(|t| t.id).collect();
            return Ok(json!({
                "before": r3(before),
                "after": r3(after),
                "target": a.target,
                "tolerance": tolerance,
                "ids": ids,
                "removed": a.track,
                "bumps": n,
                "amplitude": r3(amplitude),
                "spacing": spacing,
                "side": if side > 0 { "left" } else { "right" },
                "style": if corner == Corner::Mitre { "trombone" } else { "rounded" },
                "delay_ps_before": eeff.map(|e| r2(imp::delay_ps(before, e))),
                "delay_ps_after": eeff.map(|e| r2(imp::delay_ps(after, e))),
                "warnings": warnings,
                "notes": [VIA_NOTE, "Delay assumes outer-layer microstrip over the recorded stackup."],
            }));
        }
    }
    // Nothing fit: report how much the best clean attempt would add.
    let mut best = 0.0f64;
    let mut tries = vec![1, (nmax / 2).max(1), nmax];
    tries.dedup();
    for n in tries {
        for side in [1i8, -1i8] {
            let run = Run { side, ..base };
            let clean = |amplitude: f64| -> Result<Option<f64>, Error> {
                let Some(path) = meander::shape(&run, n, amplitude) else {
                    return Ok(None);
                };
                let mut probe = *next;
                let tracks = build_tracks(&path, &track, &mut probe)?;
                let mut trial = p.clone();
                trial.board.tracks.remove(index);
                trial.board.tracks.extend(tracks.iter().cloned());
                if new_conflicts(&trial, &tracks, &inherited).is_empty() {
                    Ok(Some(meander::path_length(&path) - run_len))
                } else {
                    Ok(None)
                }
            };
            let (mut lo, mut hi) = (meander::MIN_AMPLITUDE, amp_max);
            let Some(mut got) = clean(lo)? else {
                continue;
            };
            if let Some(top) = clean(hi)? {
                best = best.max(top);
                continue;
            }
            for _ in 0..10 {
                let mid = (lo + hi) / 2.0;
                match clean(mid)? {
                    Some(added) => {
                        lo = mid;
                        got = added;
                    }
                    None => hi = mid,
                }
            }
            best = best.max(got);
        }
    }
    Err(Error::Invalid(format!(
        "no clear room for the meander: the best clean fit adds {best:.3} mm of the {extra:.3} mm needed, short by {:.3} mm",
        (extra - best).max(0.0)
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    type T = Result<(), Error>;

    fn id_of(v: &Value) -> Result<Id, Error> {
        v["result"]["id"].as_u64().ok_or_else(missing)
    }
    fn num(v: &Value) -> Result<f64, Error> {
        v.as_f64().ok_or_else(missing)
    }
    /// A session with net SIG and one straight track from x 10 to 10 + length.
    fn fixture(length: f64) -> Result<(Session, Id, Id), Error> {
        let mut s = Session::default();
        s.execute("board.resize", json!({"width":200.0,"height":100.0}), None)?;
        let net = id_of(&s.execute("net.create", json!({"name":"SIG"}), None)?)?;
        let track = id_of(&s.execute(
            "track.add",
            json!({"net":net,"start":{"x":10.0,"y":40.0},"end":{"x":10.0 + length,"y":40.0},"width":0.3}),
            None,
        )?)?;
        Ok((s, net, track))
    }
    fn codes(s: &Session) -> Vec<&'static str> {
        pcbcraft_rules::drc(&s.project)
            .iter()
            .map(|f| f.code)
            .collect()
    }
    fn total(s: &Session, net: Id) -> f64 {
        net_length(&s.project, net)
    }

    #[test]
    fn tuning_a_short_net_lands_within_tolerance_and_checks_stay_clean() -> T {
        let (mut s, net, track) = fixture(60.0)?;
        s.execute(
            "net.set_length",
            json!({"id":net,"target":70.0,"tolerance":0.1}),
            None,
        )?;
        assert!(codes(&s).contains(&"track_length"));
        let out = s.execute(
            "track.tune_length",
            json!({"track":track,"target":70.0,"tolerance":0.1}),
            None,
        )?;
        let r = &out["result"];
        assert!((num(&r["after"])? - 70.0).abs() <= 0.1, "{r}");
        assert!((total(&s, net) - 70.0).abs() <= 0.1);
        let found = codes(&s);
        assert!(!found.contains(&"track_length"), "{found:?}");
        assert!(
            !found.contains(&"clearance") && !found.contains(&"copper_short"),
            "{found:?}"
        );
        assert!(s.project.validate().is_ok());
        Ok(())
    }

    #[test]
    fn tuning_reports_before_and_after_length_and_delay() -> T {
        let (mut s, _, track) = fixture(60.0)?;
        let r = s.execute(
            "track.tune_length",
            json!({"track":track,"target":68.0}),
            None,
        )?["result"]
            .clone();
        assert_eq!(r["before"], 60.0);
        assert!((num(&r["after"])? - 68.0).abs() <= 0.1);
        let (db, da) = (num(&r["delay_ps_before"])?, num(&r["delay_ps_after"])?);
        assert!(da > db && db > 300.0, "{db} {da}");
        assert!(r["notes"].to_string().contains("via barrels"));
        assert!(r["ids"].as_array().is_some_and(|a| a.len() > 4));
        Ok(())
    }

    #[test]
    fn default_spacing_is_three_dielectric_heights_and_small_spacing_warns() -> T {
        let (mut s, _, track) = fixture(60.0)?;
        let r = s.execute(
            "track.tune_length",
            json!({"track":track,"target":66.0}),
            None,
        )?["result"]
            .clone();
        assert!((num(&r["spacing"])? - 4.8).abs() < 1e-9);
        assert!(r["warnings"].as_array().is_some_and(|w| w.is_empty()));
        let (mut s, _, track) = fixture(60.0)?;
        let r = s.execute(
            "track.tune_length",
            json!({"track":track,"target":66.0,"spacing":1.2}),
            None,
        )?["result"]
            .clone();
        assert!(
            r["warnings"]
                .to_string()
                .contains("three dielectric heights")
        );
        Ok(())
    }

    #[test]
    fn meander_spacing_never_drops_below_three_widths() -> T {
        let (mut s, net, track) = fixture(60.0)?;
        assert!(
            s.execute(
                "track.tune_length",
                json!({"track":track,"target":66.0,"spacing":0.5}),
                None
            )
            .is_err()
        );
        assert_eq!(total(&s, net), 60.0);
        let r = s.execute(
            "track.tune_length",
            json!({"track":track,"target":66.0,"spacing":0.9}),
            None,
        )?["result"]
            .clone();
        let ids: Vec<Id> = r["ids"]
            .as_array()
            .ok_or_else(missing)?
            .iter()
            .filter_map(Value::as_u64)
            .collect();
        let legs: Vec<f64> = s
            .project
            .board
            .tracks
            .iter()
            .filter(|t| {
                ids.contains(&t.id) && (t.start.x - t.end.x).abs() < 1e-9 && t.length() > 0.3
            })
            .map(|t| t.start.x)
            .collect();
        let mut xs = legs.clone();
        xs.sort_by(f64::total_cmp);
        for pair in xs.windows(2) {
            assert!(pair[1] - pair[0] - 0.3 >= 0.9 - 1e-6, "{pair:?}");
        }
        Ok(())
    }

    #[test]
    fn rounded_style_uses_arcs() -> T {
        let (mut s, net, track) = fixture(60.0)?;
        s.execute(
            "track.tune_length",
            json!({"track":track,"target":70.0,"style":"rounded"}),
            None,
        )?;
        assert!(s.project.board.tracks.iter().any(|t| t.arc.is_some()));
        assert!((total(&s, net) - 70.0).abs() <= 0.1);
        assert!(!codes(&s).contains(&"clearance"));
        Ok(())
    }

    #[test]
    fn tuning_refuses_curved_tracks() -> T {
        let (mut s, net, track) = fixture(60.0)?;
        s.execute("track.add_arc", json!({"net":net,"start":{"x":10.0,"y":50.0},"mid":{"x":20.0,"y":55.0},"end":{"x":30.0,"y":50.0}}), None)?;
        let arc = s
            .project
            .board
            .tracks
            .iter()
            .find(|t| t.arc.is_some())
            .map(|t| t.id)
            .ok_or_else(missing)?;
        let err = s
            .execute(
                "track.tune_length",
                json!({"track":arc,"target":200.0}),
                None,
            )
            .err()
            .ok_or_else(missing)?;
        assert!(err.to_string().contains("straighten them first"));
        assert_eq!(s.project.board.tracks.len(), 2);
        assert!(track > 0);
        Ok(())
    }

    #[test]
    fn tuning_refuses_when_already_too_long_or_within_tolerance() -> T {
        let (mut s, _, track) = fixture(60.0)?;
        let err = s
            .execute(
                "track.tune_length",
                json!({"track":track,"target":50.0}),
                None,
            )
            .err()
            .ok_or_else(missing)?;
        assert!(
            err.to_string().contains("already 10.000 mm longer"),
            "{err}"
        );
        let err = s
            .execute(
                "track.tune_length",
                json!({"track":track,"target":60.05}),
                None,
            )
            .err()
            .ok_or_else(missing)?;
        assert!(err.to_string().contains("within the tolerance"));
        assert_eq!(s.project.board.tracks.len(), 1);
        Ok(())
    }

    #[test]
    fn tuning_validates_every_bound() -> T {
        let (mut s, _, track) = fixture(60.0)?;
        for bad in [
            json!({"track":track,"target":0.0}),
            json!({"track":track,"target":20000.0}),
            json!({"track":track,"target":70.0,"tolerance":0.0}),
            json!({"track":track,"target":70.0,"tolerance":11.0}),
            json!({"track":track,"target":70.0,"amplitude_max":0.0}),
            json!({"track":track,"target":70.0,"amplitude_max":11.0}),
            json!({"track":track,"target":70.0,"spacing":-1.0}),
            json!({"track":track,"target":70.0,"spacing":60.0}),
            json!({"track":track,"target":70.0,"style":"zigzag"}),
            json!({"track":9999,"target":70.0}),
            json!({"track":track,"target":70.0,"extra":1}),
        ] {
            assert!(
                s.execute("track.tune_length", bad.clone(), None).is_err(),
                "{bad}"
            );
        }
        assert_eq!(s.project.board.tracks.len(), 1);
        Ok(())
    }

    #[test]
    fn tuning_refuses_with_no_room_and_leaves_the_design() -> T {
        let (mut s, net, track) = fixture(60.0)?;
        // Foreign copper fences the run in on both sides.
        let other = id_of(&s.execute("net.create", json!({"name":"BLOCK"}), None)?)?;
        for y in [41.0, 39.0] {
            s.execute(
                "track.add",
                json!({"net":other,"start":{"x":0.0,"y":y},"end":{"x":90.0,"y":y},"width":0.3}),
                None,
            )?;
        }
        let before = s.project.clone();
        let err = s
            .execute(
                "track.tune_length",
                json!({"track":track,"target":70.0}),
                None,
            )
            .err()
            .ok_or_else(missing)?;
        assert!(
            err.to_string().contains("no clear room") && err.to_string().contains("short by"),
            "{err}"
        );
        assert_eq!(s.project, before);
        assert_eq!(total(&s, net), 60.0);
        Ok(())
    }

    #[test]
    fn tuning_picks_the_side_with_room() -> T {
        let (mut s, _, track) = fixture(60.0)?;
        let other = id_of(&s.execute("net.create", json!({"name":"BLOCK"}), None)?)?;
        s.execute(
            "track.add",
            json!({"net":other,"start":{"x":0.0,"y":41.0},"end":{"x":90.0,"y":41.0},"width":0.3}),
            None,
        )?;
        let r = s.execute(
            "track.tune_length",
            json!({"track":track,"target":70.0}),
            None,
        )?["result"]
            .clone();
        let ys: Vec<f64> = s
            .project
            .board
            .tracks
            .iter()
            .filter(|t| t.net != other)
            .flat_map(|t| [t.start.y, t.end.y])
            .collect();
        let low = ys.iter().copied().fold(f64::INFINITY, f64::min);
        let high = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            low < 40.0 && high <= 40.0 + 1e-6,
            "bumps went to the open side: {r}"
        );
        assert!(!codes(&s).contains(&"clearance"));
        Ok(())
    }

    #[test]
    fn tuning_refuses_a_differential_pair_net() -> T {
        let (mut s, net, track) = fixture(60.0)?;
        let n = id_of(&s.execute("net.create", json!({"name":"SIGN"}), None)?)?;
        s.execute(
            "board.set_diffpair",
            json!({"name":"D","net_p":net,"net_n":n,"gap":0.3}),
            None,
        )?;
        let err = s
            .execute(
                "track.tune_length",
                json!({"track":track,"target":70.0}),
                None,
            )
            .err()
            .ok_or_else(missing)?;
        assert!(err.to_string().contains("differential pair"));
        Ok(())
    }

    #[test]
    fn tuning_is_one_undo_step() -> T {
        let (mut s, net, track) = fixture(60.0)?;
        let before = s.project.clone();
        let depth = s.undo.len();
        s.execute(
            "track.tune_length",
            json!({"track":track,"target":70.0}),
            None,
        )?;
        assert_eq!(s.undo.len(), depth + 1);
        s.undo()?;
        assert_eq!(s.project, before);
        assert_eq!(total(&s, net), 60.0);
        Ok(())
    }

    #[test]
    fn length_group_reports_skew_and_drc_flags_it() -> T {
        let (mut s, a, _) = fixture(60.0)?;
        let b = id_of(&s.execute("net.create", json!({"name":"B"}), None)?)?;
        s.execute(
            "track.add",
            json!({"net":b,"start":{"x":10.0,"y":50.0},"end":{"x":50.0,"y":50.0},"width":0.3}),
            None,
        )?;
        for bad in [
            json!({"name":"","nets":[a,b],"tolerance":0.5}),
            json!({"name":"G","nets":[a],"tolerance":0.5}),
            json!({"name":"G","nets":[a,a],"tolerance":0.5}),
            json!({"name":"G","nets":[a,999],"tolerance":0.5}),
            json!({"name":"G","nets":[a,b],"tolerance":-1.0}),
            json!({"name":"G","nets":[a,b],"tolerance":101.0}),
        ] {
            assert!(
                s.execute("board.set_length_group", bad.clone(), None)
                    .is_err(),
                "{bad}"
            );
        }
        let g = id_of(&s.execute(
            "board.set_length_group",
            json!({"name":"BUS","nets":[a,b],"tolerance":0.5}),
            None,
        )?)?;
        assert!(codes(&s).contains(&"length_group_skew"));
        let report = s.execute("net.length_groups", json!({}), None)?["result"].clone();
        assert_eq!(report["groups"][0]["spread_mm"], 20.0);
        assert_eq!(report["groups"][0]["within_tolerance"], false);
        assert_eq!(report["groups"][0]["nets"][1]["short_by_mm"], 20.0);
        let again = id_of(&s.execute(
            "board.set_length_group",
            json!({"name":"BUS","nets":[a,b],"tolerance":25.0}),
            None,
        )?)?;
        assert_eq!(g, again, "the same name updates the group");
        assert!(!codes(&s).contains(&"length_group_skew"));
        s.execute("board.remove_length_group", json!({"id":g}), None)?;
        assert!(s.project.board.length_groups.is_empty());
        assert!(
            s.execute("board.remove_length_group", json!({"id":g}), None)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn tuning_the_short_net_closes_the_group() -> T {
        let (mut s, a, _) = fixture(60.0)?;
        let b = id_of(&s.execute("net.create", json!({"name":"B"}), None)?)?;
        let short = id_of(&s.execute(
            "track.add",
            json!({"net":b,"start":{"x":10.0,"y":20.0},"end":{"x":50.0,"y":20.0},"width":0.3}),
            None,
        )?)?;
        s.execute(
            "board.set_length_group",
            json!({"name":"BUS","nets":[a,b],"tolerance":0.2}),
            None,
        )?;
        assert!(codes(&s).contains(&"length_group_skew"));
        s.execute(
            "track.tune_length",
            json!({"track":short,"target":60.0,"tolerance":0.1}),
            None,
        )?;
        assert!(!codes(&s).contains(&"length_group_skew"));
        Ok(())
    }

    #[test]
    fn deleting_or_merging_a_net_prunes_groups() -> T {
        let (mut s, a, _) = fixture(60.0)?;
        let b = id_of(&s.execute("net.create", json!({"name":"B"}), None)?)?;
        let c = id_of(&s.execute("net.create", json!({"name":"C"}), None)?)?;
        s.execute(
            "board.set_length_group",
            json!({"name":"G","nets":[a,b,c],"tolerance":1.0}),
            None,
        )?;
        s.execute("net.tie", json!({"source":c,"target":b}), None)?;
        assert_eq!(s.project.board.length_groups[0].nets.len(), 2);
        s.execute("object.delete", json!({"id":b}), None)?;
        assert!(
            s.project.board.length_groups.is_empty(),
            "a group of one net is dropped"
        );
        Ok(())
    }

    #[test]
    fn recorded_copper_thickness_feeds_the_estimate() -> T {
        let (mut s, net, _) = fixture(60.0)?;
        let plain =
            s.execute("net.impedance", json!({"net":net}), None)?["result"]["rows"][0].clone();
        assert_eq!(plain["copper_mm"], 0.035);
        assert!(
            plain["copper_source"]
                .as_str()
                .is_some_and(|t| t.contains("default"))
        );
        s.execute(
            "board.set_layer_record",
            json!({"name":"F.Cu","kind":"copper","thickness":0.07}),
            None,
        )?;
        let thick =
            s.execute("net.impedance", json!({"net":net}), None)?["result"]["rows"][0].clone();
        assert_eq!(thick["copper_mm"], 0.07);
        assert!(
            thick["copper_source"]
                .as_str()
                .is_some_and(|t| t.contains("recorded"))
        );
        assert!(
            num(&thick["impedance_ohm"])? < num(&plain["impedance_ohm"])?,
            "thicker copper lowers impedance"
        );
        Ok(())
    }

    #[test]
    fn impedance_curve_through_the_engine_has_no_step_and_widths_differ() -> T {
        let (mut s, net, track) = fixture(30.0)?;
        let mut last = f64::INFINITY;
        for w in [1.5, 1.58, 1.6, 1.62, 1.7, 1.8] {
            s.execute("track.set_width", json!({"id":track,"width":w}), None)?;
            let z = num(
                &s.execute("net.impedance", json!({"net":net}), None)?["result"]["rows"][0]["impedance_ohm"],
            )?;
            assert!(z < last, "{w}: {z} after {last}");
            assert!(
                !last.is_finite() || last - z < 3.0,
                "step at {w}: {z} after {last}"
            );
            last = z;
        }
        let width = |t: f64| -> Result<f64, Error> {
            num(&s
                .clone()
                .execute("net.impedance", json!({"net":net,"target":t}), None)?["result"]["rows"]
                [0]["recommended_width"])
        };
        assert!(width(60.0)? > width(65.0)? + 0.1 && width(65.0)? > width(70.0)? + 0.1);
        assert!(
            (width(50.0)? - 2.9).abs() < 0.2,
            "50 ohm at 1.6 mm is about 2.9 mm wide"
        );
        Ok(())
    }

    #[test]
    fn diff_pair_nets_report_differential_impedance() -> T {
        let (mut s, net, _) = fixture(30.0)?;
        let n = id_of(&s.execute("net.create", json!({"name":"SIGN"}), None)?)?;
        s.execute(
            "track.add",
            json!({"net":n,"start":{"x":10.0,"y":45.0},"end":{"x":40.0,"y":45.0},"width":0.3}),
            None,
        )?;
        s.execute(
            "board.set_diffpair",
            json!({"name":"D","net_p":net,"net_n":n,"gap":0.3}),
            None,
        )?;
        let r = s.execute(
            "net.impedance",
            json!({"net":net,"target_diff":100.0}),
            None,
        )?["result"]["rows"][0]
            .clone();
        let zd = num(&r["diff"]["z_diff_ohm"])?;
        assert!((zd - 2.0 * num(&r["diff"]["z_odd_ohm"])?).abs() < 0.02);
        assert!(
            r["diff"]["recommended_width"].is_number() || r["diff"]["recommended_width"].is_null()
        );
        assert!(
            r["diff"]["validity"].is_null(),
            "w/h 0.19 and gap/h 0.19 are inside the stated range: {r}"
        );
        Ok(())
    }

    #[test]
    fn calculator_covers_each_geometry_and_refuses_bad_input() -> T {
        let (mut s, _, _) = fixture(30.0)?;
        let m = s.execute(
            "impedance.calc",
            json!({"geometry":"microstrip","width":1.6,"height":1.6,"er":4.5,"target":50.0}),
            None,
        )?["result"]
            .clone();
        assert!((num(&m["impedance_ohm"])? - 69.4).abs() < 0.1);
        assert!((num(&m["recommended_width"])? - 2.966).abs() < 0.01);
        let c = s.execute(
            "impedance.calc",
            json!({"geometry":"coupled","width":0.8,"gap":0.8,"height":1.6,"er":4.5}),
            None,
        )?["result"]
            .clone();
        assert!((num(&c["z_diff_ohm"])? - 136.54).abs() < 0.05);
        let l = s.execute(
            "impedance.calc",
            json!({"geometry":"stripline","width":0.28,"height":1.68,"er":4.5}),
            None,
        )?["result"]
            .clone();
        assert!((num(&l["impedance_ohm"])? - 71.66).abs() < 0.05);
        for bad in [
            json!({"geometry":"coupled","width":0.8}),
            json!({"geometry":"cpw","width":0.8}),
            json!({"geometry":"microstrip","width":-1.0}),
            json!({"geometry":"microstrip","width":1.0,"er":0.2}),
            json!({"geometry":"microstrip","width":1.0,"target":0.5}),
            json!({"geometry":"stripline","width":1.0,"height":0.5,"thickness":0.6}),
        ] {
            assert!(
                s.execute("impedance.calc", bad.clone(), None).is_err(),
                "{bad}"
            );
        }
        Ok(())
    }
}
