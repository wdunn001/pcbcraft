//! Reads 3D model files into plain triangle lists in millimetres, Z up.
//! Pure Rust and wasm-safe: bytes in, triangles out, no GPU and no threads.
//! Placement on the board is not done here; `board_mesh::with_models` applies
//! the footprint matrix. Every limit returns a plain-sentence [`ModelError`]
//! that the 3D View shows beside the part, so a bad file never panics.
//!
//! Formats: glTF and GLB, a bounded VRML subset
//! (`vrml_read.rs`) and STEP solids (`step_read.rs`, millimetres).
//! Units: glTF is metres and Y up, so it is scaled by 1000 and turned to Z up.
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Bumped whenever the reader changes what a file produces, so a cache of
/// older results is never served.
pub const LOADER_VERSION: u32 = 1;
/// Largest model file read, in bytes.
pub const MAX_MODEL_BYTES: usize = 32 * 1024 * 1024;
/// Most vertices one model may produce.
pub const MAX_MODEL_VERTICES: usize = 500_000;
/// Most triangles one model may produce.
pub const MAX_MODEL_FACES: usize = 300_000;
const MAX_NODE_DEPTH: usize = 64;
const MAX_NODES_VISITED: usize = 20_000;

/// One model as triangles in millimetres, Z up, in the model's own frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadedModel {
    /// Vertex positions in millimetres.
    pub positions: Vec<[f64; 3]>,
    /// Triangle corner indices into `positions`, three per face.
    pub indices: Vec<u32>,
    /// Diffuse colour 0..1 of the first material, when the file has one.
    pub color: Option<[f32; 3]>,
}
impl LoadedModel {
    /// Axis-aligned bounds, or `None` for an empty model.
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let first = *self.positions.first()?;
        Some(
            self.positions
                .iter()
                .fold((first, first), |(mut lo, mut hi), p| {
                    for i in 0..3 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i]);
                    }
                    (lo, hi)
                }),
        )
    }
}

/// Why a model file could not be used, as one plain sentence.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct ModelError(
    /// The plain sentence shown beside the part.
    pub String,
);
fn err<T>(s: impl Into<String>) -> Result<T, ModelError> {
    Err(ModelError(s.into()))
}

/// Identity of a model file for caching: changes when the file, its size,
/// its modification time or the reader changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelCacheKey {
    /// The model path as the footprint names it.
    pub path: String,
    /// File size in bytes.
    pub len: u64,
    /// Modification time in nanoseconds since the Unix epoch, if known.
    pub mtime_nanos: Option<u128>,
    /// [`LOADER_VERSION`] when the key was made.
    pub loader_version: u32,
}
/// The cache key for a file with this size and modification time.
pub fn model_cache_key(
    path: &str,
    len: u64,
    mtime: Option<std::time::SystemTime>,
) -> ModelCacheKey {
    ModelCacheKey {
        path: path.to_string(),
        len,
        mtime_nanos: mtime
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos()),
        loader_version: LOADER_VERSION,
    }
}

/// True when the extension names a format this tool can read now.
pub fn model_extension_supported(path: &str) -> bool {
    matches!(
        extension(path).as_str(),
        "gltf" | "glb" | "wrl" | "vrml" | "step" | "stp"
    )
}
fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Read one model file from its bytes. `path` picks the format by extension
/// and the folder for a glTF that points at a separate `.bin` file.
///
/// # Rules
/// Refuses files over [`MAX_MODEL_BYTES`], [`MAX_MODEL_VERTICES`] or
/// [`MAX_MODEL_FACES`] with a plain reason.
/// # Dependencies
/// `gltf` crate with default features off (no image decoding).
/// # Do not
/// Do not place the model on the board here; that is `board_mesh::with_models`.
pub fn load_model(path: &str, bytes: &[u8]) -> Result<LoadedModel, ModelError> {
    if bytes.len() > MAX_MODEL_BYTES {
        return err(format!(
            "The model file is {} MB, over the {} MB limit.",
            bytes.len() / (1024 * 1024),
            MAX_MODEL_BYTES / (1024 * 1024)
        ));
    }
    match extension(path).as_str() {
        "gltf" | "glb" => load_gltf(path, bytes),
        "wrl" | "vrml" => crate::vrml_read::read_vrml(bytes),
        "step" | "stp" => crate::step_read::read_step(bytes),
        "" => err("The model file has no extension, so its format is unknown."),
        other => err(format!("The model format .{other} is not supported.")),
    }
}

