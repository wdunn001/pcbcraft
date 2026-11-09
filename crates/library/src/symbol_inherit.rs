//! Reads one symbol from an online source even when it inherits its pins from a
//! parent symbol that lives in a different file.
//!
//! # Rules
//! - The open libraries store one symbol per file; most real chips are derived symbols
//!   (`extends`) whose pins are in a sibling file of the same library.
//! - Parents are fetched from the same library, at most [`MAX_PARENT_DEPTH`] deep, and
//!   every file read is reported so the notices name all of them.
//! - The text handed to the reader is the parent file plus the child symbol; nothing
//!   is rewritten, so the reader's own checks and limits still apply.
//!
//! # Dependencies
//! - crates/library/src/cad.rs (the source and its file type)
//! - crates/library/src/reader.rs (the symbol reader that resolves `extends`)
//! - crates/library/src/sexpr.rs (finds symbol spans)
use crate::cad::{CadKind, CadRef, CadSource, Provenance};
use crate::sexpr;
use crate::sources::SourceError;
use crate::{SymbolDefinition, read_symbols};

/// Most parent files followed from one symbol.
pub const MAX_PARENT_DEPTH: usize = 4;

fn invalid(text: String) -> SourceError {
    SourceError::Invalid(text)
}

/// The parent a symbol named `name` declares in `text`, when it declares one.
fn parent_of(text: &str, name: &str) -> Result<Option<String>, SourceError> {
    let root =
        sexpr::parse(text).map_err(|e| invalid(format!("the symbol file is not valid: {e}")))?;
    let node = root
        .children
        .iter()
        .find(|c| c.name() == "symbol" && c.value(1) == name)
        .ok_or_else(|| SourceError::NotFound(name.to_owned()))?;
    Ok(node
        .child("extends")
        .map(|x| x.value(1).to_owned())
        .filter(|p| !p.is_empty()))
}

/// The text of the symbol called `name` inside `text`, as written.
fn symbol_text<'a>(text: &'a str, name: &str) -> Result<&'a str, SourceError> {
    let root =
        sexpr::parse(text).map_err(|e| invalid(format!("the symbol file is not valid: {e}")))?;
    let node = root
        .children
        .iter()
        .find(|c| c.name() == "symbol" && c.value(1) == name)
        .ok_or_else(|| SourceError::NotFound(name.to_owned()))?;
    text.get(node.span.clone())
        .ok_or_else(|| invalid("the symbol file could not be sliced".into()))
}

/// Fetch a symbol and, when it extends another, the files that hold its parents.
///
/// # Rules
/// - A symbol with no parent behaves exactly like fetching and reading one file.
/// - The returned provenance list starts with the symbol's own file, then each parent.
/// - A parent that is not in the same library is an error; nothing is guessed.
///
/// # Dependencies
/// - crates/library/src/cad.rs
///
/// # Do not
/// - Write to a project; the caller passes the result to the engine commands.
pub fn read_symbol_with_parents(
    source: &dyn CadSource,
    symbol: &CadRef,
) -> Result<(SymbolDefinition, Vec<Provenance>), SourceError> {
    let first = source.fetch_symbol(symbol)?;
    let mut provenance = vec![first.provenance.clone()];
    let mut outermost = first.text()?.to_owned();
    let mut pieces: Vec<String> = Vec::new();
    let mut name = symbol.name.clone();
    let mut text = outermost.clone();
    let mut depth = 0;
    while let Some(parent) = parent_of(&text, &name)? {
        depth += 1;
        if depth > MAX_PARENT_DEPTH {
            return Err(invalid("the symbol inherits through too many files".into()));
        }
        pieces.push(symbol_text(&text, &name)?.to_owned());
        let reference = CadRef::new(CadKind::Symbol, &symbol.library, &parent)
            .map_err(|e| invalid(e.to_string()))?;
        let file = source.fetch_symbol(&reference)?;
        provenance.push(file.provenance.clone());
        text = file.text()?.to_owned();
        outermost = text.clone();
        name = parent;
    }
    let combined = if pieces.is_empty() {
        outermost
    } else {
        let body = outermost.trim_end();
        let body = body.strip_suffix(')').unwrap_or(body);
        format!("{body}\n{}\n)", pieces.join("\n"))
    };
    let def = read_symbols(&combined)
        .map_err(|e| invalid(format!("the symbol file could not be read: {e}")))?
        .into_iter()
        .find(|d| d.name == symbol.name)
        .ok_or_else(|| SourceError::NotFound(format!("{} {}", symbol.library, symbol.name)))?;
    provenance.dedup();
    Ok((def, provenance))
}
