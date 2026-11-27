//! Japanese to romaji: lindera with the pack's IPADIC dictionary, then `wana_kana`.
use super::RomanError;
use std::borrow::Cow;
use std::path::Path;
use wana_kana::{ConvertJapanese, IsJapaneseStr};

/// Loaded Japanese tokenizer.
pub struct Japanese {
    segmenter: lindera::segmenter::Segmenter,
}

/// One token: its text, its katakana reading if the dictionary has one, and whether it is a particle.
pub struct Tok<'a> {
    pub surface: &'a str,
    pub reading: Option<&'a str>,
    pub particle: bool,
}

/// Turn tokens into space-separated romaji. Particles は/へ/を read `wa`/`e`/`o`.
pub fn toks_to_romaji(toks: &[Tok]) -> String {
    let mut out = String::new();
    for t in toks {
        let piece = match (t.particle, t.surface) {
            (true, "は") => "wa".to_string(),
            (true, "へ") => "e".to_string(),
            (true, "を") => "o".to_string(),
            _ => {
                let src = t.reading.unwrap_or(t.surface);
                if src.is_kana() { src.to_romaji() } else { t.surface.to_string() }
            }
        };
        // Punctuation attaches to the previous word; words are space-separated.
        if !out.is_empty() && piece.chars().next().is_some_and(char::is_alphanumeric) {
            out.push(' ');
        }
        out.push_str(&piece);
    }
    out
}

/// Load the IPADIC dictionary from `dir`.
pub fn load(dir: &Path) -> Result<Japanese, RomanError> {
    if !dir.is_dir() {
        return Err(RomanError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "dictionary folder missing")));
    }
    let path = dir.to_str().ok_or_else(|| RomanError::BadData("dictionary path is not UTF-8".into()))?;
    // Memory-mapped: the pack manager unloads the dictionary before deleting or replacing its files.
    let dict = lindera::dictionary::load_dictionary_with_options(path, true).map_err(|e| RomanError::BadData(e.to_string()))?;
    let segmenter = lindera::segmenter::Segmenter::new(lindera::mode::Mode::Normal, dict, None);
    Ok(Japanese { segmenter })
}

impl Japanese {
    pub fn romanize(&self, run: &str) -> String {
        let Ok(mut tokens) = self.segmenter.segment(Cow::Borrowed(run)) else { return run.to_string() };
        let owned: Vec<(String, Option<String>, bool)> = tokens
            .iter_mut()
            .map(|t| {
                let surface = t.surface.to_string();
                let d = t.details();
                let reading = d.get(7).filter(|r| **r != "*").map(|r| r.to_string());
                let particle = d.first() == Some(&"助詞");
                (surface, reading, particle)
            })
            .collect();
        let toks: Vec<Tok> = owned.iter().map(|(s, r, p)| Tok { surface: s, reading: r.as_deref(), particle: *p }).collect();
        toks_to_romaji(&toks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t<'a>(s: &'a str, r: Option<&'a str>, p: bool) -> Tok<'a> { Tok { surface: s, reading: r, particle: p } }
    #[test] fn particles() {
        assert_eq!(toks_to_romaji(&[t("私", Some("ワタシ"), false), t("は", Some("ハ"), true), t("東京", Some("トウキョウ"), false), t("へ", Some("ヘ"), true)]), "watashi wa toukyou e");
    }
    #[test] fn wo_particle() { assert_eq!(toks_to_romaji(&[t("君", Some("キミ"), false), t("を", Some("ヲ"), true)]), "kimi o"); }
    #[test] fn sokuon_and_long_vowel() { assert_eq!(toks_to_romaji(&[t("ずっと", Some("ズット"), false), t("ラーメン", Some("ラーメン"), false)]), "zutto raamen"); }
    #[test] fn unknown_kanji_passes() { assert_eq!(toks_to_romaji(&[t("龘", None, false)]), "龘"); }
    #[test] fn no_reading_uses_kana_surface() { assert_eq!(toks_to_romaji(&[t("きらきら", None, false)]), "kirakira"); }
    #[test] fn load_missing_dir_errors() { assert!(load(Path::new("Z:/nope")).is_err()); }
    #[test] fn real_dictionary() {
        let Ok(dir) = std::env::var("OPENMUSIX_JA_DICT") else { return };
        let j = load(Path::new(&dir)).unwrap();
        assert_eq!(j.romanize("私は猫です"), "watashi wa neko desu");
    }
}
