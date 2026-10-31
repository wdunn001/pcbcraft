//! Read a bracketed board interchange file into a native project.
//!
//! Context: the board reader behind `project.import` with format `board`. It
//! accepts files from the older ordinal-net generation and from the current
//! one that names nets inside every object, with any number of copper layers.
//!
//! # Rules
//! - Never panic and never refuse a file for one odd construct: anything the
//!   native model cannot hold becomes a notice in the [`ImportReport`], and
//!   only a file that is not a board at all is an error.
//! - Every object gets a fresh sequential id; the source uuid is kept as
//!   preserved text on the object so a later export can write it back.
//! - Source coordinates move so the board outline starts at the origin.
//!
//! # Dependencies
//! - `crates/library/src/reader.rs` reads each footprint definition.
//! - `crates/doc/src/lib.rs` owns the target model and `Project::validate`.
//! - `crates/io/src/lib.rs` writes the same layer table back in `kicad_board`.
//!
//! # Do not
//! - Copy or embed any design file in a test; tests read corpus files from a
//!   path an environment variable names, and build small fixtures by hand.
//! - Resolve 3D model path variables; the path stays a reference.
use pcbcraft_doc::{
    Component, CopperZone, Cutout, Footprint, Id, Keepout, Layer, LayerRecord, LibraryFootprint,
    Net, Pad, Pin, PreservedSource, Project, RuleArea, RuleRegion, Track, Via,
};
use pcbcraft_geom::Point;
use pcbcraft_library::{
    Error as LibraryError, FootprintDefinition, FootprintPad, Limits, Node, line_number_at_offset,
    parse_limits, read_footprint,
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_NETS: usize = 8192;
const MAX_COMPONENTS: usize = 8192;
const MAX_ZONE_VERTICES: usize = 255;
const MAX_OUTLINE_VERTICES: usize = 64;
/// Smallest clearance a keepout rule area may carry.
const KEEPOUT_CLEARANCE: f64 = 0.01;
const JOIN_TOLERANCE: f64 = 1e-3;

/// Counts of the main object kinds, used for source versus imported tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportCounts {
    /// Placed parts (footprints).
    pub parts: usize,
    /// Pads on those parts.
    pub pads: usize,
    /// Distinct nets.
    pub nets: usize,
    /// Track segments and arcs.
    pub tracks: usize,
    /// Vias of every span.
    pub vias: usize,
    /// Copper zones, one per layer.
    pub zones: usize,
    /// Keepout areas (rule areas on import).
    pub keepouts: usize,
    /// Copper layers.
    pub layers: usize,
    /// Footprints that name a 3D model.
    pub models: usize,
}

/// What an import did: counts in the file, counts in the project and notices.
///
/// # Rules
/// - `notices` are plain sentences, grouped so one kind appears once with a
///   count and a first example.
///
/// # Do not
/// - Treat a notice as a failure; the project is complete and valid.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Counts found in the source file.
    pub source: ImportCounts,
    /// Counts in the imported project.
    pub imported: ImportCounts,
    /// Things the native model could not hold, grouped by kind.
    pub notices: Vec<String>,
}

/// Read a board file and return the project only.
///
/// # Rules
/// - Same behaviour as [`read_board_interchange_report`], report dropped.
///
/// # Dependencies
/// - `crates/io/src/board_import.rs` `read_board_interchange_report`.
pub fn read_board_interchange(text: &str) -> Result<Project, String> {
    read_board_interchange_report(text).map(|(project, _)| project)
}

/// Read a board file into a valid project plus an import report.
///
/// # Rules
/// - Errors only for text that is not a board file or breaks a parser limit.
/// - The returned project passed `Project::validate`.
///
/// # Dependencies
/// - `crates/library/src/sexpr.rs` parser limits (4 million nodes here).
///
/// # Do not
/// - Add a panic path; use a notice for anything unexpected.
pub fn read_board_interchange_report(text: &str) -> Result<(Project, ImportReport), String> {
    read_board_interchange_with_rules(text, None)
}

/// Read a board file and, when the text of its project file is given, take the
/// design rules from it: board clearance, minimum track and space, minimum
/// drill and via ring.
///
/// # Rules
/// - A project file that is not valid JSON or lacks the rules is ignored with
///   a notice; it never fails the import.
///
/// # Dependencies
/// - `crates/engine/src/lib.rs` passes the sibling project file of the board.
pub fn read_board_interchange_with_rules(
    text: &str,
    project_file: Option<&str>,
) -> Result<(Project, ImportReport), String> {
    let limits = Limits {
        nodes: 4_000_000,
        ..Limits::default()
    };
    let root = parse_limits(text, limits).map_err(|e| format_parse_error(text, e))?;
    if root.name() != "kicad_pcb" {
        return Err("expected a board interchange file".into());
    }
    let version = root.field("version");
    if !version.is_empty() && version.parse::<u32>().is_err() {
        return Err(format!("unsupported board file version {version:?}"));
    }
    let mut ctx = ImportCtx::new(text);
    if let Some(pro) = project_file {
        ctx.read_rules(pro);
    }
    ctx.prepare(&root);
    let mut board = Project::default().board;
    ctx.read_general(&root, &mut board);
    ctx.apply_layers(&root, &mut board);
    ctx.shift_board(&mut board);

    for child in &root.children {
        if child.atom.is_some() {
            continue;
        }
        match child.name() {
            "footprint" | "module" => ctx.import_footprint(child, &mut board),
            "segment" => ctx.import_segment(child, &mut board, false),
            "arc" => ctx.import_segment(child, &mut board, true),
            "via" => ctx.import_via(child, &mut board),
            "zone" => ctx.import_zone(child, &mut board),
            "net" | "general" | "layers" | "setup" | "version" | "generator"
            | "generator_version" => {}
            "aux_axis_origin" => {
                if let Some(p) = ctx.point_of(child) {
                    board.drill_origin = ctx.shift(p);
                }
            }
            "gr_circle" if child.field("layer") == "Edge.Cuts" => {
                ctx.import_circle_cutout(child, &mut board)
            }
            "gr_rect" | "gr_poly" | "gr_line" | "gr_arc" if child.field("layer") == "Edge.Cuts" => {
            }
            "paper" | "title_block" | "embedded_fonts" | "property" => {
                ctx.keep(&mut board.preserved, child, "board header node");
            }
            other => {
                ctx.keep(&mut board.preserved, child, other);
            }
        }
    }
    ctx.finish_outline(&mut board);
    ctx.finish(board)
}

/// One cutting line of the board edge, in source coordinates.
#[derive(Clone, Copy, Debug)]
struct EdgePiece {
    a: Point,
    b: Point,
    mid: Option<Point>,
}

struct ImportCtx {
    source: String,
    notices: BTreeMap<String, (usize, String)>,
    next_id: Id,
    components: Vec<Component>,
    nets: Vec<Net>,
    net_by_name: BTreeMap<String, Id>,
    net_table: BTreeMap<u32, String>,
    refs: BTreeSet<String>,
    copper: Vec<String>,
    origin: Point,
    edge: Vec<EdgePiece>,
    edge_circles: Vec<(Point, Point)>,
    outline_src: Vec<EdgePiece>,
    rules: Option<(f64, pcbcraft_doc::FabRules)>,
    src: ImportCounts,
    spans: BTreeSet<(u8, u8)>,
    unassigned: Option<Id>,
    seen_nets: BTreeSet<String>,
    has_stackup: bool,
}

