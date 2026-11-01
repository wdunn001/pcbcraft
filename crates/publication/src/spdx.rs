//! SPDX allowlist and lineage compatibility for publication licences.

/// SPDX identifiers publishers may choose for each layer.
pub const ALLOWED_SPDX: &[&str] = &[
    "CC0-1.0",
    "CC-BY-4.0",
    "CC-BY-SA-4.0",
    "CC-BY-SA-3.0",
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "CERN-OHL-P-2.0",
    "CERN-OHL-S-2.0",
    "CERN-OHL-W-2.0",
    "TAPR-OHL-1.0",
];

/// Returns whether `id` is on the publication allowlist.
pub fn spdx_allowed(id: &str) -> bool {
    ALLOWED_SPDX.contains(&id.trim())
}

/// Classifies an SPDX id for compatibility checks between parent and child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LicenceClass {
    Permissive,
    ShareAlike,
    Copyleft,
}

fn classify(id: &str) -> Option<LicenceClass> {
    match id {
        "CC0-1.0" | "MIT" | "Apache-2.0" | "BSD-2-Clause" | "BSD-3-Clause" | "ISC"
        | "CERN-OHL-P-2.0" => Some(LicenceClass::Permissive),
        "CC-BY-4.0" | "CC-BY-SA-3.0" => Some(LicenceClass::ShareAlike),
        "CC-BY-SA-4.0" | "GPL-3.0-only" | "GPL-3.0-or-later" | "LGPL-3.0-only"
        | "LGPL-3.0-or-later" | "CERN-OHL-S-2.0" | "CERN-OHL-W-2.0" | "TAPR-OHL-1.0" => {
            Some(LicenceClass::Copyleft)
        }
        _ => None,
    }
}

/// Whether a child publication may declare `child_layers` when derived from a
/// parent that used `parent_licence` on the relevant layer.
pub fn lineage_compatible(parent_licence: &str, child_spdx: &str) -> bool {
    let Some(parent) = classify(parent_licence) else {
        return false;
    };
    let Some(child) = classify(child_spdx) else {
        return false;
    };
    match parent {
        LicenceClass::Permissive => true,
        LicenceClass::ShareAlike => {
            child == LicenceClass::ShareAlike || child == LicenceClass::Copyleft
        }
        LicenceClass::Copyleft => child == LicenceClass::Copyleft,
    }
}
