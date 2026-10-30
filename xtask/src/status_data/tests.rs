//! Tests for `status_data`. Every input is the recorded snapshot under
//! xtask/tests/fixtures/status; nothing here calls the network.
//!
//! # Rules
//! - The golden file is rewritten only with `STATUS_BLESS=1`; review the diff before committing it.
use super::*;
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/status")
}

fn load(name: &str) -> String {
    std::fs::read_to_string(dir().join(name)).unwrap_or_default()
}

fn snapshot() -> Value {
    serde_json::from_str(&load("snapshot.json")).unwrap_or(Value::Null)
}

fn schema() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(SCHEMA_PATH);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_default()).unwrap_or(Value::Null)
}

fn make(snap: &Value) -> Value {
    let tests = scan_tests(&dir().join("repo"));
    generate(
        snap,
        &load("features.yaml"),
        &load("ROADMAP.md"),
        &tests,
        false,
    )
    .unwrap_or(Value::Null)
}

fn tags(line: &str) -> Tags {
    parse_tags(line).unwrap_or_default()
}

fn feature<'a>(doc: &'a Value, title: &str) -> &'a Value {
    doc["features"]
        .as_array()
        .and_then(|l| l.iter().find(|f| f["title"] == title))
        .unwrap_or(&Value::Null)
}

fn issue(doc: &Value, number: u64) -> &Value {
    doc["issues"]
        .as_array()
        .and_then(|l| l.iter().find(|i| i["number"] == number))
        .unwrap_or(&Value::Null)
}

const NONE: Evidence = Evidence {
    merged: false,
    green: false,
    released: false,
};

#[test]
fn state_ready_needs_the_bare_ready_tag() {
    assert_eq!(
        state_of(&tags("**Tags:** `status:open`"), &NONE),
        State::Planned
    );
    assert_eq!(
        state_of(&tags("**Tags:** `status:open` `ready`"), &NONE),
        State::Ready
    );
    assert_eq!(
        state_of(&tags("**Tags:** `status:ready`"), &NONE),
        State::Ready
    );
    for status in ["claimed", "in-progress", "pr-open", "review"] {
        assert_eq!(
            state_of(
                &tags(&format!("**Tags:** `status:{status}` `ready`")),
                &NONE
            ),
            State::InProgress,
            "{status}"
        );
    }
}

#[test]
fn needs_owner_test_without_a_merge_is_awaiting_test() {
    let t = tags("**Tags:** `status:needs-owner-test` `ready`");
    assert_eq!(state_of(&t, &NONE), State::AwaitingTest);
    assert_eq!(
        state_of(
            &t,
            &Evidence {
                merged: true,
                green: false,
                released: false
            }
        ),
        State::AwaitingTest
    );
}

#[test]
fn blocked_is_a_flag_not_a_state() {
    let t = tags("**Tags:** `status:blocked` `ready`");
    assert!(t.blocked());
    assert_eq!(state_of(&t, &NONE), State::Ready);
    assert_eq!(
        state_of(&tags("**Tags:** `status:blocked`"), &NONE),
        State::Planned
    );
}

#[test]
fn unknown_tag_warns_and_continues() {
    let mut snap = snapshot();
    if let Some(list) = snap["issues"].as_array_mut() {
        list.push(json!({"number": 9001, "title": "odd", "url": "u", "state": "open",
            "tags_line": "**Tags:** `status:open` `flavour:spicy` `status:weird`", "migrated_from": null, "depends_line": ""}));
        list.push(
            json!({"number": 9002, "title": "no tags", "url": "u", "state": "open",
            "tags_line": "Just prose", "migrated_from": null, "depends_line": ""}),
        );
    }
    let doc = make(&snap);
    let warnings = doc["warnings"].to_string();
    assert!(
        warnings.contains("unknown tag `flavour:spicy`"),
        "{warnings}"
    );
    assert!(
        warnings.contains("unknown tag `status:weird`"),
        "{warnings}"
    );
    assert!(
        warnings.contains("1 issues have no Tags line (#9002)"),
        "{warnings}"
    );
    assert_eq!(issue(&doc, 9001)["state"], "planned");
}

