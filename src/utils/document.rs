//! Document processing utilities for reading and combining various file formats.

use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

/// Read multiple document files and return their combined content as string.
///
/// Supports various file formats including .md, .txt, .rst, .log, .pdf, and files without extension.
/// Each document is prefixed with a header indicating the file path.
///
/// # Arguments
///
/// * `file_paths` - A slice of file path strings
///
/// # Returns
///
/// * `Result<String>` - Combined content of all documents, or error if any file fails to read
///
/// # Examples
///
/// ```rust
/// use crate::utils::document::read_documents;
///
/// let files = vec!["doc1.md".to_string(), "doc2.txt".to_string()];
/// let content = read_documents(&files)?;
/// ```
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
///
/// Supports multiple file formats:
/// - Text files: .md, .txt, .rst, .log, and files without extension
/// - PDF files: .pdf (text extraction)
///
/// # Arguments
///
/// * `file_path` - Path to the document file
///
/// # Returns
///
/// * `Result<String>` - File content as string, or error if file doesn't exist or unsupported format
///
/// # Examples
///
/// ```rust
/// use crate::utils::document::read_single_document;
///
/// let content = read_single_document("document.pdf")?;
/// let text_content = read_single_document("notes.txt")?;
/// ```
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
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "md" | "txt" | "rst" | "log" | "" => {
            // Read text files directly
            fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file_path, e))
        }
        "pdf" => {
            // Use PDF module for PDF files
            super::pdf::extract_pdf_text(file_path)
        }
        _ => {
            bail!("Unsupported file type: .{}\nCurrently supported: .md, .txt, .rst, .log, .pdf, and files without extension", extension);
        }
    }
}

/// Combine document content with user prompt.
///
/// Creates a formatted string combining document content and user question.
/// If user prompt is empty, returns only the document content.
///
/// # Arguments
///
/// * `doc_content` - The content of the document(s)
/// * `user_prompt` - The user's question or prompt
///
/// # Returns
///
/// * `String` - Formatted combination of document and prompt
///
/// # Format
///
/// ```text
/// Document content:
/// [doc_content]
///
/// User question: [user_prompt]
/// ```
///
/// # Examples
///
/// ```rust
/// use crate::utils::document::combine_doc_and_prompt;
///
/// let combined = combine_doc_and_prompt("File content here", "Summarize this");
/// ```
pub fn combine_doc_and_prompt(doc_content: &str, user_prompt: &str) -> String {
    if user_prompt.trim().is_empty() {
        // If no user prompt, just return document content
        format!("Document content:\n{}", doc_content)
    } else {
        // Combine document and prompt
        format!(
            "Document content:\n{}\n\nUser question: {}",
            doc_content, user_prompt
        )
    }
}
