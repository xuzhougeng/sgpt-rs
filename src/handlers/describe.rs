//! Describe-shell handler: streams brief description of a shell command.

use crate::printer::MarkdownPrinter;
use anyhow::Result;
use futures_util::StreamExt;

use crate::{
    config::Config,
    llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent},
    role::{default_role_text, DefaultRole},
};

pub async fn run(
    prompt: &str,
    model: &str,
    temperature: f32,
    top_p: f32,
    markdown: bool,
    max_tokens: Option<u32>,
    image_parts: Option<Vec<crate::llm::ContentPart>>,
) -> Result<()> {
    let cfg = Config::load();
    let client = LlmClient::from_config(&cfg)?;
    let role_text = default_role_text(&cfg, DefaultRole::DescribeShell);

    // Create user message with optional images
    let user_message = match image_parts {
        Some(mut parts) => {
            parts.insert(0, crate::llm::ContentPart::text(prompt.to_string()));
            ChatMessage::multimodal(Role::User, parts)
        }
        None => ChatMessage::new(Role::User, prompt.to_string()),
    };

    let messages = vec![ChatMessage::new(Role::System, role_text), user_message];
    let opts = ChatOptions {
        model: model.to_string(),
        temperature,
        top_p,
        tools: None,
        parallel_tool_calls: false,
        tool_choice: None,
        max_tokens,
    };

    let mut stream = client.chat_stream(messages, opts);
    let mut text = String::new();
    while let Some(ev) = stream.next().await {
        match ev? {
            StreamEvent::Content(t) => {
                if !markdown {
                    print!("{}", t)
                } else {
                    text.push_str(&t);
                }
            }
            StreamEvent::Done => {
                if !markdown {
                    println!();
                }
            }
            _ => {}
        }
    }
    if markdown && !text.is_empty() {
        MarkdownPrinter::default().print(&text);
    }
    Ok(())
}
