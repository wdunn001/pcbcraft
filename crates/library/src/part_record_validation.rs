use crate::part_types::*;
use crate::{Error, PackageKey, ParamBody, TriangleMesh};

fn bad(message: &str) -> Error {
    Error::Invalid(message.to_owned())
}

fn text(s: &str) -> Result<(), Error> {
    if s.len() > 256 * 1024 || s.contains('\0') {
        Err(bad("text exceeds bounds or contains NUL"))
    } else {
        Ok(())
    }
}

fn scalar(v: f64) -> bool {
    v.is_finite() && v.abs() <= 100_000.0
}

impl TriangleMesh {
    pub fn validate(&self) -> Result<(), Error> {
        if self.vertices.len() > 100_000 || self.triangles.len() > 200_000 {
            return Err(bad("mesh exceeds vertex or triangle cap"));
        }
        for v in &self.vertices {
            if !v.iter().all(|c| scalar(*c)) {
                return Err(bad("mesh vertex out of range"));
            }
        }
        let max_index = self.vertices.len() as u32;
        for tri in &self.triangles {
            if tri.iter().any(|i| *i >= max_index) {
                return Err(bad("mesh triangle index out of range"));
            }
        }
        Ok(())
    }
}

impl ParamBody {
    pub fn validate(&self) -> Result<(), Error> {
        text(&self.category)?;
        self.key.validate()
    }
}

impl PackageKey {
    pub fn validate(&self) -> Result<(), Error> {
        if self.pin_count == 0 || self.pin_count > 512 {
            return Err(bad("package pin count out of range"));
        }
        for v in [
            self.pitch_mm,
            self.body_length_mm,
            self.body_width_mm,
            self.lead_span_mm,
            self.exposed_pad_mm,
            self.height_mm,
        ] {
            if !scalar(v) || v < 0.0 {
                return Err(bad("package key dimension invalid"));
            }
        }
        Ok(())
    }
}

impl Part {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id == 0 {
            return Err(bad("part id must be non-zero"));
        }
        for s in [
            &self.manufacturer,
            &self.mpn,
            &self.base_mpn,
            &self.category,
            &self.lifecycle,
            &self.symbol,
            &self.footprint,
        ] {
            text(s)?;
        }
        if self.mpn.is_empty() {
            return Err(bad("part needs an MPN"));
        }
        if self.symbol.is_empty() || self.footprint.is_empty() {
            return Err(bad("part needs symbol and footprint names"));
        }
        if self.pins.len() > MAX_PART_PINS {
            return Err(bad("part pin list too long"));
        }
        if self.requirements.len() > MAX_REQUIREMENTS {
            return Err(bad("part requirements list too long"));
        }
        self.package_key.validate()?;
        for p in &self.pins {
            text(&p.number)?;
            text(&p.name)?;
            text(&p.library_type)?;
            if p.number.is_empty() {
                return Err(bad("pin number required"));
            }
            for pad in &p.pads {
                text(pad)?;
            }
        }
        for o in &self.orderables {
            text(&o.distributor)?;
            text(&o.sku)?;
        }
        if let Some(ds) = &self.datasheet {
            text(&ds.url)?;
            text(&ds.sha256)?;
            text(&ds.retrieved_at)?;
            text(&ds.checked_at)?;
            if let Some(path) = &ds.local_path {
                text(path)?;
            }
        }
        if let Some(model) = &self.model3d {
            model.validate()?;
        }
        for n in &self.layout_notes {
            text(n)?;
        }
        for a in &self.attribution {
            text(a)?;
        }
        for w in &self.import_warnings {
            text(w)?;
        }
        for r in &self.requirements {
            text(&r.kind)?;
            text(&r.detail)?;
            text(&r.source)?;
        }
        Ok(())
    }
}

impl Model3dRef {
    pub fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Step { path, offset, scale, rotate }
            | Self::Wrl { path, offset, scale, rotate }
            | Self::Gltf { path, offset, scale, rotate } => {
                text(path)?;
                if path.is_empty() || path.len() > 1024 {
                    return Err(bad("model path invalid"));
                }
                if !offset.iter().all(|v| scalar(*v))
                    || !rotate.iter().all(|v| scalar(*v))
                    || !scale.iter().all(|v| scalar(*v) && *v > 0.0)
                {
                    return Err(bad("model transform invalid"));
                }
            }
            Self::Parametric { body } => body.validate()?,
        }
        Ok(())
    }
}
