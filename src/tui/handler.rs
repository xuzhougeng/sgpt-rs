//! Async event handler for TUI REPL mode.

use std::io;
use std::time::Duration;


use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use futures_util::StreamExt;
use ratatui::prelude::*;
use tokio::sync::mpsc;

use crate::{
    cache::ChatSession,
    config::Config,
    llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent},
};
use super::{
    app::{App, InputMode},
    events::TuiEvent,
    ui::render_ui,
};

/// Run the TUI-based REPL
pub async fn run_tui_repl(
    chat_id: &str,
    init_prompt: Option<&str>,
    model: &str,
    temperature: f32,
    top_p: f32,
    max_tokens: Option<u32>,
    _markdown: bool, // Not used in TUI mode
    is_shell: bool,
    allow_interaction: bool,
    role_name: Option<&str>,
) -> Result<()> {
    // Check if we're in a proper terminal environment
    if !io::IsTerminal::is_terminal(&io::stdout()) {
        return Err(anyhow::anyhow!("TUI mode requires a proper terminal environment"));
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize application components
    let cfg = Config::load();
    let client = LlmClient::from_config(&cfg)?;
    let session = ChatSession::from_config(&cfg);

    if chat_id == "temp" {
        session.invalidate(chat_id);
    }

    // Load or create session history
    let system_role_text = if is_shell {
        crate::role::default_role_text(&cfg, crate::role::DefaultRole::Shell)
    } else {
        crate::role::resolve_role_text(&cfg, role_name, crate::role::DefaultRole::Default)
    };

    let history = if session.exists(chat_id) {
        session.read(chat_id)?
    } else {
        vec![ChatMessage {
            role: Role::System,
            content: system_role_text,
            name: None,
            tool_calls: None,
        }]
    };

    // Initialize TUI app state
    let mut app = App::new(
        chat_id.to_string(),
        history,
        is_shell,
        allow_interaction,
        model.to_string(),
    );

    // Create event channels
    let (event_tx, event_rx) = mpsc::unbounded_channel::<TuiEvent>();

    // Process initial prompt if provided
    if let Some(prompt) = init_prompt {
        let prompt_owned = prompt.to_string();
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            if let Err(_) = event_tx_clone.send(TuiEvent::UserInput(prompt_owned)) {
                // Channel closed, ignore
            }
        });
    }

    // Main event loop
    let result = run_app(&mut terminal, &mut app, client, session, event_tx, event_rx, temperature, top_p, max_tokens).await;

    // Restore terminal
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Main application loop
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    client: LlmClient,
    session: ChatSession,
    event_tx: mpsc::UnboundedSender<TuiEvent>,
    mut event_rx: mpsc::UnboundedReceiver<TuiEvent>,
    temperature: f32,
    top_p: f32,
    max_tokens: Option<u32>,
) -> Result<()> {
    // Spawn input handler
    let input_tx = event_tx.clone();
    tokio::task::spawn_blocking(move || {
        loop {
            // Poll for keyboard events
            if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    if let Err(_) = input_tx.send(TuiEvent::Key(key)) {
                        break; // Channel closed
                    }
                }
            }
        }
    });

    loop {
        // Render UI
        terminal.draw(|frame| render_ui(frame, app))?;

        // Handle events
        if let Ok(tui_event) = event_rx.try_recv() {
            match tui_event {
                TuiEvent::Key(key) => {
                    if handle_key_event(app, key, event_tx.clone()).await? {
                        break; // Quit requested
                    }
                }
                TuiEvent::UserInput(input) => {
                    // Check if we should queue the message
                    if !app.try_queue_message(input.clone()) {
                        // Not queued, process immediately
                        handle_user_input(app, input, &client, &session, event_tx.clone(), temperature, top_p, max_tokens).await?;
                    }
                }
                TuiEvent::ProcessNextMessage => {
                    // Process next message from queue
                    if let Some(next_message) = app.dequeue_message() {
                        handle_user_input(app, next_message, &client, &session, event_tx.clone(), temperature, top_p, max_tokens).await?;
                    }
                }
                TuiEvent::LlmStream(stream_event) => {
                    handle_llm_stream_event(app, stream_event, &session, event_tx.clone()).await?;
                }
                TuiEvent::Quit => break,
                TuiEvent::ExecuteCommand(cmd) => {
                    // Execute command in background and capture output
                    let cmd_clone = cmd.clone();
                    let tx = event_tx.clone();
                    tokio::task::spawn_blocking(move || {
                        let output = execute_command_with_output(&cmd_clone);
                        let _ = tx.send(TuiEvent::ExecutionResult { 
                            command: cmd_clone, 
                            output 
                        });
                    });
                }
                TuiEvent::ExecutionResult { command, output } => {
                    app.show_execution_result(command, output);
                }
                TuiEvent::DescribeCommand(cmd) => {
                    // Generate description using fake model or real describe function
                    let description = if app.model == "fake" {
                        generate_fake_command_description(&cmd)
                    } else {
                        // For real models, we could capture describe output here
                        // For now, just show a placeholder
                        "Command description would appear here in real mode.".to_string()
                    };
                    app.show_description(cmd, description);
                }
                _ => {} // Handle other events as needed
            }
        }

        // Small delay to prevent busy waiting
        tokio::time::sleep(Duration::from_millis(16)).await; // ~60 FPS
    }

    Ok(())
}

