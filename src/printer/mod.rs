//! Printers: text and markdown (termimad).

use owo_colors::OwoColorize;
use termimad::MadSkin;

pub struct TextPrinter {
    pub color: Option<&'static str>,
}

impl TextPrinter {
    pub fn print(&self, text: &str) {
        if let Some(c) = self.color {
            match c {
                "green" => println!("{}", text.green()),
                "cyan" => println!("{}", text.cyan()),
                "magenta" => println!("{}", text.magenta()),
                "yellow" => println!("{}", text.yellow()),
                _ => println!("{}", text),
            }
        } else {
            println!("{}", text);
        }
    }
}

pub struct MarkdownPrinter {
    pub skin: MadSkin,
    pub width: usize,
}

impl Default for MarkdownPrinter {
    fn default() -> Self {
        Self { skin: MadSkin::default(), width: 100 }
    }
}

impl MarkdownPrinter {
    pub fn print(&self, text: &str) { self.skin.print_text(text); println!(); }
}
