use serde::{Deserialize, Serialize};

use super::types::MAX_FIELD_TEXT;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    #[default]
    Pending,
    Accepted,
    Edited,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedField<T> {
    pub value: T,
    #[serde(default)]
    pub edited: Option<T>,
    #[serde(default)]
    pub decision: ReviewDecision,
}

impl<T> ReviewedField<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            edited: None,
            decision: ReviewDecision::Pending,
        }
    }
}

pub fn set_decision<T>(field: &mut ReviewedField<T>, decision: ReviewDecision) -> Result<(), String> {
    if decision == ReviewDecision::Edited && field.edited.is_none() {
        return Err("edited decision requires an edited value".into());
    }
    field.decision = decision;
    Ok(())
}

pub fn effective_text(field: &ReviewedField<String>) -> Option<&str> {
    match field.decision {
        ReviewDecision::Rejected | ReviewDecision::Pending => None,
        ReviewDecision::Accepted => Some(field.value.as_str()),
        ReviewDecision::Edited => field.edited.as_deref(),
    }
}

pub fn bounded_field_text(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("empty field text".into());
    }
    if t.len() > MAX_FIELD_TEXT {
        return Err(format!("field text exceeds {MAX_FIELD_TEXT} bytes"));
    }
    Ok(t.to_owned())
}
