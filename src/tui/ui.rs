//! UI layout and rendering logic for the TUI.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::app::{App, InputMode, PopupState};
use crate::llm::Role;

/// Render the main UI
pub fn render_ui(frame: &mut Frame, app: &App) {
    // Dynamically size the input area based on multiline state
    let area = frame.area();
    let input_lines = match app.input_mode {
        InputMode::Normal => 1u16,
        InputMode::MultiLine => (app.multiline_buffer.len() as u16).saturating_add(1),
    };
    // Account for borders around the input box (+2). Minimum visual height is 3.
    let desired_input_height = input_lines.saturating_add(2).max(3);
    // Ensure chat area (min 3) and status bar (1) always have room
    let max_input_height = area.height.saturating_sub(4);
    let input_height = desired_input_height.min(max_input_height.max(1));

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),               // Chat area
            Constraint::Length(input_height), // Input area (dynamic)
            Constraint::Length(1),            // Status bar
        ])
        .split(area);

    // Render chat area
    render_chat_area(frame, app, main_layout[0]);

    // Render input area
    render_input_area(frame, app, main_layout[1]);

    // Render status bar
    render_status_bar(frame, app, main_layout[2]);

    // Render help overlay if requested
    if app.show_help {
        render_help_overlay(frame, app);
    }

    // Render popup if requested
    match &app.popup_state {
        PopupState::ExecutionResult { command, output } => {
            render_execution_result_popup(frame, command, output);
        }
        PopupState::Description {
            command,
            description,
        } => {
            render_description_popup(frame, command, description);
        }
        PopupState::StreamingDescription {
            command,
            current_description,
            is_loading,
        } => {
            render_streaming_description_popup(frame, command, current_description, *is_loading);
        }
        PopupState::None => {}
    }
}

/// Render the chat conversation area
fn render_chat_area(frame: &mut Frame, app: &App, area: Rect) {
    let mut content_lines = Vec::new();
    let visible_msgs = app.visible_messages();

    // Build content as styled text lines
    for msg in visible_msgs {
        let (prefix, style) = match msg.role {
            Role::User => ("> ", Style::default().fg(Color::Green)),
            Role::Assistant => ("", Style::default().fg(Color::Cyan)),
            Role::System => ("SYS ", Style::default().fg(Color::Yellow)),
            Role::Tool => ("TOOL ", Style::default().fg(Color::Magenta)),
            Role::Developer => ("DEV ", Style::default().fg(Color::Blue)),
        };

        let content = format!("{}{}", prefix, msg.content);

        // Add each line with proper styling
        for line in content.lines() {
            content_lines.push(Line::from(vec![Span::styled(line.to_string(), style)]));
        }

        // Add empty line between messages for readability
        if !content.is_empty() {
            content_lines.push(Line::from(""));
        }
    }

    // Add current response if streaming
    if app.is_receiving_response && !app.current_response.is_empty() {
        let style = Style::default().fg(Color::Cyan);

        for line in app.current_response.lines() {
            content_lines.push(Line::from(vec![Span::styled(line.to_string(), style)]));
        }
    }

    let title = format!(
        "Chat History - Session: {} | Model: {}",
        app.chat_id, app.model
    );

    // Calculate scrolling
    let available_height = area.height.saturating_sub(2) as usize; // Account for borders
    let total_lines = content_lines.len();

    // Create text content
    let text_content = Text::from(content_lines);

    // Create paragraph with scrolling
    let mut paragraph = Paragraph::new(text_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title)
                .title_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: false });

    if total_lines > available_height {
        // Calculate scroll position - when chat_scroll_offset is 0, show the bottom
        let scroll_y = if app.chat_scroll_offset == 0 {
            // Auto-scroll: show the latest content
            total_lines.saturating_sub(available_height) as u16
        } else {
            // Manual scroll: respect scroll offset
            let max_scroll = total_lines.saturating_sub(available_height);
            let actual_offset = app.chat_scroll_offset.min(max_scroll);
            (total_lines
                .saturating_sub(available_height)
                .saturating_sub(actual_offset)) as u16
        };

        paragraph = paragraph.scroll((scroll_y, 0));
    }

    frame.render_widget(paragraph, area);
}

