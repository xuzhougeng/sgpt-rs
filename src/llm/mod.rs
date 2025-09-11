//! Reqwest-based LLM client implementing OpenAI-compatible Chat Completions streaming and Responses API.

use std::{pin::Pin, time::Duration};

use anyhow::{Context, Result};
use async_stream::try_stream;
use futures_core::Stream;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::config::Config;

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
    Developer, // New role for Responses API
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    MultiModal(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>, // "low", "high", "auto"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
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

// New structures for Responses API
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[expect(dead_code)]
pub enum ResponseInput {
    Text(String),
    Messages(Vec<ChatMessage>),
}

#[derive(Debug, Clone)]
pub struct ResponseOptions {
    pub model: String,
    pub instructions: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub reasoning: Option<ReasoningOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReasoningOptions {
    pub effort: String, // "low", "medium", "high"
}

impl ReasoningOptions {
    #[expect(dead_code)]
    pub fn low() -> Self {
        Self {
            effort: "low".to_string(),
        }
    }

    #[expect(dead_code)]
    pub fn medium() -> Self {
        Self {
            effort: "medium".to_string(),
        }
    }

    #[expect(dead_code)]
    pub fn high() -> Self {
        Self {
            effort: "high".to_string(),
        }
    }
}

// Response structures
#[derive(Debug, Deserialize)]
#[expect(dead_code)]
pub struct ResponsesApiResponse {
    pub id: String,
    pub object: String,
    pub model: String,
    pub output: Vec<ResponseOutput>,
    pub output_text: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
#[expect(dead_code)]
pub struct ResponseOutput {
    pub id: String,
    pub r#type: String,
    pub role: String,
    pub content: Vec<OutputContent>,
}

#[derive(Debug, Deserialize)]
#[expect(dead_code)]
pub struct OutputContent {
    pub r#type: String,
    pub text: Option<String>,
    pub annotations: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[expect(dead_code)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl Default for MessageContent {
    fn default() -> Self {
        MessageContent::Text(String::new())
    }
}

impl std::fmt::Display for MessageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.extract_text())
    }
}

impl MessageContent {
    /// Create a simple text content
    pub fn text(text: impl Into<String>) -> Self {
        MessageContent::Text(text.into())
    }

    /// Create multimodal content with text and images
    pub fn multimodal(parts: Vec<ContentPart>) -> Self {
        MessageContent::MultiModal(parts)
    }

    /// Get text content if it's a text message
    #[expect(dead_code)]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Extract all text from the content (for multimodal, concatenate all text parts)
    pub fn extract_text(&self) -> String {
        match self {
            MessageContent::Text(text) => text.clone(),
            MessageContent::MultiModal(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

impl ContentPart {
    /// Create a text content part
    pub fn text(text: impl Into<String>) -> Self {
        ContentPart::Text { text: text.into() }
    }

    /// Create an image content part from URL
    #[expect(dead_code)]
    pub fn image_url(url: impl Into<String>, detail: Option<String>) -> Self {
        ContentPart::ImageUrl {
            image_url: ImageUrl {
                url: url.into(),
                detail,
            },
        }
    }

    /// Create an image content part from base64 data
    pub fn image_base64(base64_data: &str, mime_type: &str, detail: Option<String>) -> Self {
        let data_url = format!("data:{};base64,{}", mime_type, base64_data);
        ContentPart::ImageUrl {
            image_url: ImageUrl {
                url: data_url,
                detail,
            },
        }
    }

    /// Create an image content part from file path
    pub fn image_from_file(file_path: &str, detail: Option<String>) -> Result<Self> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Err(anyhow::anyhow!("Image file not found: {}", file_path));
        }

        // Read file and convert to base64
        let image_data = fs::read(path)?;
        let base64_data = base64_encode(&image_data);

        // Determine MIME type from extension
        let mime_type = match path.extension().and_then(|ext| ext.to_str()) {
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("bmp") => "image/bmp",
            _ => return Err(anyhow::anyhow!("Unsupported image format: {}", file_path)),
        };

        Ok(ContentPart::image_base64(&base64_data, mime_type, detail))
    }
}

impl ChatMessage {
    /// Create a simple text message
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: MessageContent::text(content),
            name: None,
            tool_calls: None,
        }
    }

    /// Create a multimodal message with text and images
    pub fn multimodal(role: Role, parts: Vec<ContentPart>) -> Self {
        Self {
            role,
            content: MessageContent::multimodal(parts),
            name: None,
            tool_calls: None,
        }
    }

    /// Add an image from file path to the message
    #[expect(dead_code)]
    pub fn with_image(mut self, image_path: &str, detail: Option<String>) -> Result<Self> {
        let image_part = ContentPart::image_from_file(image_path, detail)?;

        match &mut self.content {
            MessageContent::Text(text) => {
                // Convert to multimodal
                let text_part = ContentPart::text(text.clone());
                self.content = MessageContent::multimodal(vec![text_part, image_part]);
            }
            MessageContent::MultiModal(parts) => {
                parts.push(image_part);
            }
        }

        Ok(self)
    }

    /// Get text content from the message
    #[expect(dead_code)]
    pub fn get_text(&self) -> String {
        self.content.extract_text()
    }
}

/// Simple base64 encoding function
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let mut buf = [0u8; 3];
        for (i, &byte) in chunk.iter().enumerate() {
            buf[i] = byte;
        }

        let b = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32);

        result.push(CHARS[((b >> 18) & 63) as usize] as char);
        result.push(CHARS[((b >> 12) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            CHARS[((b >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            CHARS[(b & 63) as usize] as char
        } else {
            '='
        });
    }

    result
}

#[derive(Debug, Clone)]
pub struct LlmClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

#[expect(dead_code)]
impl ResponseOptions {
    pub fn new(model: String) -> Self {
        Self {
            model,
            instructions: None,
            temperature: None,
            max_tokens: None,
            reasoning: None,
        }
    }

    pub fn with_instructions(mut self, instructions: String) -> Self {
        self.instructions = Some(instructions);
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_reasoning(mut self, effort: &str) -> Self {
        self.reasoning = Some(ReasoningOptions {
            effort: effort.to_string(),
        });
        self
    }
}

#[expect(dead_code)]
impl ResponsesApiResponse {
    /// Get the primary text output from the response
    pub fn get_text(&self) -> Option<&str> {
        // First try the convenience field
        if let Some(ref text) = self.output_text {
            return Some(text);
        }

        // Otherwise extract from output array
        self.output
            .iter()
            .find(|output| output.role == "assistant")
            .and_then(|output| {
                output
                    .content
                    .iter()
                    .find(|content| content.r#type == "output_text")
                    .and_then(|content| content.text.as_deref())
            })
    }

    /// Get all text outputs concatenated
    pub fn get_all_text(&self) -> String {
        if let Some(ref text) = self.output_text {
            return text.clone();
        }

        self.output
            .iter()
            .filter(|output| output.role == "assistant")
            .flat_map(|output| &output.content)
            .filter_map(|content| content.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
    }
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

    /// Create a response using the Responses API (non-streaming)
    pub async fn create_response(
        &self,
        input: ResponseInput,
        opts: ResponseOptions,
    ) -> Result<ResponsesApiResponse> {
        // Check for fake mode
        if opts.model.to_lowercase() == "fake" {
            return Ok(self.fake_response(input, opts));
        }

        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(key) = &self.api_key {
            let hv = HeaderValue::from_str(&format!("Bearer {}", key))?;
            headers.insert(AUTHORIZATION, hv);
        }

        let mut body = serde_json::json!({
            "model": opts.model
        });

        // Set input based on type
        match input {
            ResponseInput::Text(text) => {
                body["input"] = serde_json::json!(text);
            }
            ResponseInput::Messages(messages) => {
                body["input"] = serde_json::to_value(messages)?;
            }
        }

        // Add optional parameters
        if let Some(instructions) = &opts.instructions {
            body["instructions"] = serde_json::json!(instructions);
        }
        if let Some(temperature) = opts.temperature {
            body["temperature"] = serde_json::json!(temperature);
        }
        if let Some(max_tokens) = opts.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(reasoning) = &opts.reasoning {
            body["reasoning"] = serde_json::to_value(reasoning)?;
        }

        let resp = self
            .http
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .context("failed to send response request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let code = status.as_u16();
            let lower = text.to_lowercase();

            let mut error_msg = format!("Responses API error: {} {}", status, text);
            let mut hints: Vec<&str> = Vec::new();

            if code == 401 {
                hints.push("Set OPENAI_API_KEY or export it in your shell");
            }
            if code == 404 {
                hints.push("Your API provider may not support the Responses API; try using chat_stream instead");
            }
            if code == 422 || code == 400 {
                if lower.contains("model")
                    && (lower.contains("not found")
                        || lower.contains("unknown")
                        || lower.contains("invalid"))
                {
                    hints.push(
                        "Check model name or set DEFAULT_MODEL appropriately for your provider",
                    );
                }
            }

            if !hints.is_empty() {
                error_msg.push_str("\nHint: ");
                error_msg.push_str(&hints.join("; "));
            }

            return Err(anyhow::anyhow!(error_msg));
        }

        let response: ResponsesApiResponse =
            resp.json().await.context("failed to parse response")?;

        Ok(response)
    }

    /// Convenience method for simple text input
    #[expect(dead_code)]
    pub async fn generate_text(&self, input: &str, model: &str) -> Result<String> {
        let opts = ResponseOptions::new(model.to_string());
        let response = self
            .create_response(ResponseInput::Text(input.to_string()), opts)
            .await?;
        Ok(response.get_all_text())
    }

    /// Convenience method for text with instructions
    #[expect(dead_code)]
    pub async fn generate_text_with_instructions(
        &self,
        input: &str,
        instructions: &str,
        model: &str,
    ) -> Result<String> {
        let opts =
            ResponseOptions::new(model.to_string()).with_instructions(instructions.to_string());
        let response = self
            .create_response(ResponseInput::Text(input.to_string()), opts)
            .await?;
        Ok(response.get_all_text())
    }

    /// Create a fake response for testing
    fn fake_response(&self, input: ResponseInput, _opts: ResponseOptions) -> ResponsesApiResponse {
        let input_text = match input {
            ResponseInput::Text(text) => text,
            ResponseInput::Messages(messages) => messages
                .iter()
                .filter(|msg| msg.role == Role::User)
                .last()
                .map(|msg| msg.content.extract_text())
                .unwrap_or_default(),
        };

        let fake_text = generate_fake_chat_response(&input_text);

        ResponsesApiResponse {
            id: "resp_fake123".to_string(),
            object: "response".to_string(),
            model: "fake".to_string(),
            output_text: Some(fake_text.clone()),
            output: vec![ResponseOutput {
                id: "msg_fake123".to_string(),
                r#type: "message".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputContent {
                    r#type: "output_text".to_string(),
                    text: Some(fake_text),
                    annotations: vec![],
                }],
            }],
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        }
    }

    /// Check if an error indicates multimodal/vision API incompatibility and enhance error message
    fn enhance_multimodal_error(error: anyhow::Error) -> anyhow::Error {
        let error_str = error.to_string().to_lowercase();

        if error_str.contains("multimodal")
            || error_str.contains("vision")
            || error_str.contains("image")
            || error_str.contains("content")
            || error_str.contains("deserialize")
            || error_str.contains("untagged enum")
            || error_str.contains("chatcompletionrequestcontent")
            || error_str.contains("did not match any variant")
        {
            anyhow::anyhow!(
                "❌ Your LLM provider doesn't support --image functionality.\n\
                 💡 Try running without --image parameter, or use a provider that supports vision models (like OpenAI GPT-4o).\n\
                 \n\
                 Original error: {}", 
                error
            )
        } else {
            error
        }
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
                .map_err(|e| Self::enhance_multimodal_error(anyhow::Error::from(e)))
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

                let llm_error = anyhow::anyhow!("LLM error: {} {}", status, msg);
                Err(Self::enhance_multimodal_error(llm_error))?;
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
                .map(|msg| msg.content.extract_text())
                .unwrap_or_default();

            // Check if this is a shell mode based on system message
            let is_shell_mode = messages.iter()
                .any(|msg| msg.role == Role::System &&
                     (msg.content.extract_text().contains("shell command") ||
                      msg.content.extract_text().contains("Shell Command Generator")));

            // Generate appropriate fake response
            let response = if is_shell_mode {
                generate_fake_shell_response(&last_user_message)
            } else {
                generate_fake_chat_response(&last_user_message)
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
