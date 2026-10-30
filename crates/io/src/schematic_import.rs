//! Read interchange schematic text into a native [`Project`].
use pcbcraft_doc::{
    Component, ComponentField, DrawnWire, Label, LibrarySymbol, Net, NoConnectFlag, Port,
    Project, ProjectLibrary, Sheet,
};
use pcbcraft_geom::Point;
use pcbcraft_library::{parse_sexpr, read_symbols, Node, SymbolDefinition};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

const MIN_VERSION: i64 = 20211123;
const MAX_SYMBOLS: usize = 256;
const MAX_COMPONENTS: usize = 2048;
const MAX_WIRES: usize = 4096;
const MAX_LABELS: usize = 4096;
const MAX_SHEETS: usize = 64;
const MAX_PRESERVED_BYTES: usize = 512 * 1024;

pub fn read_schematic_interchange(text: &str) -> Result<Project, String> {
    read_schematic_interchange_path(Path::new("."), text)
}

pub fn read_schematic_interchange_path(path: &Path, text: &str) -> Result<Project, String> {
    let root = parse_sexpr(text).map_err(format_sexpr_error(text))?;
    if root.name() != "kicad_sch" {
        return Err("expected a schematic interchange root".into());
    }
    let version = parse_version(root.field("version"))?;
    if version < MIN_VERSION {
        return Err(
            "this schematic needs embedded symbol definitions; cache-library-only files are not supported".into(),
        );
    }
    let lib = root.child("lib_symbols").ok_or(
        "this schematic needs embedded symbol definitions; cache-library-only files are not supported"
            .to_string(),
    )?;
    let symbols = read_embedded_symbols(text, lib)?;
    let mut ctx = ImportCtx::new(path, symbols);
    ctx.parse_page(0, None, Vec::new(), &root, text)?;
    while let Some(job) = ctx.pending_pages.pop() {
        let child_text = std::fs::read_to_string(&job.path).map_err(|e| e.to_string())?;
        if child_text.len() > super::MAX_FILE_BYTES as usize {
            return Err("schematic page exceeds 8 MiB".into());
        }
        let child_root = parse_sexpr(&child_text).map_err(format_sexpr_error(&child_text))?;
        if child_root.name() != "kicad_sch" {
            return Err("expected a schematic interchange root".into());
        }
        ctx.parse_page(
            job.sheet_id,
            job.instance_uuid,
            job.parent_links,
            &child_root,
            &child_text,
        )?;
    }
    ctx.finish()
}

fn format_sexpr_error(text: &str) -> impl Fn(pcbcraft_library::Error) -> String {
    move |error| match error {
        pcbcraft_library::Error::Syntax { offset, message } => {
            format!("line {}: {message}", line_number(text, offset))
        }
        pcbcraft_library::Error::Limit(name) => format!("schematic exceeds {name}"),
        pcbcraft_library::Error::Invalid(message) => message,
    }
}

fn line_number(text: &str, offset: usize) -> usize {
    text.get(..offset.min(text.len()))
        .map(|prefix| prefix.lines().count())
        .unwrap_or(1)
        .max(1)
}

fn parse_version(raw: &str) -> Result<i64, String> {
    raw.parse::<i64>()
        .map_err(|_| format!("invalid schematic version {raw:?}"))
}

fn read_embedded_symbols(source: &str, lib: &Node) -> Result<BTreeMap<String, SymbolDefinition>, String> {
    let mut body = String::from("(kicad_symbol_lib (version 20250114)\n");
    for child in lib.children.iter().filter(|c| c.name() == "symbol") {
        body.push_str(&source[child.span.clone()]);
        body.push('\n');
    }
    body.push(')');
    let defs = read_symbols(&body).map_err(|e| e.to_string())?;
    if defs.is_empty() {
        return Err("schematic carries no embedded symbol definitions".into());
    }
    if defs.len() > MAX_SYMBOLS {
        return Err("schematic embeds too many symbol definitions".into());
    }
    Ok(defs
        .into_iter()
        .map(|d| (d.name.clone(), d))
        .collect())
}

fn prop(node: &Node, name: &str) -> Option<String> {
    node.children
        .iter()
        .find(|x| x.name() == "property" && x.value(1) == name)
        .map(|x| x.value(2).to_owned())
}

