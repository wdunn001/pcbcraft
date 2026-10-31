//! Stories and checks that every place showing findings tells the same
//! numbers: the status strip, the Findings tab badge, the Output summary and
//! the Findings list (`finding_set.rs`).

use super::super::bottom_sheet::SeverityFilter;
use super::super::*;
use super::support::*;

const FILTERS: [SeverityFilter; 4] = [
    SeverityFilter::All,
    SeverityFilter::Errors,
    SeverityFilter::Warnings,
    SeverityFilter::Notes,
];

fn examples() -> Result<Vec<std::path::PathBuf>, String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples");
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pcbcraft"))
        .collect();
    found.sort();
    if found.is_empty() {
        return Err("no example projects found".into());
    }
    Ok(found)
}

fn open_example(app: &mut App, path: &std::path::Path) -> Result<(), String> {
    let text = path.to_str().ok_or("the example path is text")?.to_owned();
    if !app.tool_call("open", json!({ "path": text })) {
        return Err(format!("{} does not open: {}", path.display(), app.message));
    }
    app.refresh();
    Ok(())
}

/// Counts taken by hand from the raw check output and the library findings,
/// the oracle the shared set is held against.
fn by_hand(app: &mut App) -> (usize, usize, usize) {
    app.refresh_footprint_quality();
    let mut c = (0, 0, 0);
    for key in ["erc", "drc"] {
        for f in app
            .checks
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match f.get("severity").and_then(Value::as_str) {
                Some("error") => c.0 += 1,
                Some("warning") => c.1 += 1,
                _ => c.2 += 1,
            }
        }
    }
    for f in &app.library.quality.findings {
        if f.error {
            c.0 += 1;
        } else {
            c.1 += 1;
        }
    }
    c
}

/// Every place agrees with the hand count and with each other, under every
/// filter.
fn assert_agree(app: &mut App, what: &str) {
    let (e, w, n) = by_hand(app);
    let set = app.findings();
    let all = set.counts();
    assert_eq!((all.errors, all.warnings, all.notes), (e, w, n), "{what}");
    assert_eq!(all.strip(), (e, w + n), "{what}: status strip");
    assert_eq!(all.total(), e + w + n, "{what}: badge");
    for filter in FILTERS {
        let shown = set.visible_counts(filter);
        let listed = set.visible(filter).count();
        assert_eq!(shown.total(), listed, "{what}: {filter:?} list length");
        let want = match filter {
            SeverityFilter::All => (e, w, n),
            SeverityFilter::Errors => (e, 0, 0),
            SeverityFilter::Warnings => (0, w, 0),
            SeverityFilter::Notes => (0, 0, n),
        };
        assert_eq!(
            (shown.errors, shown.warnings, shown.notes),
            want,
            "{what}: {filter:?}"
        );
    }
    let mut last = 0;
    for row in set.visible(SeverityFilter::All) {
        let rank = row.loudness as usize;
        assert!(rank >= last, "{what}: errors sort above warnings");
        last = rank;
    }
}

/// Counts agree on every example project, as opened and after hiding a
/// finding.
#[test]
fn finding_counts_agree_on_every_example() -> Result<(), String> {
    for path in examples()? {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        open_example(&mut app, &path)?;
        let name = path.display().to_string();
        assert_agree(&mut app, &name);
        let before = app.findings().counts().total();
        let target = ["erc", "drc"].iter().find_map(|key| {
            app.checks
                .get(*key)
                .and_then(Value::as_array)
                .and_then(|list| {
                    list.iter().find_map(|f| {
                        let ids = f.get("objects")?.as_array()?.clone();
                        let code = f.get("code")?.as_str()?.to_owned();
                        (!ids.is_empty()).then_some((code, ids))
                    })
                })
        });
        if let Some((code, ids)) = target {
            assert!(
                app.tool_call("rules.exclude", json!({"code": code, "ids": ids})),
                "{name}: hide works: {}",
                app.message
            );
            app.refresh();
            assert_agree(&mut app, &format!("{name} after hiding {code}"));
            assert!(
                app.findings().counts().total() < before,
                "{name}: hiding {code} removes it from every count"
            );
        }
    }
    Ok(())
}

/// A newcomer opens the sample with known problems and sees the same counts
/// on the status strip, on the Findings tab and in the list, with the error
/// listed above the warnings.
#[test]
fn a_newcomer_sees_the_same_finding_counts_everywhere() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("showcase-with-findings.pcbcraft");
    open_example(&mut app, &path)?;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.region().bottom_open, "View > Checks opens the sheet");
    let set = app.findings();
    let counts = set.counts();
    let (errors, warnings) = counts.strip();
    assert!(errors >= 1, "the sample carries an error");
    let strip_errors = app.catalog.plural("board.footer.errors", errors as u64);
    let strip_warnings = app.catalog.plural("board.footer.warnings", warnings as u64);
    assert!(
        paints_text(&ctx, &mut app, &strip_errors)?,
        "the status strip says {strip_errors}"
    );
    assert!(
        paints_text(&ctx, &mut app, &strip_warnings)?,
        "the status strip says {strip_warnings}"
    );
    assert!(
        paints_text(&ctx, &mut app, &counts.total().to_string())?,
        "the Findings tab badge says {}",
        counts.total()
    );
    let rows: Vec<_> = set.visible(SeverityFilter::All).cloned().collect();
    assert_eq!(rows.len(), counts.total(), "the list has as many rows");
    let first = rows.first().ok_or("the list has rows")?;
    assert_eq!(
        first.loudness,
        super::super::finding_set::Loudness::Error,
        "the list starts with the error"
    );
    let y_of = |app: &mut App, needle: &str| -> Option<f32> {
        let output = frame(&ctx, app, vec![], egui::Modifiers::NONE);
        output.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text().contains(needle) => Some(t.pos.y),
            _ => None,
        })
    };
    let error_y = y_of(&mut app, &first.message).ok_or("the error row is on screen")?;
    for row in rows.iter().skip(1) {
        if let Some(y) = y_of(&mut app, &row.message) {
            assert!(error_y <= y, "the error sits above {}", row.message);
        }
    }
    // Pick each filter and read the list against the same numbers.
    for filter in FILTERS {
        app.sheet.filter = filter;
        let shown = app.findings().visible_counts(filter);
        let want = match filter {
            SeverityFilter::All => counts.total(),
            SeverityFilter::Errors => counts.errors,
            SeverityFilter::Warnings => counts.warnings,
            SeverityFilter::Notes => counts.notes,
        };
        assert_eq!(shown.total(), want, "{filter:?} lists what the counts say");
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok(())
}

/// `--tab checks` lands on Layout with the Findings sheet expanded.
#[test]
fn the_checks_start_option_opens_layout_with_the_sheet() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, Some("checks"));
    app.refresh();
    let ctx = egui::Context::default();
    for _ in 0..6 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(app.tab, Tab::Pcb, "checks opens Layout");
    assert!(app.region().bottom_open, "the Findings sheet is expanded");
    assert_eq!(app.region().bottom_tab, "findings");
    Ok(())
}
