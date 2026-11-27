//! Chinese to pinyin, from the pack's `pinyin.txt` (mozillazg/pinyin-data format).
use super::RomanError;
use std::collections::HashMap;
use std::path::Path;

/// Loaded pinyin data: the first listed reading for each character.
pub struct Pinyin {
    map: HashMap<char, String>,
}

/// Parse `pinyin.txt`: lines like `U+4E2D: zhōng,zhòng  # 中`.
pub fn parse(text: &str) -> Pinyin {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("U+") else { continue };
        let Some((hex, readings)) = rest.split_once(':') else { continue };
        let Some(c) = u32::from_str_radix(hex.trim(), 16).ok().and_then(char::from_u32) else { continue };
        let readings = readings.split('#').next().unwrap_or_default();
        if let Some(first) = readings.split(',').map(str::trim).find(|r| !r.is_empty()) {
            map.insert(c, first.to_string());
        }
    }
    Pinyin { map }
}

/// Load `dir/pinyin.txt`.
pub fn load(dir: &Path) -> Result<Pinyin, RomanError> {
    let p = parse(&std::fs::read_to_string(dir.join("pinyin.txt"))?);
    if p.map.is_empty() {
        return Err(RomanError::BadData("pinyin.txt has no readings".into()));
    }
    Ok(p)
}

impl Pinyin {
    /// One syllable per character, space-separated. Unknown characters pass through.
    pub fn romanize(&self, run: &str) -> String {
        run.chars()
            .map(|c| self.map.get(&c).cloned().unwrap_or_else(|| c.to_string()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roman::Romanizer;
    const FIX: &str = "# c\nU+4E2D: zhōng,zhòng  # 中\nU+56FD: guó  # 国\nU+7231: ài  # 爱\n";
    #[test] fn parse_and_romanize() { assert_eq!(parse(FIX).romanize("中国"), "zhōng guó"); }
    #[test] fn unknown_char_passes() { assert_eq!(parse(FIX).romanize("中龘"), "zhōng 龘"); }
    #[test] fn romanizer_uses_zh() {
        let r = Romanizer::new();
        r.set_zh(Some(parse(FIX)));
        let o = r.romanize(&["我爱中国!", "Hi 中国"]);
        assert_eq!(o.lines, vec![Some("我 ài zhōng guó!".into()), Some("Hi zhōng guó".into())]);
        assert!(o.missing.is_empty());
    }
    #[test] fn load_missing_file_errors() { assert!(load(Path::new("Z:/nope")).is_err()); }
    #[test] fn load_reads_pinyin_txt() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("pinyin.txt"), FIX).unwrap();
        assert_eq!(load(d.path()).unwrap().romanize("国"), "guó");
    }
}
