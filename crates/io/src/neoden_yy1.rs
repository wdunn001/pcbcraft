//! NeoDen YY1 pick and place files: one placement CSV per board side and a
//! feeder sheet the operator loads reels from.
//!
//! # Rules
//! - Pure text generation from a `Project` and a `pick_place` machine profile;
//!   no file is written here, the engine writes through `atomic_write`.
//! - Every file is CRLF, has no byte order mark and has no comma inside a field.
//! - Coordinates are positive millimetres to two decimals from the lower-left
//!   corner of the board (lower-right for the bottom side, so X is mirrored).
//! - Through-hole parts are never written; `placement_text` stays untouched.
//!
//! # Dependencies
//! - crates/doc/src/machine_profile.rs
//! - machine-profiles/neoden-yy1.toml
//! - crates/io/tests/fixtures/neoden_yy1_top.native.csv

use pcbcraft_doc::{Component, Footprint, Layer, MachineProfile, Project};
use std::collections::{BTreeMap, BTreeSet};

const MAX_PARTS: usize = 5_000;
const NATIVE_PREAMBLE_HEAD: &str = "NEODEN,YY1,P&P FILE,,,,,,,,,,,";
const BLANK_ROW: &str = ",,,,,,,,,,,,,";
const NOZZLE_ROWS: [&str; 4] = [
    "NozzleChange,OFF,BeforeComponent,1,Head1,Drop,Station2,PickUp,Station1,",
    "NozzleChange,OFF,BeforeComponent,2,Head2,Drop,Station3,PickUp,Station2,",
    "NozzleChange,OFF,BeforeComponent,1,Head1,Drop,Station1,PickUp,Station1,",
    "NozzleChange,OFF,BeforeComponent,1,Head1,Drop,Station1,PickUp,Station1,",
];
// The trailing spaces in `Mid Y(mm) ` and `Head ` are the machine's own.
const NATIVE_COLUMNS: &str = "Designator,Comment,Footprint,Mid X(mm),Mid Y(mm) ,Rotation,Head ,FeederNo,Mount Speed(%),Pick Height(mm),Place Height(mm),Mode,Skip";
const IMPORT_COLUMNS: &str =
    "Designator,Footprint,Mid X,Mid Y,Ref X,Ref Y,Pad X,Pad Y,Layer,Rotation,Comment";
const FEEDER_COLUMNS: &str = "FeederNo,Comment,Footprint,TapeWidth_mm,Nozzle,Quantity,Load";
const MOUNT_SPEED_PCT: u32 = 75;
const RECOGNITION_CAMERA: u32 = 1;

/// Which board side or sides to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YySide {
    /// Parts on the front; origin at the lower-left corner.
    Top,
    /// Parts on the back; origin at the lower-right corner, X mirrored.
    Bottom,
    /// One file per side plus a single feeder sheet covering both.
    Both,
}

/// Which of the two layouts the machine accepts to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YyLayout {
    /// What the machine writes back: header rows, 13 columns, rotation 0 to 360.
    Native,
    /// The 11-column template, rotation -180 to 180; feeders are typed on the machine.
    Import,
}

/// Choices for one YY1 export.
#[derive(Clone, Debug)]
pub struct NeodenYy1Options {
    /// Column layout to write.
    pub layout: YyLayout,
    /// Name of a variant whose unpopulated parts are left out, if any.
    pub variant: Option<String>,
    /// Write unpopulated parts with Skip set to 1 instead of omitting them (native only).
    pub include_dnp_as_skip: bool,
}

/// The files of one export and anything the operator should know about.
#[derive(Clone, Debug, PartialEq)]
pub struct NeodenYy1Output {
    /// File name and bytes: `Top.csv` and or `Bottom.csv`, then `FeederSheet.csv`.
    pub files: Vec<(String, Vec<u8>)>,
    /// Plain sentences, for example a missing fiducial or a part with no nozzle.
    pub warnings: Vec<String>,
}

struct Part {
    reference: String,
    comment: String,
    footprint: String,
    x: f64,
    y: f64,
    rotation: f64,
    bottom: bool,
    skip: bool,
}

