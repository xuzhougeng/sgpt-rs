//! Async event handler for TUI REPL mode.

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use futures_util::StreamExt;
use ratatui::prelude::*;
use tokio::sync::mpsc;

use super::{
    app::{App, InputMode},
    events::TuiEvent,
    ui::render_ui,
};
use crate::execution::ExecutionResult as CodeExecResult;
use crate::process::{self, InterpreterType};
use crate::{
    cache::ChatSession,
    config::Config,
    llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

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
    interpreter: Option<InterpreterType>,
) -> Result<()> {
    // Check if we're in a proper terminal environment
    if !io::IsTerminal::is_terminal(&io::stdout()) {
        return Err(anyhow::anyhow!(
            "TUI mode requires a proper terminal environment"
        ));
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(crossterm::event::EnableMouseCapture)?;
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
        vec![ChatMessage::new(Role::System, system_role_text)]
    };

    // Initialize TUI app state
    let mut app = App::new(
        chat_id.to_string(),
        history,
        is_shell,
        allow_interaction,
        model.to_string(),
        interpreter,
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
    let result = run_app(
        &mut terminal,
        &mut app,
        client,
        session,
        event_tx,
        event_rx,
        temperature,
        top_p,
        max_tokens,
    )
    .await;

    // Restore terminal
    disable_raw_mode()?;
    terminal
        .backend_mut()
        .execute(crossterm::event::DisableMouseCapture)?;
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
    // Optional: initialize interpreter session (Python MVP)
    let mut py_stdin_opt: Option<tokio::process::ChildStdin> = None;
    let mut _py_child_opt: Option<tokio::process::Child> = None;
    if matches!(app.interpreter, Some(InterpreterType::Python)) {
        // Python bootstrap script for NDJSON loop
        let bootstrap = r#"
import sys, json, io, traceback, contextlib
user_globals = {}
orig_stdout = sys.stdout
orig_stderr = sys.stderr

def summarize_vars(g):
    summary = {}
    for k, v in g.items():
        if k.startswith('_'):
            continue
        tname = type(v).__name__
        info = tname
        try:
            if tname == 'DataFrame':
                try:
                    info = f'DataFrame({v.shape[0]}x{v.shape[1]})'
                except Exception:
                    info = 'DataFrame'
            elif hasattr(v, 'shape'):
                try:
                    info = f'array{tuple(v.shape)}'
                except Exception:
                    pass
        except Exception:
            pass
        summary[k] = info
    return summary

while True:
    line = sys.stdin.readline()
    if not line:
        break
    line = line.strip()
    if not line:
        continue
    try:
        req = json.loads(line)
    except Exception as e:
        print(json.dumps({"id": None, "error": {"message": "invalid_json", "detail": str(e)}}), file=orig_stdout, flush=True)
        continue
    rid = req.get('id')
    method = req.get('method')
    params = req.get('params', {})
    if method == 'execute':
        code = params.get('code', '')
        capture_output = params.get('capture_output', True)
        out = io.StringIO()
        errors = []
        success = True
        try:
            if capture_output:
                with contextlib.redirect_stdout(out):
                    with contextlib.redirect_stderr(out):
                        exec(code, user_globals)
            else:
                exec(code, user_globals)
        except Exception as e:
            success = False
            tb = traceback.format_exc()
            errors.append(tb)
        output = out.getvalue() if capture_output else ''
        vars_summary = summarize_vars(user_globals)
        resp = {"id": rid, "result": {"success": success, "output": output, "errors": errors, "variables": vars_summary, "plots": []}}
        print(json.dumps(resp), file=orig_stdout, flush=True)
    elif method == 'vars':
        vars_summary = summarize_vars(user_globals)
        resp = {"id": rid, "result": {"success": True, "output": "", "errors": [], "variables": vars_summary, "plots": []}}
        print(json.dumps(resp), file=orig_stdout, flush=True)
    elif method == 'ping':
        print(json.dumps({"id": rid, "result": "pong"}), file=orig_stdout, flush=True)
    else:
        print(json.dumps({"id": rid, "error": {"message": "unknown_method"}}), file=orig_stdout, flush=True)
"#;

        let handle = process::python::start_python(bootstrap).await?;
        let child = handle.child;
        let py_stdin = handle.stdin;
        let stdout = handle.stdout;

        // Spawn reader task for NDJSON responses
        let mut reader = BufReader::new(stdout);
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                let n = match reader.read_line(&mut line).await {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if n == 0 {
                    break;
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let parsed: serde_json::Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let id_str = parsed
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let res = if let Some(obj) = parsed.get("result") {
                    let success = obj
                        .get("success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let output = obj
                        .get("output")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let errors_vec = obj
                        .get("errors")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let mut errors = Vec::new();
                    for e in errors_vec {
                        if let Some(s) = e.as_str() {
                            errors.push(s.to_string());
                        }
                    }
                    let mut variables = std::collections::HashMap::new();
                    if let Some(vars_obj) = obj.get("variables").and_then(|v| v.as_object()) {
                        for (k, v) in vars_obj {
                            if let Some(s) = v.as_str() {
                                variables.insert(k.clone(), s.to_string());
                            }
                        }
                    }
                    let plots = Vec::new();
                    CodeExecResult {
                        success,
                        output,
                        errors,
                        variables,
                        plots,
                    }
                } else if let Some(err) = parsed.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("error");
                    CodeExecResult {
                        success: false,
                        output: String::new(),
                        errors: vec![msg.to_string()],
                        variables: Default::default(),
                        plots: vec![],
                    }
                } else {
                    CodeExecResult {
                        success: false,
                        output: String::new(),
                        errors: vec!["invalid_response".to_string()],
                        variables: Default::default(),
                        plots: vec![],
                    }
                };
                if id_str.starts_with("vars-") {
                    // Format variables snapshot
                    let mut text = String::from("Variables:\n");
                    if res.variables.is_empty() {
                        text.push_str("(none)\n");
                    } else {
                        let mut keys: Vec<_> = res.variables.keys().cloned().collect();
                        keys.sort();
                        for k in keys {
                            if let Some(v) = res.variables.get(&k) {
                                text.push_str(&format!("- {}: {}\n", k, v));
                            }
                        }
                    }
                    let _ = tx.send(TuiEvent::VariablesSnapshot(text));
                } else {
                    let _ = tx.send(TuiEvent::CodeExecutionResult(res));
                }
            }
        });

        py_stdin_opt = Some(py_stdin);
        _py_child_opt = Some(child);
    }
    let mut req_counter: u64 = 1;
    // Spawn input handler
    let input_tx = event_tx.clone();
    tokio::task::spawn_blocking(move || {
        loop {
            // Poll for keyboard events
            if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                match event::read() {
                    Ok(Event::Key(key)) => {
                        if let Err(_) = input_tx.send(TuiEvent::Key(key)) {
                            break;
                        }
                    }
                    Ok(Event::Mouse(m)) => {
                        if let Err(_) = input_tx.send(TuiEvent::Mouse(m)) {
                            break;
                        }
                    }
                    _ => {}
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
                TuiEvent::Mouse(m) => match m.kind {
                    MouseEventKind::ScrollUp => app.scroll_up(),
                    MouseEventKind::ScrollDown => app.scroll_down(),
                    _ => {}
                },
                TuiEvent::UserInput(input) => {
                    // Check if we should queue the message
                    if !app.try_queue_message(input.clone()) {
                        // In interpreter mode, first generate code via LLM, then confirm/execute
                        handle_user_input(
                            app,
                            input,
                            &client,
                            &session,
                            event_tx.clone(),
                            temperature,
                            top_p,
                            max_tokens,
                        )
                        .await?;
                    }
                }
                TuiEvent::ProcessNextMessage => {
                    // Process next message from queue
                    if let Some(next_message) = app.dequeue_message() {
                        handle_user_input(
                            app,
                            next_message,
                            &client,
                            &session,
                            event_tx.clone(),
                            temperature,
                            top_p,
                            max_tokens,
                        )
                        .await?;
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
                            output,
                        });
                    });
                }
                TuiEvent::ExecutionResult { command, output } => {
                    app.show_execution_result(command, output);
                }
                TuiEvent::DescribeCommand(cmd) => {
                    // Generate description using fake model or real describe function
                    if app.model == "fake" {
                        let description = generate_fake_command_description(&cmd);
                        app.show_description(cmd, description);
                    } else {
                        // For real models, start streaming description
                        let _ = event_tx.send(TuiEvent::StartStreamingDescription(cmd.clone()));

                        let cmd_clone = cmd.clone();
                        let model_clone = app.model.clone();
                        let tx = event_tx.clone();
                        tokio::spawn(async move {
                            match generate_streaming_command_description(
                                &cmd_clone,
                                &model_clone,
                                tx.clone(),
                            )
                            .await
                            {
                                Ok(_) => {
                                    let _ = tx.send(TuiEvent::DescriptionStreamFinished);
                                }
                                Err(_) => {
                                    let _ = tx.send(TuiEvent::DescriptionContent(format!(
                                        "Failed to generate description for: {}",
                                        cmd_clone
                                    )));
                                    let _ = tx.send(TuiEvent::DescriptionStreamFinished);
                                }
                            }
                        });
                    }
                }
                TuiEvent::ExecuteCode { language, code } => match language {
                    InterpreterType::Python => {
                        if let Some(stdin) = py_stdin_opt.as_mut() {
                            let id = {
                                let cur = req_counter;
                                req_counter = req_counter.wrapping_add(1);
                                format!("req-{}", cur)
                            };
                            let code = sanitize_generated_code(&code);
                            let req = serde_json::json!({
                                "id": id,
                                "method": "execute",
                                "params": {"code": code, "capture_output": true}
                            });
                            let _ = stdin
                                .write_all((serde_json::to_string(&req).unwrap() + "\n").as_bytes())
                                .await;
                        } else {
                            app.add_message(ChatMessage::new(
                                Role::Assistant,
                                "Interpreter not initialized".to_string(),
                            ));
                        }
                    }
                    InterpreterType::R => {
                        app.add_message(ChatMessage::new(
                            Role::Assistant,
                            "R interpreter is not yet implemented".to_string(),
                        ));
                    }
                },
                TuiEvent::ShowVariables => {
                    if matches!(app.interpreter, Some(InterpreterType::Python)) {
                        if let Some(stdin) = py_stdin_opt.as_mut() {
                            let id = {
                                let cur = req_counter;
                                req_counter = req_counter.wrapping_add(1);
                                format!("vars-{}", cur)
                            };
                            let req =
                                serde_json::json!({ "id": id, "method": "vars", "params": {} });
                            let _ = stdin
                                .write_all((serde_json::to_string(&req).unwrap() + "\n").as_bytes())
                                .await;
                        }
                    }
                }
                TuiEvent::CodeExecutionResult(res) => {
                    let mut text = String::new();
                    if !res.output.is_empty() {
                        text.push_str(&res.output);
                    }
                    if !res.errors.is_empty() {
                        if !text.is_empty() {
                            text.push_str("\n");
                        }
                        text.push_str(&res.errors.join("\n"));
                    }
                    if text.is_empty() && res.success {
                        text = "(ok)".to_string();
                    }
                    app.add_message(ChatMessage::new(Role::Assistant, text));
                }
                TuiEvent::VariablesSnapshot(text) => {
                    app.add_message(ChatMessage::new(Role::Assistant, text));
                }
                TuiEvent::CommandDescription {
                    command,
                    description,
                } => {
                    app.show_description(command, description);
                }
                TuiEvent::StartStreamingDescription(command) => {
                    app.start_streaming_description(command);
                }
                TuiEvent::DescriptionContent(content) => {
                    app.append_description_content(&content);
                }
                TuiEvent::DescriptionStreamFinished => {
                    app.finish_streaming_description();
                }
                _ => {} // Handle other events as needed
            }
        }

        // Small delay to prevent busy waiting
        tokio::time::sleep(Duration::from_millis(16)).await; // ~60 FPS
    }

    // Attempt to terminate interpreter if running
    if let Some(mut child) = _py_child_opt {
        let _ = child.kill().await;
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
        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app.interpreter.is_some() {
                let _ = event_tx.send(TuiEvent::ShowVariables);
            }
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
                // Handle shell/interpreter shortcuts
                if (app.is_shell_mode && app.allow_interaction) || app.interpreter.is_some() {
                    match input.trim() {
                        "e" | "r" if !app.last_command.is_empty() => {
                            if app.interpreter.is_some() {
                                let lang = app.interpreter.unwrap();
                                let _ = event_tx.send(TuiEvent::ExecuteCode {
                                    language: lang,
                                    code: app.last_command.clone(),
                                });
                            } else {
                                let _ = event_tx
                                    .send(TuiEvent::ExecuteCommand(app.last_command.clone()));
                            }
                            app.clear_input();
                            return Ok(false);
                        }
                        "d" if !app.last_command.is_empty() => {
                            let _ =
                                event_tx.send(TuiEvent::DescribeCommand(app.last_command.clone()));
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
    _session: &ChatSession,
    event_tx: mpsc::UnboundedSender<TuiEvent>,
    temperature: f32,
    top_p: f32,
    max_tokens: Option<u32>,
) -> Result<()> {
    if input.trim().is_empty() {
        return Ok(());
    }

    // Add user message to history
    app.add_message(ChatMessage::new(Role::User, input.clone()));

    // Start streaming response
    app.start_response();

    // Prepare messages for LLM
    // If in interpreter mode, inject a system message to produce code only
    let mut messages: Vec<ChatMessage> = Vec::new();
    if let Some(lang) = app.interpreter {
        let content = match lang {
            InterpreterType::Python => "You are a Python code generator. Given the user's request, produce ONLY executable Python code without explanations, comments, or Markdown fences. Avoid triple backticks.",
            InterpreterType::R => "You are an R code generator. Given the user's request, produce ONLY executable R code without explanations, comments, or Markdown fences. Avoid triple backticks.",
        };
        messages.push(ChatMessage::new(Role::System, content.to_string()));
    }
    messages.extend(app.messages.clone());
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
            // Always auto-scroll to show new content when streaming
            app.scroll_to_bottom();
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
                format!(
                    "Command failed with exit code: {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                    output.status.code().unwrap_or(-1),
                    stdout,
                    stderr
                )
            }
        }
        Err(e) => {
            format!("Failed to execute command: {}", e)
        }
    }
}

