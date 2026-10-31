//! Reader for component records in the CC0 base library native format.
//!
//! # Rules
//! - Only the `librepcb_component` root is accepted.
//! - Names, descriptions and signal names are bounded; geometry is not parsed here.
//!
//! # Dependencies
//! - crates/library/src/sexpr.rs
//! - crates/library/src/sources.rs
use crate::sexpr::{self, Error as SexprError};
use crate::sources::SourceError;

/// Longest name or description string on a component record.
pub const MAX_TEXT_BYTES: usize = 256;

fn invalid(text: impl Into<String>) -> SourceError {
    SourceError::Invalid(text.into())
}

fn from_sexpr(err: SexprError) -> SourceError {
    SourceError::Invalid(err.to_string())
}

fn check_text(field: &str, text: &str) -> Result<(), SourceError> {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES || text.chars().any(|c| c.is_control()) {
        return Err(invalid(format!("the {field} is missing or too long")));
    }
    Ok(())
}

/// One signal named on a CC0 base component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cc0Signal {
    /// Signal id in the library file.
    pub uuid: String,
    /// Name shown on the schematic, such as `A` or `C`.
    pub name: String,
    /// Electrical role token from the file, such as `passive`.
    pub role: String,
}

/// A schematic component record from the CC0 base library format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cc0ComponentRecord {
    /// Component id in the library file.
    pub uuid: String,
    /// Component name, such as `Diode`.
    pub name: String,
    /// Description text when the file gives one.
    pub description: String,
    /// Reference prefix, such as `D`.
    pub prefix: String,
    /// Signals exported by the component.
    pub signals: Vec<Cc0Signal>,
}

/// Read one component file from the CC0 base library format.
///
/// # Rules
/// - The root tag must be `librepcb_component`.
/// - At most 256 signals are accepted.
///
/// # Do not
/// - Parse symbols, footprints or 3D models; those live in sibling files.
pub fn read_cc0_component(text: &str) -> Result<Cc0ComponentRecord, SourceError> {
    let limits = sexpr::Limits {
        bytes: 256 * 1024,
        nodes: 20_000,
        depth: 32,
        atom_bytes: 4 * 1024,
    };
    let root = sexpr::parse_limits(text, limits).map_err(from_sexpr)?;
    if root.name() != "librepcb_component" {
        return Err(invalid("the file is not a base library component"));
    }
    let uuid = root.value(1);
    check_text("uuid", uuid)?;
    let name = root.field("name");
    check_text("name", name)?;
    let description = root.field("description");
    if !description.is_empty() {
        check_text("description", description)?;
    }
    let prefix = root.field("prefix");
    check_text("prefix", prefix)?;
    let mut signals = Vec::new();
    for child in &root.children {
        if child.name() != "signal" {
            continue;
        }
        if signals.len() >= 256 {
            return Err(invalid("too many signals on the component"));
        }
        let sig_uuid = child.value(0);
        check_text("signal id", sig_uuid)?;
        let sig_name = child.field("name");
        check_text("signal name", sig_name)?;
        let role = child.field("role");
        check_text("signal role", role)?;
        signals.push(Cc0Signal {
            uuid: sig_uuid.to_owned(),
            name: sig_name.to_owned(),
            role: role.to_owned(),
        });
    }
    if signals.is_empty() {
        return Err(invalid("the component names no signals"));
    }
    Ok(Cc0ComponentRecord {
        uuid: uuid.to_owned(),
        name: name.to_owned(),
        description: description.to_owned(),
        prefix: prefix.to_owned(),
        signals,
    })
}
