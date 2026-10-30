//! Back-drill: a second, larger drill pass that removes the unused barrel of
//! a via below (or above) the last layer the signal uses.
//!
//! A via may carry a `BackDrill`, and a net class may carry a default one.
//! Nothing derived is saved: the drilled-out layers, the remaining stub and
//! the conducting layers are computed from the spec, the via span and the
//! stackup records.
//!
//! # Rules
//! - `from` names the face the drill enters. The drill removes barrel copper on every span layer between that face and the must-not-cut layer, and never touches the must-not-cut layer or anything past it.
//! - The stub is the stackup distance from the must-not-cut layer to the nearest drilled-out layer, in millimetres. It is unknown when the stackup carries no such rows.
//! - A via with a hole of zero diameter cannot be back-drilled.
//! - A class default applies to a member via only when it is valid for that via's span; otherwise the via is not back-drilled.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` for `Via`, `Layer`, `NetClass`, `Project::via_span`, `Project::board.layer_records`.
//!
//! # Do not
//! - Store the stub or the drilled layers.
//! - Treat a back-drilled layer as conducting; connectivity reads `Project::via_conducting_layers`.

use crate::{Layer, Project, Via};
use serde::{Deserialize, Serialize};

/// Largest back-drill diameter or stub the model accepts, in millimetres.
pub const BACKDRILL_MAX_MM: f64 = 20.0;

/// The face a back-drill enters from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackDrillFrom {
    /// Drilled from the top face downwards.
    Top,
    /// Drilled from the bottom face upwards.
    Bottom,
}

impl BackDrillFrom {
    /// The word used in files, commands and the drill attribute text.
    pub fn label(self) -> &'static str {
        match self {
            BackDrillFrom::Top => "top",
            BackDrillFrom::Bottom => "bottom",
        }
    }
}

/// A back-drill setting for one via or one net class.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BackDrill {
    /// Face the drill enters from.
    pub from: BackDrillFrom,
    /// Copper layer the drill must not cut.
    pub must_not_cut: Layer,
    /// Longest stub allowed to remain, in millimetres.
    pub max_stub: f64,
    /// Back-drill diameter in millimetres; larger than the via drill.
    pub diameter: f64,
}

impl BackDrill {
    /// Check the two sizes alone, without a via.
    ///
    /// # Rules
    /// - Diameter above 0, stub from 0, both finite and at most `BACKDRILL_MAX_MM`.
    pub fn validate_sizes(&self) -> Result<(), String> {
        let spec = self;
        if !spec.diameter.is_finite() || spec.diameter <= 0.0 || spec.diameter > BACKDRILL_MAX_MM {
            return Err(format!(
                "back-drill diameter must be above 0 and at most {BACKDRILL_MAX_MM} mm"
            ));
        }
        if !spec.max_stub.is_finite() || spec.max_stub < 0.0 || spec.max_stub > BACKDRILL_MAX_MM {
            return Err(format!(
                "maximum stub must be between 0 and {BACKDRILL_MAX_MM} mm"
            ));
        }
        Ok(())
    }
}

impl Project {
    /// The back-drill that applies to a via: its own, else its net class
    /// default when that default is valid for this via's span.
    ///
    /// # Rules
    /// - A via's own setting wins and is returned as saved, even if the span later made it invalid; the rules crate reports that.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs `NetClass::backdrill`, `Project::validate_backdrill`.
    ///
    /// # Do not
    /// - Return a class default that does not fit the span.
    pub fn effective_backdrill(&self, via: &Via) -> Option<BackDrill> {
        if via.backdrill.is_some() {
            return via.backdrill;
        }
        let class = self
            .net_classes
            .iter()
            .find(|c| c.nets.contains(&via.net))?;
        let spec = class.backdrill?;
        self.validate_backdrill(via, &spec).ok().map(|()| spec)
    }

