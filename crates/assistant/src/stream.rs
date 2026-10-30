//! Server-sent event parsers for streaming chat replies (OpenAI-compatible and Anthropic Messages).

use crate::api::Protocol;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Text the worker should show while bytes still arrive.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamText {
    pub thought: bool,
    pub text: String,
}

/// Parses one protocol's event stream into live text and a final JSON body for [`crate::api::reply`].
pub struct StreamParser {
    protocol: Protocol,
    line_buf: String,
    event_name: String,
    openai: OpenAiAcc,
    anthropic: AnthropicAcc,
}

struct OpenAiAcc {
    content: String,
    tool_calls: BTreeMap<usize, OpenAiToolAcc>,
    finish_reason: Option<String>,
}

struct OpenAiToolAcc {
    id: String,
    name: String,
    arguments: String,
}

struct AnthropicAcc {
    blocks: BTreeMap<usize, AnthropicBlockAcc>,
    stop_reason: Option<String>,
}

enum AnthropicBlockAcc {
    Text(String),
    Thinking(String),
    Tool {
        id: String,
        name: String,
        input_json: String,
    },
}

impl StreamParser {
    pub fn new(protocol: Protocol) -> Self {
        Self {
            protocol,
            line_buf: String::new(),
            event_name: String::new(),
            openai: OpenAiAcc {
                content: String::new(),
                tool_calls: BTreeMap::new(),
                finish_reason: None,
            },
            anthropic: AnthropicAcc {
                blocks: BTreeMap::new(),
                stop_reason: None,
            },
        }
    }

