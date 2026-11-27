//! LRC parser: `[mm:ss.xx]` stamps (several per line), `[offset:±ms]`, metadata tags dropped.
use crate::model::LyricLine;

/// Parse `[mm:ss]`, `[mm:ss.x]`, `[mm:ss.xx]`, `[mm:ss.xxx]` into milliseconds.
fn parse_stamp(tag: &str) -> Option<u64> {
    let (m, rest) = tag.split_once(':')?;
    let min: u64 = m.trim().parse().ok()?;
    let (sec, frac) = match rest.split_once(['.', ':']) {
        Some((s, f)) => (s, Some(f)),
        None => (rest, None),
    };
    let sec: u64 = sec.trim().parse().ok()?;
    if sec >= 60 {
        return None;
    }
    let frac_ms = match frac {
        None => 0,
        Some(f) if !f.is_empty() && f.len() <= 3 && f.bytes().all(|b| b.is_ascii_digit()) => {
            let v: u64 = f.parse().ok()?;
            v * 10u64.pow(3 - f.len() as u32)
        }
        Some(_) => return None,
    };
    Some(min * 60_000 + sec * 1000 + frac_ms)
}

/// Parse LRC text into time-sorted lines. Returns empty when no timed line exists.
pub fn parse_lrc(src: &str) -> Vec<LyricLine> {
    let mut offset_ms: i64 = 0;
    let mut raw: Vec<(u64, String)> = Vec::new();
    for line in src.trim_start_matches('\u{feff}').lines() {
        let mut rest = line.trim_end_matches('\r').trim_start();
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let tag = &stripped[..end];
            if let Some(t) = parse_stamp(tag) {
                stamps.push(t);
            } else if let Some(v) = tag.strip_prefix("offset:") {
                offset_ms = v.trim().trim_start_matches('+').parse().unwrap_or(0);
            }
            rest = &stripped[end + 1..];
        }
        let text = rest.trim().to_string();
        for t in stamps {
            raw.push((t, text.clone()));
        }
    }
    let mut out: Vec<LyricLine> = raw
        .into_iter()
        .map(|(t, text)| LyricLine { time_ms: (t as i64 - offset_ms).max(0) as u64, text })
        .collect();
    out.sort_by_key(|l| l.time_ms);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ll(t: u64, s: &str) -> LyricLine { LyricLine { time_ms: t, text: s.into() } }

    #[test] fn basic() { assert_eq!(parse_lrc("[00:12.34]Hello\n[01:02.5]World"), vec![ll(12340, "Hello"), ll(62500, "World")]); }
    #[test] fn three_digit_ms_and_no_fraction() { assert_eq!(parse_lrc("[00:01.234]a\n[00:02]b")[0].time_ms, 1234); assert_eq!(parse_lrc("[00:02]b")[0].time_ms, 2000); }
    #[test] fn multi_timestamps() {
        let l = parse_lrc("[00:10.00][00:30.00]Chorus\n[00:20.00]Verse");
        assert_eq!(l.iter().map(|x| x.time_ms).collect::<Vec<_>>(), vec![10000, 20000, 30000]);
        assert_eq!(l[2].text, "Chorus");
    }
    #[test] fn offset_tag_shifts_earlier() { assert_eq!(parse_lrc("[offset:+500]\n[00:10.00]x")[0].time_ms, 9500); }
    #[test] fn negative_offset_and_clamp() {
        assert_eq!(parse_lrc("[offset:-250]\n[00:01.00]x")[0].time_ms, 1250);
        assert_eq!(parse_lrc("[offset:+5000]\n[00:01.00]x")[0].time_ms, 0);
    }
    #[test] fn metadata_ignored_blank_kept() {
        let l = parse_lrc("[ar:Someone]\n[ti:T]\n[00:05.00]\n[00:06.00]Go");
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].text, "");
    }
    #[test] fn crlf_and_bom() { assert_eq!(parse_lrc("\u{feff}[00:01.00]a\r\n[00:02.00]b\r\n").len(), 2); }
    #[test] fn no_timestamps_is_empty() { assert!(parse_lrc("just plain words\nmore").is_empty()); }
}
