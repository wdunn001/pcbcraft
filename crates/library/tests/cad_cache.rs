use pcbcraft_library::cad_cache::{CadCache, content_hash, default_cache_dir_with};
use pcbcraft_library::sources::SourceError;
use std::path::PathBuf;

const PATH: &str = "Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod";
const REV_A: &str = "a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9";
const REV_B: &str = "b0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9";

fn dir() -> Result<tempfile::TempDir, std::io::Error> {
    tempfile::tempdir()
}

#[test]
fn cache_stores_a_file_under_its_upstream_path_and_hash() -> Result<(), Box<dyn std::error::Error>>
{
    let tmp = dir()?;
    let cache = CadCache::open(tmp.path());
    let hash = cache.put("open-library", PATH, Some(REV_A), b"(footprint)")?;
    assert_eq!(hash, REV_A);
    let stored = tmp
        .path()
        .join("files/open-library/Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod")
        .join(REV_A);
    assert!(stored.is_file(), "stored at {}", stored.display());
    assert_eq!(std::fs::read(stored)?, b"(footprint)");
    Ok(())
}

#[test]
fn cache_serves_a_stored_file_without_any_source() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = dir()?;
    CadCache::open(tmp.path()).put("open-library", PATH, Some(REV_A), b"bytes")?;
    // A new cache object over the same folder: the process "restarted" and is offline.
    let again = CadCache::open(tmp.path());
    assert_eq!(
        again.get("open-library", PATH, Some(REV_A))?.as_deref(),
        Some(&b"bytes"[..])
    );
    assert_eq!(
        again.get("open-library", PATH, None)?.as_deref(),
        Some(&b"bytes"[..]),
        "asking for the newest also answers"
    );
    assert_eq!(again.get("open-library", "Other/file", None)?, None);
    Ok(())
}

#[test]
fn cache_keeps_two_hashes_of_one_path() -> Result<(), SourceError> {
    let tmp = tempfile::tempdir().map_err(|e| SourceError::Unavailable(e.to_string()))?;
    let cache = CadCache::open(tmp.path());
    cache.put("s", PATH, Some(REV_A), b"old")?;
    cache.put("s", PATH, Some(REV_B), b"new")?;
    assert_eq!(
        cache.get("s", PATH, Some(REV_A))?.as_deref(),
        Some(&b"old"[..])
    );
    assert_eq!(
        cache.get("s", PATH, Some(REV_B))?.as_deref(),
        Some(&b"new"[..])
    );
    assert_eq!(
        cache.get("s", PATH, None)?.as_deref(),
        Some(&b"new"[..]),
        "the newest put answers a lookup with no hash"
    );
    Ok(())
}

#[test]
fn a_file_without_a_revision_is_keyed_by_its_content() -> Result<(), SourceError> {
    let cache = CadCache::in_memory();
    let hash = cache.put("s", PATH, None, b"abc")?;
    assert_eq!(hash, content_hash(b"abc"));
    assert_ne!(hash, content_hash(b"abd"));
    assert_eq!(
        cache.get("s", PATH, Some(&hash))?.as_deref(),
        Some(&b"abc"[..])
    );
    Ok(())
}

#[test]
fn cache_refuses_paths_that_climb_out_of_the_root() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = dir()?;
    let cache = CadCache::open(tmp.path());
    for bad in [
        "../escape",
        "a/../../b",
        "/etc/passwd",
        "a//b",
        "a\\b",
        "a/./b",
        "",
        "C:evil",
    ] {
        assert!(
            cache.put("s", bad, Some(REV_A), b"x").is_err(),
            "{bad:?} must be refused"
        );
        assert!(
            cache.get("s", bad, None).is_err(),
            "{bad:?} must be refused on read"
        );
    }
    assert!(cache.put("../s", PATH, Some(REV_A), b"x").is_err());
    assert!(cache.put("s", PATH, Some("../../x"), b"x").is_err());
    let outside: PathBuf = tmp
        .path()
        .parent()
        .map(|p| p.join("escape"))
        .unwrap_or_default();
    assert!(!outside.exists());
    Ok(())
}

