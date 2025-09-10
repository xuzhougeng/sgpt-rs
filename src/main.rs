mod cli;
mod handlers;
mod role;
mod printer;
mod config;
mod cache;
mod functions;
mod utils;
mod integration;
mod llm;
mod external;

use anyhow::{anyhow, bail, Result};
use config::Config;
use is_terminal::IsTerminal;
use role::{DefaultRole, SystemRole};
use std::io::{self, Read};

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Cli::parse();

    // Optional: override target shell via CLI before loading config
    if let Some(ts) = args.target_shell.as_deref() {
        // Normalize common values
        let lower = ts.to_ascii_lowercase();
        let norm_owned = match lower.as_str() {
            "pwsh" | "powershell" | "powershell.exe" => "powershell.exe".to_string(),
            "cmd" | "cmd.exe" => "cmd.exe".to_string(),
            other => other.to_string(),
        };
        std::env::set_var("SHELL_NAME", norm_owned);
    }

    // Load config
    let cfg = Config::load();
    // Ensure default roles exist
    let _ = SystemRole::create_defaults(&cfg);

    // Resolve model: CLI overrides config; fall back to DEFAULT_MODEL
    let effective_model = args
        .model
        .clone()
        .or_else(|| cfg.get("DEFAULT_MODEL"))
        .unwrap_or_else(|| "gpt-4o".to_string());

    // stdin handling (pipe support with __sgpt__eof__ delimiter)
    let mut prompt_from_stdin = String::new();
    let stdin_is_tty = io::stdin().is_terminal();
    if !stdin_is_tty {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        if let Some((before, _after)) = buf.split_once("__sgpt__eof__") {
            prompt_from_stdin = before.to_string();
        } else {
            prompt_from_stdin = buf;
        }
    }

    // Editor cannot be combined with stdin input
    if args.editor && !stdin_is_tty {
        bail!("--editor cannot be used with stdin input");
    }

    // Resolve prompt: stdin + optional positional + document
    let arg_prompt = args.prompt.unwrap_or_default();
    let mut prompt = if !prompt_from_stdin.is_empty() && !arg_prompt.is_empty() {
        format!("{}\n\n{}", prompt_from_stdin, arg_prompt)
    } else if !prompt_from_stdin.is_empty() {
        prompt_from_stdin
    } else {
        arg_prompt
    };

    // Process document files if --doc is provided
    if !args.doc.is_empty() {
        let doc_content = utils::read_documents(&args.doc)
            .map_err(|e| anyhow!("Document processing failed: {}", e))?;
        prompt = utils::combine_doc_and_prompt(&doc_content, &prompt);
    }

    // Compute markdown preference early for show_chat
    let md_for_show = if args.no_md { false } else if args.md { true } else { cfg.get_bool("PRETTIFY_MARKDOWN") };

    // Role management shortcuts
    if args.list_roles {
        for p in SystemRole::list(&cfg) { println!("{}", p.display()); }
        return Ok(());
    }
    if let Some(name) = &args.show_role { println!("{}", SystemRole::show(&cfg, name)?); return Ok(()); }
    if let Some(name) = &args.create_role {
        SystemRole::create_interactive(&cfg, name)?;
        println!("Created/updated role: {}", name);
        return Ok(());
    }

    // Show/list chat shortcuts
    if let Some(id) = &args.show_chat {
        use owo_colors::OwoColorize;
        use crate::printer::MarkdownPrinter;
        let session = cache::ChatSession::from_config(&cfg);
        if !session.exists(id) {
            bail!("chat not found: {}", cfg.chat_cache_path().join(id).display());
        }
        let messages = session.read(id)?;
        if md_for_show {
            let mut md_text = String::new();
            for m in messages {
                let role = match m.role { llm::Role::System => "system", llm::Role::User => "user", llm::Role::Assistant => "assistant", llm::Role::Tool => "tool" };
                md_text.push_str(&format!("### {}\n\n{}\n\n", role, m.content));
            }
            MarkdownPrinter::default().print(&md_text);
        } else {
            for m in messages {
                let (role, color) = match m.role {
                    llm::Role::System => ("system", "cyan"),
                    llm::Role::User => ("user", "magenta"),
                    llm::Role::Assistant => ("assistant", "green"),
                    llm::Role::Tool => ("tool", "yellow"),
                };
                let header = match color {
                    "cyan" => format!("{}", role.cyan()),
                    "magenta" => format!("{}", role.magenta()),
                    "green" => format!("{}", role.green()),
                    "yellow" => format!("{}", role.yellow()),
                    _ => role.to_string(),
                };
                println!("{}: {}\n", header, m.content);
            }
        }
        return Ok(());
    }
    if args.list_chats {
        let dir = cfg.chat_cache_path();
        if dir.exists() {
            for entry in std::fs::read_dir(dir)? {
                let e = entry?;
                println!("{}", e.path().display());
            }
        }
        return Ok(());
    }

    // Effective boolean switches with config defaults
    let mut md = if args.no_md {
        false
    } else if args.md {
        true
    } else {
        cfg.get_bool("PRETTIFY_MARKDOWN")
    };
    let interaction = if args.no_interaction {
        false
    } else if args.interaction {
        true
    } else {
        cfg.get_bool("SHELL_INTERACTION")
    };
    let cache = if args.no_cache {
        false
    } else if args.cache {
        true
    } else {
        true // default enabled
    };
    let mut functions = if args.functions {
        true
    } else {
        cfg.get_bool("OPENAI_USE_FUNCTIONS")
    };

    let role = DefaultRole::from_flags(args.shell, args.describe_shell, args.code);
    // Force md off for shell/code/describe; and disable functions in those modes
    if matches!(role, DefaultRole::Shell | DefaultRole::Code | DefaultRole::DescribeShell) {
        md = false;
        functions = false;
    }

    // Handle install-functions shortcut
    if args.install_functions {
        let path = functions::install_default_functions(&cfg)?;
        println!("Installed default function at {}", path.display());
        return Ok(());
    }

    // Handle install-integration (bash/zsh) shortcut
    if args.install_integration {
        integration::install()?;
        return Ok(());
    }

    // Route to handler
    match (args.repl.as_deref(), args.chat.as_deref()) {
        (Some(repl_id), None) => handlers::repl::ReplHandler::run(
            repl_id,
            if prompt.is_empty() { None } else { Some(prompt.as_str()) },
            &effective_model,
            args.temperature,
            args.top_p,
            md_for_show,
            args.shell,
            interaction,
            args.role.as_deref(),
        ).await,
        (None, Some(chat_id)) => handlers::chat::ChatHandler::run(chat_id, prompt.as_str(), &effective_model, args.temperature, args.top_p, cache, md_for_show, functions, args.role.as_deref()).await,
        (None, None) => {
            if args.search {
                if prompt.trim().is_empty() {
                    bail!("Provide a query after --search or via stdin");
                }
                let client = external::tavily::TavilyClient::from_config(&cfg)?;
                let value = client.search(&prompt).await?;
                if let Some(results) = value.get("results").and_then(|v| v.as_array()) {
                    for (i, item) in results.iter().enumerate() {
                        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                        let snippet = item.get("snippet").or_else(|| item.get("content")).and_then(|v| v.as_str()).unwrap_or("");
                        println!("{}. {}\n{}\n{}\n", i + 1, title, url, snippet);
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()));
                }
                Ok(())
            } else if args.enhanced_search {
                if prompt.trim().is_empty() {
                    bail!("Provide a query after --enhanced-search or via stdin");
                }
                handlers::enhanced_search::EnhancedSearchHandler::run(
                    &prompt,
                    &effective_model,
                    Some(args.temperature),
                    Some(args.top_p),
                    &cfg,
                    md_for_show,
                ).await
            } else
            if args.shell {
                let no_interact = !interaction || !stdin_is_tty;
                let explicit_no_interact = args.no_interaction; // only auto-exec when user explicitly passed --no-interaction
                handlers::shell::run(&prompt, &effective_model, args.temperature, args.top_p, no_interact, explicit_no_interact).await
            } else if args.describe_shell {
                handlers::describe::DescribeShellHandler::run(&prompt, &effective_model, args.temperature, args.top_p, md).await
            } else if args.code {
                handlers::code::CodeHandler::run(&prompt, &effective_model, args.temperature, args.top_p).await
            } else {
                handlers::default::DefaultHandler::run(&prompt, &effective_model, args.temperature, args.top_p, cache, md, functions, args.role.as_deref()).await
            }
        }
        _ => Err(anyhow!("--chat and --repl cannot be used together")),
    }
}
