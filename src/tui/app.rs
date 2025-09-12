//! TUI application state management.

use anyhow::Result;

use crate::llm::{ChatMessage, Role};
use crate::process::InterpreterType;

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
    Description {
        command: String,
        description: String,
    },
    /// Streaming description popup (shows loading state and streams content)
    StreamingDescription {
        command: String,
        current_description: String,
        is_loading: bool,
    },
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
    /// Cursor position in input (character index)
    pub input_cursor: usize,
    /// Current input mode
    pub input_mode: InputMode,
    /// Multi-line input buffer
    pub multiline_buffer: Vec<String>,
    /// Input history (user-entered lines)
    pub input_history: Vec<String>,
    /// Current history index when navigating (None = new line)
    pub history_index: Option<usize>,
    /// Whether we're in shell mode
    pub is_shell_mode: bool,
    /// Active interpreter (Python/R) if in analytics mode
    pub interpreter: Option<InterpreterType>,
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
    /// Stored collapsed paste content for potential expansion
    pub collapsed_paste_content: Option<String>,
    /// Pending paste mappings: (placeholder -> actual content)
    pub pending_pastes: Vec<(String, String)>,
    /// Timestamp of last Ctrl+C press for double Ctrl+C detection
    pub last_ctrl_c_time: Option<std::time::Instant>,
}

impl App {
    /// Create a new TUI application instance
    pub fn new(
        chat_id: String,
        messages: Vec<ChatMessage>,
        is_shell_mode: bool,
        allow_interaction: bool,
        model: String,
        interpreter: Option<InterpreterType>,
    ) -> Self {
        let status_message = if let Some(lang) = interpreter {
            match lang {
                InterpreterType::Python => "Python REPL: e=execute, r=repeat | ctrl+h help",
                InterpreterType::R => "R REPL: e=execute, r=repeat | ctrl+h help",
            }
        } else if is_shell_mode {
            if allow_interaction {
                "Shell REPL: e=execute, r=repeat, d=describe | ctrl+h help"
            } else {
                "Shell Mode | ctrl+h help"
            }
        } else {
            "Chat Mode | ctrl+h help"
        }
        .to_string();

        Self {
            chat_id,
            messages,
            input: String::new(),
            input_cursor: 0,
            input_mode: InputMode::Normal,
            multiline_buffer: Vec::new(),
            input_history: Vec::new(),
            history_index: None,
            is_shell_mode,
            interpreter,
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
            collapsed_paste_content: None,
            pending_pastes: Vec::new(),
            last_ctrl_c_time: None,
        }
    }

