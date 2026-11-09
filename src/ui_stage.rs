//! Center: live preview composed from layers (background, video, sticker,
//! frame, text) using the same layout math as the ffmpeg render, plus the
//! filmstrip used to pick the thumbnail frame.

use crate::app::{AppView, StageMode};
use crate::assets::icon;
use crate::ui_inspector::INSPECTOR_WIDTH;
use crate::ui_queue::QUEUE_WIDTH;
use crate::widgets::{format_time, rgb_color, segmented};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_ce_motion::{Motion, Target};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::Input;
use gpui_component::{ActiveTheme, Sizable, h_flex, v_flex};
use showcase_core::layout::{HAlign, VAlign, compute_layout, expand_text, parse_hex_color, text_anchor};
use showcase_core::model::*;

const STAGE_PAD: f32 = 24.;
const HEADER_H: f32 = 52.;
const FOOTER_H: f32 = 132.;
const TITLEBAR_H: f32 = 34.;

impl AppView {
    pub fn render_stage(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let has_job = self.job().map(|j| j.probe.is_some()).unwrap_or(false);

        let header = h_flex()
            .h(px(HEADER_H))
            .px(px(STAGE_PAD))
            .gap_3()
            .items_center()
            .child(div().w(px(300.)).child(segmented(
                "stage-mode",
                &[(StageMode::Social, "Sosmed"), (StageMode::Porto, "Porto"), (StageMode::Thumbnail, "Thumbnail")],
                self.stage,
                cx,
                |this, v, _, cx| this.set_stage(v, cx),
            )))
            .child(div().flex_1())
            .when(self.job().is_some(), |d| {
                d.child(
                    h_flex()
                        .gap_2()
                        .child(div().w(px(180.)).child(Input::new(&self.inputs[&crate::app::InputId::Name]).small().prefix(icon("film").xsmall())))
                        .child(div().w(px(180.)).child(Input::new(&self.inputs[&crate::app::InputId::Client]).small().prefix(icon("pencil").xsmall()))),
                )
            });

        let body = if has_job {
            self.render_canvas(window, cx).into_any_element()
        } else {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(theme.muted_foreground)
                .child(icon("image").size(px(32.)))
                .child(div().text_sm().child(if self.jobs.is_empty() {
                    "Tambahkan video untuk melihat preview"
                } else {
                    "Menyiapkan preview…"
                }))
                .into_any_element()
        };

        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(theme.muted.opacity(0.5))
            .child(header)
            .child(div().flex_1().min_h_0().flex().items_center().justify_center().child(body))
            .child(self.render_stage_footer(cx))
    }

    /// Size of the preview box that fits the window between the side panels.
    fn stage_box(&self, out_w: u32, out_h: u32, window: &Window) -> (f32, f32) {
        let vp = window.viewport_size();
        let avail_w = (f32::from(vp.width) - QUEUE_WIDTH - INSPECTOR_WIDTH - STAGE_PAD * 2.).max(200.);
        let avail_h = (f32::from(vp.height) - TITLEBAR_H - HEADER_H - FOOTER_H - STAGE_PAD).max(150.);
        let k = (avail_w / out_w as f32).min(avail_h / out_h as f32);
        (out_w as f32 * k, out_h as f32 * k)
    }