/// Write the YY1 placement file or files and the feeder sheet for a design.
///
/// # Rules
/// - The profile must be kind `pick_place` with a board limit; a larger board is refused.
/// - Package names pass through the profile's footprint aliases, then rotation
///   offsets are added before the bottom side negates the angle.
/// - Feeder numbers come from the profile's feeder map first, then the next free
///   tape feeder for chip packages or the next flexible feeder for the rest.
///
/// # Dependencies
/// - crates/doc/src/machine_profile.rs
///
/// # Do not
/// - Change `placement_text`, or write a comma or a negative coordinate into a file.
pub fn placement_neoden_yy1(
    p: &Project,
    side: YySide,
    profile: &MachineProfile,
    options: &NeodenYy1Options,
) -> Result<NeodenYy1Output, String> {
    p.validate()?;
    generate(p, side, profile, options)
}

/// The export without the whole-design check, so tests can place back-side
/// parts that a design cannot hold yet.
fn generate(
    p: &Project,
    side: YySide,
    profile: &MachineProfile,
    options: &NeodenYy1Options,
) -> Result<NeodenYy1Output, String> {
    if profile.kind != "pick_place" {
        return Err(format!(
            "machine \"{}\" is not a pick and place machine",
            profile.id
        ));
    }
    profile.validate()?;
    let pp = &profile.pick_place;
    let board = pp
        .board
        .as_ref()
        .ok_or("the machine profile has no board size limit")?;
    if p.board.width > board.max_x_mm || p.board.height > board.max_y_mm {
        return Err(format!(
            "the board is {:.1} x {:.1} mm and this machine takes at most {:.0} x {:.0} mm",
            p.board.width, p.board.height, board.max_x_mm, board.max_y_mm
        ));
    }
    if p.board.footprints.len() > MAX_PARTS {
        return Err("placement file exceeds the placed-object limit".into());
    }
    let variant = match options.variant.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(name) => Some(
            p.variants
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| format!("no variant named {name}"))?,
        ),
    };
    let components: BTreeMap<u64, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
    let mut warnings = Vec::new();
    let mut parts = Vec::new();
    for f in &p.board.footprints {
        let c = components
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        if !c.in_placement() {
            continue;
        }
        if is_through_hole(f) {
            continue;
        }
        let unpopulated = variant.is_some_and(|v| v.dnp.contains(&c.id));
        if unpopulated && !options.include_dnp_as_skip {
            continue;
        }
        let bottom = is_bottom(f);
        let package = variant
            .and_then(|v| v.footprints.iter().find(|o| o.component == c.id))
            .map(|o| o.footprint.clone())
            .unwrap_or_else(|| match &c.library_footprint {
                Some(fp) => fp.definition.name.clone(),
                None => c.kind.clone(),
            });
        let footprint = clean(&alias(pp, &package));
        let comment = clean(&c.value);
        let offset = pp
            .rotation_offset
            .iter()
            .find(|r| {
                (r.comment.is_empty() || r.comment == comment)
                    && footprint.starts_with(&r.footprint)
            })
            .map_or(0.0, |r| r.offset_deg);
        let mut rotation = f.rotation + offset;
        if bottom {
            rotation = -rotation;
        }
        let x = if bottom {
            p.board.width - f.position.x
        } else {
            f.position.x
        };
        let reference = clean(&c.reference);
        if round2(x) < 0.0 || round2(f.position.y) < 0.0 {
            return Err(format!(
                "part {reference} sits outside the board, so its machine coordinate would be negative"
            ));
        }
        parts.push(Part {
            reference,
            comment,
            footprint,
            x,
            y: f.position.y,
            rotation,
            bottom,
            skip: unpopulated,
        });
    }
    parts.sort_by_key(|a| natural(&a.reference));
    let feeders = assign_feeders(pp, &parts)?;
    let mut files = Vec::new();
    let sides: &[(bool, &str)] = match side {
        YySide::Top => &[(false, "Top.csv")],
        YySide::Bottom => &[(true, "Bottom.csv")],
        YySide::Both => &[(false, "Top.csv"), (true, "Bottom.csv")],
    };
    for (bottom, name) in sides {
        let text = match options.layout {
            YyLayout::Native => native_file(p, &parts, *bottom, &feeders, &mut warnings),
            YyLayout::Import => import_file(&parts, *bottom),
        };
        files.push(((*name).to_string(), text.into_bytes()));
    }
    let sheet = feeder_sheet(
        pp,
        &parts,
        sides.iter().map(|s| s.0).collect(),
        &feeders,
        &mut warnings,
    );
    files.push(("FeederSheet.csv".into(), sheet.into_bytes()));
    Ok(NeodenYy1Output { files, warnings })
}

