//! Custom event types for TUI application.

use crate::execution::ExecutionResult;
use crate::llm::StreamEvent;
use crate::process::InterpreterType;
use crossterm::event::{KeyEvent, MouseEvent};

/// Events that can occur in the TUI application
#[derive(Debug)]
pub enum TuiEvent {
    /// User keyboard input
    Key(KeyEvent),
    /// LLM streaming response event
    LlmStream(StreamEvent),
    /// Mouse event (for scrolling)
    Mouse(MouseEvent),
    /// User input text (processed from keyboard events)
    UserInput(String),
    /// Request to quit the application
    Quit,
    /// Request to execute a shell command
    ExecuteCommand(String),
    /// Command execution completed with result
    ExecutionResult { command: String, output: String },
    /// Request to describe a shell command
    DescribeCommand(String),
    /// Process next message from queue
    ProcessNextMessage,
    /// Session state change
    SessionUpdate,

    // --- Analytics/Interpreter mode events ---
    /// Execute provided code in the selected interpreter
    ExecuteCode {
        language: InterpreterType,
        code: String,
    },
    /// Code execution result returned from interpreter
    CodeExecutionResult(ExecutionResult),
    /// Switch current interpreter (Python/R)
    SwitchInterpreter(InterpreterType),
    /// Show variables summary from interpreter session
    ShowVariables,
    /// Variables snapshot string to display
    VariablesSnapshot(String),
    /// Clear current interpreter session (restart)
    ClearSession,
}
