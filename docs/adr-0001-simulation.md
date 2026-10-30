# ADR 0001: Circuit simulation

Status: Proposed. The owner confirms or changes it in the #352 register (line "#292: simulation approach"). Epic: #292.

## 1. Decision

PCBCraft gets a native solver for passive circuits first. A SPICE netlist file is the escape hatch for anything the solver does not cover. The app writes that file and never runs another program. This keeps the Rust-only rule and leaves transient work to whichever engine the user already trusts.

## 2. Method

The solver uses modified nodal analysis. Each resistor adds conductance to the matrix G. Each voltage source adds one row and one column. The system `A x = z` is solved directly. Once voltage sources are stamped, A is no longer symmetric positive definite, so Cholesky solvers do not apply.

First version: a dense LU with partial pivoting written by hand, about 60 lines, no new dependency. Boards of a few hundred nets fit well inside the bounds (512 nodes, 2048 elements). The upgrade path is `faer` 0.24 (MIT, dense and sparse LU). Rejected, checked 2026-10-29:
- `nalgebra-sparse` 0.12 (Apache-2.0): sparse Cholesky only, wrong for sources.
- `sprs` 0.11 (MIT or Apache-2.0): no general sparse LU. `sprs-ldl` 0.10 (LGPL-2.1): symmetric systems only.
- `russell_sparse` 5.0 (MIT): wraps external C or Fortran solvers, which breaks the Rust-only rule.
- `rsparse` 1.2 (MIT): pure Rust sparse LU, a second candidate if `faer` ever falls short.

## 3. Values

Component values are free text today, so one parser in `crates/sim` reads them. It accepts `4k7`, `1R0`, `330`, `330 Ω`, `100nF`, `2.2u`, `1meg`, a lowercase `1m` as milli, and the micro sign. A bare `1M` is refused, and the message asks for `meg` or `m`, because SPICE reads `M` as milli and silently turns a megaohm into a milliohm. Every refusal echoes the input.

The SPICE writer never copies the stored text. Per the SPICE3 manual (circuit description chapter), scale factors ignore case and trailing letters, so `1F` becomes a femtofarad and `4k7` is not valid at all. The writer prints parsed numbers in plain exponent form (`1e-07`) and adds one comment line per node that maps the node name back to the net name.

## 4. Checks before solving

The same chapter gives three rules. Each makes the DC matrix singular, so the graph is checked first and the message names the parts:
- no loop made only of voltage sources and inductors;
- no cut-set made only of current sources and capacitors;
- every node has a DC path to ground.

If the plain matrix still has a pivot below 1e-18, the solver retries once with a 1e-12 S shunt to ground on every node. It is not added otherwise, so exact answers stay exact. Nodes without a DC path are reported as a plain finding instead of a voltage. The netlist writer runs the same checks.

## 5. Diodes

Diodes come after the passive solver. The method is Newton iteration on the Shockley equation, bounded at about 100 iterations. If it fails, gmin stepping: start with a large shunt on every node and remove it in steps. If that fails, source stepping: ramp all sources up from zero, each step starting from the last solution. Each step limits the junction voltage change so `exp` cannot overflow. The error names the stage that failed. There is no standard LED parameter set, so the documented default is a generic diode with an emission coefficient near 2. The convergence aids come from secondary sources (the Electronic Design article on SPICE internals), so treat the numbers as starting points.

## 6. Acceptance fixture

The proof is a resistor divider: 10 V, 1k over 1k, middle node 5 V within 1e-9. The sample design cannot be solved as it stands: J1 is a connector with no source and the load is an LED. A component marks its role with fields: `sim.model` (one of R, C, L, V, I, D, ignore), `sim.value` (overrides `value`) and `sim.ground` (picks the ground net, otherwise a net named `GND` or `0`). A connector pin pair becomes a source by setting `sim.model` to V on it.

## 7. Out of version one

- Transient. It needs companion models for C and L at every step, the DC operating point as the start, time steps controlled by truncation error, and breakpoints at source edges. That is a second project. Until a user story needs it, transient is SPICE hand-off only.
- AC analysis, and nonlinear parts beyond diodes.

## 8. Licence notes

Checked 2026-10-29. The broadest active Rust simulator is source-available under a personal-use licence that forbids commercial use, redistribution and derivative works. It is incompatible with MIT OR Apache-2.0: do not use it and do not read it for adaptation. A small Unlicense project (operating point and transient for R, C, D, E, G, I, V) may be adapted freely. A GPL-3.0 learning project may be read for ideas only: its code cannot be adapted into a MIT OR Apache-2.0 project. A third project is Apache-2.0 on its repository page and MIT or Apache-2.0 in its manifest, so its terms are unclear; do not adapt it. Anything adapted is recorded in ATTRIBUTION.md with source, revision and licence. Writing a netlist for another engine to read has no licence interaction.

## 9. Plan

1. This page (#754).
2. The `crates/sim` crate: value parser, topology checks, DC solver for R, V and I (#755).
3. Filed after step 2 merges, so their anchors are real: the engine command `sim.dc` (read-only, no revision bump), `pcbcraft_io::spice_netlist` beside `hyperlynx_csv` with a `spice` output kind, diodes with the fallbacks in section 5, and a results section in the Checks tab with the story `a_newcomer_reads_the_voltage_on_each_net`.

Sources: SPICE3 manual, circuit description chapter; Electronic Design, "Taking a peek under the hood of your SPICE circuit simulation engine"; crates.io licence fields for the solver crates; research-292-simulation.md.
