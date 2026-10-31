use super::review::ReviewedField;
use super::text::{TextCell, TextDocument, source_of_cells};
use super::types::{
    DatasheetExtractBundle, ExtractedOrderingCode, ExtractedPackageDimension, ExtractedPinRow,
    ExtractedRatingRow, MAX_EXTRACT_ROWS, MAX_FIELD_TEXT,
};

/// Extract tables from an already-built text document (tests and sidecar output).
pub struct TextTableExtractor {
    pub document: TextDocument,
}

pub fn extract_from_text_document(doc: &TextDocument) -> Result<DatasheetExtractBundle, String> {
    let mut bundle = DatasheetExtractBundle::default();
    let lines = group_lines(doc)?;
    let mut section: Option<Section> = None;
    for line in lines {
        if let Some(next) = detect_section(&line.text) {
            section = Some(next);
            continue;
        }
        let Some(sec) = section else {
            continue;
        };
        match sec {
            Section::PinTable => {
                if bundle.pin_table.len() >= MAX_EXTRACT_ROWS {
                    continue;
                }
                if let Some(row) = parse_pin_line(&line)? {
                    bundle.pin_table.push(ReviewedField::new(row));
                }
            }
            Section::AbsoluteMaximum => push_rating(&mut bundle.absolute_maximum_ratings, &line)?,
            Section::RecommendedOperating => {
                push_rating(&mut bundle.recommended_operating_conditions, &line)?
            }
            Section::Ordering => push_ordering(&mut bundle.ordering_codes, &line)?,
            Section::PackageDimensions => push_dimension(&mut bundle.package_dimensions, &line)?,
        }
    }
    Ok(bundle)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    PinTable,
    AbsoluteMaximum,
    RecommendedOperating,
    Ordering,
    PackageDimensions,
}

struct LineGroup {
    text: String,
    cells: Vec<TextCell>,
}

