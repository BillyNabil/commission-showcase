//! Prints render progress with timestamps: cargo run --example progress -- <video>
use showcase_core::model::*;
use showcase_core::render::{render_job, Bins, CancelToken, RenderJob};
use std::path::PathBuf;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let input = PathBuf::from(std::env::args().nth(1).expect("video"));
    let bins = Bins { ffmpeg: "ffmpeg".into(), ffprobe: "ffprobe".into() };
    let probe = showcase_core::probe::probe(&bins.ffprobe, &input)?;
    let out = std::env::temp_dir().join("cs-progress");
    let job = RenderJob {
        job_id: "p".into(), input, probe, name: "p".into(), client: "x".into(), profile_name: "x".into(),
        settings: RenderSettings::default(), assets: ResolvedAssets::default(), out_dir: out.clone(),
        file_template: "{name}".into(), temp_dir: out.clone(), default_font: showcase_core::util::default_font(), preview_seconds: None,
    };
    let t0 = Instant::now();
    let r = render_job(&bins, &job, &mut |p| println!("{:>5.1}s {:.3}", t0.elapsed().as_secs_f32(), p), &CancelToken::new());
    println!("done {:?} in {:.1}s", r.map(|r| r.outputs.len()), t0.elapsed().as_secs_f32());
    let _ = std::fs::remove_dir_all(out);
    Ok(())
}
