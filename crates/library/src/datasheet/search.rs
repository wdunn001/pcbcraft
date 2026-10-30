//! Text search over a page's character list.
//!
//! Context: the reader highlights matches and jumps between them. Matching ignores case and all
//! whitespace because the text layer synthesises spaces from geometry and a typed phrase may
//! not agree on them.
//!
//! # Dependencies
//! - crates/library/src/datasheet/mod.rs (`CharBox`, `Hit`)
//!
//! # Do not
//! - Treat this as a full-text engine: no ligature folding and no hyphen joins.
use super::{CharBox, Hit};

/// Lowercase text with whitespace removed.
pub fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Every occurrence of `needle` in `layer`, each as the union box of the characters it covers.
///
/// # Rules
/// - An empty or all-space needle finds nothing.
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
        .map(|&i| layer[i].ch.to_lowercase().next().unwrap_or(layer[i].ch))
        .collect();
    let mut hits = Vec::new();
    if hay.len() < want.len() {
        return hits;
    }
    for s in 0..=hay.len() - want.len() {
        if hay[s..s + want.len()] == want[..] {
            let covered = &idx[s..s + want.len()];
            let mut hit = Hit {
                x0: f32::MAX,
                y0: f32::MAX,
                x1: f32::MIN,
                y1: f32::MIN,
                first: covered[0],
                last: covered[covered.len() - 1] + 1,
            };
            for &i in covered {
                hit.x0 = hit.x0.min(layer[i].x0);
                hit.y0 = hit.y0.min(layer[i].y0);
                hit.x1 = hit.x1.max(layer[i].x1);
                hit.y1 = hit.y1.max(layer[i].y1);
            }
            hits.push(hit);
        }
    }
    hits
}

/// The text of characters `first..last` of a layer, for copying a selection.
pub fn text_of(layer: &[CharBox], first: usize, last: usize) -> String {
    layer
        .get(first.min(layer.len())..last.min(layer.len()))
        .map(|s| s.iter().map(|c| c.ch).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(text: &str) -> Vec<CharBox> {
        text.chars()
            .enumerate()
            .map(|(i, ch)| CharBox {
                ch,
                x0: i as f32 * 6.0,
                x1: i as f32 * 6.0 + 6.0,
                y0: 0.0,
                y1: 10.0,
            })
            .collect()
    }

    #[test]
    fn matches_ignore_case_and_spacing() {
        let l = layer("Power rating at 70 C");
        let hits = find(&l, "POWERRATING");
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].first, hits[0].last), (0, 12));
        assert!(hits[0].x1 > hits[0].x0);
    }

    #[test]
    fn an_empty_needle_finds_nothing_and_text_of_copies_a_range() {
        let l = layer("abc def");
        assert!(find(&l, "  ").is_empty());
        assert_eq!(text_of(&l, 4, 7), "def");
        assert_eq!(text_of(&l, 5, 99), "ef");
    }
}