    /// Add a new message to the conversation
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        // Keep only recent messages for display performance
        if self.messages.len() > self.max_display_messages {
            self.messages
                .drain(0..self.messages.len() - self.max_display_messages);
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
            self.add_message(ChatMessage::new(Role::Assistant, response));

            if self.is_shell_mode || self.interpreter.is_some() {
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
        self.input_cursor = 0;
        self.multiline_buffer.clear();
        self.input_mode = InputMode::Normal;
        self.history_index = None;
    }

    /// Get the current input text
    pub fn get_input_text(&self) -> String {
        match self.input_mode {
            InputMode::MultiLine => {
                let mut lines = self.multiline_buffer.clone();
                lines.push(self.input.clone());
                lines.join("\n")
            }
            _ => self.input.clone(),
        }
    }

    /// Toggle help display
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Scroll chat history up (show older messages) - now line-based
    pub fn scroll_up(&mut self) {
        // Scroll up by one line at a time, but we need terminal dimensions
        // For now, increment by 1 and let the UI handle the actual calculation
        self.chat_scroll_offset += 1;
    }

    /// Scroll chat history down (show newer messages) - now line-based
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

    // ----- Input editing helpers -----
    pub fn move_cursor_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        let total_chars = self.input.chars().count();
        if self.input_cursor < total_chars {
            self.input_cursor += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.input_cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.input_cursor = self.input.chars().count();
    }

    pub fn move_cursor_word_left(&mut self) {
        let new_pos = prev_word_boundary(&self.input, self.input_cursor);
        self.input_cursor = new_pos;
    }

    pub fn move_cursor_word_right(&mut self) {
        let new_pos = next_word_boundary(&self.input, self.input_cursor);
        self.input_cursor = new_pos;
    }

    pub fn insert_char(&mut self, c: char) {
        let total_chars = self.input.chars().count();
        if self.input_cursor >= total_chars {
            self.input.push(c);
            self.input_cursor = self.input.chars().count();
        } else {
            let byte_idx =
                crate::utils::unicode::char_to_byte_index(&self.input, self.input_cursor);
            self.input.insert(byte_idx, c);
            self.input_cursor += 1;
        }
        // Any edit may invalidate pending placeholders not present anymore
        self.cleanup_pending_pastes();
    }

    pub fn backspace(&mut self) {
        // Try atomic placeholder removal at boundary
        if self.try_remove_placeholder_at_cursor(true) {
            self.cleanup_pending_pastes();
            return;
        }
        if self.input_cursor > 0 {
            let del_char_pos = self.input_cursor - 1;
            let byte_idx = crate::utils::unicode::char_to_byte_index(&self.input, del_char_pos);
            self.input.remove(byte_idx);
            self.input_cursor -= 1;
        } else if self.input_cursor == 0
            && self.input_mode == InputMode::MultiLine
            && !self.multiline_buffer.is_empty()
        {
            // At the beginning of current line in multiline mode, merge with previous line
            let previous_line = self.multiline_buffer.pop().unwrap();
            let current_line = self.input.clone();
            let prev_chars = previous_line.chars().count();
            self.input = previous_line + &current_line;
            // place cursor right after the previous line content
            self.input_cursor = prev_chars;

            // If multiline buffer is now empty, switch back to normal mode
            if self.multiline_buffer.is_empty() {
                self.input_mode = InputMode::Normal;
            }
        }
        self.cleanup_pending_pastes();
    }

    pub fn delete(&mut self) {
        // Try atomic placeholder removal at boundary
        if self.try_remove_placeholder_at_cursor(false) {
            self.cleanup_pending_pastes();
            return;
        }
        let total_chars = self.input.chars().count();
        if self.input_cursor < total_chars {
            let byte_idx =
                crate::utils::unicode::char_to_byte_index(&self.input, self.input_cursor);
            self.input.remove(byte_idx);
        }
        self.cleanup_pending_pastes();
    }

    pub fn delete_prev_word(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let start = prev_word_boundary(&self.input, self.input_cursor);
        if start < self.input_cursor {
            let start_b = crate::utils::unicode::char_to_byte_index(&self.input, start);
            let end_b = crate::utils::unicode::char_to_byte_index(&self.input, self.input_cursor);
            self.input.replace_range(start_b..end_b, "");
            self.input_cursor = start;
            self.cleanup_pending_pastes();
        }
    }

    pub fn delete_next_word(&mut self) {
        let end = next_word_boundary(&self.input, self.input_cursor);
        let cur_b = crate::utils::unicode::char_to_byte_index(&self.input, self.input_cursor);
        let end_b = crate::utils::unicode::char_to_byte_index(&self.input, end);
        if end_b > cur_b {
            self.input.replace_range(cur_b..end_b, "");
            self.cleanup_pending_pastes();
        }
    }

    pub fn kill_to_line_start(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let end_b = crate::utils::unicode::char_to_byte_index(&self.input, self.input_cursor);
        self.input.replace_range(0..end_b, "");
        self.input_cursor = 0;
        self.cleanup_pending_pastes();
    }

    pub fn kill_to_line_end(&mut self) {
        let cur_b = crate::utils::unicode::char_to_byte_index(&self.input, self.input_cursor);
        self.input.replace_range(cur_b..self.input.len(), "");
        self.cleanup_pending_pastes();
    }

    pub fn push_history(&mut self, line: String) {
        if !line.trim().is_empty() {
            if self.input_history.last().map(|s| s.as_str()) != Some(line.as_str()) {
                self.input_history.push(line);
            }
        }
        self.history_index = None;
    }

    pub fn history_prev(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        let next_index = match self.history_index {
            None => Some(self.input_history.len().saturating_sub(1)),
            Some(0) => Some(0),
            Some(i) => Some(i.saturating_sub(1)),
        };
        if let Some(i) = next_index {
            self.history_index = Some(i);
            self.input = self.input_history[i].clone();
            self.move_cursor_end();
        }
    }

    pub fn history_next(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        match self.history_index {
            None => {}
            Some(i) if i + 1 < self.input_history.len() => {
                let ni = i + 1;
                self.history_index = Some(ni);
                self.input = self.input_history[ni].clone();
                self.move_cursor_end();
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
                self.input_cursor = 0;
            }
        }
    }

    /// Show execution result popup
    pub fn show_execution_result(&mut self, command: String, output: String) {
        self.popup_state = PopupState::ExecutionResult { command, output };
    }

    /// Show command description popup
    pub fn show_description(&mut self, command: String, description: String) {
        self.popup_state = PopupState::Description {
            command,
            description,
        };
    }

    /// Start streaming description popup
    pub fn start_streaming_description(&mut self, command: String) {
        self.popup_state = PopupState::StreamingDescription {
            command,
            current_description: String::new(),
            is_loading: true,
        };
    }

    /// Append content to streaming description
    pub fn append_description_content(&mut self, content: &str) {
        if let PopupState::StreamingDescription {
            current_description,
            is_loading,
            ..
        } = &mut self.popup_state
        {
            current_description.push_str(content);
            *is_loading = false; // Mark as no longer loading once we start receiving content
        }
    }

    /// Finish streaming description
    pub fn finish_streaming_description(&mut self) {
        if let PopupState::StreamingDescription {
            command,
            current_description,
            ..
        } = &self.popup_state
        {
            let final_description = current_description.clone();
            let final_command = command.clone();
            self.popup_state = PopupState::Description {
                command: final_command,
                description: final_description,
            };
        }
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
        self.status_message = if let Some(lang) = self.interpreter {
            match lang {
                InterpreterType::Python => {
                    "Python REPL: e=execute, r=repeat | ctrl+h help".to_string()
                }
                InterpreterType::R => "R REPL: e=execute, r=repeat | ctrl+h help".to_string(),
            }
        } else if self.is_shell_mode {
            if self.allow_interaction {
                "Shell REPL: e=execute, r=repeat, d=describe | ctrl+h help".to_string()
            } else {
                "Shell Mode | ctrl+h help".to_string()
            }
        } else {
            "Chat Mode | ctrl+h help".to_string()
        };
    }

    /// Store collapsed paste content for potential expansion
    pub fn store_collapsed_paste_content(&mut self, content: String) {
        self.collapsed_paste_content = Some(content);
    }

    /// Check if current input contains a collapsed paste indicator and expand it if requested
    pub fn try_expand_collapsed_paste(&mut self) -> bool {
        if let Some(ref stored_content) = self.collapsed_paste_content.clone() {
            // Check if current input contains the collapsed indicator pattern
            if self.input.contains("[pasted content ") && self.input.contains(" chars]") {
                // Replace the collapsed indicator with the actual content
                let pattern_start = self.input.find("[pasted content ").unwrap();
                let pattern_end = self.input.find(" chars]").unwrap() + " chars]".len();

                let before = self.input[..pattern_start].to_string();
                let after = self.input[pattern_end..].to_string();

                let new_input = format!("{}{}{}", before, stored_content, after);
                let new_cursor = before.chars().count() + stored_content.chars().count();

                self.input = new_input;
                self.input_cursor = new_cursor;
                self.collapsed_paste_content = None;

                // If the expanded content has newlines, switch to multiline mode
                if self.input.contains('\n') {
                    let parts: Vec<String> =
                        self.input.split('\n').map(|s| s.to_string()).collect();
                    if parts.len() > 1 {
                        self.multiline_buffer = parts[..parts.len() - 1].to_vec();
                        self.input = parts.last().unwrap_or(&String::new()).clone();
                        self.input_cursor = self.input.chars().count();
                        self.input_mode = InputMode::MultiLine;
                    }
                }

                return true;
            }
        }
        false
    }

    /// Handle Ctrl+C press and detect double press for quit
    /// Returns true if should quit (double Ctrl+C), false otherwise
    pub fn handle_ctrl_c(&mut self) -> bool {
        const DOUBLE_CTRL_C_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

        let now = std::time::Instant::now();

        if let Some(last_time) = self.last_ctrl_c_time {
            if now.duration_since(last_time) <= DOUBLE_CTRL_C_TIMEOUT {
                // Double Ctrl+C detected - quit
                self.last_ctrl_c_time = None;
                return true;
            }
        }

        // Single Ctrl+C - clear input and record timestamp
        self.input.clear();
        self.input_cursor = 0;
        self.multiline_buffer.clear();
        self.input_mode = InputMode::Normal;
        self.history_index = None;
        self.last_ctrl_c_time = Some(now);

        false
    }

    /// Register a placeholder to actual pasted content mapping
    pub fn register_pending_paste(&mut self, placeholder: String, actual: String) {
        self.pending_pastes.push((placeholder, actual));
    }

    /// Expand placeholders to actual pasted content for submission, then clear mappings
    pub fn expand_placeholders_for_submit(&mut self) -> String {
        let mut text = self.get_input_text();
        if self.pending_pastes.is_empty() {
            return text;
        }

        // Replace each placeholder once, in order
        for (placeholder, actual) in &self.pending_pastes {
            if let Some(idx_chars) = find_substring_char_index(&text, placeholder) {
                let (before, after) = split_by_char_index(&text, idx_chars);
                let after_idx = placeholder.chars().count();
                let (_, tail) = split_by_char_index(after, after_idx);
                let mut combined = before;
                combined.push_str(actual);
                combined.push_str(tail);
                text = combined;
            }
        }
        // Clear mappings after expansion
        self.pending_pastes.clear();
        text
    }

    /// Try to expand all placeholders inline in the current composer input.
    /// Returns true if any expansion occurred.
    pub fn expand_placeholders_inline(&mut self) -> bool {
        if self.pending_pastes.is_empty() {
            return false;
        }
        let mut changed = false;
        // Work only on the current line for inline expansion to avoid heavy state changes
        let mut line = self.input.clone();
        for (placeholder, actual) in &self.pending_pastes {
            if let Some(start_chars) = find_substring_char_index(&line, placeholder) {
                let (before, after) = split_by_char_index(&line, start_chars);
                let after_idx = placeholder.chars().count();
                let (_, tail) = split_by_char_index(after, after_idx);
                let mut combined = before;
                combined.push_str(actual);
                combined.push_str(tail);
                line = combined;
                // Adjust cursor if needed
                let new_cursor = if self.input_cursor >= start_chars {
                    start_chars + actual.chars().count()
                } else {
                    self.input_cursor
                };
                self.input_cursor = new_cursor;
                changed = true;
            }
        }
        if changed {
            self.input = line;
            // If now multi-line content present, switch mode accordingly
            if self.input.contains('\n') {
                let parts: Vec<String> = self.input.split('\n').map(|s| s.to_string()).collect();
                if parts.len() > 1 {
                    self.multiline_buffer = parts[..parts.len() - 1].to_vec();
                    self.input = parts.last().cloned().unwrap_or_default();
                    self.input_cursor = self.input.chars().count();
                    self.input_mode = InputMode::MultiLine;
                }
            }
            // After expansion, drop placeholders that disappeared
            self.cleanup_pending_pastes();
        }
        changed
    }

    /// Attempt to remove a whole placeholder at the cursor boundary for Backspace/Delete.
    /// Returns true if a placeholder was removed.
    pub fn try_remove_placeholder_at_cursor(&mut self, is_backspace: bool) -> bool {
        // Only operate on the current input line
        let line = &self.input;
        if line.is_empty() || self.pending_pastes.is_empty() {
            return false;
        }
        // Scan for any placeholder occurrence enclosing the boundary
        // Boundary is to the left for backspace, to the right for delete
        let boundary = self.input_cursor;
        for (idx, (placeholder, _actual)) in self.pending_pastes.iter().enumerate() {
            if let Some(start_chars) = find_substring_char_index(line, placeholder) {
                let end_chars = start_chars + placeholder.chars().count();
                let hit = if is_backspace {
                    boundary == end_chars
                } else {
                    boundary == start_chars
                };
                if hit {
                    // Remove substring from input
                    let (before, after) = split_by_char_index(line, start_chars);
                    let (_, tail) = split_by_char_index(after, placeholder.chars().count());
                    self.input = format!("{}{}", before, tail);
                    self.input_cursor = start_chars;
                    // Remove mapping (only one occurrence)
                    self.pending_pastes.remove(idx);
                    return true;
                }
            }
        }
        false
    }

    /// Remove any pending paste entries whose placeholders are no longer present in the composer.
    pub fn cleanup_pending_pastes(&mut self) {
        if self.pending_pastes.is_empty() {
            return;
        }
        let full = self.get_input_text();
        self.pending_pastes.retain(|(ph, _)| full.contains(ph));
    }
}

/// Find the character index of a substring in a string (first occurrence).
fn find_substring_char_index(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .find(needle)
        .map(|byte_idx| haystack[..byte_idx].chars().count())
}

/// Split a string by a character index, returning (left, right) string slices as owned Strings.
fn split_by_char_index<'a>(s: &'a str, char_idx: usize) -> (String, &'a str) {
    if char_idx == 0 {
        return (String::new(), s);
    }
    let byte_idx = crate::utils::unicode::char_to_byte_index(s, char_idx);
    (s[..byte_idx].to_string(), &s[byte_idx..])
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn prev_word_boundary(s: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let chars: Vec<char> = s.chars().collect();
    let mut i = cursor.min(chars.len());
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && chars[i].is_whitespace() {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let cls = is_word_char(chars[i]);
    while i > 0 {
        let p = chars[i - 1];
        if p.is_whitespace() {
            break;
        }
        if is_word_char(p) != cls {
            break;
        }
        i -= 1;
    }
    i
}

fn next_word_boundary(s: &str, cursor: usize) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut i = cursor.min(n);
    if i >= n {
        return n;
    }
    while i < n && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= n {
        return n;
    }
    let cls = is_word_char(chars[i]);
    while i < n {
        let c = chars[i];
        if c.is_whitespace() {
            break;
        }
        if is_word_char(c) != cls {
            break;
        }
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ChatMessage, Role};

    fn new_empty_app() -> App {
        App::new(
            "test".to_string(),
            vec![ChatMessage::new(Role::System, "test".to_string())],
            false,
            false,
            "gpt-4o".to_string(),
            None,
        )
    }

    #[test]
    fn cjk_insert_backspace_delete_are_safe() {
        let mut app = new_empty_app();

        app.insert_char('徐');
        app.insert_char('洲');
        app.insert_char('更');
        assert_eq!(app.input, "徐洲更");
        assert_eq!(app.input_cursor, 3);

        // Move left and backspace one char (should remove '个')
        app.move_cursor_left(); // cursor at 2
        app.backspace(); // remove at 1
        assert_eq!(app.input, "徐更");
        assert_eq!(app.input_cursor, 1);

        // Delete at cursor (should remove '发')
        app.delete();
        assert_eq!(app.input, "徐");
        assert_eq!(app.input_cursor, 1);

        // Ensure no panic and cursor end works
        app.move_cursor_end();
        assert_eq!(app.input_cursor, app.input.chars().count());
    }

    #[test]
    fn placeholder_submit_and_cleanup() {
        let mut app = new_empty_app();
        // Simulate inserting a placeholder and mapping
        let placeholder = "[Pasted Content 1234 chars]".to_string();
        app.input = format!("foo {} bar", placeholder);
        app.input_cursor = app.input.chars().count();
        app.register_pending_paste(placeholder.clone(), "X".repeat(5));

        // Submit should expand and clear mapping
        let expanded = app.expand_placeholders_for_submit();
        assert_eq!(expanded, format!("foo {} bar", "X".repeat(5)));
        assert!(app.pending_pastes.is_empty());

        // Cleanup should be a no-op
        app.cleanup_pending_pastes();
        assert!(app.pending_pastes.is_empty());
    }
}
