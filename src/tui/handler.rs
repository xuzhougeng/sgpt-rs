//! Async event handler for TUI REPL mode.

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyModifiers,
    KeyboardEnhancementFlags, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
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
    // Enable richer keyboard reporting so Shift+Enter etc. can be detected when supported.
    let _ = stdout.execute(EnableBracketedPaste);
    let _ = stdout.execute(PushKeyboardEnhancementFlags(
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
    ));
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

    // Restore terminal: follow crossterm recommended order and fully reset
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal
        .backend_mut()
        .execute(crossterm::event::DisableMouseCapture)?;
    let _ = terminal.backend_mut().execute(DisableBracketedPaste);
    let _ = terminal.backend_mut().execute(PopKeyboardEnhancementFlags);
    terminal.show_cursor()?;

    // Drop terminal backend before writing to stdout
    drop(terminal);

    // Ensure the shell prompt returns cleanly without requiring an extra keypress
    use std::io::Write as _;
    let mut out = io::stdout();
    write!(out, "\r\n")?;
    out.flush()?;

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
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();
    // Spawn input handler (blocking) and keep a handle so we can abort it cleanly on exit
    let input_tx = event_tx.clone();
    let input_handle = tokio::task::spawn_blocking(move || {
        loop {
            if !running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
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
                    Ok(Event::Paste(s)) => {
                        if let Err(_) = input_tx.send(TuiEvent::Paste(s)) {
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
                TuiEvent::Paste(content) => {
                    app_paste_text(app, &content);
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

    // Signal input thread to stop and wait a moment for it to exit
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = input_handle.await;

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
        // Fallback newline: Ctrl+J inserts newline (for terminals not reporting Shift+Enter)
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            match app.input_mode {
                InputMode::Normal => {
                    app.input_mode = InputMode::MultiLine;
                    if !app.input.is_empty() {
                        app.multiline_buffer.push(app.input.clone());
                        app.input.clear();
                        app.input_cursor = 0;
                    }
                }
                InputMode::MultiLine => {
                    app.multiline_buffer.push(app.input.clone());
                    app.input.clear();
                    app.input_cursor = 0;
                }
            }
        }
        // Fallback submit: Ctrl+S to send (some terminals can't detect Ctrl+Enter)
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Expand any pending paste placeholders before sending
            let input = app.expand_placeholders_for_submit();

            if input.trim() == "exit()" {
                return Ok(true);
            }

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
                            let _ =
                                event_tx.send(TuiEvent::ExecuteCommand(app.last_command.clone()));
                        }
                        app.clear_input();
                        return Ok(false);
                    }
                    "d" if !app.last_command.is_empty() => {
                        let _ = event_tx.send(TuiEvent::DescribeCommand(app.last_command.clone()));
                        app.clear_input();
                        return Ok(false);
                    }
                    "p" if !app.last_command.is_empty() => {
                        // Show last command in a popup (reuse description popup UI)
                        let title = "Last Command".to_string();
                        let cmd = app.last_command.clone();
                        app.show_description(title, cmd);
                        app.clear_input();
                        return Ok(false);
                    }
                    _ => {}
                }
            }

            if !input.trim().is_empty() {
                app.push_history(input.clone());
                let _ = event_tx.send(TuiEvent::UserInput(input));
            }
            app.clear_input();
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Handle double Ctrl+C for quit, single Ctrl+C for clear
            if app.handle_ctrl_c() {
                return Ok(true); // Quit on double Ctrl+C
            }
            return Ok(false);
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+D: delete next char; quit only if composer empty
            if app.get_input_text().trim().is_empty() {
                return Ok(true);
            } else {
                app.delete();
                return Ok(false);
            }
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+L: show variables snapshot (interpreter mode)
            if app.interpreter.is_some() {
                let _ = event_tx.send(TuiEvent::ShowVariables);
            }
        }
        KeyCode::Char('e')
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            // Expand any placeholders inline (optional user action)
            app.expand_placeholders_inline();
        }
        KeyCode::Char('E') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Control + uppercase E (some terminals)
            app.expand_placeholders_inline();
        }
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+A: beginning of line
            app.move_cursor_home();
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+E: end of line
            app.move_cursor_end();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+U: kill to line start
            app.kill_to_line_start();
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+K: kill to line end
            app.kill_to_line_end();
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Ctrl+W: delete previous word
            app.delete_prev_word();
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => {
            // Alt+Backspace: delete previous word
            app.delete_prev_word();
        }
        KeyCode::Delete if key.modifiers.contains(KeyModifiers::ALT) => {
            // Alt+Delete: delete next word
            app.delete_next_word();
        }
        KeyCode::Left
            if key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
        {
            // Word-left
            app.move_cursor_word_left();
        }
        KeyCode::Right
            if key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
        {
            // Word-right
            app.move_cursor_word_right();
        }
        KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Toggle multiline mode
            match app.input_mode {
                InputMode::Normal => {
                    app.input_mode = InputMode::MultiLine;
                    // If there's existing input, move it to multiline buffer
                    if !app.input.is_empty() {
                        app.multiline_buffer.push(app.input.clone());
                        app.input.clear();
                        app.input_cursor = 0;
                    }
                }
                InputMode::MultiLine => {
                    // Convert back to normal mode, joining all lines
                    if !app.multiline_buffer.is_empty() || !app.input.is_empty() {
                        let mut all_lines = app.multiline_buffer.clone();
                        if !app.input.is_empty() {
                            all_lines.push(app.input.clone());
                        }
                        app.input = all_lines.join("\n");
                        app.input_cursor = app.input.len();
                        app.multiline_buffer.clear();
                    }
                    app.input_mode = InputMode::Normal;
                }
            }
        }
        KeyCode::F(1) => {
            app.toggle_help();
        }
        // Ctrl+H: toggle help (some terminals map Ctrl+H to Backspace and may not trigger this)
        KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_help();
        }
        KeyCode::Char('/') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_help();
        }
        KeyCode::Char('?') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_help();
        }
        KeyCode::Up => {
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || app.input_mode == InputMode::MultiLine
            {
                app.scroll_up();
            } else {
                app.history_prev();
            }
        }
        KeyCode::Down => {
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || app.input_mode == InputMode::MultiLine
            {
                app.scroll_down();
            } else {
                app.history_next();
            }
        }
        KeyCode::Enter => {
            // New behavior: Enter=send, Shift+Enter=newline
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                // Shift+Enter -> newline (force multiline behavior)
                match app.input_mode {
                    InputMode::Normal => {
                        app.input_mode = InputMode::MultiLine;
                        if !app.input.is_empty() {
                            app.multiline_buffer.push(app.input.clone());
                            app.input.clear();
                            app.input_cursor = 0;
                        }
                    }
                    InputMode::MultiLine => {
                        app.multiline_buffer.push(app.input.clone());
                        app.input.clear();
                        app.input_cursor = 0;
                    }
                }
            } else {
                // Enter (no Shift) -> submit. Expand placeholders first
                let input = app.expand_placeholders_for_submit();

                if input.trim() == "exit()" {
                    return Ok(true);
                }

                // Handle shell/interpreter shortcuts for single letters
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
                        "p" if !app.last_command.is_empty() => {
                            let title = "Last Command".to_string();
                            let cmd = app.last_command.clone();
                            app.show_description(title, cmd);
                            app.clear_input();
                            return Ok(false);
                        }
                        _ => {}
                    }
                }

                if !input.trim().is_empty() {
                    app.push_history(input.clone());
                    let _ = event_tx.send(TuiEvent::UserInput(input));
                }
                app.clear_input();
            }
        }
        KeyCode::Backspace => {
            app.backspace();
        }
        KeyCode::Delete => {
            app.delete();
        }
        KeyCode::Left => {
            app.move_cursor_left();
        }
        KeyCode::Right => {
            app.move_cursor_right();
        }
        KeyCode::Home => {
            app.move_cursor_home();
        }
        KeyCode::End => {
            app.move_cursor_end();
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ignore control-modified chars for now
            } else {
                app.insert_char(c);
            }
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
    let model_for_error = app.model.clone();
    tokio::spawn(async move {
        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(stream_event) => {
                    if event_tx.send(TuiEvent::LlmStream(stream_event)).is_err() {
                        break; // Channel closed
                    }
                }
                Err(err) => {
                    // On stream error, surface a friendly message and ensure we close the response
                    let friendly = format_stream_error_message(&err.to_string(), &model_for_error);
                    let _ = event_tx.send(TuiEvent::LlmStream(StreamEvent::Content(friendly)));
                    let _ = event_tx.send(TuiEvent::LlmStream(StreamEvent::Done));
                    break;
                }
            }
        }
        // If the stream ended without explicitly sending Done (rare), send Done to unblock queue
        let _ = event_tx.send(TuiEvent::LlmStream(StreamEvent::Done));
    });

    Ok(())
}

