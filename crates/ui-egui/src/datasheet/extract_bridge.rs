//! Bridge from the open PDF text layers to the library extractor's TextDocument.
//!
//! The datasheet pane renders one page at a time and reads its text layer on demand.
//! This bridge collects all layers into the shape the library extractor accepts,
//! so the desktop sidecar can build the same object from hayro.

use pcbcraft_datasheet::CharBox;
use pcbcraft_library::datasheet_extract::{TextDocument, TextPage, cells_from_char_boxes};

/// Turn the text layers of every page into one extractable document.
///
/// # Rules
/// - Each page gets a page_label equal to page_index + 1.
/// - Pages are capped at 200 to bound memory.
/// - Character boxes become cells via cells_from_char_boxes.
///
/// # Dependencies
/// - crates/datasheet/src/lib.rs (CharBox)
/// - crates/library/src/datasheet_extract/text.rs (cells_from_char_boxes)
///
/// # Do not
/// - Call with more than 200 pages; later pages are silently dropped.
pub(crate) fn text_document_from_layers(pages: &[Vec<CharBox>]) -> TextDocument {
    let pages: Vec<TextPage> = pages
        .iter()
        .enumerate()
        .take(200)
        .map(|(i, layer)| {
            let page_index = i as u32;
            let page_label = (i + 1) as u32;
            let cells = layer
                .iter()
                .map(|c| (c.ch, c.x0, c.y0, c.x1, c.y1))
                .collect::<Vec<_>>();
            TextPage {
                page_index,
                page_label,
                cells: cells_from_char_boxes(page_index, page_label, &cells),
            }
        })
        .collect();
    TextDocument { pages }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn char_box(ch: char, x0: f32, y0: f32, x1: f32, y1: f32) -> CharBox {
        CharBox { ch, x0, y0, x1, y1 }
    }

    #[test]
    fn text_document_from_layers_keeps_pages_and_labels() {
        let layer1 = vec![char_box('A', 0.0, 0.0, 10.0, 10.0)];
        let layer2 = vec![char_box('B', 0.0, 0.0, 10.0, 10.0)];
        let doc = text_document_from_layers(&[layer1, layer2]);
        assert_eq!(doc.pages.len(), 2);
        assert_eq!(doc.pages[0].page_index, 0);
        assert_eq!(doc.pages[0].page_label, 1);
        assert_eq!(doc.pages[1].page_index, 1);
        assert_eq!(doc.pages[1].page_label, 2);
        assert_eq!(doc.pages[0].cells.len(), 1);
        assert_eq!(doc.pages[1].cells.len(), 1);
    }
}
