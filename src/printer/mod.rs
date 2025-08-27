//! Printers: text and markdown (termimad).

use termimad::MadSkin;

pub struct MarkdownPrinter {
    pub skin: MadSkin,
}

impl Default for MarkdownPrinter {
    fn default() -> Self {
        Self { skin: MadSkin::default() }
    }
}

impl MarkdownPrinter {
    pub fn print(&self, text: &str) { self.skin.print_text(text); println!(); }
}
