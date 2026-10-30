//! Material classes for the 3D board mesh and colour classification for loaded models.

/// A small set of part materials shared by the mesh builder and the software renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MaterialClass {
    MouldCompound = 0,
    TinLead = 1,
    GoldContact = 2,
    Ceramic = 3,
    Resistor = 4,
    CanSleeve = 5,
    ConnectorPlastic = 6,
    Lens = 7,
    Marking = 8,
    /// Triangle keeps its own sRGB colour in the parallel authored list.
    Authored = 255,
}

impl MaterialClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::MouldCompound => "mould_compound",
            Self::TinLead => "tin_lead",
            Self::GoldContact => "gold_contact",
            Self::Ceramic => "ceramic",
            Self::Resistor => "resistor",
            Self::CanSleeve => "can_sleeve",
            Self::ConnectorPlastic => "connector_plastic",
            Self::Lens => "lens",
            Self::Marking => "marking",
            Self::Authored => "authored",
        }
    }
}

/// Convert one sRGB channel in 0..1 to linear light.
pub fn srgb_channel_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert an sRGB triplet in 0..1 to linear RGB.
pub fn srgb_to_linear(c: [f32; 3]) -> [f32; 3] {
    [
        srgb_channel_to_linear(c[0]),
        srgb_channel_to_linear(c[1]),
        srgb_channel_to_linear(c[2]),
    ]
}

/// Convert linear RGB in 0..1 to one sRGB byte.
pub fn linear_to_srgb_byte(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Map a loaded model diffuse colour to a class, or keep it authored.
pub fn classify_model_color(c: [f32; 3]) -> (MaterialClass, [f32; 3]) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let lum = (max + min) * 0.5;
    let sat = if max <= 1e-6 {
        0.0
    } else {
        (max - min) / max
    };
    if lum < 0.12 {
        return (MaterialClass::MouldCompound, c);
    }
    if sat < 0.12 && lum < 0.72 {
        return (MaterialClass::TinLead, c);
    }
    if sat < 0.25 && lum >= 0.55 {
        return (MaterialClass::TinLead, c);
    }
    if c[0] > 0.55 && c[1] > 0.45 && c[2] < 0.35 && lum > 0.35 {
        return (MaterialClass::GoldContact, c);
    }
    (MaterialClass::Authored, c)
}

/// How a triangle should look under the software 3D renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialSurface {
    /// Diffuse albedo in sRGB 0..1.
    pub base_srgb: [f32; 3],
    pub metal: bool,
    pub shine: f32,
}

/// Board finish and part role for [`material_surface`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialDraw {
    pub gold_finish: bool,
    /// True for solder-mask sheet parts.
    pub mask_sheet: bool,
}

fn srgb255(c: [f32; 3]) -> [f32; 3] {
    [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0]
}

const MASK: [f32; 3] = [18.0, 104.0, 58.0];
const FR4: [f32; 3] = [196.0, 172.0, 112.0];
const SILK: [f32; 3] = [244.0, 244.0, 238.0];
const PACKAGE: [f32; 3] = [38.0, 40.0, 47.0];
const GOLD: [f32; 3] = [222.0, 176.0, 62.0];
const TIN: [f32; 3] = [200.0, 202.0, 206.0];
const RESISTOR: [f32; 3] = [48.0, 38.0, 32.0];
const CAN: [f32; 3] = [62.0, 58.0, 54.0];
const CONNECTOR: [f32; 3] = [28.0, 32.0, 38.0];
const LENS: [f32; 3] = [180.0, 220.0, 160.0];

/// Resolve a material class to base colour and shading knobs for the 3D tab.
pub fn material_surface(
    mat: MaterialClass,
    authored: [f32; 3],
    draw: MaterialDraw,
) -> MaterialSurface {
    match mat {
        MaterialClass::Authored => MaterialSurface {
            base_srgb: authored,
            metal: false,
            shine: 0.22,
        },
        MaterialClass::TinLead => MaterialSurface {
            base_srgb: srgb255(TIN),
            metal: true,
            shine: 0.55,
        },
        MaterialClass::GoldContact => MaterialSurface {
            base_srgb: srgb255(if draw.gold_finish { GOLD } else { TIN }),
            metal: true,
            shine: 0.62,
        },
        MaterialClass::Ceramic => MaterialSurface {
            base_srgb: srgb255(FR4),
            metal: false,
            shine: 0.1,
        },
        MaterialClass::Resistor => MaterialSurface {
            base_srgb: srgb255(RESISTOR),
            metal: false,
            shine: 0.18,
        },
        MaterialClass::CanSleeve => MaterialSurface {
            base_srgb: srgb255(CAN),
            metal: false,
            shine: 0.28,
        },
        MaterialClass::ConnectorPlastic => MaterialSurface {
            base_srgb: srgb255(CONNECTOR),
            metal: false,
            shine: 0.2,
        },
        MaterialClass::Lens => MaterialSurface {
            base_srgb: srgb255(LENS),
            metal: false,
            shine: 0.35,
        },
        MaterialClass::Marking => MaterialSurface {
            base_srgb: srgb255(SILK),
            metal: false,
            shine: 0.0,
        },
        MaterialClass::MouldCompound => MaterialSurface {
            base_srgb: srgb255(if draw.mask_sheet { MASK } else { PACKAGE }),
            metal: false,
            shine: if draw.mask_sheet { 0.0 } else { 0.22 },
        },
    }
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn env_reflection(n: [f64; 3], view: [f64; 3]) -> f32 {
    let up = [0.0, 0.0, 1.0];
    let hemi = (dot3(n, up) * 0.5 + 0.5).clamp(0.0, 1.0) as f32;
    let sky = 0.55 + 0.35 * hemi;
    let facing = dot3(n, view).max(0.0) as f32;
    sky * (0.35 + 0.65 * facing.powf(3.0))
}

/// Camera-fixed lights for flat triangle shading in the 3D tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialLights {
    pub toward_eye: [f64; 3],
    pub key: [f64; 3],
    pub fill: [f64; 3],
    pub rim: [f64; 3],
    pub half: [f64; 3],
}

