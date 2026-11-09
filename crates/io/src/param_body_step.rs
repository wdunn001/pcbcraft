//! AP214 STEP export for parametric part bodies (millimetres).
use pcbcraft_geom::Point;
use pcbcraft_library::{ParamBody, TriangleMesh};

const STEP_MAX_ENTITIES: u32 = 50_000;

type Coords = (f64, f64, f64);

struct StepDoc {
    lines: Vec<String>,
    next: u32,
}

impl StepDoc {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            next: 1,
        }
    }
    fn add(&mut self, body: &str) -> Result<u32, String> {
        if self.next > STEP_MAX_ENTITIES {
            return Err("the parametric body is too complex for one STEP file".into());
        }
        let id = self.next;
        self.next += 1;
        self.lines.push(format!("#{id} = {body};"));
        Ok(id)
    }
    fn set(&mut self, id: u32, body: &str) {
        self.lines[id as usize - 1] = format!("#{id} = {body};");
    }
    fn n(v: f64) -> String {
        format!("{:.4}", v + 0.0)
    }
    fn point(&mut self, c: Coords) -> Result<u32, String> {
        self.add(&format!(
            "CARTESIAN_POINT('',({},{},{}))",
            Self::n(c.0),
            Self::n(c.1),
            Self::n(c.2)
        ))
    }
    fn direction(&mut self, d: Coords) -> Result<u32, String> {
        self.add(&format!(
            "DIRECTION('',({},{},{}))",
            Self::n(d.0),
            Self::n(d.1),
            Self::n(d.2)
        ))
    }
    fn vector(&mut self, dir: u32, length: f64) -> Result<u32, String> {
        self.add(&format!(
            "VECTOR('',#{dir},{})",
            Self::n(length.max(0.0001))
        ))
    }
    fn face(
        &mut self,
        corners: &[Coords],
        normal: Coords,
        reference: Coords,
    ) -> Result<u32, String> {
        let count = corners.len();
        let mut points = Vec::new();
        for c in corners {
            points.push(self.point(*c)?);
        }
        let mut vertices = Vec::new();
        for p in &points {
            vertices.push(self.add(&format!("VERTEX_POINT('',#{p})"))?);
        }
        let mut oriented = Vec::new();
        for index in 0..count {
            let next = (index + 1) % count;
            let a = corners[index];
            let b = corners[next];
            let delta = (b.0 - a.0, b.1 - a.1, b.2 - a.2);
            let length = (delta.0 * delta.0 + delta.1 * delta.1 + delta.2 * delta.2).sqrt();
            let dir = self.direction(delta)?;
            let vec = self.vector(dir, length)?;
            let line = self.add(&format!("LINE('',#{},#{vec})", points[index]))?;
            let edge = self.add(&format!(
                "EDGE_LINE('',#{line},#{},#{})",
                vertices[index], vertices[next]
            ))?;
            oriented.push(self.add(&format!("ORIENTED_EDGE('',*,*,#{edge},.T.)"))?);
        }
        let listed: Vec<String> = oriented.iter().map(|e| format!("#{e}")).collect();
        let loop_id = self.add(&format!("EDGE_LOOP('',({}))", listed.join(",")))?;
        let bound = self.add(&format!("FACE_OUTER_BOUND('',#{loop_id},.T.)"))?;
        let axis = self.direction(normal)?;
        let ref_dir = self.direction(reference)?;
        let placement = self.add(&format!(
            "AXIS2_PLACEMENT_3D('',#{},#{axis},#{ref_dir})",
            points[0]
        ))?;
        let plane = self.add(&format!("PLANE('',#{placement})"))?;
        self.add(&format!("ADVANCED_FACE((),(#{bound}),#{plane},.T.)"))
    }
    fn prism_between(&mut self, ring: &[Point], z0: f64, z1: f64) -> Result<Vec<u32>, String> {
        let count = ring.len();
        if count < 3 {
            return Ok(Vec::new());
        }
        let mut faces = Vec::new();
        for index in 0..count {
            let next = (index + 1) % count;
            let a = ring[index];
            let b = ring[next];
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let length = (dx * dx + dy * dy).sqrt();
            if length < 1e-9 {
                continue;
            }
            let corners = [
                (a.x, a.y, z0),
                (b.x, b.y, z0),
                (b.x, b.y, z1),
                (a.x, a.y, z1),
            ];
            faces.push(self.face(
                &corners,
                (dy / length, -dx / length, 0.0),
                (dx / length, dy / length, 0.0),
            )?);
        }
        let bottom: Vec<Coords> = ring.iter().map(|p| (p.x, p.y, z0)).collect();
        let mut top: Vec<Coords> = ring.iter().map(|p| (p.x, p.y, z1)).collect();
        top.reverse();
        faces.push(self.face(&bottom, (0.0, 0.0, -1.0), (1.0, 0.0, 0.0))?);
        faces.push(self.face(&top, (0.0, 0.0, 1.0), (1.0, 0.0, 0.0))?);
        Ok(faces)
    }
}

