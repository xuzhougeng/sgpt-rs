//! TUI module for enhanced REPL experience using Ratatui.

pub mod app;
pub mod events;
pub mod handler;
pub mod ui;

// Public exports available if needed in the future
// pub use app::App;
// pub use events::TuiEvent;
pub use handler::run_tui_repl;
