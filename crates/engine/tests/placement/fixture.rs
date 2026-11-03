//! Reader for the text fixtures in `tests/fixtures/placement/`.
//!
//! Format, one item per line, `#` starts a comment:
//! - `board x0 y0 x1 y1` and `keepout x0 y0 x1 y1`
//! - `cell NAME x y half_x half_y fixed|free [turns=N]`
//! - `net NAME weight CELL[:dx,dy] CELL[:dx,dy] ...`
//! - `expect KEY value ...` pinned numbers or words the tests read back
use pcbcraft_engine::placement::{Cell, Model, NetSpring, Pin, Rect};

/// One parsed fixture: the model, the cell names and the pinned values.
pub struct Fixture {
    pub model: Model,
    pub names: Vec<String>,
    pub expects: Vec<(String, Vec<String>)>,
}

fn num(t: &str) -> Result<f64, String> {
    t.parse::<f64>()
        .map_err(|e| format!("bad number {t:?}: {e}"))
}

fn rect(t: &[&str]) -> Result<Rect, String> {
    if t.len() < 5 {
        return Err("a rectangle needs four numbers".into());
    }
    Ok(Rect {
        x0: num(t[1])?,
        y0: num(t[2])?,
        x1: num(t[3])?,
        y1: num(t[4])?,
    })
}

impl Fixture {
    /// Load `tests/fixtures/placement/<name>.txt`.
    pub fn load(name: &str) -> Result<Fixture, String> {
        let path = format!(
            "{}/tests/fixtures/placement/{name}.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
        Self::parse(&text)
    }

    /// Parse fixture text.
    pub fn parse(text: &str) -> Result<Fixture, String> {
        let mut model = Model {
            cells: vec![],
            nets: vec![],
            board: Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 100.0,
                y1: 100.0,
            },
            keepouts: vec![],
        };
        let (mut names, mut expects) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("");
            let t: Vec<&str> = line.split_whitespace().collect();
            let Some(&kind) = t.first() else { continue };
            match kind {
                "board" => model.board = rect(&t)?,
                "keepout" => model.keepouts.push(rect(&t)?),
                "cell" => {
                    if t.len() < 7 {
                        return Err(format!("short cell line: {line}"));
                    }
                    let turns = t[7..]
                        .iter()
                        .find_map(|s| s.strip_prefix("turns="))
                        .map_or(Ok(0), |s| s.parse::<u8>().map_err(|e| e.to_string()))?;
                    names.push(t[1].to_string());
                    model.cells.push(Cell {
                        id: names.len() as u64,
                        x: num(t[2])?,
                        y: num(t[3])?,
                        half_x: num(t[4])?,
                        half_y: num(t[5])?,
                        turns,
                        fixed: t[6] == "fixed",
                    });
                }
                "net" => {
                    let mut pins = Vec::new();
                    for p in &t[3..] {
                        let (cell, off) = p.split_once(':').unwrap_or((p, "0,0"));
                        let (dx, dy) = off.split_once(',').ok_or("pin offset is dx,dy")?;
                        let idx = names
                            .iter()
                            .position(|n| n == cell)
                            .ok_or_else(|| format!("unknown cell {cell}"))?;
                        pins.push(Pin {
                            cell: idx,
                            dx: num(dx)?,
                            dy: num(dy)?,
                        });
                    }
                    model.nets.push(NetSpring {
                        net: model.nets.len() as u64 + 1,
                        weight: num(t[2])?,
                        pins,
                    });
                }
                "expect" => expects.push((
                    t[1].to_string(),
                    t[2..].iter().map(|s| s.to_string()).collect(),
                )),
                other => return Err(format!("unknown fixture line {other}")),
            }
        }
        Ok(Fixture {
            model,
            names,
            expects,
        })
    }

    /// Index of a named cell.
    pub fn index(&self, name: &str) -> Result<usize, String> {
        self.names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| format!("no cell {name}"))
    }

    /// The words after `expect KEY`.
    pub fn words(&self, key: &str) -> Result<Vec<String>, String> {
        self.expects
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("fixture has no expect {key}"))
    }

    /// The numbers after `expect KEY`.
    pub fn nums(&self, key: &str) -> Result<Vec<f64>, String> {
        self.words(key)?.iter().map(|w| num(w)).collect()
    }
}

/// Assert two numbers agree within `tol`, with a plain sentence.
pub fn near(what: &str, got: f64, want: f64, tol: f64) -> Result<(), String> {
    if (got - want).abs() <= tol {
        Ok(())
    } else {
        Err(format!("{what}: got {got}, want {want} within {tol}"))
    }
}