fn num(raw: &str) -> Result<f64, String> {
    let n = raw
        .parse::<f64>()
        .map_err(|_| format!("invalid number {raw:?}"))?;
    if !n.is_finite() || n.abs() > 100_000.0 {
        return Err("number outside finite geometry bounds".into());
    }
    Ok(n)
}

fn sch_point(node: &Node, field: &str) -> Result<Point, String> {
    let p = node
        .child(field)
        .ok_or_else(|| format!("missing {field}"))?;
    Ok(Point::new(num(p.value(1))?, num(p.value(2))?))
}

fn at_point(node: &Node) -> Result<(Point, f64), String> {
    let at = node
        .child("at")
        .ok_or_else(|| "placed symbol has no position".to_string())?;
    let rotation = if at.value(3).is_empty() {
        0.0
    } else {
        num(at.value(3))?
    };
    Ok((Point::new(num(at.value(1))?, num(at.value(2))?), rotation))
}

fn snap_key(sheet: u64, point: Point) -> (u64, i64, i64) {
    (
        sheet,
        (point.x * 10_000.0).round() as i64,
        (point.y * 10_000.0).round() as i64,
    )
}

struct PendingPage {
    path: PathBuf,
    sheet_id: u64,
    instance_uuid: Option<String>,
    parent_links: Vec<(String, u64, Point)>,
}

struct PageDraft {
    sheet_id: u64,
    sheet_name: String,
    sheet_uuid: Option<String>,
    components: Vec<Component>,
    wires: Vec<Vec<Point>>,
    junctions: Vec<Point>,
    labels: Vec<(String, String, Point, f64, String)>,
    no_connect_points: Vec<Point>,
    sheet_pins: Vec<(String, Point, Option<String>)>,
    hierarchical_labels: Vec<(String, Point)>,
    global_labels: Vec<(String, Point)>,
    power_nets: Vec<(String, Point)>,
    parent_links: Vec<(String, u64, Point)>,
    preserved_bytes: usize,
}

struct ImportCtx {
    base: Option<PathBuf>,
    symbols: BTreeMap<String, SymbolDefinition>,
    next_id: u64,
    pages: Vec<PageDraft>,
    pending_pages: Vec<PendingPage>,
    preserved_bytes: usize,
}

impl ImportCtx {
    fn new(base: &Path, symbols: BTreeMap<String, SymbolDefinition>) -> Self {
        Self {
            base: Some(base.to_path_buf()),
            symbols,
            next_id: 1,
            pages: Vec::new(),
            pending_pages: Vec::new(),
            preserved_bytes: 0,
        }
    }

