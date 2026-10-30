//! Direct-current solver: modified nodal analysis with a hand-written dense LU.
//!
//! # Rules
//! - Unknowns: voltages of nodes 1.., then one current per voltage source. Ground is 0 V.
//! - `check` runs first; a circuit it refuses is never factored.
//! - Partial pivoting; a pivot under 1e-18 is a singular error naming the node or source.
//! - If the plain matrix is singular, one retry adds a 1e-12 S shunt from every node to ground.
//! - Index order, no hash maps, no recursion; every loop is bounded by the matrix size.
//!
//! # Dependencies
//! - docs/adr-0001-simulation.md sections 2 and 4
//! - crates/sim/src/check.rs
//! - crates/sim/src/net.rs
//!
//! # Do not
//! - Do not return a non-finite number; turn it into an error.
use crate::check::check;
use crate::net::{Element, Netlist};

/// Pivots below this are treated as zero.
const PIVOT_MIN: f64 = 1e-18;
/// Conductance to ground used by the retry, in siemens.
const SHUNT: f64 = 1e-12;

/// The current through one voltage source.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceCurrent {
    /// The source's name.
    pub name: String,
    /// Amps flowing in at the plus terminal, through the source, out of the minus terminal.
    /// A source that delivers power therefore reports a negative number.
    pub amps: f64,
}

/// The result of a DC solve.
#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    /// Volts at each node; index 0 is ground and is always 0.
    pub node_volts: Vec<f64>,
    /// One entry per voltage source, in netlist order.
    pub source_currents: Vec<SourceCurrent>,
}