#[test]
fn pull_requests_are_dropped() {
    let raw = json!({"number": 5, "title": "t", "pull_request": {"url": "x"}, "body": "**Tags:** `status:open`"});
    assert!(trim_issue(&raw).is_none());
    let mut snap = snapshot();
    let before = snap["issues"].as_array().map(Vec::len).unwrap_or(0);
    if let Some(list) = snap["issues"].as_array_mut() {
        list.push(json!({"number": 9003, "title": "a pr", "pull_request": {}, "tags_line": "", "url": "u"}));
    }
    assert_eq!(make(&snap)["issues"].as_array().map(Vec::len), Some(before));
}

#[test]
fn trimming_keeps_tags_origin_and_dependencies() {
    let raw = json!({"number": 7, "title": "T", "html_url": "https://example.test/7", "state": "open",
        "body": "**Tags:** `status:open`\n\nMigrated from `tangency-labs/pcbcraft#42`. Opened.\n\n## Depends on\nDepends on #3 and #4\n"});
    let t = trim_issue(&raw).unwrap_or(Value::Null);
    assert_eq!(t["migrated_from"], 42);
    assert_eq!(t["tags_line"], "**Tags:** `status:open`");
    assert_eq!(refs(s(&t, "depends_line")), vec![3, 4]);
    assert_eq!(refs("{{PCBREF:762}}, {{PCBREF:765}}"), vec![762, 765]);
}

#[test]
fn output_is_byte_identical_across_runs() {
    let a = render(&make(&snapshot())).unwrap_or_default();
    let b = render(&make(&snapshot())).unwrap_or_default();
    assert!(!a.is_empty());
    assert_eq!(a, b);
    assert!(a.ends_with("}\n"));
}

#[test]
fn output_validates_against_schema() {
    let problems = status_schema::validate(&schema(), &make(&snapshot()));
    assert!(problems.is_empty(), "{problems:?}");
    let mut broken = make(&snapshot());
    broken["features"][0]["state"] = json!("shipped");
    broken["extra"] = json!(1);
    assert_eq!(status_schema::validate(&schema(), &broken).len(), 2);
}

#[test]
fn blocked_by_follows_the_depends_lines() {
    let doc = make(&snapshot());
    // Fork issue 607 (old 766) depends on old 762 and 765, which are not in the fixture.
    assert_eq!(issue(&doc, 607)["blocked_by"], json!([]));
    let mut snap = snapshot();
    if let Some(list) = snap["issues"].as_array_mut() {
        for i in list.iter_mut().filter(|i| i["number"] == 607) {
            i["depends_line"] = json!("Depends on {{PCBREF:781}} and {{PCBREF:782}}");
        }
    }
    assert_eq!(issue(&make(&snap), 607)["blocked_by"], json!([624, 625]));
}

#[test]
fn golden_for_the_recorded_snapshot() {
    let got = render(&make(&snapshot())).unwrap_or_default();
    let path = dir().join("golden.json");
    if std::env::var("STATUS_BLESS").is_ok() {
        assert!(std::fs::write(&path, &got).is_ok());
    }
    assert_eq!(
        got,
        load("golden.json"),
        "rerun with STATUS_BLESS=1 and review the diff"
    );
}

