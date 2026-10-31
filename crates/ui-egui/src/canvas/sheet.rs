//! The drawn schematic sheet: colours, frame with zone letters and numbers,
//! title block, grid dots, power and ground symbols, junction dots and the
//! selection box. Painting only; nothing here changes the project.
//!
//! Local rules: every colour of the schematic picture is a named constant in
//! `SheetColours` (the approved Schematic page of the design system); no other
//! file in `canvas` holds a schematic colour literal. Lengths are millimetres
//! in the world and pixels only for text and handle sizes.
//!
//! Dependencies: `crates/ui-egui/src/canvas.rs` calls in from `paint_schematic`
//! and `symbol`; `crates/doc/src/lib.rs` supplies `Project`, `Wire`, `Label`.
use super::*;
use egui::{Align2, Color32, FontId};
use std::collections::HashMap;

/// Every colour of the schematic picture for one theme.
///
/// Why: the approved Schematic page defines canvas colours separately from the
/// shell, so a theme change flips this set as one unit.
///
/// # Rules
/// - Values are the design system canvas tokens: sheet fill, wire, symbol
///   body, pin number and selection.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas.rs` reads it through `SheetColours::for_theme`.
///
/// # Do not
/// - Add a literal colour for a schematic part in `canvas.rs`; add a field.
#[derive(Clone, Copy, Debug)]
pub(super) struct SheetColours {
    pub backdrop: Color32,
    pub sheet: Color32,
    pub dots: Color32,
    pub frame: Color32,
    pub zone_text: Color32,
    pub wire: Color32,
    pub body: Color32,
    pub pin: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub select: Color32,
}

impl SheetColours {
    const DARK: Self = Self {
        backdrop: Color32::from_rgb(0x07, 0x10, 0x0D),
        sheet: Color32::from_rgb(0x0B, 0x17, 0x13),
        dots: Color32::from_rgb(0x1B, 0x33, 0x29),
        frame: Color32::from_rgb(0x2C, 0x4A, 0x3E),
        zone_text: Color32::from_rgb(0x9D, 0xB3, 0xA8),
        wire: Color32::from_rgb(0x4F, 0xCB, 0x8C),
        body: Color32::from_rgb(0xE6, 0xBE, 0x5A),
        pin: Color32::from_rgb(0x5C, 0xC8, 0xE8),
        text: Color32::from_rgb(0xE8, 0xF0, 0xEB),
        muted: Color32::from_rgb(0x9D, 0xB3, 0xA8),
        select: Color32::from_rgb(0x5C, 0xC8, 0xE8),
    };
    const LIGHT: Self = Self {
        backdrop: Color32::from_rgb(0xF1, 0xF0, 0xE6),
        sheet: Color32::from_rgb(0xFB, 0xFA, 0xF3),
        dots: Color32::from_rgb(0xCF, 0xCE, 0xBF),
        frame: Color32::from_rgb(0xB9, 0xC4, 0xBC),
        zone_text: Color32::from_rgb(0x44, 0x57, 0x4E),
        wire: Color32::from_rgb(0x14, 0x66, 0x3F),
        body: Color32::from_rgb(0x7A, 0x5A, 0x00),
        pin: Color32::from_rgb(0x0B, 0x67, 0x85),
        text: Color32::from_rgb(0x10, 0x1C, 0x17),
        muted: Color32::from_rgb(0x44, 0x57, 0x4E),
        select: Color32::from_rgb(0x0B, 0x67, 0x85),
    };

    /// The colour set for the dark or the light theme.
    pub(super) fn for_theme(dark: bool) -> Self {
        if dark { Self::DARK } else { Self::LIGHT }
    }
}

/// How a net is drawn when it is a supply: a rail hangs from a bar above, a
/// ground hangs from a stack of bars below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PowerKind {
    Rail,
    Ground,
}