#[test]
fn a_corrupt_cache_file_is_a_miss() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = dir()?;
    let cache = CadCache::open(tmp.path());
    cache.put("s", PATH, Some(REV_A), b"good bytes")?;
    let file = tmp
        .path()
        .join("files/s/Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod")
        .join(REV_A);
    std::fs::write(&file, b"damaged")?;
    assert_eq!(
        cache.get("s", PATH, Some(REV_A))?,
        None,
        "damaged file is a miss"
    );
    assert!(
        !file.exists(),
        "and it is removed so a refetch can replace it"
    );
    Ok(())
}

#[test]
fn a_corrupt_index_is_a_miss() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = dir()?;
    let cache = CadCache::open(tmp.path());
    cache.put_listing("s", "symbols/root.json", "[\"Device\"]", 100)?;
    let got = cache.get_listing("s", "symbols/root.json")?;
    assert_eq!(
        got.map(|l| (l.stored_at, l.body)),
        Some((100, "[\"Device\"]".to_owned()))
    );
    let path = tmp.path().join("listings/s/symbols/root.json");
    std::fs::write(&path, "not a stored listing")?;
    assert_eq!(cache.get_listing("s", "symbols/root.json")?, None);
    assert!(!path.exists());
    cache.put_listing("s", "k", "body", 5)?;
    cache.forget_listing("s", "k")?;
    assert_eq!(cache.get_listing("s", "k")?, None);
    Ok(())
}

#[test]
fn cache_evicts_least_recently_used_over_the_cap() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = dir()?;
    let cache = CadCache::open(tmp.path()).with_cap(250);
    let hash_a = cache.put("s", "L/a", Some(REV_A), &[1u8; 100])?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    let hash_b = cache.put("s", "L/b", Some(REV_B), &[2u8; 100])?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    // Reading a makes b the least recently used.
    assert!(cache.get("s", "L/a", Some(&hash_a))?.is_some());
    std::thread::sleep(std::time::Duration::from_millis(30));
    cache.put("s", "L/c", Some("c0b1c2d3e4f5a6b7"), &[3u8; 100])?;
    assert!(
        cache.disk_bytes() <= 250,
        "over the cap: {}",
        cache.disk_bytes()
    );
    assert!(
        cache.get("s", "L/b", Some(&hash_b))?.is_none(),
        "b was evicted"
    );
    assert!(
        cache.get("s", "L/a", Some(&hash_a))?.is_some(),
        "a was kept"
    );
    Ok(())
}

#[test]
fn purge_empties_the_cache() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = dir()?;
    let cache = CadCache::open(tmp.path());
    cache.put("s", PATH, Some(REV_A), b"x")?;
    cache.put_listing("s", "k", "body", 1)?;
    cache.purge()?;
    assert_eq!(cache.get("s", PATH, None)?, None);
    assert_eq!(cache.get_listing("s", "k")?, None);
    assert_eq!(cache.disk_bytes(), 0);
    Ok(())
}

#[test]
fn a_file_over_the_size_limit_is_refused() {
    let cache = CadCache::in_memory();
    let big = vec![0u8; 8 * 1024 * 1024 + 1];
    assert!(cache.put("s", PATH, None, &big).is_err());
}

#[test]
fn default_cache_dir_follows_platform_variables() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    };
    assert_eq!(
        default_cache_dir_with(
            &env(&[("XDG_CACHE_HOME", "/x/cache"), ("HOME", "/h")]),
            "linux"
        ),
        Some(PathBuf::from("/x/cache/pcbcraft/online"))
    );
    assert_eq!(
        default_cache_dir_with(&env(&[("HOME", "/h")]), "linux"),
        Some(PathBuf::from("/h/.cache/pcbcraft/online"))
    );
    assert_eq!(
        default_cache_dir_with(&env(&[("HOME", "/Users/me")]), "macos"),
        Some(PathBuf::from("/Users/me/Library/Caches/pcbcraft/online"))
    );
    assert_eq!(
        default_cache_dir_with(&env(&[("XDG_CACHE_HOME", "relative/dir")]), "linux"),
        None,
        "a relative value is ignored"
    );
    assert_eq!(default_cache_dir_with(&env(&[]), "linux"), None);
}