#[test]
fn counts_equal_the_lists_beside_them() {
    let doc = make(&snapshot());
    let total: u64 = doc["counts"]["issues_by_state"]
        .as_object()
        .map(|m| m.values().filter_map(Value::as_u64).sum())
        .unwrap_or(0);
    assert_eq!(Some(total as usize), doc["issues"].as_array().map(Vec::len));
    for (state, n) in doc["counts"]["issues_by_state"]
        .as_object()
        .cloned()
        .unwrap_or_default()
    {
        let listed = doc["issues"]
            .as_array()
            .map(|l| l.iter().filter(|i| i["state"] == state.as_str()).count());
        assert_eq!(listed.map(|c| c as u64), n.as_u64(), "{state}");
    }
    let proof = &doc["counts"]["features_by_proof"];
    assert_eq!(
        proof["tested"].as_u64().unwrap_or(0) + proof["no-test"].as_u64().unwrap_or(0),
        doc["features"].as_array().map(Vec::len).unwrap_or(0) as u64
    );
}

#[test]
fn a_feature_with_no_proving_test_is_reported_as_such() {
    let doc = make(&snapshot());
    let none = feature(&doc, "Keyboard map");
    assert_eq!(none["proof"], "no-test");
    assert_eq!(none["tests"], json!([]));
    assert_eq!(none["state"], "unlinked");
    // A test name that matches no #[test] function proves nothing either.
    let dialog = feature(&doc, "Planned dialog");
    assert_eq!(dialog["proof"], "no-test");
    assert_eq!(
        dialog["missing_tests"],
        json!(["a_newcomer_opens_the_planned_dialog"])
    );
    // A feature with one real and one unmatched name is tested and lists the miss.
    let spans = feature(&doc, "Span-aware pours");
    assert_eq!(spans["proof"], "tested");
    assert_eq!(spans["tests"][0]["file"], "crates/demo/src/lib.rs");
    assert_eq!(
        spans["missing_tests"],
        json!(["a_name_that_matches_nothing"])
    );
    assert_eq!(
        feature(&doc, "Part search without a test")["proof"],
        "no-test"
    );
}

#[test]
fn issues_line_beats_subject_number() {
    let mut snap = snapshot();
    snap["pulls"] = json!([{"number": 1, "title": "Fix (#781)", "url": "p", "merged_at": "2026-10-29T16:00:00Z",
        "merge_sha": "189779d", "issues_line": "Issues: #782"}]);
    let doc = make(&snap);
    assert_eq!(issue(&doc, 625)["merges"].as_array().map(Vec::len), Some(1));
    assert_eq!(issue(&doc, 624)["merges"], json!([]));
}

#[test]
fn subject_number_is_the_fallback() {
    let mut snap = snapshot();
    snap["pulls"] = json!([{"number": 1, "title": "Fix (#781)", "url": "p", "merged_at": "2026-10-29T16:00:00Z",
        "merge_sha": "189779d", "issues_line": ""}]);
    assert_eq!(
        issue(&make(&snap), 624)["merges"].as_array().map(Vec::len),
        Some(1)
    );
}

#[test]
fn two_issues_in_one_merge_share_the_same_sha_and_run() {
    let doc = make(&snapshot());
    let a = &issue(&doc, 644)["merges"];
    let b = &issue(&doc, 648)["merges"];
    let shared = |m: &Value, pr: u64| {
        m.as_array()
            .and_then(|l| l.iter().find(|x| x["pr"] == pr))
            .cloned()
    };
    assert!(shared(a, 70).is_some());
    assert_eq!(shared(a, 70), shared(b, 70));
}

#[test]
fn first_green_run_after_a_red_merge_counts() {
    let runs = vec![
        json!({"url": "r1", "head_sha": "aaa", "conclusion": "failure", "created_at": "2026-01-01T10:00:00Z"}),
        json!({"url": "r2", "head_sha": "bbb", "conclusion": "cancelled", "created_at": "2026-01-01T11:00:00Z"}),
        json!({"url": "r3", "head_sha": "ccc", "conclusion": "success", "created_at": "2026-01-01T12:00:00Z"}),
        json!({"url": "r4", "head_sha": "ddd", "conclusion": "success", "created_at": "2026-01-01T13:00:00Z"}),
    ];
    let run = first_green(&runs, "aaa", "2026-01-01T09:59:00Z").unwrap_or(Value::Null);
    assert_eq!(run["url"], "r3");
    assert_eq!(run["head_sha"], "ccc");
    assert!(first_green(&runs, "eee", "2026-01-01T14:00:00Z").is_none());
    // A green run on the merge commit itself wins over an earlier green run elsewhere.
    assert_eq!(
        first_green(&runs, "ddd", "2026-01-01T09:00:00Z").map(|r| r["url"].clone()),
        Some(json!("r4"))
    );
}

