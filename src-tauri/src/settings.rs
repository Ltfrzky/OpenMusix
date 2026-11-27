//! Persistent settings (JSON in the app data dir). Every struct defaults field-by-field.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

pub const DEFAULT_PORT: u16 = 7878;
pub const OFFSET_LIMIT_MS: i32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Focus,
    Widget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextEffect {
    None,
    Outline,
    #[default]
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Background {
    #[default]
    Transparent,
    Panel,
}

/// Where romanized text goes relative to the original line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RomanDisplay {
    #[default]
    Below,
    Replace,
}

/// Romanization of Chinese, Japanese and Korean lyrics. Off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Romanize {
    pub enabled: bool,
    pub display: RomanDisplay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub preset: String,
    pub font_family: String,
    pub font_size_px: u16,
    pub text_color: String,
    pub highlight_color: String,
    pub dim_color: String,
    pub effect: TextEffect,
    pub align: Align,
    pub line_count: u8,
    pub show_art: bool,
    pub background: Background,
}

impl Style {
    /// Must equal `PRESETS.spotlight` in `src/lib/presets.ts`.
    pub fn spotlight() -> Self {
        Self {
            preset: "spotlight".into(),
            font_family: "Outfit".into(),
            font_size_px: 44,
            text_color: "#FFFFFF".into(),
            highlight_color: "#FFFFFF".into(),
            dim_color: "rgba(255,255,255,0.45)".into(),
            effect: TextEffect::Shadow,
            align: Align::Center,
            line_count: 7,
            show_art: false,
            background: Background::Transparent,
        }
    }
    /// Must equal `PRESETS.glass` in `src/lib/presets.ts`.
    pub fn glass() -> Self {
        Self {
            preset: "glass".into(),
            font_family: "Inter".into(),
            font_size_px: 22,
            text_color: "#F4F4F5".into(),
            highlight_color: "#FFFFFF".into(),
            dim_color: "rgba(244,244,245,0.55)".into(),
            effect: TextEffect::None,
            align: Align::Left,
            line_count: 3,
            show_art: true,
            background: Background::Panel,
        }
    }
    fn clamp(&mut self) {
        self.line_count = self.line_count.clamp(1, 9);
        self.font_size_px = self.font_size_px.clamp(10, 160);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Styles {
    pub focus: Style,
    pub widget: Style,
}

impl Default for Styles {
    fn default() -> Self {
        Self { focus: Style::spotlight(), widget: Style::glass() }
    }
}

/// Overlay a partial JSON object onto a default value, field by field.
fn merge_onto<T: Serialize + serde::de::DeserializeOwned>(base: T, patch: Option<serde_json::Value>) -> T {
    let Some(serde_json::Value::Object(patch)) = patch else { return base };
    let mut v = serde_json::to_value(&base).expect("serializable");
    if let serde_json::Value::Object(m) = &mut v {
        for (k, val) in patch {
            m.insert(k, val);
        }
    }
    serde_json::from_value(v).unwrap_or(base)
}

impl<'de> Deserialize<'de> for Styles {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            focus: Option<serde_json::Value>,
            widget: Option<serde_json::Value>,
        }
        let r = Raw::deserialize(d)?;
        Ok(Styles { focus: merge_onto(Style::spotlight(), r.focus), widget: merge_onto(Style::glass(), r.widget) })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub port: u16,
    pub hotkey: String,
    pub mode: Mode,
    pub global_offset_ms: i32,
    pub track_offsets_ms: HashMap<String, i32>,
    pub styles: Styles,
    pub show_plain_lyrics: bool,
    pub hide_when_idle: bool,
    pub start_with_windows: bool,
    pub minimize_to_tray: bool,
    pub floating_window: bool,
    pub spotify_client_id: Option<String>,
    pub romanize: Romanize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            hotkey: "Ctrl+Alt+L".into(),
            mode: Mode::Focus,
            global_offset_ms: 0,
            track_offsets_ms: HashMap::new(),
            styles: Styles::default(),
            show_plain_lyrics: false,
            hide_when_idle: false,
            start_with_windows: false,
            minimize_to_tray: true,
            floating_window: false,
            spotify_client_id: None,
            romanize: Romanize::default(),
        }
    }
}