/// Solves the circuit at DC.
///
/// # Rules
/// - Runs `check` first and returns its message when the circuit is unsound.
///
/// # Dependencies
/// - crates/sim/src/check.rs
///
/// # Do not
/// - Do not call it with a netlist you intend to edit; it only reads.
pub fn solve_dc(net: &Netlist) -> Result<Solution, String> {
    check(net)?;
    let nodes = net.nodes() - 1;
    let sources: Vec<&str> = net
        .elements()
        .iter()
        .filter_map(|e| match e {
            Element::VoltageSource { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let size = nodes + sources.len();
    let x = match stamp_and_solve(net, nodes, size, 0.0, &sources) {
        Ok(x) => x,
        Err(_) => stamp_and_solve(net, nodes, size, SHUNT, &sources)?,
    };
    if x.iter().any(|v| !v.is_finite()) {
        return Err("the solution is not a finite number; check the element values".to_string());
    }
    let mut node_volts = vec![0.0];
    node_volts.extend_from_slice(&x[..nodes]);
    let source_currents = sources
        .iter()
        .enumerate()
        .map(|(k, name)| SourceCurrent {
            name: (*name).to_string(),
            amps: x[nodes + k],
        })
        .collect();
    Ok(Solution {
        node_volts,
        source_currents,
    })
}

/// Builds `A x = z` with the given shunt and solves it.
fn stamp_and_solve(
    net: &Netlist,
    nodes: usize,
    size: usize,
    shunt: f64,
    sources: &[&str],
) -> Result<Vec<f64>, String> {
    let mut a = vec![0.0; size * size];
    let mut z = vec![0.0; size];
    // Matrix row for a node; ground has none.
    let row = |node: usize| node.checked_sub(1);
    for d in 0..nodes {
        a[d * size + d] += shunt;
    }
    let mut k = 0;
    for element in net.elements() {
        match element {
            Element::Resistor {
                a: p, b: q, ohms, ..
            } => {
                let g = 1.0 / ohms;
                if let Some(i) = row(*p) {
                    a[i * size + i] += g;
                }
                if let Some(j) = row(*q) {
                    a[j * size + j] += g;
                }
                if let (Some(i), Some(j)) = (row(*p), row(*q)) {
                    a[i * size + j] -= g;
                    a[j * size + i] -= g;
                }
            }
            Element::VoltageSource {
                plus, minus, volts, ..
            } => {
                let r = nodes + k;
                if let Some(i) = row(*plus) {
                    a[i * size + r] += 1.0;
                    a[r * size + i] += 1.0;
                }
                if let Some(j) = row(*minus) {
                    a[j * size + r] -= 1.0;
                    a[r * size + j] -= 1.0;
                }
                z[r] = *volts;
                k += 1;
            }
            Element::CurrentSource { from, to, amps, .. } => {
                if let Some(i) = row(*from) {
                    z[i] -= amps;
                }
                if let Some(j) = row(*to) {
                    z[j] += amps;
                }
            }
        }
    }
    lu_solve(&mut a, &mut z, size, nodes, sources)
}

/// Solves in place by LU with partial pivoting.
fn lu_solve(
    a: &mut [f64],
    z: &mut [f64],
    size: usize,
    nodes: usize,
    sources: &[&str],
) -> Result<Vec<f64>, String> {
    for col in 0..size {
        let mut best = col;
        for r in col + 1..size {
            if a[r * size + col].abs() > a[best * size + col].abs() {
                best = r;
            }
        }
        if a[best * size + col].abs() < PIVOT_MIN {
            return Err(if col < nodes {
                format!("the circuit is singular at node {}", col + 1)
            } else {
                let name = sources.get(col - nodes).copied().unwrap_or("?");
                format!("the circuit is singular at voltage source {name}")
            });
        }
        if best != col {
            for c in 0..size {
                a.swap(col * size + c, best * size + c);
            }
            z.swap(col, best);
        }
        let pivot = a[col * size + col];
        for r in col + 1..size {
            let factor = a[r * size + col] / pivot;
            if factor == 0.0 {
                continue;
            }
            for c in col..size {
                a[r * size + c] -= factor * a[col * size + c];
            }
            z[r] -= factor * z[col];
        }
    }
    let mut x = vec![0.0; size];
    for r in (0..size).rev() {
        let mut sum = z[r];
        for c in r + 1..size {
            sum -= a[r * size + c] * x[c];
        }
        x[r] = sum / a[r * size + r];
    }
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: &str, a: usize, b: usize, ohms: f64) -> Element {
        Element::Resistor {
            name: name.to_string(),
            a,
            b,
            ohms,
        }
    }
    fn v(name: &str, plus: usize, minus: usize, volts: f64) -> Element {
        Element::VoltageSource {
            name: name.to_string(),
            plus,
            minus,
            volts,
        }
    }
    fn i(name: &str, from: usize, to: usize, amps: f64) -> Element {
        Element::CurrentSource {
            name: name.to_string(),
            from,
            to,
            amps,
        }
    }
    fn build(nodes: usize, elements: Vec<Element>) -> Result<Netlist, String> {
        let mut net = Netlist::new(nodes)?;
        for e in elements {
            net.add(e)?;
        }
        Ok(net)
    }
    fn near(got: f64, want: f64, tol: f64) -> Result<(), String> {
        if (got - want).abs() <= tol {
            Ok(())
        } else {
            Err(format!("got {got}, wanted {want}"))
        }
    }

    #[test]
    fn divider_middle_node_is_five_volts() -> Result<(), String> {
        let net = build(
            3,
            vec![
                v("V1", 1, 0, 10.0),
                r("R1", 1, 2, 1000.0),
                r("R2", 2, 0, 1000.0),
            ],
        )?;
        let s = solve_dc(&net)?;
        near(s.node_volts[1], 10.0, 1e-9)?;
        near(s.node_volts[2], 5.0, 1e-9)
    }

    #[test]
    fn two_sources_in_series_add() -> Result<(), String> {
        let net = build(
            3,
            vec![v("V1", 1, 0, 3.0), v("V2", 2, 1, 4.0), r("R1", 2, 0, 700.0)],
        )?;
        let s = solve_dc(&net)?;
        near(s.node_volts[2], 7.0, 1e-9)
    }

    #[test]
    fn current_source_into_a_resistor_gives_i_times_r() -> Result<(), String> {
        let net = build(2, vec![i("I1", 0, 1, 0.002), r("R1", 1, 0, 1500.0)])?;
        let s = solve_dc(&net)?;
        near(s.node_volts[1], 3.0, 1e-9)
    }

    #[test]
    fn source_current_sign_is_documented_and_tested() -> Result<(), String> {
        let net = build(2, vec![v("V1", 1, 0, 10.0), r("R1", 1, 0, 1000.0)])?;
        let s = solve_dc(&net)?;
        let first = s.source_currents.first().ok_or("no source current")?;
        if first.name != "V1" {
            return Err(format!("wrong source name {}", first.name));
        }
        // A source delivering 10 mA reports minus 10 mA (see `SourceCurrent`).
        near(first.amps, -0.01, 1e-12)
    }

    #[test]
    fn floating_node_is_a_named_error_not_nan() -> Result<(), String> {
        let net = build(3, vec![v("V1", 1, 0, 5.0), r("R1", 1, 0, 100.0)])?;
        let err = solve_dc(&net).err().ok_or("should be refused")?;
        if err.contains("node 2") {
            Ok(())
        } else {
            Err(err)
        }
    }

    #[test]
    fn tiny_conductance_falls_back_to_the_shunt() -> Result<(), String> {
        let net = build(2, vec![i("I1", 0, 1, 1e-12), r("R1", 1, 0, 1e20)])?;
        let s = solve_dc(&net)?;
        near(s.node_volts[1], 1.0, 1e-6)
    }

    #[test]
    fn solving_is_repeatable_to_the_last_bit() -> Result<(), String> {
        let net = build(
            4,
            vec![
                v("V1", 1, 0, 9.0),
                r("R1", 1, 2, 330.0),
                r("R2", 2, 3, 470.0),
                r("R3", 3, 0, 220.0),
                r("R4", 2, 0, 1000.0),
            ],
        )?;
        let (a, b) = (solve_dc(&net)?, solve_dc(&net)?);
        if a == b {
            Ok(())
        } else {
            Err("two solves differ".to_string())
        }
    }
}
