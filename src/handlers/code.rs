//! Code-only handler: streams code output without explanations.

use anyhow::Result;
use futures_util::StreamExt;

use crate::{
    config::Config,
    llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent},
    role::{default_role_text, DefaultRole},
};

#[allow(dead_code)]
pub struct CodeHandler;

impl CodeHandler {
    pub async fn run(prompt: &str, model: &str, temperature: f32, top_p: f32) -> Result<()> {
        let cfg = Config::load();
        let client = LlmClient::from_config(&cfg)?;
        let role_text = default_role_text(&cfg, DefaultRole::Code);

        let messages = vec![
            ChatMessage { role: Role::System, content: role_text, name: None, tool_calls: None },
            ChatMessage { role: Role::User, content: prompt.to_string(), name: None, tool_calls: None },
        ];
        let opts = ChatOptions { model: model.to_string(), temperature, top_p, tools: None, parallel_tool_calls: false, tool_choice: None };

        let mut stream = client.chat_stream(messages, opts);
        while let Some(ev) = stream.next().await {
            match ev? {
                StreamEvent::Content(t) => print!("{}", t),
                StreamEvent::Done => { println!(); },
                _ => {}
            }
        }
        Ok(())
    }
}
