use anyhow::Result;
use pdf_extract::extract_text;
use std::path::Path;

#[tokio::test]
async fn test_pdf_extract_demo1() -> Result<()> {
    let test_pdf_path = "tests/demo1.pdf";

    if !Path::new(test_pdf_path).exists() {
        println!("Warning: Test PDF file not found at {}", test_pdf_path);
        println!("Please ensure demo1.pdf exists in the tests directory");
        return Ok(());
    }

    let text = extract_text(test_pdf_path)?;

    assert!(!text.is_empty(), "Extracted text should not be empty");
    println!("Extracted text from demo1.pdf:");
    println!("=== START ===");
    println!("{}", text);
    println!("=== END ===");

    Ok(())
}

#[tokio::test]
async fn test_pdf_extract_basic() -> Result<()> {
    let test_pdf_path = "tests/fixtures/sample.pdf";

    if !Path::new(test_pdf_path).exists() {
        println!("Warning: Test PDF file not found at {}", test_pdf_path);
        println!("Skipping test - sample.pdf not available");
        return Ok(());
    }

    let text = extract_text(test_pdf_path)?;

    assert!(!text.is_empty(), "Extracted text should not be empty");
    println!("Extracted text: {}", text);

    Ok(())
}

#[tokio::test]
async fn test_pdf_extract_nonexistent_file() -> Result<()> {
    let nonexistent_path = "tests/nonexistent.pdf";

    let result = extract_text(nonexistent_path);
    assert!(result.is_err(), "Should fail on nonexistent file");
    println!(
        "Expected error for nonexistent file: {:?}",
        result.unwrap_err()
    );

    Ok(())
}

#[tokio::test]
async fn test_pdf_extract_invalid_file() -> Result<()> {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Create a temporary file with invalid PDF content
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(b"This is not a PDF file")?;

    let result = extract_text(temp_file.path());
    assert!(result.is_err(), "Should fail on invalid PDF data");
    println!("Expected error for invalid PDF: {:?}", result.unwrap_err());

    Ok(())
}

#[cfg(test)]
mod utility_tests {
    use super::*;

    #[tokio::test]
    async fn test_pdf_extract_library_info() -> Result<()> {
        println!("pdf-extract library test suite");
        println!("Testing with demo1.pdf file from tests directory");

        // Check if demo1.pdf exists and get its metadata
        let demo_path = "tests/demo1.pdf";
        if Path::new(demo_path).exists() {
            let metadata = std::fs::metadata(demo_path)?;
            println!("demo1.pdf file size: {} bytes", metadata.len());
            println!("demo1.pdf exists and is ready for testing");
        } else {
            println!("demo1.pdf not found in tests directory");
        }

        Ok(())
    }
}
