//! Topology checks that run before solving, so the user reads a part name instead of a pivot failure.
//!
//! # Rules
//! - Three findings, each naming elements or nodes: a loop of voltage sources, a cut-set of
//!   current sources, a node with no DC path to ground.
//! - Every walk is iterative and bounded by the netlist size.
//!
//! # Dependencies
//! - docs/adr-0001-simulation.md section 4
//! - crates/sim/src/net.rs
//!
//! # Do not
//! - Do not factor a matrix here; this module only reads the graph.
use crate::net::{Element, Netlist, node_label};
use std::collections::VecDeque;

/// Runs all three checks and returns every finding in one message, or `Ok` when the circuit is sound.
///
/// # Rules
/// - Order: voltage-source loops, current-source cut-sets, missing DC paths.
///
/// # Do not
/// - Do not skip it before `solve_dc`; the solver calls it and trusts the result.
pub fn check(net: &Netlist) -> Result<(), String> {
    let mut findings = voltage_loops(net);
    let (cut_sets, floating) = dc_paths(net);
    findings.extend(cut_sets);
    findings.extend(floating);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(findings.join("; "))
    }
}

/// Finds voltage sources that close a loop with earlier ones, naming the sources on the loop.
fn voltage_loops(net: &Netlist) -> Vec<String> {
    let n = net.nodes();
    let mut findings = Vec::new();
    // Adjacency of the sources accepted so far: node -> (neighbour, source name).
    let mut adjacent: Vec<Vec<(usize, String)>> = vec![Vec::new(); n];
    for element in net.elements() {
        let Element::VoltageSource {
            name, plus, minus, ..
        } = element
        else {
            continue;
        };
        if let Some(path) = path_between(&adjacent, *plus, *minus) {
            let mut names = path;
            names.push(name.clone());
            findings.push(format!(
                "voltage sources {} form a loop, which no circuit can satisfy",
                names.join(" and ")
            ));
        } else {
            adjacent[*plus].push((*minus, name.clone()));
            adjacent[*minus].push((*plus, name.clone()));
        }
    }
    findings
}

/// Breadth-first search for the source names on a path from `from` to `to`.
fn path_between(adjacent: &[Vec<(usize, String)>], from: usize, to: usize) -> Option<Vec<String>> {
    let mut seen = vec![false; adjacent.len()];
    let mut via: Vec<Option<(usize, String)>> = vec![None; adjacent.len()];
    let mut queue = VecDeque::new();
    seen[from] = true;
    queue.push_back(from);
    while let Some(node) = queue.pop_front() {
        if node == to {
            let mut names = Vec::new();
            let mut at = to;
            while let Some((prev, name)) = &via[at] {
                names.push(name.clone());
                at = *prev;
            }
            names.reverse();
            return Some(names);
        }
        for (next, name) in &adjacent[node] {
            if !seen[*next] {
                seen[*next] = true;
                via[*next] = Some((node, name.clone()));
                queue.push_back(*next);
            }
        }
    }
    None
}

/// Splits nodes into groups joined by resistors and voltage sources, then reports each group
/// that cannot reach ground: as a current-source cut-set when current sources touch it, else
/// as a node with no DC path.
fn dc_paths(net: &Netlist) -> (Vec<String>, Vec<String>) {
    let n = net.nodes();
    let mut parent: Vec<usize> = (0..n).collect();
    for element in net.elements() {
        if matches!(element, Element::CurrentSource { .. }) {
            continue;
        }
        let (p, q) = element.nodes();
        let (rp, rq) = (find(&mut parent, p), find(&mut parent, q));
        if rp != rq {
            parent[rp] = rq;
        }
    }
    let ground = find(&mut parent, 0);
    let mut cut_sets = Vec::new();
    let mut floating = Vec::new();
    let mut done = vec![false; n];
    for node in 1..n {
        let root = find(&mut parent, node);
        if root == ground || done[root] {
            continue;
        }
        done[root] = true;
        let members: Vec<usize> = (1..n).filter(|m| find(&mut parent, *m) == root).collect();
        let list = members
            .iter()
            .map(|m| node_label(*m))
            .collect::<Vec<_>>()
            .join(", ");
        let sources: Vec<&str> = net
            .elements()
            .iter()
            .filter(|e| matches!(e, Element::CurrentSource { .. }))
            .filter(|e| {
                let (p, q) = e.nodes();
                members.contains(&p) || members.contains(&q)
            })
            .map(Element::name)
            .collect();
        if sources.is_empty() {
            floating.push(format!("{list} has no DC path to ground"));
        } else {
            cut_sets.push(format!(
                "current sources {} are the only link between {list} and the rest of the circuit",
                sources.join(" and ")
            ));
        }
    }
    (cut_sets, floating)
}

/// Union-find lookup with path halving.
fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str, plus: usize, minus: usize) -> Element {
        Element::VoltageSource {
            name: name.to_string(),
            plus,
            minus,
            volts: 1.0,
        }
    }
    fn r(name: &str, a: usize, b: usize) -> Element {
        Element::Resistor {
            name: name.to_string(),
            a,
            b,
            ohms: 1000.0,
        }
    }
    fn i(name: &str, from: usize, to: usize) -> Element {
        Element::CurrentSource {
            name: name.to_string(),
            from,
            to,
            amps: 0.001,
        }
    }
    fn build(nodes: usize, elements: Vec<Element>) -> Result<Netlist, String> {
        let mut net = Netlist::new(nodes)?;
        for e in elements {
            net.add(e)?;
        }
        Ok(net)
    }

    #[test]
    fn voltage_source_loop_is_refused_before_solving() -> Result<(), String> {
        let net = build(3, vec![v("V1", 1, 0), v("V2", 2, 1), v("V3", 2, 0)])?;
        let err = check(&net).err().unwrap_or_default();
        if err.contains("V1") && err.contains("V2") && err.contains("V3") && err.contains("loop") {
            Ok(())
        } else {
            Err(err)
        }
    }

    #[test]
    fn current_source_cut_set_is_refused_by_name() -> Result<(), String> {
        let net = build(3, vec![r("R1", 1, 0), i("I1", 1, 2), r("R2", 2, 0)])?;
        // Node 2 reaches ground through R2, so this one is sound.
        check(&net)?;
        let net = build(3, vec![r("R1", 1, 0), i("I1", 1, 2), i("I2", 2, 1)])?;
        let err = check(&net).err().unwrap_or_default();
        if err.contains("I1") && err.contains("I2") && err.contains("node 2") {
            Ok(())
        } else {
            Err(err)
        }
    }

    #[test]
    fn node_without_a_dc_path_to_ground_is_reported() -> Result<(), String> {
        let net = build(4, vec![r("R1", 1, 0), r("R2", 2, 3)])?;
        let err = check(&net).err().unwrap_or_default();
        if err.contains("node 2") && err.contains("node 3") && err.contains("no DC path") {
            Ok(())
        } else {
            Err(err)
        }
    }

    #[test]
    fn sound_circuit_passes() -> Result<(), String> {
        let net = build(3, vec![v("V1", 1, 0), r("R1", 1, 2), r("R2", 2, 0)])?;
        check(&net)
    }
}
