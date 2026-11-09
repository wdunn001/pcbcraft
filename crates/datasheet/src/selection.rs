//! Drag selection and copy in reading order.

use crate::text::CharBox;

/// A text selection as inclusive character indices into the page layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    /// Where the drag started.
    pub anchor: usize,
    /// Where the pointer is now.
    pub focus: usize,
}

impl Selection {
    /// Normalised (lo, hi) indices for slicing.
    pub fn range(self, len: usize) -> (usize, usize) {
        if len == 0 {
            return (0, 0);
        }
        let lo = self.anchor.min(self.focus).min(len - 1);
        let hi = self.anchor.max(self.focus).min(len - 1);
        (lo, hi)
    }
}

/// Selected characters in reading order, without trailing layout newlines.
pub fn selected_text(layer: &[CharBox], sel: Selection) -> String {
    if layer.is_empty() {
        return String::new();
    }
    let (lo, hi) = sel.range(layer.len());
    let mut out: String = layer[lo..=hi].iter().map(|c| c.ch).collect();
    while out.ends_with('\n') || out.ends_with('\r') {
        out.pop();
    }
    out
}

/// Character index under a point in page coordinates, if any.
pub fn char_at_point(layer: &[CharBox], x: f32, y: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, c) in layer.iter().enumerate() {
        if c.ch.is_whitespace() {
            continue;
        }
        if x >= c.x0 && x <= c.x1 && y >= c.y0 && y <= c.y1 {
            return Some(i);
        }
        let cx = (c.x0 + c.x1) * 0.5;
        let cy = (c.y0 + c.y1) * 0.5;
        let d = (cx - x).powi(2) + (cy - y).powi(2);
        if best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}
