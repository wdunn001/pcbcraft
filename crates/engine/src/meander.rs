//! Orthogonal meander bumps shared by length tuning and differential skew tuning.
use crate::Error;
use pcbcraft_doc::{Id, Layer, Project, Track};
use pcbcraft_geom::Point;

pub(crate) const MAX_BUMPS: usize = 32;
const MIN_AMPLITUDE: f64 = 0.05;
const MAX_AMPLITUDE: f64 = 5.0;
const MIN_SPAN: f64 = 0.2;
const MAX_SPAN: f64 = 10.0;
const END_MARGIN: f64 = 0.15;

pub(crate) fn net_routed_length(p: &Project, net: Id) -> f64 {
    p.board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .map(|t| t.length())
        .sum()
}

fn unit_along(start: Point, end: Point) -> Option<(f64, f64)> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 1e-6 {
        return None;
    }
    Some((dx / len, dy / len))
}

fn point_on_segment(start: Point, end: Point, t: f64) -> Point {
    Point::new(start.x + (end.x - start.x) * t, start.y + (end.y - start.y) * t)
}

/// Extra copper length one rectangular bump adds compared with a straight run.
pub(crate) fn bump_extra_length(amplitude: f64) -> f64 {
    2.0 * amplitude
}

pub(crate) fn pick_longest_track<'a>(
    p: &'a Project,
    net: Id,
    layer: Layer,
) -> Option<(usize, &'a Track)> {
    p.board
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.net == net && t.layer == layer && t.arc.is_none())
        .max_by(|(_, a), (_, b)| {
            a.length()
                .partial_cmp(&b.length())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, t)| (i, t))
}

pub(crate) fn outward_from_partner(
    track: &Track,
    partner_center: Point,
) -> Result<(f64, f64), Error> {
    let mid = point_on_segment(track.start, track.end, 0.5);
    let Some((ux, uy)) = unit_along(track.start, track.end) else {
        return Err(Error::Invalid(
            "meander needs a straight track with positive length".into(),
        ));
    };
    let perp_a = (-uy, ux);
    let perp_b = (uy, -ux);
    let away = (mid.x - partner_center.x, mid.y - partner_center.y);
    let dot_a = perp_a.0 * away.0 + perp_a.1 * away.1;
    let (px, py) = if dot_a >= 0.0 { perp_a } else { perp_b };
    let plen = (px * px + py * py).sqrt();
    if plen <= 1e-9 {
        return Err(Error::Invalid(
            "could not pick a side for the meander bump".into(),
        ));
    }
    Ok((px / plen, py / plen))
}

pub(crate) fn partner_center_on_layer(p: &Project, partner_net: Id, layer: Layer) -> Option<Point> {
    let mut count = 0usize;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for t in p.board.tracks.iter().filter(|t| t.net == partner_net && t.layer == layer) {
        let mid = point_on_segment(t.start, t.end, 0.5);
        sum_x += mid.x;
        sum_y += mid.y;
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some(Point::new(sum_x / count as f64, sum_y / count as f64))
}

fn clamp_meander_dims(segment_len: f64, amplitude: f64, span: f64) -> Result<(f64, f64), Error> {
    if segment_len < MIN_SPAN + 2.0 * END_MARGIN {
        return Err(Error::Invalid(
            "no track segment is long enough for a meander bump".into(),
        ));
    }
    let amp = amplitude.clamp(MIN_AMPLITUDE, MAX_AMPLITUDE);
    let max_span = (segment_len - 2.0 * END_MARGIN).min(MAX_SPAN);
    if max_span < MIN_SPAN {
        return Err(Error::Invalid(
            "no track segment is long enough for a meander bump".into(),
        ));
    }
    let sp = span.clamp(MIN_SPAN, max_span);
    Ok((amp, sp))
}

/// Replace the middle of one straight track with a single rectangular bump.
pub(crate) fn push_bump_on_track(
    p: &mut Project,
    next: &mut Id,
    track_index: usize,
    outward: (f64, f64),
    amplitude: f64,
    span: f64,
) -> Result<Vec<Id>, Error> {
    let track = p.board.tracks.get(track_index).ok_or_else(|| {
        Error::Invalid("meander target track is missing".into())
    })?;
    if track.arc.is_some() {
        return Err(Error::Invalid(
            "meander bumps need straight tracks; straighten curves first".into(),
        ));
    }
    let (amp, sp) = clamp_meander_dims(track.length(), amplitude, span)?;
    let (ux, uy) = unit_along(track.start, track.end).ok_or_else(|| {
        Error::Invalid("meander needs a straight track with positive length".into())
    })?;
    let half = sp / 2.0;
    let center = point_on_segment(track.start, track.end, 0.5);
    let p1 = Point::new(center.x - ux * half, center.y - uy * half);
    let p2 = Point::new(center.x + ux * half, center.y + uy * half);
    if p1.distance(track.start) < END_MARGIN || track.end.distance(p2) < END_MARGIN {
        return Err(Error::Invalid(
            "track ends are too close for a meander bump".into(),
        ));
    }
    let (ox, oy) = outward;
    let top1 = Point::new(p1.x + ox * amp, p1.y + oy * amp);
    let top2 = Point::new(p2.x + ox * amp, p2.y + oy * amp);
    let (net, width, layer, start, end) = {
        let t = &p.board.tracks[track_index];
        (t.net, t.width, t.layer, t.start, t.end)
    };
    let leg_specs = [(start, p1), (p1, top1), (top1, top2), (top2, p2), (p2, end)];
    let mut ids = Vec::with_capacity(leg_specs.len());
    for (i, (from, to)) in leg_specs.into_iter().enumerate() {
        if from.distance(to) <= 1e-6 {
            continue;
        }
        let id = crate::allocate(next)?;
        ids.push(id);
        let seg = Track {
            id,
            net,
            start: from,
            end: to,
            width,
            layer,
            arc: None,
        };
        if i == 0 {
            p.board.tracks[track_index] = seg;
        } else {
            p.board.tracks.push(seg);
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Board;

    #[test]
    fn bump_extra_length_is_twice_amplitude() {
        assert!((bump_extra_length(0.4) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn outward_points_away_from_partner() -> Result<(), Error> {
        let track = Track {
            id: 1,
            net: 1,
            start: Point::new(0.0, 0.0),
            end: Point::new(10.0, 0.0),
            width: 0.2,
            layer: Layer::Front,
            arc: None,
        };
        let partner = Point::new(5.0, -1.0);
        let (ox, oy) = outward_from_partner(&track, partner)?;
        assert!(oy > 0.0, "bump should move away from partner below");
        assert!(ox.abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn push_bump_splits_one_track_into_five() -> Result<(), Error> {
        let mut p = Project::default();
        p.board = Board {
            tracks: vec![Track {
                id: 10,
                net: 2,
                start: Point::new(0.0, 10.0),
                end: Point::new(20.0, 10.0),
                width: 0.2,
                layer: Layer::Front,
                arc: None,
            }],
            ..Board::default()
        };
        let mut next = 11_u64;
        let before_len = net_routed_length(&p, 2);
        push_bump_on_track(&mut p, &mut next, 0, (0.0, 1.0), 0.5, 2.0)?;
        let after_len = net_routed_length(&p, 2);
        assert!((after_len - before_len - 1.0).abs() < 1e-3);
        assert_eq!(p.board.tracks.len(), 5);
        Ok(())
    }
}