/// Classify a net name as a ground, a supply rail or neither.
///
/// Why: a schematic shows VCC and GND as standard symbols instead of long
/// wires and bare labels; the name is the only signal a plain net carries.
///
/// # Rules
/// - Case-insensitive. Ground: GND, AGND, DGND, PGND, VSS, 0V, EARTH and names
///   starting with GND. Rail: VCC, VDD, VBUS, VIN, VBAT, 3V3, 5V, 12V, V+ and
///   names starting with VCC, VDD, + or a digit followed by V.
///
/// # Do not
/// - Treat an unnamed or `unassigned` net as power.
pub(super) fn power_kind(name: &str) -> Option<PowerKind> {
    let n = name.trim().to_ascii_uppercase();
    let n = n.trim_start_matches('+');
    if n.is_empty() || n == "UNASSIGNED" {
        return None;
    }
    if n.starts_with("GND") || matches!(n, "VSS" | "0V" | "EARTH" | "AGND" | "DGND" | "PGND") {
        return Some(PowerKind::Ground);
    }
    let digit_v = n.chars().next().is_some_and(|c| c.is_ascii_digit())
        && n.chars().any(|c| c == 'V')
        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '.');
    if n.starts_with("VCC")
        || n.starts_with("VDD")
        || matches!(n, "VBUS" | "VIN" | "VBAT" | "V")
        || digit_v
    {
        return Some(PowerKind::Rail);
    }
    None
}

/// Power kind of every net the sheet draws as a symbol: nets named like a
/// supply, plus nets a `power` label flags (a flagged net with an ordinary
/// name is a rail).
pub(super) fn power_nets(p: &Project) -> HashMap<Id, PowerKind> {
    let mut found = HashMap::new();
    for net in &p.nets {
        if let Some(kind) = power_kind(&net.name) {
            found.insert(net.id, kind);
        }
    }
    for label in p.labels.iter().filter(|l| l.kind == "power") {
        found
            .entry(label.net)
            .or_insert_with(|| power_kind(p.net_name(label.net)).unwrap_or(PowerKind::Rail));
    }
    found
}

/// Standard ISO landscape sheets, smallest first, in millimetres.
const SHEET_SIZES: [(&str, f64, f64); 8] = [
    ("A7", 105.0, 74.0),
    ("A6", 148.0, 105.0),
    ("A5", 210.0, 148.0),
    ("A4", 297.0, 210.0),
    ("A3", 420.0, 297.0),
    ("A2", 594.0, 420.0),
    ("A1", 841.0, 594.0),
    ("A0", 1189.0, 841.0),
];

/// The drawing frame of the schematic: where it sits, how it is zoned and
/// what its title block says.
#[derive(Clone, Debug)]
pub(super) struct Sheet {
    pub lo: Point,
    pub hi: Point,
    /// Width of the margin band that carries the zone letters and numbers.
    pub band: f64,
    pub cols: usize,
    pub rows: usize,
    pub size: String,
    pub title: String,
    /// The revision typed in Project settings, empty when none.
    pub revision: String,
    pub date: String,
    pub revision: String,
    pub company: String,
    pub drawn_by: String,
    /// False when the page switched its title block off.
    pub title_shown: bool,
    pub page: usize,
    pub pages: usize,
}

/// Everything the schematic draws, as one world rectangle, with room for the
/// reference text above, the value text below and the power symbols.
fn content_bounds(p: &Project) -> Option<(Point, Point)> {
    let mut pts: Vec<Point> = Vec::new();
    for c in p.components.iter().flat_map(|c| c.schematic_views()) {
        pts.push(c.position);
        for pin in &c.pins {
            pts.push(c.position.offset(pin.offset.rotate(-c.rotation)));
        }
        pts.extend(
            c.drawing_points()
                .iter()
                .map(|q| c.position.offset(q.rotate(-c.rotation))),
        );
    }
    for w in &p.wires {
        if let Some(path) = p.wire_path(w) {
            pts.extend(path);
        }
    }
    for w in &p.drawn_wires {
        pts.extend(w.points.iter().copied());
    }
    pts.extend(p.labels.iter().map(|l| l.position));
    pts.extend(p.ports.iter().map(|q| q.position));
    pts.extend(
        p.graphics
            .iter()
            .filter(|g| g.layer == "schematic")
            .flat_map(|g| g.points.iter().copied()),
    );
    let first = *pts.first()?;
    let (mut lo, mut hi) = (first, first);
    for q in &pts {
        lo = Point::new(lo.x.min(q.x), lo.y.min(q.y));
        hi = Point::new(hi.x.max(q.x), hi.y.max(q.y));
    }
    Some((
        Point::new(lo.x - 6.0, lo.y - 12.0),
        Point::new(hi.x + 6.0, hi.y + 14.0),
    ))
}

