//! Project-wide drawing frame template for plots and PDFs.

use crate::Id;
use crate::Project;
use serde::{Deserialize, Serialize};

/// Standard ISO A-series landscape widths and heights in millimetres.
pub const PAGE_SIZES: [(&str, f64, f64); 8] = [
    ("A7", 105.0, 74.0),
    ("A6", 148.0, 105.0),
    ("A5", 210.0, 148.0),
    ("A4", 297.0, 210.0),
    ("A3", 420.0, 297.0),
    ("A2", 594.0, 420.0),
    ("A1", 841.0, 594.0),
    ("A0", 1189.0, 841.0),
];

pub const MAX_TITLE_BLOCK_FIELDS: usize = 16;
pub const MAX_TITLE_BLOCK_LABEL: usize = 64;
pub const MAX_TITLE_BLOCK_TEMPLATE: usize = 256;
pub const MAX_LOGO_TEXT: usize = 256;

/// One row of the title block: a label and a template string resolved from
/// project metadata and page settings when the frame is drawn.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TitleBlockField {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u8>,
}

/// Frame and title-block template stored on the project.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DrawingFrame {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enabled: bool,
    /// A standard size such as `A3`; empty picks the smallest sheet that fits
    /// the schematic content when the frame is enabled.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub page_size: String,
    #[serde(default = "default_show_zones")]
    pub show_zones: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub logo_text: String,
    #[serde(default = "default_title_block_fields")]
    pub fields: Vec<TitleBlockField>,
}

fn default_show_zones() -> bool {
    true
}

fn default_title_block_fields() -> Vec<TitleBlockField> {
    vec![
        TitleBlockField {
            label: "Title".into(),
            template: "${title}".into(),
            column: None,
        },
        TitleBlockField {
            label: "Revision".into(),
            template: "${revision}".into(),
            column: None,
        },
        TitleBlockField {
            label: "Company".into(),
            template: "${company}".into(),
            column: None,
        },
        TitleBlockField {
            label: "Date".into(),
            template: "${date}".into(),
            column: None,
        },
        TitleBlockField {
            label: "Design".into(),
            template: "${name}".into(),
            column: None,
        },
    ]
}

impl Default for DrawingFrame {
    fn default() -> Self {
        Self {
            enabled: false,
            page_size: String::new(),
            show_zones: default_show_zones(),
            logo_text: String::new(),
            fields: default_title_block_fields(),
        }
    }
}

impl DrawingFrame {
    pub fn validate(&self) -> Result<(), String> {
        if self.page_size.len() > 8 {
            return Err("drawing frame page size fits 8 bytes".into());
        }
        if !self.page_size.is_empty()
            && !PAGE_SIZES
                .iter()
                .any(|s| s.0.eq_ignore_ascii_case(&self.page_size))
        {
            return Err("drawing frame page size is A7 to A0 or empty".into());
        }
        if self.logo_text.len() > MAX_LOGO_TEXT {
            return Err("drawing frame logo text fits 256 bytes".into());
        }
        if self.fields.len() > MAX_TITLE_BLOCK_FIELDS {
            return Err("drawing frame title block holds at most 16 rows".into());
        }
        for field in &self.fields {
            if field.label.len() > MAX_TITLE_BLOCK_LABEL {
                return Err("title block labels fit 64 bytes".into());
            }
            if field.template.is_empty() || field.template.len() > MAX_TITLE_BLOCK_TEMPLATE {
                return Err("title block templates are 1 to 256 bytes".into());
            }
            if field.column.is_some_and(|c| c > 3) {
                return Err("title block column is 0 to 3".into());
            }
        }
        Ok(())
    }
}

/// True when `name` is a known ISO A-series page size.
pub fn known_page_size(name: &str) -> bool {
    PAGE_SIZES.iter().any(|s| s.0.eq_ignore_ascii_case(name))
}

/// Width and height in millimetres for a known page size name.
pub fn page_size_mm(name: &str) -> Option<(f64, f64)> {
    PAGE_SIZES
        .iter()
        .find(|s| s.0.eq_ignore_ascii_case(name))
        .map(|s| (s.1, s.2))
}

/// Page index and count for sheet `page`, matching schematic sheet numbering.
pub fn sheet_page_numbers(p: &Project, page: Id) -> (usize, usize) {
    let pages = p.sheets.len() + 1;
    let number = if page == 0 {
        1
    } else {
        p.sheets
            .iter()
            .position(|s| s.id == page)
            .map_or(1, |i| i + 2)
    };
    (number, pages)
}
