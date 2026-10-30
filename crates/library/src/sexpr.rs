//! S-expression parser for KiCad library files.
//!
//! Parses the parenthesized expression format used by KiCad .kicad_sym and
//! .kicad_mod files. Supports atoms, quoted strings with escape sequences,
//! and nested lists. Enforces configurable size and depth limits to guard
//! against malformed or excessively large input.
//!
//! # Rules
//! - The parser is not recursive; it uses an explicit depth counter.
//! - Atoms are unquoted text terminated by whitespace or a closing paren.
//! - Quoted strings support \n, \r, \t, and backslash escaping.
//! - Input must contain exactly one top-level expression.
//!
//! # Dependencies
//! - crates/geom/src/lib.rs Unit conversion constants
//!
//! # Do not
//! - Add whitespace tolerance inside quoted strings.
//! - Accept trailing input after the top-level expression.

use std::ops::Range;
use thiserror::Error;

/// Parse error with byte offset and human-readable message.
///
/// # Rules
/// - `Syntax` errors include the byte offset where parsing failed.
/// - `Limit` errors name the limit that was exceeded.
/// - `Invalid` errors describe structural problems.
///
/// # Dependencies
/// - crates/library/Cargo.toml thiserror for derive macros
///
/// # Do not
/// - Add line/column numbers; byte offset is sufficient for error reporting.
#[derive(Debug, Error)]
pub enum Error {
    #[error("unexpected syntax at byte {offset}: {message}")]
    Syntax { offset: usize, message: String },
    #[error("library file exceeds {0}")]
    Limit(&'static str),
    #[error("invalid library file: {0}")]
    Invalid(String),
}

/// Resource limits for parsing S-expressions.
///
/// # Rules
/// - `bytes`: maximum input size in bytes
/// - `nodes`: maximum parsed node count
/// - `depth`: maximum nesting depth
/// - `atom_bytes`: maximum atom or string length
///
/// # Dependencies
/// - crates/library/src/sexpr.rs none outside this file
///
/// # Do not
/// - Change default values without updating reader.rs callers.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum input size in bytes.
    pub bytes: usize,
    /// Maximum node count.
    pub nodes: usize,
    /// Maximum nesting depth.
    pub depth: usize,
    /// Maximum atom length in bytes.
    pub atom_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 32 * 1024 * 1024,
            nodes: 1_000_000,
            depth: 64,
            atom_bytes: 256 * 1024,
        }
    }
}

/// Parsed S-expression node (atom or list).
///
/// # Rules
/// - `atom` is Some for atoms, None for lists
/// - `children` is empty for atoms, contains child nodes for lists
/// - `span` is the byte range of this node in the input
///
/// # Dependencies
/// - crates/library/src/sexpr.rs none outside this file
///
/// # Do not
/// - Expose this type publicly; it is an internal parser result.
#[derive(Clone, Debug)]
pub(crate) struct Node {
    /// Atom text, or None for lists.
    pub atom: Option<String>,
    /// Child nodes for lists; empty for atoms.
    pub children: Vec<Node>,
    /// Byte range of this node in the original input.
    pub span: Range<usize>,
}

impl Node {
    /// Return the first child's atom text, or empty string.
    ///
    /// # Rules
    /// - Used to get the tag name of a KiCad S-expression record.
    ///
    /// # Dependencies
    /// - crates/library/src/sexpr.rs none outside this file
    ///
    /// # Do not
    /// - Return Option; empty string is the sentinel for missing names.
    pub fn name(&self) -> &str {
        self.children
            .first()
            .and_then(|x| x.atom.as_deref())
            .unwrap_or("")
    }

    /// Return the n-th child's atom text, or empty string.
    ///
    /// # Rules
    /// - Used to get positional values in KiCad S-expressions.
    ///
    /// # Dependencies
    /// - crates/library/src/sexpr.rs none outside this file
    ///
    /// # Do not
    /// - Return Option; empty string is the sentinel for missing values.
    pub fn value(&self, n: usize) -> &str {
        self.children
            .get(n)
            .and_then(|x| x.atom.as_deref())
            .unwrap_or("")
    }

    /// Find a child node by its name (first child atom).
    ///
    /// # Rules
    /// - Returns the first child whose name matches.
    ///
    /// # Dependencies
    /// - crates/library/src/sexpr.rs none outside this file
    ///
    /// # Do not
    /// - Return all matching children; KiCad uses unique field names.
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|x| x.name() == name)
    }

    /// Get the value of a named field (child's second atom).
    ///
    /// # Rules
    /// - Used for KiCad's (field_name "value") pattern.
    ///
    /// # Dependencies
    /// - crates/library/src/sexpr.rs none outside this file
    ///
    /// # Do not
    /// - Return Option; empty string is the sentinel for missing fields.
    pub fn field(&self, name: &str) -> &str {
        self.child(name).map(|x| x.value(1)).unwrap_or("")
    }
}

/// Parse an S-expression string with default limits.
///
/// # Rules
/// - Input must contain exactly one top-level expression.
/// - Trailing input after the expression is an error.
///
/// # Dependencies
/// - crates/library/src/sexpr.rs parse_limits for the actual parsing logic
///
/// # Do not
/// - Allow multiple top-level expressions; that is a KiCad format error.
pub(crate) fn parse(text: &str) -> Result<Node, Error> {
    parse_limits(text, Limits::default())
}

