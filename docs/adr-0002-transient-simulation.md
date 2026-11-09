# ADR 0002: Transient and AC simulation scope

Status: Proposed. Supersedes section 7 of ADR 0001 for transient and AC analysis. Epic: #292.

## Decision

Transient and AC analysis enter the version one scope of the native solver. The SPICE netlist file remains the escape hatch for anything the solver does not cover. The app writes that file and never runs another program.

## Method

Transient analysis uses the Backward Euler integration rule by default. It is stable for stiff circuits and requires one solve per time step. The trapezoidal rule is offered as an option for higher accuracy when ringing is acceptable.

For a linear circuit with fixed step size, the conductance matrix A is assembled once, factored once, and reused for every time step. Only the right-hand side vector z changes per step. The cost is one factorization (about (2/3) n^3 operations) plus one solve per step (about 2 n^2 operations).

The time step h must not exceed tr/10 of the fastest source edge. If a source has no specified rise time, the user supplies one.

## Bounds

One transient run may use at most MAX_STEPS time steps. The proposed value is 200000. The owner confirms or changes this value.

One run may store at most MAX_SAMPLES waveform samples across all nets. The proposed value is 4194304. The owner confirms or changes this value.

If the user requests more steps or samples than the bound, the solver returns an error naming the bound that was exceeded.

## Factor reuse

A linear circuit with constant step size factors the matrix A once. The factor is reused for every step, so the per-step cost is n^2 not n^3.

When the circuit is nonlinear, each Newton iteration requires a new factor. The factor is not reused across Newton iterations.

When the step size changes, a new factor is required.

## When the sparse upgrade is needed

The dense LU solver costs about (2/3) n^3 operations for a factorization. At 512 nodes, one factorization is about 9e7 operations. At 1024 nodes, it is about 7e8 operations.

The rule for upgrading to a sparse solver is based on operation count, not wall time. When the user requests a transient run with more than 128 nodes and more than 10000 steps, the solver estimates the total operation count as (2/3) n^3 + steps * 2 n^2. If that count exceeds 1e9 operations, the solver returns an error recommending the sparse solver.

The sparse solver dependency is not added until a user story requires more than 128 nodes. The candidate is faer 0.24 (MIT, dense and sparse LU), as named in ADR 0001.

## AC analysis

AC analysis uses the same MNA matrix assembly as DC. The conductances are replaced by impedances at the requested frequency. One solve is performed per frequency point. The number of frequency points is bounded by MAX_STEPS.

## What stays out

- Nonlinear transient analysis with automatic step size selection.
- Event-driven simulation.
- Monte Carlo analysis.
- Noise analysis.

## Sources

- ADR 0001, section 7 (transient and AC scope).
- SPICE3 manual, circuit description chapter.
- Electronic Design, "Taking a peek under the hood of your SPICE circuit simulation engine".
