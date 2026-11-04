//! Suggested track width that would meet a net's impedance target.
//!
//! Uses the same closed-form models as the impedance-target check, but
//! inverts them: given a target ohms and the stackup, find the width that
//! produces that impedance. For differential pairs, the pair gap is not
//! adjustable by this function; return None and let the pair check handle it.
//!
//! # Rules
//! - Returns None when the track has no impedance target, no stackup sample,
//!   or belongs to a differential pair (pair legs need a gap change).
//! - Uses width_for_ohms from pcbcraft_geom::impedance for the bisection.
//!
//! # Dependencies
//! - crates/geom/src/impedance.rs (width_for_ohms, microstrip, stripline)
//! - crates/doc/src/lib.rs (Project, Track, Layer)
//! - crates/rules/src/impedance_target.rs (stack_sample, segment_impedance, pair_for_net)
//!
//! # Do not
//! - Try to suggest a gap change for pairs; that is a separate concern.
//! - Return a width for a track that already meets its target; return the
//!   width that would produce the target, which may be very close to the
//!   current width.

use super::impedance_target::{pair_for_net, segment_impedance, stack_sample};
use pcbcraft_doc::{Id, Project};

/// Suggested track width that would meet the net's impedance target.
///
/// Returns None when the track has no impedance target, no stackup sample,
/// or belongs to a differential pair (pair legs need a gap change, not a
/// width change). For a single-ended track, solves for the width that
/// produces the target ohms using the same closed-form model as the
/// impedance-target check.
pub fn suggested_width(project: &Project, track_id: Id) -> Option<f64> {
    let track = project.board.tracks.iter().find(|t| t.id == track_id)?;
    let (target, _tolerance_pct) = project.impedance_target_for_net(track.net)?;
    if pair_for_net(project, track.net).is_some() {
        return None;
    }
    let sample = stack_sample(project, track.layer)?;
    pcbcraft_geom::impedance::width_for_ohms(target, |w| {
        let mut t = track.clone();
        t.width = w;
        segment_impedance(project, &t, None, &sample).map(|(z, _)| z)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Layer, LayerRecord, Net, Track};
    use pcbcraft_geom::Point;

    fn four_layer_stack() -> Project {
        let mut p = Project::default();
        p.board.thickness = 1.6;
        p.board.er = 4.5;
        p.board.layer_records = vec![
            LayerRecord {
                name: "F.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: Some("signal".into()),
                ..Default::default()
            },
            LayerRecord {
                name: "Prepreg1".into(),
                kind: "prepreg".into(),
                thickness: 0.2,
                dk: Some(4.5),
                ..Default::default()
            },
            LayerRecord {
                name: "In1.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: Some("plane".into()),
                ..Default::default()
            },
            LayerRecord {
                name: "Core".into(),
                kind: "core".into(),
                thickness: 1.0,
                dk: Some(4.5),
                ..Default::default()
            },
            LayerRecord {
                name: "In2.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: Some("plane".into()),
                ..Default::default()
            },
            LayerRecord {
                name: "Prepreg2".into(),
                kind: "prepreg".into(),
                thickness: 0.2,
                dk: Some(4.5),
                ..Default::default()
            },
            LayerRecord {
                name: "B.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: Some("signal".into()),
                ..Default::default()
            },
        ];
        p.nets.push(Net {
            id: 1,
            name: "SIG".into(),
            impedance_target: Some(50.0),
            impedance_tolerance_pct: Some(10.0),
            ..Default::default()
        });
        p
    }

    fn add_track(p: &mut Project, id: Id, width: f64) {
        p.board.tracks.push(Track {
            id,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(40.0, 10.0),
            width,
            layer: Layer::Front,
            arc: None,
            preserved: Default::default(),
        });
    }

    #[test]
    fn the_suggested_width_for_fifty_ohm_is_a_plausible_microstrip() -> Result<(), String> {
        let mut p = four_layer_stack();
        add_track(&mut p, 10, 0.12);
        p.validate()?;

        let w = suggested_width(&p, 10).ok_or("suggested width must be Some")?;
        assert!(
            (0.30..=0.42).contains(&w),
            "suggested width {:.3} mm should be in [0.30, 0.42] for 50 ohm microstrip over 0.2 mm FR4",
            w
        );

        // After setting the width, the finding should be gone.
        p.board.tracks[0].width = w;
        p.validate()?;
        let findings = super::super::impedance_target::impedance_target_findings(&p);
        assert!(
            findings.is_empty(),
            "no impedance finding after applying suggested width; got {:?}",
            findings
                .iter()
                .map(|f| f.message.clone())
                .collect::<Vec<_>>()
        );

        // With the original narrow width, the finding should fire.
        p.board.tracks[0].width = 0.12;
        p.validate()?;
        let findings = super::super::impedance_target::impedance_target_findings(&p);
        assert!(
            !findings.is_empty(),
            "impedance finding should fire at 0.12 mm width"
        );

        Ok(())
    }

    #[test]
    fn a_track_already_on_target_returns_a_close_width() -> Result<(), String> {
        let mut p = four_layer_stack();
        // First compute the actual 50 ohm width, then use it as the track width.
        let sample = super::super::impedance_target::stack_sample(&p, pcbcraft_doc::Layer::Front)
            .ok_or("stack sample")?;
        let target_width = pcbcraft_geom::impedance::width_for_ohms(50.0, |w| {
            pcbcraft_geom::impedance::microstrip(w, sample.height, sample.copper, sample.er)
                .map(|m| m.ohms)
        })
        .ok_or("solve 50 ohm width")?;
        add_track(&mut p, 10, target_width);
        p.validate()?;

        let w = suggested_width(&p, 10).ok_or("suggested width must be Some")?;
        let current = p.board.tracks[0].width;
        let ratio = w / current;
        assert!(
            (0.99..=1.01).contains(&ratio),
            "suggested width {:.3} should be within 1 percent of current width {:.3}",
            w,
            current
        );

        Ok(())
    }

    #[test]
    fn a_net_with_no_target_returns_none() -> Result<(), String> {
        let mut p = four_layer_stack();
        p.nets[0].impedance_target = None;
        add_track(&mut p, 10, 0.15);
        p.validate()?;

        assert!(
            suggested_width(&p, 10).is_none(),
            "suggested width must be None when the net has no target"
        );

        Ok(())
    }
}
