//! Data model. JSON field names stay camelCase so data saved by the earlier
//! Electron version (%APPDATA%\CommissionShowcase) keeps loading.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Background,
    Frame,
    Overlay,
    Audio,
    Font,
}

impl AssetKind {
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Frame => "frame",
            Self::Overlay => "overlay",
            Self::Audio => "audio",
            Self::Font => "font",
        }
    }
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Background | Self::Overlay => &["png", "jpg", "jpeg", "webp"],
            Self::Frame => &["png", "webp"],
            Self::Audio => &["mp3", "wav", "ogg", "m4a", "aac", "flac"],
            Self::Font => &["ttf", "otf"],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub kind: AssetKind,
    pub name: String,
    pub file: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProbeInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub fps: f64,
    pub vcodec: String,
    pub pix_fmt: String,
    pub has_alpha: bool,
    /// WebM VP8/VP9 with alpha must be decoded with libvpx or the alpha is dropped.
    pub needs_libvpx: Option<&'static str>,
    pub has_audio: bool,
    pub format_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextPosition {
    TopLeft,
    Top,
    TopRight,
    Center,
    BottomLeft,
    Bottom,
    BottomRight,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextOverlay {
    pub enabled: bool,
    /// Placeholders: {client} {filename} {date} {profile}
    pub text: String,
    pub font_asset_id: Option<String>,
    /// Font size in % of output height
    pub size: f32,
    pub color: String,
    pub outline_color: String,
    pub outline_width: f32,
    pub shadow: bool,
    pub position: TextPosition,
    /// Margin in % of output height
    pub margin: f32,
    pub social: bool,
    pub porto: bool,
}

impl Default for TextOverlay {
    fn default() -> Self {
        Self {
            enabled: false,
            text: "Commission for {client}".into(),
            font_asset_id: None,
            size: 4.0,
            color: "#ffffff".into(),
            outline_color: "#000000".into(),
            outline_width: 3.0,
            shadow: true,
            position: TextPosition::Bottom,
            margin: 4.0,
            social: true,
            porto: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolutionMode {
    Preset,
    Source,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Resolution {
    pub mode: ResolutionMode,
    pub preset: String,
    pub width: u32,
    pub height: u32,
}
impl Default for Resolution {
    fn default() -> Self {
        Self { mode: ResolutionMode::Preset, preset: "1920x1080".into(), width: 1920, height: 1080 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BgMode {
    Image,
    Color,
    Blur,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    Cover,
    Contain,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Background {
    pub mode: BgMode,
    pub asset_id: Option<String>,
    pub color: String,
    pub fit: Fit,
    pub blur: f32,
}
impl Default for Background {
    fn default() -> Self {
        Self { mode: BgMode::Color, asset_id: None, color: "#1d1b26".into(), fit: Fit::Cover, blur: 30.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoPlacement {
    /// 100 = fit inside output
    pub scale: f32,
    /// Offset in % of output width/height
    pub offset_x: f32,
    pub offset_y: f32,
}
impl Default for VideoPlacement {
    fn default() -> Self {
        Self { scale: 90.0, offset_x: 0.0, offset_y: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FrameSettings {
    pub asset_id: Option<String>,
    pub social: bool,
    pub porto: bool,
}
impl Default for FrameSettings {
    fn default() -> Self {
        Self { asset_id: None, social: true, porto: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntroSource {
    Timestamp,
    Image,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Intro {
    pub enabled: bool,
    /// Seconds of static frames at the start
    pub duration: f32,
    pub source: IntroSource,
    pub timestamp: f64,
    pub image_path: Option<PathBuf>,
    pub overlay_asset_id: Option<String>,
    pub social: bool,
    pub porto: bool,
}
impl Default for Intro {
    fn default() -> Self {
        Self {
            enabled: true,
            duration: 0.2,
            source: IntroSource::Timestamp,
            timestamp: 0.0,
            image_path: None,
            overlay_asset_id: None,
            social: true,
            porto: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LoopSettings {
    pub enabled: bool,
    /// Repeat whole plays until at least this many seconds
    pub min_duration: f32,
    /// 0 = automatic from min_duration, >0 = total plays
    pub count: u32,
}
impl Default for LoopSettings {
    fn default() -> Self {
        Self { enabled: true, min_duration: 8.0, count: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    None,
    Original,
    Replace,
    Mix,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioSettings {
    pub mode: AudioMode,
    pub asset_id: Option<String>,
    pub volume: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub porto: bool,
}
impl Default for AudioSettings {
    fn default() -> Self {
        Self { mode: AudioMode::Original, asset_id: None, volume: 1.0, fade_in: 0.5, fade_out: 1.0, porto: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QualityPreset {
    Small,
    Balanced,
    High,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    H265,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Quality {
    pub preset: QualityPreset,
    pub porto_codec: Codec,
}
impl Default for Quality {
    fn default() -> Self {
        Self { preset: QualityPreset::Balanced, porto_codec: Codec::H264 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Outputs {
    pub social: bool,
    pub porto: bool,
    pub thumbnail: bool,
    /// Also export an animated GIF (made in the same ffmpeg pass)
    pub gif: bool,
}
impl Default for Outputs {
    fn default() -> Self {
        Self { social: true, porto: true, thumbnail: true, gif: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GifSource {
    Social,
    Porto,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GifSettings {
    /// Which version the GIF is made from
    pub source: GifSource,
    /// Width in px (height follows the aspect ratio)
    pub width: u32,
    pub fps: u32,
}
impl Default for GifSettings {
    fn default() -> Self {
        Self { source: GifSource::Social, width: 640, fps: 15 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RenderSettings {
    pub resolution: Resolution,
    /// 0 = follow source
    pub fps: f64,
    pub background: Background,
    pub video: VideoPlacement,
    pub frame: FrameSettings,
    pub intro: Intro,
    pub text: TextOverlay,
    #[serde(rename = "loop")]
    pub loop_: LoopSettings,
    pub audio: AudioSettings,
    pub quality: Quality,
    pub outputs: Outputs,
    pub gif: GifSettings,
}

/// A saved style ("preset"). Stored in profiles.json for compatibility.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub settings: RenderSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub output_dir: PathBuf,
    /// Placeholders: {name} {profile} {date}
    pub file_template: String,
    pub concurrency: u32,
    pub default_profile_id: Option<String>,
}
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ffmpeg_path: "ffmpeg".into(),
            ffprobe_path: "ffprobe".into(),
            output_dir: PathBuf::new(),
            file_template: "{name}".into(),
            concurrency: 1,
            default_profile_id: None,
        }
    }
}

pub struct ResolutionPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub width: u32,
    pub height: u32,
}

pub const RESOLUTION_PRESETS: &[ResolutionPreset] = &[
    ResolutionPreset { id: "1920x1080", label: "16:9 · 1920×1080", width: 1920, height: 1080 },
    ResolutionPreset { id: "2560x1440", label: "16:9 · 2560×1440", width: 2560, height: 1440 },
    ResolutionPreset { id: "1280x720", label: "16:9 · 1280×720", width: 1280, height: 720 },
    ResolutionPreset { id: "1080x1920", label: "9:16 · 1080×1920", width: 1080, height: 1920 },
    ResolutionPreset { id: "1080x1080", label: "1:1 · 1080×1080", width: 1080, height: 1080 },
    ResolutionPreset { id: "1080x1350", label: "4:5 · 1080×1350", width: 1080, height: 1350 },
];

pub const VIDEO_EXTENSIONS: &[&str] = &["webm", "mov", "mp4", "mkv", "avi", "gif", "apng", "m4v"];

/// Resolved library file paths handed to the ffmpeg graph builder.
#[derive(Clone, Debug, Default)]
pub struct ResolvedAssets {
    pub background: Option<PathBuf>,
    pub frame: Option<PathBuf>,
    pub intro_overlay: Option<PathBuf>,
    pub font: Option<PathBuf>,
    pub audio: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputKind {
    Social,
    Porto,
}

impl OutputKind {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Social => "social",
            Self::Porto => "porto",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RenderResult {
    pub outputs: Vec<(PathBuf, u64)>,
}
