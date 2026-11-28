//! Japanese to romaji: lindera with the pack's IPADIC dictionary, then `wana_kana`.
//!
//! IPADIC tokens are finer than words: verb stems are split from their endings (行っ + て),
//! numbers from counters (二 + 人), and each piece carries its own reading. Romanizing piece by
//! piece drops sounds (行っ → "i") and misses irregular readings (二人 → "ni nin"), so tokens are
//! first fixed up and grouped into words, and each word's kana is romanized as a whole.
use super::RomanError;
use std::borrow::Cow;
use std::path::Path;
use wana_kana::{ConvertJapanese, IsJapaneseStr};

/// Loaded Japanese tokenizer.
pub struct Japanese {
    segmenter: lindera::segmenter::Segmenter,
}

/// One IPADIC token: its text, katakana reading if the dictionary has one, and part of speech
/// (the first three IPADIC fields, e.g. 名詞 / 接尾 / 助数詞).
#[derive(Clone, Debug)]
pub struct Tok {
    pub surface: String,
    pub reading: Option<String>,
    pub pos: [String; 3],
}

impl Tok {
    fn is(&self, pos: &str, sub: &str) -> bool {
        self.pos[0] == pos && (sub.is_empty() || self.pos[1] == sub)
    }
    /// The token's kana: its reading, or its surface when that is already kana.
    fn kana(&self) -> Option<&str> {
        self.reading.as_deref().or(Some(self.surface.as_str()).filter(|s| s.is_kana()))
    }
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
        let toks: Vec<Tok> = tokens
            .iter_mut()
            .map(|t| {
                let surface = t.surface.to_string();
                let d = t.details();
                let field = |i: usize| d.get(i).copied().unwrap_or("*").to_string();
                let reading = d.get(7).filter(|r| **r != "*").map(|r| r.to_string());
                Tok { surface, reading, pos: [field(0), field(1), field(2)] }
            })
            .collect();
        toks_to_romaji(toks)
    }
}

/// Turn tokens into space-separated romaji.
pub fn toks_to_romaji(mut toks: Vec<Tok>) -> String {
    apply_overrides(&mut toks);
    merge_counters(&mut toks);
    fix_wa(&mut toks);
    let mut words: Vec<String> = Vec::new();
    let mut word: Vec<&Tok> = Vec::new();
    for t in &toks {
        // ・ separates words and is not shown (wana_kana would turn it into "/").
        if t.surface == "・" {
            words.extend(word_romaji(&word));
            word.clear();
            continue;
        }
        if word.last().is_some_and(|p| !glues(p, t)) {
            words.extend(word_romaji(&word));
            word.clear();
        }
        word.push(t);
    }
    words.extend(word_romaji(&word));
    let mut out = String::new();
    for w in words {
        // Punctuation attaches to the previous word; words are space-separated.
        if !out.is_empty() && w.chars().next().is_some_and(char::is_alphanumeric) {
            out.push(' ');
        }
        out.push_str(&w);
    }
    out
}

/// Whether `cur` belongs to the same word as `prev`: verb and adjective endings, te-form,
/// noun suffixes like たち, and anything after a trailing small っ.
fn glues(prev: &Tok, cur: &Tok) -> bool {
    let inflects = prev.is("動詞", "") || prev.is("形容詞", "") || prev.is("助動詞", "");
    let after_sokuon = prev.kana().is_some_and(|k| k.ends_with(['ッ', 'っ'])) && cur.kana().is_some();
    let after_stem = prev.pos[2] == STEM && cur.surface.chars().next().is_some_and(|c| ('ぁ'..='ゖ').contains(&c));
    after_sokuon
        || after_stem
        || (inflects && cur.is("助動詞", ""))
        || (inflects && cur.is("助詞", "接続助詞") && matches!(cur.surface.as_str(), "て" | "で" | "ば" | "ちゃ" | "じゃ" | "たり" | "だり"))
        || cur.is("動詞", "接尾")
        || (cur.is("動詞", "非自立") && cur.surface.starts_with(['て', 'で', 'ち', 'じ']))
        || ((prev.is("名詞", "") || prev.is("形容詞", "")) && cur.is("名詞", "接尾") && cur.pos[2] != "助数詞")
}

