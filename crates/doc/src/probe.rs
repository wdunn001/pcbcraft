//! Virtual probes: points on the schematic where a person wants to read a simulated voltage.
//!
//! A probe sits on one net or one pin and reads the voltage there, measured against a
//! reference net (ground when none is named). Nothing derived is saved: the reading comes
//! from the simulator each time it is asked for.
//!
//! # Rules
//! - A probe names exactly one point: a net or a pin, never both and never neither.
//! - A design holds at most `MAX_PROBES` probes.
//! - The net, the pin and the reference net must exist; a probe that loses one of them is
//!   dropped by `Project::prune_probes`.
//! - A label, when set, is 1 to `MAX_PROBE_LABEL` bytes with no control characters.
//! - An expected range, when set, has a minimum, a maximum or both, each finite and within
//!   plus or minus `MAX_PROBE_VOLTS`, and the minimum is not above the maximum.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` for `Project`, `PinRef`, `Id`.
//!
//! # Do not
//! - Store a measured value here; the expected range is what a person wants, not what was read.
//! - Name a real instrument or a serial port; these probes are virtual.

use crate::{Id, PinRef, Project};
use serde::{Deserialize, Serialize};

/// Most probes one design may hold.
pub const MAX_PROBES: usize = 64;

/// Longest probe label, in bytes.
pub const MAX_PROBE_LABEL: usize = 40;

/// Largest voltage, either sign, an expected range may name.
pub const MAX_PROBE_VOLTS: f64 = 1000.0;

/// The voltage range a probe should read: a floor, a ceiling or both, in volts.
///
/// # Rules
/// - At least one bound is present, each is finite and within plus or minus `MAX_PROBE_VOLTS`.
/// - The minimum is not above the maximum.
/// - A reading on either bound is inside the range.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProbeExpect {
    /// The lowest acceptable voltage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// The highest acceptable voltage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// A voltage in plain words without the unit: up to three decimals, trailing zeros dropped.
pub fn plain_volts(volts: f64) -> String {
    let volts = if volts.abs() < 0.0005 { 0.0 } else { volts };
    let text = format!("{volts:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn round9(v: f64) -> f64 {
    (v * 1e9).round() / 1e9
}

impl ProbeExpect {
    /// Check the bounds.
    pub fn validate(&self) -> Result<(), String> {
        if self.min.is_none() && self.max.is_none() {
            return Err("an expected range needs a minimum, a maximum or both".into());
        }
        for (what, bound) in [("minimum", self.min), ("maximum", self.max)] {
            if let Some(v) = bound
                && (!v.is_finite() || v.abs() > MAX_PROBE_VOLTS)
            {
                return Err(format!(
                    "the expected {what} must be a number from -{MAX_PROBE_VOLTS} to {MAX_PROBE_VOLTS} volts"
                ));
            }
        }
        if let (Some(lo), Some(hi)) = (self.min, self.max)
            && lo > hi
        {
            return Err("the expected minimum is above the expected maximum".into());
        }
        Ok(())
    }

    /// The range around `target` volts that is `tolerance_percent` wide on each side.
    ///
    /// # Rules
    /// - The tolerance is 0 to 100 percent of the size of the target.
    /// - The bounds are rounded to nine decimals so 5 V at 5 percent is exactly 4.75 to 5.25.
    pub fn from_target(target: f64, tolerance_percent: f64) -> Result<Self, String> {
        if !target.is_finite() || target.abs() > MAX_PROBE_VOLTS {
            return Err(format!(
                "the target must be a number from -{MAX_PROBE_VOLTS} to {MAX_PROBE_VOLTS} volts"
            ));
        }
        if !tolerance_percent.is_finite() || !(0.0..=100.0).contains(&tolerance_percent) {
            return Err("the tolerance must be a percentage from 0 to 100".into());
        }
        let span = target.abs() * tolerance_percent / 100.0;
        let range = Self {
            min: Some(round9(target - span)),
            max: Some(round9(target + span)),
        };
        range.validate()?;
        Ok(range)
    }

    /// True when `volts` is inside the range, bounds included.
    pub fn contains(&self, volts: f64) -> bool {
        self.min.is_none_or(|lo| volts >= lo) && self.max.is_none_or(|hi| volts <= hi)
    }

    /// The range in words: `4.75 to 5.25 V`, `at least 4.75 V` or `at most 5.25 V`.
    pub fn range_text(&self) -> String {
        match (self.min, self.max) {
            (Some(lo), Some(hi)) => format!("{} to {} V", plain_volts(lo), plain_volts(hi)),
            (Some(lo), None) => format!("at least {} V", plain_volts(lo)),
            (None, Some(hi)) => format!("at most {} V", plain_volts(hi)),
            (None, None) => String::new(),
        }
    }
}

/// One virtual probe: a point of the schematic and the net it is measured against.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    /// Stable id, shared with every other object of the design.
    pub id: Id,
    /// The probed net, when the probe sits on a net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Id>,
    /// The probed pin, when the probe sits on a pin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<PinRef>,
    /// The net the voltage is measured against; ground when none is named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Id>,
    /// A short name shown beside the probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The voltage this probe should read; the design reports a finding when it reads outside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<ProbeExpect>,
}

