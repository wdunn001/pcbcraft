use crate::part_types::{Model3dRef, Part, Verification, VerificationChecks};
use crate::{FootprintDefinition, SymbolDefinition, TriangleMesh};
use std::collections::BTreeSet;

pub struct VerifyContext<'a> {
    pub symbol: Option<&'a SymbolDefinition>,
    pub footprint: Option<&'a FootprintDefinition>,
}

pub fn verify_part(part: &Part, ctx: VerifyContext<'_>) -> (Verification, Vec<String>) {
    let mut messages = Vec::new();
    let mut checks = VerificationChecks::default();
    checks.symbol_pads_match = check_symbol_pads(part, ctx.symbol, ctx.footprint, &mut messages);
    checks.package_key_match =
        check_package_key(part, ctx.footprint, &mut messages);
    checks.model_fits = check_model_fits(part, ctx.footprint, &mut messages);
    checks.datasheet_ok = check_datasheet(part, &mut messages);
    checks.pin_kinds_present = check_pin_kinds(part, &mut messages);
    checks.mpn_confirmed = check_mpn(part, &mut messages);
    let verified = checks.symbol_pads_match
        && checks.package_key_match
        && checks.model_fits
        && checks.datasheet_ok
        && checks.pin_kinds_present
        && checks.mpn_confirmed;
    let verification = Verification {
        verified,
        checked_at: iso_now(),
        checks,
    };
    (verification, messages)
}

fn iso_now() -> String {
    // Stable placeholder: callers may overwrite after fetch; no wall-clock dependency in tests.
    String::new()
}

fn check_symbol_pads(
    part: &Part,
    symbol: Option<&SymbolDefinition>,
    footprint: Option<&FootprintDefinition>,
    messages: &mut Vec<String>,
) -> bool {
    let Some(symbol) = symbol else {
        messages.push("symbol definition is missing from the project library".into());
        return false;
    };
    let Some(footprint) = footprint else {
        messages.push("footprint definition is missing from the project library".into());
        return false;
    };
    let pin_numbers: BTreeSet<&str> = symbol.pins.iter().map(|p| p.number.as_str()).collect();
    let pad_numbers: BTreeSet<&str> = footprint.pads.iter().map(|p| p.number.as_str()).collect();
    let mut ok = true;
    for spec in &part.pins {
        if !pin_numbers.contains(spec.number.as_str()) {
            messages.push(format!(
                "pin {} is listed on the part but not on symbol {}",
                spec.number, part.symbol
            ));
            ok = false;
        }
        for pad in &spec.pads {
            if !pad_numbers.contains(pad.as_str()) {
                messages.push(format!(
                    "pin {} claims pad {} which is not on footprint {}",
                    spec.number, pad, part.footprint
                ));
                ok = false;
            }
        }
    }
    if part.pins.is_empty() {
        for pin in &symbol.pins {
            if !pad_numbers.contains(pin.number.as_str()) {
                messages.push(format!(
                    "symbol pin {} has no matching footprint pad",
                    pin.number
                ));
                ok = false;
            }
        }
    }
    ok
}

fn check_package_key(
    part: &Part,
    footprint: Option<&FootprintDefinition>,
    messages: &mut Vec<String>,
) -> bool {
    let Some(fp) = footprint else {
        return false;
    };
    match crate::PackageKey::from_footprint(fp) {
        Ok(computed) => {
            if part.package_key.matches(&computed) {
                true
            } else {
                messages.push(
                    "footprint package key does not match the part record".into(),
                );
                false
            }
        }
        Err(e) => {
            messages.push(e);
            false
        }
    }
}

fn check_model_fits(
    part: &Part,
    footprint: Option<&FootprintDefinition>,
    messages: &mut Vec<String>,
) -> bool {
    let mesh = match &part.model3d {
        Some(Model3dRef::Parametric { body }) => body.mesh(),
        Some(Model3dRef::Wrl { path, scale, .. }) => {
            match crate::read_wrl_file(path, *scale) {
                Ok(m) => m,
                Err(e) => {
                    messages.push(e);
                    return false;
                }
            }
        }
        Some(Model3dRef::Gltf { path, .. }) => {
            messages.push(format!(
                "glTF model {} must be checked through the io loader",
                path
            ));
            return false;
        }
        Some(Model3dRef::Step { .. }) => {
            messages.push("STEP bodies are not measured yet; use a mesh model or parametric body".into());
            return false;
        }
        None => {
            messages.push("part has no 3D body".into());
            return false;
        }
    };
    mesh_fits_courtyard(&mesh, footprint, part, messages)
}

fn mesh_fits_courtyard(
    mesh: &TriangleMesh,
    footprint: Option<&FootprintDefinition>,
    part: &Part,
    messages: &mut Vec<String>,
) -> bool {
    let Some((min, max)) = mesh.bounds() else {
        messages.push("3D body has no geometry".into());
        return false;
    };
    let (cx, cy) = (
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
    );
    let (hx, hy) = ((max[0] - min[0]) / 2.0, (max[1] - min[1]) / 2.0);
    let (limit_x, limit_y) = if let Some(hx) = part.package_key.courtyard_half_x {
        (hx, part.package_key.courtyard_half_y.unwrap_or(hx))
    } else if let Some(fp) = footprint {
        match crate::PackageKey::from_footprint(fp) {
            Ok(k) => (
                k.courtyard_half_x.unwrap_or(k.body_length_mm),
                k.courtyard_half_y.unwrap_or(k.body_width_mm),
            ),
            Err(_) => (part.package_key.body_length_mm, part.package_key.body_width_mm),
        }
    } else {
        (part.package_key.body_length_mm, part.package_key.body_width_mm)
    };
    if hx + cx.abs() > limit_x + 0.2 || hy + cy.abs() > limit_y + 0.2 {
        messages.push("3D body extends beyond the footprint courtyard".into());
        return false;
    }
    true
}

fn check_datasheet(part: &Part, messages: &mut Vec<String>) -> bool {
    let Some(ds) = &part.datasheet else {
        messages.push("part has no datasheet URL".into());
        return false;
    };
    if ds.url.is_empty() {
        messages.push("datasheet URL is empty".into());
        return false;
    }
    if !ds.url.starts_with("http://") && !ds.url.starts_with("https://") {
        messages.push("datasheet URL must be http or https".into());
        return false;
    }
    if !ds.sha256.is_empty() && ds.sha256.len() != 64 {
        messages.push("datasheet sha256 must be 64 hex characters when set".into());
        return false;
    }
    true
}

fn check_pin_kinds(part: &Part, messages: &mut Vec<String>) -> bool {
    if part.pins.is_empty() {
        messages.push("part has no pin specifications".into());
        return false;
    }
    let mut ok = true;
    for p in &part.pins {
        if p.number.is_empty() {
            messages.push("a pin spec is missing its number".into());
            ok = false;
        }
    }
    ok
}

fn check_mpn(part: &Part, messages: &mut Vec<String>) -> bool {
    use crate::part_types::MpnSource;
    match part.mpn_source {
        MpnSource::UserEntered => {
            messages.push("MPN is user entered and not confirmed by a source".into());
            false
        }
        _ => true,
    }
}
