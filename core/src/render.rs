//! Running ffmpeg: two outputs per render, progress, cancel, thumbnail export.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use crate::graph::{build_render_args, fmt, BuildOptions};
use crate::layout::expand_text;
use crate::model::{GifSource, OutputKind, ProbeInfo, RenderResult, RenderSettings, ResolvedAssets};
use crate::probe::background_command;

pub const CANCELED_MESSAGE: &str = "Dibatalkan";

#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub fn is_canceled(e: &anyhow::Error) -> bool {
    e.to_string() == CANCELED_MESSAGE
}

#[derive(Clone, Debug)]
pub struct Bins {
    pub ffmpeg: String,
    pub ffprobe: String,
}

#[derive(Clone, Debug)]
pub struct RenderJob {
    pub job_id: String,
    pub input: PathBuf,
    pub probe: ProbeInfo,
    pub name: String,
    pub client: String,
    pub profile_name: String,
    pub settings: RenderSettings,
    pub assets: ResolvedAssets,
    pub out_dir: PathBuf,
    pub file_template: String,
    pub temp_dir: PathBuf,
    pub default_font: PathBuf,
    pub preview_seconds: Option<f64>,
}

enum Msg {
    Progress(f64),
}

/// Run ffmpeg with `-progress pipe:1` in the args, reporting progress (0..1)
/// and killing the process when `cancel` is set.
pub fn run_ffmpeg(
    bin: &str,
    args: &[String],
    total_duration: Option<f64>,
    on_progress: &mut dyn FnMut(f32),
    cancel: &CancelToken,
) -> Result<()> {
    if cancel.is_cancelled() {
        bail!(CANCELED_MESSAGE);
    }
    let mut child = background_command(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Tidak bisa menjalankan {bin}"))?;

    let (tx, rx) = mpsc::channel::<Msg>();
    let stdout = child.stdout.take().unwrap();
    let out_thread = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(v) = line.strip_prefix("out_time_us=") {
                if let Ok(us) = v.trim().parse::<f64>() {
                    let _ = tx.send(Msg::Progress(us / 1e6));
                }
            }
        }
    });
    let stderr_buf = Arc::new(Mutex::new(String::new()));
    let stderr = child.stderr.take().unwrap();
    let err_buf = stderr_buf.clone();
    let err_thread = std::thread::spawn(move || {
        let mut r = BufReader::new(stderr);
        let mut chunk = [0u8; 4096];
        while let Ok(n) = r.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let mut b = err_buf.lock().unwrap();
            b.push_str(&String::from_utf8_lossy(&chunk[..n]));
            if b.len() > 6000 {
                let mut cut = b.len() - 6000;
                while !b.is_char_boundary(cut) {
                    cut += 1;
                }
                b.drain(..cut);
            }
        }
    });

    // Monotonic progress.
    let mut best = 0.0f64;
    let mut report = |msg: Msg| {
        let Msg::Progress(t) = msg;
        let p = total_duration.filter(|d| *d > 0.0).map(|d| t / d);
        if let Some(p) = p {
            let p = p.clamp(0.0, 0.999);
            if p > best {
                best = p;
                on_progress(p as f32);
            }
        }
    };
    let mut killed = false;
    let status = loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => report(msg),
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
        }
        if cancel.is_cancelled() && !killed {
            let _ = child.kill();
            killed = true;
        }
        if let Some(st) = child.try_wait()? {
            // drain remaining progress lines
            while let Ok(msg) = rx.try_recv() {
                report(msg);
            }
            break st;
        }
    };
    let _ = out_thread.join();
    let _ = err_thread.join();

    if cancel.is_cancelled() {
        bail!(CANCELED_MESSAGE);
    }
    if status.success() {
        on_progress(1.0);
        Ok(())
    } else {
        let err = stderr_buf.lock().unwrap().trim().to_string();
        Err(anyhow!(if err.is_empty() { format!("ffmpeg keluar dengan kode {:?}", status.code()) } else { err }))
    }
}

