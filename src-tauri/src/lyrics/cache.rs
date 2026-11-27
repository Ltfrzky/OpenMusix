//! On-disk lyrics cache: one JSON file per track key, TTL by result kind.
use crate::model::LyricsResult;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const NOT_FOUND_TTL: Duration = Duration::from_secs(7 * 86_400);
const FOUND_TTL: Duration = Duration::from_secs(30 * 86_400);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    /// Unix seconds.
    stored_at: u64,
    result: LyricsResult,
}

pub struct LyricsCache {
    dir: PathBuf,
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl LyricsCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, key: &str) -> PathBuf {
        let h = Sha256::digest(key.as_bytes());
        let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
        self.dir.join(format!("{hex}.json"))
    }

    pub fn get(&self, key: &str, now: SystemTime) -> Option<LyricsResult> {
        let bytes = std::fs::read(self.path(key)).ok()?;
        let e: Entry = serde_json::from_slice(&bytes).ok()?;
        let ttl = if e.result == LyricsResult::NotFound { NOT_FOUND_TTL } else { FOUND_TTL };
        let age = secs(now).saturating_sub(e.stored_at);
        (age <= ttl.as_secs()).then_some(e.result)
    }

    /// Best effort: failures are logged and ignored (the cache is an optimisation).
    pub fn put(&self, key: &str, r: &LyricsResult, now: SystemTime) {
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.dir)?;
            let path = self.path(key);
            let tmp = path.with_extension("tmp");
            let data = serde_json::to_vec(&Entry { stored_at: secs(now), result: r.clone() })?;
            std::fs::write(&tmp, data)?;
            std::fs::rename(&tmp, &path)
        };
        if let Err(e) = write() {
            tracing::warn!("lyrics cache write failed: {e}");
        }
    }

    pub fn clear(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LyricLine;
    use std::time::Duration;
    const DAY: u64 = 86_400;
    fn t0() -> SystemTime { SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000) }

    #[test] fn not_found_expires_after_7_days() {
        let d = tempfile::tempdir().unwrap(); let c = LyricsCache::new(d.path().into());
        c.put("k", &LyricsResult::NotFound, t0());
        assert!(c.get("k", t0() + Duration::from_secs(6 * DAY)).is_some());
        assert!(c.get("k", t0() + Duration::from_secs(7 * DAY + 1)).is_none());
    }
    #[test] fn synced_survives_7_days() {
        let d = tempfile::tempdir().unwrap(); let c = LyricsCache::new(d.path().into());
        let r = LyricsResult::Synced(vec![LyricLine { time_ms: 1, text: "a".into() }]);
        c.put("k", &r, t0());
        assert_eq!(c.get("k", t0() + Duration::from_secs(8 * DAY)), Some(r));
    }
    #[test] fn corrupt_entry_is_miss() {
        let d = tempfile::tempdir().unwrap(); let c = LyricsCache::new(d.path().into());
        c.put("k", &LyricsResult::NotFound, t0());
        for e in std::fs::read_dir(d.path()).unwrap() { std::fs::write(e.unwrap().path(), "{oops").unwrap(); }
        assert!(c.get("k", t0()).is_none());
    }
    #[test] fn clear_empties() {
        let d = tempfile::tempdir().unwrap(); let c = LyricsCache::new(d.path().join("sub"));
        c.put("k", &LyricsResult::NotFound, t0()); c.clear().unwrap();
        assert!(c.get("k", t0()).is_none());
    }
}
