//! Unicode-safe helpers for working with UTF-8 strings.

/// Convert a character index (0-based) to a byte index in the given string.
/// If `n` exceeds the number of characters, returns `s.len()`.
pub fn char_to_byte_index(s: &str, n: usize) -> usize {
    match s.char_indices().nth(n) {
        Some((i, _)) => i,
        None => s.len(),
    }
}
