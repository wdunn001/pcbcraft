//! In-app datasheet reader pane: page image, text layer, search, copy, accessibility.

mod extract_bridge;
mod pane;

pub(crate) use pane::Pane;

/// Read a local `file://` PDF the person chose; the name for the title comes with the bytes.
///
/// # Rules
/// - Call it from the action that asked for the file, never from painting.
/// - `file:///C:/x.pdf` carries a slash before the drive letter; `file:///tmp/x.pdf` keeps its
///   root slash.
pub(crate) fn read_file_url(url: &str) -> Option<(String, Vec<u8>)> {
    let path = url.strip_prefix("file://")?;
    let rooted = format!("/{}", path.trim_start_matches('/'));
    let path = match rooted.as_bytes() {
        [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => &rooted[1..],
        _ => rooted.as_str(),
    };
    let bytes = std::fs::read(path).ok()?;
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Datasheet")
        .to_owned();
    Some((name, bytes))
}
