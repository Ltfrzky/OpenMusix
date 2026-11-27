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

pub mod ja;
pub mod ko;
pub mod script;
pub mod zh;

use std::sync::RwLock;

/// Romanized lines (parallel to the input) plus the packs that were needed but missing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RomanOutput {
    pub lines: Vec<Option<String>>,
    pub missing: Vec<RomanLang>,
}

/// Holds the loaded packs. Korean is built in.
pub struct Romanizer {
    zh: RwLock<Option<zh::Pinyin>>,
    ja: RwLock<Option<ja::Japanese>>,
}

impl Default for Romanizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Romanizer {
    pub fn new() -> Self {
        Self { zh: RwLock::new(None), ja: RwLock::new(None) }
    }

    pub fn set_zh(&self, p: Option<zh::Pinyin>) {
        *self.zh.write().expect("zh lock") = p;
    }

    pub fn set_ja(&self, j: Option<ja::Japanese>) {
        *self.ja.write().expect("ja lock") = j;
    }

    pub fn has(&self, l: RomanLang) -> bool {
        match l {
            RomanLang::Ko => true,
            RomanLang::Zh => self.zh.read().expect("zh lock").is_some(),
            RomanLang::Ja => self.ja.read().expect("ja lock").is_some(),
        }
    }

    /// Romanize each line. Han reads as Japanese when the song has kana, otherwise as Chinese.
    /// Runs whose pack is missing stay in the original script and are listed in `missing`.
    pub fn romanize(&self, lines: &[&str]) -> RomanOutput {
        use script::Script;
        let japanese = script::song_has_kana(lines);
        let zh = self.zh.read().expect("zh lock");
        let ja = self.ja.read().expect("ja lock");
        let mut missing = Vec::new();
        let mut out = Vec::with_capacity(lines.len());
        for line in lines {
            // Under a Japanese reading, Han joins Kana so kanji and kana are tokenized together.
            let mut groups: Vec<(Script, String)> = Vec::new();
            for (s, text) in script::runs(line) {
                let s = if japanese && s == Script::Han { Script::Kana } else { s };
                match groups.last_mut() {
                    Some((p, t)) if *p == s => t.push_str(text),
                    _ => groups.push((s, text.to_string())),
                }
            }
            let mut pieces: Vec<(bool, String)> = Vec::new();
            let mut any = false;
            for (s, text) in groups {
                let roman = match s {
                    Script::Other => None,
                    Script::Hangul => Some(ko::romanize(&text)),
                    Script::Kana => match ja.as_ref() {
                        Some(j) => Some(j.romanize(&text)),
                        None => {
                            missing.push(RomanLang::Ja);
                            None
                        }
                    },
                    Script::Han => match zh.as_ref() {
                        Some(p) => Some(p.romanize(&text)),
                        None => {
                            missing.push(RomanLang::Zh);
                            None
                        }
                    },
                };
                match roman {
                    Some(r) => {
                        any = true;
                        pieces.push((true, r));
                    }
                    None => pieces.push((false, text)),
                }
            }
            out.push(any.then(|| join(&pieces)));
        }
        missing.sort();
        missing.dedup();
        RomanOutput { lines: out, missing }
    }
}

/// Join pieces, putting a space between a romanized piece and a neighbouring letter or digit.
fn join(pieces: &[(bool, String)]) -> String {
    let mut s = String::new();
    let mut prev_roman = false;
    for (roman, text) in pieces {
        let touches = s.chars().last().is_some_and(char::is_alphanumeric) && text.chars().next().is_some_and(char::is_alphanumeric);
        if touches && (prev_roman || *roman) {
            s.push(' ');
        }
        s.push_str(text);
        prev_roman = *roman;
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn non_cjk_lines_are_none() {
        let r = Romanizer::new().romanize(&["Hello world", "", "♪", "🎵!!", "ＡＢＣ"]);
        assert_eq!(r.lines, vec![None, None, None, None, None]);
        assert!(r.missing.is_empty());
    }
    #[test] fn mixed_latin_and_hangul() {
        assert_eq!(Romanizer::new().romanize(&["I love you 사랑해"]).lines, vec![Some("I love you saranghae".into())]);
    }
    #[test] fn missing_zh_keeps_hangul() {
        let r = Romanizer::new().romanize(&["사랑 愛"]);
        assert_eq!(r.lines, vec![Some("sarang 愛".into())]);
        assert_eq!(r.missing, vec![RomanLang::Zh]);
    }
    #[test] fn kana_song_routes_han_to_ja() {
        let r = Romanizer::new().romanize(&["愛してる", "空"]);
        assert_eq!(r.missing, vec![RomanLang::Ja]);
        assert_eq!(r.lines, vec![None, None]);
    }
    #[test] fn korean_punctuation_spacing() {
        assert_eq!(Romanizer::new().romanize(&["(사랑해!)"]).lines, vec![Some("(saranghae!)".into())]);
    }
}