/// Sanitize generated code by stripping common Markdown code fences
fn sanitize_generated_code(s: &str) -> String {
    let trimmed = s.trim();
    // If contains triple backticks, attempt to extract the first fenced block
    if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        // Skip first fence line (possibly ```python)
        let _ = lines.next();
        let mut buf = String::new();
        for line in lines {
            if line.starts_with("```") {
                break;
            }
            buf.push_str(line);
            buf.push('\n');
        }
        return buf;
    }
    // Also handle indented or leading language lines
    if trimmed.starts_with("# %%") {
        // Simple noop for now
        return trimmed.to_string();
    }
    trimmed.to_string()
}

/// Generate real command description using AI (non-streaming, kept for compatibility)
#[expect(dead_code)]
async fn generate_real_command_description(command: &str, model: &str) -> Result<String> {
    use crate::config::Config;
    use crate::role::{default_role_text, DefaultRole};

    let cfg = Config::load();
    let client = LlmClient::from_config(&cfg)?;
    let role_text = default_role_text(&cfg, DefaultRole::DescribeShell);

    let messages = vec![
        ChatMessage::new(Role::System, role_text),
        ChatMessage::new(Role::User, command.to_string()),
    ];

    let opts = ChatOptions {
        model: model.to_string(),
        temperature: 0.1, // Lower temperature for more consistent descriptions
        top_p: 1.0,
        tools: None,
        parallel_tool_calls: false,
        tool_choice: None,
        max_tokens: Some(500), // Limit description length
    };

    let mut stream = client.chat_stream(messages, opts);
    let mut description = String::new();

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::Content(content) => {
                description.push_str(&content);
            }
            StreamEvent::Done => break,
            _ => {}
        }
    }

    Ok(description.trim().to_string())
}

/// Generate streaming command description using AI
async fn generate_streaming_command_description(
    command: &str,
    model: &str,
    event_sender: mpsc::UnboundedSender<TuiEvent>,
) -> Result<()> {
    use crate::config::Config;
    use crate::role::{default_role_text, DefaultRole};

    let cfg = Config::load();
    let client = LlmClient::from_config(&cfg)?;
    let role_text = default_role_text(&cfg, DefaultRole::DescribeShell);

    let messages = vec![
        ChatMessage::new(Role::System, role_text),
        ChatMessage::new(Role::User, command.to_string()),
    ];

    let opts = ChatOptions {
        model: model.to_string(),
        temperature: 0.1, // Lower temperature for more consistent descriptions
        top_p: 1.0,
        tools: None,
        parallel_tool_calls: false,
        tool_choice: None,
        max_tokens: Some(500), // Limit description length
    };

    let mut stream = client.chat_stream(messages, opts);

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::Content(content) => {
                let _ = event_sender.send(TuiEvent::DescriptionContent(content));
            }
            StreamEvent::Done => break,
            _ => {}
        }
    }

    Ok(())
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