/// Handle keyboard events
async fn handle_key_event(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    event_tx: mpsc::UnboundedSender<TuiEvent>,
) -> Result<bool> {
    // If any popup is shown, any key closes it
    if app.is_popup_shown() {
        app.hide_popup();
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Ok(true); // Quit
        }
        KeyCode::F(1) => {
            app.toggle_help();
        }
        KeyCode::Up => {
            app.scroll_up();
        }
        KeyCode::Down => {
            app.scroll_down();
        }
        KeyCode::Enter => {
            let input = app.get_input_text();
            
            // Handle special inputs
            if input.trim() == "exit()" {
                return Ok(true);
            }
            
            // Handle multiline mode
            if input.trim() == "\"\"\"" {
                match app.input_mode {
                    InputMode::Normal => {
                        app.input_mode = InputMode::MultiLine;
                        app.clear_input();
                    }
                    InputMode::MultiLine => {
                        let multiline_input = app.multiline_buffer.join("\n");
                        app.clear_input();
                        if !multiline_input.trim().is_empty() {
                            let _ = event_tx.send(TuiEvent::UserInput(multiline_input));
                        }
                    }
                }
            } else if app.input_mode == InputMode::MultiLine {
                app.multiline_buffer.push(app.input.clone());
                app.input.clear();
            } else {
                // Handle shell shortcuts
                if app.is_shell_mode && app.allow_interaction {
                    match input.trim() {
                        "e" | "r" if !app.last_command.is_empty() => {
                            let _ = event_tx.send(TuiEvent::ExecuteCommand(app.last_command.clone()));
                            app.clear_input();
                            return Ok(false);
                        }
                        "d" if !app.last_command.is_empty() => {
                            let _ = event_tx.send(TuiEvent::DescribeCommand(app.last_command.clone()));
                            app.clear_input();
                            return Ok(false);
                        }
                        _ => {}
                    }
                }
                
                // Send regular input
                if !input.trim().is_empty() {
                    let _ = event_tx.send(TuiEvent::UserInput(input));
                }
                app.clear_input();
            }
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(c) => {
            app.input.push(c);
        }
        _ => {}
    }
    
    Ok(false)
}

