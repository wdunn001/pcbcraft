# Code documentation standard

Intent lives next to the code, so a reader never has to guess it from a distant document.
Each rule lives at one layer and nowhere else; lower layers point up to it.
Agents read the header and the doc block first, and `cargo xtask docs check` keeps the paths in them honest.

## The five layers

| Layer | Rust form | Content | Size |
| --- | --- | --- | --- |
| Global | AGENTS.md | Defaults and pointers that hold for the whole repo | 60 lines or fewer |
| File | `//!` at the top of the file | Why the file exists, its local rules, its dependencies | 5 to 12 lines |
| Contract | `///` on a trait or shared type | The rules every implementation obeys, the list of implementations | 5 to 12 lines |
| Function | `///` on a fn | What it does and why, only if not obvious; rules, dependencies, what not to do | 1 to 12 lines |
| Implementation | `///` on `impl Trait for X` | One `Contract:` line plus only what is specific to this implementation | 2 to 5 lines |

Never restate a rule that lives at a higher layer. Point to it.

## Section names

A block uses only the sections it needs.

- `# Rules`: what must stay true.
- `# Dependencies`: the files this code relies on.
- `# Do not`: what a change must never do. Keep every item; append, never shorten.
- `# Implementations`: traits and the command registry only; every file that implements them.

A trivial function is one line. A block is 5 to 12 lines. The checker warns above 15.

## Paths

Paths are relative to the repo root, for example `crates/library/src/sources.rs`. Write one per list item, with an optional `#item` anchor after the path and optional text after a space. A path must exist on main.

## Examples

Global layer, an excerpt of AGENTS.md:

```
- **Pre-edit:** read the file's `//!` header and the doc block of each function you change.
```

File layer, for crates/library/src/sources.rs:

```rust
//! Online part sources behind one trait. A source answers a part search with
//! stock, prices, lifecycle and a datasheet link.
//!
//! # Rules
//! - No network calls and no keys in this file; live sources implement `PartSource` elsewhere.
//! - Every input is bounded (`MAX_QUERY_BYTES`, `MAX_RESULTS`).
//!
//! # Dependencies
//! - crates/library/src/lib.rs
```

Contract layer, on the `PartSource` trait:

```rust
/// One online catalogue that answers part searches.
///
/// # Rules
/// - `search` and `fetch` reject input through `check_query` before any lookup.
/// - A source never writes into the project; it only returns `PartResult` records.
///
/// # Implementations
/// - crates/library/src/sources.rs (`FixtureSource`)
/// - crates/library/tests/sources.rs (`Counting`, test only)
///
/// # Do not
/// - Add a method without a default unless every listed implementation is updated in the same change.
```

Function layer, on `Session::batch` in crates/engine/src/lib.rs:

```rust
/// Runs 1 to 128 commands as one undo step; either all apply or none do.
///
/// # Rules
/// - Rejects a stale `expected` revision with `Error::Stale`.
/// - Validates the whole project after every command, on a clone.
///
/// # Dependencies
/// - crates/doc/src/lib.rs
///
/// # Do not
/// - Mutate `self.project` before the last command succeeds.
```

Implementation layer, on `impl PartSource for FixtureSource`:

```rust
/// Contract: crates/library/src/sources.rs
///
/// Answers from recorded parts only; makes no network call.
```

Registry, on `commands()` in crates/engine/src/lib.rs:

```rust
/// Every command the engine accepts: id, description and JSON input schema.
///
/// # Rules
/// - A command is a string id plus JSON params; inputs are bounded and an invalid batch leaves state untouched.
/// - No handler opens a dialog; the UI, the CLI and MCP all call the same ids.
///
/// # Implementations
/// - crates/engine/src/lib.rs (`run_command`)
/// - crates/engine/src/authoring.rs
/// - crates/engine/src/tuning.rs
///
/// # Do not
/// - Add an id here without a handler in one of the listed files.
```

## What the checker enforces and what review reads

`cargo xtask docs check` (job "Documentation standard" in docs/ci.md) enforces four things:

1. Every `Contract:` path and every item under `# Dependencies` or `# Implementations` exists.
2. Each public trait lists exactly the files that implement it, and each implementation carries a `Contract:` line.
3. AGENTS.md is 60 lines or fewer, has no business-rules heading, and keeps its three `Pre-edit:` lines.
4. Missing-docs warnings per crate do not rise above xtask/doc-ratchet.toml.

Review still reads whether the prose is true, whether the rule belongs at that layer, and whether a `# Do not` section was shortened.

## Contracts in PCBCraft

- `ChatBackend`, crates/assistant/src/backend.rs. Implementations: `ApiBackend` in crates/assistant/src/api.rs, `ScriptBackend` in crates/assistant/src/script.rs.
- `Secrets`, crates/assistant/src/backend.rs. Implementations: none yet.
- `PartSource`, crates/library/src/sources.rs. Implementations: `FixtureSource` (same file), `Counting` (crates/library/tests/sources.rs, test only).
- Engine command registry, crates/engine/src/lib.rs: `Command`, `CommandSpec` and `commands()`, which chains the specs from crates/engine/src/authoring.rs and crates/engine/src/tuning.rs. Handlers run through `run_command` in crates/engine/src/lib.rs and the `run` function in each of those two modules. Consumers: crates/mcp/src/lib.rs, crates/ui-egui/src/lib.rs, apps/pcbcraft-cli/src/main.rs.

A `CloudApp` trait does not exist yet. If one lands it follows the same shape.

## Agent pre-edit checks

AGENTS.md carries three checks, each one sentence:

1. Read the file's `//!` header and the doc block of each function you change.
2. Confirm every path under `# Dependencies` and every `Contract:` line exists; if one is missing, stop and report it.
3. Before editing a trait or the command registry, open every file under its `# Implementations`; if the list is missing or stale, stop and report.

Background: [Documentation: beating your AI stack](https://themildtake.com/articles/2026-05-05-documentation-beating-your-ai-stack/).