/// Title block width in millimetres for a sheet of this width.
fn title_block_width(sheet_width: f64) -> f64 {
    (sheet_width * 0.32).clamp(36.0, 110.0)
}

/// Today's date as `YYYY-MM-DD` in UTC, for a project that records none.
fn today() -> String {
let secs = crate::wall_clock::unix_secs_now();
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The sheet for a project: the smallest standard size that holds the drawing
/// with its title block, centred on the drawing. Title, date and size come
/// from the project's metadata keys `title`, `date` and `paper` when set.
///
/// # Rules
/// - Total: an empty schematic gets the smallest sheet around the origin
///   area, so a fresh project still shows a frame.
///
/// # Dependencies
/// - `crates/doc/src/lib.rs` for `Project::meta` and `Project::sheets`.
///
/// # Do not
/// - Move or resize anything in the project; the sheet follows the drawing.
pub(super) fn sheet_for(p: &Project, page: pcbcraft_doc::Id) -> Sheet {
    let settings = p.page_settings_of(page);
    let (clo, chi) = content_bounds(p).unwrap_or((Point::new(0.0, 0.0), Point::new(80.0, 48.0)));
    let (cw, ch) = (chi.x - clo.x, chi.y - clo.y);
    let fits = |w: f64, h: f64| {
        let band = (w * 0.02).clamp(3.0, 8.0);
        let tb_h = title_block_width(w) * 0.25;
        cw <= w - 2.0 * band && ch <= h - 2.0 * band - tb_h
    };
    let paper = Some(settings.size.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| p.meta.get("paper").map(|s| s.trim().to_ascii_uppercase()));
    let picked = paper
        .as_deref()
        .and_then(|name| SHEET_SIZES.iter().find(|s| s.0 == name))
        .or_else(|| SHEET_SIZES.iter().find(|s| fits(s.1, s.2)))
        .copied()
        .unwrap_or(SHEET_SIZES[SHEET_SIZES.len() - 1]);
    let (size, w, h) = picked;
    let band = (w * 0.02).clamp(3.0, 8.0);
    let tb_h = title_block_width(w) * 0.25;
    let cx = (clo.x + chi.x) / 2.0;
    let cy = (clo.y + chi.y) / 2.0;
    let lo = Point::new(
        (cx - w / 2.0).round(),
        (cy - band - (h - 2.0 * band - tb_h) / 2.0).round(),
    );
    let title = Some(settings.title.clone())
        .filter(|t| !t.is_empty())
        .or_else(|| {
            p.meta
                .get("title")
                .filter(|t| !t.trim().is_empty())
                .cloned()
        })
        .unwrap_or_else(|| p.name.clone());
    let number = if page == 0 {
        1
    } else {
        p.sheets
            .iter()
            .position(|s| s.id == page)
            .map_or(1, |i| i + 2)
    };
    Sheet {
        lo,
        hi: Point::new(lo.x + w, lo.y + h),
        band,
        cols: ((w / 50.0).round() as usize).clamp(4, 24),
        rows: ((h / 50.0).round() as usize).clamp(3, 16),
        size: size.to_string(),
        title,
revision: p
            .meta
            .get("revision")
            .map(|r| r.trim().to_owned())
            .unwrap_or_else(|| settings.revision.clone()),
        date: Some(settings.date.clone())
            .filter(|d| !d.is_empty())
            .or_else(|| p.meta.get("date").cloned())
            .unwrap_or_else(today),
        company: settings.company.clone(),
        drawn_by: settings.drawn_by.clone(),
        title_shown: !settings.title_hidden,
        page: number,
        pages: p.sheets.len() + 1,
    }
}

/// The world rectangle the fit button should show: the whole sheet.
pub(super) fn sheet_bounds(p: &Project) -> (Point, Point) {
    let s = sheet_for(p, 0);
    (s.lo, s.hi)
}

/// Paint the backdrop, the sheet, its frame with zone letters and numbers,
/// the grid dots and the title block.
///
/// # Rules
/// - Dots only inside the inner frame, one every 1.27 mm (2.54 mm and 5.08 mm
///   when that would be closer than 9 px) at a low contrast.
/// - Zone labels centre in each zone of the margin band; letters run down the
///   sides, numbers across the top and bottom.
///
/// # Do not
/// - Paint a symbol or a wire here; call it first.
pub(super) fn paint_sheet(
    painter: &egui::Painter,
    rect: Rect,
    camera: Camera,
    sheet: &Sheet,
    sc: &SheetColours,
    dots: bool,
) {
    painter.rect_filled(rect, 0.0, sc.backdrop);
    let outer = Rect::from_two_pos(camera.screen(sheet.lo, rect), camera.screen(sheet.hi, rect));
    let band = sheet.band as f32 * camera.zoom;
    let inner = outer.shrink(band);
    painter.rect_filled(outer, 0.0, sc.sheet);
    let line = Stroke::new(1.0, sc.frame);
    painter.rect_stroke(outer, 0.0, line, egui::StrokeKind::Inside);
    painter.rect_stroke(inner, 0.0, line, egui::StrokeKind::Inside);
    if dots {
        paint_dots(painter, rect, camera, sheet, inner, sc);
    }
    let font = FontId::monospace((band * 0.55).clamp(8.0, 11.0));
    for i in 0..sheet.cols {
        let x0 = inner.left() + inner.width() * i as f32 / sheet.cols as f32;
        let x1 = inner.left() + inner.width() * (i + 1) as f32 / sheet.cols as f32;
        if i > 0 {
            painter.line_segment(
                [Pos2::new(x0, outer.top()), Pos2::new(x0, inner.top())],
                line,
            );
            painter.line_segment(
                [Pos2::new(x0, inner.bottom()), Pos2::new(x0, outer.bottom())],
                line,
            );
        }
        let label = (i + 1).to_string();
        let mid = (x0 + x1) / 2.0;
        painter.text(
            Pos2::new(mid, outer.top() + band / 2.0),
            Align2::CENTER_CENTER,
            &label,
            font.clone(),
            sc.zone_text,
        );
        painter.text(
            Pos2::new(mid, outer.bottom() - band / 2.0),
            Align2::CENTER_CENTER,
            &label,
            font.clone(),
            sc.zone_text,
        );
    }
    for j in 0..sheet.rows {
        let y0 = inner.top() + inner.height() * j as f32 / sheet.rows as f32;
        let y1 = inner.top() + inner.height() * (j + 1) as f32 / sheet.rows as f32;
        if j > 0 {
            painter.line_segment(
                [Pos2::new(outer.left(), y0), Pos2::new(inner.left(), y0)],
                line,
            );
            painter.line_segment(
                [Pos2::new(inner.right(), y0), Pos2::new(outer.right(), y0)],
                line,
            );
        }
        let letter = char::from_u32(65 + j as u32).unwrap_or('?').to_string();
        let mid = (y0 + y1) / 2.0;
        painter.text(
            Pos2::new(outer.left() + band / 2.0, mid),
            Align2::CENTER_CENTER,
            &letter,
            font.clone(),
            sc.zone_text,
        );
        painter.text(
            Pos2::new(outer.right() - band / 2.0, mid),
            Align2::CENTER_CENTER,
            &letter,
            font.clone(),
            sc.zone_text,
        );
    }
    if sheet.title_shown {
        paint_title_block(painter, camera, sheet, inner, sc);
    }
}

/// Grid dots inside the inner frame, clipped to the visible canvas.
fn paint_dots(
    painter: &egui::Painter,
    rect: Rect,
    camera: Camera,
    sheet: &Sheet,
    inner: Rect,
    sc: &SheetColours,
) {
    let mut step_mm = 1.27_f32;
    while step_mm * camera.zoom < 9.0 && step_mm < 10.0 {
        step_mm *= 2.0;
    }
    let step = step_mm * camera.zoom;
    let area = inner.shrink(1.0).intersect(rect);
    if area.width() <= 0.0 || area.height() <= 0.0 || step < 2.0 {
        return;
    }
    let origin = camera.screen(
        Point::new(sheet.lo.x + sheet.band, sheet.lo.y + sheet.band),
        rect,
    );
    let x0 = area.left() + (origin.x - area.left()).rem_euclid(step);
    let y0 = area.top() + (origin.y - area.top()).rem_euclid(step);
    let cols = ((area.width() / step) as usize + 1).min(1600);
    let rows = ((area.height() / step) as usize + 1).min(1000);
    for i in 0..cols {
        for j in 0..rows {
            let at = Pos2::new(x0 + i as f32 * step, y0 + j as f32 * step);
            if area.contains(at) {
                painter.circle_filled(at, 0.8, sc.dots);
            }
        }
    }
}

/// The title block in the lower right corner of the inner frame: title on top,
/// then sheet number, date and size in three cells.
fn paint_title_block(
    painter: &egui::Painter,
    camera: Camera,
    sheet: &Sheet,
    inner: Rect,
    sc: &SheetColours,
) {
    let width = title_block_width(sheet.hi.x - sheet.lo.x) as f32 * camera.zoom;
    let height = width * 0.25;
    let block = Rect::from_min_max(
        Pos2::new(inner.right() - width, inner.bottom() - height),
        inner.right_bottom(),
    );
    let line = Stroke::new(1.0, sc.frame);
    painter.rect_filled(block, 0.0, sc.sheet);
    painter.rect_stroke(block, 0.0, line, egui::StrokeKind::Inside);
    let split = block.top() + height * 0.56;
    painter.line_segment(
        [
            Pos2::new(block.left(), split),
            Pos2::new(block.right(), split),
        ],
        line,
    );
    let cells = [
        block.left(),
        block.left() + width * 0.3,
        block.left() + width * 0.5,
        block.left() + width * 0.75,
        block.right(),
    ];
    for x in &cells[1..4] {
        painter.line_segment([Pos2::new(*x, split), Pos2::new(*x, block.bottom())], line);
    }
    let caption = FontId::monospace((height * 0.11).clamp(6.5, 10.0));
    let value = FontId::monospace((height * 0.15).clamp(8.0, 13.0));
    let pad = (height * 0.07).clamp(3.0, 9.0);
    let place = |origin: Pos2, name: String, text: &str, font: &FontId| {
        painter.text(
            origin + Vec2::new(pad, pad * 0.6),
            Align2::LEFT_TOP,
            name,
            caption.clone(),
            sc.muted,
        );
        painter.text(
            origin + Vec2::new(pad, pad * 0.6 + caption.size + pad * 0.5),
            Align2::LEFT_TOP,
            text,
            font.clone(),
            sc.text,
        );
    };
    place(
        block.left_top(),
        t!("schematic.sheet.title").to_uppercase(),
        &sheet.title,
        &FontId::monospace((height * 0.22).clamp(9.0, 19.0)),
    );
    if !sheet.revision.is_empty() {
        painter.text(
            block.right_top() + Vec2::new(-pad, pad * 0.6),
            Align2::RIGHT_TOP,
            format!("{} {}", t!("schematic.sheet.revision"), sheet.revision),
            caption.clone(),
            sc.muted,
        );
    }
    place(
        Pos2::new(cells[0], split),
        t!("schematic.sheet.sheet").to_uppercase(),
        &t!(
            "schematic.sheet.page",
            page = sheet.page,
            pages = sheet.pages
        ),
        &value,
    );
    place(
        Pos2::new(cells[1], split),
        t!("schematic.sheet.revision").to_uppercase(),
        &sheet.revision,
        &value,
    );
    place(
        Pos2::new(cells[2], split),
        t!("schematic.sheet.date").to_uppercase(),
        &sheet.date,
        &value,
    );
    let credit = [sheet.company.as_str(), sheet.drawn_by.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" / ");
    if !credit.is_empty() {
        painter.text(
            Pos2::new(block.right() - pad, block.top() + pad * 0.6),
            Align2::RIGHT_TOP,
            credit,
            caption.clone(),
            sc.muted,
        );
    }
    place(
        Pos2::new(cells[3], split),
        t!("schematic.sheet.size").to_uppercase(),
        &sheet.size,
        &value,
    );
}

/// The unit direction a pin leaves its symbol in, snapped to an axis, in
/// world coordinates (y grows downward).
fn pin_outward(p: &Project, r: &pcbcraft_doc::PinRef) -> Option<Point> {
    let pin = p.pin(r)?;
    let local = match pin.direction {
        Some(angle) => Point::new(-angle.to_radians().cos(), angle.to_radians().sin()),
        None => pin.offset,
    };
    let (_, rotation) = p.component(r.component)?.unit_placement(pin.unit)?;
    let d = local.rotate(-rotation);
    Some(if d.x.abs() >= d.y.abs() {
        Point::new(d.x.signum(), 0.0)
    } else {
        Point::new(0.0, d.y.signum())
    })
}

/// Paint a ground symbol hanging from `top`: three bars, shortest at the
/// bottom, with the net name under them.
fn ground_symbol(
    painter: &egui::Painter,
    rect: Rect,
    camera: Camera,
    top: Point,
    name: &str,
    sc: &SheetColours,
) {
    let stroke = Stroke::new(2.5, sc.wire);
    for (i, half) in [1.8, 1.2, 0.6].into_iter().enumerate() {
        let y = top.y + 0.9 * i as f64;
        painter.line_segment(
            [
                camera.screen(Point::new(top.x - half, y), rect),
                camera.screen(Point::new(top.x + half, y), rect),
            ],
            stroke,
        );
    }
    painter.text(
        camera.screen(Point::new(top.x, top.y + 2.4), rect),
        Align2::CENTER_TOP,
        name,
        FontId::monospace(12.0),
        sc.wire,
    );
}

/// Paint a supply symbol standing on `base`: a stem up, a bar across it and
/// the net name above.
fn rail_symbol(
    painter: &egui::Painter,
    rect: Rect,
    camera: Camera,
    base: Point,
    name: &str,
    sc: &SheetColours,
) {
    let stroke = Stroke::new(2.5, sc.wire);
    let top = Point::new(base.x, base.y - 5.0);
    painter.line_segment(
        [camera.screen(base, rect), camera.screen(top, rect)],
        stroke,
    );
    painter.line_segment(
        [
            camera.screen(Point::new(top.x - 1.6, top.y), rect),
            camera.screen(Point::new(top.x + 1.6, top.y), rect),
        ],
        stroke,
    );
    painter.text(
        camera.screen(Point::new(top.x, top.y - 0.8), rect),
        Align2::CENTER_BOTTOM,
        name,
        FontId::monospace(12.0),
        sc.wire,
    );
}

/// A filled junction dot where three or more wire ends meet.
fn junction(painter: &egui::Painter, rect: Rect, camera: Camera, at: Point, sc: &SheetColours) {
    painter.circle_filled(camera.screen(at, rect), 4.5, sc.wire);
}

/// The longest horizontal run of a path, or the longest run when none is
/// horizontal, as its two end points.
pub(super) fn longest_run(path: &[Point]) -> Option<(Point, Point)> {
    let run = |a: &Point, b: &Point| a.distance(*b);
    let horizontal = |a: &Point, b: &Point| (a.y - b.y).abs() < 1e-6;
    path.windows(2)
        .filter(|w| horizontal(&w[0], &w[1]))
        .max_by(|a, b| run(&a[0], &a[1]).total_cmp(&run(&b[0], &b[1])))
        .or_else(|| {
            path.windows(2)
                .max_by(|a, b| run(&a[0], &a[1]).total_cmp(&run(&b[0], &b[1])))
        })
        .map(|w| (w[0], w[1]))
}

/// Where a net label sits on a wire: the point and whether the run is
/// horizontal. `fraction` is how far along the run, so a supply symbol and
/// its wire label never land on the same spot.
pub(super) fn run_point(run: (Point, Point), fraction: f64) -> Point {
    Point::new(
        run.0.x + (run.1.x - run.0.x) * fraction,
        run.0.y + (run.1.y - run.0.y) * fraction,
    )
}

/// Paint ground stubs and symbols, supply symbols and junction dots for the
/// power nets and for every pin that two or more wires share.
///
/// # Rules
/// - A ground net draws no wire: each pin gets a short stub ending in a ground
///   symbol. The caller skips those wires.
/// - A supply net keeps its wire and gets one symbol on its first wire, 45 %
///   along the longest horizontal run, with a junction dot where it meets.
/// - A `power` label draws its own symbol at the label position.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas/sheet.rs` (`power_nets`) for the classification and
///   `crates/doc/src/lib.rs` (`Project::wire_path`) for the run a supply symbol attaches to.
///
/// # Do not
/// - Draw a wire the caller still paints.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_power(
    painter: &egui::Painter,
    rect: Rect,
    camera: Camera,
    p: &Project,
    kinds: &HashMap<Id, PowerKind>,
    sc: &SheetColours,
    show_wires: bool,
    page: Id,
) {
    let stub = Stroke::new(2.5, sc.wire);
    let mut ends: HashMap<(i64, i64), usize> = HashMap::new();
    let key = |q: Point| ((q.x * 100.0).round() as i64, (q.y * 100.0).round() as i64);
    let mut ruled: Vec<Id> = Vec::new();
    for w in p
        .wires
        .iter()
        .filter(|w| show_wires && p.on_schematic_page(page, w.id))
    {
        match kinds.get(&w.net) {
            Some(PowerKind::Ground) => {
                let name = p.net_name(w.net);
                for r in [&w.from, &w.to] {
                    let (Some(pin), Some(out)) = (p.pin_position(r), pin_outward(p, r)) else {
                        continue;
                    };
                    let corner = Point::new(pin.x + out.x * 4.0, pin.y + out.y * 4.0);
                    let mut path = vec![pin, corner];
                    let end = if out.y > 0.5 {
                        corner
                    } else {
                        let down = Point::new(corner.x, corner.y + 4.0);
                        path.push(down);
                        down
                    };
                    painter.add(egui::Shape::line(
                        path.iter().map(|q| camera.screen(*q, rect)).collect(),
                        stub,
                    ));
                    ground_symbol(painter, rect, camera, end, name, sc);
                }
            }
            Some(PowerKind::Rail) => {
                *ends
                    .entry(key(p.pin_position(&w.from).unwrap_or_default()))
                    .or_default() += 1;
                *ends
                    .entry(key(p.pin_position(&w.to).unwrap_or_default()))
                    .or_default() += 1;
                if !ruled.contains(&w.net)
                    && !p.labels.iter().any(|l| l.kind == "power" && l.net == w.net)
                    && let Some(run) = p.wire_path(w).as_deref().and_then(longest_run)
                {
                    ruled.push(w.net);
                    let at = run_point(run, 0.45);
                    rail_symbol(painter, rect, camera, at, p.net_name(w.net), sc);
                    junction(painter, rect, camera, at, sc);
                }
            }
            None => {
                *ends
                    .entry(key(p.pin_position(&w.from).unwrap_or_default()))
                    .or_default() += 1;
                *ends
                    .entry(key(p.pin_position(&w.to).unwrap_or_default()))
                    .or_default() += 1;
            }
        }
    }
    for (spot, count) in ends {
        if count >= 2 {
            junction(
                painter,
                rect,
                camera,
                Point::new(spot.0 as f64 / 100.0, spot.1 as f64 / 100.0),
                sc,
            );
        }
    }
    for l in p
        .labels
        .iter()
        .filter(|l| l.kind == "power" && p.on_schematic_page(page, l.id))
    {
        let name = p.net_name(l.net);
        match kinds.get(&l.net) {
            Some(PowerKind::Ground) => ground_symbol(painter, rect, camera, l.position, name, sc),
            _ => rail_symbol(painter, rect, camera, l.position, name, sc),
        }
    }
}

