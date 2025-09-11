//! Interpreter process management (startup/IO/health).

use anyhow::Result;
use tokio::process::{Child, ChildStdin, ChildStdout};

pub mod python;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpreterType {
    Python,
    R,
}

#[allow(dead_code)]
pub struct ProcessHandle {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

#[allow(dead_code)]
pub async fn start(_ty: InterpreterType) -> Result<ProcessHandle> {
    // Placeholder: implemented in language-specific modules.
    anyhow::bail!("Interpreter start not implemented yet")
}
