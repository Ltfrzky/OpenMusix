//! Single source of truth for what the overlay shows, broadcast via `tokio::sync::watch`.
use crate::lyrics::LyricsStatus;
use crate::model::NowPlaying;
use crate::roman::RomanLang;
use crate::settings::{Mode, Romanize, Styles};
use serde::Serialize;
use std::sync::RwLock;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub now_playing: Option<NowPlaying>,
    pub lyrics: LyricsStatus,
    pub track_key: Option<String>,
    pub mode: Mode,
    pub styles: Styles,
    pub global_offset_ms: i32,
    pub track_offset_ms: i32,
    pub show_plain_lyrics: bool,
    pub hide_when_idle: bool,
    pub romanize: Romanize,
    /// Romanized text parallel to the current lyric lines; `None` when off or not computed.
    pub roman: Option<Vec<Option<String>>>,
    /// Packs the current song needs that aren't installed. Read by the panel only.
    pub roman_missing: Vec<RomanLang>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            now_playing: None,
            lyrics: LyricsStatus::Loading,
            track_key: None,
            mode: Mode::Focus,
            styles: Styles::default(),
            global_offset_ms: 0,
            track_offset_ms: 0,
            show_plain_lyrics: false,
            hide_when_idle: false,
            romanize: Romanize::default(),
            roman: None,
            roman_missing: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Art {
    pub key: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub struct Hub {
    tx: watch::Sender<Snapshot>,
    art: RwLock<Option<Art>>,
}

impl Hub {
    pub fn new(initial: Snapshot) -> Self {
        Self { tx: watch::Sender::new(initial), art: RwLock::new(None) }
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.tx.subscribe()
    }

    pub fn current(&self) -> Snapshot {
        self.tx.borrow().clone()
    }

    /// Apply `f`; subscribers are notified only if the snapshot actually changed.
    pub fn modify(&self, f: impl FnOnce(&mut Snapshot)) {
        self.tx.send_if_modified(|s| {
            let before = s.clone();
            f(s);
            *s != before
        });
    }

    pub fn set_art(&self, art: Option<Art>) {
        *self.art.write().expect("art lock") = art;
    }

    pub fn current_art_key(&self) -> Option<String> {
        self.art.read().expect("art lock").as_ref().map(|a| a.key.clone())
    }

    pub fn art(&self, key: &str) -> Option<Art> {
        self.art.read().expect("art lock").as_ref().filter(|a| a.key == key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn hub_ignores_identical_modify() {
        let h = Hub::new(Snapshot::default());
        let mut rx = h.subscribe();
        rx.borrow_and_update();
        h.modify(|s| s.global_offset_ms = 0);
        assert!(!rx.has_changed().unwrap());
        h.modify(|s| s.mode = Mode::Widget);
        assert!(rx.has_changed().unwrap());
    }
    #[test] fn art_roundtrip() {
        let h = Hub::new(Snapshot::default());
        assert!(h.art("k").is_none());
        h.set_art(Some(Art { key: "k".into(), mime: "image/png".into(), bytes: vec![1] }));
        assert_eq!(h.art("k").unwrap().bytes, vec![1]);
        assert!(h.art("other").is_none());
    }
}