/// Paint the selection box around a symbol: a dashed outline and a square
/// handle on each corner. `bounds` is the screen rectangle to enclose.
pub(super) fn paint_selection(painter: &egui::Painter, bounds: Rect, sc: &SheetColours) {
    let stroke = Stroke::new(1.5, sc.select);
    painter.add(egui::Shape::dashed_line(
        &[
            bounds.left_top(),
            bounds.right_top(),
            bounds.right_bottom(),
            bounds.left_bottom(),
            bounds.left_top(),
        ],
        stroke,
        6.0,
        4.0,
    ));
    for corner in [
        bounds.left_top(),
        bounds.right_top(),
        bounds.right_bottom(),
        bounds.left_bottom(),
    ] {
        let handle = Rect::from_center_size(corner, Vec2::splat(9.0));
        painter.rect_filled(handle, 0.0, sc.sheet);
        painter.rect_stroke(handle, 0.0, stroke, egui::StrokeKind::Inside);
    }
}

/// What a hit outline belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HitKind {
    Symbol,
    Pin,
    Wire,
    NetLabel,
    Junction,
}

/// The screen outline of one drawn object that a click would act on.
#[derive(Clone, Copy, Debug)]
pub(super) struct HitOutline {
    pub kind: HitKind,
    pub bounds: Rect,
}

