//! Chinese to pinyin, from the pack's `pinyin.txt`.

/// Loaded pinyin data. Filled in by Task 3.
pub struct Pinyin;

impl Pinyin {
    pub fn romanize(&self, run: &str) -> String {
        run.to_string()
    }
}