/// Render the input area
fn render_input_area(frame: &mut Frame, app: &App, area: Rect) {
    use unicode_width::UnicodeWidthChar;

    // Helper: compute display width (terminal columns) up to n characters
    fn display_width_of_prefix(s: &str, chars: usize) -> usize {
        s.chars()
            .take(chars)
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum()
    }

    // Helper: slice string by display columns [start_col, start_col + max_cols)
    fn slice_by_display_cols(s: &str, start_col: usize, max_cols: usize) -> String {
        if max_cols == 0 {
            return String::new();
        }
        let mut cols = 0usize;
        let mut acc = 0usize;
        let mut out = String::new();
        // Skip until reaching start_col
        let mut iter = s.chars().peekable();
        while let Some(&c) = iter.peek() {
            let w = UnicodeWidthChar::width(c).unwrap_or(0);
            if acc + w > start_col {
                break;
            }
            acc += w;
            iter.next();
        }
        // Take up to max_cols columns
        while let Some(&c) = iter.peek() {
            let w = UnicodeWidthChar::width(c).unwrap_or(0);
            if cols + w > max_cols {
                break;
            }
            out.push(c);
            cols += w;
            iter.next();
        }
        out
    }
    // Compute lines for rendering
    let (lines, cursor_line_idx, cursor_col) = match app.input_mode {
        InputMode::Normal => {
            let l = vec![app.input.clone()];
            let max_chars = app.input.chars().count();
            (l, 0usize, app.input_cursor.min(max_chars))
        }
        InputMode::MultiLine => {
            let mut l = app.multiline_buffer.clone();
            l.push(app.input.clone());
            let idx = l.len().saturating_sub(1);
            let max_chars = app.input.chars().count();
            (l, idx, app.input_cursor.min(max_chars))
        }
    };

    let title = match app.input_mode {
        InputMode::Normal => "Input",
        InputMode::MultiLine => "Multi-line Input",
    };

    // Horizontal scrolling: clamp each line to visible width (columns); for current line ensure cursor is visible
    let inner_width = area.width.saturating_sub(2) as usize; // account for borders
    let mut visible_lines: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if inner_width == 0 {
            visible_lines.push(String::new());
            continue;
        }
        if i == cursor_line_idx {
            // Convert cursor char index to display columns
            let cursor_cols = display_width_of_prefix(line, cursor_col);
            // Ensure cursor is visible within inner_width columns
            let start_col = cursor_cols.saturating_sub(inner_width.saturating_sub(1));
            let slice = slice_by_display_cols(line, start_col, inner_width);
            visible_lines.push(slice);
        } else {
            // Take up to inner_width columns
            let slice = slice_by_display_cols(line, 0, inner_width);
            visible_lines.push(slice);
        }
    }

    let input_text = visible_lines.join("\n");

    let input_paragraph = Paragraph::new(input_text.clone())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title)
                .title_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(input_paragraph, area);

    // Position the cursor inside the input box
    // Border takes 1 cell, so start at x + 1, y + 1
    let inner_x = area.x.saturating_add(1);
    let inner_y = area.y.saturating_add(1);
    let inner_height = area.height.saturating_sub(2) as usize;

    // Determine rendered cursor x based on horizontal scroll (column width aware)
    let x_off = if inner_width == 0 {
        0
    } else {
        let current_line = &lines[cursor_line_idx];
        let cursor_cols = display_width_of_prefix(current_line, cursor_col);
        let start_col = cursor_cols.saturating_sub(inner_width.saturating_sub(1));
        let rel_cols = cursor_cols.saturating_sub(start_col).min(inner_width - 1);
        rel_cols
    } as u16;

    let y_off = match app.input_mode {
        InputMode::Normal => 0u16,
        InputMode::MultiLine => (cursor_line_idx.min(inner_height.saturating_sub(1))) as u16,
    };

    frame.set_cursor_position((inner_x + x_off, inner_y + y_off));
}