    fn alloc(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn parse_page(
        &mut self,
        sheet_id: u64,
        sheet_uuid: Option<String>,
        parent_links: Vec<(String, u64, Point)>,
        root: &Node,
        source: &str,
    ) -> Result<(), String> {
        if self.pages.len() >= MAX_SHEETS {
            return Err("schematic hierarchy exceeds sheet limit".into());
        }
        let sheet_name = prop(root, "title")
            .or_else(|| prop(root, "Sheetname"))
            .unwrap_or_else(|| {
                if sheet_id == 0 {
                    "Root".into()
                } else {
                    format!("Sheet{sheet_id}")
                }
            });
        if sheet_name.is_empty() || sheet_name.len() > 128 {
            return Err("sheet name must be 1 to 128 bytes".into());
        }
        let mut draft = PageDraft {
            sheet_id,
            sheet_name,
            sheet_uuid,
            components: Vec::new(),
            wires: Vec::new(),
            junctions: Vec::new(),
            labels: Vec::new(),
            no_connect_points: Vec::new(),
            sheet_pins: Vec::new(),
            hierarchical_labels: Vec::new(),
            global_labels: Vec::new(),
            power_nets: Vec::new(),
            parent_links,
            preserved_bytes: 0,
        };
        for child in &root.children {
            match child.name() {
                "symbol" => {
                    let component = self.parse_symbol(sheet_id, child, &mut draft)?;
                    draft.components.push(component);
                }
                "wire" | "bus" => draft.wires.push(parse_wire_points(child)?),
                "junction" => draft.junctions.push(sch_point(child, "at")?),
                "label" => {
                    let (pos, rot) = label_anchor(child)?;
                    let text = child.value(1).to_owned();
                    draft.labels.push(("label".into(), text, pos, rot, String::new()));
                }
                "global_label" => {
                    let (pos, rot) = label_anchor(child)?;
                    let text = child.value(1).to_owned();
                    draft
                        .global_labels
                        .push((text.clone(), pos));
                    draft
                        .labels
                        .push(("global".into(), text, pos, rot, String::new()));
                }
                "hierarchical_label" => {
                    let (pos, rot) = label_anchor(child)?;
                    let text = child.value(1).to_owned();
                    draft.hierarchical_labels.push((text.clone(), pos));
                    draft.labels.push((
                        "hierarchical".into(),
                        text,
                        pos,
                        rot,
                        String::new(),
                    ));
                }
                "bus_label" => {
                    let (pos, rot) = label_anchor(child)?;
                    let text = child.value(1).to_owned();
                    draft
                        .labels
                        .push(("bus".into(), text.clone(), pos, rot, text));
                }
                "no_connect" => draft.no_connect_points.push(sch_point(child, "at")?),
                "sheet" => self.parse_sheet_instance(&mut draft, child, source)?,
                name if name != "kicad_sch"
                    && name != "version"
                    && name != "generator"
                    && name != "generator_version"
                    && name != "uuid"
                    && name != "paper"
                    && name != "lib_symbols"
                    && name != "title_block"
                    && name != "embedded_fonts"
                    && child.atom.is_none() =>
                {
                    draft.preserved_bytes += source[child.span.clone()].len();
                }
                _ => {}
            }
        }
        if draft.components.len() > MAX_COMPONENTS {
            return Err("schematic page exceeds component limit".into());
        }
        if draft.wires.len() > MAX_WIRES {
            return Err("schematic page exceeds wire limit".into());
        }
        if draft.labels.len() > MAX_LABELS {
            return Err("schematic page exceeds label limit".into());
        }
        self.preserved_bytes = self
            .preserved_bytes
            .saturating_add(draft.preserved_bytes)
            .min(MAX_PRESERVED_BYTES + 1);
        if self.preserved_bytes > MAX_PRESERVED_BYTES {
            return Err("preserved schematic nodes exceed the byte limit".into());
        }
        self.pages.push(draft);
        Ok(())
    }

    fn parse_sheet_instance(
        &mut self,
        draft: &mut PageDraft,
        node: &Node,
        _source: &str,
    ) -> Result<(), String> {
        let sheetfile = prop(node, "Sheetfile").ok_or_else(|| {
            "hierarchical sheet is missing a sheet file reference".to_string()
        })?;
        if sheetfile.is_empty() || sheetfile.len() > 256 {
            return Err("sheet file reference is out of range".into());
        }
        let base = self
            .base
            .as_deref()
            .ok_or_else(|| "hierarchical sheets require a file path".to_string())?;
        let path = base
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&sheetfile);
        let sheet_id = self.alloc();
        let parent_sheet = draft.sheet_id;
        let uuid = node
            .child("uuid")
            .map(|u| u.value(1).to_owned())
            .filter(|s| !s.is_empty() && s.len() <= 64);
        let mut parent_links = Vec::new();
        for pin in node.children.iter().filter(|c| c.name() == "pin") {
            let name = pin.value(1);
            if name.is_empty() || name.len() > 64 {
                return Err("sheet pin name is out of range".into());
            }
            let pos = sch_point(pin, "at")?;
            draft.sheet_pins.push((name.to_owned(), pos, uuid.clone()));
            parent_links.push((name.to_owned(), parent_sheet, pos));
        }
        self.pending_pages.push(PendingPage {
            path,
            sheet_id,
            instance_uuid: uuid,
            parent_links,
        });
        Ok(())
    }

