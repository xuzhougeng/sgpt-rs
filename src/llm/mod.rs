//! Reqwest-based LLM client implementing OpenAI-compatible Chat Completions streaming.

use std::{pin::Pin, time::Duration};

use anyhow::{Context, Result};
use async_stream::try_stream;
use futures_core::Stream;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    pub max_tokens: Option<u32>,
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
        // Strategy A: if base has any version segment like /v{digits}, keep as-is; otherwise append /v1
        let has_version_seg = {
            let segs = trimmed.split('/');
            segs.clone().any(|s| {
                let s = s.trim();
                s.len() > 1 && s.starts_with('v') && s[1..].chars().all(|c| c.is_ascii_digit())
            })
        };
        base_url = if has_version_seg {
            trimmed.to_string()
        } else {
            format!("{}/v1", trimmed)
        };
        let api_key = cfg.get("OPENAI_API_KEY");

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .build()?;

        Ok(Self {
            http,
            base_url,
            api_key,
        })
    }

    pub fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        opts: ChatOptions,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>> {
        // Check for fake mode
        if opts.model.to_lowercase() == "fake" {
            return Box::pin(self.fake_stream(messages, opts));
        }

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
                "max_tokens": opts.max_tokens.unwrap_or(512)
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

            // Avoid moving `resp` in the error branch by wrapping in Option
            let mut resp_opt = Some(resp);
            let status = resp_opt.as_ref().map(|r| r.status()).unwrap();
            if !status.is_success() {
                // Include provider error payload + actionable hints (e.g., tools 422) for easier debugging
                let text = resp_opt.take().unwrap().text().await.unwrap_or_default();
                let mut msg = String::new();
                let snippet = if text.len() > 800 { &text[..800] } else { &text };
                msg.push_str(snippet);

                let code = status.as_u16();
                let lower = text.to_lowercase();
                let mut hints: Vec<&str> = Vec::new();
                if code == 401 {
                    hints.push("Set OPENAI_API_KEY or export it in your shell");
                }
                if code == 422 || code == 400 {
                    if lower.contains("tool_choice") || lower.contains("parallel_tool_calls") || lower.contains("\"tools\"") || lower.contains("function_call") || lower.contains("tool calls") {
                        hints.push("Your backend may not support OpenAI tools; retry without --functions or set OPENAI_USE_FUNCTIONS=false");
                    }
                    if lower.contains("model") && (lower.contains("not found") || lower.contains("unknown") || lower.contains("invalid")) {
                        hints.push("Check model name via --model or set DEFAULT_MODEL appropriately for your provider");
                    }
                }
                if lower.contains("rate limit") || lower.contains("quota") {
                    hints.push("You may be rate limited; retry later or reduce concurrency");
                }

                if !hints.is_empty() {
                    msg.push_str("\nHint: ");
                    msg.push_str(&hints.join("; "));
                }

                Err(anyhow::anyhow!("LLM error: {} {}", status, msg))?;
            }

            let mut buf = String::new();
            let mut stream = resp_opt.take().unwrap().bytes_stream();
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

    /// Create a fake stream that outputs the request content instead of calling the API
    fn fake_stream(
        &self,
        messages: Vec<ChatMessage>,
        _opts: ChatOptions,
    ) -> impl Stream<Item = Result<StreamEvent>> + Send {
        try_stream! {
            // Get the last user message to respond to
            let last_user_message = messages.iter()
                .rev()
                .find(|msg| msg.role == Role::User)
                .map(|msg| msg.content.as_str())
                .unwrap_or("");

            // Check if this is a shell mode based on system message
            let is_shell_mode = messages.iter()
                .any(|msg| msg.role == Role::System &&
                     (msg.content.contains("shell command") ||
                      msg.content.contains("Shell Command Generator")));

            // Generate appropriate fake response
            let response = if is_shell_mode {
                generate_fake_shell_response(last_user_message)
            } else {
                generate_fake_chat_response(last_user_message)
            };

            // Stream the response character by character to simulate real streaming
            for chunk in response.chars().collect::<Vec<_>>().chunks(3) {
                let chunk_str: String = chunk.iter().collect();
                yield StreamEvent::Content(chunk_str);

                // Small delay to simulate network latency (in a real async context)
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }

            yield StreamEvent::Done;
        }
    }
}

/// Generate fake shell command responses
fn generate_fake_shell_response(user_input: &str) -> String {
    let input_lower = user_input.to_lowercase();

    let response = if input_lower.contains("list") || input_lower.contains("show") {
        if input_lower.contains("file") {
            "ls -la"
        } else if input_lower.contains("process") {
            "ps aux"
        } else {
            "ls"
        }
    } else if input_lower.contains("find") {
        if input_lower.contains("file") {
            "find . -name \"*.txt\" -type f"
        } else {
            "find . -name \"*pattern*\""
        }
    } else if input_lower.contains("kill") || input_lower.contains("stop") {
        "pkill process_name"
    } else if input_lower.contains("copy") || input_lower.contains("cp") {
        "cp source.txt destination.txt"
    } else if input_lower.contains("move") || input_lower.contains("mv") {
        "mv oldname.txt newname.txt"
    } else if input_lower.contains("download") {
        "curl -O https://example.com/file"
    } else if input_lower.contains("install") {
        "sudo apt install package-name"
    } else if input_lower.contains("git") {
        if input_lower.contains("commit") {
            "git add . && git commit -m \"your message\""
        } else if input_lower.contains("push") {
            "git push origin main"
        } else {
            "git status"
        }
    } else if input_lower.contains("docker") {
        "docker ps -a"
    } else {
        // Default response for unrecognized patterns
        return format!(
            "# Fake response for: {}\necho \"This is a simulated shell command response\"",
            user_input
        );
    };
    response.to_string()
}

/// Generate fake chat responses  
fn generate_fake_chat_response(user_input: &str) -> String {
    let input_lower = user_input.to_lowercase();

    if input_lower.contains("hello") || input_lower.contains("hi") {
        "Hello! I'm a fake AI assistant for testing purposes. How can I help you today?".to_string()
    } else if input_lower.contains("how are you") {
        "I'm doing well, thanks for asking! I'm just a simulated response to help test the TUI interface.".to_string()
    } else if input_lower.contains("what") && input_lower.contains("time") {
        "I'm a fake model, so I don't have access to real-time information. But I'd estimate it's sometime today!".to_string()
    } else if input_lower.contains("help") {
        "I'm a fake AI assistant for testing. I can:\n- Answer questions (with fake answers)\n- Generate fake shell commands in shell mode\n- Test the streaming interface\n\nTry asking me anything!".to_string()
    } else if input_lower.contains("code") || input_lower.contains("programming") {
        format!("Here's some fake code related to your question about '{}':\n\n```rust\nfn fake_function() {{\n    println!(\"This is fake code for testing\");\n}}\n```", user_input)
    } else if user_input.trim().is_empty() {
        "I notice you sent an empty message. Feel free to ask me anything!".to_string()
    } else {
        format!("I understand you're asking about: \"{}\"\n\nThis is a fake response to test the TUI streaming interface. In a real scenario, I would provide helpful information about your query. The fake model is working correctly if you can see this message streaming in character by character!", user_input)
    }
}

#[derive(Debug)]
pub enum StreamEvent {
    Content(String),
    ToolCallDelta {
        name: Option<String>,
        arguments: Option<String>,
    },
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