fn buffer_bytes(
    source: gltf::buffer::Source<'_>,
    blob: &mut Option<Vec<u8>>,
    dir: Option<&Path>,
) -> Result<Vec<u8>, ModelError> {
    match source {
        gltf::buffer::Source::Bin => blob
            .take()
            .map_or_else(|| err("The model file is missing its binary part."), Ok),
        gltf::buffer::Source::Uri(uri) => {
            if let Some(rest) = uri.strip_prefix("data:") {
                let (_, data) = rest
                    .split_once(";base64,")
                    .map_or_else(|| err("The model has an unreadable embedded buffer."), Ok)?;
                return base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|_| ModelError("The model has a damaged embedded buffer.".into()));
            }
            read_sibling(uri, dir)
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn read_sibling(uri: &str, dir: Option<&Path>) -> Result<Vec<u8>, ModelError> {
    let bad = uri.contains("..") || uri.contains(':') || uri.starts_with('/') || uri.contains('\\');
    let Some(dir) = dir.filter(|_| !bad) else {
        return err("The model points at a buffer file outside its own folder.");
    };
    let file = dir.join(uri.replace("%20", " "));
    match std::fs::metadata(&file) {
        Ok(m) if m.len() as usize > MAX_MODEL_BYTES => {
            return err("The model's buffer file is over the size limit.");
        }
        Ok(_) => {}
        Err(_) => return err(format!("The model's buffer file {uri} is missing.")),
    }
    std::fs::read(&file)
        .map_err(|_| ModelError(format!("The model's buffer file {uri} is unreadable.")))
}
#[cfg(target_arch = "wasm32")]
fn read_sibling(_uri: &str, _dir: Option<&Path>) -> Result<Vec<u8>, ModelError> {
    err("Separate buffer files load in the desktop app.")
}

type M = [[f64; 4]; 4];
fn mat_mul(a: &M, b: &M) -> M {
    let mut r = [[0.0; 4]; 4];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}
/// glTF stores column-major; this keeps row-major with the column vector on
/// the right, so `apply` is `M * p`.
fn node_matrix(m: [[f32; 4]; 4]) -> M {
    let mut r = [[0.0; 4]; 4];
    for (c, col) in m.iter().enumerate() {
        for (row, v) in col.iter().enumerate() {
            r[row][c] = f64::from(*v);
        }
    }
    r
}
const IDENTITY: M = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];
/// Metres to millimetres, then Y up to Z up (a quarter turn about X).
fn to_board_frame(m: &M, p: [f32; 3]) -> [f64; 3] {
    let p = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
    let w = |r: usize| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3];
    let (x, y, z) = (w(0), w(1), w(2));
    [x * 1000.0, -z * 1000.0, y * 1000.0]
}