/// Handle user input processing
async fn handle_user_input(
    app: &mut App,
    input: String,
    client: &LlmClient,
    session: &ChatSession,
    event_tx: mpsc::UnboundedSender<TuiEvent>,
    temperature: f32,
    top_p: f32,
    max_tokens: Option<u32>,
) -> Result<()> {
    if input.trim().is_empty() {
        return Ok(());
    }

    // Add user message to history
    app.add_message(ChatMessage {
        role: Role::User,
        content: input.clone(),
        name: None,
        tool_calls: None,
    });

    // Start streaming response
    app.start_response();

    // Prepare messages for LLM
    let messages = app.messages.clone();
    let opts = ChatOptions {
        model: app.model.clone(),
        temperature,
        top_p,
        tools: None,
        parallel_tool_calls: false,
        tool_choice: None,
        max_tokens,
    };

    // Create streaming request
    let mut stream = client.chat_stream(messages.clone(), opts);

    // Spawn task to handle streaming response
    let _chat_id = app.chat_id.clone();
    tokio::spawn(async move {
        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(stream_event) => {
                    if let Err(_) = event_tx.send(TuiEvent::LlmStream(stream_event)) {
                        break; // Channel closed
                    }
                }
                Err(_) => {
                    // Handle stream errors
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Handle LLM streaming events
async fn handle_llm_stream_event(
    app: &mut App,
    event: StreamEvent,
    session: &ChatSession,
    event_tx: mpsc::UnboundedSender<TuiEvent>,
) -> Result<()> {
    match event {
        StreamEvent::Content(content) => {
            app.append_response(&content);
            app.scroll_to_bottom(); // Auto-scroll to follow new content
        }
        StreamEvent::Done => {
            app.finish_response()?;
            
            // Save session if not temporary
            if app.chat_id != "temp" && !app.messages.is_empty() {
                session.write(&app.chat_id, app.messages.clone())?;
            }
            
            // Process next message from queue if available
            let _ = event_tx.send(TuiEvent::ProcessNextMessage);
        }
        StreamEvent::ToolCallDelta { .. } => {
            // Handle tool calls if needed in the future
        }
        StreamEvent::ToolCallsFinish => {
            // Handle tool call completion
        }
    }
    
    Ok(())
}

/// Execute a command and capture its output
fn execute_command_with_output(command: &str) -> String {
    use std::process::{Command, Stdio};
    
    // Determine shell based on platform
    let (shell_cmd, shell_arg) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    match Command::new(shell_cmd)
        .arg(shell_arg)
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            
            if output.status.success() {
                if stdout.is_empty() && stderr.is_empty() {
                    "Command executed successfully (no output)".to_string()
                } else if stderr.is_empty() {
                    stdout.to_string()
                } else {
                    format!("STDOUT:\n{}\n\nSTDERR:\n{}", stdout, stderr)
                }
            } else {
                format!("Command failed with exit code: {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}", 
                       output.status.code().unwrap_or(-1), stdout, stderr)
            }
        }
        Err(e) => {
            format!("Failed to execute command: {}", e)
        }
    }
}

/// Generate fake command descriptions for testing
fn generate_fake_command_description(command: &str) -> String {
    let cmd_lower = command.to_lowercase();
    
    let description = if cmd_lower.starts_with("ls") {
        "List directory contents. Shows files and directories in the current or specified directory.\n\nCommon options:\n-l: Long format with details\n-a: Show hidden files\n-h: Human readable file sizes"
    } else if cmd_lower.starts_with("git") {
        if cmd_lower.contains("status") {
            "Show the working tree status. Displays which files are staged, modified, or untracked."
        } else if cmd_lower.contains("commit") {
            "Record changes to the repository. Creates a new commit with staged changes."
        } else if cmd_lower.contains("push") {
            "Update remote repository with local commits. Uploads your changes to the remote repository."
        } else {
            "Git is a distributed version control system for tracking changes in source code."
        }
    } else if cmd_lower.starts_with("find") {
        "Search for files and directories. Recursively searches through directory trees to find files matching specified criteria."
    } else if cmd_lower.starts_with("grep") {
        "Search text patterns in files. Searches for lines matching a pattern in one or more files."
    } else if cmd_lower.starts_with("ps") {
        "Display running processes. Shows information about active processes on the system."
    } else if cmd_lower.starts_with("docker") {
        "Container platform command. Manages Docker containers, images, and other Docker resources."
    } else if cmd_lower.starts_with("curl") {
        "Transfer data from or to servers. Downloads or uploads data using various protocols like HTTP, HTTPS, FTP."
    } else if cmd_lower.starts_with("apt") || cmd_lower.starts_with("sudo apt") {
        "Package management for Debian/Ubuntu systems. Installs, updates, or removes software packages."
    } else {
        return format!("Command: {}\n\nThis is a fake description for testing purposes. In real mode, this would provide detailed information about the command, its purpose, common options, and usage examples.", command);
    };
    description.to_string()
}