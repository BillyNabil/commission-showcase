//! ffmpeg argument builder. Pure function (no I/O) so it is easy to test.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::layout::{compute_layout, parse_hex_color, text_anchor, HAlign, Layout, VAlign};
use crate::model::{AudioMode, BgMode, Codec, Fit, IntroSource, OutputKind, ProbeInfo, QualityPreset, RenderSettings, ResolvedAssets};

pub struct BuildOptions<'a> {
    pub input: &'a Path,
    pub probe: &'a ProbeInfo,
    pub settings: &'a RenderSettings,
    pub assets: &'a ResolvedAssets,
    pub social: Option<PathBuf>,
    pub porto: Option<PathBuf>,
    /// Text file holding the already-expanded overlay text (drawtext textfile=)
    pub text_file: Option<PathBuf>,
    /// Font used when no library font is selected
    pub default_font: PathBuf,
    /// Short preview mode: limit duration and use a fast preset
    pub preview_seconds: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct BuildResult {
    pub args: Vec<String>,
    pub layout: Layout,
    pub social_duration: Option<f64>,
    pub porto_duration: Option<f64>,
    pub total_duration: f64,
}

struct QualitySpec {
    crf: u32,
    maxrate_1080: u32, // kbps at 1080p30
    audio: &'static str,
}

fn quality(p: QualityPreset) -> QualitySpec {
    match p {
        QualityPreset::Small => QualitySpec { crf: 25, maxrate_1080: 5000, audio: "128k" },
        QualityPreset::Balanced => QualitySpec { crf: 21, maxrate_1080: 8000, audio: "160k" },
        QualityPreset::High => QualitySpec { crf: 18, maxrate_1080: 14000, audio: "192k" },
    }
}

