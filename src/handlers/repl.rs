//! REPL handler with TUI interface using Ratatui.

use anyhow::Result;
use is_terminal::IsTerminal;
use std::io;

use crate::tui::run_tui_repl;

/// Run REPL mode with TUI interface
pub async fn run(
    chat_id: &str,
    init_prompt: Option<&str>,
    model: &str,
    temperature: f32,
    top_p: f32,
    max_tokens: Option<u32>,
    markdown: bool,
    is_shell: bool,
    allow_interaction: bool,
    role_name: Option<&str>,
) -> Result<()> {
    // Check if TUI mode is available
    if !io::IsTerminal::is_terminal(&io::stdout()) {
        eprintln!("Warning: TUI mode not available in this environment. REPL requires a proper terminal.");
        eprintln!("Try running in a terminal instead of an IDE or redirected output.");
        return Err(anyhow::anyhow!("TUI mode requires a proper terminal environment"));
    }

    run_tui_repl(
        chat_id,
        init_prompt,
        model,
        temperature,
        top_p,
        max_tokens,
        markdown,
        is_shell,
        allow_interaction,
        role_name,
    ).await
}