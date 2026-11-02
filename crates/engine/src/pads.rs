//! Pad commands: `pad.add`, `pad.edit` and `pad.remove`.
//!
//! Context: a pad on the board belongs to a placed footprint, and a footprint
//! belongs to a part with one schematic pin per pad. These commands add, change
//! and remove single pads from the canvases without opening a library, so a
//! free pad (a test point, a mounting pad, a connector pad) is an ordinary
//! one-pad part that every check and every output already understands.
//!
//! # Rules
//! - A free pad is a part of kind `test_point` with one pin and an embedded
//!   one-pad footprint; the same pin number is the pad number.
//! - A pad added to a placed part is appended to that part's embedded
//!   footprint copy in the project and gains a pin with the same number, so
//!   the net, the checks and a later board sync keep it.
//! - A part that has no embedded footprint yet (a starter part) gets one made
//!   from its current pads before the edit, so nothing else about it changes.
//! - Side `top` or `bottom` is a surface pad on that side of the part; `through`
//!   is a plated hole across all copper and needs a drill. A surface side other
//!   than the part's own is refused unless the pad is the part's only pad.
//! - Every call is checked first and changes nothing when it fails.
//!
//! # Dependencies
//! - `crates/engine/src/lib.rs` for `CommandSpec`, `schema`, `parse`, `assign`,
//!   `component_pads` and `allocate`.
//! - `crates/doc/src/lib.rs` for `Component`, `Footprint`, `Pad` and `Pin`.
//!
//! # Do not
//! - Write a pad into `Footprint::pads` alone; the embedded definition and the
//!   pin are the source a board sync rebuilds the pads from.
use super::*;
use pcbcraft_doc::{Component, Footprint, LibraryFootprint, Pin, PinRef};
use pcbcraft_library::{Drill, FootprintDefinition, FootprintPad};

/// Most pads one part may carry through these commands.
const MAX_PADS: usize = 512;
/// Copper size limits in millimetres.
const MIN_SIZE: f64 = 0.05;
const MAX_SIZE: f64 = 100.0;
/// Copper edge, in millimetres, a pad starts with when no size is given.
const DEFAULT_SIZE: f64 = 1.6;
/// Corner ratio of a rounded rectangle pad.
const ROUNDRECT_RATIO: f64 = 0.25;
/// The name of the embedded footprint made for a free pad or a starter part.
const DEFINITION_NAME: &str = "Pad";

