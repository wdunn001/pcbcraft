//! Via protection by face: the twelve protection types of the Gerber
//! specification, derived from what each outer face of a via is given.
//!
//! A via keeps one `ViaCovering` for the top face and may carry a second for
//! the bottom face. Nothing here is saved: the type is computed from those
//! two coverings and from which faces the via's span reaches.
//!
//! # Rules
//! - The type names follow the Gerber specification: Ia, Ib, IIa, IIb, IIIa, IIIb, IVa, IVb, V, VI, VII and None.
//! - The `a` forms protect one face, the `b` forms protect both. Types V, VI and VII treat the whole hole, so they have no `a` or `b` form.
//! - A via that reaches no outer face has nothing to protect and reads as None.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` for `ViaCovering`, `Via`, `Project` and `Board`.
//!
//! # Do not
//! - Store the type; store the coverings and derive the type.
//! - Rename a token: drill files and saved files depend on them.

use crate::{Project, Via, ViaCovering};

/// The protection type written on a drill tool: the twelve values of the
/// Gerber specification for a via hole.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ViaProtectionType {
    /// Tented on one face.
    Ia,
    /// Tented on both faces.
    Ib,
    /// Tented and covered on one face.
    IIa,
    /// Tented and covered on both faces.
    IIb,
    /// Plugged on one face.
    IIIa,
    /// Plugged on both faces.
    IIIb,
    /// Plugged and covered on one face.
    IVa,
    /// Plugged and covered on both faces.
    IVb,
    /// Filled.
    V,
    /// Filled and covered.
    VI,
    /// Filled and capped.
    VII,
    /// No protection.
    None,
}

impl ViaProtectionType {
    /// Every type, in the order the specification lists them.
    pub const ALL: [ViaProtectionType; 12] = [
        ViaProtectionType::Ia,
        ViaProtectionType::Ib,
        ViaProtectionType::IIa,
        ViaProtectionType::IIb,
        ViaProtectionType::IIIa,
        ViaProtectionType::IIIb,
        ViaProtectionType::IVa,
        ViaProtectionType::IVb,
        ViaProtectionType::V,
        ViaProtectionType::VI,
        ViaProtectionType::VII,
        ViaProtectionType::None,
    ];

    /// The value written after `ViaDrill,` in a drill file, for example `Ib`.
    pub fn gerber_token(self) -> &'static str {
        match self {
            ViaProtectionType::Ia => "Ia",
            ViaProtectionType::Ib => "Ib",
            ViaProtectionType::IIa => "IIa",
            ViaProtectionType::IIb => "IIb",
            ViaProtectionType::IIIa => "IIIa",
            ViaProtectionType::IIIb => "IIIb",
            ViaProtectionType::IVa => "IVa",
            ViaProtectionType::IVb => "IVb",
            ViaProtectionType::V => "V",
            ViaProtectionType::VI => "VI",
            ViaProtectionType::VII => "VII",
            ViaProtectionType::None => "None",
        }
    }
}

/// The type for a top and a bottom covering when both faces are reached.
///
/// # Rules
/// - Equal coverings give the `b` form (or the whole-hole type).
/// - One open face and one protected face give the `a` form of the protected one.
/// - Two different protections give the greater one in `ViaCovering` order, in its `b` form.
///
/// # Dependencies
/// - `crates/doc/src/lib.rs` `ViaCovering`.
///
/// # Do not
/// - Call this for a via that reaches one face; use `protection_for_faces`.
pub fn protection_type(top: ViaCovering, bottom: ViaCovering) -> ViaProtectionType {
    let (strong, one_sided) = if top >= bottom {
        (top, bottom == ViaCovering::Open)
    } else {
        (bottom, top == ViaCovering::Open)
    };
    form(strong, one_sided)
}

fn form(covering: ViaCovering, one_sided: bool) -> ViaProtectionType {
    use ViaProtectionType as T;
    match (covering, one_sided) {
        (ViaCovering::Open, _) => T::None,
        (ViaCovering::Tented, true) => T::Ia,
        (ViaCovering::Tented, false) => T::Ib,
        (ViaCovering::TentedCovered, true) => T::IIa,
        (ViaCovering::TentedCovered, false) => T::IIb,
        (ViaCovering::Plugged, true) => T::IIIa,
        (ViaCovering::Plugged, false) => T::IIIb,
        (ViaCovering::PluggedCovered, true) => T::IVa,
        (ViaCovering::PluggedCovered, false) => T::IVb,
        (ViaCovering::Filled, _) => T::V,
        (ViaCovering::FilledCovered, _) => T::VI,
        (ViaCovering::FilledCapped, _) => T::VII,
    }
}

/// The type for a via that reaches the given outer faces.
///
/// A via that reaches only one face is protected on that face alone, so its
/// covering gives the `a` form. One that reaches neither is None.
///
/// # Rules
/// - `top` and `bottom` are the coverings of those faces; a face the via does not reach is ignored.
///
/// # Dependencies
/// - crates/doc/src/via_protection.rs `protection_type`.
///
/// # Do not
/// - Pass a covering for a face the via does not reach and expect it to count.
pub fn protection_for_faces(
    reaches_top: bool,
    reaches_bottom: bool,
    top: ViaCovering,
    bottom: ViaCovering,
) -> ViaProtectionType {
    match (reaches_top, reaches_bottom) {
        (true, true) => protection_type(top, bottom),
        (true, false) => form(top, true),
        (false, true) => form(bottom, true),
        (false, false) => ViaProtectionType::None,
    }
}

impl Project {
    /// The protection type of one via, from its two face coverings and the
    /// faces its span reaches.
    ///
    /// # Rules
    /// - Reads the via's own coverings, else the board default.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs `Project::via_span`, `Project::copper_layer_count`.
    ///
    /// # Do not
    /// - Cache the result; the span can change under it.
    pub fn via_protection_type(&self, via: &Via) -> ViaProtectionType {
        let (upper, lower) = self.via_span(via);
        let last = self.copper_layer_count().saturating_sub(1);
        protection_for_faces(
            upper.index() == 0,
            lower.index() >= last,
            via.effective_covering(&self.board),
            via.effective_covering_bottom(&self.board),
        )
    }
}
