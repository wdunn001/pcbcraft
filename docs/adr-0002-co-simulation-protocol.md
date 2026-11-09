# ADR 0002: Co-simulation protocol shape, transport and versioning

Status: Proposed. Part of #1365 (roadmap id SIM-8-01). Depends on SIM-5-01 (#1464).

## 1. Decision

PCBCraft exposes a co-simulation protocol over WebSocket to a local address. Text frames carry JSON objects with a type, version and sequence number. Binary frames are used only for image chunks. The protocol uses semantic versioning in the handshake: a major version bump breaks compatibility, a minor version bump adds optional fields. The IDE or build tool side is a separate small program that PCBCraft never ships or starts. No code editing happens in PCBCraft.

## 2. Transport options

Three options were considered for the transport:

- **WebSocket to a local address.** Chosen. Works on all platforms, supports bidirectional communication, and is widely available in IDEs and build tools. The local address keeps the protocol private to the machine.
- **Named pipe (Unix domain socket / Windows named pipe).** Rejected. More complex to implement and test across platforms, with less tooling support in IDEs.
- **Standard input/output stream.** Rejected. Limits bidirectional communication and complicates session management.

## 3. Frame format options

Three options were considered for the wire format:

- **JSON text frames.** Chosen. Human readable for debugging, widely supported, and sufficient for the control messages of the co-simulation protocol. JSON objects carry a `type`, `version` and `seq` field for identification and ordering.
- **Binary frames (protobuf or flat buffers).** Rejected. Adds a dependency and complexity without measurable benefit for the small control messages. Binary frames are still used for large payload chunks (simulation result images) where JSON would be inefficient.
- **WebSocket data channels with structured messages.** Rejected. Adds protocol layering without solving a stated problem.

## 4. Versioning

The handshake includes a version string following semantic versioning. A major version change (e.g. 1.0 to 2.0) may break compatibility and requires both sides to update. A minor version change (e.g. 1.0 to 1.1) adds optional fields and does not break existing implementations. The sequence number in each message allows reordering and duplicate detection.

## 5. Scope

This protocol carries simulation data only. It does not carry code editing commands. The coding agent communicates separately over the Agent Client Protocol (issue #200). PCBCraft acts as the simulation backend and receives design data, runs the simulation, and returns results. The client program (IDE plugin or build tool) manages the user interaction and code editing.

## 6. Plan

1. This page (#1499).
2. Protocol specification and message schemas (filed after step 1 merges).
3. Implementation in crates/sim as a WebSocket server.

Sources: SIM-5-01 research (#1464), WebSocket specification, semantic versioning specification.