const SHAPES: [&str; 4] = ["circle", "rect", "oval", "roundrect"];
const SIDES: [&str; 3] = ["top", "bottom", "through"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddParams {
    /// The placed footprint to add a pad to; absent makes a free pad.
    #[serde(default)]
    footprint: Option<Id>,
    /// Board position of a free pad, or of the pad on a placed footprint.
    #[serde(default)]
    position: Option<Point>,
    /// Local offset on a placed footprint, before the footprint's rotation.
    #[serde(default)]
    offset: Option<Point>,
    /// Schematic position of a free pad's part.
    #[serde(default)]
    schematic_position: Option<Point>,
    /// Schematic page of a free pad's part.
    #[serde(default)]
    sheet: Option<Id>,
    #[serde(default)]
    number: Option<String>,
    #[serde(default)]
    shape: Option<String>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    drill: Option<f64>,
    #[serde(default)]
    side: Option<String>,
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    reference: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditParams {
    id: Id,
    pad: String,
    #[serde(default)]
    number: Option<String>,
    #[serde(default)]
    shape: Option<String>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    drill: Option<f64>,
    #[serde(default)]
    side: Option<String>,
    #[serde(default)]
    offset: Option<Point>,
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    release_net: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveParams {
    id: Id,
    pad: String,
}

/// The command specs this module serves.
pub(super) fn commands() -> Vec<CommandSpec> {
    let copper = json!({
        "number":{"type":"string","minLength":1,"maxLength":32,"description":"pad number or name; it is also the number of the part's pin"},
        "shape":{"type":"string","enum":SHAPES},
        "width":{"type":"number","minimum":MIN_SIZE,"maximum":MAX_SIZE,"description":"copper extent in x, millimetres"},
        "height":{"type":"number","minimum":MIN_SIZE,"maximum":MAX_SIZE,"description":"copper extent in y, millimetres; a circle uses the width"},
        "drill":{"type":"number","minimum":0,"maximum":MAX_SIZE,"description":"drill diameter, millimetres; a surface pad has none"},
        "side":{"type":"string","enum":SIDES,"description":"top or bottom is a surface pad, through is a plated hole across all copper"},
    });
    let mut add = copper.clone();
    if let Some(map) = add.as_object_mut() {
        map.insert("footprint".into(), id_schema());
        map.insert("position".into(), point_schema());
        map.insert("offset".into(), point_schema());
        map.insert("schematic_position".into(), point_schema());
        map.insert("sheet".into(), json!({"type":"integer","minimum":0}));
        map.insert("net".into(), id_schema());
        map.insert(
            "reference".into(),
            json!({"type":"string","minLength":1,"maxLength":32}),
        );
    }
    let mut edit = copper;
    if let Some(map) = edit.as_object_mut() {
        map.insert("id".into(), id_schema());
        map.insert("pad".into(), json!({"type":"string","minLength":1,"maxLength":32,"description":"the pad's current number, as listed by pad.table"}));
        map.insert("offset".into(), point_schema());
        map.insert("net".into(), id_schema());
        map.insert(
            "release_net".into(),
            json!({"type":"boolean","description":"take the pad off its net"}),
        );
    }
    vec![
        CommandSpec {
            id: "pad.add",
            description: "Add a copper pad. With footprint it joins that placed part (at offset, or at a board position); without it a free pad is made: a one-pad part on the board at position, in the schematic at schematic_position, or both. Choose number, shape, size, drill (none for a surface pad), side and net",
            input_schema: schema(add, &[]),
        },
        CommandSpec {
            id: "pad.edit",
            description: "Change one placed pad: its number, shape, size, drill, side, offset or net; the part's pin follows the number and the net, and the change survives a board sync",
            input_schema: schema(edit, &["id", "pad"]),
        },
        CommandSpec {
            id: "pad.remove",
            description: "Remove one pad and its pin from a placed part; removing the only pad of a free pad deletes that part",
            input_schema: schema(
                json!({"id":id_schema(),"pad":{"type":"string","minLength":1,"maxLength":32}}),
                &["id", "pad"],
            ),
        },
    ]
}

/// Run a pad command, or None when `command` is not one of them.
pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    Some(match command {
        "pad.add" => parse::<AddParams>(params).and_then(|a| add(p, next, a)),
        "pad.edit" => parse::<EditParams>(params).and_then(|a| edit(p, a)),
        "pad.remove" => parse::<RemoveParams>(params).and_then(|a| remove(p, a)),
        _ => return None,
    })
}

fn invalid(text: &str) -> Error {
    Error::Invalid(text.into())
}

/// A surface or through-hole choice, resolved against a drill.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Top,
    Bottom,
    Through,
}

fn parse_side(text: &str) -> Result<Side, Error> {
    match text {
        "top" => Ok(Side::Top),
        "bottom" => Ok(Side::Bottom),
        "through" => Ok(Side::Through),
        _ => Err(invalid("side is top, bottom or through")),
    }
}

fn check_size(value: f64, what: &str) -> Result<f64, Error> {
    if !value.is_finite() || !(MIN_SIZE..=MAX_SIZE).contains(&value) {
        return Err(Error::Invalid(format!(
            "{what} must be 0.05 to 100 millimetres"
        )));
    }
    Ok(value)
}

fn check_shape(shape: &str) -> Result<(), Error> {
    if SHAPES.contains(&shape) {
        Ok(())
    } else {
        Err(invalid("shape is circle, rect, oval or roundrect"))
    }
}

fn check_number(number: &str) -> Result<(), Error> {
    if number.is_empty() || number.len() > 32 || number.chars().any(char::is_control) {
        return Err(invalid("a pad number is 1 to 32 plain characters"));
    }
    Ok(())
}

fn check_point(point: Point, what: &str) -> Result<(), Error> {
    if !point.valid() {
        return Err(Error::Invalid(format!("{what} is out of range")));
    }
    Ok(())
}

/// Smallest whole number not used by any pad or pin of the part.
fn free_number(c: &Component, pads: &[FootprintPad]) -> String {
    let mut n = 1usize;
    loop {
        let text = n.to_string();
        if !pads.iter().any(|q| q.number == text) && !c.pins.iter().any(|q| q.number == text) {
            return text;
        }
        n += 1;
    }
}

/// Copper layers of a surface pad as authored on the top; a bottom part swaps them.
fn surface_layers() -> Vec<String> {
    vec!["F.Cu".into(), "F.Paste".into(), "F.Mask".into()]
}

/// Copper layers of a plated through-hole pad.
fn through_layers() -> Vec<String> {
    vec!["*.Cu".into(), "*.Mask".into()]
}

/// The pad copper a request describes, with its drill and layers.
fn build_pad(
    number: &str,
    shape: &str,
    width: f64,
    height: f64,
    drill: Option<f64>,
    offset: Point,
) -> Result<FootprintPad, Error> {
    check_shape(shape)?;
    let size = if shape == "circle" {
        Point::new(width, width)
    } else {
        Point::new(width, height)
    };
    if let Some(d) = drill {
        check_size(d, "the drill")?;
        if d >= size.x.min(size.y) {
            return Err(invalid("the drill must be smaller than the copper"));
        }
    }
    Ok(FootprintPad {
        number: number.into(),
        pad_type: if drill.is_some() { "thru_hole" } else { "smd" }.into(),
        shape: shape.into(),
        position: offset,
        rotation: 0.0,
        size,
        layers: if drill.is_some() {
            through_layers()
        } else {
            surface_layers()
        },
        drill: drill.map(|d| Drill {
            size: Point::new(d, d),
            offset: Point::new(0.0, 0.0),
        }),
        roundrect_ratio: (shape == "roundrect").then_some(ROUNDRECT_RATIO),
        chamfer: None,
        polygon: Vec::new(),
        raw_source: String::new(),
    })
}

/// The embedded definition of a part, made from its current pads when it has none.
fn definition_of(c: &Component, f: &Footprint) -> FootprintDefinition {
    if let Some(library) = &c.library_footprint {
        return library.definition.clone();
    }
    let pads = f
        .pads
        .iter()
        .map(|pad| {
            pad.geometry.clone().unwrap_or_else(|| FootprintPad {
                number: pad.number.clone(),
                pad_type: "smd".into(),
                shape: "circle".into(),
                position: pad.offset,
                rotation: 0.0,
                size: Point::new(pad.diameter, pad.diameter),
                layers: surface_layers(),
                drill: None,
                roundrect_ratio: None,
                chamfer: None,
                polygon: Vec::new(),
                raw_source: String::new(),
            })
        })
        .collect();
    FootprintDefinition {
        name: DEFINITION_NAME.into(),
        pads,
        ..Default::default()
    }
}

/// Where the next pin of a part sits on its symbol: a column to the right of the body.
fn next_pin_offset(c: &Component) -> Point {
    let count = c.pins.iter().filter(|q| q.offset.x > 0.0).count();
    Point::new(5.0, 2.5 * count as f64)
}

fn new_pin(number: &str, offset: Point) -> Pin {
    Pin {
        number: number.into(),
        name: String::new(),
        offset,
        ..Pin::default()
    }
}

/// Rebuild the placed pads of every footprint of `component` from its embedded
/// definition, keeping each pad's net and castellated flag.
fn rebuild(p: &mut Project, component: Id) {
    let Some(c) = p.components.iter().find(|c| c.id == component) else {
        return;
    };
    let fresh = component_pads(c);
    for f in p
        .board
        .footprints
        .iter_mut()
        .filter(|f| f.component == component)
    {
        let castellated: Vec<(String, Option<bool>)> = f
            .pads
            .iter()
            .map(|q| (q.number.clone(), q.castellated))
            .collect();
        f.pads = fresh.clone();
        for pad in &mut f.pads {
            pad.castellated = castellated
                .iter()
                .find(|(n, _)| *n == pad.number)
                .and_then(|(_, flag)| *flag);
        }
    }
}

fn unique_reference(p: &Project, wanted: Option<String>) -> Result<String, Error> {
    match wanted {
        Some(r) => {
            if r.is_empty() || r.len() > 32 || p.components.iter().any(|c| c.reference == r) {
                return Err(invalid("the reference is empty or already used"));
            }
            Ok(r)
        }
        None => {
            let mut n = 1;
            while p.components.iter().any(|c| c.reference == format!("TP{n}")) {
                n += 1;
            }
            Ok(format!("TP{n}"))
        }
    }
}

fn add(p: &mut Project, next: &mut Id, a: AddParams) -> Result<Value, Error> {
    let width = check_size(a.width.unwrap_or(DEFAULT_SIZE), "the width")?;
    let height = check_size(a.height.unwrap_or(width), "the height")?;
    let shape = a.shape.clone().unwrap_or_else(|| "circle".into());
    let side = match (&a.side, a.drill) {
        (Some(s), _) => parse_side(s)?,
        (None, Some(d)) if d > 0.0 => Side::Through,
        (None, _) => Side::Top,
    };
    let drill = match (side, a.drill) {
        (Side::Through, Some(d)) if d > 0.0 => Some(d),
        (Side::Through, _) => return Err(invalid("a through-hole pad needs a drill size")),
        (_, Some(d)) if d > 0.0 => {
            return Err(invalid("a surface pad has no drill; choose through"));
        }
        _ => None,
    };
    if let Some(net) = a.net
        && !p.nets.iter().any(|n| n.id == net)
    {
        return Err(invalid("unknown net"));
    }
    if let Some(number) = &a.number {
        check_number(number)?;
    }
    for (point, what) in [
        (a.position, "the position"),
        (a.offset, "the offset"),
        (a.schematic_position, "the schematic position"),
    ] {
        if let Some(point) = point {
            check_point(point, what)?;
        }
    }
    match a.footprint {
        Some(footprint) => add_to_part(p, a, footprint, width, height, &shape, side, drill),
        None => add_free(p, next, a, width, height, &shape, side, drill),
    }
}

#[allow(clippy::too_many_arguments)]
fn add_to_part(
    p: &mut Project,
    a: AddParams,
    footprint: Id,
    width: f64,
    height: f64,
    shape: &str,
    side: Side,
    drill: Option<f64>,
) -> Result<Value, Error> {
    if a.schematic_position.is_some() || a.sheet.is_some() || a.reference.is_some() {
        return Err(invalid(
            "schematic_position, sheet and reference belong to a free pad, not to a placed part",
        ));
    }
    if p.is_locked(footprint) {
        return Err(invalid(
            "that footprint is locked against placement changes",
        ));
    }
    let f = p
        .board
        .footprints
        .iter()
        .find(|f| f.id == footprint)
        .ok_or_else(missing)?
        .clone();
    let c = p.component(f.component).ok_or_else(missing)?.clone();
    if f.pads.len() >= MAX_PADS {
        return Err(invalid("a part carries at most 512 pads"));
    }
    let own = f.board_side();
    if (side == Side::Top && own != Layer::Front) || (side == Side::Bottom && own == Layer::Front) {
        return Err(invalid(
            "a surface pad sits on the same side as its part; flip the part first or add a through-hole pad",
        ));
    }
    let offset = match (a.offset, a.position) {
        (Some(_), Some(_)) => return Err(invalid("give an offset or a position, not both")),
        (Some(o), None) => o,
        (None, Some(world)) => {
            Point::new(world.x - f.position.x, world.y - f.position.y).rotate(f.rotation)
        }
        (None, None) => Point::new(0.0, 0.0),
    };
    let mut definition = definition_of(&c, &f);
    let number = match a.number {
        Some(n) => n,
        None => free_number(&c, &definition.pads),
    };
    if definition.pads.iter().any(|q| q.number == number)
        || c.pins.iter().any(|q| q.number == number)
    {
        return Err(Error::Invalid(format!("pad {number} already exists")));
    }
    definition
        .pads
        .push(build_pad(&number, shape, width, height, drill, offset)?);
    definition
        .validate()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let pin_offset = next_pin_offset(&c);
    let component = f.component;
    {
        let c = p
            .components
            .iter_mut()
            .find(|c| c.id == component)
            .ok_or_else(missing)?;
        c.pins.push(new_pin(&number, pin_offset));
        set_definition(c, definition);
    }
    rebuild(p, component);
    if let Some(net) = a.net {
        assign(
            p,
            net,
            &[PinRef {
                component,
                pin: number.clone(),
                unit: None,
            }],
        )?;
    }
    Ok(json!({"id":footprint,"component":component,"pad":number}))
}

fn set_definition(c: &mut Component, definition: FootprintDefinition) {
    match &mut c.library_footprint {
        Some(library) => library.definition = definition,
        None => {
            c.library_footprint = Some(LibraryFootprint {
                library: "project".into(),
                definition,
            });
        }
    }
}

/// A staging spot beside the board for a footprint that has no board position yet.
fn staging_position(p: &Project) -> Point {
    let right = p
        .board
        .outline
        .as_ref()
        .map(|o| o.iter().map(|q| q.x).fold(0.0_f64, f64::max))
        .unwrap_or(p.board.width);
    let x = right + 10.0;
    let queued = p
        .board
        .footprints
        .iter()
        .filter(|f| f.position.x >= x - 1e-6)
        .count();
    Point::new(
        x + (queued % 4) as f64 * 12.0,
        10.0 + (queued / 4) as f64 * 8.0,
    )
}

/// A free spot on the sheet for a part that was placed from the board.
fn schematic_spot(p: &Project, sheet: Id) -> Point {
    let count = p.components.iter().filter(|c| c.sheet == sheet).count();
    Point::new(
        20.0 + (count % 6) as f64 * 20.0,
        20.0 + (count / 6) as f64 * 15.0,
    )
}

#[allow(clippy::too_many_arguments)]
fn add_free(
    p: &mut Project,
    next: &mut Id,
    a: AddParams,
    width: f64,
    height: f64,
    shape: &str,
    side: Side,
    drill: Option<f64>,
) -> Result<Value, Error> {
    if a.offset.is_some() {
        return Err(invalid("offset belongs to a pad added to a placed part"));
    }
    if a.position.is_none() && a.schematic_position.is_none() {
        return Err(invalid(
            "give a board position, a schematic position, or the footprint to add to",
        ));
    }
    let sheet = a.sheet.unwrap_or(0);
    if sheet != 0 && !p.sheets.iter().any(|s| s.id == sheet) {
        return Err(invalid("unknown schematic page"));
    }
    let number = a.number.clone().unwrap_or_else(|| "1".into());
    let pad = build_pad(&number, shape, width, height, drill, Point::new(0.0, 0.0))?;
    let reference = unique_reference(p, a.reference.clone())?;
    let component = allocate(next)?;
    let footprint = allocate(next)?;
    let position = a
        .schematic_position
        .unwrap_or_else(|| schematic_spot(p, sheet));
    let part = Component {
        id: component,
        reference: reference.clone(),
        value: "Pad".into(),
        kind: "test_point".into(),
        position,
        pins: vec![new_pin(&number, Point::new(-5.0, 0.0))],
        library_footprint: Some(LibraryFootprint {
            library: "project".into(),
            definition: FootprintDefinition {
                name: DEFINITION_NAME.into(),
                pads: vec![pad],
                ..Default::default()
            },
        }),
        include_in_bom: false,
        sheet,
        ..Component::default()
    };
    let board_position = a.position.unwrap_or_else(|| staging_position(p));
    let placed = component_pads(&part);
    p.components.push(part);
    p.board.footprints.push(Footprint {
        id: footprint,
        component,
        position: board_position,
        rotation: 0.0,
        locked: None,
        side: (side == Side::Bottom).then_some(Layer::Back),
        pads: placed,
        preserved: pcbcraft_doc::PreservedSource::default(),
    });
    if let Some(net) = a.net {
        assign(
            p,
            net,
            &[PinRef {
                component,
                pin: number.clone(),
                unit: None,
            }],
        )?;
    }
    Ok(
        json!({"id":footprint,"footprint":footprint,"component":component,"reference":reference,"pad":number}),
    )
}

fn edit(p: &mut Project, a: EditParams) -> Result<Value, Error> {
    if p.is_locked(a.id) {
        return Err(invalid(
            "that footprint is locked against placement changes",
        ));
    }
    if let Some(net) = a.net {
        if a.release_net {
            return Err(invalid("give a net or release_net, not both"));
        }
        if !p.nets.iter().any(|n| n.id == net) {
            return Err(invalid("unknown net"));
        }
    }
    let f = p
        .board
        .footprints
        .iter()
        .find(|f| f.id == a.id)
        .ok_or_else(missing)?
        .clone();
    let component = f.component;
    let c = p.component(component).ok_or_else(missing)?.clone();
    if !f.pads.iter().any(|q| q.number == a.pad) {
        return Err(Error::Invalid(format!("the part has no pad {}", a.pad)));
    }
    let mut definition = definition_of(&c, &f);
    let index = definition
        .pads
        .iter()
        .position(|q| q.number == a.pad)
        .ok_or_else(missing)?;
    let old = definition.pads[index].clone();
    let number = a.number.clone().unwrap_or_else(|| old.number.clone());
    check_number(&number)?;
    if number != old.number
        && (definition.pads.iter().any(|q| q.number == number)
            || c.pins.iter().any(|q| q.number == number))
    {
        return Err(Error::Invalid(format!("pad {number} already exists")));
    }
    let shape = a.shape.clone().unwrap_or_else(|| old.shape.clone());
    let width = check_size(a.width.unwrap_or(old.size.x), "the width")?;
    let height = check_size(a.height.unwrap_or(old.size.y), "the height")?;
    let was_through = old.drill.is_some();
    let side = match &a.side {
        Some(s) => parse_side(s)?,
        None if was_through => Side::Through,
        None => f.board_side().into_side(),
    };
    let drill = match (side, a.drill) {
        (Side::Through, Some(d)) if d > 0.0 => Some(d),
        (Side::Through, _) => match &old.drill {
            Some(d) => Some(d.size.x),
            None => return Err(invalid("a through-hole pad needs a drill size")),
        },
        (_, Some(d)) if d > 0.0 => {
            return Err(invalid("a surface pad has no drill; choose through"));
        }
        _ => None,
    };
    let offset = a.offset.unwrap_or(old.position);
    check_point(offset, "the offset")?;
    let mut rebuilt = build_pad(&number, &shape, width, height, drill, offset)?;
    rebuilt.rotation = old.rotation;
    let mut new_side = f.side;
    if matches!(side, Side::Top | Side::Bottom) {
        let wanted = if side == Side::Top {
            Layer::Front
        } else {
            Layer::Back
        };
        if wanted != f.board_side() {
            if f.pads.len() != 1 {
                return Err(invalid(
                    "a surface pad sits on the same side as its part; flip the part first or choose through",
                ));
            }
            new_side = (wanted == Layer::Back).then_some(Layer::Back);
        }
    }
    definition.pads[index] = rebuilt;
    definition
        .validate()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    {
        let c = p
            .components
            .iter_mut()
            .find(|c| c.id == component)
            .ok_or_else(missing)?;
        if let Some(pin) = c.pins.iter_mut().find(|q| q.number == a.pad) {
            pin.number = number.clone();
            if a.release_net {
                pin.net = None;
            }
        }
        set_definition(c, definition);
    }
    if number != a.pad {
        for w in &mut p.wires {
            for end in [&mut w.from, &mut w.to] {
                if end.component == component && end.pin == a.pad {
                    end.pin = number.clone();
                }
            }
        }
        for flag in &mut p.no_connects {
            if flag.pin.component == component && flag.pin.pin == a.pad {
                flag.pin.pin = number.clone();
            }
        }
    }
    if let Some(footprint) = p.board.footprints.iter_mut().find(|q| q.id == a.id) {
        footprint.side = new_side;
    }
    rebuild(p, component);
    if let Some(net) = a.net {
        let current = p
            .component(component)
            .and_then(|c| c.pins.iter().find(|q| q.number == number))
            .and_then(|q| q.net);
        if current.is_some_and(|n| n != net)
            && let Some(c) = p.components.iter_mut().find(|c| c.id == component)
        {
            for pin in c.pins.iter_mut().filter(|q| q.number == number) {
                pin.net = None;
            }
        }
        assign(
            p,
            net,
            &[PinRef {
                component,
                pin: number.clone(),
                unit: None,
            }],
        )?;
    }
    Ok(json!({"id":a.id,"component":component,"pad":number}))
}

fn remove(p: &mut Project, a: RemoveParams) -> Result<Value, Error> {
    if p.is_locked(a.id) {
        return Err(invalid(
            "that footprint is locked against placement changes",
        ));
    }
    let f = p
        .board
        .footprints
        .iter()
        .find(|f| f.id == a.id)
        .ok_or_else(missing)?
        .clone();
    let component = f.component;
    let c = p.component(component).ok_or_else(missing)?.clone();
    if !f.pads.iter().any(|q| q.number == a.pad) {
        return Err(Error::Invalid(format!("the part has no pad {}", a.pad)));
    }
    if f.pads.len() == 1 {
        if c.kind != "test_point" {
            return Err(invalid(
                "a part keeps at least one pad; delete the part itself instead",
            ));
        }
        p.components.retain(|q| q.id != component);
        p.board.footprints.retain(|q| q.component != component);
        p.wires
            .retain(|w| w.from.component != component && w.to.component != component);
        p.no_connects.retain(|q| q.pin.component != component);
        return Ok(json!({"id":a.id,"pad":a.pad,"part_removed":true}));
    }
    let mut definition = definition_of(&c, &f);
    definition.pads.retain(|q| q.number != a.pad);
    definition
        .validate()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    {
        let c = p
            .components
            .iter_mut()
            .find(|c| c.id == component)
            .ok_or_else(missing)?;
        c.pins.retain(|q| q.number != a.pad);
        set_definition(c, definition);
    }
    p.wires.retain(|w| {
        !(w.from.component == component && w.from.pin == a.pad
            || w.to.component == component && w.to.pin == a.pad)
    });
    p.no_connects
        .retain(|q| !(q.pin.component == component && q.pin.pin == a.pad));
    rebuild(p, component);
    Ok(json!({"id":a.id,"pad":a.pad,"part_removed":false}))
}

/// A part's own side as the side word the commands use.
trait IntoSide {
    fn into_side(self) -> Side;
}

impl IntoSide for Layer {
    fn into_side(self) -> Side {
        if self == Layer::Front {
            Side::Top
        } else {
            Side::Bottom
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_net() -> Result<(Session, Id), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"PROBE"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        Ok((s, net))
    }

    #[test]
    fn a_free_pad_is_a_one_pad_part_on_its_net_and_undo_removes_it() -> Result<(), Error> {
        let (mut s, net) = session_with_net()?;
        let out = s.execute(
            "pad.add",
            json!({"position":{"x":10.0,"y":12.0},"number":"A1","shape":"rect","width":2.0,"height":1.0,"net":net}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["reference"], "TP1");
        let fp = &s.project.board.footprints[0];
        assert_eq!(fp.position, Point::new(10.0, 12.0));
        assert_eq!(fp.pads.len(), 1);
        assert_eq!(fp.pads[0].number, "A1");
        assert_eq!(fp.pads[0].net, Some(net));
        let g = fp.pads[0].geometry.as_ref().ok_or_else(missing)?;
        assert_eq!((g.shape.as_str(), g.size.x, g.size.y), ("rect", 2.0, 1.0));
        assert_eq!(s.project.components[0].pins[0].net, Some(net));
        s.undo()?;
        assert!(s.project.board.footprints.is_empty());
        assert!(s.project.components.is_empty());
        s.redo()?;
        assert_eq!(s.project.board.footprints.len(), 1);
        let table = s.execute("pad.table", json!({}), None)?;
        assert!(table.to_string().contains("A1"));
        Ok(())
    }

    #[test]
    fn a_through_hole_pad_needs_a_drill_and_lands_in_the_drill_chart() -> Result<(), Error> {
        let (mut s, _) = session_with_net()?;
        assert!(
            s.execute(
                "pad.add",
                json!({"position":{"x":5.0,"y":5.0},"side":"through"}),
                None
            )
            .is_err(),
            "through without a drill is refused"
        );
        assert!(
            s.execute(
                "pad.add",
                json!({"position":{"x":5.0,"y":5.0},"side":"top","drill":0.8}),
                None
            )
            .is_err(),
            "a surface pad has no drill"
        );
        assert!(
            s.execute(
                "pad.add",
                json!({"position":{"x":5.0,"y":5.0},"width":1.0,"drill":1.2}),
                None
            )
            .is_err(),
            "the drill must fit inside the copper"
        );
        s.execute(
            "pad.add",
            json!({"position":{"x":5.0,"y":5.0},"width":1.8,"drill":0.9}),
            None,
        )?;
        let chart = s.execute("drill.chart", json!({}), None)?.to_string();
        assert!(
            chart.contains("0.9"),
            "the drill chart lists the hole: {chart}"
        );
        Ok(())
    }

    #[test]
    fn edit_changes_shape_size_number_and_net_and_survives_a_sync() -> Result<(), Error> {
        let (mut s, net) = session_with_net()?;
        let id = s.execute("pad.add", json!({"position":{"x":3.0,"y":3.0}}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "pad.edit",
            json!({"id":id,"pad":"1","number":"TP","shape":"oval","width":3.0,"height":1.5,"net":net}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let pad = &s.project.board.footprints[0].pads[0];
        assert_eq!(pad.number, "TP");
        assert_eq!(pad.net, Some(net));
        let g = pad.geometry.as_ref().ok_or_else(missing)?;
        assert_eq!((g.shape.as_str(), g.size.x), ("oval", 3.0));
        s.execute(
            "pad.edit",
            json!({"id":id,"pad":"TP","release_net":true}),
            None,
        )?;
        assert_eq!(s.project.board.footprints[0].pads[0].net, None);
        s.execute(
            "pad.edit",
            json!({"id":id,"pad":"TP","side":"bottom"}),
            None,
        )?;
        assert_eq!(s.project.board.footprints[0].board_side(), Layer::Back);
        s.execute(
            "pad.edit",
            json!({"id":id,"pad":"TP","side":"through","drill":0.6}),
            None,
        )?;
        assert!(
            s.project.board.footprints[0].pads[0]
                .geometry
                .as_ref()
                .is_some_and(|g| g.drill.is_some())
        );
        let reload: Project = serde_json::from_str(
            &serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?,
        )
        .map_err(|e| Error::Invalid(e.to_string()))?;
        reload.validate()?;
        assert_eq!(reload, s.project);
        assert!(
            s.execute("pad.edit", json!({"id":id,"pad":"nope"}), None)
                .is_err()
        );
        s.execute("pad.remove", json!({"id":id,"pad":"TP"}), None)?;
        assert!(s.project.components.is_empty() && s.project.board.footprints.is_empty());
        Ok(())
    }

    #[test]
    fn a_pad_added_to_a_placed_part_gets_a_pin_and_keeps_the_part() -> Result<(), Error> {
        let (mut s, net) = session_with_net()?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":20.0,"y":20.0}}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let fp = s.project.board.footprints[0].id;
        let before = s.project.board.footprints[0].pads.len();
        let out = s.execute(
            "pad.add",
            json!({"footprint":fp,"offset":{"x":0.0,"y":3.0},"shape":"rect","width":2.0,"height":1.0,"net":net}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["pad"], "3");
        let f = &s.project.board.footprints[0];
        assert_eq!(f.pads.len(), before + 1);
        assert_eq!(f.pads[before].net, Some(net));
        assert_eq!(s.project.components[0].pins.len(), before + 1);
        s.execute("board.sync", json!({}), None)?;
        assert_eq!(s.project.board.footprints[0].pads.len(), before + 1);
        assert!(
            s.execute("pad.add", json!({"footprint":fp,"number":"1"}), None)
                .is_err()
        );
        assert!(
            s.execute("pad.add", json!({"footprint":fp,"side":"bottom"}), None)
                .is_err(),
            "a surface pad stays on the part's side"
        );
        s.execute("pad.remove", json!({"id":fp,"pad":"3"}), None)?;
        assert_eq!(s.project.board.footprints[0].pads.len(), before);
        assert!(
            s.execute("pad.remove", json!({"id":fp,"pad":"1"}), None)
                .is_ok()
        );
        assert!(
            s.execute("pad.remove", json!({"id":fp,"pad":"2"}), None)
                .is_err(),
            "the last pad of an ordinary part stays"
        );
        Ok(())
    }

    #[test]
    fn a_schematic_pad_appears_on_the_board_on_the_same_net() -> Result<(), Error> {
        let (mut s, net) = session_with_net()?;
        s.execute(
            "pad.add",
            json!({"schematic_position":{"x":40.0,"y":30.0},"net":net}),
            None,
        )?;
        assert_eq!(s.project.components[0].position, Point::new(40.0, 30.0));
        assert_eq!(s.project.board.footprints.len(), 1);
        assert_eq!(s.project.board.footprints[0].pads[0].net, Some(net));
        assert!(s.execute("pad.add", json!({}), None).is_err());
        Ok(())
    }
}
