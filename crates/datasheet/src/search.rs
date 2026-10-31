//! Text search over one page or a whole document.

use crate::text::{CharBox, text_layer};
use hayro::hayro_interpret::InterpreterCache;
use hayro::hayro_syntax::Pdf;

/// A search hit as a rectangle in page points.
#[derive(Clone, Debug, PartialEq)]
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

/// One match in a document, with its page index.
#[derive(Clone, Debug, PartialEq)]
pub struct DocHit {
    /// 0-based page index.
    pub page: usize,
    /// Match bounds on that page.
    pub bounds: Hit,
}

/// Document-wide search state.
#[derive(Clone, Debug, Default)]
pub struct DocSearch {
    /// The folded query last run.
    pub query: String,
    /// Every match in page order.
    pub hits: Vec<DocHit>,
    /// Index into `hits` for the active highlight.
    pub active: usize,
}

impl DocSearch {
    /// Clear results and the active index.
    pub fn clear(&mut self) {
        self.query.clear();
        self.hits.clear();
        self.active = 0;
    }

    /// Number of matches for the status line.
    pub fn count(&self) -> usize {
        self.hits.len()
    }

    /// Step the active match forward or backward; returns the new page to show.
    pub fn step(&mut self, delta: isize) -> Option<usize> {
        if self.hits.is_empty() {
            return None;
        }
        let n = self.hits.len() as isize;
        let next = (self.active as isize + delta).rem_euclid(n) as usize;
        self.active = next;
        Some(self.hits[next].page)
    }

    /// Bounds of the active match, if any.
    pub fn active_hit(&self) -> Option<&DocHit> {
        self.hits.get(self.active)
    }
}

/// Fold text for matching: lowercase, drop whitespace.
pub fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Find every occurrence of `needle` on one page layer.
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

/// Search every page of `pdf` for `needle`.
pub fn search_document(pdf: &Pdf, needle: &str) -> Result<DocSearch, String> {
    let page_count = (0..).take_while(|i| pdf.pages().get(*i).is_some()).count();
    let ic = InterpreterCache::new();
    let mut hits = Vec::new();
    for page in 0..page_count {
        let layer = text_layer(pdf, page, &ic)?;
        for h in find(&layer, needle) {
            hits.push(DocHit { page, bounds: h });
        }
    }
    Ok(DocSearch {
        query: needle.to_owned(),
        hits,
        active: 0,
    })
}
