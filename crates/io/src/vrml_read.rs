//! A bounded VRML 2.0 reader for 3D model files: Group, Transform, Shape,
//! Appearance and Material diffuseColor, IndexedFaceSet with Coordinate, and
//! DEF/USE. `Inline` is refused with a plain reason; every other node is
//! skipped. Used only by `model_load.rs`, which owns limits and units.
//! Units: one VRML unit is 0.1 inch (2.54 mm); a Transform scale applies on top.

use crate::model_load::{LoadedModel, MAX_MODEL_FACES, MAX_MODEL_VERTICES, ModelError};
use std::collections::HashMap;

/// Millimetres per VRML unit (0.1 inch).
pub const VRML_UNIT_MM: f64 = 2.54;
const MAX_NODES: usize = 200_000;
const MAX_DEPTH: usize = 48;

type M = [[f64; 4]; 4];
fn err<T>(s: impl Into<String>) -> Result<T, ModelError> {
    Err(ModelError(s.into()))
}

#[derive(Clone, Debug)]
enum Tok {
    Open,
    Close,
    LOpen,
    LClose,
    Word(String),
}
fn tokenize(text: &str) -> Vec<Tok> {
    let mut out = vec![];
    let mut word = String::new();
    let flush = |w: &mut String, out: &mut Vec<Tok>| {
        if !w.is_empty() {
            out.push(Tok::Word(std::mem::take(w)));
        }
    };
    for line in text.lines() {
        let mut in_str = false;
        for ch in line.chars() {
            if in_str {
                if ch == '"' {
                    in_str = false;
                    out.push(Tok::Word("\"\"".into()));
                }
                continue;
            }
            match ch {
                '#' => break,
                '"' => {
                    flush(&mut word, &mut out);
                    in_str = true;
                }
                '{' => {
                    flush(&mut word, &mut out);
                    out.push(Tok::Open);
                }
                '}' => {
                    flush(&mut word, &mut out);
                    out.push(Tok::Close);
                }
                '[' => {
                    flush(&mut word, &mut out);
                    out.push(Tok::LOpen);
                }
                ']' => {
                    flush(&mut word, &mut out);
                    out.push(Tok::LClose);
                }
                c if c.is_whitespace() || c == ',' => flush(&mut word, &mut out),
                c => word.push(c),
            }
        }
        flush(&mut word, &mut out);
    }
    out
}

