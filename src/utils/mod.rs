//! Utilities (shell command execution, document processing, etc.).

use std::fs;
use std::path::Path;
use std::process::Command;
use anyhow::{bail, Result};

pub fn run_command(cmd: &str) {
    if cfg!(windows) {
        // Allow explicit override via SHELL_NAME
        let override_shell = std::env::var("SHELL_NAME").unwrap_or_default().to_ascii_lowercase();
        let prefer_ps = if override_shell.contains("powershell") {
            true
        } else if override_shell.contains("cmd") {
            false
        } else {
            // Fallback heuristic: if PSModulePath exists, prefer PowerShell; otherwise cmd
            !std::env::var("PSModulePath").unwrap_or_default().is_empty()
        };
        if prefer_ps {
            let _ = Command::new("powershell.exe")
                .args(["-NoLogo", "-NoProfile", "-Command", cmd])
                .status();
        } else {
            let _ = Command::new("cmd.exe").args(["/c", cmd]).status();
        }
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let _ = Command::new(shell).arg("-c").arg(cmd).status();
    }
}

/// Read multiple document files and return their combined content as string.
/// Currently supports .md, .txt, and other text-based files.
pub fn read_documents(file_paths: &[String]) -> Result<String> {
    let mut combined_content = String::new();
    
    for (i, file_path) in file_paths.iter().enumerate() {
        let content = read_single_document(file_path)?;
        
        if i > 0 {
            combined_content.push_str("\n\n");
        }
        
        combined_content.push_str(&format!("=== Document: {} ===\n", file_path));
        combined_content.push_str(&content);
    }
    
    Ok(combined_content)
}

/// Read single document file and return its content as string.
/// Currently supports .md, .txt, and other text-based files.
pub fn read_single_document(file_path: &str) -> Result<String> {
    let path = Path::new(file_path);
    
    // Check if file exists
    if !path.exists() {
        bail!("Document file '{}' does not exist", file_path);
    }
    
    // Check if it's a file (not directory)
    if !path.is_file() {
        bail!("'{}' is not a file", file_path);
    }
    
    // Get file extension and check if supported
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    match extension.as_str() {
        "md" | "txt" | "rst" | "log" | "" => {
            // Read text files directly
            fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file_path, e))
        }
        _ => {
            bail!("Unsupported file type: .{}\nCurrently supported: .md, .txt, .rst, .log, and files without extension", extension);
        }
    }
}

// removed legacy read_document wrapper; use read_single_document instead

/// Combine document content with user prompt.
/// Format: "Document content: [content]\n\nUser question: [prompt]"
pub fn combine_doc_and_prompt(doc_content: &str, user_prompt: &str) -> String {
    if user_prompt.trim().is_empty() {
        // If no user prompt, just return document content
        format!("Document content:\n{}", doc_content)
    } else {
        // Combine document and prompt
        format!("Document content:\n{}\n\nUser question: {}", doc_content, user_prompt)
    }
}
