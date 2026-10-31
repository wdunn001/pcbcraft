//! Design calculator read commands backed by `pcbcraft-calc` (#174).
use super::*;
use pcbcraft_calc::basic::{VoltageDivider, voltage_divider};
use pcbcraft_calc::copper::{
    CopperLayer, TraceMethod, TraceWidth, ViaCurrent, trace_width, via_current_resistance,
};
use pcbcraft_calc::impedance::{
    CoplanarWaveguide, DifferentialPair, EmbeddedMicrostrip, Microstrip, Stripline, coplanar_waveguide,
    differential_pair, embedded_microstrip, microstrip, stripline,
};
use pcbcraft_calc::meta;
use pcbcraft_calc::utilities::{LengthUnit, UnitConvert, unit_converter};
use pcbcraft_calc::Error as CalcError;

const DEFAULT_COPPER_MM: f64 = 0.035;

fn calc_err(e: CalcError) -> Error {
    Error::Invalid(e.to_string())
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn r4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn meta_block(id: &str) -> Result<Value, Error> {
    let m = meta::calculator(id).ok_or_else(|| Error::Invalid(format!("unknown calculator {id}")))?;
    Ok(json!({
        "id": m.id,
        "name": m.name,
        "group": m.group,
        "summary": m.summary,
        "formula": m.formula,
        "accuracy": m.accuracy,
        "validity": m.validity,
        "source": m.source,
    }))
}

fn copper_thickness(p: &Project, layer_name: &str) -> f64 {
    p.board
        .layer_records
        .iter()
        .find(|r| r.name == layer_name && r.kind == "copper" && r.thickness > 0.0)
        .map(|r| r.thickness)
        .unwrap_or(DEFAULT_COPPER_MM)
}

fn dielectric_for(p: &Project, layer_name: &str) -> (f64, f64) {
    p.dielectric_beside(layer_name)
        .unwrap_or((p.board.thickness, p.board.er))
}

fn parse_layer(name: &str) -> Result<CopperLayer, Error> {
    match name {
        "external" => Ok(CopperLayer::External),
        "internal" => Ok(CopperLayer::Internal),
        _ => Err(Error::Invalid("layer must be external or internal".into())),
    }
}

fn parse_method(name: &str) -> Result<TraceMethod, Error> {
    match name {
        "auto" => Ok(TraceMethod::Auto),
        "ipc2152" => Ok(TraceMethod::Ipc2152),
        "ipc2221" => Ok(TraceMethod::Ipc2221),
        _ => Err(Error::Invalid("method must be auto, ipc2152 or ipc2221".into())),
    }
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "calc.list",
            description: "List every design calculator with its formula text and accuracy label; read only",
            input_schema: schema(json!({}), &[]),
        },
        CommandSpec {
            id: "calc.track_width",
            description: "Minimum trace width for a current and temperature rise using the IPC-2152 table when it applies and IPC-2221 otherwise; read only",
            input_schema: schema(
                json!({
                    "current":{"type":"number","exclusiveMinimum":0,"maximum":100},
                    "temperature_rise":{"type":"number","minimum":1,"maximum":100,"default":10},
                    "copper_weight":{"type":"number","minimum":0.25,"maximum":6,"default":1},
                    "layer":{"type":"string","enum":["external","internal"],"default":"external"},
                    "method":{"type":"string","enum":["auto","ipc2152","ipc2221"],"default":"auto"}
                }),
                &["current"],
            ),
        },
        CommandSpec {
            id: "calc.via",
            description: "Plated via current for a temperature rise, barrel resistance, and optional parallel count for a target current; read only",
            input_schema: schema(
                json!({
                    "drill":{"type":"number","exclusiveMinimum":0,"maximum":10},
                    "plating":{"type":"number","minimum":5,"maximum":100,"default":25},
                    "length":{"type":"number","minimum":0.1,"maximum":10,"default":1.6},
                    "temperature_rise":{"type":"number","minimum":1,"maximum":100,"default":10},
                    "target_current":{"type":"number","exclusiveMinimum":0,"maximum":100}
                }),
                &["drill"],
            ),
        },
        CommandSpec {
            id: "calc.impedance",
            description: "Controlled impedance for microstrip, embedded microstrip, stripline, coplanar or differential pair from explicit dimensions or the recorded stackup beside a copper layer; read only",
            input_schema: schema(
                json!({
                    "geometry":{"type":"string","enum":["microstrip","embedded_microstrip","stripline","coplanar","differential"]},
                    "width":{"type":"number","exclusiveMinimum":0,"maximum":100},
                    "gap":{"type":"number","exclusiveMinimum":0,"maximum":100},
                    "height":{"type":"number","exclusiveMinimum":0,"maximum":100},
                    "spacing":{"type":"number","exclusiveMinimum":0,"maximum":20,"description":"plane to plane spacing for stripline, millimetres"},
                    "cover":{"type":"number","minimum":0,"maximum":10},
                    "ground_backed":{"type":"boolean","default":true},
                    "er":{"type":"number","minimum":1,"maximum":20},
                    "thickness":{"type":"number","minimum":0,"maximum":0.5},
                    "target":{"type":"number","minimum":1,"maximum":1000},
                    "layer":{"type":"string","description":"copper layer name for stackup defaults, default F.Cu"}
                }),
                &["geometry"],
            ),
        },
        CommandSpec {
            id: "calc.divider",
            description: "Resistor voltage divider with an optional load on the output; read only",
            input_schema: schema(
                json!({
                    "vin":{"type":"number","minimum":0,"maximum":100000},
                    "r1":{"type":"number","exclusiveMinimum":0,"maximum":1000000000000.0},
                    "r2":{"type":"number","exclusiveMinimum":0,"maximum":1000000000000.0},
                    "load":{"type":"number","exclusiveMinimum":0,"maximum":1000000000000.0}
                }),
                &["vin", "r1", "r2"],
            ),
        },
        CommandSpec {
            id: "calc.convert",
            description: "Convert length or copper weight units among millimetres, mils, inches, micrometres and oz per square foot; read only",
            input_schema: schema(
                json!({
                    "value":{"type":"number"},
                    "from":{"type":"string","enum":["mm","mil","inch","um","oz"]},
                    "to":{"type":"string","enum":["mm","mil","inch","um","oz"]}
                }),
                &["value", "from", "to"],
            ),
        },
        CommandSpec {
            id: "calc.apply",
            description: "Write a calculator result into the design: track width to a net class default width, or via drill and pad diameter to a net class",
            input_schema: schema(
                json!({
                    "kind":{"type":"string","enum":["track_width","via"]},
                    "class":id_schema(),
                    "width_mm":{"type":"number","exclusiveMinimum":0,"maximum":100},
                    "current":{"type":"number","exclusiveMinimum":0,"maximum":100},
                    "temperature_rise":{"type":"number","minimum":1,"maximum":100},
                    "copper_weight":{"type":"number","minimum":0.25,"maximum":6},
                    "layer":{"type":"string","enum":["external","internal"]},
                    "method":{"type":"string","enum":["auto","ipc2152","ipc2221"]},
                    "drill":{"type":"number","exclusiveMinimum":0,"maximum":10},
                    "via_diameter":{"type":"number","exclusiveMinimum":0,"maximum":100},
                    "plating":{"type":"number","minimum":5,"maximum":100},
                    "length":{"type":"number","minimum":0.1,"maximum":10}
                }),
                &["kind", "class"],
            ),
        },
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackWidthParams {
    current: f64,
    #[serde(default = "default_rise")]
    temperature_rise: f64,
    #[serde(default = "one_f64")]
    copper_weight: f64,
    #[serde(default = "external_layer")]
    layer: String,
    #[serde(default = "auto_method")]
    method: String,
}
fn default_rise() -> f64 {
    10.0
}
fn one_f64() -> f64 {
    1.0
}
fn external_layer() -> String {
    "external".into()
}
fn auto_method() -> String {
    "auto".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViaParams {
    drill: f64,
    #[serde(default = "default_plating")]
    plating: f64,
    #[serde(default = "default_board")]
    length: f64,
    #[serde(default = "default_rise")]
    temperature_rise: f64,
    #[serde(default)]
    target_current: Option<f64>,
}
fn default_plating() -> f64 {
    25.0
}
fn default_board() -> f64 {
    1.6
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImpedanceParams {
    geometry: String,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    gap: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    spacing: Option<f64>,
    #[serde(default)]
    cover: Option<f64>,
    #[serde(default)]
    ground_backed: Option<bool>,
    #[serde(default)]
    er: Option<f64>,
    #[serde(default)]
    thickness: Option<f64>,
    #[serde(default)]
    target: Option<f64>,
    #[serde(default)]
    layer: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DividerParams {
    vin: f64,
    r1: f64,
    r2: f64,
    #[serde(default)]
    load: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConvertParams {
    value: f64,
    from: String,
    to: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyParams {
    kind: String,
    class: Id,
    #[serde(default)]
    width_mm: Option<f64>,
    #[serde(default)]
    current: Option<f64>,
    #[serde(default)]
    temperature_rise: Option<f64>,
    #[serde(default)]
    copper_weight: Option<f64>,
    #[serde(default)]
    layer: Option<String>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    drill: Option<f64>,
    #[serde(default)]
    via_diameter: Option<f64>,
    #[serde(default)]
    plating: Option<f64>,
    #[serde(default)]
    length: Option<f64>,
}

pub(super) fn run(p: &Project, command: &str, params: &Value) -> Option<Result<Value, Error>> {
    if command == "calc.apply" || !commands().iter().any(|c| c.id == command) {
        return None;
    }
    Some(match command {
        "calc.list" => calc_list(),
        "calc.track_width" => track_width_cmd(params),
        "calc.via" => via_cmd(params),
        "calc.impedance" => impedance_cmd(p, params),
        "calc.divider" => divider_cmd(params),
        "calc.convert" => convert_cmd(params),
        _ => Err(Error::Invalid(format!("unknown calculator command: {command}"))),
    })
}

pub(super) fn run_mut(p: &mut Project, command: &str, params: &Value) -> Option<Result<Value, Error>> {
    if command != "calc.apply" {
        return None;
    }
    Some(apply_cmd_mut(p, params))
}

fn calc_list() -> Result<Value, Error> {
    let rows: Vec<Value> = meta::CALCULATORS
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "group": m.group,
                "summary": m.summary,
                "accuracy": m.accuracy,
            })
        })
        .collect();
    Ok(json!({"calculators": rows, "count": rows.len()}))
}

fn track_width_cmd(params: &Value) -> Result<Value, Error> {
    let a: TrackWidthParams = parse(params)?;
    let out = trace_width(TraceWidth {
        current: a.current,
        temperature_rise: a.temperature_rise,
        copper_weight: a.copper_weight,
        layer: parse_layer(&a.layer)?,
        method: parse_method(&a.method)?,
    })
    .map_err(calc_err)?;
    let mut meta = meta_block("trace_width")?;
    if let Some(map) = meta.as_object_mut() {
        map.insert("method_used".into(), json!(out.method_used));
    }
    let mut notes = Vec::new();
    if out.scaled_from_1oz {
        notes.push("IPC-2152 table is for 1 oz copper; width scaled by 1/oz as an approximation".into());
    }
    if a.layer == "internal" {
        notes.push("Internal layers use the IPC-2221 halved-current constant; a nearby plane may behave differently".into());
    }
    Ok(json!({
        "width_mm": r4(out.width_mm),
        "width_mil": r4(out.width_mil),
        "cross_section_mil2": r4(out.cross_section),
        "method_used": out.method_used,
        "notes": notes,
        "meta": meta,
    }))
}

fn via_cmd(params: &Value) -> Result<Value, Error> {
    let a: ViaParams = parse(params)?;
    let out = via_current_resistance(ViaCurrent {
        drill: a.drill,
        plating: a.plating,
        length: a.length,
        temperature_rise: a.temperature_rise,
        target_current: a.target_current,
    })
    .map_err(calc_err)?;
    Ok(json!({
        "barrel_area_mm2": r4(out.barrel_area),
        "current_a": r4(out.current),
        "resistance_ohm": r4(out.resistance),
        "parallel_count": out.count.map(r4),
        "meta": meta_block("via_current_resistance")?,
    }))
}

fn impedance_cmd(p: &Project, params: &Value) -> Result<Value, Error> {
    let a: ImpedanceParams = parse(params)?;
    let layer = a
        .layer
        .clone()
        .unwrap_or_else(|| p.layer_name(Layer::Top));
    let (height, er) = dielectric_for(p, &layer);
    let h = a.height.unwrap_or(height);
    let er_v = a.er.unwrap_or(er);
    let t = a.thickness.unwrap_or(copper_thickness(p, &layer));
    let mut stack = json!({
        "layer": layer,
        "height_mm": r3(h),
        "er": r3(er_v),
        "copper_mm": r3(t),
    });
    match a.geometry.as_str() {
        "microstrip" => {
            let out = microstrip(Microstrip {
                width: a.width,
                height: h,
                er: er_v,
                thickness: t,
                target: a.target,
            })
            .map_err(calc_err)?;
            Ok(json!({
                "geometry": "microstrip",
                "impedance_ohm": out.z0.map(r3),
                "eeff": out.eeff.map(r3),
                "delay_ps_per_mm": out.delay.map(r3),
                "width_for_target_mm": out.width_for_target.map(r3),
                "stackup": stack,
                "meta": meta_block("microstrip")?,
            }))
        }
        "embedded_microstrip" => {
            let width = a.width.ok_or_else(|| {
                Error::Invalid("embedded microstrip needs a width".into())
            })?;
            let cover = a.cover.unwrap_or(0.0);
            let out = embedded_microstrip(EmbeddedMicrostrip {
                width,
                height: h,
                cover,
                er: er_v,
                thickness: t,
            })
            .map_err(calc_err)?;
            Ok(json!({
                "geometry": "embedded_microstrip",
                "impedance_ohm": r3(out.z0),
                "eeff": r3(out.eeff),
                "cover_mm": r3(cover),
                "stackup": stack,
                "meta": meta_block("embedded_microstrip")?,
            }))
        }
        "stripline" => {
            let width = a.width.ok_or_else(|| Error::Invalid("stripline needs a width".into()))?;
            let spacing = a.spacing.unwrap_or(p.board.thickness);
            if let Some(map) = stack.as_object_mut() {
                map.insert("spacing_mm".into(), json!(r3(spacing)));
            }
            let out = stripline(Stripline {
                width,
                spacing,
                er: er_v,
                thickness: t,
            })
            .map_err(calc_err)?;
            Ok(json!({
                "geometry": "stripline",
                "impedance_ohm": r3(out.z0),
                "delay_ps_per_mm": r3(out.delay),
                "stackup": stack,
                "meta": meta_block("stripline")?,
            }))
        }
        "coplanar" => {
            let width = a.width.ok_or_else(|| Error::Invalid("coplanar needs a width".into()))?;
            let gap = a.gap.ok_or_else(|| Error::Invalid("coplanar needs a gap".into()))?;
            let out = coplanar_waveguide(CoplanarWaveguide {
                width,
                gap,
                height: h,
                er: er_v,
                ground_backed: a.ground_backed.unwrap_or(true),
            })
            .map_err(calc_err)?;
            Ok(json!({
                "geometry": "coplanar",
                "impedance_ohm": r3(out.z0),
                "eeff": r3(out.eeff),
                "delay_ps_per_mm": r3(out.delay),
                "gap_mm": r3(gap),
                "ground_backed": a.ground_backed.unwrap_or(true),
                "stackup": stack,
                "meta": meta_block("coplanar_waveguide")?,
            }))
        }
        "differential" => {
            let width = a.width.ok_or_else(|| {
                Error::Invalid("differential pair needs a width".into())
            })?;
            let gap = a.gap.ok_or_else(|| Error::Invalid("differential pair needs a gap".into()))?;
            let out = differential_pair(DifferentialPair {
                width,
                gap,
                height: h,
                er: er_v,
                thickness: t,
            })
            .map_err(calc_err)?;
            Ok(json!({
                "geometry": "differential",
                "z_diff_ohm": r3(out.zdiff),
                "z_single_ohm": r3(out.z0_single),
                "coupling_factor": r3(out.coupling_factor),
                "width_for_target_mm": out.width_for_target.map(r3),
                "gap_mm": r3(gap),
                "stackup": stack,
                "meta": meta_block("differential_pair")?,
            }))
        }
        _ => Err(Error::Invalid(
            "geometry must be microstrip, embedded_microstrip, stripline, coplanar or differential".into(),
        )),
    }
}

fn divider_cmd(params: &Value) -> Result<Value, Error> {
    let a: DividerParams = parse(params)?;
    let out = voltage_divider(VoltageDivider {
        vin: a.vin,
        r1: a.r1,
        r2: a.r2,
        load: a.load,
    })
    .map_err(calc_err)?;
    Ok(json!({
        "vout_v": r4(out.vout),
        "current_a": r4(out.current),
        "r2_effective_ohm": r4(out.r2_effective),
        "power_r1_w": r4(out.power_r1),
        "power_r2_w": r4(out.power_r2),
        "meta": meta_block("voltage_divider")?,
    }))
}

fn convert_cmd(params: &Value) -> Result<Value, Error> {
    let a: ConvertParams = parse(params)?;
    let from = LengthUnit::parse(&a.from)
        .ok_or_else(|| Error::Invalid("from must be mm, mil, inch, um or oz".into()))?;
    let to = LengthUnit::parse(&a.to)
        .ok_or_else(|| Error::Invalid("to must be mm, mil, inch, um or oz".into()))?;
    let out = unit_converter(UnitConvert {
        value: a.value,
        from,
        to,
    })
    .map_err(calc_err)?;
    Ok(json!({
        "value": r4(out.value),
        "from": a.from,
        "to": a.to,
        "meta": meta_block("unit_converter")?,
    }))
}

fn apply_cmd_mut(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ApplyParams = parse(params)?;
    let class = p
        .net_classes
        .iter_mut()
        .find(|c| c.id == a.class)
        .ok_or_else(missing)?;
    match a.kind.as_str() {
        "track_width" => {
            let width = match a.width_mm {
                Some(w) => {
                    if !w.is_finite() || w <= 0.0 || w > 100.0 {
                        return Err(Error::Invalid(
                            "width_mm must be between 0 and 100 millimetres".into(),
                        ));
                    }
                    w
                }
                None => {
                    let current = a.current.ok_or_else(|| {
                        Error::Invalid("give width_mm or current for track_width".into())
                    })?;
                    let tw = trace_width(TraceWidth {
                        current,
                        temperature_rise: a.temperature_rise.unwrap_or(10.0),
                        copper_weight: a.copper_weight.unwrap_or(1.0),
                        layer: parse_layer(a.layer.as_deref().unwrap_or("external"))?,
                        method: parse_method(a.method.as_deref().unwrap_or("auto"))?,
                    })
                    .map_err(calc_err)?;
                    tw.width_mm
                }
            };
            if class.width == width {
                return Err(Error::Invalid(
                    "that class already carries that default width".into(),
                ));
            }
            class.width = width;
            Ok(json!({"kind": "track_width", "class": a.class, "width_mm": r4(width)}))
        }
        "via" => {
            let drill = a.drill.ok_or_else(|| {
                Error::Invalid("via apply needs a drill diameter".into())
            })?;
            if !drill.is_finite() || drill <= 0.0 || drill > 10.0 {
                return Err(Error::Invalid(
                    "drill must be between 0 and 10 millimetres".into(),
                ));
            }
            let diameter = a.via_diameter.unwrap_or(drill * 2.0 + 0.1);
            if !diameter.is_finite() || diameter <= drill || diameter > 100.0 {
                return Err(Error::Invalid(
                    "via_diameter must exceed drill and stay under 100 millimetres".into(),
                ));
            }
            if class.via_diameter == Some(diameter) && class.via_drill == Some(drill) {
                return Err(Error::Invalid(
                    "that class already carries those via dimensions".into(),
                ));
            }
            class.via_diameter = Some(diameter);
            class.via_drill = Some(drill);
            let _ = (a.plating, a.length);
            Ok(json!({
                "kind": "via",
                "class": a.class,
                "via_diameter": r4(diameter),
                "via_drill": r4(drill),
            }))
        }
        _ => Err(Error::Invalid("kind must be track_width or via".into())),
    }
}