fn is_bottom(f: &Footprint) -> bool {
    f.pads.iter().any(|pad| {
        pad.geometry.as_ref().is_some_and(|g| {
            let layers = f.pad_geometry_layers(pad);
            g.pad_type == "smd" && !layers.is_empty() && !layers.iter().any(|l| l.starts_with("F."))
        })
    })
}

fn is_through_hole(f: &Footprint) -> bool {
    f.pads.iter().any(|pad| {
        pad.geometry
            .as_ref()
            .is_some_and(|g| g.pad_type == "thru_hole")
    })
}

/// Strip what would break a comma separated field.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, ',' | '\r' | '\n' | '"') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Round to two decimals and fold negative zero into zero.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0 + 0.0
}

/// Apply the first matching alias row. A pattern `^abc.*` matches any name
/// that starts with `abc`; `^abc$` or plain `abc` match that name exactly.
fn alias(pp: &pcbcraft_doc::PickPlaceSection, name: &str) -> String {
    for row in &pp.footprint_alias {
        let pat = row.pattern.strip_prefix('^').unwrap_or(&row.pattern);
        let hit = if let Some(prefix) = pat.strip_suffix(".*") {
            name.starts_with(prefix)
        } else {
            name == pat.strip_suffix('$').unwrap_or(pat)
        };
        if hit {
            return row.replacement.clone();
        }
    }
    name.to_string()
}

/// Sort key that puts C2 before C10.
fn natural(reference: &str) -> (String, u64, String) {
    let split = reference
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(reference.len());
    let (prefix, rest) = reference.split_at(split);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let number = digits.parse().unwrap_or(0);
    (prefix.to_string(), number, rest[digits.len()..].to_string())
}

fn is_chip(footprint: &str) -> bool {
    footprint.len() == 4 && footprint.bytes().all(|b| b.is_ascii_digit())
}