    fn render_canvas(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let job = self.job().expect("job");
        let probe = job.probe.clone().expect("probe");
        let s = self.effective_settings(job).unwrap_or_default();
        let client = job.client.clone();
        let name = job.name.clone();
        let thumb_image = job.thumb_image.clone();
        let preset_name = self.current_preset().map(|p| p.name.clone()).unwrap_or_default();
        let l = compute_layout(&s, &probe);
        let (bw, bh) = self.stage_box(l.width, l.height, window);
        let k = bw / l.width as f32;
        let mode = self.stage;
        let target = match mode {
            StageMode::Porto => OutputKind::Porto,
            _ => OutputKind::Social,
        };
        let intro = mode == StageMode::Thumbnail;
        let base = rgb_color(parse_hex_color(&s.background.color).unwrap_or((0, 0, 0)));

        let mut canvas = div().relative().w(px(bw)).h(px(bh)).overflow_hidden().bg(base).shadow_lg();

        // Background
        if intro && thumb_image.is_some() {
            canvas = canvas.child(img(thumb_image.clone().unwrap()).absolute().size_full().object_fit(ObjectFit::Cover));
        } else {
            match s.background.mode {
                BgMode::Image => {
                    if let Some(a) = self.asset(s.background.asset_id.as_deref()) {
                        let fit = if s.background.fit == Fit::Cover { ObjectFit::Cover } else { ObjectFit::Contain };
                        canvas = canvas.child(img(a.file.clone()).absolute().size_full().object_fit(fit));
                    }
                }
                BgMode::Blur => {
                    if let Some(b) = self.preview.blur_bg.clone() {
                        canvas = canvas.child(img(b).absolute().size_full().object_fit(ObjectFit::Fill));
                    }
                }
                BgMode::Color => {}
            }
            // Video
            if let Some(frame) = self.preview.frame.clone() {
                canvas = canvas.child(
                    img(frame)
                        .absolute()
                        .left(px(l.x as f32 * k))
                        .top(px(l.y as f32 * k))
                        .w(px(l.vw as f32 * k))
                        .h(px(l.vh as f32 * k))
                        .object_fit(ObjectFit::Fill),
                );
            }
        }
        // Thumbnail sticker
        if intro {
            if let Some(a) = self.asset(s.intro.overlay_asset_id.as_deref()) {
                // Same sizing rule as the render: designed size on a 1920x1080 canvas.
                let ks = showcase_core::graph::sticker_scale(l.width, l.height) as f32;
                let (sw, sh) = (a.width.unwrap_or(l.width) as f32 * ks, a.height.unwrap_or(l.height) as f32 * ks);
                let fit = (l.width as f32 / sw).min(l.height as f32 / sh).min(1.0);
                let (dw, dh) = (sw * fit * k, sh * fit * k);
                canvas = canvas.child(
                    img(a.file.clone())
                        .absolute()
                        .left(px((bw - dw) / 2.0))
                        .top(px((bh - dh) / 2.0))
                        .w(px(dw))
                        .h(px(dh))
                        .object_fit(ObjectFit::Fill),
                );
            }
        }
        // Frame / watermark
        let frame_on = match target {
            OutputKind::Social => s.frame.social,
            OutputKind::Porto => s.frame.porto,
        };
        if frame_on {
            if let Some(a) = self.asset(s.frame.asset_id.as_deref()) {
                canvas = canvas.child(img(a.file.clone()).absolute().size_full().object_fit(ObjectFit::Fill));
            }
        }
        // Credit text
        let text_on = s.text.enabled
            && match target {
                OutputKind::Social => s.text.social,
                OutputKind::Porto => s.text.porto,
            };
        if text_on {
            let text = expand_text(&s.text.text, &client, &name, &preset_name);
            canvas = canvas.child(self.render_text_layer(&s, text, l.height as f32, k, bw, bh));
        }

        if intro && !s.intro.enabled {
            canvas = canvas.child(
                div()
                    .absolute()
                    .top_2()
                    .left_2()
                    .px_2()
                    .py_1()
                    .rounded(theme.radius)
                    .bg(theme.warning)
                    .text_color(theme.warning_foreground)
                    .text_xs()
                    .child("Thumbnail awal sedang mati"),
            );
        }

        // Cross-fade when switching Sosmed / Porto / Thumbnail.
        let fade = self.motion(format!("stage-{mode:?}")).frame(
            Motion::new(Target::default().opacity(1.0)).initial(Target::default().opacity(0.4)),
            window,
            cx,
        );
        fade.apply(canvas)
    }

