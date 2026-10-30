# Assistant backends: wire formats

What the assistant sends to each kind of model endpoint, checked against each API's own
documentation (retrieved 2026-10-29), and what is still missing. The code is
`crates/assistant/src/api.rs`; the tests are in `crates/assistant/src/api_tests.rs`.

Sources: the OpenAI chat completions description, the Anthropic Messages and tool use pages, the
Ollama API page and its OpenAI compatibility page, and the llama.cpp server README and function
calling page.

## Status table

"Yes" means the code matches the documented shape. Every "No" and "Partial" row names the issue
that closes it or the decision to leave it.

| Protocol | Feature | What the documentation says | Status | Issue or test |
|---|---|---|---|---|
| OpenAI chat | Tools array | `{"type":"function","function":{name,description,parameters}}` | Yes | request carries tools |
| OpenAI chat | Tool name pattern | letters, digits, underscore, dash, at most 64 | Yes | #472, test `every_outgoing_tool_name_matches_the_hosted_name_pattern` |
| OpenAI chat | Tool calls in a reply | `message.tool_calls[]` with `id`, `function.name`, and `function.arguments` as a JSON string | Yes | string arguments, object arguments, bad JSON |
| OpenAI chat | Tool result | `{"role":"tool","tool_call_id":...,"content":...}` | Yes | tool message follows the assistant message |
| OpenAI chat | Parallel calls | several entries in one `tool_calls` | Yes, capped at 32 | two calls, two tool messages in order |
| OpenAI chat | `finish_reason` | `stop`, `length`, `tool_calls`, `content_filter` | Yes | #473, test `an_openai_reply_cut_by_length_shows_a_note_and_ends_the_turn_with_that_reason` |
| OpenAI chat | Output limit | `max_tokens` is deprecated; `max_completion_tokens` replaces it | Neither is sent; the server default applies | none: left to the server |
| OpenAI chat | Streaming | `stream: true` with server-sent deltas | No | #475 |
| OpenAI chat | Model list | `GET /v1/models` | Yes | list parse |
| Anthropic Messages | Request | `model`, required `max_tokens`, top-level `system`, `messages`, `tools[]{name,description,input_schema}` | Yes, `max_tokens` is 8192 | body shape |
| Anthropic Messages | Headers | `x-api-key`, `anthropic-version`, JSON content type | Yes, version `2023-06-01` | headers present |
| Anthropic Messages | Tool name pattern | letters, digits, underscore, dash, at most 128 | Yes | #472, same name test |
| Anthropic Messages | Tool use and result | `tool_use` blocks; the next user message holds `tool_result` blocks with `tool_use_id` and optional `is_error` | Yes | all results in one user message |
| Anthropic Messages | `stop_reason` | `end_turn`, `tool_use`, `max_tokens`, `refusal` and others | Yes | #473, test `an_anthropic_reply_with_max_tokens_shows_a_note` |
| Ollama native `/api/chat` | Tools and results | same function shape; arguments come as a JSON object; results use `tool_name`, no id | No native protocol | decision: not built, see below |
| Ollama native | Options | `options.num_ctx`, `keep_alive`, `stream: false` | No | decision: not built |
| Ollama OpenAI route `/v1` | Tools | supported; `tool_choice` is not | Yes, no `tool_choice` is sent | none |
| Ollama OpenAI route | Context size | the route cannot set `num_ctx`; a long tool list can be cut silently | Yes | #474: Ollama preset note |
| llama.cpp server | OpenAI route | `POST /v1/chat/completions`, `GET /v1/models`, optional Bearer key | Yes | none |
| llama.cpp server | Tool calls | needs the chat template engine (on by default); unrecognized templates use a generic handler that costs more tokens; `parallel_tool_calls: true` allows several calls | Partial: `parallel_tool_calls` is not sent | #474: llama.cpp preset note; sending the flag is optional |
| llama.cpp server | `/props` | `GET /props` returns server properties | Not used | none: optional |

## Decisions

1. Dotted tool names (#472). Names go out with each `.` turned into `__`, other illegal
   characters into `_`, and names over 64 characters are clipped with a short hash. The model's
   answer is mapped back by table, so a name the model invents is reported as an unknown tool.
   Two tools that would share a wire name stop the assistant from starting.
2. Stop reasons (#473). `length`, `max_tokens`, `content_filter` and `refusal` show one plain
   note and end the turn with that reason. A cut-off reply runs none of its tool calls.
3. Streaming is a feature, not a conformance failure (#475).
4. Ollama and llama.cpp need no new protocol. Both speak the OpenAI chat route with tools, so
   they get presets with notes (#474). The Ollama preset uses `http://localhost:11434/v1`, no
   key, and the note about setting context size in a Modelfile. The llama.cpp preset uses
   `http://localhost:8080/v1`, an optional key, and the note about chat templates and tool calls.
5. No native Ollama entry. The only gain would be `num_ctx` control from the app. The owner can
   reopen this.

## Closing rows

When a child issue lands on main, change its row from No or Partial to Yes and keep the issue
number in the last column.