    fn parse_symbol(
        &mut self,
        sheet_id: u64,
        node: &Node,
        draft: &mut PageDraft,
    ) -> Result<Component, String> {
        let lib_id = node
            .child("lib_id")
            .ok_or_else(|| "placed symbol has no lib_id".to_string())?
            .value(1);
        if lib_id.is_empty() || lib_id.len() > 128 {
            return Err("symbol lib_id is out of range".into());
        }
        let definition = self
            .symbols
            .get(lib_id)
            .ok_or_else(|| format!("unknown embedded symbol {lib_id}"))?
            .clone();
        if !definition.unsupported.is_empty() {
            return Err(format!(
                "embedded symbol {lib_id} uses unsupported features"
            ));
        }
        let (position, rotation) = at_point(node)?;
        let reference = prop(node, "Reference")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| definition.reference.clone());
        if reference.is_empty() || reference.len() > 64 {
            return Err("component reference is out of range".into());
        }
        let value = prop(node, "Value").unwrap_or_else(|| definition.value.clone());
        if value.len() > 256 {
            return Err("component value is out of range".into());
        }
        let footprint = prop(node, "Footprint").unwrap_or_default();
        let datasheet = prop(node, "Datasheet").unwrap_or_default();
        let mut fields = Vec::new();
        if !footprint.is_empty() {
            fields.push(ComponentField {
                key: "Footprint".into(),
                value: footprint,
            });
        }
        if !datasheet.is_empty() {
            fields.push(ComponentField {
                key: "Datasheet".into(),
                value: datasheet,
            });
        }
        for child in node.children.iter().filter(|c| c.name() == "property") {
            let key = child.value(1);
            if matches!(key, "Reference" | "Value" | "Footprint" | "Datasheet") {
                continue;
            }
            if key.is_empty() || key.len() > 64 {
                return Err("property key is out of range".into());
            }
            let val = child.value(2);
            if val.len() > 256 {
                return Err("property value is out of range".into());
            }
            fields.push(ComponentField {
                key: key.to_owned(),
                value: val.to_owned(),
            });
        }
        let id = self.alloc();
        let mut component = pcbcraft_doc::library_component(
            definition.clone(),
            "embedded".into(),
            id,
            reference,
            position,
            Some(value),
        )?;
        component.rotation = rotation.rem_euclid(360.0);
        component.sheet = sheet_id;
        component.fields = fields;
        component.kind = if lib_id.starts_with("power:") {
            "power".into()
        } else {
            "library".into()
        };
        component.library_symbol = Some(LibrarySymbol {
            library: "embedded".into(),
            unit: 1,
            definition,
        });
        if lib_id.starts_with("power:") {
            let net_name = lib_id
                .split(':')
                .nth(1)
                .filter(|s| !s.is_empty())
                .unwrap_or("GND")
                .to_owned();
            if let Some(pin) = component.pins.first() {
                if let Some(pos) = component.pin_position(&pin.number) {
                    draft.power_nets.push((net_name, pos));
                }
            }
        }
        Ok(component)
    }

    fn finish(mut self) -> Result<Project, String> {
        let pages = std::mem::take(&mut self.pages);
        let mut project = Project::default();
        project.name = pages
            .first()
            .map(|p| p.sheet_name.clone())
            .unwrap_or_else(|| "Imported schematic".into());
        project.library = ProjectLibrary {
            symbols: self.symbols.into_values().collect(),
            ..ProjectLibrary::default()
        };
        for page in &pages {
            if page.sheet_id != 0 {
                project.sheets.push(Sheet {
                    id: page.sheet_id,
                    name: page.sheet_name.clone(),
                    interchange_uuid: page.sheet_uuid.clone(),
                    parent: Some(0),
                    entry: None,
                });
            }
        }
        project.components = pages
            .iter()
            .flat_map(|p| p.components.clone())
            .collect();

        let mut uf = UnionFind::new();
        let mut keys: HashMap<(u64, i64, i64), usize> = HashMap::new();

        for page in &pages {
            for wire in &page.wires {
                for window in wire.windows(2) {
                    let a = connection_key(&mut keys, &mut uf, page.sheet_id, window[0]);
                    let b = connection_key(&mut keys, &mut uf, page.sheet_id, window[1]);
                    uf.union(a, b);
                }
            }
            for junction in &page.junctions {
                let _ = connection_key(&mut keys, &mut uf, page.sheet_id, *junction);
            }
            for component in &page.components {
                for pin in &component.pins {
                    if let Some(pos) = component.pin_position(&pin.number) {
                        let _ = connection_key(&mut keys, &mut uf, component.sheet, pos);
                    }
                }
            }
        }

        let mut named: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();

        for page in &pages {
            for (kind, text, pos, _, bus) in &page.labels {
                if kind == "bus" {
                    continue;
                }
                attach_net_name(&mut named, &mut keys, &mut uf, text, page.sheet_id, *pos)?;
                if !bus.is_empty() {
                    attach_net_name(&mut named, &mut keys, &mut uf, bus, page.sheet_id, *pos)?;
                }
            }
            for (text, pos) in &page.global_labels {
                attach_net_name(&mut named, &mut keys, &mut uf, text, page.sheet_id, *pos)?;
            }
            for (text, pos) in &page.hierarchical_labels {
                attach_net_name(&mut named, &mut keys, &mut uf, text, page.sheet_id, *pos)?;
                for (pin_name, parent_sheet, parent_pos) in &page.parent_links {
                    if pin_name == text {
                        let child_idx =
                            connection_key(&mut keys, &mut uf, page.sheet_id, *pos);
                        let parent_idx =
                            connection_key(&mut keys, &mut uf, *parent_sheet, *parent_pos);
                        uf.union(child_idx, parent_idx);
                    }
                }
            }
            for (text, pos) in &page.power_nets {
                attach_net_name(&mut named, &mut keys, &mut uf, text, page.sheet_id, *pos)?;
            }
        }

        for page in &pages {
            for (pin_name, pos, _) in &page.sheet_pins {
                attach_net_name(
                    &mut named,
                    &mut keys,
                    &mut uf,
                    pin_name,
                    page.sheet_id,
                    *pos,
                )?;
            }
        }

        for names in named.values() {
            let mut iter = names.iter();
            if let Some(first) = iter.next() {
                for other in iter {
                    uf.union(*first, *other);
                }
            }
        }

        let mut root_name: BTreeMap<usize, String> = BTreeMap::new();
        for (name, keys) in &named {
            if let Some(rep) = keys.iter().copied().min() {
                let root = uf.find(rep);
                root_name
                    .entry(root)
                    .and_modify(|current| {
                        if name < current {
                            *current = name.clone();
                        }
                    })
                    .or_insert_with(|| name.clone());
            }
        }
        let mut net_ids: BTreeMap<usize, u64> = BTreeMap::new();
        let mut nets: Vec<Net> = Vec::new();
        for (root, name) in root_name {
            let id = self.alloc();
            net_ids.insert(root, id);
            nets.push(Net {
                id,
                name,
                target_length: None,
                length_tolerance: None,
            });
        }

        let mut orphan = 1usize;
        let roots: BTreeSet<usize> = keys.values().map(|&idx| uf.find(idx)).collect();
        for root in roots {
            if net_ids.contains_key(&root) {
                continue;
            }
            let id = self.alloc();
            net_ids.insert(root, id);
            nets.push(Net {
                id,
                name: format!("Net{orphan}"),
                target_length: None,
                length_tolerance: None,
            });
            orphan += 1;
        }

        for page in &pages {
            for wire in &page.wires {
                let id = self.alloc();
                let net = wire.first().and_then(|p| {
                    lookup_net_id(&keys, &mut uf, &net_ids, page.sheet_id, *p)
                });
                project.drawn_wires.push(DrawnWire {
                    id,
                    net,
                    points: wire.clone(),
                });
            }
            for (kind, text, pos, rot, bus) in &page.labels {
                let id = self.alloc();
                let net = if kind == "bus" {
                    None
                } else {
                    lookup_net_id(&keys, &mut uf, &net_ids, page.sheet_id, *pos)
                };
                project.labels.push(Label {
                    id,
                    kind: kind.clone(),
                    net: net.ok_or_else(|| format!("label {text} is not on a derived net"))?,
                    position: *pos,
                    rotation: *rot,
                    text: bus.clone(),
                });
            }
            for junction in &page.junctions {
                let id = self.alloc();
                let net = lookup_net_id(&keys, &mut uf, &net_ids, page.sheet_id, *junction)
                    .ok_or_else(|| "junction is not on a derived net".to_string())?;
                project.labels.push(Label {
                    id,
                    kind: "junction".into(),
                    net,
                    position: *junction,
                    rotation: 0.0,
                    text: String::new(),
                });
            }
            for (pin_name, pos, uuid) in &page.sheet_pins {
                let id = self.alloc();
                let net = lookup_net_id(&keys, &mut uf, &net_ids, page.sheet_id, *pos).unwrap_or(0);
                project.ports.push(Port {
                    id,
                    name: pin_name.clone(),
                    net,
                    sheet: page.sheet_id,
                    position: *pos,
                    interchange_uuid: uuid.clone(),
                });
            }
        }

        let mut pin_nets: Vec<(u64, String, Option<u64>)> = Vec::new();
        for component in &project.components {
            for pin in &component.pins {
                if let Some(pos) = component.pin_position(&pin.number) {
                    pin_nets.push((
                        component.id,
                        pin.number.clone(),
                        lookup_net_id(&keys, &mut uf, &net_ids, component.sheet, pos),
                    ));
                }
            }
        }
        for component in &mut project.components {
            for pin in &mut component.pins {
                if let Some((_, _, net)) = pin_nets
                    .iter()
                    .find(|(id, number, _)| *id == component.id && *number == pin.number)
                {
                    pin.net = *net;
                }
            }
        }

        for page in &pages {
            for point in &page.no_connect_points {
                if let Some((component_id, pin_number)) =
                    find_pin_at(&project.components, page.sheet_id, *point)
                {
                    project.no_connects.push(NoConnectFlag {
                        id: self.alloc(),
                        pin: pcbcraft_doc::PinRef {
                            component: component_id,
                            pin: pin_number,
                        },
                    });
                }
            }
        }

        project.nets = nets;
        project.validate()?;
        Ok(project)
    }
}

