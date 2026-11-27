//! Japanese to romaji, from the pack's IPADIC dictionary.

/// Loaded Japanese tokenizer. Filled in by Task 4.
pub struct Japanese;

impl Japanese {
    pub fn romanize(&self, run: &str) -> String {
        run.to_string()
    }
}
