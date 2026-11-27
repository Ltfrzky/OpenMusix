//! Hangul to Revised Romanization, with linking and nasalization only.

const INITIAL: [&str; 19] = ["g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h"];
const MEDIAL: [&str; 21] = [
    "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i",
];
/// Final consonant at the end of a word or before another consonant.
const FINAL: [&str; 28] = [
    "", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "l", "l", "l", "p", "l", "m", "p", "p", "t", "t", "ng", "t", "t", "k", "t", "p", "t",
];
/// Final consonant when the next syllable starts with a silent ㅇ: (part that stays, part that moves).
const LINKED: [(&str, &str); 28] = [
    ("", ""), ("", "g"), ("", "kk"), ("k", "s"), ("", "n"), ("n", "j"), ("", "n"), ("", "d"), ("", "r"), ("l", "g"),
    ("l", "m"), ("l", "b"), ("l", "s"), ("l", "t"), ("l", "p"), ("", "r"), ("", "m"), ("", "b"), ("p", "s"), ("", "s"),
    ("", "ss"), ("ng", ""), ("", "j"), ("", "ch"), ("", "k"), ("", "t"), ("", "p"), ("", ""),
];
const SILENT_INITIAL: usize = 11;

fn decompose(c: char) -> Option<(usize, usize, usize)> {
    let v = (c as u32).checked_sub(0xAC00)?;
    if v > 0xD7A3 - 0xAC00 {
        return None;
    }
    let v = v as usize;
    Some((v / 588, (v % 588) / 28, v % 28))
}

/// Nasalized final before ㄴ/ㅁ: k-sounds become ng, t-sounds n, p-sounds m.
fn nasal(fin: usize) -> Option<&'static str> {
    match FINAL[fin] {
        "k" => Some("ng"),
        "t" => Some("n"),
        "p" => Some("m"),
        _ => None,
    }
}

pub fn romanize(run: &str) -> String {
    let chars: Vec<char> = run.chars().collect();
    let mut out = String::new();
    // Initial carried over from the previous syllable's linked final.
    let mut carried: Option<&str> = None;
    for (i, &c) in chars.iter().enumerate() {
        let Some((ini, med, fin)) = decompose(c) else {
            out.push(c);
            carried = None;
            continue;
        };
        match carried.take() {
            Some(m) if ini == SILENT_INITIAL => out.push_str(m),
            _ => out.push_str(INITIAL[ini]),
        }
        out.push_str(MEDIAL[med]);
        match chars.get(i + 1).and_then(|&n| decompose(n)) {
            Some((SILENT_INITIAL, _, _)) if fin != 0 => {
                let (stay, moved) = LINKED[fin];
                out.push_str(stay);
                carried = Some(moved);
            }
            Some((2 | 6, _, _)) if nasal(fin).is_some() => out.push_str(nasal(fin).unwrap_or_default()),
            _ => out.push_str(FINAL[fin]),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn ko_basic() { assert_eq!(romanize("안녕하세요"), "annyeonghaseyo"); assert_eq!(romanize("사랑해"), "saranghae"); }
    #[test] fn ko_linking() { assert_eq!(romanize("한국어"), "hangugeo"); }
    #[test] fn ko_nasalization() { assert_eq!(romanize("감사합니다"), "gamsahamnida"); }
    #[test] fn ko_keeps_spaces() { assert_eq!(romanize("사랑 해"), "sarang hae"); }
    #[test] fn ko_final_at_end() { assert_eq!(romanize("밥"), "bap"); assert_eq!(romanize("꽃"), "kkot"); }
}