impl Probe {
    /// Check the probe against the design it lives in.
    ///
    /// # Rules
    /// - Exactly one of `net` and `pin` is set, and it exists.
    /// - The reference net exists and differs from the probed net.
    /// - The label is 1 to `MAX_PROBE_LABEL` bytes without control characters.
    /// - The expected range, when set, is valid (see `ProbeExpect::validate`).
    pub fn validate(&self, project: &Project) -> Result<(), String> {
        match (&self.net, &self.pin) {
            (Some(_), Some(_)) => return Err("a probe sits on a net or a pin, not both".into()),
            (None, None) => return Err("a probe needs a net or a pin".into()),
            _ => {}
        }
        if !self.point_exists(project) {
            return Err("a probe names a net or pin that is not in the design".into());
        }
        if let Some(reference) = self.reference {
            if !project.nets.iter().any(|n| n.id == reference) {
                return Err("a probe names a reference net that is not in the design".into());
            }
            if self.net == Some(reference) {
                return Err("a probe cannot be measured against its own net".into());
            }
        }
        if let Some(label) = &self.label
            && (label.is_empty()
                || label.len() > MAX_PROBE_LABEL
                || label.chars().any(char::is_control))
        {
            return Err(format!(
                "a probe label is 1 to {MAX_PROBE_LABEL} bytes of plain text"
            ));
        }
        if let Some(expect) = &self.expect {
            expect.validate()?;
        }
        Ok(())
    }

    /// True when the net or pin the probe sits on, and its reference net, are all present.
    pub fn is_intact(&self, project: &Project) -> bool {
        self.point_exists(project)
            && self
                .reference
                .is_none_or(|r| project.nets.iter().any(|n| n.id == r))
    }

    /// The net the probe reads: its own net, or the net its pin is on.
    pub fn probed_net(&self, project: &Project) -> Option<Id> {
        match (&self.net, &self.pin) {
            (Some(net), _) => Some(*net),
            (None, Some(pin)) => project.pin(pin).and_then(|p| p.net),
            (None, None) => None,
        }
    }

    fn point_exists(&self, project: &Project) -> bool {
        match (&self.net, &self.pin) {
            (Some(net), None) => project.nets.iter().any(|n| n.id == *net),
            (None, Some(pin)) => project.pin(pin).is_some(),
            _ => false,
        }
    }
}

impl Project {
    /// Check every probe and the probe count.
    pub fn validate_probes(&self) -> Result<(), String> {
        if self.probes.len() > MAX_PROBES {
            return Err(format!("a design holds at most {MAX_PROBES} probes"));
        }
        for probe in &self.probes {
            probe.validate(self)?;
        }
        Ok(())
    }