struct Walk<'a> {
    buffers: &'a [Vec<u8>],
    out: LoadedModel,
    visited: usize,
}
impl Walk<'_> {
    fn node(&mut self, node: gltf::Node<'_>, parent: &M, depth: usize) -> Result<(), ModelError> {
        self.visited += 1;
        if depth > MAX_NODE_DEPTH || self.visited > MAX_NODES_VISITED {
            return err("The model has too many nested parts.");
        }
        let world = mat_mul(parent, &node_matrix(node.transform().matrix()));
        if let Some(mesh) = node.mesh() {
            for prim in mesh.primitives() {
                self.primitive(&prim, &world)?;
            }
        }
        for child in node.children() {
            self.node(child, &world, depth + 1)?;
        }
        Ok(())
    }
    fn primitive(&mut self, prim: &gltf::Primitive<'_>, world: &M) -> Result<(), ModelError> {
        if prim.mode() != gltf::mesh::Mode::Triangles {
            return Ok(());
        }
        let reader = prim.reader(|b| self.buffers.get(b.index()).map(Vec::as_slice));
        let Some(positions) = reader.read_positions() else {
            return Ok(());
        };
        let base = self.out.positions.len();
        let before = self.out.positions.len();
        for p in positions {
            if self.out.positions.len() >= MAX_MODEL_VERTICES {
                return err(format!(
                    "The model has more than {MAX_MODEL_VERTICES} vertices, over the limit."
                ));
            }
            self.out.positions.push(to_board_frame(world, p));
        }
        let count = self.out.positions.len() - before;
        let indices: Vec<u32> = match reader.read_indices() {
            Some(i) => i.into_u32().collect(),
            None => (0..count as u32).collect(),
        };
        if self.out.indices.len() / 3 + indices.len() / 3 > MAX_MODEL_FACES {
            return err(format!(
                "The model has more than {MAX_MODEL_FACES} faces, over the limit."
            ));
        }
        for tri in indices.as_chunks::<3>().0 {
            if tri.iter().any(|i| *i as usize >= count) {
                return err("The model has a face that points past its vertices.");
            }
            self.out.indices.extend(tri.iter().map(|i| base as u32 + i));
        }
        if self.out.color.is_none() {
            let c = prim.material().pbr_metallic_roughness().base_color_factor();
            self.out.color = Some([c[0], c[1], c[2]]);
        }
        Ok(())
    }
}

fn load_gltf(path: &str, bytes: &[u8]) -> Result<LoadedModel, ModelError> {
    let parsed = gltf::Gltf::from_slice(bytes).map_err(|_| {
        ModelError("The glTF file could not be read; it may be damaged or not glTF.".into())
    })?;
    let gltf::Gltf { document, mut blob } = parsed;
    let dir = Path::new(path).parent();
    let mut buffers = Vec::new();
    for b in document.buffers() {
        buffers.push(buffer_bytes(b.source(), &mut blob, dir)?);
    }
    let mut walk = Walk {
        buffers: &buffers,
        out: LoadedModel {
            positions: vec![],
            indices: vec![],
            color: None,
        },
        visited: 0,
    };
    match document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        Some(scene) => {
            for node in scene.nodes() {
                walk.node(node, &IDENTITY, 0)?;
            }
        }
        None => {
            for node in document.nodes() {
                walk.node(node, &IDENTITY, 0)?;
            }
        }
    }
    if walk.out.indices.is_empty() {
        return err("The model file holds no triangles.");
    }
    Ok(walk.out)
}

