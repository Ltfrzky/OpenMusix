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
pub mod packs;
pub mod script;
pub mod zh;

use std::sync::RwLock;

/// Failure loading a pack's data.
#[derive(Debug, thiserror::Error)]
pub enum RomanError {
    #[error("could not read pack data: {0}")]
    Io(#[from] std::io::Error),
    #[error("pack data is invalid: {0}")]
    BadData(String),
}

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
                    // Bare jamo like ㅋㅋㅋ have no syllables to romanize; leave them as they are.
                    Script::Hangul => Some(ko::romanize(&text)).filter(|r| *r != text),
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

/// The lines to romanize for the current snapshot, or `None` when romanization doesn't apply.
pub fn roman_input(snap: &crate::hub::Snapshot) -> Option<Vec<String>> {
    use crate::lyrics::LyricsStatus;
    use crate::model::LyricsResult;
    if !snap.romanize.enabled {
        return None;
    }
    match &snap.lyrics {
        LyricsStatus::Ready(LyricsResult::Synced(lines)) => Some(lines.iter().map(|l| l.text.clone()).collect()),
        LyricsStatus::Ready(LyricsResult::PlainOnly(text)) if snap.show_plain_lyrics => Some(text.lines().map(str::to_string).collect()),
        _ => None,
    }
}

/// Publish `out` only if the snapshot still shows the same track and lines it was computed for.
pub fn apply_roman(snap: &mut crate::hub::Snapshot, key: &Option<String>, input: &[String], out: RomanOutput) -> bool {
    if snap.track_key != *key || roman_input(snap).as_deref() != Some(input) {
        return false;
    }
    snap.roman = Some(out.lines);
    snap.roman_missing = out.missing;
    true
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;
    use crate::hub::Snapshot;
    use crate::lyrics::LyricsStatus;
    use crate::model::{LyricLine, LyricsResult};
    use crate::settings::{Romanize, Settings};

    fn on() -> Romanize { Romanize { enabled: true, ..Default::default() } }
    fn synced(texts: &[&str]) -> LyricsStatus {
        LyricsStatus::Ready(LyricsResult::Synced(texts.iter().enumerate().map(|(i, t)| LyricLine { time_ms: i as u64 * 1000, text: t.to_string() }).collect()))
    }
    fn out() -> RomanOutput { RomanOutput { lines: vec![Some("a1".into()), None], missing: vec![RomanLang::Ja] } }

    #[test] fn roman_input_rules() {
        let mut s = Snapshot { lyrics: synced(&["a", "b"]), ..Default::default() };
        assert_eq!(roman_input(&s), None, "disabled");
        s.romanize = on();
        assert_eq!(roman_input(&s), Some(vec!["a".to_string(), "b".to_string()]));
        s.lyrics = LyricsStatus::Loading;
        assert_eq!(roman_input(&s), None, "loading");
        s.lyrics = LyricsStatus::Ready(LyricsResult::PlainOnly("x\ny".into()));
        assert_eq!(roman_input(&s), None, "plain hidden");
        s.show_plain_lyrics = true;
        assert_eq!(roman_input(&s), Some(vec!["x".to_string(), "y".to_string()]));
        s.lyrics = LyricsStatus::Ready(LyricsResult::Instrumental);
        assert_eq!(roman_input(&s), None);
    }
    #[test] fn apply_publishes_when_current() {
        let mut s = Snapshot { lyrics: synced(&["a", "b"]), romanize: on(), track_key: Some("a|b".into()), ..Default::default() };
        let input = roman_input(&s).unwrap();
        assert!(apply_roman(&mut s, &Some("a|b".into()), &input, out()));
        assert_eq!(s.roman, Some(out().lines));
        assert_eq!(s.roman_missing, out().missing);
    }
    #[test] fn stale_roman_dropped() {
        let mut s = Snapshot { lyrics: synced(&["a", "b"]), romanize: on(), track_key: Some("a|b".into()), ..Default::default() };
        let input = roman_input(&s).unwrap();
        s.track_key = Some("c|d".into());
        assert!(!apply_roman(&mut s, &Some("a|b".into()), &input, out()));
        assert_eq!(s.roman, None);
    }
    #[test] fn stale_lines_dropped() {
        let mut s = Snapshot { lyrics: synced(&["a", "b"]), romanize: on(), track_key: Some("a|b".into()), ..Default::default() };
        let input = roman_input(&s).unwrap();
        s.lyrics = synced(&["other"]);
        assert!(!apply_roman(&mut s, &Some("a|b".into()), &input, out()));
        assert_eq!(s.roman, None);
    }
    #[test] fn disabled_clears_and_blocks_late_result() {
        let mut s = Snapshot { lyrics: synced(&["a", "b"]), romanize: on(), track_key: Some("a|b".into()), ..Default::default() };
        let input = roman_input(&s).unwrap();
        crate::commands::project_settings(&Settings::default(), &mut s);
        assert!(!apply_roman(&mut s, &Some("a|b".into()), &input, out()));
        assert_eq!(s.roman, None);
    }
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
    #[test] fn jamo_only_line_is_none() {
        let r = Romanizer::new().romanize(&["ㅋㅋㅋ", "사랑 ㅠㅠ"]);
        assert_eq!(r.lines, vec![None, Some("sarang ㅠㅠ".into())]);
    }
    #[test] fn korean_punctuation_spacing() {
        assert_eq!(Romanizer::new().romanize(&["(사랑해!)"]).lines, vec![Some("(saranghae!)".into())]);
    }
}
