//! Back-drill passes for the drill export: hits, files and map markers.
//!
//! Context: a back-drill is a second drilling pass that removes the unused
//! barrel of a via. It is written to its own Excellon file per pass, never
//! into the through, blind or buried files, so a project without back-drills
//! exports exactly what it did before.
//!
//! # Rules
//! - One pass is one entry face plus one drilled layer range; each pass gets one file named `pcb-BackDrill-L<entry>-L<stop>.drl`, layers counted from 1 at the top.
//! - Tools carry `TA.AperFunction,NonPlated,NPTH,BackDrill`; the file header names the entry and the deepest drilled layer.
//! - Hits come from `Project::effective_backdrill`; a spec that no longer fits its via is skipped here and reported by the rules crate.
//!
//! # Dependencies
//! - `crates/doc/src/backdrill.rs` `BackDrill`, `Project::effective_backdrill`, `Project::backdrilled_layers`.
//! - `crates/io/src/fab_export.rs` `MAX_TOOLS` limit, written through `excellon_pass`.
//!
//! # Do not
//! - Add a back-drill hit to `collect_drill_hits`; the other files must not change.
use crate::fab_export::MAX_TOOLS;
use pcbcraft_doc::{BackDrillFrom, Project};
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

/// One back-drilled hole.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BackDrillHit {
    /// Drill diameter in micrometres.
    pub d_um: u64,
    pub x: f64,
    pub y: f64,
    pub from: BackDrillFrom,
    /// Layer index of the face the drill enters (0 for the top).
    pub entry: u8,
    /// Layer index of the deepest layer the drill removes.
    pub stop: u8,
}

impl BackDrillHit {
    /// The drilled range as ascending layer indices.
    pub(crate) fn range(&self) -> (u8, u8) {
        (self.entry.min(self.stop), self.entry.max(self.stop))
    }
}

/// Every back-drilled hole of the project, in via order.
pub(crate) fn collect_backdrill_hits(p: &Project) -> Vec<BackDrillHit> {
    let last = p.copper_layer_count().saturating_sub(1);
    let mut hits = vec![];
    for v in &p.board.vias {
        let Some(spec) = p.effective_backdrill(v) else {
            continue;
        };
        let gone = p.backdrilled_layers(v, &spec);
        let stop = match spec.from {
            BackDrillFrom::Top => gone.last(),
            BackDrillFrom::Bottom => gone.first(),
        };
        let Some(stop) = stop else {
            continue;
        };
        hits.push(BackDrillHit {
            d_um: (spec.diameter * 1_000.0).round() as u64,
            x: v.position.x,
            y: v.position.y,
            from: spec.from,
            entry: match spec.from {
                BackDrillFrom::Top => 0,
                BackDrillFrom::Bottom => last,
            },
            stop: stop.index(),
        });
    }
    hits
}

/// The back-drill files of a project: file name to Excellon text. Empty when
/// no via is back-drilled.
pub(crate) fn backdrill_files(
    p: &Project,
    scale: f64,
    origin: Point,
    inch: bool,
    tolerance: Option<f64>,
) -> Result<BTreeMap<String, String>, String> {
    let mut passes: BTreeMap<(u8, u8), Vec<BackDrillHit>> = BTreeMap::new();
    for h in collect_backdrill_hits(p) {
        passes.entry((h.entry, h.stop)).or_default().push(h);
    }
    let mut files = BTreeMap::new();
    for ((entry, stop), hits) in passes {
        files.insert(
            format!("pcb-BackDrill-L{}-L{}.drl", entry + 1, stop + 1),
            excellon_pass(&hits, entry, stop, scale, origin, inch, tolerance)?,
        );
    }
    Ok(files)
}

fn excellon_pass(
    hits: &[BackDrillHit],
    entry: u8,
    stop: u8,
    scale: f64,
    origin: Point,
    inch: bool,
    tolerance: Option<f64>,
) -> Result<String, String> {
    let mut tools: BTreeMap<u64, Vec<&BackDrillHit>> = BTreeMap::new();
    for h in hits {
        tools.entry(h.d_um).or_default().push(h);
    }
    if tools.len() > MAX_TOOLS {
        return Err("back-drill export needs 64 or fewer distinct tools".into());
    }
    let (unit, prec) = if inch {
        ("INCH,TZ", 4)
    } else {
        ("METRIC,TZ", 3)
    };
    let mut out = format!(
        "M48\nNonPlated,{},{},Blind\nFMAT,2\n{unit}\n",
        entry + 1,
        stop + 1
    );
    for (n, d) in tools.keys().enumerate() {
        if let Some(t) = tolerance {
            out.push_str(&format!(
                "; #@! TA.DrillTolerance,{t:.prec$},{t:.prec$}\n",
                t = t * scale
            ));
        }
        out.push_str("; #@! TA.AperFunction,NonPlated,NPTH,BackDrill\n");
        out.push_str(&format!(
            "T{}C{:.prec$}\n; #@! TD\n",
            n + 1,
            *d as f64 / 1000.0 * scale
        ));
    }
    out.push_str("%\nG90\nG05\n");
    for (n, group) in tools.values().enumerate() {
        out.push_str(&format!("T{}\n", n + 1));
        for h in group {
            let x = (h.x - origin.x) * scale;
            let y = (h.y - origin.y) * scale;
            out.push_str(&format!("X{x:.prec$}Y{y:.prec$}\n"));
        }
    }
    out.push_str("M30\n");
    Ok(out)
}
