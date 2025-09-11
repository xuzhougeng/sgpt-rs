//! Chat handler: temporary streaming without persisted history.

use anyhow::Result;
use futures_util::StreamExt;

use crate::cache::{ChatSession, RequestCache};
use crate::config::Config;
use crate::functions::Registry;
use crate::llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent};
use crate::llm::{FunctionCall, ToolCall, ToolSchema};
use crate::printer::MarkdownPrinter;
use crate::role::{resolve_role_text, DefaultRole};

pub async fn run(
    chat_id: &str,
    prompt: &str,
    model: &str,
    temperature: f32,
    top_p: f32,
    max_tokens: Option<u32>,
    caching: bool,
    markdown: bool,
    allow_functions: bool,
    role_name: Option<&str>,
    image_parts: Option<Vec<crate::llm::ContentPart>>,
) -> Result<()> {
    let cfg = Config::load();
    let client = LlmClient::from_config(&cfg)?;
    let session = ChatSession::from_config(&cfg);
    let base_url = cfg.get("API_BASE_URL").unwrap_or_else(|| "default".into());
    let req_cache = RequestCache::from_config(&cfg);
    let registry = Registry::load(&cfg)?;
    let system_text = resolve_role_text(&cfg, role_name, DefaultRole::Default);

    // temp chat id shouldn't persist
    if chat_id == "temp" {
        session.invalidate(chat_id);
    }

    // Prepare messages
    let mut messages = if session.exists(chat_id) {
        session.read(chat_id)?
    } else {
        vec![ChatMessage::new(Role::System, system_text)]
    };
    if !prompt.is_empty() {
        // Create user message with optional images
        let user_message = match image_parts {
            Some(mut parts) => {
                parts.insert(0, crate::llm::ContentPart::text(prompt.to_string()));
                ChatMessage::multimodal(Role::User, parts)
            }
            None => ChatMessage::new(Role::User, prompt.to_string()),
        };
        messages.push(user_message);
    }
    let mut opts = ChatOptions {
        model: model.to_string(),
        temperature,
        top_p,
        tools: None,
        parallel_tool_calls: false,
        tool_choice: None,
        max_tokens,
    };
    if allow_functions {
        let schemas: Vec<ToolSchema> = registry.schemas();
        if !schemas.is_empty() {
            opts.tools = Some(schemas);
            opts.tool_choice = Some("auto".into());
        }
    }

    // Cache check
    if caching {
        let key = req_cache.key_for(&base_url, model, temperature, top_p, &messages);
        if let Some(text) = req_cache.get(&key) {
            print!("{}\n", text);
            if chat_id != "temp" && !text.is_empty() {
                let mut msgs_to_persist = messages.clone();
                msgs_to_persist.push(ChatMessage::new(Role::Assistant, text));
                session.write(chat_id, msgs_to_persist)?;
            }
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
            StreamEvent::Content(t) => {
                assistant_text.push_str(&t);
                if !markdown {
                    print!("{}", t);
                }
            }
            StreamEvent::ToolCallDelta { name, arguments } => {
                saw_tool_calls = true;
                if let Some(n) = name {
                    tool_name = Some(n);
                }
                if let Some(a) = arguments {
                    tool_args.push_str(&a);
                }
            }
            StreamEvent::ToolCallsFinish => {
                saw_tool_calls = true;
            }
            StreamEvent::Done => {
                if !markdown {
                    println!();
                }
                break;
            }
        }
    }
    if markdown && !assistant_text.is_empty() {
        MarkdownPrinter::default().print(&assistant_text);
    }

    // Persist chat if not temp
    if chat_id != "temp" {
        if !assistant_text.is_empty() {
            messages.push(ChatMessage::new(Role::Assistant, assistant_text.clone()));
            session.write(chat_id, messages.clone())?;
        }
    }
    // Tool call execution and second pass
    if saw_tool_calls {
        if let Some(name) = tool_name.clone() {
            let mut assistant_msg = ChatMessage::new(Role::Assistant, String::new());
            assistant_msg.tool_calls = Some(vec![ToolCall {
                id: None,
                r#type: "function".into(),
                function: FunctionCall {
                    name: name.clone(),
                    arguments: tool_args.clone(),
                },
            }]);
            messages.push(assistant_msg);
            let result = registry
                .execute(&name, &tool_args)
                .await
                .unwrap_or_else(|e| format!("tool error: {}", e));
            let mut tool_msg = ChatMessage::new(Role::Tool, result);
            tool_msg.name = Some(name);
            messages.push(tool_msg);
            assistant_text.clear();
            tool_args.clear();
            let mut stream2 = client.chat_stream(messages.clone(), opts.clone());
            while let Some(ev) = stream2.next().await {
                match ev? {
                    StreamEvent::Content(t) => {
                        assistant_text.push_str(&t);
                        if !markdown {
                            print!("{}", t);
                        }
                    }
                    StreamEvent::Done => {
                        if !markdown {
                            println!();
                        }
                        break;
                    }
                    _ => {}
                }
            }
            if markdown && !assistant_text.is_empty() {
                MarkdownPrinter::default().print(&assistant_text);
            }
            if chat_id != "temp" && !assistant_text.is_empty() {
                messages.push(ChatMessage::new(Role::Assistant, assistant_text.clone()));
                session.write(chat_id, messages.clone())?;
            }
        }
    }
    // Write request cache last
    if caching && !assistant_text.is_empty() && !saw_tool_calls {
        let key = req_cache.key_for(&base_url, model, temperature, top_p, &messages);
        let _ = req_cache.set(&key, &assistant_text);
    }
    Ok(())
}
