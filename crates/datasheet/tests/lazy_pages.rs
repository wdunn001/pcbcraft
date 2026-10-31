use pcbcraft_datasheet::layout::visible_pages;
use pcbcraft_datasheet::traits::{PageDocument, RenderRequest};
use pcbcraft_datasheet::{PageSizePt, RenderError};
use std::cell::Cell;
use std::sync::Arc;

struct MockDoc {
    pages: u32,
    renders: Cell<u32>,
}

impl PageDocument for MockDoc {
    fn page_count(&self) -> u32 {
        self.pages
    }

    fn page_size_pt(&self, page: u32) -> Result<PageSizePt, RenderError> {
        if page >= self.pages {
            return Err(RenderError::PageOutOfRange);
        }
        Ok(PageSizePt {
            width: 612.0,
            height: 792.0,
        })
    }

    fn render(&self, _request: RenderRequest) -> Result<image::RgbaImage, RenderError> {
        self.renders.set(self.renders.get() + 1);
        Ok(image::RgbaImage::new(8, 8))
    }
}

#[test]
fn a_two_hundred_page_scroll_never_touches_every_page() {
    let heights = vec![800.0; 200];
    let mut visits = 0u32;
    for step in 0..30 {
        let y = step as f32 * 500.0;
        visits += visible_pages(y, 700.0, &heights, 1).len() as u32;
    }
    assert!(visits < 200, "lazy window visited {visits} slots");
    let doc = Arc::new(MockDoc {
        pages: 200,
        renders: Cell::new(0),
    });
    let visible = visible_pages(0.0, 700.0, &heights, 1);
    for page in visible {
        let _ = doc.render(RenderRequest {
            page,
            scale: 1.0,
            tile: None,
        });
    }
    assert!(doc.renders.get() <= 3);
}