/// A shared handle to a loaded model, cheap to clone between threads.
pub type SharedModel = Arc<LoadedModel>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_matrix, model_matrix};

    type Check = Result<(), String>;
    const WEDGE: &[u8] = include_bytes!("../tests/fixtures/wedge.gltf");

    #[test]
    fn gltf_triangle_loads_in_millimetres() -> Check {
        let m = load_model("wedge.gltf", WEDGE).map_err(|e| e.to_string())?;
        assert_eq!(m.positions.len(), 4, "four vertices");
        assert_eq!(m.indices.len(), 6, "two faces");
        let (lo, hi) = m.bounds().ok_or("no bounds")?;
        let near = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        assert!(near(lo, [0.0, 0.0, 0.0]), "low corner {lo:?}");
        assert!(
            near(hi, [4.0, 3.0, 2.0]),
            "metres become millimetres and Y up becomes Z up: {hi:?}"
        );
        assert_eq!(m.color, Some([1.0, 0.0, 0.0]));
        Ok(())
    }

    #[test]
    fn oversized_model_is_refused_with_a_reason() {
        let big = vec![0u8; MAX_MODEL_BYTES + 1];
        let e = load_model("big.glb", &big)
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(e.contains("over the 32 MB limit"), "{e}");
        let junk = load_model("x.gltf", b"not json")
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(junk.contains("could not be read"), "{junk}");
        let odd = load_model("x.abc", b"1")
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(odd.contains(".abc"), "{odd}");
    }

    #[test]
    fn placement_applies_scale_then_turns_then_offset() -> Check {
        let m = load_model("wedge.gltf", WEDGE).map_err(|e| e.to_string())?;
        let model = pcbcraft_library::Model3D {
            path: "wedge.gltf".into(),
            offset: [10.0, 0.0, 0.0],
            scale: [2.0, 1.0, 1.0],
            rotate: [0.0, 0.0, 90.0],
            raw_source: String::new(),
        };
        let mat = model_matrix(&model, pcbcraft_geom::Point::new(0.0, 0.0), 0.0, false, 0.0);
        let at = |i: usize| apply_matrix(&mat, m.positions[i]);
        let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        // Vertex (4,0,0): scale x2 -> (8,0,0); turn -90 about Z -> (0,-8,0);
        // offset 10 in x -> (10,-8,0). An asymmetric wedge pins the order.
        assert!(close(at(1), [10.0, -8.0, 0.0]), "{:?}", at(1));
        // Vertex (0,0,2) is on the turn axis: only the offset moves it.
        assert!(close(at(2), [10.0, 0.0, 2.0]), "{:?}", at(2));
        // Vertex (0,3,0): turn -90 about Z -> (3,0,0); offset -> (13,0,0).
        assert!(close(at(3), [13.0, 0.0, 0.0]), "{:?}", at(3));
        Ok(())
    }

    const VRML: &str = "#VRML V2.0 utf8\nDEF A Transform { translation 1 0 0 children [ Shape { appearance Appearance { material Material { diffuseColor 0 1 0 } } geometry IndexedFaceSet { coord Coordinate { point [ 0 0 0, 1 0 0, 0 2 0, 0 0 3 ] } coordIndex [ 0 1 2 -1 0 3 1 -1 ] } } ] }\nUSE A\n";

    #[test]
    fn vrml_unit_is_a_tenth_of_an_inch() -> Check {
        let m = load_model("p.wrl", VRML.as_bytes()).map_err(|e| e.to_string())?;
        assert_eq!(m.positions.len(), 8, "the shape is drawn twice through USE");
        assert_eq!(m.indices.len(), 12);
        let (lo, hi) = m.bounds().ok_or("bounds")?;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(lo[0], 2.54) && near(hi[0], 5.08), "x {lo:?} {hi:?}");
        assert!(near(hi[1], 5.08) && near(hi[2], 7.62), "y and z {hi:?}");
        assert_eq!(m.color, Some([0.0, 1.0, 0.0]));
        Ok(())
    }

    #[test]
    fn vrml_inline_is_refused() {
        let t = "#VRML V2.0 utf8\nInline { url \"other.wrl\" }\n";
        let e = load_model("p.wrl", t.as_bytes())
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(e.contains("Inline"), "{e}");
    }

    #[test]
    fn step_solid_loads_in_millimetres() -> Check {
        let m = load_model("box.step", include_bytes!("../tests/fixtures/box.step"))
            .map_err(|e| e.to_string())?;
        assert!(
            m.indices.len() / 3 >= 12,
            "six faces make at least 12 triangles"
        );
        let (lo, hi) = m.bounds().ok_or("bounds")?;
        let near = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(near(lo, [0.0, 0.0, 0.0]), "low corner {lo:?}");
        assert!(near(hi, [10.0, 5.0, 2.0]), "a 10 x 5 x 2 mm box: {hi:?}");
        let broken = load_model("x.step", b"ISO-10303-21; garbage")
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(broken.contains("STEP"), "{broken}");
        Ok(())
    }

    #[test]
    fn cache_key_changes_with_mtime() {
        let t0 = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let t1 = t0 + std::time::Duration::from_secs(1);
        let a = model_cache_key("a.glb", 10, Some(t0));
        assert_eq!(a, model_cache_key("a.glb", 10, Some(t0)));
        assert_ne!(a, model_cache_key("a.glb", 10, Some(t1)));
        assert_ne!(a, model_cache_key("a.glb", 11, Some(t0)));
    }
}
