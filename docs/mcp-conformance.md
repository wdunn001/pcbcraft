# MCP conformance

How the PCBCraft MCP server (`crates/mcp`) measures against the Model Context Protocol
specification, revision 2025-11-25. Spec pages read: lifecycle, transports, authorization,
tools, resources, prompts, pagination, logging, cancellation, ping, changelog and schema
reference, all retrieved 2026-10-29. Usage is in [mcp.md](mcp.md).

Status values: supported, partial, absent, not needed. "Needed" says whether the spec
requires the feature of a server that offers tools. Functions are in
`crates/mcp/src/lib.rs` unless a file is named.

## Result table

| Feature (spec page) | Status | Where | Needed | Issue |
| --- | --- | --- | --- | --- |
| initialize and version negotiation (lifecycle) | supported | `handle_rpc` answers 2025-11-25, 2025-06-18 or 2024-11-05 as asked, else 2025-11-25; checks protocolVersion, capabilities and clientInfo | yes | |
| capabilities declaration | supported | `handle_rpc`: `tools`, `resources` and `prompts`, all with `listChanged` false and `subscribe` false | yes | |
| serverInfo | partial | name and version only; `title`, `description`, `icons`, `websiteUrl` are optional | no | |
| notifications/initialized | supported | a message with no id gets no reply | yes | |
| ordering before initialize | supported | `serve` answers -32002 to everything but `initialize` and `ping` until initialize succeeds | no | #637 |
| ping | supported | `handle_rpc` returns `{}` | yes | |
| cancellation (notifications/cancelled) | absent | no reply; in-flight tools are not stopped (see [mcp.md](mcp.md) "Cancellation") | no | #479 |
| tools/list | supported | `tool_list`, one page | yes | |
| pagination (cursor, nextCursor) | not needed | `tools/list`, `resources/list` and `prompts/list` return every item in one response | no | |
| tool `title` | supported | `title_for`, applied in `tool_list` | no | #636 |
| tool `annotations` | supported | `annotations_for`, applied in `tool_list`; table in [mcp.md](mcp.md) "Tool hints" | no | #636 |
| tool `outputSchema` | absent | results carry `structuredContent` and a text copy, with no schema promise | no | #640 |
| tool name rules | supported | letters, digits, underscore, hyphen and dot, 1 to 128 characters (some model APIs are stricter, see #615) | yes | |
| tools/call result shape | supported | `text_result` and `error_result`: text content, `structuredContent`, `isError`; image content for `screenshot` | yes | |
| error model | supported | unknown tool or missing name is -32602; bad arguments (`validate_tool_schema`) and tool failures are `isError` results | yes | |
| JSON-RPC rules | supported | wrong version, missing method or bad id is -32600; broken JSON is -32700; arrays are rejected because batching left the spec in 2025-06-18 | yes | |
| multi-document tools | supported | `design.list_open` and `design.activate` in `crates/mcp/src/document_hub.rs`; they only work through the desktop bridge and are refused over plain stdio | no | |
| resources | supported | `crates/mcp/src/resources.rs`: `pcbcraft://design`, `commands`, `checks`, template `pcbcraft://design/preview/{view}`; no subscribe | no | #643 |
| prompts | supported | `crates/mcp/src/prompts.rs`: `review_board`, `fix_unrouted_nets`, `prepare_fabrication` | no | #644 |
| logging (`logging/setLevel`) | absent | stdout carries only protocol messages; stderr is free | no | skip: nothing to report yet |
| completion, sampling, roots, elicitation, tasks | absent | client features or optional | no | skip: no use in the product |
| icons metadata | absent | optional | no | skip |
| stdio framing (transports) | supported | `serve` and `read_message`: newline-delimited UTF-8, 1 MiB cap, only JSON on stdout | yes | |
| Streamable HTTP (transports) | supported | `crates/mcp/src/http.rs`: one endpoint `/mcp`, POST only, JSON replies, loopback bind, bearer token, Origin check, stateless, no event stream; GET and DELETE answer 405 | no | #641 |
| loopback TCP bridge | not a spec transport | `start_bridge` and `forward` carry one request per connection to the desktop app; kept for `--connect` | n/a | |
| authorization | not needed | stdio takes no credentials; the HTTP transport uses a per-run bearer token; the OAuth flow applies only to remote listeners, which are out of scope | n/a | #641 |

## Gaps and issues

- Tool title and annotations: #636.
- Lifecycle gate before initialize: #637.
- Protocol conformance tests: #638 (part of #509).
- Cancellation of long file-writing tools: #479, optional (documented limitation).
- `outputSchema` for `inspect_design` and `checks`: #640, optional.
- Streamable HTTP: #641. Resources: #643. Prompts: #644. Parent of these three: #614.
- Skipped on purpose: logging, completion, sampling, roots, elicitation, tasks, icons,
  `serverInfo` extras. None is required of a tools server, and the product has no use for them yet.

## Conformance tests

Protocol tests in `crates/mcp/src/protocol_tests.rs` (#638):

1. `initialize_answers_each_supported_version_and_falls_back_to_the_newest_for_an_unknown_one`
2. `initialize_without_client_info_is_refused_with_invalid_params`
3. `a_notification_never_gets_a_reply`
4. `ping_returns_an_empty_object`
5. `an_unknown_method_gets_method_not_found`
6. `a_request_with_the_wrong_jsonrpc_version_or_a_missing_method_gets_invalid_request`
7. `a_json_array_request_is_rejected_because_batching_is_not_supported`
8. `broken_json_gets_parse_error_with_a_null_id`
9. `every_tool_name_is_one_to_128_letters_digits_underscore_hyphen_or_dot`
10. `an_unknown_tool_name_is_a_protocol_error_but_bad_arguments_are_a_tool_error_with_is_error_true`
11. `a_tool_result_carries_text_content_and_a_structured_copy`
12. `a_message_over_one_mebibyte_is_refused_and_stdout_stays_json_only`
13. `capabilities_advertise_only_tools_and_resources_and_prompts_calls_are_method_not_found`

Lifecycle tests in `crates/mcp/src/lib.rs` (#637):
`methods_other_than_initialize_and_ping_are_refused_before_initialize`,
`ping_works_before_initialize`, `a_failed_initialize_does_not_open_the_session`.

Annotation tests in `crates/mcp/src/lib.rs` (#636):
`every_listed_tool_has_a_title_and_the_four_annotation_hints`,
`tools_whose_description_says_without_editing_the_project_are_not_destructive`,
`read_only_tools_never_change_the_revision`, `new_design_and_open_are_marked_destructive`.

Transport tests in `crates/mcp/src/http_tests.rs` (#641) and
`a_http_client_lists_tools_resources_and_prompts`; resource tests in
`crates/mcp/src/resources.rs` (#643); prompt tests in `crates/mcp/src/prompts.rs` (#644).
