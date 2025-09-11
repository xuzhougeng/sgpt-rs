//! Shell command handler with interactive flow.

use std::io::{self, Write};

use anyhow::Result;
use futures_util::StreamExt;

use crate::{
    config::Config,
    llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent},
    role::{resolve_role_text, DefaultRole},
    utils::run_command,
};

/// Generate shell command for a prompt and optionally interact/execute.
pub async fn run(
    prompt: &str,
    model: &str,
    temperature: f32,
    top_p: f32,
    max_tokens: Option<u32>,
    no_interaction: bool,
    auto_execute: bool,
    image_parts: Option<Vec<crate::llm::ContentPart>>,
) -> Result<()> {
    let cfg = Config::load();
    let client = LlmClient::from_config(&cfg)?;
    let role_text = resolve_role_text(&cfg, None, DefaultRole::Shell);
    let default_exec = cfg.get_bool("DEFAULT_EXECUTE_SHELL_CMD");

    // Helper to ask LLM for a command based on a user prompt
    async fn gen_cmd(
        client: &LlmClient,
        role_text: &str,
        model: &str,
        temperature: f32,
        top_p: f32,
        max_tokens: Option<u32>,
        user_prompt: String,
        image_parts: Option<Vec<crate::llm::ContentPart>>,
    ) -> Result<String> {
        // Create user message with optional images
        let user_message = match image_parts {
            Some(mut parts) => {
                parts.insert(0, crate::llm::ContentPart::text(user_prompt));
                ChatMessage::multimodal(Role::User, parts)
            }
            None => ChatMessage::new(Role::User, user_prompt),
        };

        let messages = vec![
            ChatMessage::new(Role::System, role_text.to_string()),
            user_message,
        ];
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
        let mut cmd = String::new();
        while let Some(ev) = stream.next().await {
            if let StreamEvent::Content(t) = ev? {
                cmd.push_str(&t);
            }
        }
        Ok(cmd.trim().to_string())
    }

    let mut cmd = gen_cmd(
        &client,
        &role_text,
        model,
        temperature,
        top_p,
        max_tokens,
        prompt.to_string(),
        image_parts.clone(),
    )
    .await?;
    println!("{}", cmd);
    if no_interaction {
        if auto_execute {
            run_command(&cmd);
        }
        return Ok(());
    }

    // Interactive loop until execute or abort
    loop {
        let prompt_str = if default_exec {
            "[E]xecute, [M]odify, [D]escribe, [A]bort (Enter=Execute): "
        } else {
            "[E]xecute, [M]odify, [D]escribe, [A]bort: "
        };
        print!("{}", prompt_str);
        io::stdout().flush().ok();
        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;
        let c = choice.trim().to_lowercase();
        let c = if c.is_empty() && default_exec {
            "e".to_string()
        } else {
            c
        };

        match c.as_str() {
            "e" | "y" => {
                run_command(&cmd);
                break;
            }
            "d" => {
                super::describe::run(&cmd, model, temperature, top_p, false, max_tokens, None)
                    .await?;
                // After describe, show prompt again
            }
            "m" => {
                print!("Modify with instructions: ");
                io::stdout().flush().ok();
                let mut add = String::new();
                io::stdin().read_line(&mut add)?;
                let refine = format!("{}\n\n{}", prompt, add.trim());
                cmd = gen_cmd(
                    &client,
                    &role_text,
                    model,
                    temperature,
                    top_p,
                    max_tokens,
                    refine,
                    image_parts.clone(),
                )
                .await?;
                println!("{}", cmd);
            }
            _ => {
                break;
            } // Abort on anything else
        }
    }

    Ok(())
}
