//! Application state and behaviour. Rendering of the three panels lives in
//! `ui_queue.rs`, `ui_stage.rs` and `ui_inspector.rs` (separate `impl AppView` blocks).

use crate::player::{self, StreamHandle};
use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_component::{Theme, WindowExt, notification::Notification};
use gpui_ce_motion::MotionState;
use showcase_core::model::*;
use showcase_core::model::Asset;
use showcase_core::render::{self, Bins, CancelToken, RenderJob};
use showcase_core::store::Store;
use showcase_core::{probe, util};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JobStatus {
    Waiting,
    Rendering,
    Done,
    Failed,
    Canceled,
}

pub struct Job {
    pub id: String,
    pub input: PathBuf,
    pub name: String,
    pub client: String,
    pub preset_id: String,
    /// Per-video thumbnail choice (overrides the preset)
    pub thumb_time: f64,
    pub thumb_image: Option<PathBuf>,
    pub probe: Option<ProbeInfo>,
    pub status: JobStatus,
    pub progress: f32,
    pub outputs: Vec<(PathBuf, u64)>,
    pub error: Option<String>,
    pub cancel: Option<CancelToken>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StageMode {
    Social,
    Porto,
    Thumbnail,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tab {
    Background,
    Frame,
    Thumbnail,
    Text,
    Audio,
    Export,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SliderId {
    Scale,
    OffsetX,
    OffsetY,
    Blur,
    IntroDuration,
    TextSize,
    TextMargin,
    Outline,
    Volume,
    FadeIn,
    FadeOut,
    MinLoop,
}

impl SliderId {
    pub const ALL: [SliderId; 12] = [
        Self::Scale,
        Self::OffsetX,
        Self::OffsetY,
        Self::Blur,
        Self::IntroDuration,
        Self::TextSize,
        Self::TextMargin,
        Self::Outline,
        Self::Volume,
        Self::FadeIn,
        Self::FadeOut,
        Self::MinLoop,
    ];
    fn range(self) -> (f32, f32, f32) {
        match self {
            Self::Scale => (10.0, 200.0, 1.0),
            Self::OffsetX | Self::OffsetY => (-50.0, 50.0, 0.5),
            Self::Blur => (4.0, 100.0, 1.0),
            Self::IntroDuration => (0.04, 3.0, 0.02),
            Self::TextSize => (1.0, 20.0, 0.1),
            Self::TextMargin => (0.0, 20.0, 0.5),
            Self::Outline => (0.0, 12.0, 0.5),
            Self::Volume => (0.0, 2.0, 0.05),
            Self::FadeIn | Self::FadeOut => (0.0, 5.0, 0.1),
            Self::MinLoop => (1.0, 60.0, 0.5),
        }
    }
    pub fn get(self, s: &RenderSettings) -> f32 {
        match self {
            Self::Scale => s.video.scale,
            Self::OffsetX => s.video.offset_x,
            Self::OffsetY => s.video.offset_y,
            Self::Blur => s.background.blur,
            Self::IntroDuration => s.intro.duration,
            Self::TextSize => s.text.size,
            Self::TextMargin => s.text.margin,
            Self::Outline => s.text.outline_width,
            Self::Volume => s.audio.volume,
            Self::FadeIn => s.audio.fade_in,
            Self::FadeOut => s.audio.fade_out,
            Self::MinLoop => s.loop_.min_duration,
        }
    }
    fn set(self, s: &mut RenderSettings, v: f32) {
        match self {
            Self::Scale => s.video.scale = v,
            Self::OffsetX => s.video.offset_x = v,
            Self::OffsetY => s.video.offset_y = v,
            Self::Blur => s.background.blur = v,
            Self::IntroDuration => s.intro.duration = v,
            Self::TextSize => s.text.size = v,
            Self::TextMargin => s.text.margin = v,
            Self::Outline => s.text.outline_width = v,
            Self::Volume => s.audio.volume = v,
            Self::FadeIn => s.audio.fade_in = v,
            Self::FadeOut => s.audio.fade_out = v,
            Self::MinLoop => s.loop_.min_duration = v,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum InputId {
    Client,
    Name,
    Text,
    BgHex,
    TextHex,
    PresetName,
    FfmpegPath,
    FfprobePath,
    Template,
}

pub struct Preview {
    pub frame: Option<Arc<RenderImage>>,
    pub time: f64,
    pub playing: bool,
    stream: Option<StreamHandle>,
    generation: u64,
    pub blur_bg: Option<Arc<RenderImage>>,
    /// Images no longer shown; released from GPUI's sprite atlas on the next render.
    pub retired: Vec<Arc<RenderImage>>,
    blur_key: String,
}

pub struct AppView {
    pub store: Store,
    pub settings: AppSettings,
    pub assets: Vec<Asset>,
    pub presets: Vec<Preset>,
    pub jobs: Vec<Job>,
    pub selected: Option<String>,
    /// Preset shown in the inspector when no video is selected
    pub editing_preset: Option<String>,
    pub stage: StageMode,
    pub tab: Tab,
    pub batch_running: bool,
    stop_requested: bool,
    pub ffmpeg: Option<Result<String, String>>,
    pub preview: Preview,
    pub filmstrips: HashMap<String, Vec<(f64, Arc<RenderImage>)>>,
    pub font_families: HashMap<String, String>,
    pub motions: HashMap<String, MotionState>,
    pub sliders: HashMap<SliderId, Entity<SliderState>>,
    pub inputs: HashMap<InputId, Entity<InputState>>,
    pub show_settings: bool,
    pub confirm_delete: Option<String>,
    syncing: bool,
    save_generation: u64,
    _subscriptions: Vec<Subscription>,
}

fn uid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_micros();
    format!("{:x}{:x}", t, N.fetch_add(1, Ordering::Relaxed))
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = Store::open(util::app_data_root(), util::default_output_dir()).expect("cannot open app data");
        let settings = store.get_settings();
        let assets = store.list_assets();
        let presets = store.list_presets();
        let mut subs = Vec::new();

        let mut sliders = HashMap::new();
        for id in SliderId::ALL {
            let (min, max, step) = id.range();
            let state = cx.new(|_| SliderState::new().min(min).max(max).step(step).default_value(min));
            subs.push(cx.subscribe_in(&state, window, move |this: &mut Self, _, ev: &SliderEvent, _, cx| {
                let SliderEvent::Change(SliderValue::Single(v)) = ev else { return };
                if this.syncing {
                    return;
                }
                this.edit_settings(cx, |s| id.set(s, *v));
            }));
            sliders.insert(id, state);
        }

        let mut inputs = HashMap::new();
        let placeholders = [
            (InputId::Client, "Nama klien, mis. @klien"),
            (InputId::Name, "Nama file hasil"),
            (InputId::Text, "Commission for {client}"),
            (InputId::BgHex, "#1d1b26"),
            (InputId::TextHex, "#ffffff"),
            (InputId::PresetName, "Nama gaya"),
            (InputId::FfmpegPath, "ffmpeg"),
            (InputId::FfprobePath, "ffprobe"),
            (InputId::Template, "{name}"),
        ];
        for (id, ph) in placeholders {
            let state = cx.new(|cx| InputState::new(window, cx).placeholder(ph));
            subs.push(cx.subscribe_in(&state, window, move |this: &mut Self, state, ev: &InputEvent, _, cx| {
                if !matches!(ev, InputEvent::Change) || this.syncing {
                    return;
                }
                let v = state.read(cx).value().to_string();
                this.on_input(id, v, cx);
            }));
            inputs.insert(id, state);
        }

        subs.push(cx.observe_window_appearance(window, |_, window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
        }));

        let mut this = Self {
            store,
            settings,
            assets,
            presets,
            jobs: vec![],
            selected: None,
            editing_preset: None,
            stage: StageMode::Social,
            tab: Tab::Background,
            batch_running: false,
            stop_requested: false,
            ffmpeg: None,
            preview: Preview { frame: None, time: 0.0, playing: true, stream: None, generation: 0, blur_bg: None, blur_key: String::new(), retired: Vec::new() },
            filmstrips: HashMap::new(),
            font_families: HashMap::new(),
            motions: HashMap::new(),
            sliders,
            inputs,
            show_settings: false,
            confirm_delete: None,
            syncing: false,
            save_generation: 0,
            _subscriptions: subs,
        };
        this.editing_preset = this.default_preset_id();
        this.load_fonts(cx);
        this.sync_controls(window, cx);
        this.check_ffmpeg(cx);
        // Files passed on the command line / dropped onto the .exe icon.
        let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).filter(|p| p.exists()).collect();
        if !args.is_empty() {
            this.add_videos(args, window, cx);
        }
        this
    }

    // ---------- lookups ----------
    pub fn job(&self) -> Option<&Job> {
        self.selected.as_ref().and_then(|id| self.jobs.iter().find(|j| &j.id == id))
    }
    fn job_mut(&mut self, id: &str) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }
    pub fn default_preset_id(&self) -> Option<String> {
        self.settings
            .default_profile_id
            .clone()
            .filter(|id| self.presets.iter().any(|p| &p.id == id))
            .or_else(|| self.presets.first().map(|p| p.id.clone()))
    }
    /// The preset shown/edited in the inspector.
    pub fn current_preset_id(&self) -> Option<String> {
        self.job().map(|j| j.preset_id.clone()).or_else(|| self.editing_preset.clone())
    }
    pub fn current_preset(&self) -> Option<&Preset> {
        let id = self.current_preset_id()?;
        self.presets.iter().find(|p| p.id == id)
    }
    pub fn preset_usage(&self, id: &str) -> usize {
        self.jobs.iter().filter(|j| j.preset_id == id).count()
    }
    pub fn asset(&self, id: Option<&str>) -> Option<&Asset> {
        id.and_then(|id| self.assets.iter().find(|a| a.id == id))
    }
    pub fn assets_of(&self, kind: AssetKind) -> Vec<&Asset> {
        self.assets.iter().filter(|a| a.kind == kind).collect()
    }
    pub fn bins(&self) -> Bins {
        Bins { ffmpeg: self.settings.ffmpeg_path.clone(), ffprobe: self.settings.ffprobe_path.clone() }
    }

    /// Preset settings + the per-video thumbnail choice.
    pub fn effective_settings(&self, job: &Job) -> Option<RenderSettings> {
        let p = self.presets.iter().find(|p| p.id == job.preset_id)?;
        let mut s = p.settings.clone();
        s.intro.timestamp = job.thumb_time;
        if let Some(img) = &job.thumb_image {
            s.intro.source = IntroSource::Image;
            s.intro.image_path = Some(img.clone());
        } else {
            s.intro.source = IntroSource::Timestamp;
        }
        Some(s)
    }

    pub fn motion(&mut self, key: impl Into<String>) -> &mut MotionState {
        self.motions.entry(key.into()).or_default()
    }

    // ---------- settings editing ----------
    pub fn edit_settings(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut RenderSettings)) {
        let Some(id) = self.current_preset_id() else { return };
        let Some(p) = self.presets.iter_mut().find(|p| p.id == id) else { return };
        let before = p.settings.clone();
        f(&mut p.settings);
        if p.settings == before {
            return;
        }
        let blur_changed = before.background.blur != p.settings.background.blur
            || before.background.mode != p.settings.background.mode
            || before.background.color != p.settings.background.color
            || before.resolution != p.settings.resolution;
        for j in self.jobs.iter_mut().filter(|j| j.preset_id == id) {
            if matches!(j.status, JobStatus::Done | JobStatus::Failed | JobStatus::Canceled) {
                j.status = JobStatus::Waiting;
                j.progress = 0.0;
            }
        }
        if blur_changed {
            self.preview.blur_key.clear();
            self.refresh_blur(cx);
        }
        self.schedule_save(cx);
        cx.notify();
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save_generation += 1;
        let generation = self.save_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(400)).await;
            let _ = this.update(cx, |this, _| {
                if this.save_generation == generation {
                    if let Some(p) = this.current_preset().cloned() {
                        let _ = this.store.save_preset(&p);
                    }
                }
            });
        })
        .detach();
    }

    pub fn edit_job(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Job)) {
        let Some(id) = self.selected.clone() else { return };
        if let Some(j) = self.job_mut(&id) {
            f(j);
            if matches!(j.status, JobStatus::Done | JobStatus::Failed | JobStatus::Canceled) {
                j.status = JobStatus::Waiting;
                j.progress = 0.0;
            }
        }
        cx.notify();
    }

    fn on_input(&mut self, id: InputId, v: String, cx: &mut Context<Self>) {
        match id {
            InputId::Client => self.edit_job(cx, |j| j.client = v),
            InputId::Name => self.edit_job(cx, |j| j.name = v),
            InputId::Text => self.edit_settings(cx, |s| s.text.text = v),
            InputId::BgHex => {
                if showcase_core::layout::parse_hex_color(&v).is_some() {
                    self.edit_settings(cx, |s| s.background.color = v)
                }
            }
            InputId::TextHex => {
                if showcase_core::layout::parse_hex_color(&v).is_some() {
                    self.edit_settings(cx, |s| s.text.color = v)
                }
            }
            InputId::PresetName => {
                if let Some(pid) = self.current_preset_id() {
                    if let Some(p) = self.presets.iter_mut().find(|p| p.id == pid) {
                        p.name = v;
                    }
                    self.schedule_save(cx);
                    cx.notify();
                }
            }
            InputId::FfmpegPath | InputId::FfprobePath | InputId::Template => {}
        }
    }

    /// Push the current job/preset values into the input and slider widgets.
    pub fn sync_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.syncing = true;
        let settings = self.current_preset().map(|p| p.settings.clone()).unwrap_or_default();
        let preset_name = self.current_preset().map(|p| p.name.clone()).unwrap_or_default();
        let (client, name) = self.job().map(|j| (j.client.clone(), j.name.clone())).unwrap_or_default();
        for id in SliderId::ALL {
            let v = id.get(&settings);
            self.sliders[&id].update(cx, |s, cx| s.set_value(v, window, cx));
        }
        let values = [
            (InputId::Client, client),
            (InputId::Name, name),
            (InputId::Text, settings.text.text.clone()),
            (InputId::BgHex, settings.background.color.clone()),
            (InputId::TextHex, settings.text.color.clone()),
            (InputId::PresetName, preset_name),
            (InputId::FfmpegPath, self.settings.ffmpeg_path.clone()),
            (InputId::FfprobePath, self.settings.ffprobe_path.clone()),
            (InputId::Template, self.settings.file_template.clone()),
        ];
        for (id, v) in values {
            self.inputs[&id].update(cx, |s, cx| s.set_value(v, window, cx));
        }
        self.syncing = false;
    }

    // ---------- presets ----------
    pub fn assign_preset(&mut self, preset_id: &str, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        let sel = self.selected.clone();
        for j in self.jobs.iter_mut() {
            if (all || Some(&j.id) == sel.as_ref()) && j.status != JobStatus::Rendering {
                j.preset_id = preset_id.to_string();
                j.status = JobStatus::Waiting;
                j.progress = 0.0;
            }
        }
        if self.selected.is_none() {
            self.editing_preset = Some(preset_id.to_string());
        }
        self.preview.blur_key.clear();
        self.sync_controls(window, cx);
        self.refresh_preview(cx);
        cx.notify();
    }

    pub fn duplicate_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(src) = self.current_preset().cloned() else { return };
        let p = Preset { id: uid(), name: format!("{} (salinan)", src.name), settings: src.settings };
        let _ = self.store.save_preset(&p);
        self.presets.push(p.clone());
        self.assign_preset(&p.id, false, window, cx);
    }

    pub fn delete_preset(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.presets.len() <= 1 {
            return;
        }
        let _ = self.store.remove_preset(id);
        self.presets.retain(|p| p.id != id);
        let fallback = self.presets[0].id.clone();
        for j in self.jobs.iter_mut().filter(|j| j.preset_id == id) {
            j.preset_id = fallback.clone();
        }
        if self.editing_preset.as_deref() == Some(id) {
            self.editing_preset = Some(fallback.clone());
        }
        if self.settings.default_profile_id.as_deref() == Some(id) {
            self.settings.default_profile_id = Some(fallback);
            let _ = self.store.set_settings(&self.settings);
        }
        self.confirm_delete = None;
        self.sync_controls(window, cx);
        cx.notify();
    }

    pub fn set_default_preset(&mut self, cx: &mut Context<Self>) {
        self.settings.default_profile_id = self.current_preset_id();
        let _ = self.store.set_settings(&self.settings);
        cx.notify();
    }

    // ---------- library ----------
    pub fn add_assets(&mut self, kind: AssetKind, paths: Vec<PathBuf>, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        let ffprobe = self.settings.ffprobe_path.clone();
        let mut last = None;
        for p in paths {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if !kind.extensions().contains(&ext.as_str()) {
                continue;
            }
            let (mut w, mut h, mut d) = (None, None, None);
            match kind {
                AssetKind::Audio => d = probe::probe_duration(&ffprobe, &p).ok(),
                AssetKind::Font => {}
                _ => {
                    if let Ok((a, b)) = probe::probe_image(&ffprobe, &p) {
                        w = Some(a);
                        h = Some(b);
                    }
                }
            }
            if let Ok(a) = self.store.add_asset(kind, &p, w, h, d) {
                last = Some(a.id.clone());
                self.assets.push(a);
            }
        }
        self.load_fonts(cx);
        if select {
            if let Some(id) = last {
                self.edit_settings(cx, |s| match kind {
                    AssetKind::Background => {
                        s.background.mode = BgMode::Image;
                        s.background.asset_id = Some(id);
                    }
                    AssetKind::Frame => s.frame.asset_id = Some(id),
                    AssetKind::Overlay => s.intro.overlay_asset_id = Some(id),
                    AssetKind::Audio => {
                        s.audio.asset_id = Some(id);
                        if s.audio.mode == AudioMode::None || s.audio.mode == AudioMode::Original {
                            s.audio.mode = AudioMode::Replace;
                        }
                    }
                    AssetKind::Font => s.text.font_asset_id = Some(id),
                });
            }
        }
        let _ = window;
        cx.notify();
    }

    pub fn pick_assets(&mut self, kind: AssetKind, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| this.add_assets(kind, paths, true, window, cx));
            }
        })
        .detach();
    }

    pub fn remove_asset(&mut self, id: &str, cx: &mut Context<Self>) {
        let _ = self.store.remove_asset(id);
        self.assets.retain(|a| a.id != id);
        let id = id.to_string();
        for p in self.presets.iter_mut() {
            let s = &mut p.settings;
            for slot in [&mut s.background.asset_id, &mut s.frame.asset_id, &mut s.intro.overlay_asset_id, &mut s.audio.asset_id, &mut s.text.font_asset_id] {
                if slot.as_deref() == Some(id.as_str()) {
                    *slot = None;
                }
            }
            let _ = self.store.save_preset(p);
        }
        self.confirm_delete = None;
        cx.notify();
    }

    fn load_fonts(&mut self, cx: &mut Context<Self>) {
        for a in self.assets.iter().filter(|a| a.kind == AssetKind::Font) {
            if self.font_families.contains_key(&a.id) {
                continue;
            }
            if let (Ok(bytes), Some(family)) = (std::fs::read(&a.file), player::font_family(&a.file)) {
                if cx.text_system().add_fonts(vec![std::borrow::Cow::Owned(bytes)]).is_ok() {
                    self.font_families.insert(a.id.clone(), family);
                }
            }
        }
    }

    // ---------- queue ----------
    pub fn add_videos(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let files = util::expand_video_paths(&paths);
        let preset_id = self.current_preset_id().or_else(|| self.default_preset_id()).unwrap_or_default();
        let mut added = vec![];
        for f in files {
            if self.jobs.iter().any(|j| j.input == f) {
                continue;
            }
            let id = uid();
            let name = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            self.jobs.push(Job {
                id: id.clone(),
                input: f.clone(),
                name,
                client: String::new(),
                preset_id: preset_id.clone(),
                thumb_time: 0.0,
                thumb_image: None,
                probe: None,
                status: JobStatus::Waiting,
                progress: 0.0,
                outputs: vec![],
                error: None,
                cancel: None,
            });
            added.push((id, f));
        }
        if added.is_empty() {
            window.push_notification(Notification::warning("Tidak ada video baru (webm, mov, mp4, mkv, gif…)"), cx);
            return;
        }
        if self.selected.is_none() {
            self.selected = Some(added[0].0.clone());
            self.sync_controls(window, cx);
        }
        let ffprobe = self.settings.ffprobe_path.clone();
        let ffmpeg = self.settings.ffmpeg_path.clone();
        for (id, path) in added {
            let ffprobe = ffprobe.clone();
            let ffmpeg = ffmpeg.clone();
            cx.spawn(async move |this, cx| {
                let res = cx.background_executor().spawn(async move {
                    let p = probe::probe(&ffprobe, &path).map_err(|e| e.to_string())?;
                    let strip = player::filmstrip(&ffmpeg, &p, 14, 72);
                    Ok::<_, String>((p, strip))
                });
                let res = res.await;
                let _ = this.update(cx, |this, cx| {
                    match res {
                        Ok((p, strip)) => {
                            let first = this.selected.as_deref() == Some(id.as_str());
                            if let Some(j) = this.job_mut(&id) {
                                j.thumb_time = j.thumb_time.min(p.duration);
                                j.probe = Some(p);
                            }
                            this.filmstrips.insert(id.clone(), strip);
                            if first {
                                this.refresh_preview(cx);
                            }
                        }
                        Err(e) => {
                            if let Some(j) = this.job_mut(&id) {
                                j.status = JobStatus::Failed;
                                j.error = Some(format!("Tidak bisa membaca video: {e}"));
                            }
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    pub fn pick_videos(&mut self, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: !folder, directories: folder, multiple: !folder, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| this.add_videos(paths, window, cx));
            }
        })
        .detach();
    }

    pub fn select_job(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(id) {
            return;
        }
        self.selected = Some(id.to_string());
        self.retire_frame(None);
        self.preview.blur_key.clear();
        self.sync_controls(window, cx);
        self.refresh_preview(cx);
        cx.notify();
    }

    pub fn remove_job(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(j) = self.jobs.iter().find(|j| j.id == id) {
            if let Some(c) = &j.cancel {
                c.cancel();
            }
        }
        let idx = self.jobs.iter().position(|j| j.id == id);
        self.jobs.retain(|j| j.id != id);
        if let Some(strip) = self.filmstrips.remove(id) {
            self.preview.retired.extend(strip.into_iter().map(|(_, img)| img));
        }
        self.motions.remove(&format!("job-{id}"));
        if self.selected.as_deref() == Some(id) {
            self.selected = idx.and_then(|i| self.jobs.get(i.min(self.jobs.len().saturating_sub(1)))).map(|j| j.id.clone());
            self.retire_frame(None);
            self.sync_controls(window, cx);
            self.refresh_preview(cx);
        }
        cx.notify();
    }

    pub fn clear_done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.jobs.iter().filter(|j| j.status == JobStatus::Done).map(|j| j.id.clone()).collect();
        for id in ids {
            self.remove_job(&id, window, cx);
        }
    }

    // ---------- preview ----------
    /// Swap the shown video frame; the old one is queued for atlas release.
    pub fn retire_frame(&mut self, next: Option<Arc<RenderImage>>) {
        if let Some(old) = std::mem::replace(&mut self.preview.frame, next) {
            self.preview.retired.push(old);
        }
    }

    /// Free images that are no longer displayed (call at the start of render).
    pub fn release_retired(&mut self, window: &mut Window) {
        for img in self.preview.retired.drain(..) {
            let _ = window.drop_image(img);
        }
    }

    pub fn set_stage(&mut self, mode: StageMode, cx: &mut Context<Self>) {
        if self.stage != mode {
            self.stage = mode;
            self.refresh_preview(cx);
            cx.notify();
        }
    }

    pub fn toggle_play(&mut self, cx: &mut Context<Self>) {
        self.preview.playing = !self.preview.playing;
        if self.stage == StageMode::Thumbnail {
            self.stage = StageMode::Social;
        }
        self.refresh_preview(cx);
        cx.notify();
    }

    pub fn set_thumbnail_time(&mut self, t: f64, cx: &mut Context<Self>) {
        self.edit_job(cx, |j| {
            j.thumb_time = t;
            j.thumb_image = None;
        });
        self.edit_settings(cx, |s| s.intro.enabled = true);
        self.stage = StageMode::Thumbnail;
        self.refresh_preview(cx);
    }

    /// (Re)start playback or load a still, depending on mode.
    pub fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        self.preview.stream = None;
        self.preview.generation += 1;
        let generation = self.preview.generation;
        let Some(job) = self.job() else { return };
        let Some(p) = job.probe.clone() else { return };
        let thumb_time = job.thumb_time;
        let ffmpeg = self.settings.ffmpeg_path.clone();
        let size = player::preview_size(&p, 960);
        self.refresh_blur(cx);
        let thumb_mode = self.stage == StageMode::Thumbnail;
        if !thumb_mode && self.preview.playing {
            let (tx, rx) = async_channel::bounded::<player::VideoFrame>(2);
            let fps = p.fps.clamp(1.0, 30.0);
            let start = if self.preview.time < p.duration { self.preview.time } else { 0.0 };
            self.preview.stream = Some(player::play(&ffmpeg, &p, start, size, fps, tx));
            cx.spawn(async move |this, cx| {
                while let Ok(f) = rx.recv().await {
                    let alive = this
                        .update(cx, |this, cx| {
                            if this.preview.generation != generation {
                                return false;
                            }
                            this.retire_frame(Some(f.image));
                            this.preview.time = f.time;
                            cx.notify();
                            true
                        })
                        .unwrap_or(false);
                    if !alive {
                        break;
                    }
                }
            })
            .detach();
        } else {
            let t = if thumb_mode { thumb_time } else { self.preview.time };
            cx.spawn(async move |this, cx| {
                let img = cx.background_executor().spawn(async move { player::still(&ffmpeg, &p, t, size) }).await;
                let _ = this.update(cx, |this, cx| {
                    if this.preview.generation == generation {
                        if let Some(img) = img {
                            this.retire_frame(Some(img));
                        }
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    fn refresh_blur(&mut self, cx: &mut Context<Self>) {
        let Some(job) = self.job() else { return };
        let Some(p) = job.probe.clone() else { return };
        let Some(s) = self.effective_settings(job) else { return };
        if s.background.mode != BgMode::Blur {
            return;
        }
        let key = format!("{}|{}|{}|{:?}", job.id, s.background.blur, s.background.color, s.resolution);
        let t = job.thumb_time;
        if key == self.preview.blur_key {
            return;
        }
        self.preview.blur_key = key.clone();
        let layout = showcase_core::layout::compute_layout(&s, &p);
        let ffmpeg = self.settings.ffmpeg_path.clone();
        cx.spawn(async move |this, cx| {
            let img = cx
                .background_executor()
                .spawn(async move { player::blurred_background(&ffmpeg, &p, t, (layout.width, layout.height), s.background.blur, &s.background.color) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.preview.blur_key == key {
                    if let Some(old) = std::mem::replace(&mut this.preview.blur_bg, img) {
                        this.preview.retired.push(old);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ---------- rendering ----------
    pub fn pending_count(&self) -> usize {
        self.jobs.iter().filter(|j| j.status == JobStatus::Waiting && j.probe.is_some()).count()
    }

    pub fn render_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ffmpeg.as_ref().map(|r| r.is_err()).unwrap_or(true) {
            window.push_notification(Notification::error("ffmpeg tidak ditemukan. Atur path-nya di Pengaturan."), cx);
            return;
        }
        if self.pending_count() == 0 {
            return;
        }
        self.batch_running = true;
        self.stop_requested = false;
        self.pump(window, cx);
        cx.notify();
    }

    pub fn render_one(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(j) = self.job_mut(id) {
            j.status = JobStatus::Waiting;
            j.progress = 0.0;
            j.error = None;
        }
        self.render_all(window, cx);
    }

    pub fn stop_all(&mut self, cx: &mut Context<Self>) {
        self.stop_requested = true;
        for j in self.jobs.iter().filter(|j| j.status == JobStatus::Rendering) {
            if let Some(c) = &j.cancel {
                c.cancel();
            }
        }
        cx.notify();
    }

    pub fn cancel_job(&mut self, id: &str) {
        if let Some(c) = self.jobs.iter().find(|j| j.id == id).and_then(|j| j.cancel.clone()) {
            c.cancel();
        }
    }

    /// Start waiting jobs until the concurrency limit is reached.
    fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let limit = self.settings.concurrency.clamp(1, 4) as usize;
        loop {
            let running = self.jobs.iter().filter(|j| j.status == JobStatus::Rendering).count();
            if self.stop_requested || running >= limit {
                break;
            }
            let Some(idx) = self.jobs.iter().position(|j| j.status == JobStatus::Waiting && j.probe.is_some()) else {
                break;
            };
            self.start_job(idx, window, cx);
        }
        let running = self.jobs.iter().any(|j| j.status == JobStatus::Rendering);
        if !running && self.batch_running {
            self.batch_running = false;
            let done = self.jobs.iter().filter(|j| j.status == JobStatus::Done).count();
            let failed = self.jobs.iter().filter(|j| j.status == JobStatus::Failed).count();
            let n = if failed > 0 {
                Notification::warning(format!("{done} video selesai, {failed} gagal"))
            } else {
                Notification::success(format!("{done} video selesai dirender"))
            };
            window.push_notification(n, cx);
        }
    }

    fn start_job(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(settings) = self.effective_settings(&self.jobs[idx]) else { return };
        let preset_name = self.presets.iter().find(|p| p.id == self.jobs[idx].preset_id).map(|p| p.name.clone()).unwrap_or_default();
        let cancel = CancelToken::new();
        let job = &mut self.jobs[idx];
        job.status = JobStatus::Rendering;
        job.progress = 0.0;
        job.error = None;
        job.outputs.clear();
        job.cancel = Some(cancel.clone());
        let id = job.id.clone();
        let s = &settings;
        let assets = ResolvedAssets {
            background: if s.background.mode == BgMode::Image { self.store.asset_path(s.background.asset_id.as_deref()) } else { None },
            frame: self.store.asset_path(s.frame.asset_id.as_deref()),
            intro_overlay: if s.intro.enabled { self.store.asset_path(s.intro.overlay_asset_id.as_deref()) } else { None },
            font: self.store.asset_path(s.text.font_asset_id.as_deref()),
            audio: if matches!(s.audio.mode, AudioMode::Replace | AudioMode::Mix) { self.store.asset_path(s.audio.asset_id.as_deref()) } else { None },
        };
        let render_job = RenderJob {
            job_id: id.clone(),
            input: job.input.clone(),
            probe: job.probe.clone().unwrap_or_default(),
            name: if job.name.trim().is_empty() { "video".into() } else { job.name.clone() },
            client: job.client.clone(),
            profile_name: preset_name,
            settings,
            assets,
            out_dir: self.settings.output_dir.clone(),
            file_template: self.settings.file_template.clone(),
            temp_dir: self.store.temp_dir.clone(),
            default_font: util::default_font(),
            preview_seconds: None,
        };
        let bins = self.bins();
        let (tx, rx) = async_channel::unbounded::<Result<f32, Result<RenderResult, String>>>();
        std::thread::spawn(move || {
            let ptx = tx.clone();
            let mut last = 0.0f32;
            let res = render::render_job(
                &bins,
                &render_job,
                &mut |p| {
                    if p - last >= 0.005 || p >= 1.0 {
                        last = p;
                        let _ = ptx.send_blocking(Ok(p));
                    }
                },
                &cancel,
            );
            let _ = tx.send_blocking(Err(res.map_err(|e| e.to_string())));
        });
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(msg) = rx.recv().await {
                let finished = matches!(msg, Err(_));
                let _ = this.update_in(cx, |this, window, cx| {
                    if let Some(j) = this.job_mut(&id) {
                        match msg {
                            Ok(p) => j.progress = p,
                            Err(Ok(r)) => {
                                j.status = JobStatus::Done;
                                j.progress = 1.0;
                                j.outputs = r.outputs;
                                j.cancel = None;
                            }
                            Err(Err(e)) => {
                                j.status = if e == "Dibatalkan" { JobStatus::Canceled } else { JobStatus::Failed };
                                j.error = (e != "Dibatalkan").then_some(e);
                                j.progress = 0.0;
                                j.cancel = None;
                            }
                        }
                    }
                    if finished {
                        this.pump(window, cx);
                    }
                    cx.notify();
                });
                if finished {
                    break;
                }
            }
        })
        .detach();
    }

    // ---------- app settings ----------
    pub fn check_ffmpeg(&mut self, cx: &mut Context<Self>) {
        let bin = self.settings.ffmpeg_path.clone();
        cx.spawn(async move |this, cx| {
            let r = cx.background_executor().spawn(async move { probe::check_ffmpeg(&bin).map_err(|e| e.to_string()) }).await;
            let _ = this.update(cx, |this, cx| {
                this.ffmpeg = Some(r);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn save_app_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let read = |this: &Self, id: InputId, cx: &App| this.inputs[&id].read(cx).value().to_string();
        let ffmpeg = read(self, InputId::FfmpegPath, cx);
        let ffprobe = read(self, InputId::FfprobePath, cx);
        let template = read(self, InputId::Template, cx);
        self.settings.ffmpeg_path = if ffmpeg.trim().is_empty() { "ffmpeg".into() } else { ffmpeg };
        self.settings.ffprobe_path = if ffprobe.trim().is_empty() { "ffprobe".into() } else { ffprobe };
        self.settings.file_template = if template.trim().is_empty() { "{name}".into() } else { template };
        let _ = self.store.set_settings(&self.settings);
        self.check_ffmpeg(cx);
        window.push_notification(Notification::success("Pengaturan disimpan"), cx);
        cx.notify();
    }

    pub fn pick_output_dir(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await {
                if let Some(dir) = paths.pop() {
                    let _ = this.update(cx, |this, cx| {
                        this.settings.output_dir = dir;
                        let _ = this.store.set_settings(&this.settings);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    pub fn pick_thumb_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await {
                if let Some(p) = paths.pop() {
                    let _ = this.update(cx, |this, cx| {
                        this.edit_job(cx, |j| j.thumb_image = Some(p));
                        this.stage = StageMode::Thumbnail;
                        this.refresh_preview(cx);
                    });
                }
            }
        })
        .detach();
    }

    /// Files dropped anywhere on the window.
    pub fn on_drop_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let ext = |p: &Path| p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let (videos, rest): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| p.is_dir() || VIDEO_EXTENSIONS.contains(&ext(p).as_str()));
        if !videos.is_empty() {
            self.add_videos(videos, window, cx);
        }
        if rest.is_empty() {
            return;
        }
        // Images go to whichever library the open tab is about.
        let image_kind = match self.tab {
            Tab::Frame => AssetKind::Frame,
            Tab::Thumbnail => AssetKind::Overlay,
            _ => AssetKind::Background,
        };
        let mut groups: HashMap<AssetKind, Vec<PathBuf>> = HashMap::new();
        for p in rest {
            let kind = match ext(&p).as_str() {
                "ttf" | "otf" => AssetKind::Font,
                "mp3" | "wav" | "ogg" | "m4a" | "aac" | "flac" => AssetKind::Audio,
                "png" | "jpg" | "jpeg" | "webp" => image_kind,
                _ => continue,
            };
            groups.entry(kind).or_default().push(p);
        }
        for (kind, paths) in groups {
            self.add_assets(kind, paths, true, window, cx);
        }
    }
}
