//! JSON storage in the app data folder (library, presets, settings).
//! File layout and JSON format match the earlier Electron version.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use crate::model::{AppSettings, Asset, AssetKind, AudioMode, Codec, Outputs, Preset, RenderSettings};

fn read_json<T: DeserializeOwned>(file: &Path) -> Option<T> {
    let text = fs::read_to_string(file).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_json<T: Serialize + ?Sized>(file: &Path, data: &T) -> Result<()> {
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(data)?)?;
    fs::rename(&tmp, file).with_context(|| format!("Gagal menyimpan {}", file.display()))?;
    Ok(())
}

/// Random-ish unique id (hex) without extra crates.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut x = (nanos as u64) ^ (c.wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ (std::process::id() as u64).rotate_left(32);
    // splitmix64 finalizer
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    format!("{x:016x}{c:04x}")
}

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

pub struct Store {
    pub root: PathBuf,
    pub library_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub temp_dir: PathBuf,
    lib_file: PathBuf,
    presets_file: PathBuf,
    settings_file: PathBuf,
    default_output_dir: PathBuf,
}

impl Store {
    pub fn open(root: PathBuf, default_output_dir: PathBuf) -> Result<Store> {
        let s = Store {
            library_dir: root.join("library"),
            cache_dir: root.join("cache"),
            temp_dir: root.join("tmp"),
            lib_file: root.join("library.json"),
            presets_file: root.join("profiles.json"),
            settings_file: root.join("settings.json"),
            default_output_dir,
            root,
        };
        for d in [&s.root, &s.library_dir, &s.cache_dir, &s.temp_dir] {
            fs::create_dir_all(d).with_context(|| format!("Tidak bisa membuat folder {}", d.display()))?;
        }
        Ok(s)
    }

    // ---------- Settings ----------
    pub fn get_settings(&self) -> AppSettings {
        let mut s: AppSettings = read_json(&self.settings_file).unwrap_or_default();
        if s.output_dir.as_os_str().is_empty() {
            s.output_dir = self.default_output_dir.clone();
        }
        if s.ffmpeg_path.is_empty() {
            s.ffmpeg_path = "ffmpeg".into();
        }
        if s.ffprobe_path.is_empty() {
            s.ffprobe_path = "ffprobe".into();
        }
        if s.file_template.is_empty() {
            s.file_template = "{name}".into();
        }
        s.concurrency = s.concurrency.clamp(1, 4);
        s
    }

    pub fn set_settings(&self, s: &AppSettings) -> Result<()> {
        write_json(&self.settings_file, s)
    }