    /// Drop probes whose net, pin or reference net no longer exists.
    ///
    /// Runs after every engine command so a delete never leaves a dangling probe.
    pub fn prune_probes(&mut self) {
        if self.probes.is_empty() {
            return;
        }
        let probes = std::mem::take(&mut self.probes);
        self.probes = probes.into_iter().filter(|p| p.is_intact(self)).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Net;

    fn with_net(id: Id) -> Project {
        let mut p = Project::default();
        p.nets.push(Net {
            id,
            name: "MID".into(),
            ..Net::default()
        });
        p
    }

    fn on_net(id: Id, net: Id) -> Probe {
        Probe {
            id,
            net: Some(net),
            pin: None,
            reference: None,
            label: None,
            expect: None,
        }
    }

    #[test]
    fn a_probe_on_a_known_net_is_valid_and_one_on_a_missing_net_is_not() {
        let p = with_net(3);
        assert!(on_net(9, 3).validate(&p).is_ok());
        assert!(on_net(9, 4).validate(&p).is_err());
    }

    #[test]
    fn a_probe_needs_exactly_one_point() {
        let p = with_net(3);
        let mut both = on_net(9, 3);
        both.pin = Some(PinRef {
            component: 1,
            pin: "1".into(),
            unit: None,
        });
        assert!(both.validate(&p).is_err());
        let mut neither = on_net(9, 3);
        neither.net = None;
        assert!(neither.validate(&p).is_err());
    }

    #[test]
    fn a_probe_cannot_measure_a_net_against_itself() {
        let p = with_net(3);
        let mut probe = on_net(9, 3);
        probe.reference = Some(3);
        assert!(probe.validate(&p).is_err());
    }

    #[test]
    fn labels_are_bounded_plain_text() {
        let p = with_net(3);
        let bads = vec![
            String::new(),
            "x".repeat(MAX_PROBE_LABEL + 1),
            "a\nb".to_string(),
        ];
        for bad in bads {
            let mut probe = on_net(9, 3);
            probe.label = Some(bad.clone());
            assert!(probe.validate(&p).is_err(), "label {bad:?} must be refused");
        }
        let mut ok = on_net(9, 3);
        ok.label = Some("P1".into());
        assert!(ok.validate(&p).is_ok());
    }

    #[test]
    fn probes_travel_with_the_design_and_old_designs_load_without_them() -> Result<(), String> {
        let mut p = with_net(3);
        p.probes.push(on_net(9, 3));
        let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
        let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert_eq!(back.probes, p.probes, "the probe travels");
        let plain = serde_json::to_string(&Project::default()).map_err(|e| e.to_string())?;
        assert!(
            !plain.contains("\"probes\""),
            "a design with no probes writes no probes field"
        );
        let old: Project = serde_json::from_str(&plain).map_err(|e| e.to_string())?;
        assert!(old.probes.is_empty(), "designs saved before probes load");
        Ok(())
    }

    #[test]
    fn the_count_is_bounded_and_a_lost_net_drops_its_probe() {
        let mut p = with_net(3);
        for n in 0..=MAX_PROBES as u64 {
            p.probes.push(on_net(100 + n, 3));
        }
        assert!(p.validate_probes().is_err(), "65 probes are too many");
        p.probes.truncate(2);
        p.nets.clear();
        p.prune_probes();
        assert!(p.probes.is_empty(), "no net, no probe");
    }

    #[test]
    fn expected_ranges_are_bounded_and_ordered() {
        let p = with_net(3);
        let bads = [
            ProbeExpect {
                min: None,
                max: None,
            },
            ProbeExpect {
                min: Some(5.0),
                max: Some(4.0),
            },
            ProbeExpect {
                min: Some(f64::NAN),
                max: None,
            },
            ProbeExpect {
                min: None,
                max: Some(f64::INFINITY),
            },
            ProbeExpect {
                min: Some(-1000.1),
                max: None,
            },
            ProbeExpect {
                min: None,
                max: Some(1000.1),
            },
        ];
        for bad in bads {
            let mut probe = on_net(9, 3);
            probe.expect = Some(bad);
            assert!(probe.validate(&p).is_err(), "{bad:?} must be refused");
        }
        for good in [
            ProbeExpect {
                min: Some(4.9),
                max: Some(5.1),
            },
            ProbeExpect {
                min: Some(-1000.0),
                max: None,
            },
            ProbeExpect {
                min: None,
                max: Some(1000.0),
            },
            ProbeExpect {
                min: Some(5.0),
                max: Some(5.0),
            },
        ] {
            let mut probe = on_net(9, 3);
            probe.expect = Some(good);
            assert!(probe.validate(&p).is_ok(), "{good:?} must be accepted");
        }
    }

    #[test]
    fn a_target_with_a_tolerance_becomes_a_minimum_and_a_maximum() -> Result<(), String> {
        let range = ProbeExpect::from_target(5.0, 5.0)?;
        assert_eq!(range.min, Some(4.75));
        assert_eq!(range.max, Some(5.25));
        let negative = ProbeExpect::from_target(-12.0, 10.0)?;
        assert_eq!(negative.min, Some(-13.2));
        assert_eq!(negative.max, Some(-10.8));
        assert!(ProbeExpect::from_target(5.0, -1.0).is_err());
        assert!(ProbeExpect::from_target(5.0, 100.5).is_err());
        assert!(ProbeExpect::from_target(f64::NAN, 5.0).is_err());
        assert!(
            ProbeExpect::from_target(1000.0, 5.0).is_err(),
            "the top would pass 1000 V"
        );
        Ok(())
    }

    #[test]
    fn a_reading_on_a_bound_is_inside_and_the_range_reads_in_words() {
        let both = ProbeExpect {
            min: Some(4.75),
            max: Some(5.25),
        };
        assert!(both.contains(4.75) && both.contains(5.25) && both.contains(5.0));
        assert!(!both.contains(4.7499) && !both.contains(5.2501));
        assert_eq!(both.range_text(), "4.75 to 5.25 V");
        let floor = ProbeExpect {
            min: Some(3.0),
            max: None,
        };
        assert!(floor.contains(1000.0) && !floor.contains(2.9));
        assert_eq!(floor.range_text(), "at least 3 V");
        let ceiling = ProbeExpect {
            min: None,
            max: Some(-1.5),
        };
        assert_eq!(ceiling.range_text(), "at most -1.5 V");
    }

    #[test]
    fn the_expected_range_travels_and_old_probes_load_without_one() -> Result<(), String> {
        let mut p = with_net(3);
        let mut probe = on_net(9, 3);
        probe.expect = Some(ProbeExpect {
            min: Some(4.9),
            max: None,
        });
        p.probes.push(probe);
        p.probes.push(on_net(10, 3));
        let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
        assert_eq!(
            text.matches("\"expect\"").count(),
            1,
            "only the probe with a range writes it"
        );
        assert!(!text.contains("\"max\""), "an absent bound is not written");
        let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert_eq!(back.probes, p.probes);
        let old = r#"{"id":9,"net":3}"#;
        let loaded: Probe = serde_json::from_str(old).map_err(|e| e.to_string())?;
        assert!(
            loaded.expect.is_none(),
            "a probe saved before ranges loads unchanged"
        );
        Ok(())
    }
}