/// Every actionable object of the schematic as a screen outline: symbol body
/// boxes, pin ends, wire runs, net labels and junctions.
///
/// Why: one list answers both "what is under the pointer" and "what should
/// glow", so the hover highlight cannot drift from what is drawn.
///
/// # Rules
/// - A ground wire is not listed as a wire (it is drawn as stubs); its pins
///   are listed as pins.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas/sheet.rs` (`hover_outline`, below) and `crates/ui-egui/src/canvas.rs` (`paint_schematic`).
///
/// # Do not
/// - Paint here, or read the pointer.
pub(super) fn hit_outlines(
    p: &Project,
    rect: Rect,
    camera: Camera,
    kinds: &HashMap<Id, PowerKind>,
) -> Vec<HitOutline> {
    let mut out = Vec::new();
    let dot = |at: Point, half: f32| {
        Rect::from_center_size(camera.screen(at, rect), Vec2::splat(half * 2.0))
    };
    for c in p.components.iter().flat_map(|c| c.schematic_views()) {
        let mut lo = c.position;
        let mut hi = c.position;
        for pin in &c.pins {
            let at = c.position.offset(pin.offset.rotate(-c.rotation));
            lo = Point::new(lo.x.min(at.x), lo.y.min(at.y));
            hi = Point::new(hi.x.max(at.x), hi.y.max(at.y));
            out.push(HitOutline {
                kind: HitKind::Pin,
                bounds: dot(at, 6.0),
            });
        }
        let body = Rect::from_two_pos(camera.screen(lo, rect), camera.screen(hi, rect));
        out.push(HitOutline {
            kind: HitKind::Symbol,
            bounds: body.expand2(Vec2::new(4.0, 2.5 * camera.zoom)),
        });
    }
    for w in &p.wires {
        if kinds.get(&w.net) == Some(&PowerKind::Ground) {
            continue;
        }
        let Some(path) = p.wire_path(w) else { continue };
        for pair in path.windows(2) {
            out.push(HitOutline {
                kind: HitKind::Wire,
                bounds: Rect::from_two_pos(
                    camera.screen(pair[0], rect),
                    camera.screen(pair[1], rect),
                )
                .expand(4.0),
            });
        }
    }
    for l in &p.labels {
        let kind = if l.kind == "junction" {
            HitKind::Junction
        } else {
            HitKind::NetLabel
        };
        let half = if kind == HitKind::Junction { 6.0 } else { 14.0 };
        out.push(HitOutline {
            kind,
            bounds: dot(l.position, half),
        });
    }
    out
}