impl Settings {
    fn clamp(&mut self) {
        self.global_offset_ms = self.global_offset_ms.clamp(-OFFSET_LIMIT_MS, OFFSET_LIMIT_MS);
        for v in self.track_offsets_ms.values_mut() {
            *v = (*v).clamp(-OFFSET_LIMIT_MS, OFFSET_LIMIT_MS);
        }
        self.styles.focus.clamp();
        self.styles.widget.clamp();
        if self.port == 0 {
            self.port = DEFAULT_PORT;
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<Settings>,
}

impl SettingsStore {
    /// Never fails: a missing file gives defaults; a corrupt one is copied to `.bak` first.
    pub fn load(path: PathBuf) -> Self {
        let settings = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
                Ok(mut s) => {
                    s.clamp();
                    s
                }
                Err(e) => {
                    tracing::warn!("settings unreadable ({e}); backing up and using defaults");
                    let mut bak = path.clone().into_os_string();
                    bak.push(".bak");
                    let _ = std::fs::write(PathBuf::from(bak), &bytes);
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        Self { path, current: RwLock::new(settings) }
    }

    pub fn get(&self) -> Settings {
        self.current.read().expect("settings lock").clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> std::io::Result<Settings> {
        let mut guard = self.current.write().expect("settings lock");
        let mut next = guard.clone();
        f(&mut next);
        next.clamp();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&next)?)?;
        std::fs::rename(&tmp, &self.path)?;
        *guard = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn missing_file_gives_defaults() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(SettingsStore::load(d.path().join("settings.json")).get().port, 7878);
    }
    #[test] fn missing_fields_default() {
        let d = tempfile::tempdir().unwrap(); let p = d.path().join("settings.json");
        std::fs::write(&p, r#"{"port":9000,"styles":{"widget":{"fontSizePx":30}}}"#).unwrap();
        let s = SettingsStore::load(p).get();
        assert_eq!(s.port, 9000);
        assert_eq!(s.hotkey, "Ctrl+Alt+L");
        assert_eq!(s.styles.focus.line_count, 7);
        assert_eq!(s.styles.widget.font_size_px, 30);
        assert_eq!(s.styles.widget.preset, "glass");
    }
    #[test] fn corrupt_file_backed_up() {
        let d = tempfile::tempdir().unwrap(); let p = d.path().join("settings.json");
        std::fs::write(&p, "{not json").unwrap();
        let s = SettingsStore::load(p.clone()).get();
        assert_eq!(s, Settings::default());
        assert_eq!(std::fs::read_to_string(d.path().join("settings.json.bak")).unwrap(), "{not json");
    }
    #[test] fn offset_clamped() {
        let d = tempfile::tempdir().unwrap();
        let st = SettingsStore::load(d.path().join("settings.json"));
        assert_eq!(st.update(|s| s.global_offset_ms = 5000).unwrap().global_offset_ms, 2000);
        assert_eq!(st.update(|s| s.styles.focus.line_count = 0).unwrap().styles.focus.line_count, 1);
    }
    #[test] fn roundtrip() {
        let d = tempfile::tempdir().unwrap(); let p = d.path().join("settings.json");
        let st = SettingsStore::load(p.clone());
        let saved = st.update(|s| { s.mode = Mode::Widget; s.track_offsets_ms.insert("a|b".into(), 300); }).unwrap();
        assert_eq!(SettingsStore::load(p).get(), saved);
    }
    #[test] fn romanize_defaults_off_below() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.romanize, Romanize { enabled: false, display: RomanDisplay::Below });
    }
    #[test] fn romanize_partial_patch_keeps_display() {
        let s: Settings = serde_json::from_str(r#"{"romanize":{"enabled":true}}"#).unwrap();
        assert!(s.romanize.enabled);
        assert_eq!(s.romanize.display, RomanDisplay::Below);
    }
    #[test] fn romanize_json_shape() {
        let v = serde_json::to_value(Romanize { enabled: true, display: RomanDisplay::Replace }).unwrap();
        assert_eq!(v, serde_json::json!({"enabled": true, "display": "replace"}));
    }
}