/// Marks a token made from an override verb stem; the okurigana after it joins the word.
const STEM: &str = "override-stem";

/// Lyric words IPADIC lacks or reads unlike songs do: (written form, reading, is a verb stem).
/// A stem takes the okurigana that follows it (彷徨 + って → samayotte).
const OVERRIDES: &[(&str, &str, bool)] = &[
    ("煌びやか", "キラビヤカ", false), ("愛おしい", "イトオシイ", false), ("明後日", "アサッテ", false),
    ("所謂", "イワユル", false), ("其処", "ソコ", false), ("貴女", "アナタ", false), ("一途", "イチズ", false),
    ("憂い", "ウレイ", false), ("颯", "ハヤテ", false), ("蒼", "アオ", false),
    ("彷徨", "サマヨ", true), ("揺蕩", "タユタ", true), ("嗤", "ワラ", true), ("煌", "キラメ", true),
];

/// Replace runs of tokens spelling an override word with one token carrying its reading.
fn apply_overrides(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i < toks.len() {
        'entries: for (word, reading, stem) in OVERRIDES {
            let mut spelled = String::new();
            for end in i..toks.len() {
                spelled.push_str(&toks[end].surface);
                if spelled.len() >= word.len() {
                    if spelled == *word {
                        let pos = if *stem { ["動詞".into(), "自立".into(), STEM.into()] } else { ["名詞".into(), "一般".into(), "*".into()] };
                        toks.splice(i..=end, [Tok { surface: spelled, reading: Some(reading.to_string()), pos }]);
                        break 'entries;
                    }
                    break;
                }
            }
        }
        i += 1;
    }
}

/// Romanize one word. Kana is romanized as a whole so sounds that span tokens survive;
/// a word with a piece that has no kana (an unknown kanji) keeps that piece as written.
fn word_romaji(word: &[&Tok]) -> Option<String> {
    if word.is_empty() {
        return None;
    }
    let kana: Option<String> = word.iter().map(|t| t.kana()).collect();
    Some(match kana {
        Some(k) => kana_to_romaji(&k),
        None => word.iter().map(|t| t.kana().map_or_else(|| t.surface.clone(), kana_to_romaji)).collect(),
    })
}

/// Particles は/へ/を read ワ/エ/オ, as does the は ending greetings and conjunctions (こんにちは, では).
fn fix_wa(toks: &mut [Tok]) {
    for t in toks {
        let new = match t.surface.as_str() {
            "は" if t.is("助詞", "") => Some("ワ".to_string()),
            "へ" if t.is("助詞", "") => Some("エ".to_string()),
            "を" if t.is("助詞", "") => Some("オ".to_string()),
            s if s.ends_with('は') && (t.is("感動詞", "") || t.is("接続詞", "")) => {
                t.reading.as_deref().and_then(|r| r.strip_suffix('ハ')).map(|r| format!("{r}ワ"))
            }
            _ => None,
        };
        if new.is_some() {
            t.reading = new;
        }
    }
}

/// Number + counter compounds whose reading the pieces cannot produce.
const IRREGULAR_COUNTERS: &[(&str, &str)] = &[
    ("一人", "ヒトリ"), ("二人", "フタリ"), ("二十歳", "ハタチ"),
    ("二日", "フツカ"), ("三日", "ミッカ"), ("四日", "ヨッカ"), ("五日", "イツカ"), ("六日", "ムイカ"),
    ("七日", "ナノカ"), ("八日", "ヨウカ"), ("九日", "ココノカ"), ("十日", "トオカ"), ("二十日", "ハツカ"),
];

