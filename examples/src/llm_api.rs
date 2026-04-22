// Copyright 2026 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal OpenAI-compatible API client with streaming and tool use.
//!
//! Works with any OpenAI-compatible endpoint including ollama
//! (`http://localhost:11434/v1/chat/completions`).

use std::io::BufRead;
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

/// A message in the conversation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Create a simple text message.
    pub fn text(role: &str, content: &str) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message that includes tool calls.
    pub fn assistant_with_tools(
        content: Option<String>,
        tool_calls: Vec<ToolCallMessage>,
    ) -> Self {
        Self {
            role: "assistant".into(),
            content,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    /// Create a tool result message.
    pub fn tool_result(tool_call_id: &str, content: &str) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// A tool call as it appears in an assistant message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// The function name and arguments in a tool call.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// A tool definition for the API.
#[derive(Clone, Debug, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

impl Tool {
    /// Create a function tool.
    pub fn function(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            tool_type: "function".into(),
            function: FunctionDef {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// Function definition within a tool.
#[derive(Clone, Debug, Serialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Events streamed back from the API.
#[derive(Debug)]
pub enum StreamEvent {
    /// A chunk of text from the assistant.
    TextDelta(String),
    /// The assistant wants to call a tool.
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// The response is complete.
    Done,
    /// An error occurred.
    Error(String),
}

/// Configuration for the API endpoint.
pub struct ApiConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            api_key: None,
            model: "llama3.2".into(),
        }
    }
}

impl ApiConfig {
    /// Create config from environment variables, falling back to ollama defaults.
    ///
    /// Reads `LLM_BASE_URL`, `LLM_API_KEY`, and `LLM_MODEL`.
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("LLM_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".into()),
            api_key: std::env::var("LLM_API_KEY").ok(),
            model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "llama3.2".into()),
        }
    }
}

/// Request body for the chat completions API.
#[derive(Serialize)]
struct ApiRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: &'a Vec<Tool>,
    stream: bool,
}

/// Sends a streaming request to an OpenAI-compatible API in a background thread.
///
/// Returns a receiver that yields `StreamEvent`s as they arrive.
pub fn send_streaming(
    config: &ApiConfig,
    messages: Vec<Message>,
    tools: Vec<Tool>,
) -> mpsc::Receiver<StreamEvent> {
    let (tx, rx) = mpsc::channel();
    let url = format!("{}/v1/chat/completions", config.base_url);
    let api_key = config.api_key.clone();
    let model = config.model.clone();

    std::thread::spawn(move || {
        let body = ApiRequest {
            model: &model,
            messages: &messages,
            tools: &tools,
            stream: true,
        };

        let body_json = match serde_json::to_vec(&body) {
            Ok(j) => j,
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(format!("serialize: {e}")));
                return;
            }
        };

        let mut req = ureq::post(&url).header("content-type", "application/json");
        if let Some(key) = &api_key {
            req = req.header("authorization", &format!("Bearer {key}"));
        }
        let response = req.send(&body_json);

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(format!("request: {e}")));
                return;
            }
        };

        let reader = std::io::BufReader::new(response.into_body().into_reader());

        // Track in-progress tool calls by index.
        let mut tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, args)

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(format!("read: {e}")));
                    break;
                }
            };

            if !line.starts_with("data: ") {
                continue;
            }
            let data = &line[6..];
            if data == "[DONE]" {
                break;
            }

            let chunk: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) else {
                continue;
            };
            let Some(choice) = choices.first() else {
                continue;
            };

            // Check finish_reason.
            if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str())
                && (reason == "stop" || reason == "tool_calls")
            {
                // Emit any accumulated tool calls.
                for (id, name, args) in &tool_calls {
                    let input: serde_json::Value = serde_json::from_str(args)
                        .unwrap_or(serde_json::Value::Null);
                    let _ = tx.send(StreamEvent::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input,
                    });
                }
                let _ = tx.send(StreamEvent::Done);
                return;
            }

            let Some(delta) = choice.get("delta") else {
                continue;
            };

            // Text content.
            if let Some(content) = delta.get("content").and_then(|c| c.as_str())
                && !content.is_empty()
            {
                let _ = tx.send(StreamEvent::TextDelta(content.to_string()));
            }

            // Tool call deltas.
            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tcs {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                    // Grow the vec if needed.
                    while tool_calls.len() <= idx {
                        tool_calls.push((String::new(), String::new(), String::new()));
                    }

                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                        tool_calls[idx].0 = id.to_string();
                    }
                    if let Some(func) = tc.get("function") {
                        if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                            tool_calls[idx].1 = name.to_string();
                        }
                        if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                            tool_calls[idx].2.push_str(args);
                        }
                    }
                }
            }
        }

        let _ = tx.send(StreamEvent::Done);
    });

    rx
}