    /// Check a back-drill against a via: the must-not-cut layer lies inside
    /// the via's span and has at least one span layer between it and the
    /// entry face, the via has a hole, and the diameter exceeds the hole.
    ///
    /// # Rules
    /// - Sizes must be finite, positive (stub may be zero) and at most `BACKDRILL_MAX_MM`.
    /// - From the top the must-not-cut layer is below the span's upper end; from the bottom it is above the span's lower end.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs `Project::via_span`.
    ///
    /// # Do not
    /// - Check the stub against the stackup here; that is a design rule finding.
    pub fn validate_backdrill(&self, via: &Via, spec: &BackDrill) -> Result<(), String> {
        spec.validate_sizes()?;
        if via.drill <= 0.0 {
            return Err("a via without a drilled hole cannot be back-drilled".into());
        }
        if spec.diameter <= via.drill {
            return Err("the back-drill must be larger than the via drill".into());
        }
        let (upper, lower) = self.via_span(via);
        let m = spec.must_not_cut.index();
        if m < upper.index() || m > lower.index() {
            return Err("the must-not-cut layer is outside the via span".into());
        }
        let ok = match spec.from {
            BackDrillFrom::Top => m > upper.index(),
            BackDrillFrom::Bottom => m < lower.index(),
        };
        if !ok {
            return Err(
                "the must-not-cut layer must lie below the start layer, so the drill removes at least one layer"
                    .into(),
            );
        }
        Ok(())
    }

    /// Span layers a back-drill removes, top to bottom; empty when the spec
    /// does not fit the via.
    pub fn backdrilled_layers(&self, via: &Via, spec: &BackDrill) -> Vec<Layer> {
        if self.validate_backdrill(via, spec).is_err() {
            return vec![];
        }
        let m = spec.must_not_cut.index();
        self.via_span_layers(via)
            .into_iter()
            .filter(|l| match spec.from {
                BackDrillFrom::Top => l.index() < m,
                BackDrillFrom::Bottom => l.index() > m,
            })
            .collect()
    }

    /// Span layers a via conducts on once its back-drill is taken out. Equal
    /// to `via_span_layers` for a via with no back-drill.
    ///
    /// # Dependencies
    /// - crates/doc/src/backdrill.rs `Project::effective_backdrill`.
    ///
    /// # Do not
    /// - Use `via_span_layers` for connectivity; it still counts drilled-out layers.
    pub fn via_conducting_layers(&self, via: &Via) -> Vec<Layer> {
        let all = self.via_span_layers(via);
        let Some(spec) = self.effective_backdrill(via) else {
            return all;
        };
        let gone = self.backdrilled_layers(via, &spec);
        all.into_iter().filter(|l| !gone.contains(l)).collect()
    }

    /// The stub a back-drill leaves, in millimetres: the stackup distance
    /// from the must-not-cut layer to the nearest drilled-out layer. None when
    /// the spec does not fit the via or the stackup has no rows for it.
    pub fn backdrill_stub(&self, via: &Via, spec: &BackDrill) -> Option<f64> {
        let gone = self.backdrilled_layers(via, spec);
        let nearest = match spec.from {
            BackDrillFrom::Top => gone.last(),
            BackDrillFrom::Bottom => gone.first(),
        }?;
        let (a, b) = if nearest.index() < spec.must_not_cut.index() {
            (*nearest, spec.must_not_cut)
        } else {
            (spec.must_not_cut, *nearest)
        };
        self.layer_distance(a, b)
    }

    /// Distance in millimetres from the upper to the lower copper layer, from
    /// the stackup records; none when the stackup has no such rows.
    pub fn layer_distance(&self, upper: Layer, lower: Layer) -> Option<f64> {
        let mut copper = 0usize;
        let mut inside = false;
        let mut depth = 0.0;
        for rec in &self.board.layer_records {
            if rec.kind == "copper" {
                if inside && copper == lower.index() as usize {
                    return Some(depth);
                }
                if copper == upper.index() as usize {
                    inside = true;
                }
                copper += 1;
            }
            if inside {
                depth += rec.thickness;
            }
        }
        None
    }
}
