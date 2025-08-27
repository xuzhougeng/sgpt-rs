//! Reqwest-based LLM client implementing OpenAI-compatible Chat Completions streaming.

use std::{pin::Pin, time::Duration};

use anyhow::{Context, Result};
use async_stream::try_stream;
use futures_core::Stream;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>, // for tool messages if needed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>, // for assistant with tool_calls
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSchema {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    pub r#type: String, // must be "function"
    pub function: FunctionSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub r#type: String, // "function"
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub model: String,
    pub temperature: f32,
    pub top_p: f32,
    pub tools: Option<Vec<ToolSchema>>,
    pub parallel_tool_calls: bool,
    pub tool_choice: Option<String>, // e.g., "auto"
}

#[derive(Debug)]
pub struct LlmClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl LlmClient {
    pub fn from_config(cfg: &Config) -> Result<Self> {
        let timeout = cfg
            .get("REQUEST_TIMEOUT")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        let api_base_url = cfg.get("API_BASE_URL").unwrap_or_else(|| "default".into());
        let mut base_url = if api_base_url == "default" {
            "https://api.openai.com/v1".to_string()
        } else {
            api_base_url
        };
        let trimmed = base_url.trim_end_matches('/');
        if !trimmed.ends_with("/v1") && !trimmed.contains("/v1/") {
            base_url = format!("{}/v1", trimmed);
        } else {
            base_url = trimmed.to_string();
        }
        let api_key = cfg.get("OPENAI_API_KEY");

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .build()?;

        Ok(Self { http, base_url, api_key })
    }

    pub fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        opts: ChatOptions,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>> {
        let http = self.http.clone();
        let base_url = self.base_url.clone();
        let api_key = self.api_key.clone();

        Box::pin(try_stream! {
            let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            headers.insert(reqwest::header::ACCEPT, HeaderValue::from_static("text/event-stream"));
            if let Some(key) = api_key.clone() {
                let hv = HeaderValue::from_str(&format!("Bearer {}", key))?;
                headers.insert(AUTHORIZATION, hv);
            }

            let mut body = serde_json::json!({
                "model": opts.model,
                "temperature": opts.temperature,
                "top_p": opts.top_p,
                "messages": messages,
                "stream": true,
                "max_tokens": 512
            });

            if let Some(tools) = &opts.tools {
                body["tools"] = serde_json::to_value(tools)?;
                body["parallel_tool_calls"] = serde_json::json!(opts.parallel_tool_calls);
                if let Some(choice) = &opts.tool_choice {
                    body["tool_choice"] = serde_json::json!(choice);
                }
            }

            let resp = http
                .post(url)
                .headers(headers)
                .json(&body)
                .send()
                .await
                .context("failed to send chat request")?;

            if !resp.status().is_success() {
                let status = resp.status();
                Err(anyhow::anyhow!("LLM error: {}", status))?;
            }

            let mut buf = String::new();
            let mut stream = resp.bytes_stream();
            use futures_util::StreamExt as _;

            while let Some(chunk) = stream.next().await {
                let bytes = chunk.context("stream error")?;
                buf.push_str(&String::from_utf8_lossy(&bytes));
                // process lines
                while let Some(pos) = buf.find('\n') {
                    let mut line = buf[..pos].to_string();
                    buf = buf[pos+1..].to_string();
                    line = line.trim().to_string();
                    if line.is_empty() || line.starts_with(":") { continue; }
                    if let Some(payload) = line.strip_prefix("data:") {
                        let payload = payload.trim();
                        if payload == "[DONE]" { yield StreamEvent::Done; return; }
                        match serde_json::from_str::<Chunk>(payload) {
                            Ok(chunk) => {
                                for choice in chunk.choices.into_iter() {
                                    if let Some(delta) = choice.delta {
                                        if let Some(content) = delta.content {
                                            if !content.is_empty() {
                                                yield StreamEvent::Content(content);
                                            }
                                        }
                                        if let Some(tcalls) = delta.tool_calls {
                                            for t in tcalls.into_iter() {
                                                let name = t.function.as_ref().and_then(|f| f.name.clone());
                                                let args = t.function.as_ref().and_then(|f| f.arguments.clone());
                                                yield StreamEvent::ToolCallDelta { name, arguments: args };
                                            }
                                        }
                                    }
                                    if let Some(fr) = choice.finish_reason {
                                        if fr == "tool_calls" { yield StreamEvent::ToolCallsFinish; }
                                    }
                                }
                            }
                            Err(_e) => {
                                // ignore malformed lines
                            }
                        }
                    }
                }
            }
        })
    }
}

#[derive(Debug)]
pub enum StreamEvent {
    Content(String),
    ToolCallDelta { name: Option<String>, arguments: Option<String> },
    ToolCallsFinish,
    Done,
}

// Minimal chunk structures for OpenAI-like streaming
#[derive(Debug, Deserialize)]
struct Chunk {
    #[allow(dead_code)]
    id: Option<String>,
    #[allow(dead_code)]
    model: Option<String>,
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    delta: Option<Delta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<String>,
    tool_calls: Option<Vec<ToolCallDeltaPart>>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDeltaPart {
    function: Option<FunctionDeltaPart>,
}

#[derive(Debug, Deserialize)]
struct FunctionDeltaPart {
    name: Option<String>,
    arguments: Option<String>,
}