/// IPADIC splits numbers from counters (二 + 人) and even numbers themselves (二十 → 二 + 十).
/// Join each number run and its counter into one word with the compound's reading.
fn merge_counters(toks: &mut Vec<Tok>) {
    let mut c = 0;
    while c < toks.len() {
        if toks[c].is("名詞", "接尾") && toks[c].pos[2] == "助数詞" {
            let mut start = c;
            while start > 0 && toks[start - 1].is("名詞", "数") && toks[start - 1].reading.is_some() {
                start -= 1;
            }
            if start < c {
                let surface: String = toks[start..=c].iter().map(|t| t.surface.as_str()).collect();
                let number: String = toks[start..c].iter().filter_map(|t| t.reading.as_deref()).collect();
                let counter = &toks[c];
                let reading = match IRREGULAR_COUNTERS.iter().find(|(s, _)| *s == surface) {
                    Some((_, r)) => r.to_string(),
                    None => counter.reading.as_deref().map_or(number.clone(), |r| counter_reading(&number, r, &counter.surface)),
                };
                let merged = Tok { surface, reading: Some(reading), pos: ["名詞".into(), "一般".into(), "*".into()] };
                toks.splice(start..=c, [merged]);
                c = start;
            }
        }
        c += 1;
    }
}

/// Sound changes where a number meets a counter: 一回 イッカイ, 六本 ロッポン, 三本 サンボン,
/// 三分 サンプン, 四時 ヨジ, 四人 ヨニン, 九時 クジ.
fn counter_reading(number: &str, counter: &str, counter_surface: &str) -> String {
    const SOKUON: &[(&str, &str)] = &[("イチ", "イッ"), ("ロク", "ロッ"), ("ハチ", "ハッ"), ("ジュウ", "ジュッ")];
    let first = counter.chars().next().unwrap_or(' ');
    let h_row = "ハヒフヘホ".contains(first);
    let voiceless = "カキクケコサシスセソタチツテト".contains(first);
    for (tail, short) in SOKUON {
        if let Some(head) = number.strip_suffix(tail) {
            if voiceless {
                return format!("{head}{short}{counter}");
            }
            if h_row {
                return format!("{head}{short}{}", shift_h(counter, "パピプペポ"));
            }
        }
    }
    if number.ends_with('ン') && h_row {
        let row = if counter_surface == "分" { "パピプペポ" } else { "バビブベボ" };
        return format!("{number}{}", shift_h(counter, row));
    }
    if let Some(head) = number.strip_suffix("ヨン").filter(|_| matches!(counter_surface, "時" | "人")) {
        return format!("{head}ヨ{counter}");
    }
    if let Some(head) = number.strip_suffix("キュウ").filter(|_| counter_surface == "時") {
        return format!("{head}ク{counter}");
    }
    format!("{number}{counter}")
}

/// Replace a leading ハ-row kana with the same column of `row` (パ or バ row).
fn shift_h(kana: &str, row: &str) -> String {
    let mut chars = kana.chars();
    let first = chars.next().unwrap_or_default();
    match "ハヒフヘホ".chars().position(|c| c == first) {
        Some(i) => row.chars().nth(i).into_iter().chain(chars).collect(),
        None => kana.to_string(),
    }
}

/// Katakana combinations for foreign sounds that wana_kana romanizes badly (ティ → "tei").
const EXTENDED: &[(&str, &str)] = &[
    ("ヴァ", "va"), ("ヴィ", "vi"), ("ヴェ", "ve"), ("ヴォ", "vo"), ("ヴュ", "vyu"), ("ヴ", "vu"),
    ("ティ", "ti"), ("ディ", "di"), ("トゥ", "tu"), ("ドゥ", "du"), ("テュ", "tyu"), ("デュ", "dyu"),
    ("ファ", "fa"), ("フィ", "fi"), ("フェ", "fe"), ("フォ", "fo"), ("フュ", "fyu"),
    ("ウィ", "wi"), ("ウェ", "we"), ("ウォ", "wo"), ("イェ", "ye"),
    ("ツァ", "tsa"), ("ツィ", "tsi"), ("ツェ", "tse"), ("ツォ", "tso"),
    ("クァ", "kwa"), ("グァ", "gwa"), ("スィ", "si"), ("ズィ", "zi"),
];

