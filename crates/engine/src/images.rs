//! Reference pictures and bitmap tracing: PNG attachments, the underlay
//! records, and `image.trace`, which turns a picture into board geometry.
use super::*;
use pcbcraft_io::{TraceOptions, bitmap_trace_polygons, decode_embedded_png};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "file.attach_binary",
            description: "Attach a PNG picture to the project as base64 text, given either as data or as the path of a file on disk; attaching an existing name replaces it. PNG only, at most 4096 pixels per side, about 2 MiB, no animation and no 16-bit depth; at most 8 binary files",
            input_schema: schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":128},"data":{"type":"string","description":"standard base64 of the PNG bytes","maxLength":2_796_204},"path":{"type":"string","minLength":1,"maxLength":1024,"description":"a PNG file to read instead of data"}}),
                &["name"],
            ),
        },
        CommandSpec {
            id: "reference_image.add",
            description: "Show an attached PNG under the schematic or the board for tracing. Position is the top-left corner in millimetres, scale is millimetres per pixel. Reference pictures are editor-only and never reach any export",
            input_schema: schema(
                json!({"file":{"type":"string","minLength":1,"maxLength":128},"view":{"type":"string","enum":["schematic","pcb"]},"x":{"type":"number"},"y":{"type":"number"},"scale":{"type":"number","minimum":0.001,"maximum":100},"opacity":{"type":"number","minimum":0,"maximum":1}}),
                &["file", "view"],
            ),
        },
        CommandSpec {
            id: "reference_image.set",
            description: "Move, rescale, fade or hide a reference picture",
            input_schema: schema(
                json!({"id":id_schema(),"x":{"type":"number"},"y":{"type":"number"},"scale":{"type":"number","minimum":0.001,"maximum":100},"opacity":{"type":"number","minimum":0,"maximum":1},"visible":{"type":"boolean"}}),
                &["id"],
            ),
        },
        CommandSpec {
            id: "reference_image.remove",
            description: "Remove a reference picture; the attached file stays",
            input_schema: schema(json!({"id":id_schema()}), &["id"]),
        },
        CommandSpec {
            id: "image.trace",
            description: "Trace the dark parts of an attached PNG into closed outlines on a board target: silk_front, silk_back, mask_front, mask_back (board graphics with a fabrication sublayer), layer (a drawn machine layer, needs layer) or copper (a filled zone, needs net). Give either file with x, y and scale, or reference to reuse a reference picture's placement. Only the outer edge of each blob is traced, at most 32 blobs. An invalid trace changes nothing",
            input_schema: schema(
                json!({"file":{"type":"string","minLength":1,"maxLength":128},"reference":id_schema(),"target":{"type":"string","enum":["silk_front","silk_back","mask_front","mask_back","layer","copper"]},"layer":id_schema(),"net":id_schema(),"copper_layer":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"scale":{"type":"number","minimum":0.001,"maximum":100},"threshold":{"type":"integer","minimum":0,"maximum":255},"smooth":{"type":"number","minimum":0,"maximum":10,"description":"simplification tolerance in millimetres"},"max_vertices":{"type":"integer","minimum":3,"maximum":256},"invert":{"type":"boolean"},"width":{"type":"number","exclusiveMinimum":0,"maximum":5,"description":"outline stroke width in millimetres for silk and mask targets"}}),
                &["target"],
            ),
        },
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BinaryParams {
    name: String,
    data: Option<String>,
    path: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceAddParams {
    file: String,
    view: String,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    scale: Option<f64>,
    opacity: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceSetParams {
    id: Id,
    x: Option<f64>,
    y: Option<f64>,
    scale: Option<f64>,
    opacity: Option<f64>,
    visible: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdOnly {
    id: Id,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TraceParams {
    file: Option<String>,
    reference: Option<Id>,
    target: String,
    layer: Option<Id>,
    net: Option<Id>,
    #[serde(default = "front")]
    copper_layer: Layer,
    x: Option<f64>,
    y: Option<f64>,
    scale: Option<f64>,
    threshold: Option<u8>,
    smooth: Option<f64>,
    max_vertices: Option<usize>,
    #[serde(default)]
    invert: bool,
    width: Option<f64>,
}

pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    Some(match command {
        "file.attach_binary" => attach_binary(p, params),
        "reference_image.add" => add(p, next, params),
        "reference_image.set" => set(p, params),
        "reference_image.remove" => remove(p, params),
        "image.trace" => trace(p, next, params),
        _ => return None,
    })
}

fn attach_binary(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: BinaryParams = parse(params)?;
    let name = a.name.trim().to_string();
    if name.is_empty() || name.len() > 128 {
        return Err(Error::Invalid(
            "attached file names must be 1 to 128 bytes".into(),
        ));
    }
    let data = match (a.data, a.path) {
        (Some(data), None) => data,
        (None, Some(path)) => {
            pcbcraft_io::read_png_base64(std::path::Path::new(&path)).map_err(Error::Invalid)?
        }
        _ => {
            return Err(Error::Invalid("give either data or path, not both".into()));
        }
    };
    if data.len() > pcbcraft_doc::MAX_BINARY_ATTACHMENT_BASE64 {
        return Err(Error::Invalid(
            "an attached picture holds at most about 2 MiB".into(),
        ));
    }
    let file = pcbcraft_doc::EmbeddedFile {
        name: name.clone(),
        data,
        encoding: "base64".into(),
    };
    // The picture must decode, so a broken file never reaches the project.
    let image = decode_embedded_png(&file).map_err(Error::Invalid)?;
    if let Some(existing) = p.files.iter_mut().find(|f| f.name == name) {
        *existing = file;
    } else {
        if p.files.len() >= 32 {
            return Err(Error::Invalid("a project attaches at most 32 files".into()));
        }
        p.files.push(file);
        p.files.sort_by(|x, y| x.name.cmp(&y.name));
    }
    Ok(
        json!({"name": name, "width": image.width(), "height": image.height(), "count": p.files.len()}),
    )
}

fn picture(p: &Project, name: &str) -> Result<pcbcraft_io::RgbaImage, Error> {
    let file = p
        .files
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| Error::Invalid(format!("no attached file named {name}")))?;
    decode_embedded_png(file).map_err(Error::Invalid)
}

fn add(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: ReferenceAddParams = parse(params)?;
    let image = picture(p, &a.file)?;
    let id = allocate(next)?;
    p.reference_images.push(pcbcraft_doc::ReferenceImage {
        id,
        file: a.file,
        view: a.view,
        x: a.x,
        y: a.y,
        scale: a.scale.unwrap_or(0.1),
        opacity: a.opacity.unwrap_or(0.5),
        visible: true,
    });
    Ok(json!({"id": id, "width": image.width(), "height": image.height()}))
}

fn set(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ReferenceSetParams = parse(params)?;
    let r = p
        .reference_images
        .iter_mut()
        .find(|r| r.id == a.id)
        .ok_or_else(missing)?;
    if let Some(x) = a.x {
        r.x = x;
    }
    if let Some(y) = a.y {
        r.y = y;
    }
    if let Some(scale) = a.scale {
        r.scale = scale;
    }
    if let Some(opacity) = a.opacity {
        r.opacity = opacity;
    }
    if let Some(visible) = a.visible {
        r.visible = visible;
    }
    Ok(json!({"id": a.id}))
}

fn remove(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: IdOnly = parse(params)?;
    let Some(index) = p.reference_images.iter().position(|r| r.id == a.id) else {
        return Err(missing());
    };
    p.reference_images.remove(index);
    Ok(json!({"id": a.id, "count": p.reference_images.len()}))
}

fn trace(p: &mut Project, next: &mut Id, params: &Value) -> Result<Value, Error> {
    let a: TraceParams = parse(params)?;
    let (file, x, y, scale) = match (&a.file, a.reference) {
        (Some(_), Some(_)) => {
            return Err(Error::Invalid(
                "name either file or reference, not both".into(),
            ));
        }
        (None, None) => {
            return Err(Error::Invalid("name a file or a reference picture".into()));
        }
        (Some(file), None) => (
            file.clone(),
            a.x.unwrap_or(0.0),
            a.y.unwrap_or(0.0),
            a.scale.unwrap_or(0.1),
        ),
        (None, Some(id)) => {
            let r = p
                .reference_images
                .iter()
                .find(|r| r.id == id)
                .ok_or_else(missing)?;
            (
                r.file.clone(),
                a.x.unwrap_or(r.x),
                a.y.unwrap_or(r.y),
                a.scale.unwrap_or(r.scale),
            )
        }
    };
    if !x.is_finite() || !y.is_finite() || x.abs() > 100_000.0 || y.abs() > 100_000.0 {
        return Err(Error::Invalid("trace position is out of range".into()));
    }
    let max_vertices = a.max_vertices.unwrap_or(64);
    if a.target == "layer" && max_vertices > 64 {
        return Err(Error::Invalid(
            "a drawn layer outline takes at most 64 vertices".into(),
        ));
    }
    let width = a.width.unwrap_or(0.15);
    if !width.is_finite() || width <= 0.0 || width > 5.0 {
        return Err(Error::Invalid(
            "outline width must be within 0 to 5 mm".into(),
        ));
    }
    let image = picture(p, &file)?;
    let options = TraceOptions {
        threshold: a.threshold.unwrap_or(128),
        smooth_mm: a.smooth.unwrap_or(0.05),
        max_vertices,
        scale,
        invert: a.invert,
    };
    let polys = bitmap_trace_polygons(&image, &options).map_err(Error::Invalid)?;
    let placed: Vec<Vec<Point>> = polys
        .into_iter()
        .map(|poly| {
            poly.into_iter()
                .map(|pt| Point::new(pt.x + x, pt.y + y))
                .collect()
        })
        .collect();
    if placed.iter().flatten().any(|pt| !pt.valid()) {
        return Err(Error::Invalid("traced vertex is out of range".into()));
    }
    let mut ids = Vec::new();
    match a.target.as_str() {
        "silk_front" | "silk_back" | "mask_front" | "mask_back" => {
            let sublayer = match a.target.as_str() {
                "silk_front" => "F.SilkS",
                "silk_back" => "B.SilkS",
                "mask_front" => "F.Mask",
                _ => "B.Mask",
            };
            for poly in placed {
                let id = allocate(next)?;
                p.graphics.push(pcbcraft_doc::Graphic {
                    id,
                    layer: "board".into(),
                    shape: "polygon".into(),
                    points: poly,
                    width,
                    text: String::new(),
                    sublayer: sublayer.into(),
                    color: String::new(),
                });
                ids.push(id);
            }
        }
        "layer" => {
            let layer_id = a
                .layer
                .ok_or_else(|| Error::Invalid("the layer target needs a layer id".into()))?;
            let Some(layer) = p.board.user_layers.iter_mut().find(|l| l.id == layer_id) else {
                return Err(Error::Invalid("unknown drawn layer".into()));
            };
            if layer.shapes.len() + placed.len() > 512 {
                return Err(Error::Invalid(
                    "a drawn layer carries at most 512 figures".into(),
                ));
            }
            for poly in placed {
                layer
                    .shapes
                    .push(pcbcraft_doc::ArtShape::Poly { points: poly });
                ids.push(layer.shapes.len() as u64 - 1);
            }
        }
        "copper" => {
            let net = a
                .net
                .ok_or_else(|| Error::Invalid("the copper target needs a net id".into()))?;
            if !p.nets.iter().any(|n| n.id == net) {
                return Err(missing());
            }
            for poly in placed {
                let mut outline = poly;
                if let Some(first) = outline.first().copied() {
                    outline.push(first);
                }
                pcbcraft_doc::validate_closed_outline(&outline).map_err(Error::Invalid)?;
                let id = allocate(next)?;
                p.board.zones.push(pcbcraft_doc::CopperZone {
                    preserved: pcbcraft_doc::PreservedSource::default(),
                    id,
                    net,
                    layer: a.copper_layer,
                    outline,
                    cutouts: vec![],
                    filled: true,
                    kind: None,
                    preserved: pcbcraft_doc::PreservedSource::default(),
                    pour: None,
                });
                ids.push(id);
            }
        }
        _ => {
            return Err(Error::Invalid(
                "trace target must be silk_front, silk_back, mask_front, mask_back, layer or copper"
                    .into(),
            ));
        }
    }
    Ok(json!({"added": ids.len(), "ids": ids, "target": a.target}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    const LOGO: &[u8] = include_bytes!("../../io/tests/fixtures/logo-64x64-two-color.png");

    fn logo_b64() -> String {
        base64::engine::general_purpose::STANDARD.encode(LOGO)
    }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffff_u32;
        for b in bytes {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
        out
    }
    /// A PNG header that claims a huge picture; nothing past it is needed.
    fn oversized_png() -> Vec<u8> {
        let mut ihdr = 5000_u32.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&1_u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(chunk(b"IHDR", &ihdr));
        png.extend(chunk(b"IDAT", &[0, 0, 0, 0]));
        png
    }

    #[test]
    fn image_trace_to_silk_adds_graphic_with_sublayer() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "file.attach_binary",
            json!({"name":"logo.png","data":logo_b64()}),
            None,
        )?;
        let out = s.execute(
            "image.trace",
            json!({"file":"logo.png","target":"silk_front","x":10,"y":5,"scale":0.1}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["added"], 1);
        let g = s
            .project
            .graphics
            .first()
            .ok_or_else(|| Error::Invalid("no graphic added".into()))?;
        assert_eq!(g.sublayer, "F.SilkS");
        assert_eq!(g.layer, "board");
        assert_eq!(g.shape, "polygon");
        assert!(g.points.len() >= 3 && g.points.len() <= 64);
        assert!(g.points.iter().all(|p| p.x >= 11.0 && p.y >= 6.0));
        let silk = pcbcraft_io::gerber_silk(&s.project, Layer::Front).map_err(Error::Invalid)?;
        assert!(
            silk.contains("D01"),
            "the traced outline reaches the silk file"
        );
        // Back silk and the legacy front-only rule stay apart.
        s.execute(
            "image.trace",
            json!({"file":"logo.png","target":"silk_back"}),
            None,
        )?;
        let back = pcbcraft_io::gerber_silk(&s.project, Layer::Back).map_err(Error::Invalid)?;
        assert!(back.contains("D01"));
        Ok(())
    }

    #[test]
    fn a_picture_attaches_from_a_file_path() -> Result<(), Error> {
        let mut s = Session::default();
        let dir = tempfile::tempdir().map_err(|e| Error::Invalid(e.to_string()))?;
        let path = dir.path().join("logo.png");
        std::fs::write(&path, LOGO).map_err(|e| Error::Invalid(e.to_string()))?;
        let out = s.execute(
            "file.attach_binary",
            json!({"name":"logo.png","path":path.to_string_lossy()}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["width"], 64);
        assert!(
            s.execute(
                "file.attach_binary",
                json!({"name":"b.png","path":dir.path().join("none.png").to_string_lossy()}),
                None
            )
            .is_err(),
            "a missing file is refused"
        );
        assert!(
            s.execute("file.attach_binary", json!({"name":"c.png"}), None)
                .is_err(),
            "data or path is required"
        );
        Ok(())
    }

    #[test]
    fn image_trace_rejects_oversized_png() -> Result<(), Error> {
        let mut s = Session::default();
        let big = base64::engine::general_purpose::STANDARD.encode(oversized_png());
        assert!(
            s.execute(
                "file.attach_binary",
                json!({"name":"big.png","data":big}),
                None
            )
            .is_err(),
            "a picture wider than 4096 pixels is refused"
        );
        assert!(
            s.execute(
                "file.attach_binary",
                json!({"name":"x.png","data":"bm90IGEgcG5n"}),
                None
            )
            .is_err(),
            "bytes that are not a PNG are refused"
        );
        assert!(s.project.files.is_empty(), "a refusal changes nothing");
        Ok(())
    }

    #[test]
    fn image_trace_bad_requests_change_nothing() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "file.attach_binary",
            json!({"name":"logo.png","data":logo_b64()}),
            None,
        )?;
        let before = s.revision;
        for bad in [
            json!({"target":"silk_front"}),
            json!({"file":"logo.png","target":"copper"}),
            json!({"file":"logo.png","target":"copper","net":99}),
            json!({"file":"logo.png","target":"layer","layer":7}),
            json!({"file":"logo.png","target":"nowhere"}),
            json!({"file":"missing.png","target":"silk_front"}),
            json!({"file":"logo.png","target":"silk_front","threshold":0}),
            json!({"file":"logo.png","target":"silk_front","max_vertices":300}),
        ] {
            assert!(
                s.execute("image.trace", bad.clone(), None).is_err(),
                "refused: {bad}"
            );
        }
        assert_eq!(s.revision, before);
        assert!(s.project.graphics.is_empty());
        Ok(())
    }

    #[test]
    fn image_trace_to_copper_zone() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "file.attach_binary",
            json!({"name":"logo.png","data":logo_b64()}),
            None,
        )?;
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("net id".into()))?;
        s.execute(
            "image.trace",
            json!({"file":"logo.png","target":"copper","net":net,"x":5,"y":5}),
            None,
        )?;
        assert_eq!(s.project.board.zones.len(), 1);
        Ok(())
    }

    #[test]
    fn reference_image_save_load_round_trip() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "file.attach_binary",
            json!({"name":"logo.png","data":logo_b64()}),
            None,
        )?;
        let id = s.execute(
            "reference_image.add",
            json!({"file":"logo.png","view":"pcb","x":3,"y":4,"scale":0.2,"opacity":0.4}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(|| Error::Invalid("reference id".into()))?;
        s.execute(
            "reference_image.set",
            json!({"id":id,"opacity":0.75,"visible":false}),
            None,
        )?;
        let dir = tempfile::tempdir().map_err(|e| Error::Invalid(e.to_string()))?;
        let path = dir.path().join("logo.pcbcraft");
        pcbcraft_io::save(&s.project, &path).map_err(Error::Invalid)?;
        let loaded = pcbcraft_io::load(&path).map_err(Error::Invalid)?;
        assert_eq!(loaded.reference_images, s.project.reference_images);
        assert_eq!(loaded.reference_images[0].opacity, 0.75);
        assert!(!loaded.reference_images[0].visible);
        let file = loaded
            .files
            .first()
            .ok_or_else(|| Error::Invalid("attachment lost".into()))?;
        assert_eq!(
            pcbcraft_io::embedded_bytes(file).map_err(Error::Invalid)?,
            LOGO
        );
        // A picture in use cannot be detached, and removing the record frees it.
        assert!(
            s.execute("file.remove", json!({"name":"logo.png"}), None)
                .is_err()
        );
        s.execute("reference_image.remove", json!({"id":id}), None)?;
        s.execute("file.remove", json!({"name":"logo.png"}), None)?;
        Ok(())
    }

    #[test]
    fn reference_image_rejects_bad_values_and_text_files() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("file.attach", json!({"name":"notes.txt","data":"hi"}), None)?;
        s.execute(
            "file.attach_binary",
            json!({"name":"logo.png","data":logo_b64()}),
            None,
        )?;
        for bad in [
            json!({"file":"notes.txt","view":"pcb"}),
            json!({"file":"absent.png","view":"pcb"}),
            json!({"file":"logo.png","view":"3d"}),
            json!({"file":"logo.png","view":"pcb","scale":0.0}),
            json!({"file":"logo.png","view":"pcb","opacity":1.5}),
        ] {
            assert!(
                s.execute("reference_image.add", bad.clone(), None).is_err(),
                "refused: {bad}"
            );
        }
        assert!(s.project.reference_images.is_empty());
        Ok(())
    }
}