/// Escape a path for use inside a filter option. Two parsing levels: the
/// filtergraph (quotes protect , ; [ ]) and the filter options (split on ':'),
/// so ':' becomes '\:' inside the quotes and ' is closed-escaped-reopened.
pub fn escape_filter_path(p: impl AsRef<str>) -> String {
    let s = p.as_ref().replace('\\', "/").replace(':', "\\:").replace('\'', "'\\\\\\''");
    format!("'{s}'")
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn hex_color(c: &str) -> String {
    match parse_hex_color(c) {
        Some((r, g, b)) => format!("0x{r:02x}{g:02x}{b:02x}"),
        None => "0x000000".into(),
    }
}

fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// Format like JS `String(Math.round(n * 1000) / 1000)`.
/// Scale factor for overlays designed on a 1920x1080 canvas.
pub fn sticker_scale(out_w: u32, out_h: u32) -> f64 {
    (out_w as f64 / 1920.0).min(out_h as f64 / 1080.0)
}

pub(crate) fn fmt(n: f64) -> String {
    let v = js_round(n * 1000.0) / 1000.0;
    if v == 0.0 { "0".into() } else { format!("{v}") }
}

struct Graph {
    inputs: Vec<Vec<String>>,
    fc: Vec<String>,
    n: usize,
}

impl Graph {
    fn lbl(&mut self, base: &str) -> String {
        let l = format!("{base}{}", self.n);
        self.n += 1;
        l
    }
    fn add_input(&mut self, opts: &[String], file: &str) -> usize {
        let mut v = opts.to_vec();
        v.push("-i".into());
        v.push(file.into());
        self.inputs.push(v);
        self.inputs.len() - 1
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

pub fn build_render_args(o: &BuildOptions) -> Result<BuildResult> {
    let s = o.settings;
    let l = compute_layout(s, o.probe);
    let (w, h, f) = (l.width, l.height, l.fps);
    let preview = o.preview_seconds.filter(|p| *p > 0.0);

    let want_social = o.social.is_some();
    let want_porto = o.porto.is_some();
    if !want_social && !want_porto {
        bail!("Tidak ada output yang dipilih");
    }

    let mut g = Graph { inputs: vec![], fc: vec![], n: 0 };
    let decoder: Vec<String> = match o.probe.needs_libvpx {
        Some(d) => vec!["-c:v".into(), d.into()],
        None => vec![],
    };

    // ---------- Background ----------
    let make_background = |g: &mut Graph, video_for_blur: Option<&str>| -> String {
        let out = g.lbl("bg");
        let bg = &s.background;
        if bg.mode == BgMode::Image {
            if let Some(bg_path) = &o.assets.background {
                let i = g.add_input(&strs(&["-loop", "1", "-framerate", &fmt(f)]), &path_str(bg_path));
                if bg.fit == Fit::Contain {
                    g.fc.push(format!(
                        "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=decrease:flags=lanczos,\
pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color={},setsar=1,format=gbrp[{out}]",
                        hex_color(&bg.color)
                    ));
                } else {
                    g.fc.push(format!(
                        "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=increase:flags=lanczos,\
crop={w}:{h},setsar=1,format=gbrp[{out}]"
                    ));
                }
                return out;
            }
        }
        if bg.mode == BgMode::Blur {
            if let Some(src) = video_for_blur {
                let sw = (js_round(w as f64 / 4.0) as u32).max(2);
                let sh = (js_round(h as f64 / 4.0) as u32).max(2);
                let blurred = g.lbl("blur");
                let base = g.lbl("black");
                g.fc.push(format!(
                    "[{src}]scale={sw}:{sh}:force_original_aspect_ratio=increase,crop={sw}:{sh},\
gblur=sigma={},scale={w}:{h},format=rgba[{blurred}]",
                    fmt((bg.blur as f64 / 4.0).max(1.0))
                ));
                g.fc.push(format!("color=c={}:s={w}x{h}:r={},format=gbrp[{base}]", hex_color(&bg.color), fmt(f)));
                g.fc.push(format!("[{base}][{blurred}]overlay=0:0:shortest=1:format=gbrp[{out}]"));
                return out;
            }
        }
        g.fc.push(format!("color=c={}:s={w}x{h}:r={},format=gbrp[{out}]", hex_color(&bg.color), fmt(f)));
        out
    };

    // Paste the (scaled, rgba) video onto a background.
    let composite = |g: &mut Graph, video_label: &str| -> String {
        let mut fg = video_label.to_string();
        let mut blur_src: Option<String> = None;
        if s.background.mode == BgMode::Blur {
            fg = g.lbl("fg");
            let b = g.lbl("bsrc");
            g.fc.push(format!("[{video_label}]split[{fg}][{b}]"));
            blur_src = Some(b);
        }
        let base = make_background(g, blur_src.as_deref());
        let out = g.lbl("comp");
        g.fc.push(format!("[{base}][{fg}]overlay={}:{}:shortest=1:format=gbrp,setsar=1[{out}]", l.x, l.y));
        out
    };

    // ---------- Main video (with loop) ----------
    let loop_opts: Vec<String> =
        if l.plays > 1 { vec!["-stream_loop".into(), (l.plays - 1).to_string()] } else { vec![] };
    let input = path_str(o.input);
    let mut main_opts = decoder.clone();
    main_opts.extend(loop_opts.iter().cloned());
    let main_idx = g.add_input(&main_opts, &input);
    let main_vid = g.lbl("vid");
    g.fc.push(format!(
        "[{main_idx}:v]setpts=PTS-STARTPTS,fps={},trim=duration={},setpts=PTS-STARTPTS,\
format=rgba,scale={}:{}:flags=lanczos[{main_vid}]",
        fmt(f),
        fmt(l.main_duration),
        l.vw,
        l.vh
    ));
    let comp = composite(&mut g, &main_vid);

    // ---------- Intro thumbnail ----------
    let intro_social = s.intro.enabled && want_social && s.intro.social;
    let intro_porto = s.intro.enabled && want_porto && s.intro.porto;

    let build_intro = |g: &mut Graph| -> String {
        let mut still: String;
        match (&s.intro.source, &s.intro.image_path) {
            (IntroSource::Image, Some(img)) => {
                let i = g.add_input(&[], &path_str(img));
                still = g.lbl("istill");
                g.fc.push(format!(
                    "[{i}:v]trim=end_frame=1,setpts=PTS-STARTPTS,scale={w}:{h}:force_original_aspect_ratio=increase:flags=lanczos,\
crop={w}:{h},setsar=1,format=gbrp[{still}]"
                ));
            }
            _ => {
                let ts = s.intro.timestamp.max(0.0).min((l.source_duration - 0.05).max(0.0));
                let mut opts = decoder.clone();
                opts.push("-ss".into());
                opts.push(fmt(ts));
                let i = g.add_input(&opts, &input);
                let iv = g.lbl("ivid");
                g.fc.push(format!(
                    "[{i}:v]trim=end_frame=1,setpts=PTS-STARTPTS,format=rgba,scale={}:{}:flags=lanczos[{iv}]",
                    l.vw, l.vh
                ));
                still = composite(g, &iv);
            }
        }
        if let Some(ov_path) = &o.assets.intro_overlay {
            let i = g.add_input(&[], &path_str(ov_path));
            let ov = g.lbl("iov");
            let out = g.lbl("istk");
            // Stickers keep their designed size relative to a 1920x1080 canvas
            // (scaled for other output sizes) and never exceed the canvas.
            let k = fmt(sticker_scale(w, h));
            g.fc.push(format!(
                "[{i}:v]scale=w='min(iw*{k},{w})':h='min(ih*{k},{h})':force_original_aspect_ratio=decrease:flags=lanczos,format=rgba[{ov}]"
            ));
            g.fc.push(format!("[{still}][{ov}]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2:format=gbrp[{out}]"));
            still = out;
        }
        let out = g.lbl("intro");
        g.fc.push(format!(
            "[{still}]trim=end_frame=1,loop=loop={}:size=1:start=0,setpts=N/({}*TB),setsar=1[{out}]",
            l.intro_frames - 1,
            fmt(f)
        ));
        out
    };

    let split_n = |g: &mut Graph, label: String, count: usize| -> Vec<String> {
        if count <= 1 {
            return vec![label];
        }
        let outs: Vec<String> = (0..count).map(|_| g.lbl("sp")).collect();
        let tail: String = outs.iter().map(|x| format!("[{x}]")).collect();
        g.fc.push(format!("[{label}]split={count}{tail}"));
        outs
    };

    let kinds: Vec<OutputKind> = [(OutputKind::Social, want_social), (OutputKind::Porto, want_porto)]
        .into_iter()
        .filter(|(_, wanted)| *wanted)
        .map(|(k, _)| k)
        .collect();
    let comp_parts = split_n(&mut g, comp, kinds.len());
    let intro_kinds: Vec<OutputKind> = kinds
        .iter()
        .copied()
        .filter(|k| match k {
            OutputKind::Social => intro_social,
            OutputKind::Porto => intro_porto,
        })
        .collect();
    let intro_parts = if intro_kinds.is_empty() {
        vec![]
    } else {
        let intro = build_intro(&mut g);
        split_n(&mut g, intro, intro_kinds.len())
    };

    // ---------- Finishing per output ----------
    let mut video_out: Vec<(OutputKind, String)> = vec![];
    let mut durations: Vec<(OutputKind, f64)> = vec![];
    for (idx, k) in kinds.iter().copied().enumerate() {
        let mut cur = comp_parts[idx].clone();
        let intro_idx = intro_kinds.iter().position(|x| *x == k);
        if let Some(ii) = intro_idx {
            let out = g.lbl("cat");
            g.fc.push(format!("[{}][{cur}]concat=n=2:v=1:a=0[{out}]", intro_parts[ii]));
            cur = out;
        }
        durations.push((k, l.main_duration + if intro_idx.is_some() { l.intro_duration } else { 0.0 }));

        let frame_for = match k {
            OutputKind::Social => s.frame.social,
            OutputKind::Porto => s.frame.porto,
        };
        if let (Some(frame), true) = (&o.assets.frame, frame_for) {
            let i = g.add_input(&[], &path_str(frame));
            let fr = g.lbl("frm");
            let out = g.lbl("framed");
            g.fc.push(format!("[{i}:v]scale={w}:{h}:flags=lanczos,format=rgba[{fr}]"));
            g.fc.push(format!("[{cur}][{fr}]overlay=0:0:format=gbrp[{out}]"));
            cur = out;
        }

        let text_for = match k {
            OutputKind::Social => s.text.social,
            OutputKind::Porto => s.text.porto,
        };
        if let (true, true, Some(text_file)) = (s.text.enabled, text_for, &o.text_file) {
            let t = &s.text;
            let font_path = o.assets.font.clone().unwrap_or_else(|| o.default_font.clone());
            let size = (js_round((t.size as f64 / 100.0) * h as f64) as i64).max(6);
            let m = js_round((t.margin as f64 / 100.0) * h as f64) as i64;
            let (ha, va) = text_anchor(t.position);
            let x = match ha {
                HAlign::Left => format!("{m}"),
                HAlign::Right => format!("w-text_w-{m}"),
                HAlign::Center => "(w-text_w)/2".into(),
            };
            let y = match va {
                VAlign::Top => format!("{m}"),
                VAlign::Bottom => format!("h-text_h-{m}"),
                VAlign::Middle => "(h-text_h)/2".into(),
            };
            let mut parts = vec![
                format!("fontfile={}", escape_filter_path(path_str(&font_path))),
                format!("textfile={}", escape_filter_path(path_str(text_file))),
                "expansion=none".into(),
                format!("fontsize={size}"),
                format!("fontcolor={}", hex_color(&t.color)),
                format!("x={x}"),
                format!("y={y}"),
            ];
            if t.outline_width > 0.0 {
                parts.push(format!("borderw={}", js_round(t.outline_width as f64) as i64));
                parts.push(format!("bordercolor={}", hex_color(&t.outline_color)));
            }
            if t.shadow {
                let sd = (js_round(size as f64 / 18.0) as i64).max(1);
                parts.push(format!("shadowx={sd}"));
                parts.push(format!("shadowy={sd}"));
                parts.push("shadowcolor=0x000000@0.6".into());
            }
            let out = g.lbl("txt");
            g.fc.push(format!("[{cur}]drawtext={}[{out}]", parts.join(":")));
            cur = out;
        }

        let out = format!("v_{}", k.suffix());
        g.fc.push(format!(
            "[{cur}]scale=out_color_matrix=bt709:out_range=tv:flags=lanczos+accurate_rnd+full_chroma_int,format=yuv420p[{out}]"
        ));
        video_out.push((k, out));
    }
    let duration_of = |k: OutputKind| durations.iter().find(|(x, _)| *x == k).map(|(_, d)| *d);

    // ---------- Audio ----------
    let mut audio_out: Vec<(OutputKind, String)> = vec![];
    let au = &s.audio;
    if au.mode != AudioMode::None {
        for k in kinds.iter().copied() {
            if k == OutputKind::Porto && !au.porto {
                continue;
            }
            let t_total = duration_of(k).unwrap_or(l.main_duration);
            let delay_ms = js_round((t_total - l.main_duration) * 1000.0) as i64;
            let use_orig = matches!(au.mode, AudioMode::Original | AudioMode::Mix) && o.probe.has_audio;
            let use_music = matches!(au.mode, AudioMode::Replace | AudioMode::Mix) && o.assets.audio.is_some();
            if !use_orig && !use_music {
                continue;
            }
            let mut parts: Vec<String> = vec![];
            if use_orig {
                let i = g.add_input(&loop_opts, &input);
                let a = g.lbl("aorig");
                let delay = if delay_ms > 0 { format!(",adelay={delay_ms}:all=1") } else { String::new() };
                g.fc.push(format!(
                    "[{i}:a]atrim=duration={},asetpts=PTS-STARTPTS,aresample=48000,\
aformat=sample_rates=48000:channel_layouts=stereo{delay},apad,atrim=duration={}[{a}]",
                    fmt(l.main_duration),
                    fmt(t_total)
                ));
                parts.push(a);
            }
            if use_music {
                let music = o.assets.audio.as_ref().unwrap();
                let i = g.add_input(&strs(&["-stream_loop", "-1"]), &path_str(music));
                let a = g.lbl("amus");
                g.fc.push(format!(
                    "[{i}:a]atrim=duration={},asetpts=PTS-STARTPTS,aresample=48000,\
aformat=sample_rates=48000:channel_layouts=stereo,volume={}[{a}]",
                    fmt(t_total),
                    fmt(au.volume as f64)
                ));
                parts.push(a);
            }
            let mut cur = parts[0].clone();
            if parts.len() == 2 {
                cur = g.lbl("amix");
                g.fc.push(format!("[{}][{}]amix=inputs=2:duration=first:normalize=0[{cur}]", parts[0], parts[1]));
            }
            let mut fades: Vec<String> = vec![];
            if au.fade_in > 0.0 {
                fades.push(format!("afade=t=in:st=0:d={}", fmt((au.fade_in as f64).min(t_total / 2.0))));
            }
            if au.fade_out > 0.0 {
                let d = (au.fade_out as f64).min(t_total / 2.0);
                fades.push(format!("afade=t=out:st={}:d={}", fmt((t_total - d).max(0.0)), fmt(d)));
            }
            let out = format!("a_{}", k.suffix());
            let chain = if fades.is_empty() { "anull".to_string() } else { fades.join(",") };
            g.fc.push(format!("[{cur}]{chain}[{out}]"));
            audio_out.push((k, out));
        }
    }

    // ---------- Encoder ----------
    let q = quality(s.quality.preset);
    let pixel_factor = (w as f64 * h as f64) / (1920.0 * 1080.0);
    let fps_factor = if f > 30.0 { 1.5 } else { 1.0 };
    let maxrate = js_round(q.maxrate_1080 as f64 * pixel_factor * fps_factor) as i64;
    let gop = (js_round(f * 2.0) as i64).max(1);

    let mut out_args: Vec<String> = vec![];
    for (k, vlabel) in &video_out {
        out_args.push("-map".into());
        out_args.push(format!("[{vlabel}]"));
        let alabel = audio_out.iter().find(|(x, _)| x == k).map(|(_, a)| a.clone());
        if let Some(a) = &alabel {
            out_args.push("-map".into());
            out_args.push(format!("[{a}]"));
        }
        let h265 = *k == OutputKind::Porto && s.quality.porto_codec == Codec::H265;
        if h265 {
            out_args.extend(strs(&[
                "-c:v",
                "libx265",
                "-preset",
                if preview.is_some() { "ultrafast" } else { "slow" },
                "-crf",
                &(q.crf + 4).to_string(),
                "-tag:v",
                "hvc1",
                "-x265-params",
                &format!("log-level=error:keyint={gop}"),
            ]));
        } else {
            out_args.extend(strs(&[
                "-c:v",
                "libx264",
                "-preset",
                if preview.is_some() { "veryfast" } else { "slow" },
                "-crf",
                &q.crf.to_string(),
                "-tune",
                "animation",
                "-profile:v",
                "high",
                "-g",
                &gop.to_string(),
            ]));
        }
        out_args.extend(strs(&["-maxrate", &format!("{maxrate}k"), "-bufsize", &format!("{}k", maxrate * 2)]));
        out_args.extend(strs(&[
            "-pix_fmt",
            "yuv420p",
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-color_range",
            "tv",
            "-r",
            &fmt(f),
        ]));
        if alabel.is_some() {
            out_args.extend(strs(&["-c:a", "aac", "-b:a", q.audio]));
        }
        if let Some(p) = preview {
            out_args.push("-t".into());
            out_args.push(fmt(p));
        }
        let target = match k {
            OutputKind::Social => o.social.as_ref().unwrap(),
            OutputKind::Porto => o.porto.as_ref().unwrap(),
        };
        out_args.push("-movflags".into());
        out_args.push("+faststart".into());
        out_args.push(path_str(target));
    }

    let mut args = strs(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]);
    for i in g.inputs {
        args.extend(i);
    }
    args.push("-filter_complex".into());
    args.push(g.fc.join(";\n"));
    args.extend(out_args);

    let mut total_duration = durations.iter().map(|(_, d)| *d).fold(0.0, f64::max);
    if let Some(p) = preview {
        total_duration = total_duration.min(p);
    }
    Ok(BuildResult {
        args,
        layout: l,
        social_duration: duration_of(OutputKind::Social),
        porto_duration: duration_of(OutputKind::Porto),
        total_duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::compute_layout;

    const INPUT: &str = "C:/in/anim.webm";
    const SOCIAL: &str = "C:/out/a_social.mp4";
    const PORTO: &str = "C:/out/a_porto.mp4";

    fn probe() -> ProbeInfo {
        ProbeInfo {
            path: INPUT.into(),
            width: 1000,
            height: 1000,
            duration: 3.0,
            fps: 30.0,
            vcodec: "vp9".into(),
            pix_fmt: "yuv420p".into(),
            has_alpha: true,
            needs_libvpx: Some("libvpx-vp9"),
            has_audio: false,
            format_name: "matroska,webm".into(),
        }
    }

    fn build_with(tweak: impl FnOnce(&mut RenderSettings), assets: ResolvedAssets) -> BuildResult {
        let mut s = RenderSettings::default();
        tweak(&mut s);
        let p = probe();
        build_render_args(&BuildOptions {
            input: Path::new(INPUT),
            probe: &p,
            settings: &s,
            assets: &assets,
            social: Some(SOCIAL.into()),
            porto: Some(PORTO.into()),
            text_file: Some("C:/tmp/t.txt".into()),
            default_font: "C:/Windows/Fonts/arial.ttf".into(),
            preview_seconds: None,
        })
        .unwrap()
    }
    fn build(tweak: impl FnOnce(&mut RenderSettings)) -> BuildResult {
        build_with(tweak, ResolvedAssets::default())
    }
    fn graph_of(args: &[String]) -> &str {
        let i = args.iter().position(|a| a == "-filter_complex").unwrap();
        &args[i + 1]
    }
    fn count(args: &[String], v: &str) -> usize {
        args.iter().filter(|a| *a == v).count()
    }
    fn pos(args: &[String], v: &str) -> usize {
        args.iter().position(|a| a == v).unwrap()
    }

    #[test]
    fn layout_fits_video_inside_output() {
        let mut s = RenderSettings::default();
        s.video.scale = 100.0;
        let l = compute_layout(&s, &probe());
        assert_eq!((l.width, l.height), (1920, 1080));
        assert_eq!((l.vw, l.vh, l.x, l.y), (1080, 1080, 420, 0));
    }

    #[test]
    fn layout_loops_short_clips() {
        let mut s = RenderSettings::default();
        s.loop_.min_duration = 8.0;
        assert_eq!(compute_layout(&s, &probe()).plays, 3);
        s.loop_.count = 5;
        assert_eq!(compute_layout(&s, &probe()).main_duration, 15.0);
        s.loop_.enabled = false;
        assert_eq!(compute_layout(&s, &probe()).plays, 1);
    }

    #[test]
    fn decodes_alpha_webm_with_libvpx_and_loops() {
        let r = build(|_| {});
        let i = pos(&r.args, "-stream_loop");
        assert_eq!(&r.args[i - 2..i + 4], &strs(&["-c:v", "libvpx-vp9", "-stream_loop", "2", "-i", INPUT])[..]);
    }

    #[test]
    fn produces_two_mapped_outputs() {
        let r = build(|_| {});
        assert_eq!(count(&r.args, "[v_social]") + count(&r.args, "[v_porto]"), 2);
        assert!(r.args.iter().any(|a| a == SOCIAL));
        assert!(r.args.iter().any(|a| a == PORTO));
        assert!((r.social_duration.unwrap() - (9.0 + 6.0 / 30.0)).abs() < 1e-9);
        assert_eq!(r.porto_duration, Some(9.0));
    }

    #[test]
    fn frame_only_on_social_by_default() {
        let a = ResolvedAssets { frame: Some("C:/lib/frame.png".into()), ..Default::default() };
        let r = build_with(|_| {}, a.clone());
        assert_eq!(count(&r.args, "C:/lib/frame.png"), 1);
        let both = build_with(|s| s.frame.porto = true, a);
        assert_eq!(count(&both.args, "C:/lib/frame.png"), 2);
    }

    #[test]
    fn intro_from_timestamp_with_sticker() {
        let a = ResolvedAssets { intro_overlay: Some("C:/lib/sticker.png".into()), ..Default::default() };
        let r = build_with(|s| s.intro.timestamp = 1.25, a);
        let g = graph_of(&r.args);
        assert!(r.args.iter().any(|x| x == "1.25"));
        assert!(g.contains("loop=loop=5:size=1"));
        assert!(g.contains("concat=n=2:v=1:a=0"));
        assert!(r.args.iter().any(|x| x == "C:/lib/sticker.png"));
    }

    #[test]
    fn skips_intro_when_disabled() {
        let r = build(|s| s.intro.enabled = false);
        assert!(!graph_of(&r.args).contains("concat"));
        assert_eq!(r.social_duration, Some(9.0));
    }

    #[test]
    fn image_background_cover_crop() {
        let a = ResolvedAssets { background: Some("C:/lib/bg.png".into()), ..Default::default() };
        let r = build_with(|s| s.background.mode = BgMode::Image, a);
        assert!(graph_of(&r.args).contains("force_original_aspect_ratio=increase:flags=lanczos,crop=1920:1080"));
    }

    #[test]
    fn music_with_fades_social_only_unless_porto() {
        let a = ResolvedAssets { audio: Some("C:/lib/m.mp3".into()), ..Default::default() };
        let r = build_with(|s| s.audio.mode = AudioMode::Replace, a.clone());
        assert!(r.args.iter().any(|x| x == "[a_social]"));
        assert!(!r.args.iter().any(|x| x == "[a_porto]"));
        assert!(graph_of(&r.args).contains("afade=t=out:st=8.2:d=1"));
        let both = build_with(
            |s| {
                s.audio.mode = AudioMode::Replace;
                s.audio.porto = true;
            },
            a,
        );
        assert!(both.args.iter().any(|x| x == "[a_porto]"));
    }

    #[test]
    fn h265_with_hvc1_for_porto() {
        let r = build(|s| s.quality.porto_codec = Codec::H265);
        assert!(r.args.iter().any(|x| x == "libx265"));
        assert!(r.args.iter().any(|x| x == "hvc1"));
        assert!(pos(&r.args, "libx265") > pos(&r.args, SOCIAL));
    }

    #[test]
    fn escapes_windows_paths_for_drawtext() {
        assert_eq!(escape_filter_path("C:\\Users\\a'b\\f.ttf"), "'C\\:/Users/a'\\\\\\''b/f.ttf'");
        let r = build(|s| s.text.enabled = true);
        assert!(graph_of(&r.args).contains("fontfile='C\\:/Windows/Fonts/arial.ttf'"));
    }

    #[test]
    fn single_output() {
        let s = RenderSettings::default();
        let p = probe();
        let r = build_render_args(&BuildOptions {
            input: Path::new(INPUT),
            probe: &p,
            settings: &s,
            assets: &ResolvedAssets::default(),
            social: None,
            porto: Some(PORTO.into()),
            text_file: None,
            default_font: "x".into(),
            preview_seconds: None,
        })
        .unwrap();
        assert!(!r.args.iter().any(|x| x == "[v_social]"));
        assert!(!graph_of(&r.args).contains("split"));
    }

    #[test]
    fn preview_limits_duration() {
        let s = RenderSettings::default();
        let p = probe();
        let r = build_render_args(&BuildOptions {
            input: Path::new(INPUT),
            probe: &p,
            settings: &s,
            assets: &ResolvedAssets::default(),
            social: Some(SOCIAL.into()),
            porto: None,
            text_file: None,
            default_font: "x".into(),
            preview_seconds: Some(3.0),
        })
        .unwrap();
        assert_eq!(r.total_duration, 3.0);
        assert!(r.args.windows(2).any(|w| w[0] == "-t" && w[1] == "3"));
        assert!(r.args.iter().any(|x| x == "veryfast"));
    }

    #[test]
    fn fmt_matches_js() {
        assert_eq!(fmt(30.0), "30");
        assert_eq!(fmt(8.2), "8.2");
        assert_eq!(fmt(1.0 / 3.0), "0.333");
        assert_eq!(fmt(0.0), "0");
    }

    #[test]
    fn sticker_keeps_designed_size() {
        assert_eq!(sticker_scale(1920, 1080), 1.0);
        assert_eq!(sticker_scale(1280, 720), 1280.0 / 1920.0);
        assert!((sticker_scale(1080, 1920) - 0.5625).abs() < 1e-9);
    }

}
