# ADR 0004: Firmware execution route

Status: Proposed. The owner confirms or changes it in the simulation epic register. Epic: #1363. Roadmap id: SIM-6-01. Depends on: SIM-5-01 (#1464).

## 1. Decision

PCBCraft runs firmware with a built-in RV32IMC interpreter for the first year, starting with the ESP32-C3 and C6 class parts.
The Xtensa cores use an external emulator over WebSocket.
A wasm emulator is available only if licence and size allow.
Hardware execution is always available.

The first deliverable is an RV32IMC interpreter crate that loads a C3 class ELF and blinks one GPIO visible on the scope, tracked as SIM-6-12.

## 2. The four routes

The browser cannot spawn processes, so the choice is between code that ships inside PCBCraft and code reached over a socket or a worker.

### Route A: Built-in core in portable Rust

A Rust interpreter for a subset of a core, written from scratch and shipped inside PCBCraft. Starting with RV32IMC for the C3 and C6 class parts.

RV32I has 40 base instructions. The M extension adds integer multiplication and division. The C extension adds compressed instructions. Together they form a small, well-defined instruction set that is feasible to implement in portable Rust.

Pros: deterministic, offline, no external process, no protocol overhead, full control over timing model.
Cons: must be written from scratch, must maintain correctness, ROM images not included.

### Route B: External emulator over WebSocket

An existing emulator runs on the user machine and PCBCraft communicates with it over a WebSocket using the co-simulation protocol defined in phase 8. The user starts the emulator process themselves.

Pros: reuses a mature emulator, only route to the Xtensa LX6 and LX7 cores in the first year, no need to implement those instruction sets.
Cons: requires a user-side process, requires a protocol, not offline if the user does not have the emulator installed, adds setup complexity.

### Route C: Third-party emulator compiled to wasm

A third-party emulator compiled to WebAssembly and run in a Web Worker. This keeps the experience wasm-first and avoids the external process requirement of route B.

Pros: no user-side process required, keeps the browser experience unified.
Cons: large download size, licence risk if the emulator is not compatible with MIT OR Apache-2.0, Web Worker adds complexity, still requires ROM images.

### Route D: Hardware only

Run firmware on the actual silicon. No emulation involved.

Pros: runs on real hardware, no emulation bugs, accurate timing.
Cons: requires the user to have the device and a programmer, not offline for users without hardware, not useful for simulation during design.

## 3. Comparison

| Route | Size | Determinism | Offline use | Licence risk | First deliverable |
| --- | --- | --- | --- | --- | --- |