/// Kana to romaji, handling the extended katakana in `EXTENDED` (with a preceding ッ or a following ー).
pub fn kana_to_romaji(kana: &str) -> String {
    let mut out = String::new();
    let mut buf = String::new();
    let mut rest = kana;
    while let Some(c) = rest.chars().next() {
        let Some((k, r)) = EXTENDED.iter().find(|(k, _)| rest.starts_with(k)) else {
            buf.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        };
        let doubled = buf.ends_with(['ッ', 'っ']);
        if doubled {
            buf.pop();
        }
        out.push_str(&buf.to_romaji());
        buf.clear();
        if doubled {
            out.push_str(&r[..1]);
        }
        out.push_str(r);
        rest = &rest[k.len()..];
        while let Some(tail) = rest.strip_prefix('ー') {
            out.push_str(&r[r.len() - 1..]);
            rest = tail;
        }
    }
    out.push_str(&buf.to_romaji());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tk(surface: &str, reading: Option<&str>, pos: &str, sub: &str, sub2: &str) -> Tok {
        Tok { surface: surface.into(), reading: reading.map(Into::into), pos: [pos.into(), sub.into(), sub2.into()] }
    }
    fn noun(s: &str, r: &str) -> Tok { tk(s, Some(r), "名詞", "一般", "*") }
    fn particle(s: &str, r: &str) -> Tok { tk(s, Some(r), "助詞", "格助詞", "*") }
    fn num(s: &str, r: &str) -> Tok { tk(s, Some(r), "名詞", "数", "*") }
    fn ctr(s: &str, r: &str) -> Tok { tk(s, Some(r), "名詞", "接尾", "助数詞") }

    #[test] fn particles() {
        let v = vec![noun("私", "ワタシ"), tk("は", Some("ハ"), "助詞", "係助詞", "*"), noun("東京", "トウキョウ"), particle("へ", "ヘ")];
        assert_eq!(toks_to_romaji(v), "watashi wa toukyou e");
    }
    #[test] fn wo_particle() { assert_eq!(toks_to_romaji(vec![noun("君", "キミ"), particle("を", "ヲ")]), "kimi o"); }
    #[test] fn sokuon_and_long_vowel() { assert_eq!(toks_to_romaji(vec![noun("ずっと", "ズット"), noun("ラーメン", "ラーメン")]), "zutto raamen"); }
    #[test] fn unknown_kanji_passes() { assert_eq!(toks_to_romaji(vec![tk("龘", None, "名詞", "一般", "*")]), "龘"); }
    #[test] fn no_reading_uses_kana_surface() { assert_eq!(toks_to_romaji(vec![tk("きらきら", None, "名詞", "一般", "*")]), "kirakira"); }
    #[test] fn load_missing_dir_errors() { assert!(load(Path::new("Z:/nope")).is_err()); }

    #[test] fn sokuon_across_tokens() {
        let v = vec![tk("行っ", Some("イッ"), "動詞", "自立", "*"), tk("て", Some("テ"), "助詞", "接続助詞", "*")];
        assert_eq!(toks_to_romaji(v), "itte");
    }
    #[test] fn verb_endings_join() {
        let v = vec![tk("行か", Some("イカ"), "動詞", "自立", "*"), tk("ない", Some("ナイ"), "助動詞", "*", "*"), noun("夢", "ユメ"), tk("だ", Some("ダ"), "助動詞", "*", "*")];
        assert_eq!(toks_to_romaji(v), "ikanai yume da");
    }
    #[test] fn greeting_wa() { assert_eq!(toks_to_romaji(vec![tk("こんにちは", Some("コンニチハ"), "感動詞", "*", "*")]), "konnichiwa"); }
    #[test] fn middle_dot_separates() { assert_eq!(toks_to_romaji(vec![noun("夢", "ユメ"), tk("・", Some("・"), "記号", "一般", "*"), noun("希望", "キボウ")]), "yume kibou"); }
    #[test] fn counters() {
        assert_eq!(toks_to_romaji(vec![num("二", "ニ"), ctr("人", "ニン")]), "futari");
        assert_eq!(toks_to_romaji(vec![num("二", "ニ"), num("十", "ジュウ"), ctr("日", "ニチ")]), "hatsuka");
        assert_eq!(toks_to_romaji(vec![num("三", "サン"), num("十", "ジュウ"), ctr("日", "ニチ")]), "sanjuunichi");
        assert_eq!(toks_to_romaji(vec![num("三", "サン"), ctr("人", "ニン")]), "sannin");
    }
    #[test] fn counter_sound_changes() {
        assert_eq!(counter_reading("イチ", "カイ", "回"), "イッカイ");
        assert_eq!(counter_reading("ロク", "ホン", "本"), "ロッポン");
        assert_eq!(counter_reading("サン", "ホン", "本"), "サンボン");
        assert_eq!(counter_reading("サン", "フン", "分"), "サンプン");
        assert_eq!(counter_reading("ジュウ", "コ", "個"), "ジュッコ");
        assert_eq!(counter_reading("ヨン", "ジ", "時"), "ヨジ");
        assert_eq!(counter_reading("キュウ", "ジ", "時"), "クジ");
        assert_eq!(counter_reading("イチ", "ド", "度"), "イチド");
    }
    #[test] fn extended_katakana() {
        assert_eq!(kana_to_romaji("ヴァイオリン"), "vaiorin");
        assert_eq!(kana_to_romaji("ティーン"), "tiin");
        assert_eq!(kana_to_romaji("フィーリング"), "fiiringu");
        assert_eq!(kana_to_romaji("ディズニー"), "dizunii");
        assert_eq!(kana_to_romaji("パーティー"), "paatii");
        assert_eq!(kana_to_romaji("ハッティ"), "hatti");
        assert_eq!(kana_to_romaji("シェイク"), "sheiku");
    }

    /// Run with OPENMUSIX_JA_DICT pointing at an installed `ja` pack.
    #[test] fn real_dictionary() {
        let Ok(dir) = std::env::var("OPENMUSIX_JA_DICT") else { return };
        let j = load(Path::new(&dir)).unwrap();
        for (input, want) in [
            ("私は猫です", "watashi wa neko desu"),
            ("二人だけの空が広がる夜に", "futari dake no sora ga hirogaru yoru ni"),
            ("一人で", "hitori de"),
            ("二十日", "hatsuka"),
            ("行って", "itte"),
            ("言った", "itta"),
            ("会いたかった", "aitakatta"),
            ("忘れられない", "wasurerarenai"),
            ("愛してるよ", "aishiteru yo"),
            ("抱きしめて", "dakishimete"),
            ("こんにちは", "konnichiwa"),
            ("では", "dewa"),
            ("一回", "ikkai"),
            ("三本", "sanbon"),
            ("四時", "yoji"),
            ("何人", "nannin"),
            ("私たち", "watashitachi"),
            ("夢・希望", "yume kibou"),
            ("ヴァイオリン", "vaiorin"),
            ("騒がしい日々に笑えない君に", "sawagashii hibi ni waraenai kimi ni"),
            ("彷徨って", "samayotte"),
            ("嗤った", "waratta"),
            ("揺蕩い", "tayutai"),
            ("煌く", "kirameku"),
            ("煌びやか", "kirabiyaka"),
            ("煌めく", "kirameku"),
            ("一途", "ichizu"),
            ("颯が", "hayate ga"),
            ("蒼い空", "aoi sora"),
            ("寂しさ", "sabishisa"),
            ("儚げ", "hakanage"),
            ("三年前", "sannen mae"),
            ("一歩", "ippo"),
            ("六分", "roppun"),
        ] {
            assert_eq!(j.romanize(input), want, "{input}");
        }
    }
}
