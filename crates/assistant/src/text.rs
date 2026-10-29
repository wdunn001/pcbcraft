//! Plain refusal reasons the pane maps to catalog strings.

pub use crate::transcript::MAX_PROMPT;

/// Returns an error when `char_count` exceeds [`MAX_PROMPT`].
pub fn check_prompt_length(char_count: usize) -> Result<(), String> {
    if char_count > MAX_PROMPT {
        Err(prompt_too_long_reason(char_count))
    } else {
        Ok(())
    }
}

pub fn prompt_too_long_reason(char_count: usize) -> String {
    format!(
        "That message is {char_count} characters; the limit is {MAX_PROMPT}."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_longer_than_the_cap_is_refused_with_the_reason() {
        let over = MAX_PROMPT + 1;
        let err = check_prompt_length(over).unwrap_err();
        assert_eq!(err, prompt_too_long_reason(over));
        assert!(err.contains(&MAX_PROMPT.to_string()));
    }
}