fn connection_key(
    keys: &mut HashMap<(u64, i64, i64), usize>,
    uf: &mut UnionFind,
    sheet: u64,
    point: Point,
) -> usize {
    let key = snap_key(sheet, point);
    *keys
        .entry(key)
        .or_insert_with(|| uf.insert())
}

fn attach_net_name(
    named: &mut BTreeMap<String, BTreeSet<usize>>,
    keys: &mut HashMap<(u64, i64, i64), usize>,
    uf: &mut UnionFind,
    name: &str,
    sheet: u64,
    point: Point,
) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err("net name is out of range".into());
    }
    let idx = connection_key(keys, uf, sheet, point);
    named.entry(name.to_owned()).or_default().insert(idx);
    Ok(())
}

fn lookup_net_id(
    keys: &HashMap<(u64, i64, i64), usize>,
    uf: &mut UnionFind,
    net_ids: &BTreeMap<usize, u64>,
    sheet: u64,
    point: Point,
) -> Option<u64> {
    let idx = keys.get(&snap_key(sheet, point)).copied()?;
    let root = uf.find(idx);
    net_ids.get(&root).copied()
}

fn find_pin_at(
    components: &[Component],
    sheet: u64,
    point: Point,
) -> Option<(u64, String)> {
    let target = snap_key(sheet, point);
    for component in components.iter().filter(|c| c.sheet == sheet) {
        for pin in &component.pins {
            if let Some(pos) = component.pin_position(&pin.number) {
                if snap_key(sheet, pos) == target {
                    return Some((component.id, pin.number.clone()));
                }
            }
        }
    }
    None
}

