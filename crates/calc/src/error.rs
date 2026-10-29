//! Calculator input and domain errors.

use core::fmt;

/// Failure from a calculator entry point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalcError {
    /// A numeric input was not finite.
    NonFinite {
        name: &'static str,
    },
    /// A value lay outside the allowed closed interval.
    OutOfRange {
        name: &'static str,
        min: f64,
        max: f64,
        value: f64,
    },
    /// A value must be strictly positive.
    NonPositive {
        name: &'static str,
    },
    /// Inputs are inconsistent for the requested solve.
    Domain {
        message: &'static str,
    },
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { name } => write!(f, "{name} must be a finite number"),
            Self::OutOfRange {
                name,
                min,
                max,
                value,
            } => write!(
                f,
                "{name}={value} is outside the allowed range [{min}, {max}]"
            ),
            Self::NonPositive { name } => write!(f, "{name} must be greater than zero"),
            Self::Domain { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for CalcError {}

pub(crate) fn check_finite(name: &'static str, value: f64) -> Result<(), CalcError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CalcError::NonFinite { name })
    }
}

pub(crate) fn check_range(
    name: &'static str,
    value: f64,
    min: f64,
    max: f64,
) -> Result<(), CalcError> {
    check_finite(name, value)?;
    if value < min || value > max {
        Err(CalcError::OutOfRange {
            name,
            min,
            max,
            value,
        })
    } else {
        Ok(())
    }
}

pub(crate) fn check_positive(name: &'static str, value: f64) -> Result<(), CalcError> {
    check_finite(name, value)?;
    if value <= 0.0 {
        Err(CalcError::NonPositive { name })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_error_message_includes_bounds() -> Result<(), CalcError> {
        let err = check_range("test_param", 99.0, 0.0, 10.0).expect_err("out of range");
        let text = err.to_string();
        assert!(text.contains("test_param"));
        assert!(text.contains("[0"));
        assert!(text.contains("10"));
        Ok(())
    }
}
