//! Execution engine: protocol and result types.

use std::collections::HashMap;

pub mod python;

#[derive(Debug, Clone, Default)]
pub struct ExecutionResult {
    pub success: bool,
    pub output: String,
    pub errors: Vec<String>,
    pub variables: HashMap<String, String>,
    #[expect(dead_code)]
    pub plots: Vec<String>,
}