/// Render the status bar
fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    // Build base status text (reuse existing semantics)
    // Minimal status text per user preference
    let base_text = app.status_message.clone();

    // Spinner while streaming
    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let tick = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        / 120
        % spinner_frames.len() as u128) as usize;

    let mut spans: Vec<Span> = Vec::new();
    if app.is_receiving_response {
        spans.push(Span::styled(
            format!(" {} ", spinner_frames[tick]),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::styled(
        base_text,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));

    let line = Line::from(spans);
    let status_paragraph = Paragraph::new(line).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(status_paragraph, area);
}

/// Render help overlay
fn render_help_overlay(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Create centered popup area
    let popup_area = centered_rect(80, 70, area);

    // Clear the background
    frame.render_widget(Clear, popup_area);

    let help_lines = if app.is_shell_mode && app.allow_interaction {
        vec![
            Line::from("Shell REPL Help (Ctrl+H to close)"),
            Line::from(""),
            Line::from(
                "Enter = Send    | Shift+Enter = Newline | Ctrl+S = Send | Ctrl+J = Newline",
            ),
            Line::from("↑/↓ = Scroll    | Ctrl+↑/↓ = Scroll chat"),
            Line::from("Ctrl+C = Clear (2x=Quit) | Ctrl+D = Quit | F1/Ctrl+H = Help"),
            Line::from("Ctrl+E = Expand paste placeholders inline"),
            Line::from("e = Execute last | r = Repeat | d = Describe | exit() = Quit REPL"),
        ]
    } else {
        vec![
            Line::from("Help (Ctrl+H to close)"),
            Line::from(""),
            Line::from(
                "Enter = Send    | Shift+Enter = Newline | Ctrl+S = Send | Ctrl+J = Newline",
            ),
            Line::from("↑/↓ = History    | Ctrl+↑/↓ = Scroll chat"),
            Line::from("Ctrl+C = Clear (2x=Quit) | Ctrl+D = Quit | F1/Ctrl+H = Help"),
            Line::from("Ctrl+E = Expand paste placeholders inline"),
        ]
    };

    let help_text = Text::from(help_lines);
    let help_paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Help")
                .title_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(help_paragraph, popup_area);
}

/// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render execution result popup
fn render_execution_result_popup(frame: &mut Frame, command: &str, output: &str) {
    let area = frame.area();

    // Create centered popup area
    let popup_area = centered_rect(85, 75, area);

    // Clear the background
    frame.render_widget(Clear, popup_area);

    // Split the popup into command and result sections
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Command section
            Constraint::Min(5),    // Result section
            Constraint::Length(2), // Instructions
        ])
        .split(popup_area);

    // Render command that was executed
    let command_paragraph = Paragraph::new(format!("Command: {}", command))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Executed Command")
                .title_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(command_paragraph, popup_layout[0]);

    // Render execution result
    let result_paragraph = Paragraph::new(output)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Output")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(result_paragraph, popup_layout[1]);

    // Render instructions
    let instructions = Paragraph::new("Press any key to close")
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
    frame.render_widget(instructions, popup_layout[2]);
}

/// Render streaming command description popup
fn render_streaming_description_popup(
    frame: &mut Frame,
    command: &str,
    current_description: &str,
    is_loading: bool,
) {
    let area = frame.area();

    // Create centered popup area
    let popup_area = centered_rect(85, 75, area);

    // Clear the background
    frame.render_widget(Clear, popup_area);

    // Split the popup into command and description sections
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Command section
            Constraint::Min(5),    // Description section
            Constraint::Length(2), // Instructions
        ])
        .split(popup_area);

    // Render command
    let command_paragraph = Paragraph::new(format!("Command: {command}"))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Command")
                .title_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(command_paragraph, popup_layout[0]);

    // Render description (streaming or completed)
    let description_text = if is_loading && current_description.is_empty() {
        "Generating description..."
    } else if current_description.is_empty() {
        "No description available"
    } else {
        current_description
    };

    let title = if is_loading && !current_description.is_empty() {
        "Description (streaming...)"
    } else {
        "Description"
    };

    let description_paragraph = Paragraph::new(description_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title)
                .title_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(description_paragraph, popup_layout[1]);

    // Render instructions
    let instructions_text = if is_loading {
        "Generating... Press any key to close when done"
    } else {
        "Press any key to close"
    };

    let instructions = Paragraph::new(instructions_text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
    frame.render_widget(instructions, popup_layout[2]);
}

/// Render command description popup
fn render_description_popup(frame: &mut Frame, command: &str, description: &str) {
    let area = frame.area();

    // Create centered popup area
    let popup_area = centered_rect(85, 75, area);

    // Clear the background
    frame.render_widget(Clear, popup_area);

    // Split the popup into command and description sections
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Command section
            Constraint::Min(5),    // Description section
            Constraint::Length(2), // Instructions
        ])
        .split(popup_area);

    // Render command
    let command_paragraph = Paragraph::new(format!("Command: {}", command))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Command")
                .title_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(command_paragraph, popup_layout[0]);

    // Render description
    let description_paragraph = Paragraph::new(description)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Description")
                .title_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(description_paragraph, popup_layout[1]);

    // Render instructions
    let instructions = Paragraph::new("Press any key to close")
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
    frame.render_widget(instructions, popup_layout[2]);
}
