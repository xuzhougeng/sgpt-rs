//! UI layout and rendering logic for the TUI.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::llm::{ChatMessage, Role};
use super::app::{App, InputMode, PopupState};

/// Render the main UI
pub fn render_ui(frame: &mut Frame, app: &App) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),      // Chat area
            Constraint::Length(3),   // Input area  
            Constraint::Length(1),   // Status bar
        ])
        .split(frame.area());

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
        PopupState::Description { command, description } => {
            render_description_popup(frame, command, description);
        }
        PopupState::None => {}
    }
}

/// Render the chat conversation area
fn render_chat_area(frame: &mut Frame, app: &App, area: Rect) {
    let mut messages = Vec::new();
    let visible_msgs = app.visible_messages();
    
    // Show messages with proper scrolling: offset 0 = show latest messages
    let total_messages = visible_msgs.len();
    let available_height = area.height.saturating_sub(2) as usize; // Account for borders
    
    let msgs_to_show: Vec<&ChatMessage> = if total_messages <= available_height {
        // All messages fit on screen
        visible_msgs
    } else {
        // More messages than screen space - apply scrolling
        let start_idx = if app.chat_scroll_offset >= total_messages.saturating_sub(available_height) {
            0 // Scrolled to top
        } else {
            total_messages.saturating_sub(available_height).saturating_sub(app.chat_scroll_offset)
        };
        let end_idx = total_messages.saturating_sub(app.chat_scroll_offset);
        
        if start_idx < end_idx {
            visible_msgs[start_idx..end_idx].to_vec()
        } else {
            visible_msgs[total_messages.saturating_sub(available_height)..].to_vec()
        }
    };
    
    for msg in msgs_to_show {
        let (prefix, style) = match msg.role {
            Role::User => (">>> ", Style::default().fg(Color::Green)),
            Role::Assistant => ("", Style::default().fg(Color::Cyan)),
            Role::System => ("SYS ", Style::default().fg(Color::Yellow)),
            Role::Tool => ("TOOL ", Style::default().fg(Color::Magenta)),
        };
        
        let content = format!("{}{}", prefix, msg.content);
        messages.push(ListItem::new(Text::from(content)).style(style));
    }
    
    // Add current response if streaming
    if app.is_receiving_response && !app.current_response.is_empty() {
        let style = Style::default().fg(Color::Cyan);
        messages.push(ListItem::new(Text::from(app.current_response.clone())).style(style));
    }

    let title = format!("Chat History - Session: {} | Model: {}", app.chat_id, app.model);
    let chat_list = List::new(messages)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
        );
        
    frame.render_widget(chat_list, area);
}

/// Render the input area
fn render_input_area(frame: &mut Frame, app: &App, area: Rect) {
    let input_text = match app.input_mode {
        InputMode::Normal => app.input.clone(),
        InputMode::MultiLine => {
            if app.multiline_buffer.is_empty() {
                app.input.clone()
            } else {
                format!("{}\n{}", app.multiline_buffer.join("\n"), app.input)
            }
        },
    };

    let title = match app.input_mode {
        InputMode::Normal => "Input (type \"\"\" for multiline)",
        InputMode::MultiLine => "Multi-line Input (\"\"\" to finish)",
    };

    let input_paragraph = Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
        )
        .wrap(Wrap { trim: true });
        
    frame.render_widget(input_paragraph, area);
}

/// Render the status bar
fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status_text = if app.is_shell_mode && app.allow_interaction && !app.last_command.is_empty() {
        format!("{} | Last: {}", app.status_message, 
                if app.last_command.len() > 50 { 
                    format!("{}...", &app.last_command[..50])
                } else { 
                    app.last_command.clone() 
                })
    } else {
        app.status_message.clone()
    };

    let status_paragraph = Paragraph::new(status_text)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
        
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
            Line::from("Shell REPL Mode Help"),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ↑/↓        - Scroll chat history"),
            Line::from("  Ctrl+C     - Quit"),
            Line::from("  F1         - Toggle this help"),
            Line::from(""),
            Line::from("Input:"),
            Line::from("  Enter      - Send message/command"),
            Line::from("  \"\"\"        - Start/end multi-line input"),
            Line::from(""),
            Line::from("Shell Shortcuts:"),
            Line::from("  e          - Execute last command"),
            Line::from("  r          - Repeat last command"),
            Line::from("  d          - Describe last command"),
            Line::from("  exit()     - Quit REPL"),
        ]
    } else {
        vec![
            Line::from("Chat Mode Help"),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ↑/↓        - Scroll chat history"),
            Line::from("  Ctrl+C     - Quit"),
            Line::from("  F1         - Toggle this help"),
            Line::from(""),
            Line::from("Input:"),
            Line::from("  Enter      - Send message"),
            Line::from("  \"\"\"        - Start/end multi-line input"),
            Line::from("  exit()     - Quit REPL"),
        ]
    };

    let help_text = Text::from(help_lines);
    let help_paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
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
                .title("Executed Command")
                .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(command_paragraph, popup_layout[0]);

    // Render execution result
    let result_paragraph = Paragraph::new(output)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Output")
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(result_paragraph, popup_layout[1]);

    // Render instructions
    let instructions = Paragraph::new("Press any key to close")
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL));
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
                .title("Command")
                .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(command_paragraph, popup_layout[0]);

    // Render description
    let description_paragraph = Paragraph::new(description)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Description")
                .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(description_paragraph, popup_layout[1]);

    // Render instructions
    let instructions = Paragraph::new("Press any key to close")
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(instructions, popup_layout[2]);
}