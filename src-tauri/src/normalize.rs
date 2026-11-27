//! Title/artist cleanup used both for LRCLIB queries and for match comparisons.
use crate::model::Track;
use regex::Regex;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

static BRACKETS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*[\(\[][^\)\]]*[\)\]]").unwrap());
static NOISE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\s+[-\x{2013}\x{2014}]\s+[^-\x{2013}\x{2014}]*\b(remaster(ed)?|live|mono|stereo|edit|version|mix|official|lyrics?|audio|video|hd|4k|feat\.?|ft\.?|single)\b.*$",
    )
    .unwrap()
});
static FEAT_TAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+(feat\.|ft\.|featuring)\s.*$").unwrap());
static TOPIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s*-\s*topic$").unwrap());
static VEVO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s*vevo$").unwrap());

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Strip ` - Topic` and `VEVO` channel noise from an artist name, keeping case.
pub fn clean_artist(raw: &str) -> String {
    let s = TOPIC.replace(raw.trim(), "");
    let s = VEVO.replace(&s, "");
    let out = collapse(&s);
    if out.is_empty() { collapse(raw) } else { out }
}

fn strip_artist_prefix<'a>(title: &'a str, artist: &str) -> &'a str {
    for a in [clean_artist(artist), artist.trim().to_string()] {
        if a.is_empty() {
            continue;
        }
        for sep in [" - ", " \u{2013} ", " \u{2014} "] {
            let prefix = format!("{a}{sep}");
            if title.len() > prefix.len()
                && title.is_char_boundary(prefix.len())
                && match_key(&title[..prefix.len()]) == match_key(&prefix)
            {
                return &title[prefix.len()..];
            }
        }
    }
    title
}

/// Remove remaster/live/feat/YouTube noise and a leading "<artist> - ", keeping case.
/// Returns the trimmed original when cleaning would leave nothing.
pub fn clean_title(raw_title: &str, raw_artist: &str) -> String {
    let s = strip_artist_prefix(raw_title.trim(), raw_artist);
    let s = BRACKETS.replace_all(s, "");
    let s = NOISE_SUFFIX.replace(&s, "");
    let s = FEAT_TAIL.replace(&s, "");
    let out = collapse(&s);
    if out.is_empty() { collapse(raw_title) } else { out }
}

/// Comparison key: NFKC, straight quotes, lowercase, collapsed whitespace.
pub fn match_key(s: &str) -> String {
    let n: String = s
        .nfkc()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            c => c,
        })
        .collect();
    collapse(&n.to_lowercase())
}

/// Stable key for a track: `artist|title`, both cleaned and keyed.
pub fn track_key(t: &Track) -> String {
    format!(
        "{}|{}",
        match_key(&clean_artist(&t.artist)),
        match_key(&clean_title(&t.title, &t.artist))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn strips_remaster()  { assert_eq!(clean_title("Song - Remastered 2011", "A"), "Song"); }
    #[test] fn strips_feat()      { assert_eq!(clean_title("Song (feat. X & Y)", "A"), "Song"); assert_eq!(clean_title("Song [ft. X]", "A"), "Song"); }
    #[test] fn strips_live()      { assert_eq!(clean_title("Song - Live at Wembley", "A"), "Song"); }
    #[test] fn strips_youtube()   {
        assert_eq!(clean_title("Artist - Song (Official Music Video)", "ArtistVEVO"), "Song");
        assert_eq!(clean_title("Song [Lyrics]", "A"), "Song");
        assert_eq!(clean_title("Song (Official Audio)", "A"), "Song");
    }
    #[test] fn keeps_real_parens_only_when_not_noise() { assert_eq!(clean_title("Song (Radio Edit) - 2004 Remaster", "A"), "Song"); }
    #[test] fn artist_noise()     { assert_eq!(clean_artist("ArtistVEVO"), "Artist"); assert_eq!(clean_artist("Artist - Topic"), "Artist"); }
    #[test] fn key_is_case_and_space_insensitive() { assert_eq!(match_key("  Don\u{2019}t  Stop "), "don't stop"); }
    #[test] fn unicode_survives() { assert_eq!(match_key("夜に駆ける"), "夜に駆ける"); }
    #[test] fn empty_result_returns_original() { assert_eq!(clean_title("(Intro)", "A"), "(Intro)"); }
    #[test] fn track_key_combines() {
        let t = crate::model::Track { title: "Song - Remastered".into(), artist: "ArtistVEVO".into(), ..Default::default() };
        assert_eq!(track_key(&t), "artist|song");
    }
}
