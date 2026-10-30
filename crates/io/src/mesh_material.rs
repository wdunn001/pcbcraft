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
}