    fn render_text_layer(&self, s: &RenderSettings, text: String, out_h: f32, k: f32, bw: f32, bh: f32) -> impl IntoElement {
        let t = &s.text;
        let size = ((t.size / 100.0) * out_h).round().max(6.0) * k;
        let margin = (t.margin / 100.0) * out_h * k;
        let (h, v) = text_anchor(t.position);
        let color = rgb_color(parse_hex_color(&t.color).unwrap_or((255, 255, 255)));
        let outline = rgb_color(parse_hex_color(&t.outline_color).unwrap_or((0, 0, 0)));
        let family = t.font_asset_id.as_ref().and_then(|id| self.font_families.get(id).cloned());
        let bold = family.is_none();
        let ow = (t.outline_width * k).max(if t.outline_width > 0.0 { 0.6 } else { 0.0 });

        // One copy of the label, positioned by offset.
        let label = |dx: f32, dy: f32, c: Hsla| {
            let mut d = div()
                .absolute()
                .left(px(dx))
                .top(px(dy))
                .whitespace_nowrap()
                .text_size(px(size))
                .line_height(px(size * 1.2))
                .text_color(c)
                .child(text.clone());
            if let Some(f) = &family {
                d = d.font_family(f.clone());
            }
            if bold {
                d = d.font_weight(FontWeight::BOLD);
            }
            d
        };
        // Outline: offset copies around the fill (GPUI has no text stroke).
        let mut stack = div().relative().child(label(0., 0., transparent_black()).relative());
        if t.shadow {
            let sd = (size / 18.0).max(1.0);
            stack = stack.child(label(sd, sd, black().opacity(0.6)));
        }
        if ow > 0.0 {
            for i in 0..12 {
                let a = i as f32 / 12.0 * std::f32::consts::TAU;
                stack = stack.child(label(a.cos() * ow, a.sin() * ow, outline));
            }
        }
        stack = stack.child(label(0., 0., color));

        let mut wrap = div().absolute().w(px(bw)).h(px(bh)).flex().p(px(margin));
        wrap = match h {
            HAlign::Left => wrap.justify_start(),
            HAlign::Center => wrap.justify_center(),
            HAlign::Right => wrap.justify_end(),
        };
        wrap = match v {
            VAlign::Top => wrap.items_start(),
            VAlign::Middle => wrap.items_center(),
            VAlign::Bottom => wrap.items_end(),
        };
        wrap.child(stack)
    }

    fn render_stage_footer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let Some(job) = self.job() else {
            return div().h(px(FOOTER_H)).into_any_element();
        };
        let duration = job.probe.as_ref().map(|p| p.duration).unwrap_or(0.0);
        let thumb_t = job.thumb_time;
        let custom_thumb = job.thumb_image.is_some();
        let strip = self.filmstrips.get(&job.id).cloned().unwrap_or_default();
        let playing = self.preview.playing && self.stage != StageMode::Thumbnail;
        let time = self.preview.time;

        // Index of the filmstrip cell nearest the chosen thumbnail time.
        let chosen = (!custom_thumb)
            .then(|| {
                strip
                    .iter()
                    .enumerate()
                    .min_by(|a, b| (a.1.0 - thumb_t).abs().partial_cmp(&(b.1.0 - thumb_t).abs()).unwrap())
                    .map(|(i, _)| i)
            })
            .flatten();

        let mut cells = h_flex().gap_1().w_full().h(px(62.));
        for (i, (t, image)) in strip.into_iter().enumerate() {
            let is_chosen = chosen == Some(i);
            cells = cells.child(
                div()
                    .id(("film", i))
                    .relative()
                    .flex_1()
                    .h_full()
                    .rounded(theme.radius - px(2.))
                    .overflow_hidden()
                    .bg(theme.background)
                    .cursor_pointer()
                    .border_2()
                    .border_color(if is_chosen { theme.primary } else { transparent_black() })
                    .hover(|d| d.border_color(theme.ring))
                    .child(img(image).size_full().object_fit(ObjectFit::Contain))
                    .when(is_chosen, |d| {
                        d.child(
                            div()
                                .absolute()
                                .bottom(px(2.))
                                .left(px(2.))
                                .px_1()
                                .rounded_sm()
                                .bg(theme.primary)
                                .text_color(theme.primary_foreground)
                                .child(icon("pin").xsmall()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.set_thumbnail_time(t, cx))),
            );
        }

        v_flex()
            .h(px(FOOTER_H))
            .px(px(STAGE_PAD))
            .pb_3()
            .gap_2()
            .justify_end()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("play")
                            .ghost()
                            .small()
                            .icon(icon(if playing { "pause" } else { "play" }))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_play(cx))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("{} / {}", format_time(if self.stage == StageMode::Thumbnail { thumb_t } else { time }), format_time(duration))),
                    )
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(theme.muted_foreground).child(if custom_thumb {
                        "Thumbnail: gambar sendiri".to_string()
                    } else {
                        format!("Thumbnail di {}  ·  klik frame untuk mengganti", format_time(thumb_t))
                    })),
            )
            .child(cells)
            .into_any_element()
    }
}
