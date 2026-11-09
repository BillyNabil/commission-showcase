//! Preview decoding. ffmpeg decodes the source straight to BGRA (straight alpha),
//! which is the pixel layout GPUI's `RenderImage` expects. This plays WebM alpha,
//! ProRes 4444 and anything else ffmpeg reads, with no proxy files.

use gpui::RenderImage;
use image::{Frame, ImageBuffer};
use showcase_core::model::ProbeInfo;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn command(bin: &str) -> Command {
    let mut c = Command::new(bin);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

fn decoder_args(p: &ProbeInfo) -> Vec<String> {
    match p.needs_libvpx {
        Some(dec) => vec!["-c:v".into(), dec.into()],
        None => vec![],
    }
}

pub fn to_image(w: u32, h: u32, bgra: Vec<u8>) -> Option<Arc<RenderImage>> {
    let buf = ImageBuffer::from_raw(w, h, bgra)?;
    Some(Arc::new(RenderImage::new(vec![Frame::new(buf)])))
}

/// Size the decoded preview so the longest side is at most `max` (even numbers).
pub fn preview_size(p: &ProbeInfo, max: u32) -> (u32, u32) {
    let (w, h) = (p.width.max(2) as f64, p.height.max(2) as f64);
    let k = (max as f64 / w.max(h)).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    (even(w * k), even(h * k))
}

pub struct StreamHandle {
    stop: Arc<AtomicBool>,
}

impl StreamHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct VideoFrame {
    pub image: Arc<RenderImage>,
    pub time: f64,
}

/// Plays the video in real time (looping) from `start`, sending each frame.
pub fn play(
    ffmpeg: &str,
    p: &ProbeInfo,
    start: f64,
    size: (u32, u32),
    fps: f64,
    tx: async_channel::Sender<VideoFrame>,
) -> StreamHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let handle = StreamHandle { stop: stop.clone() };
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostdin".into()];
    args.extend(decoder_args(p));
    args.extend(["-re", "-stream_loop", "-1", "-ss", &format!("{start:.3}"), "-i"].map(String::from));
    args.push(p.path.to_string_lossy().into_owned());
    args.extend([
        "-an".to_string(),
        "-vf".into(),
        format!("fps={fps},scale={}:{}:flags=bilinear,format=bgra", size.0, size.1),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let ffmpeg = ffmpeg.to_string();
    let duration = p.duration.max(0.04);
    std::thread::spawn(move || {
        let Ok(mut child) = command(&ffmpeg).args(&args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
            return;
        };
        let mut out = child.stdout.take().expect("stdout");
        let frame_len = (size.0 * size.1 * 4) as usize;
        let mut n: u64 = 0;
        loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let mut buf = vec![0u8; frame_len];
            if out.read_exact(&mut buf).is_err() {
                break;
            }
            let time = (start + n as f64 / fps) % duration;
            n += 1;
            if let Some(image) = to_image(size.0, size.1, buf) {
                if tx.send_blocking(VideoFrame { image, time }).is_err() {
                    break;
                }
            }
        }
        kill(&mut child);
    });
    handle
}

fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn run_capture(ffmpeg: &str, args: &[String]) -> Option<Vec<u8>> {
    let out = command(ffmpeg).args(args).stdout(Stdio::piped()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then_some(out.stdout)
}

/// One decoded frame at `t`.
pub fn still(ffmpeg: &str, p: &ProbeInfo, t: f64, size: (u32, u32)) -> Option<Arc<RenderImage>> {
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostdin".into()];
    args.extend(decoder_args(p));
    args.extend(["-ss".to_string(), format!("{:.3}", t.max(0.0)), "-i".into(), p.path.to_string_lossy().into_owned()]);
    args.extend([
        "-frames:v".to_string(),
        "1".into(),
        "-vf".into(),
        format!("scale={}:{}:flags=bilinear,format=bgra", size.0, size.1),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let data = run_capture(ffmpeg, &args)?;
    let len = (size.0 * size.1 * 4) as usize;
    (data.len() >= len).then(|| to_image(size.0, size.1, data[..len].to_vec())).flatten()
}

/// `count` evenly spaced thumbnails for the filmstrip, each `height` px tall.
pub fn filmstrip(ffmpeg: &str, p: &ProbeInfo, count: u32, height: u32) -> Vec<(f64, Arc<RenderImage>)> {
    let dur = p.duration.max(0.1);
    let w = (((height as f64 * p.width.max(1) as f64 / p.height.max(1) as f64) / 2.0).round() as u32 * 2).max(2);
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostdin".into()];
    args.extend(decoder_args(p));
    args.extend(["-i".to_string(), p.path.to_string_lossy().into_owned()]);
    args.extend([
        "-vf".to_string(),
        format!("fps={:.6},scale={w}:{height}:flags=bilinear,format=bgra", count as f64 / dur),
        "-frames:v".into(),
        count.to_string(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let Some(data) = run_capture(ffmpeg, &args) else { return vec![] };
    let len = (w * height * 4) as usize;
    data.chunks_exact(len)
        .enumerate()
        .filter_map(|(i, c)| {
            let t = ((i as f64 + 0.5) * dur / count as f64).min(dur - 0.02).max(0.0);
            to_image(w, height, c.to_vec()).map(|img| (t, img))
        })
        .collect()
}

/// Blurred, background-filling frame for the "Blur video" background preview.
pub fn blurred_background(ffmpeg: &str, p: &ProbeInfo, t: f64, out: (u32, u32), sigma: f32, base_hex: &str) -> Option<Arc<RenderImage>> {
    let (w, h) = ((out.0 / 4).max(2), (out.1 / 4).max(2));
    let base = format!("0x{}", base_hex.trim_start_matches('#'));
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostdin".into()];
    args.extend(["-f".to_string(), "lavfi".into(), "-i".into(), format!("color=c={base}:s={w}x{h}:d=1")]);
    args.extend(decoder_args(p));
    args.extend(["-ss".to_string(), format!("{:.3}", t.max(0.0)), "-i".into(), p.path.to_string_lossy().into_owned()]);
    args.extend([
        "-filter_complex".to_string(),
        format!(
            "[1:v]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},gblur=sigma={:.2},format=rgba[b];[0:v][b]overlay=0:0,format=bgra",
            (sigma / 4.0).max(1.0)
        ),
        "-frames:v".into(),
        "1".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let data = run_capture(ffmpeg, &args)?;
    let len = (w * h * 4) as usize;
    (data.len() >= len).then(|| to_image(w, h, data[..len].to_vec())).flatten()
}

/// Family name of a TTF/OTF so the preview can draw text with the chosen font.
pub fn font_family(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let face = ttf_parser::Face::parse(&data, 0).ok()?;
    face.names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY || n.name_id == ttf_parser::name_id::FAMILY)
        .find_map(|n| n.to_string())
}