/// Feeder number for each (comment, footprint) pair that is actually placed.
fn assign_feeders(
    pp: &pcbcraft_doc::PickPlaceSection,
    parts: &[Part],
) -> Result<BTreeMap<(String, String), u32>, String> {
    let bank = |name: &str| pp.feeders.iter().find(|b| b.name == name);
    let mut used: BTreeSet<u32> = pp.feeder_map.iter().map(|r| r.feeder_no).collect();
    let mut out = BTreeMap::new();
    let mut keys: Vec<(String, String)> = Vec::new();
    for part in parts.iter().filter(|p| !p.skip) {
        let key = (part.comment.clone(), part.footprint.clone());
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    for key in keys {
        if let Some(row) = pp
            .feeder_map
            .iter()
            .find(|r| r.footprint == key.1 && (r.comment.is_empty() || r.comment == key.0))
        {
            out.insert(key, row.feeder_no);
            continue;
        }
        let name = if is_chip(&key.1) { "tape" } else { "flexible" };
        let bank =
            bank(name).ok_or_else(|| format!("the machine profile has no {name} feeder range"))?;
        let number = (bank.first..=bank.last)
            .find(|n| !used.contains(n))
            .ok_or_else(|| format!("the {name} feeders are all taken; this board needs more part types than the machine has"))?;
        used.insert(number);
        out.insert(key, number);
    }
    Ok(out)
}

fn native_file(
    p: &Project,
    parts: &[Part],
    bottom: bool,
    feeders: &BTreeMap<(String, String), u32>,
    warnings: &mut Vec<String>,
) -> String {
    let wanted = if bottom { Layer::Back } else { Layer::Front };
    let (fx, fy) = match p.board.fiducials.iter().find(|f| f.layer == wanted) {
        Some(f) => (
            if bottom {
                p.board.width - f.position.x
            } else {
                f.position.x
            },
            f.position.y,
        ),
        None => {
            warnings.push(format!(
                "the {} side has no fiducial, so the header writes 0.00 for fiducial 1",
                if bottom { "bottom" } else { "top" }
            ));
            (0.0, 0.0)
        }
    };
    let mut rows = vec![
        NATIVE_PREAMBLE_HEAD.to_string(),
        BLANK_ROW.to_string(),
        "PanelizedPCB,UnitLength,0,UnitWidth,0,Rows,1,Columns,1,".to_string(),
        BLANK_ROW.to_string(),
        format!(
            "Fiducial,1-X,{:.2},1-Y,{:.2},OverallOffsetX,0,OverallOffsetY,0,",
            round2(fx),
            round2(fy)
        ),
        BLANK_ROW.to_string(),
    ];
    rows.extend(NOZZLE_ROWS.iter().map(|r| (*r).to_string()));
    rows.push(BLANK_ROW.to_string());
    rows.push(BLANK_ROW.to_string());
    rows.push(NATIVE_COLUMNS.to_string());
    for part in parts.iter().filter(|q| q.bottom == bottom) {
        let feeder = feeders
            .get(&(part.comment.clone(), part.footprint.clone()))
            .copied()
            .unwrap_or(0);
        rows.push(format!(
            "{},{},{},{:.2},{:.2},{:.2},0,{},{},0.0,0.0,{},{}",
            part.reference,
            part.comment,
            part.footprint,
            round2(part.x),
            round2(part.y),
            round2(part.rotation.rem_euclid(360.0)).rem_euclid(360.0),
            feeder,
            MOUNT_SPEED_PCT,
            RECOGNITION_CAMERA,
            u8::from(part.skip)
        ));
    }
    join_crlf(&rows)
}

fn import_file(parts: &[Part], bottom: bool) -> String {
    let mut rows = vec![IMPORT_COLUMNS.to_string()];
    for part in parts.iter().filter(|q| q.bottom == bottom && !q.skip) {
        let r = part.rotation.rem_euclid(360.0);
        let r = if r > 180.0 { r - 360.0 } else { r };
        let (x, y) = (round2(part.x), round2(part.y));
        rows.push(format!(
            "{},{},{x:.2},{y:.2},{x:.2},{y:.2},{x:.2},{y:.2},{},{:.2},{}",
            part.reference,
            part.footprint,
            if bottom { "B" } else { "T" },
            round2(r),
            part.comment
        ));
    }
    join_crlf(&rows)
}

fn feeder_sheet(
    pp: &pcbcraft_doc::PickPlaceSection,
    parts: &[Part],
    sides: Vec<bool>,
    feeders: &BTreeMap<(String, String), u32>,
    warnings: &mut Vec<String>,
) -> String {
    let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
    for part in parts
        .iter()
        .filter(|q| sides.contains(&q.bottom) && !q.skip)
    {
        *counts
            .entry((part.comment.clone(), part.footprint.clone()))
            .or_default() += 1;
    }
    let mut lines: Vec<(u32, String)> = Vec::new();
    for (key, quantity) in &counts {
        let number = feeders.get(key).copied().unwrap_or(0);
        let nozzle = pp
            .nozzle
            .iter()
            .find(|n| n.packages.iter().any(|pkg| key.1.starts_with(pkg.as_str())));
        if nozzle.is_none() {
            warnings.push(format!(
                "no nozzle in the machine profile fits package {}",
                key.1
            ));
        }
        let tape = pp
            .feeders
            .iter()
            .any(|b| b.name == "tape" && number >= b.first && number <= b.last);
        lines.push((
            number,
            format!(
                "{number},{},{},{},{},{quantity},",
                key.0,
                key.1,
                if tape { "8" } else { "" },
                nozzle.map_or("", |n| n.id.as_str())
            ),
        ));
    }
    lines.sort();
    let mut rows = vec![FEEDER_COLUMNS.to_string()];
    rows.extend(lines.into_iter().map(|l| l.1));
    join_crlf(&rows)
}

fn join_crlf(rows: &[String]) -> String {
    let mut out = rows.join("\r\n");
    out.push_str("\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Fiducial, Pad, Pin, Variant};
    use pcbcraft_geom::Point;
    use pcbcraft_library::FootprintPad;

    fn library_pad(pad_type: &str, layer: &str) -> FootprintPad {
        FootprintPad {
            number: "1".into(),
            pad_type: pad_type.into(),
            shape: "rect".into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: vec![layer.into()],
            drill: None,
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        }
    }

    fn pad(pad_type: &str, layer: &str) -> Pad {
        Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.0,
            net: None,
            castellated: None,
            geometry: Some(library_pad(pad_type, layer)),
        }
    }

    /// Add one part: id, reference, value, package kind, x, y, rotation, pad kind and layer.
    #[allow(clippy::too_many_arguments)]
    fn add(
        p: &mut Project,
        id: u64,
        reference: &str,
        value: &str,
        kind: &str,
        at: (f64, f64, f64),
        pad_kind: (&str, &str),
    ) {
        p.components.push(Component {
            id,
            reference: reference.into(),
            value: value.into(),
            kind: "resistor".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![Pin {
                number: "1".into(),
                name: "1".into(),
                offset: Point::default(),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Test".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: kind.into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![library_pad(pad_kind.0, pad_kind.1)],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            fields: vec![],
            include_in_bom: true,
            include_in_placement: true,
            sheet: 0,
        });
        p.board.footprints.push(Footprint {
            id: id + 100,
            component: id,
            position: Point::new(at.0, at.1),
            rotation: at.2,
            locked: None,
            side: None,
            pads: vec![pad(pad_kind.0, pad_kind.1)],
        });
    }

    /// Flip a placed part to the bottom the way the `footprint.flip` command
    /// does: the side is set and the pads are mirrored about the local Y axis.
    fn flip_to_bottom(p: &mut Project, footprint: u64) {
        for f in p.board.footprints.iter_mut().filter(|f| f.id == footprint) {
            f.side = Some(Layer::Back);
            for pad in &mut f.pads {
                pad.offset = Point::new(-pad.offset.x, pad.offset.y);
            }
        }
    }

    fn sample() -> Project {
        let mut p = Project::default();
        p.board.width = 50.0;
        p.board.height = 40.0;
        add(
            &mut p,
            1,
            "R1",
            "10k",
            "0603",
            (10.0, 5.0, -90.0),
            ("smd", "F.Cu"),
        );
        add(
            &mut p,
            2,
            "C1",
            "100nF",
            "0805",
            (28.85, 2.25, 180.0),
            ("smd", "F.Cu"),
        );
        add(
            &mut p,
            3,
            "U1",
            "AMS1117",
            "SOT-23",
            (20.0, 20.0, 0.0),
            ("smd", "F.Cu"),
        );
        p.board.fiducials.push(Fiducial {
            id: 900,
            position: Point::new(3.0, 3.0),
            diameter: 1.0,
            layer: Layer::Front,
        });
        p
    }

    fn yy1() -> Result<MachineProfile, String> {
        crate::builtin_machine_profile("neoden-yy1")
    }

    fn native() -> NeodenYy1Options {
        NeodenYy1Options {
            layout: YyLayout::Native,
            variant: None,
            include_dnp_as_skip: false,
        }
    }

    fn text(out: &NeodenYy1Output, name: &str) -> Result<String, String> {
        let bytes = out
            .files
            .iter()
            .find(|f| f.0 == name)
            .ok_or_else(|| format!("{name} was not written"))?;
        String::from_utf8(bytes.1.clone()).map_err(|e| e.to_string())
    }

    #[test]
    fn neoden_yy1_native_top_matches_golden_fixture() -> Result<(), String> {
        let out = placement_neoden_yy1(&sample(), YySide::Top, &yy1()?, &native())?;
        let golden = include_str!("../tests/fixtures/neoden_yy1_top.native.csv");
        assert_eq!(text(&out, "Top.csv")?, golden);
        let sheet = include_str!("../tests/fixtures/neoden_yy1_feeder_sheet.csv");
        assert_eq!(text(&out, "FeederSheet.csv")?, sheet);
        Ok(())
    }

    #[test]
    fn neoden_yy1_rotation_minus_90_writes_270_native() -> Result<(), String> {
        let out = placement_neoden_yy1(&sample(), YySide::Top, &yy1()?, &native())?;
        assert!(text(&out, "Top.csv")?.contains("R1,10k,0603,10.00,5.00,270.00,"));
        Ok(())
    }

    #[test]
    fn neoden_yy1_bottom_mirror_negates_rotation() -> Result<(), String> {
        let mut p = Project::default();
        p.board.width = 50.0;
        p.board.height = 40.0;
        add(
            &mut p,
            1,
            "R1",
            "10k",
            "0603",
            (10.0, 5.0, 30.0),
            ("smd", "F.Cu"),
        );
        // The normal path: library footprints stay authored on the top and the
        // placed part is flipped to the bottom, as `footprint.flip` does.
        flip_to_bottom(&mut p, 101);
        let out = generate(&p, YySide::Bottom, &yy1()?, &native())?;
        let file = text(&out, "Bottom.csv")?;
        assert!(file.contains("R1,10k,0603,40.00,5.00,330.00,"), "{file}");
        for row in file.lines().skip(12) {
            for field in row.split(',').take(6).skip(3) {
                assert!(!field.starts_with('-'), "negative value in {row}");
            }
        }
        Ok(())
    }

    #[test]
    fn neoden_yy1_omits_tht_and_dnp_unless_skip() -> Result<(), String> {
        let mut p = sample();
        add(
            &mut p,
            4,
            "J1",
            "HDR",
            "header",
            (5.0, 5.0, 0.0),
            ("thru_hole", "*.Cu"),
        );
        p.variants.push(Variant {
            name: "Lean".into(),
            note: None,
            dnp: vec![2],
            footprints: vec![],
        });
        let mut opts = native();
        opts.variant = Some("Lean".into());
        let out = generate(&p, YySide::Top, &yy1()?, &opts)?;
        let file = text(&out, "Top.csv")?;
        assert!(!file.contains("J1,") && !file.contains("C1,"));
        opts.include_dnp_as_skip = true;
        let out = generate(&p, YySide::Top, &yy1()?, &opts)?;
        let file = text(&out, "Top.csv")?;
        assert!(!file.contains("J1,"));
        assert!(
            file.contains("C1,100nF,0805,28.85,2.25,180.00,0,0,75,0.0,0.0,1,1\r\n"),
            "{file}"
        );
        assert!(!text(&out, "FeederSheet.csv")?.contains("100nF"));
        Ok(())
    }

    #[test]
    fn neoden_yy1_import_layout_has_eleven_columns() -> Result<(), String> {
        let mut opts = native();
        opts.layout = YyLayout::Import;
        let out = placement_neoden_yy1(&sample(), YySide::Top, &yy1()?, &opts)?;
        let file = text(&out, "Top.csv")?;
        let mut lines = file.lines();
        assert_eq!(
            lines.next(),
            Some("Designator,Footprint,Mid X,Mid Y,Ref X,Ref Y,Pad X,Pad Y,Layer,Rotation,Comment")
        );
        let mut rows = 0;
        for row in lines {
            let cols: Vec<&str> = row.split(',').collect();
            assert_eq!(cols.len(), 11, "{row}");
            assert!(cols[8] == "T" || cols[8] == "B");
            rows += 1;
        }
        assert_eq!(rows, 3);
        assert!(
            file.contains("R1,0603,10.00,5.00,10.00,5.00,10.00,5.00,T,-90.00,10k"),
            "{file}"
        );
        Ok(())
    }

    #[test]
    fn neoden_yy1_refuses_oversize_board() -> Result<(), String> {
        for (w, h) in [(351.0, 100.0), (100.0, 250.0)] {
            let mut p = sample();
            p.board.width = w;
            p.board.height = h;
            let err = placement_neoden_yy1(&p, YySide::Top, &yy1()?, &native())
                .err()
                .ok_or("an oversize board was accepted")?;
            assert!(err.contains("at most 350 x 249 mm"), "{err}");
        }
        Ok(())
    }

    #[test]
    fn neoden_yy1_warns_when_the_side_has_no_fiducial() -> Result<(), String> {
        let mut p = sample();
        p.board.fiducials.clear();
        let out = placement_neoden_yy1(&p, YySide::Top, &yy1()?, &native())?;
        assert!(out.warnings.iter().any(|w| w.contains("no fiducial")));
        assert!(text(&out, "Top.csv")?.contains("Fiducial,1-X,0.00,1-Y,0.00,"));
        Ok(())
    }

    #[test]
    fn neoden_yy1_refuses_a_profile_of_another_kind() -> Result<(), String> {
        let other = crate::builtin_machine_profile("eufymake-e1")?;
        assert!(placement_neoden_yy1(&sample(), YySide::Top, &other, &native()).is_err());
        Ok(())
    }
}