    /// Feed raw bytes; returns any new text to emit.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<StreamText> {
        let text = match std::str::from_utf8(chunk) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        self.line_buf.push_str(text);
        let mut out = Vec::new();
        while let Some(pos) = self.line_buf.find('\n') {
            let line = self.line_buf[..pos].trim_end_matches('\r').to_string();
            self.line_buf.drain(..=pos);
            if let Some(t) = self.line(&line) {
                out.extend(t);
            }
        }
        out
    }

    fn line(&mut self, line: &str) -> Option<Vec<StreamText>> {
        if line.is_empty() {
            self.event_name.clear();
            return None;
        }
        if let Some(name) = line.strip_prefix("event:") {
            self.event_name = name.trim().to_string();
            return None;
        }
        let data = line.strip_prefix("data:")?.trim();
        if data == "[DONE]" {
            return None;
        }
        let v: Value = serde_json::from_str(data).ok()?;
        match self.protocol {
            Protocol::OpenAi => self.openai_data(&v),
            Protocol::Anthropic => self.anthropic_data(&v),
        }
    }

    fn openai_data(&mut self, v: &Value) -> Option<Vec<StreamText>> {
        let choice = v.get("choices")?.as_array()?.first()?;
        if let Some(r) = choice.get("finish_reason").and_then(Value::as_str) {
            if !r.is_empty() {
                self.openai.finish_reason = Some(r.to_string());
            }
        }
        let delta = choice.get("delta")?;
        let mut out = Vec::new();
        if let Some(t) = delta.get("content").and_then(Value::as_str) {
            if !t.is_empty() {
                self.openai.content.push_str(t);
                out.push(StreamText {
                    thought: false,
                    text: t.to_string(),
                });
            }
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for c in calls.iter().take(32) {
                let idx = c.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let slot = self
                    .openai
                    .tool_calls
                    .entry(idx)
                    .or_insert_with(|| OpenAiToolAcc {
                        id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    });
                if let Some(id) = c.get("id").and_then(Value::as_str) {
                    slot.id = id.to_string();
                }
                if let Some(name) = c.pointer("/function/name").and_then(Value::as_str) {
                    slot.name = name.to_string();
                }
                if let Some(args) = c.pointer("/function/arguments").and_then(Value::as_str) {
                    slot.arguments.push_str(args);
                }
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    fn anthropic_data(&mut self, v: &Value) -> Option<Vec<StreamText>> {
        let kind = v.get("type").and_then(Value::as_str)?;
        match kind {
            "content_block_start" => {
                let idx = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let block = v.get("content_block")?;
                let t = block.get("type").and_then(Value::as_str)?;
                let acc = match t {
                    "text" => AnthropicBlockAcc::Text(String::new()),
                    "thinking" => AnthropicBlockAcc::Thinking(String::new()),
                    "tool_use" => AnthropicBlockAcc::Tool {
                        id: block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        name: block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        input_json: String::new(),
                    },
                    _ => return None,
                };
                self.anthropic.blocks.insert(idx, acc);
                None
            }
            "content_block_delta" => {
                let idx = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let delta = v.get("delta")?;
                let dt = delta.get("type").and_then(Value::as_str)?;
                let slot = self.anthropic.blocks.get_mut(&idx)?;
                match (slot, dt) {
                    (AnthropicBlockAcc::Text(buf), "text_delta") => {
                        let t = delta.get("text").and_then(Value::as_str).unwrap_or("");
                        if !t.is_empty() {
                            buf.push_str(t);
                            Some(vec![StreamText {
                                thought: false,
                                text: t.to_string(),
                            }])
                        } else {
                            None
                        }
                    }
                    (AnthropicBlockAcc::Thinking(buf), "thinking_delta") => {
                        let t = delta
                            .get("thinking")
                            .or_else(|| delta.get("text"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if !t.is_empty() {
                            buf.push_str(t);
                            Some(vec![StreamText {
                                thought: true,
                                text: t.to_string(),
                            }])
                        } else {
                            None
                        }
                    }
                    (AnthropicBlockAcc::Tool { input_json, .. }, "input_json_delta") => {
                        let p = delta
                            .get("partial_json")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        input_json.push_str(p);
                        None
                    }
                    _ => None,
                }
            }
            "message_delta" => {
                if let Some(r) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.anthropic.stop_reason = Some(r.to_string());
                }
                None
            }
            "message_stop" => None,
            _ => None,
        }
    }

    /// Build the non-streaming JSON shape [`crate::api::reply`] expects.
    pub fn finish_body(&self) -> Value {
        match self.protocol {
            Protocol::OpenAi => {
                let mut message = json!({"role": "assistant", "content": null});
                if !self.openai.content.is_empty() {
                    message["content"] = json!(self.openai.content);
                }
                let calls: Vec<Value> = self
                    .openai
                    .tool_calls
                    .iter()
                    .map(|(i, c)| {
                        json!({
                            "index": i,
                            "id": c.id,
                            "type": "function",
                            "function": {"name": c.name, "arguments": c.arguments},
                        })
                    })
                    .collect();
                if !calls.is_empty() {
                    message["tool_calls"] = Value::Array(calls);
                }
                let finish = self
                    .openai
                    .finish_reason
                    .clone()
                    .unwrap_or_else(|| "stop".to_string());
                json!({"choices": [{"message": message, "finish_reason": finish}]})
            }
            Protocol::Anthropic => {
                let mut content = Vec::new();
                for (_i, block) in self.anthropic.blocks.iter() {
                    match block {
                        AnthropicBlockAcc::Text(t) if !t.is_empty() => {
                            content.push(json!({"type": "text", "text": t}));
                        }
                        AnthropicBlockAcc::Thinking(t) if !t.is_empty() => {
                            content.push(json!({"type": "thinking", "thinking": t}));
                        }
                        AnthropicBlockAcc::Tool { id, name, input_json } => {
                            let input: Value = if input_json.trim().is_empty() {
                                json!({})
                            } else {
                                serde_json::from_str(input_json).unwrap_or(json!({}))
                            };
                            content.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            }));
                        }
                        _ => {}
                    }
                }
                let mut body = json!({"content": content});
                if let Some(r) = &self.anthropic.stop_reason {
                    body["stop_reason"] = json!(r);
                }
                body
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_sse_deltas_emit_text_and_finish_to_a_message() {
        let mut p = StreamParser::new(Protocol::OpenAi);
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
                   data: {\"choices\":[{\"finish_reason\":\"stop\",\"delta\":{}}]}\n\n\
                   data: [DONE]\n\n";
        let mut live = Vec::new();
        live.extend(p.push(sse.as_bytes()));
        assert_eq!(
            live,
            vec![
                StreamText {
                    thought: false,
                    text: "Hel".into()
                },
                StreamText {
                    thought: false,
                    text: "lo".into()
                }
            ]
        );
        let body = p.finish_body();
        assert_eq!(body["choices"][0]["message"]["content"], "Hello");
    }

    #[test]
    fn anthropic_text_delta_streams_then_finishes() {
        let mut p = StreamParser::new(Protocol::Anthropic);
        let sse = "event: content_block_start\n\
                   data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
                   event: content_block_delta\n\
                   data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n\
                   event: message_delta\n\
                   data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n";
        let live = p.push(sse.as_bytes());
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].text, "Hi");
        let body = p.finish_body();
        assert_eq!(body["content"][0]["text"], "Hi");
        assert_eq!(body["stop_reason"], "end_turn");
    }
}
