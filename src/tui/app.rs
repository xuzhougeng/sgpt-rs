//! TUI application state management.

use anyhow::Result;

use crate::llm::{ChatMessage, Role};

/// Input mode for the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    /// Normal single-line input mode
    Normal,
    /// Multi-line input mode (activated by """)
    MultiLine,
}

/// Popup display state
#[derive(Debug, Clone, PartialEq)]
pub enum PopupState {
    /// No popup shown
    None,
    /// Execution result popup
    ExecutionResult { command: String, output: String },
    /// Command description popup
    Description { command: String, description: String },
}

/// Application state for the TUI
#[derive(Debug)]
pub struct App {
    /// Chat session ID
    pub chat_id: String,
    /// Current conversation history
    pub messages: Vec<ChatMessage>,
    /// Input buffer
    pub input: String,
    /// Current input mode
    pub input_mode: InputMode,
    /// Multi-line input buffer
    pub multiline_buffer: Vec<String>,
    /// Whether we're in shell mode
    pub is_shell_mode: bool,
    /// Whether interaction is allowed in shell mode
    pub allow_interaction: bool,
    /// Last generated command (for shell mode)
    pub last_command: String,
    /// Current response being streamed
    pub current_response: String,
    /// Whether we're currently receiving a response
    pub is_receiving_response: bool,
    /// Message queue for pending inputs
    pub message_queue: std::collections::VecDeque<String>,
    /// Status message to display
    pub status_message: String,
    /// Model name being used
    pub model: String,
    /// Whether to show help
    pub show_help: bool,
    /// Scroll offset for chat history
    pub chat_scroll_offset: usize,
    /// Maximum messages to keep in memory for display
    pub max_display_messages: usize,
    /// Popup display state
    pub popup_state: PopupState,
}

impl App {
    /// Create a new TUI application instance
    pub fn new(
        chat_id: String,
        messages: Vec<ChatMessage>,
        is_shell_mode: bool,
        allow_interaction: bool,
        model: String,
    ) -> Self {
        let status_message = if is_shell_mode {
            if allow_interaction {
                "Shell REPL: e=execute, r=repeat, d=describe | Ctrl+C=quit, F1=help"
            } else {
                "Shell Mode | Ctrl+C=quit, F1=help"
            }
        } else {
            "Chat Mode | Ctrl+C=quit, F1=help"
        }.to_string();

        Self {
            chat_id,
            messages,
            input: String::new(),
            input_mode: InputMode::Normal,
            multiline_buffer: Vec::new(),
            is_shell_mode,
            allow_interaction,
            last_command: String::new(),
            current_response: String::new(),
            is_receiving_response: false,
            message_queue: std::collections::VecDeque::new(),
            status_message,
            model,
            show_help: false,
            chat_scroll_offset: 0,
            max_display_messages: 100,
            popup_state: PopupState::None,
        }
    }

    /// Add a new message to the conversation
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        // Keep only recent messages for display performance
        if self.messages.len() > self.max_display_messages {
            self.messages.drain(0..self.messages.len() - self.max_display_messages);
        }
        // Auto-scroll to bottom to show new message
        self.scroll_to_bottom();
    }

    /// Get visible messages for display (excluding system messages)
    pub fn visible_messages(&self) -> Vec<&ChatMessage> {
        self.messages
            .iter()
            .filter(|msg| msg.role != Role::System)
            .collect()
    }

    /// Start receiving a new response
    pub fn start_response(&mut self) {
        self.current_response.clear();
        self.is_receiving_response = true;
    }

    /// Append content to current response
    pub fn append_response(&mut self, content: &str) {
        self.current_response.push_str(content);
    }

    /// Finish receiving the current response
    pub fn finish_response(&mut self) -> Result<()> {
        if !self.current_response.is_empty() {
            let response = self.current_response.clone();
            self.add_message(ChatMessage {
                role: Role::Assistant,
                content: response,
                name: None,
                tool_calls: None,
            });
            
            if self.is_shell_mode {
                self.last_command = self.current_response.trim().to_string();
            }
        }
        
        self.current_response.clear();
        self.is_receiving_response = false;
        self.update_status_message(); // Update status after finishing response
        Ok(())
    }

    /// Clear input buffers
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.multiline_buffer.clear();
        self.input_mode = InputMode::Normal;
    }

    /// Get the current input text
    pub fn get_input_text(&self) -> String {
        match self.input_mode {
            InputMode::MultiLine => self.multiline_buffer.join("\n"),
            _ => self.input.clone(),
        }
    }

    /// Toggle help display
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Scroll chat history up (show older messages)
    pub fn scroll_up(&mut self) {
        let visible_count = self.visible_messages().len();
        // Increase offset to show older messages
        let max_scroll = visible_count.saturating_sub(1);
        if self.chat_scroll_offset < max_scroll {
            self.chat_scroll_offset += 1;
        }
    }

    /// Scroll chat history down (show newer messages)
    pub fn scroll_down(&mut self) {
        // Decrease offset to show newer messages
        if self.chat_scroll_offset > 0 {
            self.chat_scroll_offset -= 1;
        }
    }

    /// Reset scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.chat_scroll_offset = 0;
    }

    /// Show execution result popup
    pub fn show_execution_result(&mut self, command: String, output: String) {
        self.popup_state = PopupState::ExecutionResult { command, output };
    }

    /// Show command description popup
    pub fn show_description(&mut self, command: String, description: String) {
        self.popup_state = PopupState::Description { command, description };
    }

    /// Hide any popup
    pub fn hide_popup(&mut self) {
        self.popup_state = PopupState::None;
    }

    /// Check if any popup is shown
    pub fn is_popup_shown(&self) -> bool {
        self.popup_state != PopupState::None
    }

    /// Add message to queue if currently processing, otherwise return false to process immediately
    pub fn try_queue_message(&mut self, message: String) -> bool {
        if self.is_receiving_response {
            self.message_queue.push_back(message);
            self.update_status_message();
            true // Message was queued
        } else {
            false // Process immediately
        }
    }

    /// Get next message from queue
    pub fn dequeue_message(&mut self) -> Option<String> {
        let msg = self.message_queue.pop_front();
        self.update_status_message();
        msg
    }

    /// Update status message to show queue status
    fn update_status_message(&mut self) {
        let base_message = if self.is_shell_mode {
            if self.allow_interaction {
                "Shell REPL: e=execute, r=repeat, d=describe | Ctrl+C=quit, F1=help"
            } else {
                "Shell Mode | Ctrl+C=quit, F1=help"
            }
        } else {
            "Chat Mode | Ctrl+C=quit, F1=help"
        };

        self.status_message = if !self.message_queue.is_empty() {
            format!("{} | Queued: {}", base_message, self.message_queue.len())
        } else {
            base_message.to_string()
        };
    }

    /// Clear message queue
    pub fn clear_queue(&mut self) {
        self.message_queue.clear();
        self.update_status_message();
    }
}