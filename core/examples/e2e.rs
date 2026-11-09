//! End-to-end render test without UI: cargo run --example e2e [--dump-args]
//! Uses <repo>/test-assets (create it with scripts/make-test-assets.sh) and writes to %TEMP%/cs-e2e.

use std::path::Path;
use std::time::Instant;

use showcase_core::graph::{build_render_args, BuildOptions};
use showcase_core::model::{AudioMode, BgMode, Codec, QualityPreset, RenderSettings, ResolutionMode, ResolvedAssets};
use showcase_core::probe::probe;
use showcase_core::render::{is_canceled, render_job, Bins, CancelToken, RenderJob};

struct Case {
    file: &'static str,
    name: &'static str,
    tweak: fn(&mut RenderSettings),
    bg: Option<&'static str>,
    frame: Option<&'static str>,
    sticker: Option<&'static str>,
    audio: Option<&'static str>,
}

fn main() -> anyhow::Result<()> {
    let dump = std::env::args().any(|a| a == "--dump-args");
    let cancel_test = std::env::args().any(|a| a == "--cancel");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let assets_dir = manifest.parent().unwrap().join("test-assets");
    let out = std::env::temp_dir().join("cs-e2e");
    let bins = Bins { ffmpeg: "ffmpeg".into(), ffprobe: "ffprobe".into() };
    let a = |n: Option<&str>| n.map(|n| assets_dir.join(n));

    let cases = [
        Case {
            file: "alpha.webm",
            name: "webm_bgimage_frame_text_music",
            tweak: |s| {
                s.background.mode = BgMode::Image;
                s.intro.timestamp = 1.5;
                s.intro.duration = 0.5;
                s.text.enabled = true;
                s.audio.mode = AudioMode::Replace;
                s.outputs.gif = true;
            },
            bg: Some("bg.png"),
            frame: Some("frame.png"),
            sticker: Some("sticker.png"),
            audio: Some("music.mp3"),
        },
        Case {
            file: "prores.mov",
            name: "prores_blur_h265",
            tweak: |s| {
                s.background.mode = BgMode::Blur;
                s.quality.porto_codec = Codec::H265;
                s.intro.porto = true;
                s.loop_.enabled = false;
            },
            bg: None,
            frame: Some("frame.png"),
            sticker: None,
            audio: None,
        },
        Case {
            file: "short.webm",
            name: "short_loop_mix_source",
            tweak: |s| {
                s.resolution.mode = ResolutionMode::Source;
                s.audio.mode = AudioMode::Mix;
                s.audio.porto = true;
                s.quality.preset = QualityPreset::Small;
            },
            bg: None,
            frame: None,
            sticker: None,
            audio: Some("music.mp3"),
        },
    ];

    for c in &cases {
        let mut s = RenderSettings::default();
        (c.tweak)(&mut s);
        let p = probe(&bins.ffprobe, &assets_dir.join(c.file))?;
        let assets = ResolvedAssets {
            background: a(c.bg),
            frame: a(c.frame),
            intro_overlay: a(c.sticker),
            font: None,
            audio: a(c.audio),
        };
        if dump {
            let r = build_render_args(&BuildOptions {
                input: &p.path,
                probe: &p,
                settings: &s,
                assets: &assets,
                social: Some(out.join(format!("{}_social.mp4", c.name))),
                porto: Some(out.join(format!("{}_porto.mp4", c.name))),
                text_file: s.text.enabled.then(|| out.join("tmp").join(format!("text_{}.txt", c.name))),
                default_font: "C:/Windows/Fonts/arial.ttf".into(),
                preview_seconds: None,
            })?;
            println!("{}", serde_json::to_string(&r.args)?);
            continue;
        }
        let t0 = Instant::now();
        let job = RenderJob {
            job_id: c.name.into(),
            input: p.path.clone(),
            probe: p.clone(),
            name: c.name.into(),
            client: "Budi".into(),
            profile_name: "Test".into(),
            settings: s,
            assets,
            out_dir: out.clone(),
            file_template: "{name}".into(),
            temp_dir: out.join("tmp"),
            default_font: "C:/Windows/Fonts/arial.ttf".into(),
            preview_seconds: None,
        };
        if cancel_test {
            // Cancel ~1s into the render; partial outputs must be removed.
            let token = CancelToken::new();
            let t2 = token.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                t2.cancel();
            });
            let mut seen = 0.0f32;
            let err = render_job(&bins, &job, &mut |x| seen = x, &token).expect_err("should cancel");
            let leftovers = ["social.mp4", "porto.mp4", "thumb.jpg"]
                .iter()
                .filter(|s| out.join(format!("{}_{s}", c.name)).exists())
                .count();
            println!("{} canceled={} msg={:?} progress_at_cancel={seen:.2} leftovers={leftovers} after {:.1}s",
                c.name, is_canceled(&err), err.to_string(), t0.elapsed().as_secs_f64());
            break;
        }
        let mut last = 0.0f32;
        let mut ticks: Vec<f32> = vec![];
        let r = render_job(&bins, &job, &mut |x| {
            last = x;
            if x > 0.0 && x < 1.0 { ticks.push(x) }
        }, &CancelToken::new())?;
        let sample: Vec<String> = ticks.iter().step_by((ticks.len() / 5).max(1)).map(|t| format!("{t:.2}")).collect();
        println!("{} {:.1}s alpha={} {:?} progress_ticks={} {:?} end={last}", c.name, t0.elapsed().as_secs_f64(), p.has_alpha, p.needs_libvpx, ticks.len(), sample);
        for (f, size) in r.outputs {
            println!("   {} {} KB", f.file_name().unwrap().to_string_lossy(), size / 1024);
        }
    }
    Ok(())
}
