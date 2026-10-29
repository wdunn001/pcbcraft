//! The physical layer stack as the 2D canvas sees it: paint order, see-through
//! fading and stepping.
//!
//! Context: `draw_target.rs` names the targets; `layers_tree.rs` reports the row
//! under the pointer; `canvas.rs` asks [`StackView`] how strongly to paint each
//! layer. The stack runs from the top of the board to the bottom: top silkscreen,
//! top mask, top paste, every copper layer, bottom paste, bottom mask, bottom
//! silkscreen. The same list serves painting, hover preview and Page Up and Page Down.
//! Local rules: nothing here writes the document; the view is a display preference.
use crate::draw_target::{Side, Target};
use pcbcraft_doc::Layer;

/// Strength of a layer between the viewer and the focused layer.
pub(crate) const NEAR_ALPHA: f32 = 0.25;
/// Strength of a layer behind the focused layer.
pub(crate) const FAR_ALPHA: f32 = 0.5;

/// Every target in physical order, top of the board first.
///
/// Why: one list is the single answer to "what is above what".
///
/// # Rules
/// - `copper` is the copper layer count; inner layers keep their index order.
///
/// # Do not
/// - Add Edge or user layers; they have no place in the stack.
pub(crate) fn levels(copper: u8) -> Vec<Target> {
    let mut out = vec![
        Target::Silk(Side::Top),
        Target::Mask(Side::Top),
        Target::Paste(Side::Top),
    ];
    for i in 0..copper.max(2) {
        if let Ok(layer) = Layer::from_index(i) {
            out.push(Target::Copper(layer));
        }
    }
    out.extend([
        Target::Paste(Side::Bottom),
        Target::Mask(Side::Bottom),
        Target::Silk(Side::Bottom),
    ]);
    out
}

/// Where a target sits in [`levels`], or none for Edge and user layers.
pub(crate) fn position(copper: u8, target: Target) -> Option<usize> {
    levels(copper).iter().position(|t| *t == target)
}

/// The copper layer a part mounted on `side` has its pads on.
///
/// Why: a part's side is stored as the legacy two-layer name, so on a four-layer
/// board the bottom is not index 1.
pub(crate) fn mount_copper(copper: u8, side: Layer) -> Layer {
    if side == Layer::Back {
        Layer::from_index(copper.max(2) - 1).unwrap_or(Layer::Back)
    } else {
        side
    }
}

/// How the canvas paints each level this frame.
///
/// # Rules
/// - `alpha[i]` belongs to `levels(copper)[i]`; with no focus every level is 1.
/// - Not flipped, the top is nearest the viewer; flipped, the bottom is.
///
/// # Do not
/// - Hold it across frames; the fade values are animated by [`StackView::new`].
pub(crate) struct StackView {
    pub copper: u8,
    pub flipped: bool,
    pub alpha: Vec<f32>,
}

impl StackView {
    /// Fully opaque, nothing focused; used where no board is shown.
    pub(crate) fn plain(copper: u8, flipped: bool) -> Self {
        Self {
            copper,
            flipped,
            alpha: vec![1.0; levels(copper).len()],
        }
    }

    /// The strength each level should have for `focus`, before animation.
    pub(crate) fn targets(copper: u8, flipped: bool, focus: Option<Target>) -> Vec<f32> {
        let all = levels(copper);
        let at = focus.and_then(|t| position(copper, t));
        let n = all.len();
        let depth = |p: usize| if flipped { n - 1 - p } else { p };
        all.iter()
            .enumerate()
            .map(|(p, _)| match at {
                None => 1.0,
                Some(f) if depth(p) < depth(f) => NEAR_ALPHA,
                Some(f) if depth(p) == depth(f) => 1.0,
                Some(_) => FAR_ALPHA,
            })
            .collect()
    }

    /// Strength of a target's level; 1 for anything outside the stack.
    pub(crate) fn of(&self, target: Target) -> f32 {
        position(self.copper, target)
            .and_then(|i| self.alpha.get(i).copied())
            .unwrap_or(1.0)
    }

    /// Strength of a copper layer.
    pub(crate) fn copper_alpha(&self, layer: Layer) -> f32 {
        self.of(Target::Copper(layer))
    }

    /// Strength of the silkscreen on a side.
    pub(crate) fn silk_alpha(&self, bottom: bool) -> f32 {
        self.of(Target::Silk(if bottom { Side::Bottom } else { Side::Top }))
    }

    /// Paint order key: bigger is farther from the viewer, so paint it first.
    pub(crate) fn far(&self, layer: Layer) -> usize {
        let n = levels(self.copper).len();
        let p = position(self.copper, Target::Copper(layer)).unwrap_or(0);
        if self.flipped { n - 1 - p } else { p }
    }

    /// Sort items far to near by their copper layer.
    pub(crate) fn far_to_near<'a, T>(
        &self,
        items: impl Iterator<Item = &'a T>,
        layer_of: impl Fn(&T) -> Layer,
    ) -> Vec<&'a T> {
        let mut list: Vec<&T> = items.collect();
        list.sort_by_key(|i| std::cmp::Reverse(self.far(layer_of(i))));
        list
    }
}

/// The level one step toward or away from the viewer.
///
/// Why: Page Up and Page Down walk the stack one physical level at a time.
///
/// # Rules
/// - Toward the viewer is up the page for the top view and down the stack when flipped.
/// - Stops at the ends; returns none when already there or when `from` is not in the stack.
pub(crate) fn step(copper: u8, from: Target, flipped: bool, toward_viewer: bool) -> Option<Target> {
    let all = levels(copper);
    let at = all.iter().position(|t| *t == from)?;
    let up = toward_viewer != flipped;
    let next = if up { at.checked_sub(1)? } else { at + 1 };
    all.get(next).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stack_runs_from_top_silk_to_bottom_silk() {
        let all = levels(4);
        assert_eq!(all.len(), 3 + 4 + 3);
        assert_eq!(all.first(), Some(&Target::Silk(Side::Top)));
        assert_eq!(all.last(), Some(&Target::Silk(Side::Bottom)));
    }

    #[test]
    fn layers_above_the_focus_are_faint_and_those_behind_are_dim() {
        let focus = Some(Target::Copper(Layer::Front));
        let a = StackView::targets(2, false, focus);
        assert_eq!(
            a[0], NEAR_ALPHA,
            "top silkscreen is between viewer and copper"
        );
        let copper = position(2, Target::Copper(Layer::Front)).unwrap_or(0);
        assert_eq!(a[copper], 1.0);
        assert_eq!(a[copper + 1], FAR_ALPHA, "bottom copper is behind");
        let flipped = StackView::targets(2, true, focus);
        assert_eq!(
            flipped[0], FAR_ALPHA,
            "flipped, the top silkscreen is behind"
        );
    }

    #[test]
    fn stepping_goes_one_level_and_stops_at_the_ends() {
        let up = step(2, Target::Copper(Layer::Front), false, true);
        assert_eq!(up, Some(Target::Paste(Side::Top)));
        assert_eq!(step(2, Target::Silk(Side::Top), false, true), None);
        let flipped = step(2, Target::Mask(Side::Bottom), true, true);
        assert_eq!(flipped, Some(Target::Silk(Side::Bottom)));
    }
}
