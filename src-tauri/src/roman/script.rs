//! Which writing system a character belongs to.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Han,
    Kana,
    Hangul,
    Other,
}

pub fn script_of(c: char) -> Script {
    match c as u32 {
        0x3005 | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F => Script::Han,
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => Script::Kana,
        0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F => Script::Hangul,
        _ => Script::Other,
    }
}

/// Split a line into runs of consecutive characters that share a script.
pub fn runs(line: &str) -> Vec<(Script, &str)> {
    let mut out: Vec<(Script, &str)> = Vec::new();
    let mut start = 0;
    let mut cur: Option<Script> = None;
    for (i, c) in line.char_indices() {
        let s = script_of(c);
        match cur {
            Some(p) if p == s => {}
            Some(p) => {
                out.push((p, &line[start..i]));
                start = i;
                cur = Some(s);
            }
            None => cur = Some(s),
        }
    }
    if let Some(p) = cur {
        out.push((p, &line[start..]));
    }
    out
}

/// Whether any line has kana; then Han characters are read as Japanese.
pub fn song_has_kana(lines: &[&str]) -> bool {
    lines.iter().any(|l| l.chars().any(|c| script_of(c) == Script::Kana))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn script_ranges() {
        assert_eq!(script_of('ー'), Script::Kana);
        assert_eq!(script_of('ｱ'), Script::Kana);
        assert_eq!(script_of('あ'), Script::Kana);
        assert_eq!(script_of('々'), Script::Han);
        assert_eq!(script_of('中'), Script::Han);
        assert_eq!(script_of('한'), Script::Hangul);
        assert_eq!(script_of('Ａ'), Script::Other);
        assert_eq!(script_of('a'), Script::Other);
    }
    #[test] fn runs_split_by_script() {
        assert_eq!(runs("Hi 中国あ한"), vec![(Script::Other, "Hi "), (Script::Han, "中国"), (Script::Kana, "あ"), (Script::Hangul, "한")]);
        assert!(runs("").is_empty());
    }
    #[test] fn kana_detection() {
        assert!(song_has_kana(&["中国", "愛してる"]));
        assert!(!song_has_kana(&["中国", "사랑"]));
    }
}