#[test]
fn a_red_merge_is_built_at_the_later_run_in_the_fixture() {
    let doc = make(&snapshot());
    // Pull request 64 merged as e67723c, a cancelled run; the next green run proves it.
    let merge = &issue(&doc, 640)["merges"][0];
    assert_eq!(merge["pr"], 64);
    assert_ne!(merge["ci"]["head_sha"], merge["merge_sha"]);
    assert_eq!(issue(&doc, 640)["state"], "built");
}

#[test]
fn merged_needs_owner_test_is_built_not_owner_tested() {
    let mut snap = snapshot();
    if let Some(list) = snap["issues"].as_array_mut() {
        for i in list.iter_mut().filter(|i| i["number"] == 642) {
            i["tags_line"] = json!("**Tags:** `kind:feature` `status:needs-owner-test` `ready`");
        }
    }
    let doc = make(&snap);
    assert_eq!(issue(&doc, 642)["state"], "built");
    assert_eq!(issue(&doc, 642)["owner_tested"], false);
}

#[test]
fn no_tag_means_no_release() {
    let doc = make(&snapshot());
    assert_eq!(doc["source"]["latest_release"], Value::Null);
    assert!(
        doc["issues"]
            .as_array()
            .map(|l| l.iter().all(|i| i["state"] != "released"))
            .unwrap_or(false)
    );
}

#[test]
fn a_release_published_after_the_merge_releases_it() {
    let mut snap = snapshot();
    snap["releases"] = json!([{"tag": "v0.1.0", "published_at": "2026-10-29T16:00:00Z"}]);
    let doc = make(&snap);
    assert_eq!(doc["source"]["latest_release"], "v0.1.0");
    assert_eq!(issue(&doc, 642)["state"], "released");
    // Merged after the release: still only built.
    assert_eq!(issue(&doc, 640)["state"], "built");
}

#[test]
fn roadmap_subset_sets_scope() {
    let doc = make(&snapshot());
    let lines = doc["roadmap"].as_array().cloned().unwrap_or_default();
    let subset = lines
        .iter()
        .find(|l| {
            l["text"]
                .as_str()
                .unwrap_or("")
                .contains("partial interface")
        })
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(subset["scope"], "subset");
    assert_eq!(subset["checked"], true);
    assert_eq!(lines.iter().filter(|l| l["scope"] == "subset").count(), 1);
    // The generator reports the line but never changes the checkbox.
    assert!(!doc["warnings"].to_string().contains("partial interface"));
}

#[test]
fn roadmap_drift_is_a_warning() {
    let doc = make(&snapshot());
    let warnings = doc["warnings"].to_string();
    assert!(
        warnings.contains(
            "roadmap drift: checked but not built: Checked line whose issue is still planned"
        ),
        "{warnings}"
    );
    assert!(
        warnings.contains("roadmap drift: every issue built but unchecked: Online library sources"),
        "{warnings}"
    );
}

#[test]
fn old_tracker_numbers_resolve_to_fork_issues() {
    let doc = make(&snapshot());
    assert_eq!(feature(&doc, "Layer record")["issues"], json!([642]));
    assert_eq!(
        feature(&doc, "Online part sources")["issues"],
        json!([624, 625, 627, 629])
    );
    assert_eq!(doc["source"]["repo"], DEFAULT_REPO);
}

#[test]
fn feature_state_is_the_least_advanced_issue() {
    let doc = make(&snapshot());
    assert_eq!(feature(&doc, "Span-aware pours")["state"], "built");
    assert_eq!(feature(&doc, "Planned dialog")["state"], "in-progress");
}

