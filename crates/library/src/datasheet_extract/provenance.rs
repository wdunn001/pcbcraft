use serde::{Deserialize, Serialize};

/// Longest quoted snippet stored on a source record.
pub const MAX_QUOTE_BYTES: usize = 512;

/// Axis-aligned box in page points, top-left origin, y down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasheetRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl DatasheetRect {
    pub fn checked(x0: f32, y0: f32, x1: f32, y1: f32) -> Result<Self, String> {
        if !(x1 > x0 && y1 > y0 && x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite())
        {
            return Err("invalid datasheet rectangle".into());
        }
        Ok(Self { x0, y0, x1, y1 })
    }

    pub fn union(a: Self, b: Self) -> Self {
        Self {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        }
    }
}

/// Where one extracted value came from on the sheet (shared with the rules service).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasheetSourceRecord {
    /// Zero-based page index in the PDF.
    pub page_index: u32,
    /// Page number shown on badges (usually index plus one).
    pub page_label: u32,
    pub rect: DatasheetRect,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub quote: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

impl DatasheetSourceRecord {
    pub fn new(
        page_index: u32,
        page_label: u32,
        rect: DatasheetRect,
        quote: &str,
        confidence: Option<f32>,
    ) -> Result<Self, String> {
        if let Some(c) = confidence
            && !(c.is_finite() && (0.0..=1.0).contains(&c))
        {
            return Err("confidence must be between 0 and 1".into());
        }
        Ok(Self {
            page_index,
            page_label,
            rect,
            quote: bounded_quote(quote)?,
            confidence,
        })
    }
}

pub fn bounded_quote(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.len() > MAX_QUOTE_BYTES {
        return Err(format!("quote exceeds {MAX_QUOTE_BYTES} bytes"));
    }
    if t.chars().any(|c| c == '\0' || c.is_control() && c != '\t') {
        return Err("quote contains control characters".into());
    }
    Ok(t.to_owned())
}