/// Outline the smallest object under the pointer with a pulsing accent
/// border, so a person can see it is interactive.
///
/// This is the single swap point for the shared `hover_affordance` API: when
/// it lands, replace the body with a call to it using the same `bounds`.
///
/// # Rules
/// - Does nothing when the pointer is off the canvas or over nothing.
///
/// # Do not
/// - Change selection or any project state.
pub(super) fn hover_outline(painter: &egui::Painter, outlines: &[HitOutline], accent: Color32) {
    let ctx = painter.ctx();
    let Some(pointer) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let Some(hit) = outlines
        .iter()
        .filter(|o| o.bounds.contains(pointer))
        .min_by(|a, b| a.bounds.area().total_cmp(&b.bounds.area()))
    else {
        return;
    };
    let time = ctx.input(|i| i.time) as f32;
    let pulse = 0.5 + 0.5 * (time * 4.0).sin();
    // Pins and junctions glow as discs; the rest as rounded boxes.
    let round = if matches!(hit.kind, HitKind::Pin | HitKind::Junction) {
        hit.bounds.height() / 2.0
    } else {
        3.0
    };
    painter.rect_stroke(
        hit.bounds.expand(2.0),
        round + 2.0,
        Stroke::new(4.0, accent.gamma_multiply(0.12 + 0.16 * pulse)),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        hit.bounds,
        round,
        Stroke::new(1.5, accent.gamma_multiply(0.7 + 0.3 * pulse)),
        egui::StrokeKind::Inside,
    );
    ctx.request_repaint();
}
