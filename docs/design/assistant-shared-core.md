# Assistant shared core: where it should live

Status: Proposed (2026-10-30). Parent: #185. This note records facts, three options, a recommendation, and an owner decision. No code moves in #201.

## 1. Problem

SolidCraft and PCBCraft both ship an assistant: chat transcript, permission gate, HTTP and API backends, and tool calls into each app's engine. Much of that logic is the same shape but lives in two trees today. Sharing it is the end state, but craft-libs is MIT OR Apache-2.0 while PCBCraft is GPL-3.0-or-later, so only code the author relicenses can move upward. SolidCraft's UI pane stays bound to `SolveApp`, `Prefs`, and its control channel; PCBCraft's pane stays in `crates/ui-egui` with PCBCraft settings and MCP tools.

## 2. Verified facts (2026-10-28, PCBCraft tree rechecked 2026-10-30)

### craft-libs upstream (`storytold/craft-libs`, main at 1a11a48)

- `crates/ai-core` (`craft-ai-core`): shared data model for AI connections, roles, requests and jobs. Described in its `lib.rs` as an interim contract.
- Surface includes `Provider { models, estimate, submit, poll, cancel }`, `JobState`, `JobSnapshot`, `Request`, `Connection`, `Auth { None, ApiKey, Subscription }`, `Endpoint::is_loopback`, `SecretString`, `redact`, `ConnectionDraft` with `PRESETS`, `HistoryRecord`, `ConsentLedger`, `StoreFile` (launcher shared store), `picks::choose`.
- No chat turn, no tool call, no permission gate. Compiles for wasm; no I/O in the crate.
- Same revision also has `i18n`, `settings` (`craft_settings::SettingsStore`, TOML with migrations), `keymap`, `menu`, `update`.

### Pins today

| App | craft-libs remote | Revision | Crates used from craft-libs |
| --- | --- | --- | --- |
| PCBCraft | `storytold/craft-libs` | `1aa81a7` (2026-10-11) | `print`, `ui` only |
| SolidCraft | `wdunn001/craft-libs` | `316a7ad` | `craft-ui`, `craft-cloud`; no AI crate on that fork |

### SolidCraft assistant (MIT OR Apache-2.0)

- Path: `crates/ui-egui/src/agent_chat/`, about 16 files and 11,323 lines (issue inventory).
- Core-ish modules with little or no egui: `backend.rs`, `gate.rs`, `api_client.rs`, `acp.rs`, `acp_client.rs`, `process.rs`, `discover.rs`, `markdown.rs`.
- UI-bound modules: `mod.rs`, `panel.rs`, `settings.rs`, `attach.rs` (tied to `SolveApp` and `Prefs`).

### PCBCraft assistant (GPL-3.0-or-later, vendored)

- Path: `crates/assistant` (`pcbcraft-assistant`), about 4,500 lines of Rust across 17 source files (2026-10-30).
- Public seam: `ChatBackend`, `Event`, `PromptPart`, `ApiBackend`, transcript `Item`, turn accounting, `gate::{Mode, Verdict}`, connections and presets, plain HTTP helpers, MCP-oriented tool naming.
- Adapted from SolidCraft MIT OR Apache-2.0 sources (see file headers in `backend.rs`, `gate.rs`, and related modules). No ACP client in this crate yet; wire-format work lives in `docs/assistant-backends.md` and tests in `api_tests.rs` and `tests/openai_wire.rs`.
- Depends on `pcbcraft-engine` and `pcbcraft-mcp` for gate schema and tool lists (app-specific).

### Licences (direction of travel)

- SolidCraft to PCBCraft: permitted with notices (MIT OR Apache-2.0 into GPL-3.0-or-later).
- PCBCraft to craft-libs: the author must license moved code MIT OR Apache-2.0 at the move. Owner-authored and agent-authored work under the owner's account can be relicensed; third-party GPL adaptations cannot move up.

## 3. Option A: Keep `crates/assistant` in PCBCraft only

SolidCraft keeps `agent_chat` as today.

| Topic | Cost |
| --- | --- |
| Files that move | None |
| craft-libs pin | PCBCraft stays on `1aa81a7` for `print` and `ui` only |
| SolidCraft pin | Unchanged (`316a7ad`, no AI crate) |
| Licence step | None |
| Tests | Two suites drift: SolidCraft's ~90 agent_chat tests; PCBCraft's ~63 unit tests plus 7 loopback wire tests in `crates/assistant` |
| Maintenance | Two owners of the same bug fixes (gate rules, protocol quirks, security redaction) |
| SolidCraft change | None |
| PCBCraft change | None |

**Pros:** Zero coordination, no relicensing paperwork, no craft-libs release cycle.

**Cons:** Does not meet the sharing goal; fixes and protocol updates land twice; connection and secret handling may diverge from `craft-ai-core` anyway.