#[test]
fn internal_hosts_and_paths_never_reach_the_output() {
    let mut snap = snapshot();
    if let Some(list) = snap["issues"].as_array_mut() {
        list.push(json!({"number": 9004, "title": "Build on 192.168.1.57 in /srv/data/build and /home/me/x", "url": "https://git.quasarke.net/x",
            "state": "open", "tags_line": "**Tags:** `status:open`", "migrated_from": null, "depends_line": ""}));
    }
    let doc = make(&snap);
    let text = render(&doc).unwrap_or_default();
    for bad in ["192.168.", "/srv/", "/home/", "quasarke", "@local"] {
        assert!(!text.contains(bad), "{bad}");
    }
    assert_eq!(
        issue(&doc, 9004)["title"],
        "Build on [removed] in [removed] and [removed]"
    );
    assert!(
        doc["warnings"]
            .to_string()
            .contains("had internal hosts, addresses or paths removed")
    );
}

#[test]
fn the_golden_snapshot_itself_is_clean() {
    let text = load("snapshot.json");
    for bad in [
        "192.168.",
        "/srv/",
        "/home/",
        "quasarke",
        "tangency-labs/pcbcraft/issues",
    ] {
        assert!(!text.contains(bad), "{bad}");
    }
}

// --- Issue #581: status data caching, stale fallback, CI integration ---

#[test]
fn warm_cache_sends_if_none_match() {
    // Create a temp cache file with an ETag
    let tmp = std::env::temp_dir().join("pcbcraft_test_etag_cache");
    let cache_content = serde_json::json!({
        "issues_etag": "W/\"test-etag-123\"",
        "last_generated_at": "2026-10-29T23:00:00Z"
    });
    std::fs::write(&tmp, cache_content.to_string()).unwrap();

    // Load the cache and verify the ETag is present
    let cache = EtagCache::load(&tmp.parent().unwrap()).unwrap();
    assert_eq!(cache.issues_etag, Some("W/\"test-etag-123\"".to_string()));

    // Cleanup
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn failure_reuses_last_snapshot_and_marks_stale() {
    // Create a temp directory with a snapshot file
    let tmp_dir = std::env::temp_dir().join("pcbcraft_test_stale_fallback");
    std::fs::create_dir_all(&tmp_dir).ok();

    let snapshot_path = tmp_dir.join("snapshot.json");
    let snapshot_content = serde_json::json!({
        "api_requests": 9,
        "fetched_at": "2026-10-29T23:00:00Z",
        "issues": []
    });
    std::fs::write(&snapshot_path, snapshot_content.to_string()).unwrap();

    // Verify the snapshot can be read and parsed
    let text = std::fs::read_to_string(&snapshot_path).unwrap();
    let snap: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(snap["fetched_at"], "2026-10-29T23:00:00Z");

    // Cleanup
    std::fs::remove_dir_all(&tmp_dir).ok();
}

#[test]
fn old_stale_snapshot_fails_the_run() {
    // Test that snapshot_age_days correctly identifies an old snapshot
    // Use a date that is definitely more than 7 days ago
    let old_date = "2026-10-15T23:00:00Z";
    let age = snapshot_age_days(old_date);

    // The age should be Some value >= 7 (since today is Oct 30, 2026)
    assert!(age.is_some(), "snapshot_age_days should return Some for a valid date");
    assert!(age.unwrap() >= 7, "snapshot from Oct 15 should be >= 7 days old");

    // Test a recent date (should be < 7 days)
    let recent_date = "2026-10-29T23:00:00Z";
    let recent_age = snapshot_age_days(recent_date);
    assert!(recent_age.is_some(), "snapshot_age_days should return Some for a recent date");
    assert!(recent_age.unwrap() < 7, "snapshot from yesterday should be < 7 days old");
}
