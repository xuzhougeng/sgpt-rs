//! PDF text extraction utilities.
//!
//! Features:
//! - Prefer external `pdftotext` (quiet, UTF-8, layout) when available with stderr suppressed.
//! - Fallback to `pdf-extract` and temporarily suppress stderr on Unix to avoid glyph warnings.
//! - Normalize output into page/line formatted text:
//!   --page N----\n
//!   L1: ...\n
//!   L2: ...

use anyhow::Result;
use pdf_extract::extract_text;
use std::process::{Command, Stdio};

/// Extract text content from a PDF file.
///
/// Uses the pdf-extract crate to parse PDF files and extract readable text content.
/// This function handles various PDF encodings and font mappings, though some
/// complex PDFs might produce warnings about missing glyphs or characters.
///
/// # Arguments
///
/// * `file_path` - Path to the PDF file
///
/// # Returns
///
/// * `Result<String>` - Extracted text content, or error if extraction fails
///
/// # Examples
///
/// ```rust
/// use crate::utils::pdf::extract_pdf_text;
///
/// let content = extract_pdf_text("document.pdf")?;
/// println!("PDF contains {} characters", content.len());
/// ```
///
/// # Notes
///
/// - The extraction process may produce stderr warnings about font encoding issues
/// - These warnings are normal for complex PDFs and don't affect the extraction
/// - Empty or corrupted PDFs will return an error
pub fn extract_pdf_text(file_path: &str) -> Result<String> {
    // Strategy A: Use external `pdftotext` if available.
    // - `-q` quiet mode suppresses stderr warnings from the tool.
    // - `-enc UTF-8` enforces UTF-8 output.
    // - `-layout` keeps visual order reasonably.
    // - Output to stdout ("-") so we can capture it and format.
    if let Ok(output) = Command::new("pdftotext")
        .arg("-q")
        .arg("-enc")
        .arg("UTF-8")
        .arg("-layout")
        .arg(file_path)
        .arg("-")
        .stderr(Stdio::null())
        .output()
    {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout).to_string();
            return Ok(format_pages_and_lines(&raw));
        }
    }

    // Strategy B: Fallback to pdf-extract. Suppress stderr on Unix to avoid noisy glyph warnings.
    let raw = with_stderr_suppressed_unix(|| extract_text(file_path))
        .unwrap_or_else(|_| extract_text(file_path))
        .map_err(|e| anyhow::anyhow!("Failed to extract text from PDF '{}': {}", file_path, e))?;

    Ok(format_pages_and_lines(&raw))
}

/// Format raw PDF text into page/line sections.
///
/// Page boundaries are detected via form feed (\x0C) if present; otherwise the
/// entire document is treated as a single page.
fn format_pages_and_lines(raw: &str) -> String {
    let pages: Vec<&str> = if raw.contains('\u{000C}') {
        // form feed
        raw.split('\u{000C}').collect()
    } else {
        vec![raw]
    };

    let mut out = String::new();
    for (pi, page) in pages.iter().enumerate() {
        if pi > 0 {
            out.push('\n');
        }
        out.push_str(&format!("--page {}----\n", pi + 1));
        for (li, line) in page.lines().enumerate() {
            // Preserve leading spaces; trim only trailing newlines/spaces
            let line = line.trim_end_matches(['\r', '\n']);
            out.push_str(&format!("L{}: {}\n", li + 1, line));
        }
    }
    out
}

#[cfg(unix)]
fn with_stderr_suppressed_unix<F, T>(f: F) -> std::io::Result<T>
where
    F: FnOnce() -> T,
{
    use std::fs::OpenOptions;
    use std::io;
    use std::os::unix::io::AsRawFd;

    extern "C" {
        fn dup(fd: i32) -> i32;
        fn dup2(oldfd: i32, newfd: i32) -> i32;
        fn close(fd: i32) -> i32;
    }

    // Open /dev/null for writing
    let null = OpenOptions::new().write(true).open("/dev/null")?;
    let null_fd = null.as_raw_fd();

    unsafe {
        let stderr_fd = 2; // POSIX standard fd for stderr
        let saved = dup(stderr_fd);
        if saved == -1 {
            return Err(io::Error::last_os_error());
        }

        if dup2(null_fd, stderr_fd) == -1 {
            let _ = close(saved);
            return Err(io::Error::last_os_error());
        }

        // Run the function while stderr is redirected to /dev/null
        let result = f();

        // Restore stderr
        let _ = dup2(saved, stderr_fd);
        let _ = close(saved);

        Ok(result)
    }
}

#[cfg(not(unix))]
fn with_stderr_suppressed_unix<F, T>(f: F) -> std::io::Result<T>
where
    F: FnOnce() -> T,
{
    Ok(f())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Local helper used only by tests.
    fn is_pdf_file(file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".pdf")
    }

    #[test]
    fn test_is_pdf_file() {
        assert!(is_pdf_file("test.pdf"));
        assert!(is_pdf_file("Test.PDF"));
        assert!(is_pdf_file("document.Pdf"));
        assert!(!is_pdf_file("document.txt"));
        assert!(!is_pdf_file("document"));
        assert!(!is_pdf_file("document.pdf.txt"));
    }

    #[test]
    fn test_extract_nonexistent_pdf() {
        let result = extract_pdf_text("nonexistent.pdf");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_pages_and_lines_single_page() {
        let raw = "Title\nHello world\n";
        let formatted = super::format_pages_and_lines(raw);
        let expected_start = "--page 1----\nL1: Title\nL2: Hello world\n";
        assert!(formatted.starts_with(expected_start), "Got: {}", formatted);
    }

    #[test]
    fn test_format_pages_and_lines_multi_page() {
        let raw = "A\nB\n\u{000C}C\nD\n"; // page break between B and C
        let formatted = super::format_pages_and_lines(raw);
        let want = "--page 1----\nL1: A\nL2: B\n\n--page 2----\nL1: C\nL2: D\n";
        assert_eq!(formatted, want);
    }
}
