use anyhow::Result;
use pdf_extract::extract_text;
use std::path::Path;

#[test]
fn test_demo_pdf_simple() -> Result<()> {
    let test_pdf_path = "tests/demo1.pdf";

    if !Path::new(test_pdf_path).exists() {
        println!("demo1.pdf not found in tests directory");
        return Ok(());
    }

    // Suppress stderr for cleaner output
    let text = extract_text(test_pdf_path)?;

    // Display basic info about extracted text
    println!("✅ PDF text extraction successful!");
    println!("📄 File: {}", test_pdf_path);
    println!("📊 Text length: {} characters", text.len());
    println!("📝 Lines: {}", text.lines().count());

    // Show first few lines of extracted text
    let lines: Vec<&str> = text.lines().take(10).collect();
    println!("\n📖 First 10 lines of extracted text:");
    println!("=========================================");
    for (i, line) in lines.iter().enumerate() {
        println!("{:2}: {}", i + 1, line.trim());
    }

    // Show some statistics
    let word_count = text.split_whitespace().count();
    println!("=========================================");
    println!("📈 Statistics:");
    println!("   - Characters: {}", text.len());
    println!("   - Words: {}", word_count);
    println!("   - Lines: {}", text.lines().count());

    assert!(!text.is_empty(), "Extracted text should not be empty");

    Ok(())
}
