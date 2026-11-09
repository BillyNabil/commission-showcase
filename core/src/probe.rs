//! ffprobe wrappers.

use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

use crate::model::ProbeInfo;

/// Build a Command that never flashes a console window on Windows.
pub fn command(bin: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(bin);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Like [`command`] but at below-normal CPU priority, for long encodes, so the
/// app UI and the rest of the PC stay responsive while a batch renders.
pub fn background_command(bin: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(bin);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000 | 0x0000_4000); // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS
    }
    c
}

fn run(bin: &str, args: &[&str]) -> Result<String> {
    let out = command(bin).args(args).output().with_context(|| format!("Tidak bisa menjalankan {bin}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!(if err.is_empty() { format!("{bin} gagal ({})", out.status) } else { err });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_rate(rate: Option<&str>) -> f64 {
    let Some(r) = rate else { return 0.0 };
    let mut it = r.split('/');
    let n: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    match it.next().and_then(|x| x.parse::<f64>().ok()) {
        Some(d) if d != 0.0 => n / d,
        _ => n,
    }
}

fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn is_alpha_pix_fmt(p: &str) -> bool {
    ["yuva", "rgba", "bgra", "argb", "abgr", "ya", "gbrap", "pal8", "rgba64", "bgra64"]
        .iter()
        .any(|pre| p.starts_with(pre))
}

pub fn probe(ffprobe: &str, path: &Path) -> Result<ProbeInfo> {
    let p = path.to_string_lossy();
    let out = run(ffprobe, &["-v", "error", "-show_format", "-show_streams", "-of", "json", &p])?;
    let data: Value = serde_json::from_str(&out).context("Output ffprobe tidak valid")?;
    let streams = data["streams"].as_array().cloned().unwrap_or_default();
    let format = &data["format"];
    let v = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"].as_i64().unwrap_or(0) == 0)
        .ok_or_else(|| anyhow!("File tidak punya stream video"))?;
    let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");

    let vcodec = v["codec_name"].as_str().unwrap_or("").to_string();
    let pix_fmt = v["pix_fmt"].as_str().unwrap_or("").to_string();
    let tag = |k: &str| v["tags"][k].as_str().map(str::to_string).or_else(|| v["tags"][k].as_i64().map(|n| n.to_string()));
    let alpha_tag = tag("alpha_mode").or_else(|| tag("ALPHA_MODE")).as_deref() == Some("1");
    let needs_libvpx = match (alpha_tag, vcodec.as_str()) {
        (true, "vp9") => Some("libvpx-vp9"),
        (true, "vp8") => Some("libvpx"),
        _ => None,
    };
    let profile = v["profile"].as_str().unwrap_or("");
    let has_alpha = alpha_tag || is_alpha_pix_fmt(&pix_fmt) || (vcodec == "prores" && profile.contains("4444"));

    let mut fps = parse_rate(v["avg_frame_rate"].as_str());
    if fps == 0.0 {
        fps = parse_rate(v["r_frame_rate"].as_str());
    }
    if fps == 0.0 || fps > 240.0 {
        fps = 30.0;
    }

    let mut duration = num(&v["duration"]);
    if duration == 0.0 {
        duration = num(&format["duration"]);
    }
    if duration == 0.0 {
        let frames = num(&v["nb_frames"]);
        if frames > 0.0 {
            duration = frames / fps;
        }
    }

    let mut width = num(&v["width"]) as u32;
    let mut height = num(&v["height"]) as u32;
    let rotation = v["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find(|d| !d["rotation"].is_null()))
        .map(|d| num(&d["rotation"]).abs() as i64)
        .unwrap_or(0);
    if rotation == 90 || rotation == 270 {
        std::mem::swap(&mut width, &mut height);
    }

    Ok(ProbeInfo {
        path: path.to_path_buf(),
        width,
        height,
        duration,
        fps: (fps * 1000.0).round() / 1000.0,
        vcodec,
        pix_fmt,
        has_alpha,
        needs_libvpx,
        has_audio,
        format_name: format["format_name"].as_str().unwrap_or("").to_string(),
    })
}

pub fn probe_image(ffprobe: &str, path: &Path) -> Result<(u32, u32)> {
    let p = path.to_string_lossy();
    let out = run(ffprobe, &["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "json", &p])?;
    let data: Value = serde_json::from_str(&out)?;
    let s = &data["streams"][0];
    Ok((num(&s["width"]) as u32, num(&s["height"]) as u32))
}

pub fn probe_duration(ffprobe: &str, path: &Path) -> Result<f64> {
    let p = path.to_string_lossy();
    let out = run(ffprobe, &["-v", "error", "-show_entries", "format=duration", "-of", "json", &p])?;
    let data: Value = serde_json::from_str(&out)?;
    Ok(num(&data["format"]["duration"]))
}

/// Returns the first line of `ffmpeg -version`.
pub fn check_ffmpeg(ffmpeg: &str) -> Result<String> {
    let out = run(ffmpeg, &["-hide_banner", "-version"])?;
    Ok(out.lines().next().unwrap_or("").to_string())
}
