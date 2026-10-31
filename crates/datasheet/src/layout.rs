use crate::traits::PageSizePt;

/// Gap between stacked pages in the scroll view, in pixels.
pub const PAGE_GAP_PX: f32 = 12.0;

/// Maximum side length of one GPU texture; larger pages are split into tiles.
pub const MAX_TEXTURE_SIDE: u32 = 4096;

/// Pixel height of one page at the given scale.
pub fn page_height_px(size: PageSizePt, scale: f32) -> f32 {
    size.height * scale
}

/// Pixel width of one page at the given scale.
pub fn page_width_px(size: PageSizePt, scale: f32) -> f32 {
    size.width * scale
}

/// Cumulative Y offset of each page top inside the virtual scroll area.
pub fn page_offsets(heights: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(heights.len());
    let mut y = 0.0;
    for (i, h) in heights.iter().enumerate() {
        out.push(y);
        y += h;
        if i + 1 < heights.len() {
            y += PAGE_GAP_PX;
        }
    }
    out
}

/// Total scroll height for the given page pixel heights.
pub fn scroll_height(heights: &[f32]) -> f32 {
    page_offsets(heights)
        .last()
        .copied()
        .unwrap_or(0.0)
        .saturating_add(heights.last().copied().unwrap_or(0.0))
}

/// Page indices intersecting a vertical viewport in pixel space.
pub fn visible_pages(
    scroll_y: f32,
    viewport_h: f32,
    heights: &[f32],
    neighbour: u32,
) -> Vec<u32> {
    if heights.is_empty() {
        return Vec::new();
    }
    let offsets = page_offsets(heights);
    let view_bottom = scroll_y + viewport_h;
    let mut first = heights.len();
    let mut last = 0usize;
    for (i, (&top, &h)) in offsets.iter().zip(heights.iter()).enumerate() {
        let bottom = top + h;
        if bottom >= scroll_y && top <= view_bottom {
            first = first.min(i);
            last = last.max(i);
        }
    }
    if first > last {
        return Vec::new();
    }
    let pad = neighbour as usize;
    let lo = first.saturating_sub(pad);
    let hi = (last + pad).min(heights.len() - 1);
    (lo..=hi).map(|i| i as u32).collect()
}

/// Tile grid for one page bitmap that exceeds [`MAX_TEXTURE_SIDE`].
pub fn tile_grid(width: u32, height: u32) -> (u32, u32) {
    let cols = width.div_ceil(MAX_TEXTURE_SIDE).max(1);
    let rows = height.div_ceil(MAX_TEXTURE_SIDE).max(1);
    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_pages_include_one_neighbour() {
        let heights = vec![100.0; 200];
        let visible = visible_pages(50.0, 80.0, &heights, 1);
        assert!(visible.len() <= 3);
        assert!(visible.contains(&0) || visible.contains(&1));
    }

    #[test]
    fn scrolling_a_long_document_touches_few_pages() {
        let heights = vec![800.0; 200];
        let mut rendered = 0u32;
        for step in 0..20 {
            let y = step as f32 * 400.0;
            rendered += visible_pages(y, 600.0, &heights, 1).len() as u32;
        }
        assert!(rendered < 200, "expected lazy window, got {rendered} page visits");
    }
}
