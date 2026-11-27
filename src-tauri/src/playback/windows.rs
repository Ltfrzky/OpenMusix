//! Windows media session (GSMTC) source. Event-driven: re-reads only when Windows says something changed.
use super::{PlaybackEvent, SourceSnapshot};
use crate::hub::{Art, Hub};
use crate::model::Track;
use sha2::{Digest, Sha256};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

/// 100 ns ticks between 1601-01-01 and 1970-01-01.
const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;

pub fn ticks_to_unix_ms(universal_time: i64) -> i64 {
    (universal_time - EPOCH_DIFF_TICKS) / 10_000
}

pub fn timespan_to_ms(duration_100ns: i64) -> u64 {
    (duration_100ns.max(0) / 10_000) as u64
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub app_id: String,
    pub is_playing: bool,
}

fn is_spotify(s: &SessionInfo) -> bool {
    s.app_id.to_lowercase().contains("spotify")
}

/// Spotify & playing → any playing → Spotify → current → first.
pub fn pick_session(sessions: &[SessionInfo], current: Option<usize>) -> Option<usize> {
    let find = |f: &dyn Fn(&SessionInfo) -> bool| sessions.iter().position(f);
    find(&|s| is_spotify(s) && s.is_playing)
        .or_else(|| find(&|s| s.is_playing))
        .or_else(|| find(&is_spotify))
        .or(current.filter(|&i| i < sessions.len()))
        .or(if sessions.is_empty() { None } else { Some(0) })
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Spawn the reader thread. It owns all WinRT objects; handlers only poke a channel.
pub fn spawn_windows_source(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) {
    std::thread::Builder::new()
        .name("gsmtc".into())
        .spawn(move || {
            if let Err(e) = imp::run(tx, hub) {
                tracing::error!("Windows media source stopped: {e:?}");
            }
        })
        .expect("spawn gsmtc thread");
}

mod imp {
    use super::*;
    use windows::Foundation::TypedEventHandler;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Storage::Streams::DataReader;

    struct Subscribed {
        session: Session,
        tokens: [i64; 3],
    }

    impl Subscribed {
        fn unsubscribe(self) {
            let _ = self.session.RemoveMediaPropertiesChanged(self.tokens[0]);
            let _ = self.session.RemovePlaybackInfoChanged(self.tokens[1]);
            let _ = self.session.RemoveTimelinePropertiesChanged(self.tokens[2]);
        }
    }

    fn subscribe(session: &Session, poke: &std_mpsc::Sender<()>) -> windows::core::Result<Subscribed> {
        let p1 = poke.clone();
        let p2 = poke.clone();
        let p3 = poke.clone();
        let t1 = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| { let _ = p1.send(()); Ok(()) }))?;
        let t2 = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| { let _ = p2.send(()); Ok(()) }))?;
        let t3 = session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| { let _ = p3.send(()); Ok(()) }))?;
        Ok(Subscribed { session: session.clone(), tokens: [t1, t2, t3] })
    }

    fn sessions(m: &Manager) -> windows::core::Result<Vec<Session>> {
        let v = m.GetSessions()?;
        (0..v.Size()?).map(|i| v.GetAt(i)).collect()
    }

    fn info(s: &Session) -> SessionInfo {
        let app_id = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
        let is_playing = s
            .GetPlaybackInfo()
            .and_then(|p| p.PlaybackStatus())
            .map(|st| st == Status::Playing)
            .unwrap_or(false);
        SessionInfo { app_id, is_playing }
    }

    fn choose(m: &Manager) -> windows::core::Result<Option<Session>> {
        let all = sessions(m)?;
        let infos: Vec<SessionInfo> = all.iter().map(info).collect();
        let current_id = m.GetCurrentSession().ok().and_then(|s| s.SourceAppUserModelId().ok()).map(|h| h.to_string());
        let current = current_id.and_then(|id| infos.iter().position(|i| i.app_id == id));
        Ok(pick_session(&infos, current).map(|i| all[i].clone()))
    }

    fn read_thumbnail(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<Art> {
        let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
        let size = stream.Size().ok()? as u32;
        if size == 0 || size > 8 * 1024 * 1024 {
            return None;
        }
        let mime = stream.ContentType().map(|h| h.to_string()).unwrap_or_else(|_| "image/png".into());
        let reader = DataReader::CreateDataReader(&stream).ok()?;
        reader.LoadAsync(size).ok()?.get().ok()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes).ok()?;
        let h = Sha256::digest(&bytes);
        let key: String = h.iter().take(8).map(|b| format!("{b:02x}")).collect();
        Some(Art { key, mime: if mime.is_empty() { "image/png".into() } else { mime }, bytes })
    }

    /// Returns the snapshot plus the art (only re-read when the track identity changed).
    fn read(s: &Session, last_track_id: &mut String, hub: &Hub) -> windows::core::Result<Option<SourceSnapshot>> {
        let props = s.TryGetMediaPropertiesAsync()?.get()?;
        let title = props.Title()?.to_string();
        if title.trim().is_empty() {
            return Ok(None);
        }
        let artist = props.Artist()?.to_string();
        let album = props.AlbumTitle().map(|h| h.to_string()).ok().filter(|a| !a.trim().is_empty());
        let tl = s.GetTimelineProperties()?;
        let start = tl.StartTime()?.Duration;
        let end = tl.EndTime()?.Duration;
        let duration_ms = (end > start).then(|| timespan_to_ms(end - start)).filter(|&d| d > 0);
        let position_ms = timespan_to_ms(tl.Position()?.Duration - start);
        let updated = tl.LastUpdatedTime()?.UniversalTime;
        let sampled_at = if updated > EPOCH_DIFF_TICKS { ticks_to_unix_ms(updated) } else { now_unix_ms() };
        let is_playing = info(s).is_playing;

        let track_id = format!("{title}\u{1f}{artist}\u{1f}{}", album.clone().unwrap_or_default());
        if *last_track_id != track_id {
            *last_track_id = track_id;
            hub.set_art(read_thumbnail(&props));
        }
        let art_url = hub.current_art_key().map(|k| format!("/art/{k}"));
        Ok(Some(SourceSnapshot {
            track: Track { title, artist, album, duration_ms, art_url },
            position_ms,
            sampled_at,
            is_playing,
        }))
    }

    pub fn run(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) -> windows::core::Result<()> {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let manager = Manager::RequestAsync()?.get()?;
        let (poke, wake) = std_mpsc::channel::<()>();
        let pm = poke.clone();
        let _t1 = manager.SessionsChanged(&TypedEventHandler::new(move |_, _| { let _ = pm.send(()); Ok(()) }))?;
        let pc = poke.clone();
        let _t2 = manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| { let _ = pc.send(()); Ok(()) }))?;

        let mut subscribed: Option<Subscribed> = None;
        let mut last_track_id = String::new();
        let mut last_sent: Option<Option<SourceSnapshot>> = None;
        let _ = poke.send(());
        while wake.recv().is_ok() {
            // Debounce bursts (a track change fires several events at once).
            std::thread::sleep(Duration::from_millis(50));
            while wake.try_recv().is_ok() {}

            // A session can also change state without SessionsChanged (e.g. one starts playing),
            // so the choice is re-evaluated on every event.
            let chosen = choose(&manager).unwrap_or(None);
            let same = match (&subscribed, &chosen) {
                (Some(a), Some(b)) => a.session == *b,
                (None, None) => true,
                _ => false,
            };
            if !same {
                if let Some(old) = subscribed.take() {
                    old.unsubscribe();
                }
                if let Some(s) = &chosen {
                    subscribed = subscribe(s, &poke).ok();
                }
            }
            let snap = match &chosen {
                Some(s) => read(s, &mut last_track_id, &hub).unwrap_or_else(|e| {
                    tracing::warn!("GSMTC read failed: {e:?}");
                    None
                }),
                None => None,
            };
            if snap.is_none() {
                last_track_id.clear();
                hub.set_art(None);
            }
            if last_sent.as_ref() != Some(&snap) {
                last_sent = Some(snap.clone());
                if tx.blocking_send(PlaybackEvent::Windows(snap)).is_err() {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn si(id: &str, playing: bool) -> SessionInfo { SessionInfo { app_id: id.into(), is_playing: playing } }
    #[test] fn epoch_conversion() {
        assert_eq!(ticks_to_unix_ms(116_444_736_000_000_000), 0);
        assert_eq!(ticks_to_unix_ms(116_444_736_000_000_000 + 10_000), 1);
    }
    #[test] fn timespan() { assert_eq!(timespan_to_ms(12_340_000), 1234); assert_eq!(timespan_to_ms(-5), 0); }
    #[test] fn prefers_playing_session_over_paused_spotify() {
        let s = [si("Spotify.exe", false), si("chrome", true)];
        assert_eq!(pick_session(&s, Some(0)), Some(1));
    }
    #[test] fn prefers_playing_spotify() {
        let s = [si("chrome", true), si("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", true)];
        assert_eq!(pick_session(&s, None), Some(1));
    }
    #[test] fn falls_back_to_current() { assert_eq!(pick_session(&[si("a", false), si("b", false)], Some(1)), Some(1)); }
    #[test] fn paused_spotify_beats_other_paused() { assert_eq!(pick_session(&[si("a", false), si("Spotify.exe", false)], Some(0)), Some(1)); }
    #[test] fn empty() { assert_eq!(pick_session(&[], None), None); }
}