pub fn output_base_name(template: &str, name: &str, profile: &str) -> String {
    let date = chrono::Local::now().format("%Y%m%d").to_string();
    let t = if template.is_empty() { "{name}" } else { template };
    let raw = t.replace("{name}", name).replace("{profile}", profile).replace("{date}", &date);
    let cleaned: String = raw
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || (c as u32) < 0x20 { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() { name.to_string() } else { cleaned }
}

fn remove_quiet(p: &Path) {
    let _ = fs::remove_file(p);
}

pub fn render_job(bins: &Bins, job: &RenderJob, on_progress: &mut dyn FnMut(f32), cancel: &CancelToken) -> Result<RenderResult> {
    let s = &job.settings;
    fs::create_dir_all(&job.out_dir).with_context(|| format!("Tidak bisa membuat folder {}", job.out_dir.display()))?;
    fs::create_dir_all(&job.temp_dir)?;
    let base = output_base_name(&job.file_template, &job.name, &job.profile_name);

    let (mut social, mut porto) = (None, None);
    if job.preview_seconds.is_some() {
        if s.outputs.social {
            social = Some(job.out_dir.join(format!("{base}_preview_social.mp4")));
        } else {
            porto = Some(job.out_dir.join(format!("{base}_preview_porto.mp4")));
        }
    } else {
        if s.outputs.social {
            social = Some(job.out_dir.join(format!("{base}_social.mp4")));
        }
        if s.outputs.porto {
            porto = Some(job.out_dir.join(format!("{base}_porto.mp4")));
        }
    }
    let gif = (s.outputs.gif && job.preview_seconds.is_none()).then(|| job.out_dir.join(format!("{base}.gif")));
    if social.is_none() && porto.is_none() {
        bail!("Aktifkan minimal satu output (Sosmed atau Porto)");
    }

    let mut text_file = None;
    if s.text.enabled {
        let text = expand_text(&s.text.text, &job.client, &job.name, &job.profile_name);
        if !text.trim().is_empty() {
            let p = job.temp_dir.join(format!("text_{}.txt", job.job_id));
            fs::write(&p, text)?;
            text_file = Some(p);
        }
    }

    let built = build_render_args(&BuildOptions {
        input: &job.input,
        probe: &job.probe,
        settings: s,
        assets: &job.assets,
        social: social.clone(),
        porto: porto.clone(),
        text_file: text_file.clone(),
        default_font: job.default_font.clone(),
        preview_seconds: job.preview_seconds,
    })?;

    let mut produced: Vec<PathBuf> = social.iter().chain(porto.iter()).cloned().collect();
    // Main pass gets most of the bar; the GIF step (if any) the rest.
    let scale = if gif.is_some() { 0.85 } else if s.outputs.thumbnail && job.preview_seconds.is_none() { 0.97 } else { 1.0 };
    let res = run_ffmpeg(&bins.ffmpeg, &built.args, Some(built.total_duration), &mut |p| on_progress(p * scale), cancel);
    if let Some(t) = &text_file {
        remove_quiet(t);
    }
    if let Err(e) = res {
        for f in &produced {
            remove_quiet(f);
        }
        return Err(e);
    }

    // Animated GIF made from the finished video of the chosen version.
    if let Some(gif_path) = &gif {
        let src = match s.gif.source {
            GifSource::Porto => porto.as_ref().or(social.as_ref()),
            GifSource::Social => social.as_ref().or(porto.as_ref()),
        }
        .cloned()
        .expect("at least one video output");
        let width = s.gif.width.max(120) / 2 * 2;
        let fps = s.gif.fps.clamp(5, 30);
        let args: Vec<String> = [
            "-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats",
            "-i", &src.to_string_lossy(),
            "-vf", &format!(
                "fps={fps},scale='min({width},iw)':-2:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle"
            ),
            "-loop", "0", &gif_path.to_string_lossy(),
        ]
        .iter()
        .map(|x| x.to_string())
        .collect();
        let total = built.social_duration.or(built.porto_duration);
        let (lo, hi) = (0.85f32, if s.outputs.thumbnail { 0.97 } else { 1.0 });
        if let Err(e) = run_ffmpeg(&bins.ffmpeg, &args, total, &mut |p| on_progress(lo + p * (hi - lo)), cancel) {
            for f in &produced {
                remove_quiet(f);
            }
            remove_quiet(gif_path);
            return Err(e);
        }
        produced.push(gif_path.clone());
    }

    // Thumbnail JPG from the finished output (exactly what viewers will see)
    if s.outputs.thumbnail && job.preview_seconds.is_none() {
        let (src, kind) = match (&social, &porto) {
            (Some(p), _) => (p.clone(), OutputKind::Social),
            (None, Some(p)) => (p.clone(), OutputKind::Porto),
            _ => unreachable!(),
        };
        let has_intro = s.intro.enabled
            && match kind {
                OutputKind::Social => s.intro.social,
                OutputKind::Porto => s.intro.porto,
            };
        let t = if has_intro {
            0.0
        } else {
            s.intro.timestamp.max(0.0).min((built.layout.main_duration - 0.1).max(0.0))
        };
        let thumb = job.out_dir.join(format!("{base}_thumb.jpg"));
        let args: Vec<String> = [
            "-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-ss", &fmt(t), "-i", &src.to_string_lossy(),
            "-frames:v", "1", "-q:v", "2", &thumb.to_string_lossy(),
        ]
        .iter()
        .map(|x| x.to_string())
        .collect();
        if let Err(e) = run_ffmpeg(&bins.ffmpeg, &args, None, &mut |_| {}, cancel) {
            for f in &produced {
                remove_quiet(f);
            }
            remove_quiet(&thumb);
            return Err(e);
        }
        produced.push(thumb);
    }

    let mut outputs = vec![];
    for f in produced {
        let size = fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
        outputs.push((f, size));
    }
    on_progress(1.0);
    Ok(RenderResult { outputs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_name_sanitizes() {
        assert_eq!(output_base_name("{name}", "a:b", "P"), "a_b");
        assert_eq!(output_base_name("", "x", "P"), "x");
        assert_eq!(output_base_name("{profile}-{name}", "x", "Tw"), "Tw-x");
    }

    #[test]
    fn cancel_token() {
        let c = CancelToken::new();
        let c2 = c.clone();
        assert!(!c.is_cancelled());
        c2.cancel();
        assert!(c.is_cancelled());
        assert!(is_canceled(&anyhow!(CANCELED_MESSAGE)));
    }
}
