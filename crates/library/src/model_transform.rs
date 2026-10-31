//! Helpers for editing footprint 3D model transforms in project library overrides.

use crate::Model3D;

/// Rebuild `raw_source` from the structured transform fields.
pub fn refresh_model_raw_source(model: &mut Model3D) {
    model.raw_source = format!(
        "(model \"{}\" (offset (xyz {} {} {})) (scale (xyz {} {} {})) (rotate (xyz {} {} {})))",
        model.path,
        model.offset[0],
        model.offset[1],
        model.offset[2],
        model.scale[0],
        model.scale[1],
        model.scale[2],
        model.rotate[0],
        model.rotate[1],
        model.rotate[2]
    );
}

/// Wrap a degree value into `[0, 360)`.
pub fn normalize_degrees(v: f64) -> f64 {
    let mut d = v % 360.0;
    if d < 0.0 {
        d += 360.0;
    }
    d
}

/// Turn the model 90 degrees counter-clockwise about Z.
pub fn model_rotate_z_90(model: &mut Model3D) {
    model.rotate[2] = normalize_degrees(model.rotate[2] + 90.0);
    refresh_model_raw_source(model);
}

/// Mirror the model across the vertical axis by negating X scale.
pub fn model_mirror_y(model: &mut Model3D) {
    model.scale[0] *= -1.0;
    refresh_model_raw_source(model);
}

/// Advance one step through four Z rotations used to align a model to pin 1.
pub fn model_pin1_cycle(model: &mut Model3D) {
    model.rotate[2] = normalize_degrees(model.rotate[2] + 90.0);
    refresh_model_raw_source(model);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Model3D {
        Model3D {
            path: "pkg.wrl".into(),
            offset: [1.0, 2.0, 3.0],
            scale: [1.0, 1.0, 1.0],
            rotate: [0.0, 0.0, 0.0],
            raw_source: String::new(),
        }
    }

    #[test]
    fn refresh_writes_model_clause() {
        let mut m = sample();
        refresh_model_raw_source(&mut m);
        assert!(m.raw_source.contains("(offset (xyz 1 2 3))"));
        assert!(m.raw_source.contains("(rotate (xyz 0 0 0))"));
    }

    #[test]
    fn four_pin1_cycles_return_to_start() {
        let mut m = sample();
        refresh_model_raw_source(&mut m);
        for _ in 0..4 {
            model_pin1_cycle(&mut m);
        }
        assert!((m.rotate[2] - 0.0).abs() < 1e-9);
    }
}
