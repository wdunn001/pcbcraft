use serde::{Deserialize, Serialize};

use super::provenance::{DatasheetRect, DatasheetSourceRecord};

/// One text fragment with its box on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextCell {
    pub text: String,
    pub page_index: u32,
    pub page_label: u32,
    pub rect: DatasheetRect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextPage {
    pub page_index: u32,
    pub page_label: u32,
    pub cells: Vec<TextCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextDocument {
    pub pages: Vec<TextPage>,
}

/// Build cells from hayro-style character boxes (page index and label are shared).
pub fn cells_from_char_boxes(
    page_index: u32,
    page_label: u32,
    chars: &[(char, f32, f32, f32, f32)],
) -> Vec<TextCell> {
    chars
        .iter()
        .filter(|(ch, ..)| !ch.is_whitespace())
        .filter_map(|(ch, x0, y0, x1, y1)| {
            let rect = DatasheetRect::checked(*x0, *y0, *x1, *y1).ok()?;
            Some(TextCell {
                text: ch.to_string(),
                page_index,
                page_label,
                rect,
            })
        })
        .collect()
}

pub(crate) fn source_of_cells(cells: &[TextCell], quote: &str) -> Result<DatasheetSourceRecord, String> {
    let first = cells.first().ok_or("empty cell group")?;
    let mut rect = first.rect;
    for c in cells.iter().skip(1) {
        rect = DatasheetRect::union(rect, c.rect);
    }
    DatasheetSourceRecord::new(first.page_index, first.page_label, rect, quote, Some(1.0))
}