    // ---------- Library ----------
    pub fn list_assets(&self) -> Vec<Asset> {
        read_json::<Vec<Asset>>(&self.lib_file)
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a.file.exists())
            .collect()
    }

    pub fn add_asset(
        &self,
        kind: AssetKind,
        source: &Path,
        width: Option<u32>,
        height: Option<u32>,
        duration: Option<f64>,
    ) -> Result<Asset> {
        let id = new_id();
        let dir = self.library_dir.join(kind.dir_name());
        fs::create_dir_all(&dir)?;
        let ext = source.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let file = dir.join(if ext.is_empty() { id.clone() } else { format!("{id}.{ext}") });
        fs::copy(source, &file).with_context(|| format!("Gagal menyalin {}", source.display()))?;
        let asset = Asset {
            id,
            kind,
            name: source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            file,
            width,
            height,
            duration,
            created_at: now_ms(),
        };
        let mut list = self.list_assets();
        list.push(asset.clone());
        write_json(&self.lib_file, &list)?;
        Ok(asset)
    }

    pub fn rename_asset(&self, id: &str, name: &str) -> Result<Vec<Asset>> {
        let list: Vec<Asset> = self
            .list_assets()
            .into_iter()
            .map(|mut a| {
                if a.id == id {
                    a.name = name.to_string();
                }
                a
            })
            .collect();
        write_json(&self.lib_file, &list)?;
        Ok(list)
    }

    pub fn remove_asset(&self, id: &str) -> Result<Vec<Asset>> {
        let all = self.list_assets();
        if let Some(a) = all.iter().find(|a| a.id == id) {
            let _ = fs::remove_file(&a.file);
        }
        let list: Vec<Asset> = all.into_iter().filter(|a| a.id != id).collect();
        write_json(&self.lib_file, &list)?;
        Ok(list)
    }

    pub fn asset_path(&self, id: Option<&str>) -> Option<PathBuf> {
        let id = id?;
        self.list_assets().into_iter().find(|a| a.id == id).map(|a| a.file)
    }

    // ---------- Presets (profiles.json) ----------
    pub fn list_presets(&self) -> Vec<Preset> {
        let list: Vec<Preset> = read_json(&self.presets_file).unwrap_or_default();
        if !list.is_empty() {
            return list;
        }
        let mut porto = RenderSettings::default();
        porto.outputs = Outputs { social: false, porto: true, thumbnail: true, gif: false };
        porto.intro.enabled = false;
        porto.audio.mode = AudioMode::None;
        porto.quality.porto_codec = Codec::H265;
        let defaults = vec![
            Preset { id: new_id(), name: "Sosmed + Porto".into(), settings: RenderSettings::default() },
            Preset { id: new_id(), name: "Porto saja (tanpa frame)".into(), settings: porto },
        ];
        let _ = write_json(&self.presets_file, &defaults);
        defaults
    }

    pub fn save_preset(&self, p: &Preset) -> Result<Vec<Preset>> {
        let mut list = self.list_presets();
        match list.iter_mut().find(|x| x.id == p.id) {
            Some(x) => *x = p.clone(),
            None => list.push(p.clone()),
        }
        write_json(&self.presets_file, &list)?;
        Ok(list)
    }

    pub fn remove_preset(&self, id: &str) -> Result<Vec<Preset>> {
        let list: Vec<Preset> = self.list_presets().into_iter().filter(|p| p.id != id).collect();
        write_json(&self.presets_file, &list)?;
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BgMode, Codec, TextPosition};

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("showcase-core-test-{tag}-{}", new_id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// profiles.json as written by the Electron version (camelCase, extra/missing fields).
    const ELECTRON_PROFILES: &str = r##"[
      {"id":"8f1c","name":"Sosmed + Porto","settings":{
        "resolution":{"mode":"preset","preset":"1920x1080","width":1920,"height":1080},
        "fps":0,
        "background":{"mode":"image","assetId":"abc","color":"#1d1b26","fit":"cover","blur":30},
        "video":{"scale":90,"offsetX":0,"offsetY":0},
        "frame":{"assetId":null,"social":true,"porto":false},
        "intro":{"enabled":true,"duration":0.2,"source":"timestamp","timestamp":1.5,"imagePath":null,"overlayAssetId":null,"social":true,"porto":false},
        "text":{"enabled":true,"text":"Commission for {client}","fontAssetId":null,"size":4,"color":"#ffffff","outlineColor":"#000000","outlineWidth":3,"shadow":true,"position":"bottom-right","margin":4,"social":true,"porto":false},
        "loop":{"enabled":true,"minDuration":8,"count":0},
        "audio":{"mode":"mix","assetId":null,"volume":1,"fadeIn":0.5,"fadeOut":1,"porto":false},
        "quality":{"preset":"high","portoCodec":"h265"},
        "outputs":{"social":true,"porto":true,"thumbnail":true},
        "someFutureField":123
      }},
      {"id":"x2","name":"Minimal","settings":{"background":{"mode":"blur"}}}
    ]"##;

    #[test]
    fn loads_electron_profiles_json() {
        let list: Vec<Preset> = serde_json::from_str(ELECTRON_PROFILES).unwrap();
        let s = &list[0].settings;
        assert_eq!(s.background.mode, BgMode::Image);
        assert_eq!(s.background.asset_id.as_deref(), Some("abc"));
        assert_eq!(s.text.position, TextPosition::BottomRight);
        assert_eq!(s.loop_.min_duration, 8.0);
        assert_eq!(s.audio.mode, AudioMode::Mix);
        assert_eq!(s.quality.porto_codec, Codec::H265);
        assert_eq!(s.intro.timestamp, 1.5);
        // Missing fields fall back to defaults
        let m = &list[1].settings;
        assert_eq!(m.background.mode, BgMode::Blur);
        assert_eq!(m.background.blur, 30.0);
        assert_eq!(m.resolution.preset, "1920x1080");
        assert!(m.outputs.social && m.outputs.porto);
    }

    #[test]
    fn roundtrip_keeps_camel_case() {
        let json = serde_json::to_string(&RenderSettings::default()).unwrap();
        assert!(json.contains("\"loop\":"));
        assert!(json.contains("\"minDuration\""));
        assert!(json.contains("\"portoCodec\":\"h264\""));
        assert!(json.contains("\"position\":\"bottom\""));
    }

    #[test]
    fn electron_settings_and_library() {
        let root = temp_root("compat");
        let src = root.join("bg.png");
        fs::write(&src, b"fake").unwrap();
        fs::write(
            root.join("settings.json"),
            r#"{"ffmpegPath":"C:/ff/ffmpeg.exe","ffprobePath":"ffprobe","outputDir":"D:/out","fileTemplate":"{name}_{date}","concurrency":2,"defaultProfileId":"8f1c"}"#,
        )
        .unwrap();
        let store = Store::open(root.clone(), root.join("videos")).unwrap();
        let st = store.get_settings();
        assert_eq!(st.ffmpeg_path, "C:/ff/ffmpeg.exe");
        assert_eq!(st.output_dir, PathBuf::from("D:/out"));
        assert_eq!(st.concurrency, 2);
        assert_eq!(st.default_profile_id.as_deref(), Some("8f1c"));

        let a = store.add_asset(AssetKind::Background, &src, Some(1920), Some(1080), None).unwrap();
        assert!(a.file.exists());
        assert_eq!(a.name, "bg");
        assert_eq!(store.asset_path(Some(&a.id)), Some(a.file.clone()));
        let renamed = store.rename_asset(&a.id, "Langit").unwrap();
        assert_eq!(renamed[0].name, "Langit");
        let json = fs::read_to_string(root.join("library.json")).unwrap();
        assert!(json.contains("\"kind\": \"background\"") && json.contains("\"createdAt\""));
        store.remove_asset(&a.id).unwrap();
        assert!(!a.file.exists());
        assert!(store.list_assets().is_empty());

        let presets = store.list_presets();
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[1].name, "Porto saja (tanpa frame)");
        assert!(!presets[1].settings.outputs.social);
        let mut p = presets[0].clone();
        p.name = "Twitter".into();
        assert_eq!(store.save_preset(&p).unwrap()[0].name, "Twitter");
        assert_eq!(store.remove_preset(&p.id).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn default_settings_fill_output_dir() {
        let root = temp_root("defaults");
        let store = Store::open(root.clone(), PathBuf::from("V:/Videos/CS")).unwrap();
        let s = store.get_settings();
        assert_eq!(s.output_dir, PathBuf::from("V:/Videos/CS"));
        assert_eq!(s.ffmpeg_path, "ffmpeg");
        assert_eq!(s.concurrency, 1);
        let _ = fs::remove_dir_all(&root);
    }
}
