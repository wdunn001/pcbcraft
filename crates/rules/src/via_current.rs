//! Via current capacity estimate and finding, using the IPC-2221 internal
//! copper constant for the barrel plating annulus.

use pcbcraft_calc::copper::{self, ViaCurrent};
use pcbcraft_doc::{Project, Via};

/// Advice for the current a via can carry.
#[derive(Clone, Debug, PartialEq)]
pub struct ViaCurrentAdvice {
    /// Current one via can carry at the net temperature rise, amperes.
    pub current: f64,
    /// Via barrels needed for the net current, when above one.
    pub count: Option<f64>,
    /// Method used: always `ipc2221`.
    pub method: &'static str,
}

/// Estimated current a via barrel can carry.
///
/// Uses the IPC-2221 internal copper constant, the usual convention for
/// through-hole vias. The barrel copper is the plating annulus around the
/// drilled hole. Returns none when the barrel area is invalid.
pub fn via_capacity(project: &Project, via: &Via) -> Option<ViaCurrentAdvice> {
    let net = via.net;
    let temperature_rise = project.effective_temperature_rise(net).unwrap_or(10.0);
    let target = project.effective_current(net);
    let out = copper::via_current_resistance(ViaCurrent {
        drill: via.drill,
        temperature_rise,
        target_current: target,
        ..Default::default()
    })
    .ok()?;
    Some(ViaCurrentAdvice {
        current: out.current,
        count: target
            .filter(|t| *t > out.current)
            .map(|t| (t / out.current).ceil()),
        method: "ipc2221",
    })
}