impl ImportCtx {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            notices: BTreeMap::new(),
            next_id: 1,
            components: Vec::new(),
            nets: Vec::new(),
            net_by_name: BTreeMap::new(),
            net_table: BTreeMap::new(),
            refs: BTreeSet::new(),
            copper: vec!["F.Cu".into(), "B.Cu".into()],
            origin: Point::default(),
            edge: Vec::new(),
            edge_circles: Vec::new(),
            outline_src: Vec::new(),
            rules: None,
            src: ImportCounts::default(),
            spans: BTreeSet::new(),
            unassigned: None,
            seen_nets: BTreeSet::new(),
            has_stackup: false,
        }
    }

    /// Record one notice; repeats of a kind collapse into a count.
    fn notice(&mut self, kind: &str, detail: &str) {
        let entry = self
            .notices
            .entry(kind.to_owned())
            .or_insert_with(|| (0, detail.to_owned()));
        entry.0 += 1;
    }

    /// Design rules from the text of the board's project file.
    fn read_rules(&mut self, pro: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(pro) else {
            self.notice(
                "the project file could not be read; design rules not imported",
                "",
            );
            return;
        };
        let rules = &v["board"]["design_settings"]["rules"];
        let get = |x: &serde_json::Value| {
            x.as_f64()
                .filter(|n| n.is_finite() && *n > 0.0 && *n <= 100.0)
        };
        let class = v["net_settings"]["classes"]
            .as_array()
            .and_then(|c| c.first())
            .and_then(|c| get(&c["clearance"]));
        let Some(clearance) = class.or_else(|| get(&rules["min_clearance"])) else {
            return;
        };
        let fab = pcbcraft_doc::FabRules {
            min_track: get(&rules["min_track_width"]),
            min_drill: get(&rules["min_through_hole_diameter"]),
            min_ring_via: get(&rules["min_via_annular_width"]),
            ..pcbcraft_doc::FabRules::default()
        };
        self.rules = Some((clearance, fab));
    }

    fn alloc(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn num(&self, raw: &str) -> Option<f64> {
        raw.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && n.abs() <= 100_000.0)
    }

    fn point_of(&self, node: &Node) -> Option<Point> {
        Some(Point::new(
            self.num(node.value(1))?,
            self.num(node.value(2))?,
        ))
    }

    fn xy(&self, node: &Node, field: &str) -> Option<Point> {
        self.point_of(node.child(field)?)
    }

    fn shift(&self, p: Point) -> Point {
        Point::new(p.x - self.origin.x, p.y - self.origin.y)
    }

    /// Keep an unknown subtree as preserved text, or count it as dropped when
    /// the per-object limit is reached.
    fn keep(&mut self, preserved: &mut PreservedSource, node: &Node, what: &str) {
        if preserved
            .push_span(&self.source, node.span.clone())
            .is_err()
        {
            self.notice(
                "unknown nodes dropped because the preserved-text limit was reached",
                what,
            );
        }
    }

    /// First pass: net table, copper layer names, edge pieces, source counts
    /// and the origin shift.
    fn prepare(&mut self, root: &Node) {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for child in &root.children {
            match child.name() {
                "net" if child.atom.is_none() => {
                    if let Ok(ord) = child.value(1).parse::<u32>() {
                        let name = child.value(2).to_owned();
                        if ord != 0 && !name.is_empty() {
                            names.insert(name.clone());
                        }
                        self.net_table.insert(ord, name);
                    }
                }
                "layers" => {
                    let mut copper = Vec::new();
                    for entry in child.children.iter().filter(|c| c.atom.is_none()) {
                        let name = entry.value(1);
                        if entry.name().parse::<u32>().is_ok()
                            && matches!(entry.value(2), "signal" | "power" | "mixed" | "jumper")
                            && !name.is_empty()
                            && !copper.iter().any(|c: &String| c == name)
                        {
                            copper.push(name.to_owned());
                        }
                    }
                    if copper.len() >= 2 && copper.len() <= 32 {
                        self.copper = copper;
                    } else if !copper.is_empty() {
                        self.notice(
                            "copper layer table not usable; assumed two copper layers",
                            &format!("{} copper entries", copper.len()),
                        );
                    }
                }
                "footprint" | "module" => self.src.parts += 1,
                "segment" | "arc" => self.src.tracks += 1,
                "via" => self.src.vias += 1,
                "zone" => {
                    if child.child("keepout").is_some() {
                        self.src.keepouts += 1;
                    } else {
                        self.src.zones += 1;
                    }
                }
                "gr_line" | "gr_arc" | "gr_rect" | "gr_poly"
                    if child.field("layer") == "Edge.Cuts" =>
                {
                    self.collect_edge(child);
                }
                "gr_circle" if child.field("layer") == "Edge.Cuts" => {
                    if let (Some(c), Some(e)) = (self.xy(child, "center"), self.xy(child, "end")) {
                        self.edge_circles.push((c, e));
                    }
                }
                _ => {}
            }
            if matches!(child.name(), "segment" | "via" | "zone" | "arc") {
                self.scan_net_names(child, &mut names);
            }
            if matches!(child.name(), "footprint" | "module") {
                for pad in child.children.iter().filter(|c| c.name() == "pad") {
                    self.src.pads += 1;
                    self.scan_net_names(pad, &mut names);
                }
                if child.child("model").is_some() {
                    self.src.models += 1;
                }
                self.collect_part_edge(child);
            }
        }
        self.src.nets = names.len();
        self.src.layers = self.copper.len();
        self.build_outline();
        // Origin: top-left of the outline, else of all footprints.
        let mut min: Option<Point> = None;
        if !self.outline_src.is_empty() {
            let points: Vec<Point> = self.outline_src.iter().map(|p| p.a).collect();
            let arcs: Vec<Option<Point>> = self.outline_src.iter().map(|p| p.mid).collect();
            if let Some((lo, _)) = pcbcraft_doc::outline_extent(&points, &arcs) {
                grow_min(&mut min, lo);
            }
        }
        if self.outline_src.is_empty() {
            for piece in &self.edge {
                grow_min(&mut min, piece.a);
                grow_min(&mut min, piece.b);
                if let Some(m) = piece.mid {
                    for q in arc_samples(piece.a, m, piece.b) {
                        grow_min(&mut min, q);
                    }
                }
            }
            for (c, e) in &self.edge_circles {
                let r = c.distance(*e);
                grow_min(&mut min, Point::new(c.x - r, c.y - r));
            }
        }
        if min.is_none() {
            for child in &root.children {
                match child.name() {
                    "footprint" | "module" => {
                        if let Some(p) = child.child("at").and_then(|a| self.point_of(a)) {
                            grow_min(&mut min, p);
                        }
                    }
                    "segment" => {
                        if let Some(p) = self.xy(child, "start") {
                            grow_min(&mut min, p);
                        }
                    }
                    "via" => {
                        if let Some(p) = child.child("at").and_then(|a| self.point_of(a)) {
                            grow_min(&mut min, p);
                        }
                    }
                    _ => {}
                }
            }
            if min.is_some() {
                self.notice(
                    "no board outline in the file; the board is sized from its copper",
                    "no Edge.Cuts graphics",
                );
            }
        }
        self.origin = min.unwrap_or_default();
        if min.is_none() {
            self.notice(
                "no board outline or copper in the file",
                "an empty board was created",
            );
        }
    }

    fn scan_net_names(&self, node: &Node, names: &mut BTreeSet<String>) {
        if let Some(name) = self.net_name_of(node)
            && !name.is_empty()
        {
            names.insert(name);
        }
    }

    /// The net name an object refers to, in either file generation.
    fn net_name_of(&self, node: &Node) -> Option<String> {
        let net = node.child("net")?;
        let named = net.value(2);
        if !named.is_empty() {
            return Some(named.to_owned());
        }
        let first = net.value(1);
        if first.is_empty() {
            return Some(String::new());
        }
        if let Ok(ord) = first.parse::<u32>()
            && let Some(name) = self.net_table.get(&ord)
        {
            return Some(name.clone());
        }
        if let Ok(0) = first.parse::<u32>() {
            return Some(String::new());
        }
        Some(first.to_owned())
    }

    fn collect_edge(&mut self, node: &Node) {
        match node.name() {
            "gr_line" => {
                if let (Some(a), Some(b)) = (self.xy(node, "start"), self.xy(node, "end")) {
                    self.edge.push(EdgePiece { a, b, mid: None });
                }
            }
            "gr_arc" => {
                if let (Some(a), Some(m), Some(b)) = (
                    self.xy(node, "start"),
                    self.xy(node, "mid"),
                    self.xy(node, "end"),
                ) {
                    self.edge.push(EdgePiece { a, b, mid: Some(m) });
                }
            }
            "gr_rect" => {
                if let (Some(a), Some(c)) = (self.xy(node, "start"), self.xy(node, "end")) {
                    let b = Point::new(c.x, a.y);
                    let d = Point::new(a.x, c.y);
                    for (p, q) in [(a, b), (b, c), (c, d), (d, a)] {
                        self.edge.push(EdgePiece {
                            a: p,
                            b: q,
                            mid: None,
                        });
                    }
                }
            }
            "gr_poly" => {
                if let Some(pts) = node.child("pts") {
                    let list = self.pts(pts);
                    for pair in list.windows(2) {
                        self.edge.push(EdgePiece {
                            a: pair[0],
                            b: pair[1],
                            mid: None,
                        });
                    }
                    if let (Some(first), Some(last)) = (list.first(), list.last())
                        && first != last
                    {
                        self.edge.push(EdgePiece {
                            a: *last,
                            b: *first,
                            mid: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// Edge graphics drawn inside a part (a board outline kept as a part).
    fn collect_part_edge(&mut self, part: &Node) {
        let Some(at) = part.child("at") else { return };
        let (Some(x), Some(y)) = (self.num(at.value(1)), self.num(at.value(2))) else {
            return;
        };
        let rot = self.num(at.value(3)).unwrap_or(0.0).to_radians();
        let (sin, cos) = rot.sin_cos();
        let world = |p: Point| Point::new(x + p.x * cos + p.y * sin, y - p.x * sin + p.y * cos);
        for child in &part.children {
            if child.atom.is_some() || child.field("layer") != "Edge.Cuts" {
                continue;
            }
            match child.name() {
                "fp_line" => {
                    if let (Some(a), Some(b)) = (self.xy(child, "start"), self.xy(child, "end")) {
                        self.edge.push(EdgePiece {
                            a: world(a),
                            b: world(b),
                            mid: None,
                        });
                    }
                }
                "fp_arc" => {
                    if let (Some(a), Some(m), Some(b)) = (
                        self.xy(child, "start"),
                        self.xy(child, "mid"),
                        self.xy(child, "end"),
                    ) {
                        self.edge.push(EdgePiece {
                            a: world(a),
                            b: world(b),
                            mid: Some(world(m)),
                        });
                    }
                }
                "fp_rect" => {
                    if let (Some(a), Some(c)) = (self.xy(child, "start"), self.xy(child, "end")) {
                        let corners = [a, Point::new(c.x, a.y), c, Point::new(a.x, c.y)];
                        for i in 0..4 {
                            self.edge.push(EdgePiece {
                                a: world(corners[i]),
                                b: world(corners[(i + 1) % 4]),
                                mid: None,
                            });
                        }
                    }
                }
                "fp_poly" => {
                    if let Some(pts) = child.child("pts") {
                        let list = self.pts(pts);
                        for i in 0..list.len() {
                            self.edge.push(EdgePiece {
                                a: world(list[i]),
                                b: world(list[(i + 1) % list.len()]),
                                mid: None,
                            });
                        }
                    }
                }
                "fp_circle" => {
                    if let (Some(c), Some(e)) = (self.xy(child, "center"), self.xy(child, "end")) {
                        self.edge_circles.push((world(c), world(e)));
                    }
                }
                _ => {}
            }
        }
    }

    fn pts(&self, node: &Node) -> Vec<Point> {
        node.children
            .iter()
            .filter(|c| c.name() == "xy" || c.name() == "start")
            .filter_map(|c| self.point_of(c))
            .collect()
    }

    fn read_general(&mut self, root: &Node, board: &mut pcbcraft_doc::Board) {
        if let Some(general) = root.child("general")
            && let Some(t) = self.num(general.field("thickness"))
            && t > 0.0
            && t <= 100.0
        {
            board.thickness = t;
        }
    }

    /// Fill the stackup records from the setup block, or build plain copper
    /// rows when the file lists inner copper without one.
    fn apply_layers(&mut self, root: &Node, board: &mut pcbcraft_doc::Board) {
        let mut stack: Vec<LayerRecord> = Vec::new();
        let setup = root.child("setup");
        if let Some(setup) = setup {
            if let Some(stackup) = setup.child("stackup") {
                for row in stackup.children.iter().filter(|c| c.name() == "layer") {
                    if let Some(rec) = self.stack_row(row) {
                        stack.push(rec);
                    }
                }
                let finish = stackup.field("copper_finish");
                if !finish.is_empty() && !finish.eq_ignore_ascii_case("none") && finish.len() <= 64
                {
                    board.finish = finish.to_owned();
                }
            }
            for child in setup.children.iter().filter(|c| c.atom.is_none()) {
                if matches!(child.name(), "stackup" | "pad_to_mask_clearance") {
                    continue;
                }
                self.keep(&mut board.preserved, child, "setup");
            }
        }
        if let Some(first) = stack.first().map(|r| r.name.clone())
            && let Some(again) = stack.iter().skip(1).position(|r| r.name == first)
        {
            stack.truncate(again + 1);
            self.notice(
                "the stackup lists its rows twice; the second list was ignored",
                &first,
            );
        }
        let copper_rows: Vec<&str> = stack
            .iter()
            .filter(|r| r.kind == "copper")
            .map(|r| r.name.as_str())
            .collect();
        let matches_table = copper_rows.len() == self.copper.len()
            && copper_rows.iter().zip(&self.copper).all(|(a, b)| a == b);
        if matches_table && stack.len() <= 32 {
            self.has_stackup = true;
            if let Some(er) = stack
                .iter()
                .find(|r| r.is_dielectric())
                .and_then(|r| r.dk)
                .filter(|e| (1.0..=20.0).contains(e))
            {
                board.er = er;
            }
            board.layer_records = stack;
        } else if self.copper.len() > 2 || !stack.is_empty() {
            if !stack.is_empty() {
                self.notice(
                    "stackup rows do not match the copper layer table; a plain stackup was built",
                    "copper rows differ from the layer table",
                );
            } else {
                self.notice(
                    "no stackup in the file; a plain stackup was built",
                    "inner copper layers without stackup rows",
                );
            }
            board.layer_records = self.plain_stack(board.thickness);
        } else {
            // Two copper layers and no usable stackup: keep the default.
            board.layer_records = Vec::new();
        }
        self.src.layers = self.copper.len();
    }

    fn plain_stack(&self, thickness: f64) -> Vec<LayerRecord> {
        let n = self.copper.len();
        let copper_t = 0.035;
        let gaps = (n - 1).max(1) as f64;
        let gap = ((thickness - copper_t * n as f64) / gaps).max(0.05);
        let mut rows = Vec::new();
        for (i, name) in self.copper.iter().enumerate() {
            rows.push(LayerRecord {
                name: name.clone(),
                kind: "copper".into(),
                thickness: copper_t,
                copper_role: Some("signal".into()),
                ..LayerRecord::default()
            });
            if i + 1 < n {
                rows.push(LayerRecord {
                    name: format!("dielectric {}", i + 1),
                    kind: if i % 2 == 0 { "prepreg" } else { "core" }.into(),
                    thickness: gap,
                    material: Some("FR4".into()),
                    ..LayerRecord::default()
                });
            }
        }
        rows
    }

    fn stack_row(&mut self, row: &Node) -> Option<LayerRecord> {
        let name = row.value(1).to_owned();
        if name.is_empty() || name.len() > 64 {
            return None;
        }
        let kind = match row.field("type").to_ascii_lowercase().as_str() {
            "copper" => "copper",
            "core" => "core",
            "prepreg" => "prepreg",
            t if t.contains("solder mask") => "soldermask",
            t if t.contains("silk") => "silkscreen",
            t if t.contains("paste") => "paste",
            _ => "custom",
        };
        let thickness = self
            .num(row.field("thickness"))
            .filter(|t| (0.0..=100.0).contains(t))
            .unwrap_or(0.0);
        let range = |v: Option<f64>, lo: f64, hi: f64| v.filter(|x| (lo..=hi).contains(x));
        let material = row.field("material");
        Some(LayerRecord {
            name,
            kind: kind.into(),
            thickness,
            copper_role: (kind == "copper").then(|| "signal".to_string()),
            material: (!material.is_empty() && material.len() <= 64).then(|| material.to_owned()),
            dk: range(self.num(row.field("epsilon_r")), 1.0, 20.0),
            df: range(self.num(row.field("loss_tangent")), 0.0, 1.0),
            ..LayerRecord::default()
        })
    }

    /// Board size from the edge pieces, applied after the origin is known.
    fn shift_board(&mut self, board: &mut pcbcraft_doc::Board) {
        let mut max = Point::default();
        let mut grow = |p: Point| {
            max = Point::new(max.x.max(p.x), max.y.max(p.y));
        };
        if !self.outline_src.is_empty() {
            let points: Vec<Point> = self.outline_src.iter().map(|p| p.a).collect();
            let arcs: Vec<Option<Point>> = self.outline_src.iter().map(|p| p.mid).collect();
            if let Some((_, hi)) = pcbcraft_doc::outline_extent(&points, &arcs) {
                grow(Point::new(hi.x - self.origin.x, hi.y - self.origin.y));
            }
        }
        for piece in self.edge.iter().filter(|_| self.outline_src.is_empty()) {
            grow(Point::new(
                piece.a.x - self.origin.x,
                piece.a.y - self.origin.y,
            ));
            grow(Point::new(
                piece.b.x - self.origin.x,
                piece.b.y - self.origin.y,
            ));
            if let Some(m) = piece.mid {
                for q in arc_samples(piece.a, m, piece.b) {
                    grow(Point::new(q.x - self.origin.x, q.y - self.origin.y));
                }
            }
        }
        for (c, e) in self
            .edge_circles
            .iter()
            .filter(|_| self.outline_src.is_empty())
        {
            let r = c.distance(*e);
            grow(Point::new(c.x + r - self.origin.x, c.y + r - self.origin.y));
        }
        board.width = max.x.max(1.0);
        board.height = max.y.max(1.0);
    }

    /// Chain edge pieces into closed loops: the largest becomes the outline.
    /// Chain the edge pieces into loops and keep the largest as the outline,
    /// still in source coordinates.
    fn build_outline(&mut self) {
        let mut pieces: Vec<EdgePiece> = self
            .edge
            .iter()
            .filter(|p| p.a.distance(p.b) > 1e-9 || p.mid.is_some())
            .copied()
            .collect();
        let mut split: Vec<EdgePiece> = Vec::new();
        for piece in pieces.drain(..) {
            match piece.mid.and_then(|m| split_arc(piece.a, m, piece.b)) {
                Some((m1, mid, m2)) => {
                    split.push(EdgePiece {
                        a: piece.a,
                        b: mid,
                        mid: Some(m1),
                    });
                    split.push(EdgePiece {
                        a: mid,
                        b: piece.b,
                        mid: Some(m2),
                    });
                }
                None => split.push(piece),
            }
        }
        let mut pieces = split;
        let mut loops: Vec<Vec<EdgePiece>> = Vec::new();
        while let Some(first) = pieces.pop() {
            let mut chain = vec![first];
            while let Some(last) = chain.last().copied() {
                let head = chain[0].a;
                if chain.len() > 1 && last.b.distance(head) < JOIN_TOLERANCE {
                    break;
                }
                let found = pieces.iter().position(|p| {
                    p.a.distance(last.b) < JOIN_TOLERANCE || p.b.distance(last.b) < JOIN_TOLERANCE
                });
                let Some(i) = found else { break };
                let mut next = pieces.swap_remove(i);
                if next.a.distance(last.b) >= JOIN_TOLERANCE {
                    std::mem::swap(&mut next.a, &mut next.b);
                }
                chain.push(next);
            }
            let closed = chain.len() > 1
                && chain
                    .last()
                    .is_some_and(|l| l.b.distance(chain[0].a) < JOIN_TOLERANCE);
            if closed {
                loops.push(chain);
            } else {
                self.notice(
                    "open board edge pieces ignored",
                    "edge lines that do not close a loop",
                );
            }
        }
        let area_of = |l: &Vec<EdgePiece>| {
            let (mut lo, mut hi) = (
                Point::new(f64::MAX, f64::MAX),
                Point::new(f64::MIN, f64::MIN),
            );
            for p in l {
                for q in [p.a, p.b] {
                    lo = Point::new(lo.x.min(q.x), lo.y.min(q.y));
                    hi = Point::new(hi.x.max(q.x), hi.y.max(q.y));
                }
            }
            (hi.x - lo.x) * (hi.y - lo.y)
        };
        loops.sort_by(|a, b| area_of(b).total_cmp(&area_of(a)));
        let inner = loops.len().saturating_sub(1);
        if inner > 0 {
            self.notice(
                "inner board cutout shapes other than circles are not imported",
                &format!("{inner} closed edge loops inside the outline"),
            );
        }
        let Some(outer) = loops.first() else { return };
        let mut merged: Vec<EdgePiece> = Vec::new();
        for piece in outer {
            if let Some(last) = merged.last_mut()
                && last.mid.is_none()
                && piece.mid.is_none()
            {
                let cross = (last.b.x - last.a.x) * (piece.b.y - last.b.y)
                    - (last.b.y - last.a.y) * (piece.b.x - last.b.x);
                if cross.abs() < 1e-6 {
                    last.b = piece.b;
                    continue;
                }
            }
            merged.push(*piece);
        }
        if merged.len() > 2
            && merged[0].mid.is_none()
            && merged[merged.len() - 1].mid.is_none()
            && let Some(last) = merged.last().copied()
        {
            let first = merged[0];
            let cross = (last.b.x - last.a.x) * (first.b.y - last.b.y)
                - (last.b.y - last.a.y) * (first.b.x - last.b.x);
            if cross.abs() < 1e-6 {
                merged[0].a = last.a;
                merged.pop();
            }
        }
        if merged.len() > MAX_OUTLINE_VERTICES {
            self.notice(
                "board outline has more than 64 edge pieces; the board is a rectangle",
                &format!("{} pieces", merged.len()),
            );
            return;
        }
        self.outline_src = merged;
    }

    /// Shift the kept outline to the origin and give it to the board when the
    /// model accepts it.
    fn finish_outline(&mut self, board: &mut pcbcraft_doc::Board) {
        let merged = std::mem::take(&mut self.outline_src);
        if merged.is_empty() {
            return;
        }
        let points: Vec<Point> = merged.iter().map(|p| self.shift(p.a)).collect();
        let arcs: Vec<Option<Point>> = merged
            .iter()
            .map(|p| p.mid.map(|m| self.shift(m)))
            .collect();
        if let Some((lo, hi)) = pcbcraft_doc::outline_extent(&points, &arcs) {
            if hi.x > board.width {
                board.width = hi.x;
            }
            if hi.y > board.height {
                board.height = hi.y;
            }
            let _ = lo;
        }
        let distinct: BTreeSet<(u64, u64)> = points
            .iter()
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .collect();
        if points.len() < 3 || distinct.len() != points.len() {
            self.notice(
                "board outline is not a simple loop; the board is a rectangle",
                "repeated vertices",
            );
            return;
        }
        let is_rect = points.len() == 4 && arcs.iter().all(Option::is_none);
        let mut candidate = pcbcraft_doc::Board {
            outline: None,
            outline_arcs: Vec::new(),
            ..Project::default().board
        };
        candidate.outline = Some(points);
        if arcs.iter().any(Option::is_some) {
            candidate.outline_arcs = arcs;
        }
        if is_rect {
            candidate.outline = None;
            candidate.outline_arcs.clear();
        }
        // Keep the outline only if the model accepts it.
        let mut probe = Project::default();
        probe.board.width = board.width;
        probe.board.height = board.height;
        probe.board.layer_records = board.layer_records.clone();
        probe.board.outline = candidate.outline.clone();
        probe.board.outline_arcs = candidate.outline_arcs.clone();
        match probe.validate() {
            Ok(()) => {
                board.outline = candidate.outline;
                board.outline_arcs = candidate.outline_arcs;
            }
            Err(e) => self.notice(
                "board outline was not accepted by the model; the board is a rectangle",
                &e,
            ),
        }
    }

    fn import_circle_cutout(&mut self, node: &Node, board: &mut pcbcraft_doc::Board) {
        let (Some(c), Some(e)) = (self.xy(node, "center"), self.xy(node, "end")) else {
            return;
        };
        let radius = c.distance(e);
        let center = self.shift(c);
        let inside = radius > 0.0
            && center.x - radius > 0.0
            && center.y - radius > 0.0
            && center.x + radius < board.width
            && center.y + radius < board.height;
        if !inside {
            self.notice(
                "edge circles that are not strictly inside the board are ignored",
                "circle on or outside the outline",
            );
            return;
        }
        let id = self.alloc();
        board.cutouts.push(Cutout { id, center, radius });
    }

    /// Copper layer for a name in this file, if it is a copper layer.
    fn layer_of(&self, name: &str) -> Option<Layer> {
        let name = name.trim_matches('"');
        let index = self.copper.iter().position(|c| c == name)?;
        Layer::from_index(index as u8).ok()
    }

    /// Net id for an object; `None` when the object names no net.
    fn net_of(&mut self, node: &Node) -> Option<Id> {
        let name = self.net_name_of(node)?;
        self.net_id_for_name(&name)
    }

    fn net_id_for_name(&mut self, name: &str) -> Option<Id> {
        if name.is_empty() {
            return None;
        }
        if let Some(id) = self.net_by_name.get(name) {
            return Some(*id);
        }
        if self.nets.len() >= MAX_NETS {
            self.notice("net limit reached; extra nets are left unassigned", name);
            return None;
        }
        let mut stored = name.to_owned();
        if stored.len() > 128 {
            let mut cut = 128;
            while !stored.is_char_boundary(cut) {
                cut -= 1;
            }
            stored.truncate(cut);
            self.notice("net names longer than 128 bytes were shortened", name);
        }
        if let Some(id) = self.net_by_name.get(&stored) {
            return Some(*id);
        }
        let id = self.alloc();
        self.nets.push(Net {
            id,
            name: stored.clone(),
            target_length: None,
            length_tolerance: None,
        });
        self.net_by_name.insert(stored, id);
        Some(id)
    }

    /// Net for copper that must have one: an unnamed net becomes `unassigned`.
    fn net_or_unassigned(&mut self, node: &Node, what: &str) -> Id {
        if let Some(id) = self.net_of(node) {
            return id;
        }
        self.notice(
            "copper with no net was placed on a net named unassigned",
            what,
        );
        if let Some(id) = self.unassigned {
            return id;
        }
        let id = self.alloc();
        self.nets.push(Net {
            id,
            name: "unassigned".into(),
            target_length: None,
            length_tolerance: None,
        });
        self.net_by_name.insert("unassigned".into(), id);
        self.unassigned = Some(id);
        id
    }

    fn unique_reference(&mut self, wanted: &str) -> String {
        let mut base = wanted.trim().to_owned();
        if base.is_empty() {
            base = "REF".into();
        }
        if base.len() > 60 {
            let mut cut = 60;
            while !base.is_char_boundary(cut) {
                cut -= 1;
            }
            base.truncate(cut);
        }
        if self.refs.insert(base.clone()) {
            return base;
        }
        self.notice("duplicate reference designators were renumbered", &base);
        let mut n = 2;
        loop {
            let candidate = format!("{base}~{n}");
            if self.refs.insert(candidate.clone()) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Keep the source uuid on an object as preserved text.
    fn keep_uuid(&mut self, preserved: &mut PreservedSource, node: &Node) {
        if let Some(u) = node.child("uuid") {
            self.keep(preserved, u, "uuid");
        }
    }

    fn import_footprint(&mut self, node: &Node, board: &mut pcbcraft_doc::Board) {
        if self.components.len() >= MAX_COMPONENTS {
            self.notice("part limit reached; later parts were not imported", "");
            return;
        }
        let Some(at) = node.child("at") else {
            self.notice("footprints without a placement were skipped", node.value(1));
            return;
        };
        let (Some(x), Some(y)) = (self.num(at.value(1)), self.num(at.value(2))) else {
            self.notice("footprints without a placement were skipped", node.value(1));
            return;
        };
        let rotation = self.num(at.value(3)).unwrap_or(0.0);
        let position = self.shift(Point::new(x, y));
        let bottom = node.field("layer") == "B.Cu";
        let reference = property(node, "Reference")
            .or_else(|| fp_text(node, "reference"))
            .unwrap_or_else(|| "REF".into());
        let value = property(node, "Value")
            .or_else(|| fp_text(node, "value"))
            .unwrap_or_default();
        let slice = self.source[node.span.clone()].to_owned();
        let mut read = read_footprint(&slice);
        if read.is_err() {
            // Retry with the pads, model and header only: artwork the reader
            // cannot parse must not cost the part its pads.
            let mut reduced = format!("(footprint \"{}\"\n", node.value(1).replace('"', ""));
            for child in &node.children {
                if child.atom.is_none()
                    && matches!(
                        child.name(),
                        "pad" | "model" | "layer" | "descr" | "tags" | "attr"
                    )
                {
                    reduced.push_str(&self.source[child.span.clone()]);
                    reduced.push('\n');
                }
            }
            reduced.push(')');
            if let Ok(def) = read_footprint(&reduced) {
                self.notice(
                    "footprint artwork that could not be read was dropped, pads kept",
                    &reference,
                );
                read = Ok(def);
            }
        }
        let pad_nodes: Vec<&Node> = node.children.iter().filter(|c| c.name() == "pad").collect();
        if pad_nodes.is_empty() {
            self.notice(
                "parts with no pads (logos, outlines, artwork) were not imported",
                &reference,
            );
            return;
        }

        // Nets per pad node, in order.
        let pad_nets: Vec<Option<Id>> = pad_nodes.iter().map(|p| self.net_of(p)).collect();

        let mut library: Option<FootprintDefinition> = None;
        let mut placed: Vec<FootprintPad> = Vec::new();
        match read {
            Ok(mut definition) if definition.pads.len() == pad_nodes.len() => {
                if !definition.unsupported.is_empty() {
                    let odd: Vec<String> = definition
                        .unsupported
                        .iter()
                        .filter(|u| {
                            ![
                                "at",
                                "uuid",
                                "path",
                                "sheetname",
                                "sheetfile",
                                "component_classes",
                                "private_layers",
                                "net_tie_pad_groups",
                                "zone_connect",
                                "solder_mask_margin",
                                "thermal_width",
                                "thermal_gap",
                                "autoplace_cost90",
                                "autoplace_cost180",
                                "teardrops",
                                "locked",
                                "placed",
                                "tedit",
                                "tstamp",
                                "dimension",
                                "group",
                                "zone",
                                "fp_text_private",
                                "image",
                                "generated",
                            ]
                            .iter()
                            .any(|k| u.as_str() == format!("footprint expression {k}"))
                        })
                        .cloned()
                        .collect();
                    if let Some(first) = odd.first() {
                        self.notice(
                            "footprint elements the model does not draw were noted",
                            first,
                        );
                    }
                    definition.unsupported.clear();
                }
                if let Some(stripped) = definition.name.strip_prefix("pcbcraft_") {
                    definition.name = stripped.to_owned();
                }
                match self.repair_definition(&mut definition, rotation, bottom) {
                    Ok(placed_pads) => {
                        library = Some(definition);
                        placed = placed_pads;
                    }
                    Err(reason) => {
                        self.notice(
                            "parts kept as simple pads because the footprint shape is not supported",
                            &format!("{reference}: {reason}"),
                        );
                    }
                }
            }
            Ok(_) => {
                self.notice(
                    "parts kept as simple pads because the footprint shape is not supported",
                    &format!("{reference}: pad count differs from the definition"),
                );
            }
            Err(e) => {
                self.notice(
                    "parts kept as simple pads because the footprint could not be read",
                    &format!("{reference}: {}", library_error(&self.source, e)),
                );
            }
        }

        let fp_id;
        let component_id;
        let pads: Vec<Pad>;
        let mut preserved = PreservedSource::default();
        let kind = kind_for(&reference);
        if let Some(def) = &library {
            // Placed pads follow the definition order one to one.
            let mut built = Vec::new();
            for (i, geometry) in placed.iter().enumerate() {
                let net = if geometry.pad_type == "unplated" {
                    None
                } else {
                    pad_nets.get(i).copied().flatten()
                };
                built.push(Pad {
                    number: geometry.number.clone(),
                    offset: geometry.position,
                    diameter: geometry.size.x.min(geometry.size.y),
                    net,
                    castellated: None,
                    geometry: Some(geometry.clone()),
                });
            }
            pads = built;
            let pins = pins_from(&pads);
            if pins.is_empty() || pins.len() > 256 {
                self.notice(
                    "parts without numbered copper pads were not imported",
                    &reference,
                );
                return;
            }
            let reference = self.unique_reference(&reference);
            component_id = self.alloc();
            fp_id = self.alloc();
            self.components.push(Component {
                id: component_id,
                reference,
                value: truncate(&value, 256),
                kind,
                position: Point::default(),
                rotation: 0.0,
                pins,
                units: Vec::new(),
                library_symbol: None,
                library_footprint: Some(LibraryFootprint {
                    library: "project".into(),
                    definition: def.clone(),
                }),
                fields: Vec::new(),
                sheet: 0,
            });
        } else {
            let mut built: Vec<Pad> = Vec::new();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for (i, pad_node) in pad_nodes.iter().enumerate() {
                if matches!(pad_node.value(2), "np_thru_hole") {
                    continue;
                }
                let number = pad_node.value(1).to_owned();
                if number.is_empty() || number.len() > 16 || !seen.insert(number.clone()) {
                    continue;
                }
                let size = self.xy(pad_node, "size").unwrap_or(Point::new(0.5, 0.5));
                let offset = self.xy(pad_node, "at").unwrap_or_default();
                let diameter = size.x.min(size.y);
                built.push(Pad {
                    number,
                    offset,
                    diameter: if diameter > 0.0 { diameter } else { 0.1 },
                    net: pad_nets.get(i).copied().flatten(),
                    castellated: None,
                    geometry: None,
                });
            }
            if built.is_empty() {
                self.notice(
                    "parts without numbered copper pads were not imported",
                    &reference,
                );
                return;
            }
            let pins = pins_from(&built);
            pads = built;
            let reference = self.unique_reference(&reference);
            component_id = self.alloc();
            fp_id = self.alloc();
            self.components.push(Component {
                id: component_id,
                reference,
                value: truncate(&value, 256),
                kind,
                position: Point::default(),
                rotation: 0.0,
                pins,
                units: Vec::new(),
                library_symbol: None,
                library_footprint: None,
                fields: Vec::new(),
                sheet: 0,
            });
        }
        self.keep_uuid(&mut preserved, node);
        for child in &node.children {
            if child.atom.is_none()
                && !matches!(
                    child.name(),
                    "pad"
                        | "property"
                        | "at"
                        | "uuid"
                        | "layer"
                        | "fp_text"
                        | "attr"
                        | "model"
                        | "clearance"
                        | "solder_mask_margin"
                        | "solder_paste_margin"
                        | "descr"
                        | "tags"
                        | "embedded_fonts"
                )
                && !child.name().starts_with("fp_")
                && !is_graphic(child.name())
            {
                self.keep(&mut preserved, child, "footprint node");
            }
        }
        board.footprints.push(Footprint {
            id: fp_id,
            component: component_id,
            position,
            rotation,
            locked: None,
            side: bottom.then_some(Layer::Back),
            pads,
            preserved,
        });
    }

    /// Bring a read footprint definition inside what placement supports and
    /// return the pads as they sit on the board. Errors name why a footprint
    /// has to fall back to simple pads.
    fn repair_definition(
        &mut self,
        definition: &mut FootprintDefinition,
        rotation: f64,
        bottom: bool,
    ) -> Result<Vec<FootprintPad>, String> {
        // The source text of a footprint repeats everything parsed from it and
        // would roughly double the saved project; the parsed form is kept.
        definition.raw_source.clear();
        for pad in &mut definition.pads {
            pad.raw_source.clear();
        }
        let mut renamed = 0usize;
        let mut fixed = 0usize;
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut counter = 0usize;
        for pad in &mut definition.pads {
            // Pad angles in a board file include the part rotation.
            pad.rotation -= rotation;
            if bottom {
                pad.layers = pad
                    .layers
                    .iter()
                    .map(|l| pcbcraft_doc::swap_board_side_layer(l))
                    .collect();
            }
            if pad.pad_type == "unplated" {
                pad.number.clear();
                pad.layers.retain(|l| !l.ends_with(".Cu"));
                if pad.drill.is_none() {
                    pad.drill = Some(pcbcraft_library::Drill {
                        size: Point::new(
                            pad.size.x.min(pad.size.y).max(0.1),
                            pad.size.x.min(pad.size.y).max(0.1),
                        ),
                        offset: Point::default(),
                    });
                    fixed += 1;
                }
                if pad.size.x < 0.0 || pad.size.y < 0.0 || !pad_hole_fits(pad) {
                    pad.size = Point::default();
                }
                continue;
            }
            if pad.pad_type == "connect" {
                pad.pad_type = "smd".into();
                fixed += 1;
            }
            if pad.pad_type == "thru_hole" && pad.drill.is_none() {
                pad.pad_type = "smd".into();
                fixed += 1;
            }
            if pad.pad_type == "smd" && pad.drill.is_some() {
                pad.drill = None;
                fixed += 1;
            }
            if pad.pad_type == "thru_hole" {
                if !pad.layers.iter().any(|l| l == "*.Cu") {
                    pad.layers.retain(|l| !l.ends_with(".Cu"));
                    pad.layers.insert(0, "*.Cu".into());
                    fixed += 1;
                }
            } else if !(pad.layers.iter().any(|l| l == "F.Cu")
                && !pad.layers.iter().any(|l| l == "B.Cu" || l == "*.Cu"))
            {
                pad.layers.retain(|l| !l.ends_with(".Cu"));
                pad.layers.insert(0, "F.Cu".into());
                fixed += 1;
            }
            if pad.shape == "circle" && (pad.size.x - pad.size.y).abs() > 1e-6 {
                pad.shape = "oval".into();
                fixed += 1;
            }
            if matches!(pad.shape_kind(), pcbcraft_library::PadShape::Unsupported) {
                pad.shape = "rect".into();
                pad.polygon.clear();
                pad.chamfer = None;
                pad.roundrect_ratio = None;
                fixed += 1;
            }
            if pad.number.is_empty() || pad.number.len() > 16 {
                counter += 1;
                pad.number = format!("~{counter}");
                renamed += 1;
            }
            if !seen.insert(pad.number.clone()) {
                let base = pad.number.clone();
                let mut n = 2;
                loop {
                    let candidate = format!("{base}~{n}");
                    if candidate.len() <= 16 && seen.insert(candidate.clone()) {
                        pad.number = candidate;
                        break;
                    }
                    n += 1;
                    if n > 999 {
                        return Err("too many pads share one number".into());
                    }
                }
                renamed += 1;
            }
            if pad.pad_type == "thru_hole" {
                // The hole has to sit strictly inside the copper.
                for _ in 0..3 {
                    if pad_hole_fits(pad) {
                        break;
                    }
                    if let Some(d) = &pad.drill {
                        let need = d.size.x.max(d.size.y)
                            + 0.3
                            + d.offset.x.abs().max(d.offset.y.abs()) * 2.0;
                        pad.size = Point::new(pad.size.x.max(need), pad.size.y.max(need));
                        if pad.shape == "circle" {
                            pad.size =
                                Point::new(pad.size.x.max(pad.size.y), pad.size.x.max(pad.size.y));
                        }
                        pad.polygon.clear();
                        if pad.shape == "custom" {
                            pad.shape = "rect".into();
                        }
                        fixed += 1;
                    }
                }
            }
        }
        if renamed > 0 {
            self.notice(
                "pads with repeated or missing numbers were renamed",
                &format!("{renamed} pads on {}", definition.name),
            );
        }
        if fixed > 0 {
            self.notice(
                "pad layers, shapes or holes were adjusted to what placement supports",
                &format!("{fixed} changes on {}", definition.name),
            );
        }
        let before = definition.graphics.len();
        definition.graphics.retain(|g| {
            let layer = graphic_layer(g);
            matches!(
                layer,
                "F.Paste"
                    | "B.Paste"
                    | "F.Mask"
                    | "B.Mask"
                    | "F.SilkS"
                    | "B.SilkS"
                    | "F.Fab"
                    | "B.Fab"
                    | "F.CrtYd"
                    | "B.CrtYd"
            )
        });
        let dropped = before - definition.graphics.len();
        if dropped > 0 {
            self.notice(
                "footprint graphics on copper, the board edge or user layers were dropped",
                &format!("{dropped} on {}", definition.name),
            );
        }
        // Pad geometry on the board is the node pad; the definition is the
        // top-authored mirror image for a bottom part.
        let placed: Vec<FootprintPad> = definition.pads.clone();
        if bottom {
            definition.pads = definition
                .pads
                .iter()
                .map(pcbcraft_doc::mirrored_pad_for_bottom)
                .collect();
        }
        if definition.pads.len() > 256 {
            return Err("more than 256 pads".into());
        }
        pcbcraft_doc::validate_footprint_support(definition)?;
        Ok(placed)
    }

    fn import_segment(&mut self, node: &Node, board: &mut pcbcraft_doc::Board, arc: bool) {
        let (Some(start), Some(end)) = (self.xy(node, "start"), self.xy(node, "end")) else {
            self.notice("tracks without usable coordinates were skipped", "");
            return;
        };
        let width = self.num(node.field("width")).unwrap_or(0.0);
        let Some(layer) = self.layer_of(node.field("layer")) else {
            self.notice(
                "tracks on a layer that is not copper were skipped",
                node.field("layer"),
            );
            return;
        };
        let start = self.shift(start);
        let end = self.shift(end);
        let mut mid = if arc {
            self.xy(node, "mid").map(|m| self.shift(m))
        } else {
            None
        };
        if let Some(m) = mid
            && pcbcraft_geom::arc_through(start, m, end).is_none()
        {
            mid = None;
            self.notice("arcs with collinear points became straight tracks", "");
        }
        if width <= 0.0 || start.distance(end) < 1e-6 {
            self.notice("zero-length or zero-width tracks were skipped", "");
            return;
        }
        let net = self.net_or_unassigned(node, "a track");
        let mut preserved = PreservedSource::default();
        self.keep_uuid(&mut preserved, node);
        for child in &node.children {
            if child.atom.is_none()
                && !matches!(
                    child.name(),
                    "start" | "end" | "mid" | "width" | "layer" | "net" | "uuid"
                )
            {
                self.keep(&mut preserved, child, "track node");
            }
        }
        let id = self.alloc();
        board.tracks.push(Track {
            id,
            net,
            start,
            end,
            width,
            layer,
            arc: mid,
            preserved,
        });
    }

    fn import_via(&mut self, node: &Node, board: &mut pcbcraft_doc::Board) {
        let names: Vec<&str> = node
            .child("layers")
            .map(|l| {
                l.children
                    .iter()
                    .skip(1)
                    .filter_map(|c| c.atom.as_deref())
                    .collect()
            })
            .unwrap_or_default();
        let last = self.copper.len().saturating_sub(1) as u8;
        let mut ends: Vec<u8> = names
            .iter()
            .filter_map(|n| self.layer_of(n))
            .map(|l| l.index())
            .collect();
        if ends.len() < 2 {
            ends = vec![0, last];
        }
        ends.sort_unstable();
        let (a, b) = (ends[0], ends[ends.len() - 1]);
        if a >= b {
            self.notice("vias with an empty copper span were skipped", "");
            return;
        }
        let Some(at) = node.child("at").and_then(|p| self.point_of(p)) else {
            self.notice("vias without a position were skipped", "");
            return;
        };
        let position = self.shift(at);
        let diameter = self.num(node.field("size")).unwrap_or(0.0);
        let mut drill = self.num(node.field("drill")).unwrap_or(0.0).max(0.0);
        if diameter <= 0.0 {
            self.notice("vias without a pad size were skipped", "");
            return;
        }
        if drill >= diameter {
            drill = diameter * 0.5;
            self.notice("via holes as large as the pad were made smaller", "");
        }
        let net = self.net_or_unassigned(node, "a via");
        let from_layer = if a == 0 {
            None
        } else {
            Layer::from_index(a).ok()
        };
        let to_layer = if b == last {
            None
        } else {
            Layer::from_index(b).ok()
        };
        if from_layer.is_some() || to_layer.is_some() {
            self.spans.insert((a, b));
        }
        let mut preserved = PreservedSource::default();
        self.keep_uuid(&mut preserved, node);
        for child in &node.children {
            if child.atom.is_none()
                && !matches!(
                    child.name(),
                    "at" | "size" | "drill" | "layers" | "net" | "uuid" | "free" | "locked"
                )
            {
                self.keep(&mut preserved, child, "via node");
            }
        }
        let id = self.alloc();
        board.vias.push(Via {
            id,
            net,
            position,
            diameter,
            drill,
            preserved,
            from_layer,
            to_layer,
            covering: None,
        });
    }

    fn import_zone(&mut self, node: &Node, board: &mut pcbcraft_doc::Board) {
        // Layers: one `layer`, or a `layers` list; `*.Cu` and `F&B.Cu` mean all.
        let mut layers: Vec<Layer> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        if let Some(l) = node.child("layers") {
            names.extend(l.children.iter().skip(1).filter_map(|c| c.atom.clone()));
        }
        if let Some(l) = node.child("layer") {
            names.push(l.value(1).to_owned());
        }
        for name in &names {
            if name == "*.Cu" || name == "F&B.Cu" {
                layers = (0..self.copper.len())
                    .filter_map(|i| Layer::from_index(i as u8).ok())
                    .collect();
                break;
            }
            if let Some(l) = self.layer_of(name)
                && !layers.contains(&l)
            {
                layers.push(l);
            }
        }
        if layers.is_empty() {
            self.notice(
                "zones and keepouts on no copper layer were skipped",
                &names.join(" "),
            );
            return;
        }
        let polygon = node.child("polygon");
        let Some(pts) = polygon.and_then(|p| p.child("pts")) else {
            self.notice("zones without an outline were skipped", "");
            return;
        };
        let raw: Vec<Point> = self.pts(pts).into_iter().map(|p| self.shift(p)).collect();
        let raw = if node.child("keepout").is_none() {
            let clipped = clip_to_rect(&raw, board.width, board.height);
            if clipped.len() != raw.len() || clipped != raw {
                self.notice(
                    "zones larger than the board were trimmed to the board rectangle",
                    "pour outlines reach past the board edge in the source",
                );
            }
            clipped
        } else {
            raw
        };
        let Some(outline) = self.closed_outline(raw) else {
            self.notice("zones with no area were skipped", "");
            return;
        };
        if let Some(keepout) = node.child("keepout") {
            self.import_keepout(node, keepout, &outline, &layers, board);
            return;
        }
        let net = self.net_or_unassigned(node, "a zone");
        let fill = node.child("fill");
        let filled = fill.is_some_and(|f| f.value(1) == "yes")
            || node.child("filled_polygon").is_some()
            || node.field("filled") == "yes";
        let mut template = PreservedSource::default();
        self.keep_uuid(&mut template, node);
        for child in &node.children {
            if child.atom.is_none()
                && !matches!(
                    child.name(),
                    "net"
                        | "net_name"
                        | "layer"
                        | "layers"
                        | "uuid"
                        | "polygon"
                        | "filled_polygon"
                        | "fill"
                        | "filled"
                        | "hatch"
                )
            {
                self.keep(&mut template, child, "zone node");
            }
        }
        for (k, layer) in layers.iter().enumerate() {
            let id = self.alloc();
            let mut preserved = template.clone();
            if k > 0 {
                preserved.nodes.retain(|n| !n.starts_with("(uuid"));
            }
            board.zones.push(CopperZone {
                id,
                net,
                layer: *layer,
                outline: outline.clone(),
                cutouts: Vec::new(),
                filled,
                kind: None,
                preserved,
                pour: None,
            });
        }
        if layers.len() > 1 {
            self.notice(
                "zones on several layers became one zone per layer",
                &format!("{} layers", layers.len()),
            );
        }
    }

    fn import_keepout(
        &mut self,
        node: &Node,
        keepout: &Node,
        outline: &[Point],
        layers: &[Layer],
        board: &mut pcbcraft_doc::Board,
    ) {
        let denied = |field: &str| keepout.field(field) != "allowed";
        let rules = Keepout {
            tracks: denied("tracks"),
            vias: denied("vias"),
            pads: denied("pads"),
            zones: denied("copperpour"),
        };
        if !(rules.tracks || rules.vias || rules.pads || rules.zones) {
            self.notice(
                "keepouts that only restrict footprints were skipped",
                "all copper kinds allowed",
            );
            return;
        }
        let (mut lo, mut hi) = (
            Point::new(f64::MAX, f64::MAX),
            Point::new(f64::MIN, f64::MIN),
        );
        for p in outline {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        let (w, h) = (hi.x - lo.x, hi.y - lo.y);
        let rect_area = w * h;
        let poly_area = polygon_area(outline);
        if (rect_area - poly_area).abs() > rect_area * 0.01 {
            self.notice(
                "keepout shapes other than rectangles were replaced by their bounding rectangle",
                &format!("{:.1} mm by {:.1} mm", w, h),
            );
        }
        let layer = if layers.len() == 1 {
            Some(layers[0])
        } else if layers.len() == self.copper.len() {
            None
        } else {
            self.notice(
                "keepouts on some but not all layers apply to every layer",
                &format!("{} of {} layers", layers.len(), self.copper.len()),
            );
            None
        };
        let name = node.field("name");
        let id = self.alloc();
        let region = RuleRegion {
            x: lo.x,
            y: lo.y,
            width: w.max(0.01),
            height: h.max(0.01),
        };
        let area = RuleArea {
            id,
            name: (!name.is_empty() && name.len() <= 64).then(|| name.to_owned()),
            net: None,
            layer,
            clearance: KEEPOUT_CLEARANCE,
            keepout: Some(rules),
            max_height: None,
            region,
            profile: None,
        };
        board.rule_areas.push(area);
    }

    /// Close a point list and cap it at the model's vertex limit.
    fn closed_outline(&mut self, mut pts: Vec<Point>) -> Option<Vec<Point>> {
        pts.dedup();
        if pts.first() == pts.last() {
            pts.pop();
        }
        if pts.len() < 3 {
            return None;
        }
        if pts.len() > MAX_ZONE_VERTICES {
            let n = pts.len();
            let step = n as f64 / MAX_ZONE_VERTICES as f64;
            pts = (0..MAX_ZONE_VERTICES)
                .map(|i| pts[((i as f64 * step) as usize).min(n - 1)])
                .collect();
            pts.dedup();
            self.notice(
                "zone outlines with more than 255 vertices were simplified",
                &format!("{n} vertices"),
            );
        }
        if pts.len() < 3 || polygon_area(&pts) < 1e-6 {
            return None;
        }
        pts.push(pts[0]);
        Some(pts)
    }

    fn finish(mut self, board: pcbcraft_doc::Board) -> Result<(Project, ImportReport), String> {
        // Nets that exist only in the table.
        let table: Vec<String> = self
            .net_table
            .iter()
            .filter(|(o, _)| **o != 0)
            .map(|(_, n)| n.clone())
            .collect();
        for name in table {
            self.net_id_for_name(&name);
        }
        let mut project = Project {
            name: "Imported board".into(),
            unit: "mm".into(),
            components: std::mem::take(&mut self.components),
            nets: std::mem::take(&mut self.nets),
            auto_sync: false,
            board_only: true,
            ..Project::default()
        };
        // Via spans beyond the generic ones become allowed extras.
        project.board = board;
        if let Some((clearance, fab)) = self.rules.take() {
            project.board.clearance = clearance;
            project.board.rules = Some(fab);
            self.notice(
                "design rules were read from the project file",
                &format!("clearance {clearance} mm"),
            );
        }
        let allowed: BTreeSet<(u8, u8)> = project
            .allowed_via_spans()
            .iter()
            .map(|(a, b)| (a.index(), b.index()))
            .collect();
        for span in &self.spans {
            if !allowed.contains(span) {
                project.board.via_spans.push(*span);
            }
        }
        project.board.via_spans.sort_unstable();
        project.board.via_spans.dedup();
        if let Err(e) = project.validate() {
            return Err(format!("the imported board is not valid: {e}"));
        }
        let imported = ImportCounts {
            parts: project.board.footprints.len(),
            pads: project.board.footprints.iter().map(|f| f.pads.len()).sum(),
            nets: project.nets.len(),
            tracks: project.board.tracks.len(),
            vias: project.board.vias.len(),
            zones: project.board.zones.len(),
            keepouts: project.board.rule_areas.len(),
            layers: usize::from(project.copper_layer_count()),
            models: project
                .components
                .iter()
                .filter(|c| {
                    c.library_footprint
                        .as_ref()
                        .is_some_and(|l| l.definition.model.is_some())
                })
                .count(),
        };
        if imported.models > 0 {
            self.notice(
                "3D model references are kept as references; the path variables are not resolved",
                &format!("{} parts name a model", imported.models),
            );
        }
        if self.origin != Point::default() {
            self.notice(
                "the board was moved so its outline starts at the origin",
                &format!("by {:.2} mm, {:.2} mm", -self.origin.x, -self.origin.y),
            );
        }
        let notices = self
            .notices
            .iter()
            .map(|(kind, (count, detail))| {
                if detail.is_empty() {
                    format!("{kind} ({count})")
                } else {
                    format!("{kind} ({count}); first: {detail}")
                }
            })
            .collect();
        let _ = self.seen_nets.len();
        let _ = self.has_stackup;
        Ok((
            project,
            ImportReport {
                source: self.src,
                imported,
                notices,
            },
        ))
    }
}

/// Cut an arc in two at its midpoint; returns the two new midpoints and the
/// cut point. `None` when the three points are collinear.
fn split_arc(a: Point, m: Point, b: Point) -> Option<(Point, Point, Point)> {
    let d = 2.0 * (a.x * (m.y - b.y) + m.x * (b.y - a.y) + b.x * (a.y - m.y));
    if d.abs() < 1e-9 {
        return None;
    }
    let (a2, m2, b2) = (
        a.x * a.x + a.y * a.y,
        m.x * m.x + m.y * m.y,
        b.x * b.x + b.y * b.y,
    );
    let cx = (a2 * (m.y - b.y) + m2 * (b.y - a.y) + b2 * (a.y - m.y)) / d;
    let cy = (a2 * (b.x - m.x) + m2 * (a.x - b.x) + b2 * (m.x - a.x)) / d;
    let r = (a.x - cx).hypot(a.y - cy);
    let ang = |p: Point| (p.y - cy).atan2(p.x - cx);
    let tau = std::f64::consts::TAU;
    let ccw = (m.x - a.x) * (b.y - m.y) - (m.y - a.y) * (b.x - m.x) > 0.0;
    let s = if ccw { 1.0 } else { -1.0 };
    let d1 = ((ang(m) - ang(a)) * s).rem_euclid(tau) * s;
    let d2 = ((ang(b) - ang(m)) * s).rem_euclid(tau) * s;
    if d1.abs() + d2.abs() < 3.0 {
        return None;
    }
    let at = |t: f64| Point::new(cx + r * t.cos(), cy + r * t.sin());
    Some((at(ang(a) + d1 / 2.0), m, at(ang(m) + d2 / 2.0)))
}

/// Points along an arc, for bounding boxes.
fn arc_samples(a: Point, m: Point, b: Point) -> Vec<Point> {
    let d = 2.0 * (a.x * (m.y - b.y) + m.x * (b.y - a.y) + b.x * (a.y - m.y));
    if d.abs() < 1e-9 {
        return vec![a, m, b];
    }
    let (a2, m2, b2) = (
        a.x * a.x + a.y * a.y,
        m.x * m.x + m.y * m.y,
        b.x * b.x + b.y * b.y,
    );
    let cx = (a2 * (m.y - b.y) + m2 * (b.y - a.y) + b2 * (a.y - m.y)) / d;
    let cy = (a2 * (b.x - m.x) + m2 * (a.x - b.x) + b2 * (m.x - a.x)) / d;
    let r = (a.x - cx).hypot(a.y - cy);
    let ang = |p: Point| (p.y - cy).atan2(p.x - cx);
    let tau = std::f64::consts::TAU;
    let ccw = (m.x - a.x) * (b.y - m.y) - (m.y - a.y) * (b.x - m.x) > 0.0;
    let s = if ccw { 1.0 } else { -1.0 };
    let d1 = ((ang(m) - ang(a)) * s).rem_euclid(tau) * s;
    let d2 = ((ang(b) - ang(m)) * s).rem_euclid(tau) * s;
    (0..=32)
        .map(|i| {
            let t = ang(a) + (d1 + d2) * f64::from(i) / 32.0;
            Point::new(cx + r * t.cos(), cy + r * t.sin())
        })
        .collect()
}

/// Clip a polygon to the rectangle from the origin to (w, h), edge by edge.
fn clip_to_rect(poly: &[Point], w: f64, h: f64) -> Vec<Point> {
    type Inside = fn(Point, f64, f64) -> bool;
    let edges: [(Inside, u8); 4] = [
        (|p, _, _| p.x >= 0.0, 0),
        (|p, w, _| p.x <= w, 1),
        (|p, _, _| p.y >= 0.0, 2),
        (|p, _, h| p.y <= h, 3),
    ];
    let cross = |a: Point, b: Point, side: u8| -> Point {
        let (x, y) = match side {
            0 => (0.0, a.y + (b.y - a.y) * (0.0 - a.x) / (b.x - a.x)),
            1 => (w, a.y + (b.y - a.y) * (w - a.x) / (b.x - a.x)),
            2 => (a.x + (b.x - a.x) * (0.0 - a.y) / (b.y - a.y), 0.0),
            _ => (a.x + (b.x - a.x) * (h - a.y) / (b.y - a.y), h),
        };
        Point::new(x, y)
    };
    let mut out: Vec<Point> = poly.to_vec();
    for (inside, side) in edges {
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (cur, prev) = (input[i], input[(i + input.len() - 1) % input.len()]);
            match (inside(cur, w, h), inside(prev, w, h)) {
                (true, true) => out.push(cur),
                (true, false) => {
                    out.push(cross(prev, cur, side));
                    out.push(cur);
                }
                (false, true) => out.push(cross(prev, cur, side)),
                (false, false) => {}
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

fn grow_min(min: &mut Option<Point>, p: Point) {
    *min = Some(match *min {
        Some(m) => Point::new(m.x.min(p.x), m.y.min(p.y)),
        None => p,
    });
}

fn polygon_area(pts: &[Point]) -> f64 {
    let mut area = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        area += a.x * b.y - b.x * a.y;
    }
    (area / 2.0).abs()
}

fn pad_hole_fits(pad: &FootprintPad) -> bool {
    let mut probe = pad.clone();
    probe.pad_type = "thru_hole".into();
    probe.layers = vec!["*.Cu".into()];
    if probe.size.x <= 0.0 || probe.size.y <= 0.0 {
        return false;
    }
    pcbcraft_doc::validate_footprint_support(&FootprintDefinition {
        name: "probe".into(),
        description: String::new(),
        keywords: String::new(),
        pads: vec![{
            probe.number = "1".into();
            probe
        }],
        graphics: Vec::new(),
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: Vec::new(),
    })
    .is_ok()
}

fn graphic_layer(g: &pcbcraft_library::Graphic) -> &str {
    use pcbcraft_library::Graphic;
    match g {
        Graphic::Line { layer, .. }
        | Graphic::Rectangle { layer, .. }
        | Graphic::Circle { layer, .. }
        | Graphic::Arc { layer, .. }
        | Graphic::Polyline { layer, .. }
        | Graphic::Text { layer, .. } => layer,
    }
}

fn pins_from(pads: &[Pad]) -> Vec<Pin> {
    let mut pins: Vec<Pin> = Vec::new();
    for pad in pads {
        if pad.number.is_empty() || pins.iter().any(|p| p.number == pad.number) {
            continue;
        }
        pins.push(Pin {
            number: pad.number.clone(),
            name: pad.number.clone(),
            offset: Point::default(),
            net: pad.net,
            electrical_type: "passive".into(),
            direction: None,
            unit: 1,
        });
    }
    pins
}

fn kind_for(reference: &str) -> String {
    let prefix: String = reference
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    match prefix.as_str() {
        "R" => "resistor",
        "C" => "capacitor",
        "D" | "LED" => "led",
        _ => "connector",
    }
    .into()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_owned()
}

fn is_graphic(name: &str) -> bool {
    matches!(
        name,
        "fp_line"
            | "fp_rect"
            | "fp_circle"
            | "fp_arc"
            | "fp_poly"
            | "fp_text"
            | "gr_line"
            | "gr_rect"
            | "gr_circle"
            | "gr_arc"
            | "gr_poly"
    )
}

fn property(node: &Node, key: &str) -> Option<String> {
    node.children
        .iter()
        .find(|c| c.name() == "property" && c.value(1) == key)
        .map(|c| c.value(2).to_owned())
}

fn fp_text(node: &Node, kind: &str) -> Option<String> {
    node.children
        .iter()
        .find(|c| c.name() == "fp_text" && c.value(1) == kind)
        .map(|c| c.value(2).to_owned())
}

fn format_parse_error(text: &str, err: pcbcraft_library::Error) -> String {
    library_error(text, err)
}

fn library_error(text: &str, err: LibraryError) -> String {
    match err {
        LibraryError::Syntax { offset, message } => line_error(text, offset, &message),
        LibraryError::Limit(label) => format!("interchange file exceeds {label}"),
        LibraryError::Invalid(msg) => msg,
    }
}

fn line_error(text: &str, offset: usize, message: &str) -> String {
    let line = line_number_at_offset(text, offset);
    format!("line {line}: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_board_reports_line_number() -> Result<(), String> {
        let text = "(kicad_pcb\n (segment (start 0 0) (end 1))\n)\n";
        let err = read_board_interchange(text);
        // A track without an end point is skipped with a notice, not refused.
        let (_, report) = read_board_interchange_report(text)?;
        assert!(err.is_ok());
        assert!(!report.notices.is_empty());
        Ok(())
    }

    #[test]
    fn unbalanced_text_reports_line_number() -> Result<(), String> {
        let err = read_board_interchange("(kicad_pcb\n (segment\n")
            .err()
            .ok_or("expected a syntax error")?;
        assert!(err.contains("line"), "expected a line number in {err:?}");
        Ok(())
    }
}
