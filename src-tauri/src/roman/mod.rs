//! Romanization of Chinese, Japanese and Korean lyric lines (pinyin, romaji, Revised Romanization).
use serde::{Deserialize, Serialize};

/// A language the romanizer can handle. JSON: `"zh" | "ja" | "ko"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RomanLang {
    Zh,
    Ja,
    Ko,
}
