//! Text search over an extracted layer (spike helper).
//!
//! Context: the Library reader will search the text layer and jump to a rectangle. This is the
//! smallest version that the corpus run needs. Matching ignores case and all whitespace because the
//! layer synthesises spaces from geometry and the oracle text does not agree on them.
//!
//! Do not: treat this as the final matcher (no ligature folding, no hyphen joins).

use crate::CharBox;

/// A search hit as a rectangle in page points.
#[derive(Clone, Debug)]
pub struct Hit {
    /// Left edge.
    pub x0: f32,
    /// Top edge.
    pub y0: f32,
    /// Right edge.
    pub x1: f32,
    /// Bottom edge.
    pub y1: f32,
}

/// Fold text for matching: lowercase, drop whitespace.
pub fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Find every occurrence of `needle` in the layer and return the union box of each match.
pub fn find(layer: &[CharBox], needle: &str) -> Vec<Hit> {
    let want: Vec<char> = fold(needle).chars().collect();
    if want.is_empty() {
        return Vec::new();
    }
    let idx: Vec<usize> = (0..layer.len())
        .filter(|&i| !layer[i].ch.is_whitespace())
        .collect();
    let hay: Vec<char> = idx
        .iter()
        .flat_map(|&i| layer[i].ch.to_lowercase().next())
        .collect();
    let mut hits = Vec::new();
    if hay.len() < want.len() {
        return hits;
    }
    for s in 0..=hay.len() - want.len() {
        if hay[s..s + want.len()] == want[..] {
            let sl = &idx[s..s + want.len()];
            let mut h = Hit {
                x0: f32::MAX,
                y0: f32::MAX,
                x1: f32::MIN,
                y1: f32::MIN,
            };
            for &i in sl {
                h.x0 = h.x0.min(layer[i].x0);
                h.y0 = h.y0.min(layer[i].y0);
                h.x1 = h.x1.max(layer[i].x1);
                h.y1 = h.y1.max(layer[i].y1);
            }
            hits.push(h);
        }
    }
    hits
}
