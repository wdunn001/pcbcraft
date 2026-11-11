# ADR-0003: Co-simulation Interface Shape

- Ticket: #562 (part of #467, roadmap id SIM-5-01)
- Status: proposed
- Date: 2026-11-11
- Owner: @wdunn001

## Decision

Define a single, transport-agnostic Rust trait that serves as the contract between the firmware model and the analog solver. This trait is the seam that all three execution routes (built-in core, external emulator, real hardware) plug into.

The interface uses integer picoseconds (`u64`) for all time values. The analog solver advances in fixed quanta; the firmware side runs until the next quantum boundary or until it produces an event, and returns timestamped events.

## Context

ADR-0001 established that PCBCraft will use a co-simulation architecture. ADR-0002 defined the transport protocol between the UI and the simulation engine. This ADR defines the in-process interface shape that the simulation engine uses to talk to the firmware model, regardless of which execution route is active.

The interface must work for:
- The built-in Rust firmware model (Phase 6)
- An external emulator via stdio (Phase 7)
- Real hardware via serial (Phase 8)

All three present the same logical interface: a set of digital pins that can be driven and sampled, peripheral transactions, interrupts, and log output.

## Interface

```rust
/// Time unit for all co-simulation timestamps.
/// Integer picoseconds; 1 second = 1_000_000_000_000.
pub type SimTime = u64;

/// Default sync quantum: 1 microsecond for logic-level work.
/// Can be lowered to 10 ns for high-speed signal analysis.
pub const DEFAULT_QUANTUM_NS: u64 = 1_000;

/// A single event produced by the firmware model.
#[derive(Debug, Clone)]
pub enum CosimEvent {
    /// A digital pin was driven to a new level.
    PinDrive { pin: u8, level: bool, time: SimTime },
    /// A digital pin value was sampled (for input).
    PinSample { pin: u8, level: bool, time: SimTime },
    /// A peripheral transaction (I2C, SPI, UART byte, etc.).
    Peripheral { id: u8, kind: PeripheralKind, data: Vec<u8>, time: SimTime },
    /// An interrupt was raised.
    Interrupt { line: u8, time: SimTime },
    /// A log line from the firmware model.
    Log { level: u8, message: String, time: SimTime },
}

/// A command issued by the analog solver to the firmware model.
#[derive(Debug, Clone)]
pub enum CosimCommand {
    /// Advance the firmware model to this time, producing any events.
    Step { target_time: SimTime },
    /// Reset the firmware model to its initial state.
    Reset,
}

/// The co-simulation interface. Implemented by each execution route.
pub trait CosimModel {
    /// Execute firmware until the next event or the target time.
    /// Returns events produced, ordered by (time, source_id, sequence).
    fn step(&mut self, target_time: SimTime) -> Result<Vec<CosimEvent>, CosimError>;

    /// Reset the model to its initial state.
    fn reset(&mut self) -> Result<(), CosimError>;

    /// Set the value of an input pin (from the analog side).
    fn set_pin(&mut self, pin: u8, level: bool) -> Result<(), CosimError>;

    /// Query the value of an output pin (for the analog side).
    fn get_pin(&self, pin: u8) -> Result<bool, CosimError>;
}
```

## Time and Synchronization

- All timestamps are integer picoseconds (`u64`). This is the same unit used throughout the simulation stack.
- The analog solver advances in fixed quanta (default 1 us, configurable down to 10 ns).
- The firmware model runs until it produces an event or reaches the target time.
- Events are ordered by `(time, source_id, sequence)` to ensure deterministic replay.
- The `source_id` distinguishes multiple concurrent firmware instances or peripherals.
- The `sequence` number breaks ties for events with identical time and source.

## Determinism

Determinism is achieved through:
1. Fixed-precision integer time (no floating point).
2. Explicit event ordering by `(time, source_id, sequence)`.
3. The firmware model is pure: given the same input events, it produces the same output events.
4. The analog solver is pure: given the same input events, it produces the same output events.

The co-simulation loop is:
1. Analog solver advances one quantum, producing events.
2. Firmware model processes those events, producing its own events.
3. Repeat.

## Alternatives Considered

- **Floating-point nanoseconds:** Rejected due to precision issues and non-determinism across platforms.
- **Per-event blocking calls:** Rejected due to overhead; batch events per quantum instead.
- **Separate interfaces per execution route:** Rejected; the whole point is a single seam.

## Consequences

- All three execution routes must implement `CosimModel`.
- The analog solver must handle the 1 us / 10 ns quantum configuration.
- Event ordering must be enforced by the co-simulation loop, not by individual models.
- This interface is stable across Phases 6, 7, and 8.

## References

- ADR-0001: Simulation Approach
- ADR-0002: Co-simulation Transport Protocol
- ADR-0004: Firmware Execution Routes