fn app_paste_text(app: &mut App, content: &str) {
    const LARGE_PASTE_CHAR_THRESHOLD: usize = 200;
    const MEDIUM_PASTE_CHAR_THRESHOLD: usize = 50;

    let char_count = content.chars().count();
    let content_to_insert = if char_count > LARGE_PASTE_CHAR_THRESHOLD {
        // Show compact placeholder instead of full content
        format!("📋[PASTE: {} chars]", char_count)
    } else if char_count > MEDIUM_PASTE_CHAR_THRESHOLD {
        // Show truncated preview with char count, handling newlines for better display
        let preview: String = content.chars().take(MEDIUM_PASTE_CHAR_THRESHOLD).collect();
        let clean_preview = if preview.contains('\n') {
            // For multiline content, show first line only and indicate more
            let first_line = preview.lines().next().unwrap_or("").to_string();
            if first_line.len() < MEDIUM_PASTE_CHAR_THRESHOLD - 10 && content.lines().count() > 1 {
                format!("{}... (+{} lines)", first_line, content.lines().count() - 1)
            } else {
                first_line
            }
        } else {
            preview
        };
        format!("{}...📋[{} chars total]", clean_preview, char_count)
    } else {
        content.to_string()
    };

    // Register mapping so we can expand later on submit or explicit request
    if char_count > MEDIUM_PASTE_CHAR_THRESHOLD {
        app.register_pending_paste(content_to_insert.clone(), content.to_string());
    }

    let content = &content_to_insert;

    // Check if content contains newlines
    let has_newlines = content.contains('\n');

    match app.input_mode {
        InputMode::Normal => {
            if has_newlines {
                // Auto-switch to multiline mode for multi-line paste
                app.input_mode = InputMode::MultiLine;

                // Insert content properly in multiline mode
                let lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
                if lines.len() > 1 {
                    // Add all but the last line to multiline buffer
                    for line in &lines[..lines.len() - 1] {
                        app.multiline_buffer.push(line.clone());
                    }
                    // Set the last line as current input
                    app.input = lines.last().unwrap_or(&String::new()).clone();
                    app.input_cursor = app.input.chars().count();
                } else {
                    // Single line, just insert normally
                    let byte_idx =
                        crate::utils::unicode::char_to_byte_index(&app.input, app.input_cursor);
                    app.input.insert_str(byte_idx, content);
                    app.input_cursor += content.chars().count();
                }
            } else {
                // Single line paste in normal mode
                let byte_idx =
                    crate::utils::unicode::char_to_byte_index(&app.input, app.input_cursor);
                app.input.insert_str(byte_idx, content);
                app.input_cursor += content.chars().count();
            }
        }
        InputMode::MultiLine => {
            // Already in multiline mode, insert at current position
            if has_newlines {
                let before = app.input.chars().take(app.input_cursor).collect::<String>();
                let after = app.input.chars().skip(app.input_cursor).collect::<String>();

                let lines: Vec<&str> = content.split('\n').collect();
                if let Some(first) = lines.first() {
                    let new_first_line = before + first;

                    if lines.len() == 1 {
                        // Single line in multi-line content
                        let cursor_pos = new_first_line.chars().count();
                        app.input = new_first_line + &after;
                        app.input_cursor = cursor_pos;
                    } else {
                        // Multi-line paste
                        app.multiline_buffer.push(new_first_line);

                        // Add middle lines
                        for line in &lines[1..lines.len() - 1] {
                            app.multiline_buffer.push(line.to_string());
                        }

                        // Last line becomes current input with remaining text
                        let last_line = lines.last().unwrap_or(&"");
                        app.input = format!("{}{}", last_line, after);
                        app.input_cursor = last_line.chars().count();
                    }
                }
            } else {
                // Single line paste in multiline mode
                let byte_idx =
                    crate::utils::unicode::char_to_byte_index(&app.input, app.input_cursor);
                app.input.insert_str(byte_idx, content);
                app.input_cursor += content.chars().count();
            }
        }
    }
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

/// Format a user-friendly error message for streaming failures
fn format_stream_error_message(err_text: &str, model: &str) -> String {
    let mut msg = String::new();
    msg.push_str("❌ Failed to stream from LLM.\n");

    // Show a concise snippet of the original error
    let snippet = err_text.chars().take(800).collect::<String>();
    msg.push_str(&snippet);

    let lower = err_text.to_lowercase();

    // Avoid duplicating hints if backend already provided them
    if !lower.contains("hint:") {
        let mut hints: Vec<String> = Vec::new();

        if lower.contains("401") || lower.contains("unauthorized") || lower.contains("api key") {
            hints.push(
                "Set OPENAI_API_KEY in your env or add it to ~/.config/sgpt_rs/.sgptrc".to_string(),
            );
        }
        if (lower.contains("model")
            && (lower.contains("not found")
                || lower.contains("unknown")
                || lower.contains("invalid")))
            || lower.contains("unknown model")
        {
            hints.push(format!(
                "Check --model (current: {model}) or set DEFAULT_MODEL in ~/.config/sgpt_rs/.sgptrc"
            ));
        }
        if lower.contains("rate limit") || lower.contains("quota") {
            hints.push("You may be rate limited; retry later or reduce concurrency".to_string());
        }
        if lower.contains("multimodal") || lower.contains("vision") || lower.contains("image") {
            hints.push("Your provider may not support images/vision for this endpoint; try without image options or use a vision-capable model".to_string());
        }

        if !hints.is_empty() {
            msg.push_str("\n💡 Hints: ");
            msg.push_str(&hints.join("; "));
        }
    }

    msg
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
