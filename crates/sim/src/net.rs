//! The plain netlist the solver works on: numbered nodes and three element kinds.
//!
//! # Rules
//! - Node 0 is ground. Nodes are dense indices; `Netlist::new(nodes)` counts ground.
//! - Sizes are bounded by `MAX_NODES` and `MAX_ELEMENTS`; the error states the limit and the count seen.
//! - Element names are unique and non-empty so every message can name the part.
//!
//! # Dependencies
//! - docs/adr-0001-simulation.md section 2
//!
//! # Do not
//! - Do not store a hash map here; solving walks the elements in insertion order for determinism.

/// Most nodes a netlist may have, ground included.
pub const MAX_NODES: usize = 512;
/// Most elements a netlist may hold.
pub const MAX_ELEMENTS: usize = 2048;
/// Longest element name, in bytes.
const MAX_NAME: usize = 64;

/// One circuit element. Values are in base units (ohms, volts, amps).
#[derive(Clone, Debug, PartialEq)]
pub enum Element {
    /// A resistor of `ohms` between nodes `a` and `b`. Must be above zero.
    Resistor {
        /// Unique name used in messages.
        name: String,
        /// First node.
        a: usize,
        /// Second node.
        b: usize,
        /// Resistance in ohms.
        ohms: f64,
    },
    /// A voltage source holding `plus` at `volts` above `minus`.
    VoltageSource {
        /// Unique name used in messages.
        name: String,
        /// Positive terminal node.
        plus: usize,
        /// Negative terminal node.
        minus: usize,
        /// Volts from minus to plus.
        volts: f64,
    },
    /// A current source pushing `amps` out of node `from`, through the source, into node `to`.
    CurrentSource {
        /// Unique name used in messages.
        name: String,
        /// Node the current leaves.
        from: usize,
        /// Node the current enters.
        to: usize,
        /// Amps through the source.
        amps: f64,
    },
}

impl Element {
    /// The element's name.
    pub fn name(&self) -> &str {
        match self {
            Element::Resistor { name, .. }
            | Element::VoltageSource { name, .. }
            | Element::CurrentSource { name, .. } => name,
        }
    }

    /// The two nodes the element touches.
    pub fn nodes(&self) -> (usize, usize) {
        match self {
            Element::Resistor { a, b, .. } => (*a, *b),
            Element::VoltageSource { plus, minus, .. } => (*plus, *minus),
            Element::CurrentSource { from, to, .. } => (*from, *to),
        }
    }
}

/// A circuit: a node count and the elements joining those nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct Netlist {
    nodes: usize,
    elements: Vec<Element>,
}

impl Netlist {
    /// Makes an empty netlist with `nodes` nodes, ground (node 0) included.
    ///
    /// # Rules
    /// - Refuses zero nodes and more than `MAX_NODES`.
    pub fn new(nodes: usize) -> Result<Self, String> {
        if nodes == 0 || nodes > MAX_NODES {
            return Err(format!(
                "a netlist needs between 1 and {MAX_NODES} nodes, ground included; {nodes} were asked for"
            ));
        }
        Ok(Self {
            nodes,
            elements: Vec::new(),
        })
    }

    /// Number of nodes, ground included.
    pub fn nodes(&self) -> usize {
        self.nodes
    }

    /// The elements in the order they were added.
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    /// Adds an element after checking it.
    ///
    /// # Rules
    /// - Node references must be below the node count; values must be finite.
    /// - A resistor must be above zero; both ends of any element must differ.
    /// - The name must be new, non-empty and at most 64 bytes.
    /// - The netlist never grows past `MAX_ELEMENTS`.
    pub fn add(&mut self, element: Element) -> Result<(), String> {
        if self.elements.len() >= MAX_ELEMENTS {
            return Err(format!(
                "a netlist holds at most {MAX_ELEMENTS} elements; it already has {}",
                self.elements.len()
            ));
        }
        let name = element.name();
        if name.is_empty() || name.len() > MAX_NAME {
            return Err(format!(
                "an element name must be 1 to {MAX_NAME} bytes long; \"{name}\" is {} bytes",
                name.len()
            ));
        }
        if self.elements.iter().any(|e| e.name() == name) {
            return Err(format!("there are two elements named {name}"));
        }
        let (p, q) = element.nodes();
        for node in [p, q] {
            if node >= self.nodes {
                return Err(format!(
                    "{name} uses node {node} but the netlist has only {} nodes",
                    self.nodes
                ));
            }
        }
        if p == q {
            return Err(format!("{name} has both ends on node {p}"));
        }
        let (value, what) = match &element {
            Element::Resistor { ohms, .. } => (*ohms, "resistance"),
            Element::VoltageSource { volts, .. } => (*volts, "voltage"),
            Element::CurrentSource { amps, .. } => (*amps, "current"),
        };
        if !value.is_finite() {
            return Err(format!("{name} has a {what} that is not a finite number"));
        }
        if matches!(element, Element::Resistor { .. }) && value <= 0.0 {
            return Err(format!("{name} must have a resistance above zero"));
        }
        self.elements.push(element);
        Ok(())
    }
}

/// Writes a node as text for messages: ground or its number.
pub(crate) fn node_label(node: usize) -> String {
    if node == 0 {
        "ground".to_string()
    } else {
        format!("node {node}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resistor(name: &str, a: usize, b: usize, ohms: f64) -> Element {
        Element::Resistor {
            name: name.to_string(),
            a,
            b,
            ohms,
        }
    }

    #[test]
    fn oversized_netlist_is_refused_with_the_limit_in_the_message() -> Result<(), String> {
        let big = Netlist::new(MAX_NODES + 1).err().unwrap_or_default();
        if !big.contains("512") || !big.contains("513") {
            return Err(format!("node limit message: {big}"));
        }
        let mut net = Netlist::new(3)?;
        for i in 0..MAX_ELEMENTS {
            net.add(resistor(&format!("R{i}"), 1, 2, 1.0))?;
        }
        let over = net.add(resistor("Rx", 1, 2, 1.0)).err().unwrap_or_default();
        if over.contains("2048") {
            Ok(())
        } else {
            Err(format!("element limit message: {over}"))
        }
    }

    #[test]
    fn references_past_the_end_are_refused() -> Result<(), String> {
        let mut net = Netlist::new(2)?;
        let err = net.add(resistor("R1", 0, 2, 1.0)).err().unwrap_or_default();
        if err.contains("R1") && err.contains("node 2") {
            Ok(())
        } else {
            Err(err)
        }
    }

    #[test]
    fn bad_values_and_names_are_refused() -> Result<(), String> {
        let mut net = Netlist::new(3)?;
        net.add(resistor("R1", 0, 1, 1.0))?;
        for bad in [
            resistor("R1", 1, 2, 1.0),
            resistor("", 1, 2, 1.0),
            resistor("R2", 1, 1, 1.0),
            resistor("R3", 1, 2, 0.0),
            resistor("R4", 1, 2, f64::NAN),
        ] {
            if net.add(bad.clone()).is_ok() {
                return Err(format!("should be refused: {bad:?}"));
            }
        }
        Ok(())
    }
}
