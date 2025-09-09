//! REPL handler with ChatSession persistence.

use std::io::{self, Write};

use anyhow::Result;
use futures_util::StreamExt;

use crate::{
    cache::ChatSession,
    config::Config,
    llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent},
    printer::MarkdownPrinter,
    role::{default_role_text, resolve_role_text, DefaultRole},
    handlers::describe::DescribeShellHandler,
    utils::run_command,
};

#[allow(dead_code)]
pub struct ReplHandler;

impl ReplHandler {
    pub async fn run(chat_id: &str, init_prompt: Option<&str>, model: &str, temperature: f32, top_p: f32, markdown: bool, is_shell: bool, allow_interaction: bool, role_name: Option<&str>) -> Result<()> {
        let cfg = Config::load();
        let client = LlmClient::from_config(&cfg)?;
        let session = ChatSession::from_config(&cfg);

        if chat_id == "temp" { session.invalidate(chat_id); }

        println!("Entering REPL mode, press Ctrl+C to exit.");

        // start session with system role if not exists
        let system_role_text = if is_shell { default_role_text(&cfg, DefaultRole::Shell) } else { resolve_role_text(&cfg, role_name, DefaultRole::Default) };
        let mut history = if session.exists(chat_id) {
            session.read(chat_id)?
        } else {
            vec![ChatMessage { role: Role::System, content: system_role_text.clone(), name: None, tool_calls: None }]
        };

        // helper to send one prompt and persist (Default role)
        let do_one = |prompt: String, history_ref: &mut Vec<ChatMessage>| -> Result<()> {
            if prompt.trim().is_empty() { return Ok(()); }
            // build messages
            let mut msgs = history_ref.clone();
            msgs.push(ChatMessage { role: Role::User, content: prompt, name: None, tool_calls: None });
            let opts = ChatOptions {
                model: model.to_string(),
                temperature,
                top_p,
                tools: None,
                parallel_tool_calls: false,
                tool_choice: None,
                max_tokens: None,
            };
            let mut stream = client.chat_stream(msgs.clone(), opts);
            let mut assistant_text = String::new();
            futures::executor::block_on(async {
                while let Some(ev) = stream.next().await {
                    match ev? {
                        StreamEvent::Content(t) => { assistant_text.push_str(&t); if !markdown { print!("{}", t); io::stdout().flush().ok(); } },
                        StreamEvent::ToolCallDelta { .. } => {},
                        StreamEvent::ToolCallsFinish => {},
                        StreamEvent::Done => { if !markdown { println!(); } break; }
                    }
                }
                Ok::<(), anyhow::Error>(())
            })?;
            if markdown && !assistant_text.is_empty() { MarkdownPrinter::default().print(&assistant_text); }
            if chat_id != "temp" && !assistant_text.is_empty() {
                msgs.push(ChatMessage { role: Role::Assistant, content: assistant_text, name: None, tool_calls: None });
                session.write(chat_id, msgs.clone())?;
                *history_ref = msgs; // update in-memory
            }
            Ok(())
        };

        // helper for shell role: generate command from user prompt, print and persist
        let mut last_cmd = String::new();
        let do_one_shell = |prompt: String, history_ref: &mut Vec<ChatMessage>, last_cmd_ref: &mut String| -> Result<()> {
            if prompt.trim().is_empty() { return Ok(()); }
            let mut msgs = history_ref.clone();
            msgs.push(ChatMessage { role: Role::User, content: prompt, name: None, tool_calls: None });
            let opts = ChatOptions { model: model.to_string(), temperature, top_p, tools: None, parallel_tool_calls: false, tool_choice: None, max_tokens: None };
            let mut stream = client.chat_stream(msgs.clone(), opts);
            let mut cmd = String::new();
            futures::executor::block_on(async {
                while let Some(ev) = stream.next().await { if let StreamEvent::Content(t) = ev? { cmd.push_str(&t); } }
                Ok::<(), anyhow::Error>(())
            })?;
            let new_cmd = cmd.trim().to_string();
            println!("{}", new_cmd);
            if chat_id != "temp" && !new_cmd.is_empty() {
                msgs.push(ChatMessage { role: Role::Assistant, content: new_cmd.clone(), name: None, tool_calls: None });
                session.write(chat_id, msgs.clone())?;
                *history_ref = msgs;
            }
            *last_cmd_ref = new_cmd;
            Ok(())
        };

        // initial prompt if any
        if let Some(p) = init_prompt {
            if is_shell { do_one_shell(p.to_string(), &mut history, &mut last_cmd)?; } else { do_one(p.to_string(), &mut history)?; }
        }

        // loop reading input
        let stdin = io::stdin();
        loop {
            print!(">>> ");
            io::stdout().flush().ok();
            let mut line = String::new();
            if stdin.read_line(&mut line)? == 0 { break; }
            let line = line.trim_end().to_string();
            if line == "exit()" { break; }
            let prompt = if line == "\"\"\"" {
                // multiline until closing """
                let mut buf = String::new();
                loop {
                    let mut l = String::new();
                    print!("... "); io::stdout().flush().ok();
                    if stdin.read_line(&mut l)? == 0 { break; }
                    let t = l.trim_end();
                    if t == "\"\"\"" { break; }
                    buf.push_str(t);
                    buf.push('\n');
                }
                buf
            } else { line };
            if is_shell {
                // handle e/d shortcuts
                if allow_interaction {
                    let current = last_cmd.clone();
                    if prompt == "e" { if !current.is_empty() { run_command(&current); } continue; }
                    if prompt == "d" { if !current.is_empty() { DescribeShellHandler::run(&current, model, temperature, top_p, false).await?; } continue; }
                    if prompt == "r" { if !current.is_empty() { run_command(&current); } continue; }
                    if prompt == "p" { if !current.is_empty() { println!("{}", current); } continue; }
                    if prompt == "m" {
                        print!("Modify with instructions: "); io::stdout().flush().ok();
                        let mut add = String::new();
                        if stdin.read_line(&mut add)? == 0 { continue; }
                        let refine = format!("{}\n\n{}", current, add.trim());
                        do_one_shell(refine, &mut history, &mut last_cmd)?;
                        continue;
                    }
                }
                do_one_shell(prompt, &mut history, &mut last_cmd)?;
            } else {
                do_one(prompt, &mut history)?;
            }
        }

        Ok(())
    }
}
