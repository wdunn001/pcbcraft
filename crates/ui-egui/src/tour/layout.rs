//! Card and spotlight hole geometry shared by the overlay (#770) and CI replay.
//!
//! Pure math: no widgets, no platform code.

use egui::{Align2, Pos2, Rect, Vec2};

const CARD_MIN: Vec2 = Vec2::new(280.0, 120.0);
const CARD_MAX: Vec2 = Vec2::new(360.0, 220.0);
const GAP: f32 = 12.0;
const HOLE_PAD: f32 = 8.0;

/// Where the card and cutout land for one step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StepLayout {
    pub card: Rect,
    pub hole: Option<Rect>,
}

/// Place the card beside the anchor (or centered when there is no target).
pub(crate) fn step_layout(
    screen: Rect,
    target: &str,
    anchor: Option<Rect>,
    placement: &str,
) -> StepLayout {
    let card_size = CARD_MIN;
    let hole = match (target, anchor) {
        ("none", _) | (_, None) => None,
        (_, Some(r)) => Some(r.expand(HOLE_PAD).intersect(screen)),
    };
    let card = if target == "none" || anchor.is_none() {
        Align2::CENTER_CENTER
            .align_size_within_rect(card_size, screen)
            .shrink(24.0)
    } else {
        place_card(screen, anchor.unwrap(), placement, card_size)
    };
    StepLayout { card, hole }
}

fn place_card(screen: Rect, anchor: Rect, placement: &str, size: Vec2) -> Rect {
    let size = size.min(CARD_MAX);
    let pick = |p: &str| match p {
        "top" => card_above(screen, anchor, size),
        "bottom" => card_below(screen, anchor, size),
        "left" => card_left(screen, anchor, size),
        "right" => card_right(screen, anchor, size),
        _ => {
            let above = card_above(screen, anchor, size);
            if above.min.y >= screen.min.y {
                return above;
            }
            let below = card_below(screen, anchor, size);
            if below.max.y <= screen.max.y {
                return below;
            }
            card_right(screen, anchor, size)
        }
    };
    let rect = pick(placement);
    rect.intersect(screen)
}

fn card_above(screen: Rect, anchor: Rect, size: Vec2) -> Rect {
    let y = anchor.min.y - GAP - size.y;
    Rect::from_min_size(
        Pos2::new(anchor.center().x - size.x * 0.5, y),
        size,
    )
}

fn card_below(screen: Rect, anchor: Rect, size: Vec2) -> Rect {
    let y = anchor.max.y + GAP;
    Rect::from_min_size(
        Pos2::new(anchor.center().x - size.x * 0.5, y),
        size,
    )
}

fn card_left(screen: Rect, anchor: Rect, size: Vec2) -> Rect {
    let x = anchor.min.x - GAP - size.x;
    Rect::from_min_size(Pos2::new(x, anchor.center().y - size.y * 0.5), size)
}

fn card_right(screen: Rect, anchor: Rect, size: Vec2) -> Rect {
    let x = anchor.max.x + GAP;
    Rect::from_min_size(Pos2::new(x, anchor.center().y - size.y * 0.5), size)
}