/// Shade one pixel in linear light and return sRGB bytes.
pub fn shade_material_pixel(
    surface: MaterialSurface,
    normal: [f64; 3],
    lights: MaterialLights,
) -> [u8; 3] {
    let base = srgb_to_linear(surface.base_srgb);
    let hemi = (dot3(normal, [0.0, 0.0, 1.0]) * 0.5 + 0.5).max(0.0) as f32;
    let sky = srgb_to_linear([0.55, 0.62, 0.72]);
    let ground = srgb_to_linear([0.06, 0.05, 0.05]);
    let amb = [
        ground[0] + (sky[0] - ground[0]) * hemi,
        ground[1] + (sky[1] - ground[1]) * hemi,
        ground[2] + (sky[2] - ground[2]) * hemi,
    ];
    let ndv = dot3(normal, lights.toward_eye).max(0.0) as f32;
    let ndk = dot3(normal, lights.key).max(0.0) as f32;
    let ndf = dot3(normal, lights.fill).max(0.0) as f32;
    let ndr = dot3(normal, lights.rim).max(0.0) as f32;
    let diff = 0.18 * ndv + 0.38 * ndk + 0.16 * ndf + 0.1 * ndr;
    let mut col = [
        base[0] * (amb[0] * 0.55 + diff),
        base[1] * (amb[1] * 0.55 + diff),
        base[2] * (amb[2] * 0.55 + diff),
    ];
    if surface.metal {
        let env = env_reflection(normal, lights.toward_eye);
        col[0] += env * 0.45;
        col[1] += env * 0.42;
        col[2] += env * 0.35;
    }
    let spec = surface.shine * dot3(normal, lights.half).max(0.0).powf(24.0) as f32;
    col[0] += spec;
    col[1] += spec;
    col[2] += spec;
    [
        linear_to_srgb_byte(col[0]),
        linear_to_srgb_byte(col[1]),
        linear_to_srgb_byte(col[2]),
    ]
}

/// Parametric body compound from its family word.
pub fn parametric_body_material(family: &str, height: f64) -> MaterialClass {
    match family {
        "axial" => MaterialClass::Resistor,
        "radial" if height >= 7.0 => MaterialClass::CanSleeve,
        "radial" => MaterialClass::Ceramic,
        "header" => MaterialClass::ConnectorPlastic,
        "chip" => MaterialClass::Resistor,
        _ => MaterialClass::MouldCompound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_black_stays_dark_in_linear() {
        let l = srgb_to_linear([0.0, 0.0, 0.0]);
        assert!(l.iter().all(|v| *v < 1e-6));
    }

    #[test]
    fn near_black_model_colours_map_to_mould_compound() {
        let (c, _) = classify_model_color([0.05, 0.04, 0.04]);
        assert_eq!(c, MaterialClass::MouldCompound);
    }

    #[test]
    fn grey_model_colours_map_to_tin() {
        let (c, _) = classify_model_color([0.45, 0.46, 0.47]);
        assert_eq!(c, MaterialClass::TinLead);
    }

    #[test]
    fn saturated_green_stays_authored() {
        let (c, rgb) = classify_model_color([0.0, 1.0, 0.0]);
        assert_eq!(c, MaterialClass::Authored);
        assert_eq!(rgb, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn metal_shade_brightens_more_than_package() {
        let draw = MaterialDraw {
            gold_finish: false,
            mask_sheet: false,
        };
        let tin = material_surface(MaterialClass::TinLead, [0.0; 3], draw);
        let pkg = material_surface(MaterialClass::MouldCompound, [0.0; 3], draw);
        let n = [0.0, 0.0, 1.0];
        let lights = MaterialLights {
            toward_eye: [0.0, 0.0, 1.0],
            key: [0.6, -0.4, 0.7],
            fill: [0.2, 0.8, 0.4],
            rim: [0.1, 0.2, -0.9],
            half: [0.4, -0.2, 0.85],
        };
        let tin_px = shade_material_pixel(tin, n, lights);
        let pkg_px = shade_material_pixel(pkg, n, lights);
        let tin_lum = tin_px[0] as u32 + tin_px[1] as u32 + tin_px[2] as u32;
        let pkg_lum = pkg_px[0] as u32 + pkg_px[1] as u32 + pkg_px[2] as u32;
        assert!(tin_lum > pkg_lum);
    }
}