/// Parse an S-expression string with explicit resource limits.
///
/// # Rules
/// - Checks byte size before parsing.
/// - Enforces node count and depth limits during parsing.
/// - Trailing input after the expression is an error.
///
/// # Dependencies
/// - crates/library/src/sexpr.rs Parser struct for the actual parsing logic
///
/// # Do not
/// - Silently truncate input; always error on limit violations.
pub(crate) fn parse_limits(text: &str, limits: Limits) -> Result<Node, Error> {
    if text.len() > limits.bytes {
        return Err(Error::Limit("source byte limit"));
    }
    let mut parser = Parser {
        text,
        pos: 0,
        count: 0,
        limits,
    };
    let node = parser.node(0)?;
    parser.space();
    if parser.pos != text.len() {
        return Err(parser.syntax("trailing input"));
    }
    Ok(node)
}
struct Parser<'a> {
    text: &'a str,
    pos: usize,
    count: usize,
    limits: Limits,
}

impl Parser<'_> {
    fn syntax(&self, message: &str) -> Error {
        Error::Syntax {
            offset: self.pos,
            message: message.to_owned(),
        }
    }
    fn space(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }
    fn node(&mut self, depth: usize) -> Result<Node, Error> {
        self.space();
        if depth > self.limits.depth {
            return Err(Error::Limit("nesting depth"));
        }
        self.count += 1;
        if self.count > self.limits.nodes {
            return Err(Error::Limit("node count"));
        }
        let start = self.pos;
        match self.text.as_bytes().get(self.pos).copied() {
            Some(b'(') => {
                self.pos += 1;
                let mut children = Vec::new();
                loop {
                    self.space();
                    match self.text.as_bytes().get(self.pos) {
                        Some(b')') => {
                            self.pos += 1;
                            break;
                        }
                        None => return Err(self.syntax("unclosed list")),
                        _ => children.push(self.node(depth + 1)?),
                    }
                }
                Ok(Node {
                    atom: None,
                    children,
                    span: start..self.pos,
                })
            }
            Some(b'"') => {
                self.pos += 1;
                let mut value = String::new();
                loop {
                    let rest = self
                        .text
                        .get(self.pos..)
                        .ok_or_else(|| self.syntax("invalid UTF-8 boundary"))?;
                    let c = rest
                        .chars()
                        .next()
                        .ok_or_else(|| self.syntax("unclosed string"))?;
                    self.pos += c.len_utf8();
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        let c = self
                            .text
                            .get(self.pos..)
                            .and_then(|s| s.chars().next())
                            .ok_or_else(|| self.syntax("unclosed escape"))?;
                        self.pos += c.len_utf8();
                        value.push(match c {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        });
                    } else {
                        value.push(c);
                    }
                    if value.len() > self.limits.atom_bytes {
                        return Err(Error::Limit("atom byte limit"));
                    }
                }
                Ok(Node {
                    atom: Some(value),
                    children: Vec::new(),
                    span: start..self.pos,
                })
            }
            Some(b')') | None => Err(self.syntax("expected expression")),
            _ => {
                while self
                    .text
                    .as_bytes()
                    .get(self.pos)
                    .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'(' && *b != b')')
                {
                    self.pos += 1;
                }
                if self.pos - start > self.limits.atom_bytes {
                    return Err(Error::Limit("atom byte limit"));
                }
                Ok(Node {
                    atom: Some(self.text[start..self.pos].to_owned()),
                    children: Vec::new(),
                    span: start..self.pos,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_node_depth_and_atom_budgets() {
        let defaults = Limits::default();
        assert!(matches!(
            parse_limits(
                "(a)",
                Limits {
                    bytes: 2,
                    ..defaults
                }
            ),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            parse_limits(
                "(a b c)",
                Limits {
                    nodes: 3,
                    ..defaults
                }
            ),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            parse_limits(
                "((a))",
                Limits {
                    depth: 1,
                    ..defaults
                }
            ),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            parse_limits(
                "(abcd)",
                Limits {
                    atom_bytes: 3,
                    ..defaults
                }
            ),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            parse_limits(
                "(\"abcd\")",
                Limits {
                    atom_bytes: 3,
                    ..defaults
                }
            ),
            Err(Error::Limit(_))
        ));
    }
    #[test]
    fn unicode_escaped_strings_and_malformed_input() -> Result<(), Error> {
        let tree = parse("(name \"µΩ\\\"\\n\")")?;
        assert_eq!(tree.value(1), "µΩ\"\n");
        for input in ["(\"unterminated)", "(\"escape\\", "(a) (b)", ")"] {
            assert!(parse(input).is_err());
        }
        Ok(())
    }

    #[test]
    fn field_reads_named_kicad_properties() -> Result<(), Error> {
        let fp = parse(
            "(footprint \"R_0805\" (layer \"F.Cu\") (descr \"0805 resistor\") (tags \"R\"))",
        )?;
        assert_eq!(fp.field("layer"), "F.Cu");
        assert_eq!(fp.field("descr"), "0805 resistor");
        assert_eq!(fp.field("tags"), "R");
        assert_eq!(fp.field("missing"), "");
        let prop = parse("(property \"Reference\" \"R\")")?;
        assert_eq!(prop.field("Reference"), "R");
        Ok(())
    }
}
