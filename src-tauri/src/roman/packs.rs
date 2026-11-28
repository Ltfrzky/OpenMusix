//! Downloadable language packs: fetch, verify (SHA-256), unzip and load into the romanizer.
use super::{RomanLang, Romanizer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::Notify;

/// One downloadable pack, as listed in the manifest compiled into the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSpec {
    pub id: RomanLang,
    pub version: u32,
    pub label: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

const RELEASE: &str = "https://github.com/Ltfrzky/OpenMusix/releases/download/packs-v1/";

/// The packs this build knows about. Checksums and sizes come from the `packs-v1` release.
pub fn manifest() -> Vec<PackSpec> {
    vec![
        PackSpec {
            id: RomanLang::Zh,
            version: 1,
            label: "Chinese (pinyin)".into(),
            url: format!("{RELEASE}zh-pinyin-v1.zip"),
            sha256: "0970b5992bfed511385b32433cfa19815eb917acd73b41959576f0cb559688a7".into(),
            size: 297_410,
        },
        PackSpec {
            id: RomanLang::Ja,
            version: 1,
            label: "Japanese (romaji)".into(),
            url: format!("{RELEASE}ja-ipadic-v1.zip"),
            sha256: "14e24100598c2e5eb94fae50db37bb2c63a8ece1ae364d8e7703c3464879bb5c".into(),
            size: 10_302_523,
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PackState {
    NotInstalled,
    Installing { pct: u8 },
    Installed,
    BuiltIn,
    UpdateAvailable,
    Broken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub id: RomanLang,
    pub label: String,
    pub size: u64,
    #[serde(flatten)]
    pub state: PackState,
}

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("this pack is already being installed")]
    Busy,
    #[error("download failed: {0}")]
    Network(String),
    #[error("the download was corrupted or tampered with (checksum mismatch)")]
    Checksum,
    #[error("the pack archive is invalid: {0}")]
    BadArchive(String),
    #[error("could not write the pack: {0}")]
    Io(#[from] std::io::Error),
    #[error("download cancelled")]
    Cancelled,
    #[error("unknown language pack")]
    Unknown,
}

/// Written next to the unzipped files once a pack is installed.
#[derive(Debug, Serialize, Deserialize)]
struct PackMeta {
    id: RomanLang,
    version: u32,
    sha256: String,
}

struct Installing {
    cancel: Arc<Cancel>,
    pct: u8,
}

/// Cancellation that also wakes a download stuck waiting on the network.
#[derive(Default)]
struct Cancel {
    flag: AtomicBool,
    wake: Notify,
}

impl Cancel {
    fn set(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }
    fn is_set(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
    async fn wait(&self) {
        if !self.is_set() {
            self.wake.notified().await;
        }
    }
}

/// Rename, retrying briefly: antivirus scanners often hold freshly unzipped files on Windows.
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..5 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

pub struct PackManager {
    dir: PathBuf,
    specs: Vec<PackSpec>,
    roman: Arc<Romanizer>,
    http: reqwest::Client,
    installing: Mutex<HashMap<RomanLang, Installing>>,
    broken: Mutex<HashSet<RomanLang>>,
}

fn folder(id: RomanLang) -> &'static str {
    match id {
        RomanLang::Zh => "zh",
        RomanLang::Ja => "ja",
        RomanLang::Ko => "ko",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Unzip `zip` into `dest`, rejecting entries that would land outside it.
fn extract(zip_path: &Path, dest: &Path) -> Result<(), PackError> {
    let bad = |e: zip::result::ZipError| PackError::BadArchive(e.to_string());
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(bad)?;
    std::fs::create_dir_all(dest)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(bad)?;
        let Some(rel) = entry.enclosed_name() else {
            return Err(PackError::BadArchive(format!("unsafe path {:?}", entry.name())));
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut entry, &mut std::fs::File::create(&out)?)?;
    }
    Ok(())
}

/// Clears the installing flag however `install` ends.
struct InstallGuard<'a>(&'a Mutex<HashMap<RomanLang, Installing>>, RomanLang);
impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().expect("installing lock").remove(&self.1);
    }
}

impl PackManager {
    /// Cleans leftovers from an interrupted install, then loads every installed pack.
    pub fn new(dir: PathBuf, specs: Vec<PackSpec>, roman: Arc<Romanizer>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(crate::lyrics::lrclib::user_agent())
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(30))
            .build()
            .expect("http client");
        let m = Self { dir, specs, roman, http, installing: Mutex::new(HashMap::new()), broken: Mutex::new(HashSet::new()) };
        if let Ok(entries) = std::fs::read_dir(&m.dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.ends_with(".part") {
                    let _ = std::fs::remove_file(e.path());
                } else if name.ends_with(".tmp") {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        for id in m.specs.iter().map(|s| s.id).collect::<Vec<_>>() {
            if m.meta(id).is_some() && m.load(id).is_err() {
                m.broken.lock().expect("broken lock").insert(id);
            }
        }
        m
    }

    fn meta(&self, id: RomanLang) -> Option<PackMeta> {
        let bytes = std::fs::read(self.dir.join(folder(id)).join("pack.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn load(&self, id: RomanLang) -> Result<(), super::RomanError> {
        let dir = self.dir.join(folder(id));
        match id {
            RomanLang::Zh => self.roman.set_zh(Some(super::zh::load(&dir)?)),
            RomanLang::Ja => self.roman.set_ja(Some(super::ja::load(&dir)?)),
            RomanLang::Ko => {}
        }
        Ok(())
    }

    /// Drop the loaded data. Needed before touching the files: Windows locks memory-mapped files.
    fn unload(&self, id: RomanLang) {
        match id {
            RomanLang::Zh => self.roman.set_zh(None),
            RomanLang::Ja => self.roman.set_ja(None),
            RomanLang::Ko => {}
        }
    }

    pub fn list(&self) -> Vec<PackInfo> {
        let installing = self.installing.lock().expect("installing lock");
        let broken = self.broken.lock().expect("broken lock");
        let mut out: Vec<PackInfo> = self
            .specs
            .iter()
            .map(|s| {
                let state = if let Some(i) = installing.get(&s.id) {
                    PackState::Installing { pct: i.pct }
                } else {
                    match self.meta(s.id) {
                        None => PackState::NotInstalled,
                        Some(_) if broken.contains(&s.id) => PackState::Broken,
                        Some(m) if m.version < s.version => PackState::UpdateAvailable,
                        Some(_) => PackState::Installed,
                    }
                };
                PackInfo { id: s.id, label: s.label.clone(), size: s.size, state }
            })
            .collect();
        out.push(PackInfo { id: RomanLang::Ko, label: "Korean".into(), size: 0, state: PackState::BuiltIn });
        out
    }

    /// Download, verify, unzip and load a pack. Leaves no partial files behind on failure.
    pub async fn install(&self, id: RomanLang, on_progress: impl Fn(u8) + Send) -> Result<(), PackError> {
        let spec = self.specs.iter().find(|s| s.id == id).cloned().ok_or(PackError::Unknown)?;
        let cancel = Arc::new(Cancel::default());
        {
            let mut map = self.installing.lock().expect("installing lock");
            if map.contains_key(&id) {
                return Err(PackError::Busy);
            }
            map.insert(id, Installing { cancel: cancel.clone(), pct: 0 });
        }
        let _guard = InstallGuard(&self.installing, id);
        // Lets the caller show "installing" right away; list() already reports it.
        on_progress(0);
        let part = self.dir.join(format!("{}.part", folder(id)));
        let tmp = self.dir.join(format!("{}.tmp", folder(id)));
        let res = self.install_inner(&spec, &cancel, &part, &tmp, &on_progress).await;
        let _ = std::fs::remove_file(&part);
        let _ = std::fs::remove_dir_all(&tmp);
        res
    }

    async fn install_inner(
        &self,
        spec: &PackSpec,
        cancel: &Cancel,
        part: &Path,
        tmp: &Path,
        on_progress: &(impl Fn(u8) + Send),
    ) -> Result<(), PackError> {
        std::fs::create_dir_all(&self.dir)?;
        let net = |e: reqwest::Error| PackError::Network(e.to_string());
        let mut resp = tokio::select! {
            r = self.http.get(&spec.url).send() => r.map_err(net)?,
            _ = cancel.wait() => return Err(PackError::Cancelled),
        };
        if !resp.status().is_success() {
            return Err(PackError::Network(format!("HTTP {}", resp.status())));
        }
        let mut file = tokio::fs::File::create(part).await?;
        let mut hasher = Sha256::new();
        let mut got: u64 = 0;
        let mut last_pct = 0u8;
        loop {
            let next = tokio::select! {
                r = resp.chunk() => r.map_err(net)?,
                _ = cancel.wait() => return Err(PackError::Cancelled),
            };
            let Some(chunk) = next else { break };
            file.write_all(&chunk).await?;
            hasher.update(&chunk);
            got += chunk.len() as u64;
            let pct = got.saturating_mul(100).checked_div(spec.size).unwrap_or(0).min(99) as u8;
            if pct != last_pct {
                last_pct = pct;
                if let Some(i) = self.installing.lock().expect("installing lock").get_mut(&spec.id) {
                    i.pct = pct;
                }
                on_progress(pct);
            }
        }
        file.flush().await?;
        drop(file);
        if cancel.is_set() {
            return Err(PackError::Cancelled);
        }
        let digest = hex(&hasher.finalize());
        if digest != spec.sha256.to_ascii_lowercase() {
            return Err(PackError::Checksum);
        }
        let (zip_path, dest) = (part.to_path_buf(), tmp.to_path_buf());
        tokio::task::spawn_blocking(move || extract(&zip_path, &dest))
            .await
            .map_err(|e| PackError::BadArchive(e.to_string()))??;
        let meta = PackMeta { id: spec.id, version: spec.version, sha256: digest };
        std::fs::write(tmp.join("pack.json"), serde_json::to_vec(&meta).map_err(|e| PackError::BadArchive(e.to_string()))?)?;
        let final_dir = self.dir.join(folder(spec.id));
        self.unload(spec.id);
        let swapped = (|| {
            if final_dir.exists() {
                std::fs::remove_dir_all(&final_dir)?;
            }
            rename_with_retry(tmp, &final_dir)
        })();
        if let Err(e) = swapped {
            self.recover(spec.id);
            return Err(e.into());
        }
        if let Err(e) = self.load(spec.id) {
            self.broken.lock().expect("broken lock").insert(spec.id);
            return Err(PackError::BadArchive(e.to_string()));
        }
        self.broken.lock().expect("broken lock").remove(&spec.id);
        on_progress(100);
        Ok(())
    }

    /// Stop a running install, even one stalled waiting on the network.
    pub fn cancel(&self, id: RomanLang) {
        if let Some(i) = self.installing.lock().expect("installing lock").get(&id) {
            i.cancel.set();
        }
    }

    /// After a failed delete or swap: reload whatever is left, or mark the pack broken.
    fn recover(&self, id: RomanLang) {
        let mut broken = self.broken.lock().expect("broken lock");
        if self.meta(id).is_none() || self.load(id).is_ok() {
            broken.remove(&id);
        } else {
            broken.insert(id);
        }
    }

    pub fn remove(&self, id: RomanLang) -> Result<(), PackError> {
        if self.installing.lock().expect("installing lock").contains_key(&id) {
            return Err(PackError::Busy);
        }
        self.unload(id);
        let dir = self.dir.join(folder(id));
        if dir.exists()
            && let Err(e) = std::fs::remove_dir_all(dir)
        {
            self.recover(id);
            return Err(e.into());
        }
        self.broken.lock().expect("broken lock").remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use std::sync::Mutex;
    use std::time::Duration;

    #[test]
    fn manifest_is_pinned() {
        for p in manifest() {
            assert_eq!(p.sha256.len(), 64, "{:?} hash length", p.id);
            assert!(p.sha256.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "{:?} hash is lowercase hex", p.id);
            assert_ne!(p.sha256, "0".repeat(64), "{:?} hash is a placeholder", p.id);
            assert!(p.size > 0, "{:?} size", p.id);
            assert!(p.url.starts_with("https://github.com/Ltfrzky/OpenMusix/releases/download/packs-v1/"), "{:?} url", p.id);
        }
    }
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FIX: &str = "U+4E2D: zhōng  # 中\nU+56FD: guó  # 国\n";

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in entries {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }
    fn sha(b: &[u8]) -> String { Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect() }
    fn spec(server: &MockServer, body: &[u8]) -> PackSpec {
        PackSpec { id: RomanLang::Zh, version: 1, label: "Chinese (pinyin)".into(), url: format!("{}/zh.zip", server.uri()), sha256: sha(body), size: body.len() as u64 }
    }
    fn state(m: &PackManager, id: RomanLang) -> PackState { m.list().into_iter().find(|p| p.id == id).unwrap().state }
    async fn serve(body: Vec<u8>, delay_ms: u64) -> MockServer {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/zh.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body).set_delay(Duration::from_millis(delay_ms)))
            .mount(&s).await;
        s
    }
    fn leftovers(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".part") || n.ends_with(".tmp")).collect()).unwrap_or_default()
    }

    #[tokio::test]
    async fn install_zh_happy_path() {
        let body = make_zip(&[("pinyin.txt", FIX.as_bytes()), ("LICENSE", b"MIT")]);
        let s = serve(body.clone(), 0).await;
        let d = tempfile::tempdir().unwrap();
        let roman = Arc::new(Romanizer::new());
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], roman.clone());
        let seen = Mutex::new(Vec::new());
        m.install(RomanLang::Zh, |p| seen.lock().unwrap().push(p)).await.unwrap();
        assert_eq!(state(&m, RomanLang::Zh), PackState::Installed);
        assert!(roman.has(RomanLang::Zh));
        assert_eq!(seen.lock().unwrap().last(), Some(&100));
        assert!(leftovers(&d.path().join("packs")).is_empty());
    }

    #[tokio::test]
    async fn checksum_mismatch_rejected() {
        let body = make_zip(&[("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 0).await;
        let d = tempfile::tempdir().unwrap();
        let mut sp = spec(&s, &body);
        sp.sha256 = "00".repeat(32);
        let m = PackManager::new(d.path().join("packs"), vec![sp], Arc::new(Romanizer::new()));
        assert!(matches!(m.install(RomanLang::Zh, |_| {}).await, Err(PackError::Checksum)));
        assert!(!d.path().join("packs/zh").exists());
        assert!(leftovers(&d.path().join("packs")).is_empty());
    }

    #[tokio::test]
    async fn zip_slip_rejected() {
        let body = make_zip(&[("../evil.txt", b"x"), ("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 0).await;
        let d = tempfile::tempdir().unwrap();
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], Arc::new(Romanizer::new()));
        assert!(matches!(m.install(RomanLang::Zh, |_| {}).await, Err(PackError::BadArchive(_))));
        assert!(!d.path().join("evil.txt").exists());
        assert!(!d.path().join("packs/zh").exists());
        assert!(leftovers(&d.path().join("packs")).is_empty());
    }

    #[tokio::test]
    async fn http_404_is_network_error() {
        let s = MockServer::start().await;
        let d = tempfile::tempdir().unwrap();
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, b"x")], Arc::new(Romanizer::new()));
        assert!(matches!(m.install(RomanLang::Zh, |_| {}).await, Err(PackError::Network(_))));
        assert!(leftovers(&d.path().join("packs")).is_empty());
        assert_eq!(state(&m, RomanLang::Zh), PackState::NotInstalled);
    }

    #[tokio::test]
    async fn second_install_is_busy() {
        let body = make_zip(&[("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 300).await;
        let d = tempfile::tempdir().unwrap();
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], Arc::new(Romanizer::new()));
        let (a, b) = tokio::join!(m.install(RomanLang::Zh, |_| {}), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            m.install(RomanLang::Zh, |_| {}).await
        });
        assert!(a.is_ok(), "{a:?}");
        assert!(matches!(b, Err(PackError::Busy)));
        assert_eq!(state(&m, RomanLang::Zh), PackState::Installed);
    }

    #[tokio::test]
    async fn installing_state_is_listed() {
        let body = make_zip(&[("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 300).await;
        let d = tempfile::tempdir().unwrap();
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], Arc::new(Romanizer::new()));
        let (_, st) = tokio::join!(m.install(RomanLang::Zh, |_| {}), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            state(&m, RomanLang::Zh)
        });
        assert!(matches!(st, PackState::Installing { .. }), "{st:?}");
    }

    #[tokio::test]
    async fn cancel_mid_download() {
        let body = make_zip(&[("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 300).await;
        let d = tempfile::tempdir().unwrap();
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], Arc::new(Romanizer::new()));
        let (r, _) = tokio::join!(m.install(RomanLang::Zh, |_| {}), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            m.cancel(RomanLang::Zh)
        });
        assert!(matches!(r, Err(PackError::Cancelled)), "{r:?}");
        assert!(leftovers(&d.path().join("packs")).is_empty());
        assert!(!d.path().join("packs/zh").exists());
    }

    #[tokio::test]
    async fn cancel_interrupts_a_stalled_download() {
        let body = make_zip(&[("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 10_000).await;
        let d = tempfile::tempdir().unwrap();
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], Arc::new(Romanizer::new()));
        let started = std::time::Instant::now();
        let (r, _) = tokio::join!(m.install(RomanLang::Zh, |_| {}), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            m.cancel(RomanLang::Zh)
        });
        assert!(matches!(r, Err(PackError::Cancelled)), "{r:?}");
        assert!(started.elapsed() < Duration::from_secs(2), "cancel took {:?}", started.elapsed());
    }

    #[tokio::test]
    async fn first_progress_arrives_once_installing_is_listed() {
        let body = make_zip(&[("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 0).await;
        let d = tempfile::tempdir().unwrap();
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], Arc::new(Romanizer::new()));
        let first: Mutex<Option<(u8, PackState)>> = Mutex::new(None);
        m.install(RomanLang::Zh, |p| {
            let mut f = first.lock().unwrap();
            if f.is_none() {
                *f = Some((p, state(&m, RomanLang::Zh)));
            }
        })
        .await
        .unwrap();
        assert_eq!(first.lock().unwrap().take(), Some((0, PackState::Installing { pct: 0 })));
    }

    #[tokio::test]
    async fn failed_remove_keeps_list_and_romanizer_consistent() {
        let body = make_zip(&[("a-locked.txt", b"x"), ("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 0).await;
        let d = tempfile::tempdir().unwrap();
        let roman = Arc::new(Romanizer::new());
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], roman.clone());
        m.install(RomanLang::Zh, |_| {}).await.unwrap();
        // Another process (antivirus, indexer) holding a file open without sharing blocks deletion.
        use std::os::windows::fs::OpenOptionsExt;
        let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(d.path().join("packs/zh/a-locked.txt")).unwrap();
        let r = m.remove(RomanLang::Zh);
        let st = state(&m, RomanLang::Zh);
        drop(held);
        assert!(r.is_err(), "remove should fail on the held file");
        assert_eq!(st == PackState::Installed, roman.has(RomanLang::Zh), "listed {st:?} but loaded={}", roman.has(RomanLang::Zh));
    }

    #[test]
    fn startup_cleans_partial_install() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("packs");
        std::fs::create_dir_all(p.join("zh.tmp")).unwrap();
        std::fs::write(p.join("zh.tmp/x"), "x").unwrap();
        std::fs::write(p.join("zh.part"), "x").unwrap();
        let m = PackManager::new(p.clone(), manifest(), Arc::new(Romanizer::new()));
        assert!(leftovers(&p).is_empty());
        assert_eq!(state(&m, RomanLang::Zh), PackState::NotInstalled);
    }

    #[test]
    fn broken_pack_detected() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("packs");
        std::fs::create_dir_all(p.join("zh")).unwrap();
        std::fs::write(p.join("zh/pack.json"), r#"{"id":"zh","version":1,"sha256":"x"}"#).unwrap();
        let roman = Arc::new(Romanizer::new());
        let m = PackManager::new(p, manifest(), roman.clone());
        assert_eq!(state(&m, RomanLang::Zh), PackState::Broken);
        assert!(!roman.has(RomanLang::Zh));
    }

    #[test]
    fn installed_pack_loads_at_startup() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("packs");
        std::fs::create_dir_all(p.join("zh")).unwrap();
        std::fs::write(p.join("zh/pack.json"), r#"{"id":"zh","version":1,"sha256":"x"}"#).unwrap();
        std::fs::write(p.join("zh/pinyin.txt"), FIX).unwrap();
        let roman = Arc::new(Romanizer::new());
        let m = PackManager::new(p, manifest(), roman.clone());
        assert_eq!(state(&m, RomanLang::Zh), PackState::Installed);
        assert!(roman.has(RomanLang::Zh));
    }

    #[tokio::test]
    async fn remove_unloads() {
        let body = make_zip(&[("pinyin.txt", FIX.as_bytes())]);
        let s = serve(body.clone(), 0).await;
        let d = tempfile::tempdir().unwrap();
        let roman = Arc::new(Romanizer::new());
        let m = PackManager::new(d.path().join("packs"), vec![spec(&s, &body)], roman.clone());
        m.install(RomanLang::Zh, |_| {}).await.unwrap();
        m.remove(RomanLang::Zh).unwrap();
        assert_eq!(state(&m, RomanLang::Zh), PackState::NotInstalled);
        assert!(!roman.has(RomanLang::Zh));
        assert!(!d.path().join("packs/zh").exists());
    }

    #[test]
    fn update_available() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("packs");
        std::fs::create_dir_all(p.join("zh")).unwrap();
        std::fs::write(p.join("zh/pack.json"), r#"{"id":"zh","version":0,"sha256":"x"}"#).unwrap();
        std::fs::write(p.join("zh/pinyin.txt"), FIX).unwrap();
        let m = PackManager::new(p, manifest(), Arc::new(Romanizer::new()));
        assert_eq!(state(&m, RomanLang::Zh), PackState::UpdateAvailable);
    }

    #[test]
    fn korean_always_builtin() {
        let d = tempfile::tempdir().unwrap();
        let m = PackManager::new(d.path().join("packs"), manifest(), Arc::new(Romanizer::new()));
        let ko = m.list().into_iter().find(|p| p.id == RomanLang::Ko).unwrap();
        assert_eq!((ko.state, ko.size, ko.label.as_str()), (PackState::BuiltIn, 0, "Korean"));
    }

    #[test]
    fn pack_info_json_shape() {
        let v = serde_json::to_value(PackInfo { id: RomanLang::Ja, label: "J".into(), size: 5, state: PackState::Installing { pct: 62 } }).unwrap();
        assert_eq!(v, serde_json::json!({"id":"ja","label":"J","size":5,"state":"installing","pct":62}));
    }
}
