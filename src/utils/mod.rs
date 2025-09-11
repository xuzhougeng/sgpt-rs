//! Utilities (shell command execution, document processing, etc.).
//!
//! This module provides various utility functions organized into submodules:
//! - `command`: Shell command execution across platforms
//! - `document`: Document reading and processing for multiple file formats
//! - `pdf`: PDF text extraction utilities

// Declare submodules
pub mod command;
pub mod document;
pub mod pdf;
pub mod unicode;

// Re-export commonly used functions for backward compatibility
pub use command::run_command;
pub use document::{combine_doc_and_prompt, read_documents};
// (intentionally not re-exporting unicode helpers to avoid unused-import warnings in clippy)