#[derive(Clone, Debug, Default)]
struct Node {
    kind: String,
    nums: HashMap<String, Vec<f64>>,
    kids: Vec<Node>,
    /// Named child nodes by field (`appearance`, `geometry`, `coord`, `material`).
    fields: HashMap<String, Node>,
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    defs: HashMap<String, Node>,
    count: usize,
}
impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn word(&self) -> Option<&str> {
        match self.peek() {
            Some(Tok::Word(w)) => Some(w),
            _ => None,
        }
    }
    /// Parse one node starting at its type word (after any DEF).
    fn node(&mut self, depth: usize) -> Result<Node, ModelError> {
        if depth > MAX_DEPTH {
            return err("The VRML file nests nodes too deeply.");
        }
        self.count += 1;
        if self.count > MAX_NODES {
            return err("The VRML file has too many nodes.");
        }
        let Some(Tok::Word(kind)) = self.peek().cloned() else {
            return err("The VRML file is damaged: a node name was expected.");
        };
        self.pos += 1;
        if kind == "USE" {
            let Some(name) = self.word().map(str::to_string) else {
                return err("The VRML file is damaged: USE without a name.");
            };
            self.pos += 1;
            return self.defs.get(&name).cloned().map_or_else(
                || err(format!("The VRML file uses {name} before defining it.")),
                Ok,
            );
        }
        if kind == "Inline" {
            return err("The VRML file pulls in other files with Inline, which is not supported.");
        }
        if !matches!(self.peek(), Some(Tok::Open)) {
            return err("The VRML file is damaged: a node body was expected.");
        }
        self.pos += 1;
        let mut node = Node {
            kind,
            ..Default::default()
        };
        loop {
            match self.peek().cloned() {
                None => return err("The VRML file ends inside a node."),
                Some(Tok::Close) => {
                    self.pos += 1;
                    return Ok(node);
                }
                Some(Tok::Word(field)) => {
                    self.pos += 1;
                    self.field(&mut node, &field, depth)?;
                }
                Some(_) => return err("The VRML file is damaged: unexpected bracket."),
            }
        }
    }
    fn is_node_start(&self) -> bool {
        match self.word() {
            Some("DEF" | "USE") => true,
            Some(w) => {
                w.parse::<f64>().is_err()
                    && !matches!(w, "TRUE" | "FALSE" | "\"\"")
                    && matches!(self.toks.get(self.pos + 1), Some(Tok::Open))
            }
            None => false,
        }
    }
    fn child(&mut self, depth: usize) -> Result<Node, ModelError> {
        let mut def = None;
        if self.word() == Some("DEF") {
            self.pos += 1;
            def = self.word().map(str::to_string);
            self.pos += 1;
        }
        let n = self.node(depth + 1)?;
        if let Some(d) = def {
            self.defs.insert(d, n.clone());
        }
        Ok(n)
    }
    fn field(&mut self, node: &mut Node, name: &str, depth: usize) -> Result<(), ModelError> {
        match self.peek() {
            Some(Tok::LOpen) => {
                self.pos += 1;
                let mut nums = vec![];
                loop {
                    match self.peek() {
                        None => return err("The VRML file ends inside a list."),
                        Some(Tok::LClose) => {
                            self.pos += 1;
                            break;
                        }
                        _ if self.is_node_start() => {
                            let c = self.child(depth)?;
                            node.kids.push(c);
                        }
                        Some(Tok::Word(w)) => {
                            if let Ok(v) = w.parse::<f64>() {
                                nums.push(v);
                            }
                            self.pos += 1;
                        }
                        Some(_) => return err("The VRML file is damaged inside a list."),
                    }
                }
                if !nums.is_empty() {
                    node.nums.insert(name.to_string(), nums);
                }
            }
            _ if self.is_node_start() => {
                let c = self.child(depth)?;
                if name == "children" {
                    node.kids.push(c);
                } else {
                    node.fields.insert(name.to_string(), c);
                }
            }
            _ => {
                let mut nums = vec![];
                while let Some(Tok::Word(w)) = self.peek() {
                    match w.parse::<f64>() {
                        Ok(v) => nums.push(v),
                        Err(_) => {
                            if !nums.is_empty() {
                                break;
                            }
                            // A bare word value such as TRUE or a string.
                            self.pos += 1;
                            return Ok(());
                        }
                    }
                    self.pos += 1;
                }
                node.nums.insert(name.to_string(), nums);
            }
        }
        Ok(())
    }
}

fn mul(a: &M, b: &M) -> M {
    let mut r = [[0.0; 4]; 4];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, c) in row.iter_mut().enumerate() {
            *c = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}