## 4. Option B: New `craft-assistant` crate in craft-libs

A dedicated crate for conversational assistants: backend trait and events, transcript and turns, generic gate (`Verdict` plus a `ToolClass` trait each app implements for its tool names), HTTP and API backends with injected tool lists and call functions, ACP client when SolidCraft's modules are included, markdown reader for assistant output. Apps keep panes, settings, and engine hooks.

| Topic | Cost |
| --- | --- |
| Files that move from PCBCraft | Roughly `backend.rs`, `transcript.rs`, `turn.rs`, `api.rs`, `http.rs`, `text.rs`, `prompt.rs`, `script.rs`, `store.rs`, `describe.rs`, `guard.rs`, and a generic `gate.rs` (tool list via trait). Leave `tool_names.rs`, `context.rs`, `connections.rs`, and MCP-specific glue in PCBCraft or a thin `pcbcraft-assistant` shim |
| Files that move from SolidCraft | `backend.rs`, `api_client.rs`, `gate.rs` (genericized), plus `acp.rs`, `acp_client.rs`, `process.rs`, `discover.rs`, `markdown.rs` as they are stripped of `SolveApp` |
| craft-libs pin (PCBCraft) | Bump to a revision that publishes `craft-assistant`; add dependency; keep or add `craft-ai-core` only if connection types are shared instead of duplicated |
| craft-libs pin (SolidCraft) | Move fork or track storytold main after `craft-assistant` exists; depend on `craft-assistant` and optionally `craft-ai-core` |
| Licence step | Owner relicenses contributed PCBCraft and SolidCraft core modules MIT OR Apache-2.0 in craft-libs; retain attribution in PCBCraft `ATTRIBUTION.md` for SolidCraft origin |
| Tests | Port SolidCraft agent_chat tests that target backend, gate, and API client; port PCBCraft `api_tests.rs`, gate tests, and `openai_wire.rs` into craft-libs CI; apps keep pane e2e tests only |
| Maintenance | One crate, semver in craft-libs; breaking gate or backend changes need both apps in the same release train |

**Pros:** Clear boundary between chat assistant and job-oriented `craft-ai-core`; both apps share one implementation; wasm-friendly core can stay I/O at the edges via injected HTTP.

**Cons:** Up-front extraction work; craft-libs release and dual pin bumps; trait design for gate and tool calls must fit both MCP (PCBCraft) and SolidCraft tool surfaces.

## 5. Option C: Extend `craft-ai-core` with a `chat` module

Add turns, tool calls, and gate beside the existing job model in one crate, reusing `SecretString`, `Connection`, `Endpoint`, `ConnectionDraft`.

| Topic | Cost |
| --- | --- |
| Files that move | Same core modules as option B, but landed inside `crates/ai-core` |
| craft-libs pin | Both apps bump to the ai-core revision that adds `chat` |
| Licence step | Same relicensing as option B |
| Tests | Same test port as option B, colocated with ai-core |
| Maintenance | Single crate, but the crate documents itself as interim for jobs; a future replacement of the job contract could force a second migration for chat |

**Pros:** One dependency for "all AI"; connection and auth types stay unified without a second crate.

**Cons:** Mixes two product shapes (batch or job providers vs interactive chat and tools); higher risk if `craft-ai-core` is replaced; larger API surface for apps that only need chat or only need jobs.

## 6. Recommendation

**Option B: new `craft-assistant` in craft-libs.**

Reasoning:

1. The sharing goal is the chat backend, gate, and wire protocols, not the job provider API. Keeping chat out of `craft-ai-core` avoids coupling to an interim job contract.
2. `craft-assistant` can depend on `craft-ai-core` for `Connection`, `SecretString`, and presets without owning job lifecycle.
3. PCBCraft keeps a thin GPL layer for MCP tool names, engine gate mapping, and UI; SolidCraft keeps `SolveApp`-bound panes. That matches how much code is already split in SolidCraft's tree.
4. Option A fails the parent epic (#185). Option C saves one crate name but trades a clean boundary for migration risk when the job model changes.

Suggested first port slice (child work under #185): extract types and tests that already match between PCBCraft `crates/assistant` and SolidCraft `backend.rs` / `api_client.rs`, with gate parameterized by a `ToolClass` trait implemented once per app.

## 7. Decision

| Field | Value |
| --- | --- |
| Date | Pending |
| Owner pick | Pending comment on #201 |
| Chosen option | Not recorded yet |

When the owner comments the pick, update this table and set Status to Accepted with the date.

## 8. References

- #201 (this decision), #185 (parent handoff)
- `docs/assistant-backends.md` (PCBCraft wire conformance)
- `crates/assistant/src/lib.rs` (current PCBCraft seam)
- craft-libs `crates/ai-core` at storytold main 1a11a48 (job and connection model)
