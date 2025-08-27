//! Default handler: wires reqwest client and streams text output.

use anyhow::Result;
use futures_util::StreamExt;

use crate::config::Config;
use crate::llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent};
use crate::cache::RequestCache;
use crate::printer::MarkdownPrinter;
use crate::functions::Registry;
use crate::llm::{FunctionCall, ToolCall, ToolSchema};

#[allow(dead_code)]
pub struct DefaultHandler;

impl DefaultHandler {
    pub async fn run(prompt: &str, model: &str, temperature: f32, top_p: f32, caching: bool, markdown: bool, allow_functions: bool) -> Result<()> {
        let cfg = Config::load();
        let client = LlmClient::from_config(&cfg)?;
        let base_url = cfg.get("API_BASE_URL").unwrap_or_else(|| "default".into());
        let req_cache = RequestCache::from_config(&cfg);
        let registry = Registry::load(&cfg)?;

        let mut messages = vec![
            ChatMessage { role: Role::System, content: "You are ShellGPT".into(), name: None, tool_calls: None },
            ChatMessage { role: Role::User, content: prompt.to_string(), name: None, tool_calls: None },
        ];
        let mut opts = ChatOptions {
            model: model.to_string(),
            temperature,
            top_p,
            tools: None,
            parallel_tool_calls: false,
            tool_choice: None,
        };
        if allow_functions {
            let schemas: Vec<ToolSchema> = registry.schemas();
            if !schemas.is_empty() { opts.tools = Some(schemas); opts.tool_choice = Some("auto".into()); }
        }

        // Cache check
        if caching {
            let key = req_cache.key_for(&base_url, model, temperature, top_p, &messages);
            if let Some(text) = req_cache.get(&key) {
                print!("{}\n", text);
                return Ok(());
            }
        }

        let mut stream = client.chat_stream(messages.clone(), opts.clone());
        let mut assistant_text = String::new();
        let mut saw_tool_calls = false;
        let mut tool_name: Option<String> = None;
        let mut tool_args = String::new();
        while let Some(ev) = stream.next().await {
            match ev? {
                StreamEvent::Content(t) => { assistant_text.push_str(&t); if !markdown { print!("{}", t); } },
                StreamEvent::ToolCallDelta { name, arguments } => {
                    saw_tool_calls = true;
                    if let Some(n) = name { tool_name = Some(n); }
                    if let Some(a) = arguments { tool_args.push_str(&a); }
                },
                StreamEvent::ToolCallsFinish => { saw_tool_calls = true; },
                StreamEvent::Done => { if !markdown { println!(); } break; }
            }
        }

        if markdown && !assistant_text.is_empty() {
            MarkdownPrinter::default().print(&assistant_text);
        }

        // If tool call happened, execute once and continue the conversation
        if saw_tool_calls {
            if let Some(name) = tool_name.clone() {
                // append assistant tool_calls message
                messages.push(ChatMessage {
                    role: Role::Assistant,
                    content: String::new(),
                    name: None,
                    tool_calls: Some(vec![ToolCall{ id: None, r#type: "function".into(), function: FunctionCall{ name: name.clone(), arguments: tool_args.clone() } }]),
                });
                // execute tool
                let result = registry.execute(&name, &tool_args).await.unwrap_or_else(|e| format!("tool error: {}", e));
                messages.push(ChatMessage { role: Role::Tool, content: result, name: Some(name), tool_calls: None });
                // second call without caching
                assistant_text.clear();
                tool_args.clear();
                let mut stream2 = client.chat_stream(messages.clone(), opts.clone());
                while let Some(ev) = stream2.next().await {
                    match ev? { StreamEvent::Content(t) => { assistant_text.push_str(&t); if !markdown { print!("{}", t); } }, StreamEvent::Done => { if !markdown { println!(); } break; }, _ => {} }
                }
                if markdown && !assistant_text.is_empty() { MarkdownPrinter::default().print(&assistant_text); }
            }
        }

        if caching && !assistant_text.is_empty() && !saw_tool_calls {
            let key = req_cache.key_for(&base_url, model, temperature, top_p, &messages);
            let _ = req_cache.set(&key, &assistant_text);
        }
        Ok(())
    }
}