fn label_anchor(node: &Node) -> Result<(Point, f64), String> {
    let at = node
        .child("at")
        .ok_or_else(|| "label has no position".to_string())?;
    let rotation = if at.value(3).is_empty() {
        0.0
    } else {
        num(at.value(3))?
    };
    Ok((Point::new(num(at.value(1))?, num(at.value(2))?), rotation))
}

fn parse_wire_points(node: &Node) -> Result<Vec<Point>, String> {
    let pts = node
        .child("pts")
        .ok_or_else(|| "wire has no points".to_string())?;
    let mut points = Vec::new();
    for xy in pts.children.iter().filter(|c| c.name() == "xy") {
        points.push(Point::new(num(xy.value(1))?, num(xy.value(2))?));
    }
    if points.len() < 2 {
        return Err("wire needs at least two points".into());
    }
    Ok(points)
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new() -> Self {
        Self { parent: Vec::new() }
    }
    fn insert(&mut self) -> usize {
        let id = self.parent.len();
        self.parent.push(id);
        id
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            let root = self.parent[x];
            self.parent[x] = self.parent[root];
            x = root;
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let a = self.find(a);
        let b = self.find(b);
        if a != b {
            self.parent[b] = a;
        }
    }
}

pub fn import_summary(project: &Project, preserved_bytes: usize) -> serde_json::Value {
    serde_json::json!({
        "name": project.name,
        "components": project.components.len(),
        "nets": project.nets.len(),
        "sheets": project.sheets.len(),
        "labels": project.labels.len(),
        "preserved_bytes": preserved_bytes,
    })
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn line_numbers_start_at_one() {
        assert_eq!(line_number("a\nb", 2), 2);
    }
}