fn mesh_aabb_ring(mesh: &TriangleMesh) -> Result<(Vec<Point>, f64, f64), String> {
    let (min, max) = mesh.bounds().ok_or("parametric body has no geometry")?;
    if !(min[0].is_finite()
        && min[1].is_finite()
        && max[0].is_finite()
        && max[1].is_finite()
        && min[2].is_finite()
        && max[2].is_finite())
    {
        return Err("parametric body bounds are not finite".into());
    }
    let ring = vec![
        Point::new(min[0], min[1]),
        Point::new(max[0], min[1]),
        Point::new(max[0], max[1]),
        Point::new(min[0], max[1]),
    ];
    Ok((ring, min[2], max[2]))
}

/// Write an AP214 STEP solid for the parametric body axis-aligned bounds.
pub fn step_param_body(body: &ParamBody) -> Result<String, String> {
    let mesh = body.mesh();
    mesh.validate().map_err(|e| e.to_string())?;
    let (ring, z0, z1) = mesh_aabb_ring(&mesh)?;
    let mut doc = StepDoc::new();
    doc.add("PRODUCT('PARAM_BODY','pcbcraft parametric body','',('Mechanical Design'))")?;
    doc.add("PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE('','',#1,.NOT_KNOWN.)")?;
    doc.add("PRODUCT_CONTEXT('',#1,'mechanical')")?;
    doc.add("PRODUCT_DEFINITION('design','',#2,#3)")?;
    doc.add("PRODUCT_DEFINITION_SHAPE('','',#4)")?;
    let shape = doc.add("SHAPE_REPRESENTATION('body',(#14,#15),#7)")?;
    doc.add("(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#16))GLOBAL_UNIT_ASSIGNED_CONTEXT((#8,#9,#10))REPRESENTATION_CONTEXT('','',''))")?;
    doc.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.)LENGTH_MEASURE())")?;
    doc.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))")?;
    doc.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_MEASURE())")?;
    doc.add("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.001),#8,'distance_accuracy_value','confusion accuracy')")?;
    let origin = doc.point((0.0, 0.0, 0.0))?;
    let axis = doc.direction((0.0, 0.0, 1.0))?;
    let reference = doc.direction((1.0, 0.0, 0.0))?;
    let base = doc.add(&format!(
        "AXIS2_PLACEMENT_3D('',#{origin},#{axis},#{reference})"
    ))?;
    doc.add("SHAPE_DEFINITION_REPRESENTATION(#5,#6)")?;
    let faces = doc.prism_between(&ring, z0, z1)?;
    if faces.is_empty() {
        return Err("parametric body produced no STEP faces".into());
    }
    let listed_faces: Vec<String> = faces.iter().map(|f| format!("#{f}")).collect();
    let shell = doc.add(&format!("CLOSED_SHELL('',({}))", listed_faces.join(",")))?;
    let solid = doc.add(&format!("MANIFOLD_SOLID_BREP('parametric_body',#{shell})"))?;
    let mut items = vec![base, solid, shell];
    items.extend(faces);
    let listed: Vec<String> = items.iter().map(|i| format!("#{i}")).collect();
    doc.set(
        shape,
        &format!("SHAPE_REPRESENTATION('body',({}),#7)", listed.join(",")),
    );
    let mut out = String::new();
    out.push_str("ISO-10303-21;\nHEADER;\n");
    out.push_str("FILE_DESCRIPTION(('pcbcraft parametric body export'),'2;1');\n");
    out.push_str("FILE_NAME('parametric.step','','','(pcbcraft)','pcbcraft','pcbcraft','');\n");
    out.push_str("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\n");
    out.push_str("ENDSEC;\nDATA;\n");
    for line in &doc.lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(out)
}