fn transform_matrix(n: &Node) -> M {
    let t = n.nums.get("translation").filter(|v| v.len() == 3);
    let s = n.nums.get("scale").filter(|v| v.len() == 3);
    let mut scale = [[0.0; 4]; 4];
    for i in 0..4 {
        scale[i][i] = if i < 3 { s.map_or(1.0, |v| v[i]) } else { 1.0 };
    }
    let mut rot = [[0.0; 4]; 4];
    for (i, row) in rot.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    if let Some(r) = n.nums.get("rotation").filter(|v| v.len() == 4) {
        let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if len > 1e-12 {
            let (x, y, z) = (r[0] / len, r[1] / len, r[2] / len);
            let (c, s_, k) = (r[3].cos(), r[3].sin(), 1.0 - r[3].cos());
            rot = [
                [c + x * x * k, x * y * k - z * s_, x * z * k + y * s_, 0.0],
                [y * x * k + z * s_, c + y * y * k, y * z * k - x * s_, 0.0],
                [z * x * k - y * s_, z * y * k + x * s_, c + z * z * k, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
        }
    }
    let mut m = mul(&rot, &scale);
    if let Some(t) = t {
        for i in 0..3 {
            m[i][3] = t[i];
        }
    }
    m
}

struct Out {
    model: LoadedModel,
}
impl Out {
    fn walk(&mut self, n: &Node, parent: &M, depth: usize) -> Result<(), ModelError> {
        if depth > MAX_DEPTH {
            return err("The VRML file nests nodes too deeply.");
        }
        let world = if n.kind == "Transform" {
            mul(parent, &transform_matrix(n))
        } else {
            *parent
        };
        if n.kind == "Shape" {
            return self.shape(n, &world);
        }
        for k in &n.kids {
            self.walk(k, &world, depth + 1)?;
        }
        Ok(())
    }
    fn shape(&mut self, n: &Node, m: &M) -> Result<(), ModelError> {
        let Some(geo) = n
            .fields
            .get("geometry")
            .filter(|g| g.kind == "IndexedFaceSet")
        else {
            return Ok(());
        };
        let pts = geo
            .fields
            .get("coord")
            .and_then(|c| c.nums.get("point"))
            .map_or(&[][..], Vec::as_slice);
        let base = self.model.positions.len();
        let count = pts.len() / 3;
        if base + count > MAX_MODEL_VERTICES {
            return err(format!(
                "The model has more than {MAX_MODEL_VERTICES} vertices, over the limit."
            ));
        }
        for p in pts.as_chunks::<3>().0 {
            let w = |r: usize| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3];
            self.model.positions.push([
                w(0) * VRML_UNIT_MM,
                w(1) * VRML_UNIT_MM,
                w(2) * VRML_UNIT_MM,
            ]);
        }
        let mut face: Vec<u32> = vec![];
        let idx = geo.nums.get("coordIndex").map_or(&[][..], Vec::as_slice);
        for &v in idx.iter().chain(std::iter::once(&-1.0)) {
            if v < 0.0 {
                for i in 1..face.len().saturating_sub(1) {
                    if self.model.indices.len() / 3 >= MAX_MODEL_FACES {
                        return err(format!(
                            "The model has more than {MAX_MODEL_FACES} faces, over the limit."
                        ));
                    }
                    self.model.indices.extend([face[0], face[i], face[i + 1]]);
                }
                face.clear();
            } else {
                if v as usize >= count {
                    return err("The model has a face that points past its vertices.");
                }
                face.push(base as u32 + v as u32);
            }
        }
        if self.model.color.is_none()
            && let Some(c) = n
                .fields
                .get("appearance")
                .and_then(|a| a.fields.get("material"))
                .and_then(|m| m.nums.get("diffuseColor"))
                .filter(|c| c.len() == 3)
        {
            self.model.color = Some([c[0] as f32, c[1] as f32, c[2] as f32]);
        }
        Ok(())
    }
}

/// Read VRML text into triangles in millimetres (0.1 inch per unit).
///
/// # Rules
/// `Inline` and unknown file structure return a plain reason; limits are the
/// shared model limits.
/// # Dependencies
/// `./model_load.rs` for [`LoadedModel`] and the limits.
/// # Do not
/// Do not read other files; `Inline` is refused.
pub fn read_vrml(bytes: &[u8]) -> Result<LoadedModel, ModelError> {
    let text = String::from_utf8_lossy(bytes);
    if !text.trim_start().starts_with("#VRML") {
        return err("The file does not start with a VRML header.");
    }
    let mut p = Parser {
        toks: tokenize(&text),
        pos: 0,
        defs: HashMap::new(),
        count: 0,
    };
    let mut root = Node {
        kind: "Group".into(),
        ..Default::default()
    };
    while p.peek().is_some() {
        let c = p.child(0)?;
        root.kids.push(c);
    }
    let mut out = Out {
        model: LoadedModel {
            positions: vec![],
            indices: vec![],
            color: None,
            body_colors: None,
        },
    };
    let mut id: M = [[0.0; 4]; 4];
    for (i, row) in id.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    out.walk(&root, &id, 0)?;
    if out.model.indices.is_empty() {
        return err("The model file holds no triangles.");
    }
    Ok(out.model)
}