fn group_lines(doc: &TextDocument) -> Result<Vec<LineGroup>, String> {
    let mut all: Vec<TextCell> = Vec::new();
    for page in &doc.pages {
        all.extend(page.cells.clone());
    }
    all.sort_by(|a, b| {
        a.page_index
            .cmp(&b.page_index)
            .then(a.rect.y0.partial_cmp(&b.rect.y0).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.rect.x0.partial_cmp(&b.rect.x0).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut out: Vec<LineGroup> = Vec::new();
    let mut current: Vec<TextCell> = Vec::new();
    let mut current_y: Option<f32> = None;
    for cell in all {
        let same_line = current_y.is_some_and(|y| (cell.rect.y0 - y).abs() <= 4.0);
        if !same_line && !current.is_empty() {
            out.push(finish_line(&current)?);
            current.clear();
        }
        current_y = Some(cell.rect.y0);
        current.push(cell);
    }
    if !current.is_empty() {
        out.push(finish_line(&current)?);
    }
    Ok(out)
}

fn finish_line(cells: &[TextCell]) -> Result<LineGroup, String> {
    let mut text = String::new();
    for (i, c) in cells.iter().enumerate() {
        if i > 0 {
            text.push(' ');
        }
        text.push_str(&c.text);
    }
    Ok(LineGroup {
        text: text.trim().to_owned(),
        cells: cells.to_vec(),
    })
}

fn detect_section(line: &str) -> Option<Section> {
    let folded: String = line
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if folded.contains("pindescription") || folded.contains("pinconfiguration") {
        return Some(Section::PinTable);
    }
    if folded.contains("absolutemaximumratings") || folded == "absolutemaximum" {
        return Some(Section::AbsoluteMaximum);
    }
    if folded.contains("recommendedoperatingconditions") {
        return Some(Section::RecommendedOperating);
    }
    if folded.contains("orderinginformation") || folded.contains("orderinformation") {
        return Some(Section::Ordering);
    }
    if folded.contains("packagedimensions") || folded.contains("mechanicaldata") {
        return Some(Section::PackageDimensions);
    }
    None
}

fn parse_pin_line(line: &LineGroup) -> Result<Option<ExtractedPinRow>, String> {
    let parts: Vec<&str> = line.text.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(None);
    }
    let number = parts[0].trim_end_matches('.');
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    let name = parts[1..].join(" ");
    if name.len() > MAX_FIELD_TEXT || number.len() > 16 {
        return Ok(None);
    }
    let source = source_of_cells(&line.cells, &line.text)?;
    Ok(Some(ExtractedPinRow {
        pin_number: number.to_owned(),
        pin_name: name,
        source,
    }))
}

fn push_rating(
    target: &mut Vec<ReviewedField<ExtractedRatingRow>>,
    line: &LineGroup,
) -> Result<(), String> {
    if target.len() >= MAX_EXTRACT_ROWS {
        return Ok(());
    }
    let cols: Vec<&str> = line.text.split_whitespace().collect();
    if cols.len() < 2 {
        return Ok(());
    }
    let parameter = cols[0].to_owned();
    if parameter.len() > MAX_FIELD_TEXT {
        return Ok(());
    }
    let (value, unit) = split_value_unit(&cols[1..]);
    let source = source_of_cells(&line.cells, &line.text)?;
    target.push(ReviewedField::new(ExtractedRatingRow {
        parameter,
        value,
        unit,
        source,
    }));
    Ok(())
}

fn push_ordering(
    target: &mut Vec<ReviewedField<ExtractedOrderingCode>>,
    line: &LineGroup,
) -> Result<(), String> {
    if target.len() >= MAX_EXTRACT_ROWS {
        return Ok(());
    }
    let cols: Vec<&str> = line.text.split_whitespace().collect();
    if cols.is_empty() {
        return Ok(());
    }
    let code = cols[0].to_owned();
    if code.len() > MAX_FIELD_TEXT {
        return Ok(());
    }
    let description = cols.get(1..).unwrap_or(&[]).join(" ");
    let source = source_of_cells(&line.cells, &line.text)?;
    target.push(ReviewedField::new(ExtractedOrderingCode {
        code,
        description,
        source,
    }));
    Ok(())
}

fn push_dimension(
    target: &mut Vec<ReviewedField<ExtractedPackageDimension>>,
    line: &LineGroup,
) -> Result<(), String> {
    if target.len() >= MAX_EXTRACT_ROWS {
        return Ok(());
    }
    let cols: Vec<&str> = line.text.split_whitespace().collect();
    if cols.len() < 2 {
        return Ok(());
    }
    let name = cols[0].to_owned();
    if name.len() > MAX_FIELD_TEXT {
        return Ok(());
    }
    let (value, unit) = split_value_unit(&cols[1..]);
    let source = source_of_cells(&line.cells, &line.text)?;
    target.push(ReviewedField::new(ExtractedPackageDimension {
        name,
        value,
        unit,
        source,
    }));
    Ok(())
}

fn split_value_unit(cols: &[&str]) -> (String, String) {
    if cols.is_empty() {
        return (String::new(), String::new());
    }
    if cols.len() == 1 {
        return (cols[0].to_owned(), String::new());
    }
    let unit = cols.last().copied().unwrap_or("");
    if unit.len() <= 6 && unit.chars().all(|c| c.is_ascii_alphabetic() || c == '%' || c == '°') {
        let value = cols[..cols.len() - 1].join(" ");
        (value, unit.to_owned())
    } else {
        (cols.join(" "), String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasheet_extract::provenance::DatasheetRect;

    fn cell(page: u32, y: f32, x: f32, text: &str) -> TextCell {
        TextCell {
            text: text.to_owned(),
            page_index: page,
            page_label: page + 1,
            rect: DatasheetRect {
                x0: x,
                y0: y,
                x1: x + 20.0,
                y1: y + 10.0,
            },
        }
    }

    #[test]
    fn pin_table_carries_page_labels() -> Result<(), String> {
        let doc = TextDocument {
            pages: vec![TextPage {
                page_index: 2,
                page_label: 5,
                cells: vec![
                    cell(2, 100.0, 10.0, "Pin"),
                    cell(2, 100.0, 40.0, "Description"),
                    cell(2, 120.0, 10.0, "1"),
                    cell(2, 120.0, 30.0, "OUT"),
                ],
            }],
        };
        let bundle = extract_from_text_document(&doc)?;
        assert_eq!(bundle.pin_table.len(), 1);
        assert_eq!(bundle.pin_table[0].value.source.page_label, 5);
        assert_eq!(bundle.pin_table[0].value.pin_number, "1");
        assert_eq!(bundle.pin_table[0].value.pin_name, "OUT");
        Ok(())
    }
}
