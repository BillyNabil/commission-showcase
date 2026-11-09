//! Layout math shared by the ffmpeg graph builder and the live preview, so the
//! preview matches the rendered output.

use crate::model::{ProbeInfo, RenderSettings, ResolutionMode, TextPosition, RESOLUTION_PRESETS};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Size and position of the video inside the output
    pub vw: u32,
    pub vh: u32,
    pub x: i32,
    pub y: i32,
    /// How many times the source plays (loop)
    pub plays: u32,
    pub source_duration: f64,
    pub main_duration: f64,
    pub intro_frames: u32,
    pub intro_duration: f64,
}

fn even(n: f64) -> u32 {
    let v = ((n / 2.0).round() * 2.0) as i64;
    v.max(2) as u32
}

/// JS-style Math.round (half rounds up toward +inf).
fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

pub fn output_size(s: &RenderSettings, src_w: u32, src_h: u32) -> (u32, u32) {
    match s.resolution.mode {
        ResolutionMode::Source => {
            let w = if src_w > 0 { src_w } else { 1920 };
            let h = if src_h > 0 { src_h } else { 1080 };
            (even(w as f64), even(h as f64))
        }
        ResolutionMode::Preset => {
            let p = RESOLUTION_PRESETS
                .iter()
                .find(|p| p.id == s.resolution.preset)
                .unwrap_or(&RESOLUTION_PRESETS[0]);
            (p.width, p.height)
        }
        ResolutionMode::Custom => {
            let w = if s.resolution.width > 0 { s.resolution.width } else { 1920 };
            let h = if s.resolution.height > 0 { s.resolution.height } else { 1080 };
            (even(w as f64), even(h as f64))
        }
    }
}

pub fn compute_layout(s: &RenderSettings, probe: &ProbeInfo) -> Layout {
    let (width, height) = output_size(s, probe.width, probe.height);
    let mut fps = if s.fps > 0.0 { s.fps } else if probe.fps > 0.0 { probe.fps } else { 30.0 };
    fps = (js_round(fps * 1000.0) / 1000.0).clamp(1.0, 60.0);

    let k = (s.video.scale as f64).max(1.0) / 100.0;
    let pw = if probe.width > 0 { probe.width } else { width } as f64;
    let ph = if probe.height > 0 { probe.height } else { height } as f64;
    let fit = (width as f64 / pw).min(height as f64 / ph) * k;
    let vw = (js_round(pw * fit) as i64).max(2) as u32;
    let vh = (js_round(ph * fit) as i64).max(2) as u32;
    let x = js_round((width as f64 - vw as f64) / 2.0 + (s.video.offset_x as f64 / 100.0) * width as f64) as i32;
    let y = js_round((height as f64 - vh as f64) / 2.0 + (s.video.offset_y as f64 / 100.0) * height as f64) as i32;

    let source_duration = if probe.duration > 0.0 { probe.duration } else { 5.0 };
    let mut plays: u32 = 1;
    if s.loop_.enabled {
        let p = if s.loop_.count > 0 {
            s.loop_.count as f64
        } else {
            (s.loop_.min_duration as f64 / source_duration - 1e-6).ceil()
        };
        plays = p.clamp(1.0, 200.0) as u32;
    }
    let main_duration = source_duration * plays as f64;

    let intro_frames = (js_round((s.intro.duration as f64).max(0.0) * fps) as i64).max(1) as u32;
    let intro_duration = intro_frames as f64 / fps;

    Layout { width, height, fps, vw, vh, x, y, plays, source_duration, main_duration, intro_frames, intro_duration }
}

pub fn expand_text(template: &str, client: &str, filename: &str, profile: &str) -> String {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    template
        .replace("{client}", client)
        .replace("{filename}", filename)
        .replace("{profile}", profile)
        .replace("{date}", &date)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

pub fn text_anchor(pos: TextPosition) -> (HAlign, VAlign) {
    use TextPosition::*;
    let h = match pos {
        TopLeft | BottomLeft => HAlign::Left,
        TopRight | BottomRight => HAlign::Right,
        _ => HAlign::Center,
    };
    let v = match pos {
        TopLeft | Top | TopRight => VAlign::Top,
        BottomLeft | Bottom | BottomRight => VAlign::Bottom,
        Center => VAlign::Middle,
    };
    (h, v)
}

/// Parse `#rgb` / `#rrggbb` (leading # optional).
pub fn parse_hex_color(c: &str) -> Option<(u8, u8, u8)> {
    let h = c.trim().trim_start_matches('#');
    if !h.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let full: String = match h.len() {
        3 => h.chars().flat_map(|ch| [ch, ch]).collect(),
        6 => h.to_string(),
        _ => return None,
    };
    let v = u32::from_str_radix(&full, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe() -> ProbeInfo {
        ProbeInfo { width: 1000, height: 1000, duration: 3.0, fps: 30.0, ..Default::default() }
    }

    #[test]
    fn fits_video_inside_output_and_scales() {
        let mut s = RenderSettings::default();
        s.video.scale = 100.0;
        let l = compute_layout(&s, &probe());
        assert_eq!((l.width, l.height), (1920, 1080));
        assert_eq!((l.vw, l.vh, l.x, l.y), (1080, 1080, 420, 0));
    }

    #[test]
    fn loops_with_whole_plays() {
        let mut s = RenderSettings::default();
        s.loop_.min_duration = 8.0;
        assert_eq!(compute_layout(&s, &probe()).plays, 3);
        s.loop_.count = 5;
        assert_eq!(compute_layout(&s, &probe()).main_duration, 15.0);
        s.loop_.enabled = false;
        assert_eq!(compute_layout(&s, &probe()).plays, 1);
    }

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex_color("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("1d1b26"), Some((0x1d, 0x1b, 0x26)));
        assert_eq!(parse_hex_color("#12"), None);
        assert_eq!(parse_hex_color("#zzzzzz"), None);
    }

    #[test]
    fn source_resolution_is_even() {
        let mut s = RenderSettings::default();
        s.resolution.mode = ResolutionMode::Source;
        assert_eq!(output_size(&s, 501, 699), (502, 700));
    }
}
