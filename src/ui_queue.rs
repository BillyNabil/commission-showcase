//! Left panel: the list of videos, output folder and the render button.

use crate::app::{AppView, JobStatus};
use crate::assets::icon;
use crate::widgets::format_bytes;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_ce_motion::{Motion, Target};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex, v_flex};
use gpui_component::scroll::ScrollableElement;

pub const QUEUE_WIDTH: f32 = 288.;

impl AppView {
    pub fn render_queue(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let header = h_flex()
            .h(px(44.))
            .px_3()
            .justify_between()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().text_sm().font_semibold().child("Video"))
                    .when(!self.jobs.is_empty(), |d| {
                        d.child(div().text_xs().text_color(theme.muted_foreground).child(self.jobs.len().to_string()))
                    }),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("add-files")
                            .ghost()
                            .small()
                            .icon(icon("video-plus"))
                            .tooltip("Tambah video")
                            .on_click(cx.listener(|this, _, window, cx| this.pick_videos(false, window, cx))),
                    )
                    .child(
                        Button::new("add-folder")
                            .ghost()
                            .small()
                            .icon(icon("folder-plus"))
                            .tooltip("Tambah semua video dalam folder")
                            .on_click(cx.listener(|this, _, window, cx| this.pick_videos(true, window, cx))),
                    ),
            );

        let list = if self.jobs.is_empty() {
            self.render_empty_queue(cx).into_any_element()
        } else {
            let mut list = v_flex().p_2().gap_1();
            let ids: Vec<String> = self.jobs.iter().map(|j| j.id.clone()).collect();
            for id in ids {
                list = list.child(self.render_job_row(&id, window, cx));
            }
            div().flex_1().min_h_0().child(list.overflow_y_scrollbar()).into_any_element()
        };

        v_flex()
            .w(px(QUEUE_WIDTH))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(header)
            .child(div().flex_1().min_h_0().flex().flex_col().child(list))
            .child(self.render_queue_footer(window, cx))
    }

    fn render_empty_queue(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        v_flex().flex_1().p_3().child(
            v_flex()
                .id("drop-hint")
                .flex_1()
                .items_center()
                .justify_center()
                .gap_3()
                .rounded(theme.radius * 2.)
                .border_1()
                .border_dashed()
                .border_color(theme.border)
                .p_6()
                .cursor_pointer()
                .hover(|d| d.bg(theme.muted))
                .child(icon("film").size(px(28.)).text_color(theme.muted_foreground))
                .child(div().text_sm().font_medium().text_center().child("Tarik video ke sini"))
                .child(
                    div()
                        .text_xs()
                        .text_center()
                        .text_color(theme.muted_foreground)
                        .child("WebM/MOV transparan, MP4, GIF. Boleh banyak file atau satu folder."),
                )
                .on_click(cx.listener(|this, _, window, cx| this.pick_videos(false, window, cx))),
        )
    }

    fn render_job_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let job = self.jobs.iter().find(|j| j.id == id).expect("job");
        let selected = self.selected.as_deref() == Some(id);
        let thumb = self.filmstrips.get(id).and_then(|s| s.first()).map(|(_, img)| img.clone());
        let preset = self.presets.iter().find(|p| p.id == job.preset_id).map(|p| p.name.clone()).unwrap_or_default();
        let meta = job
            .probe
            .as_ref()
            .map(|p| format!("{}×{} · {:.1} dtk", p.width, p.height, p.duration))
            .unwrap_or_else(|| "Membaca…".into());
        let status = job.status;
        let progress = job.progress;
        let error = job.error.clone();
        let outputs = job.outputs.clone();
        let name = job.name.clone();
        let alpha = job.probe.as_ref().map(|p| p.has_alpha).unwrap_or(false);
        let needs_client = job.client.trim().is_empty()
            && self
                .effective_settings(job)
                .map(|s| s.text.enabled && s.text.text.contains("{client}") && (s.text.social || s.text.porto))
                .unwrap_or(false);

        // Entrance motion: rows slide up and fade in when added.
        let enter = self.motion(format!("job-{id}")).frame(
            Motion::new(Target::default().y(0.0).opacity(1.0)).initial(Target::default().y(6.0).opacity(0.0)),
            window,
            cx,
        );
        // Progress fill eases toward the reported value.
        let bar_w = QUEUE_WIDTH - 16.0 - 20.0;
        let bar = self.motion(format!("bar-{id}")).frame(
            Motion::new(Target::default().width(bar_w * progress.clamp(0.0, 1.0)).height(3.0)),
            window,
            cx,
        );

        let id_s = id.to_string();
        let id_rm = id.to_string();
        let id_retry = id.to_string();
        let id_cancel = id.to_string();

        let status_el = match status {
            JobStatus::Waiting => None,
            JobStatus::Rendering => Some(div().text_xs().text_color(theme.primary).child(format!("{:.0}%", progress * 100.0))),
            JobStatus::Done => Some(h_flex().gap_1().text_xs().text_color(theme.success).child(icon("check").xsmall()).child("Selesai")),
            JobStatus::Failed => Some(h_flex().gap_1().text_xs().text_color(theme.danger).child(icon("alert").xsmall()).child("Gagal")),
            JobStatus::Canceled => Some(div().text_xs().text_color(theme.warning).child("Dibatalkan")),
        };

        let row = v_flex()
            .id(ElementId::Name(format!("job-{id}").into()))
            .group("job")
            .p_2()
            .gap_1p5()
            .rounded(theme.radius)
            .cursor_pointer()
            .when(selected, |d| d.bg(theme.sidebar_accent))
            .when(!selected, |d| d.hover(|d| d.bg(theme.sidebar_accent.opacity(0.5))))
            .on_click(cx.listener(move |this, _, window, cx| this.select_job(&id_s, window, cx)))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .w(px(56.))
                            .h(px(36.))
                            .flex_shrink_0()
                            .rounded(theme.radius - px(2.))
                            .overflow_hidden()
                            .bg(theme.muted)
                            .when_some(thumb, |d, img_src| d.child(img(img_src).size_full().object_fit(ObjectFit::Contain))),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().font_medium().truncate().child(name))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(meta)
                                    .when(alpha, |d| d.child("· transparan")),
                            ),
                    )
                    .children(status_el)
                    .when(status != JobStatus::Rendering, |d| {
                        d.child(
                            Button::new(ElementId::Name(format!("rm-{id}").into()))
                                .ghost()
                                .xsmall()
                                .icon(icon("x"))
                                .tooltip("Keluarkan dari daftar")
                                .invisible()
                                .group_hover("job", |s| s.visible())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.remove_job(&id_rm, window, cx)
                                })),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .child(div().text_color(theme.muted_foreground).child(format!("Gaya: {preset}")))
                    .when(needs_client, |d| d.child(div().text_color(theme.warning).child("· nama klien kosong"))),
            )
            .when(status == JobStatus::Rendering, |d| {
                d.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .h(px(3.))
                                .w(px(bar_w))
                                .rounded_full()
                                .bg(theme.muted)
                                .child(bar.apply(div().rounded_full().bg(theme.primary))),
                        )
                        .child(
                            div()
                                .id(ElementId::Name(format!("cancel-{id}").into()))
                                .text_color(theme.muted_foreground)
                                .hover(|d| d.text_color(theme.danger))
                                .child(icon("x").xsmall())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.cancel_job(&id_cancel);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when_some(error, |d, e| {
                let short: String = e.lines().rev().take(2).collect::<Vec<_>>().join(" ");
                d.child(div().text_xs().text_color(theme.danger).line_clamp(2).child(short))
            })
            .when(status == JobStatus::Done && !outputs.is_empty(), |d| {
                let mut files = v_flex().gap(px(2.));
                for (i, (path, size)) in outputs.iter().enumerate() {
                    let label = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    let p = path.clone();
                    files = files.child(
                        h_flex()
                            .id(ElementId::Name(format!("out-{id}-{i}").into()))
                            .gap_2()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .hover(|d| d.text_color(theme.foreground))
                            .child(div().flex_1().truncate().child(label))
                            .child(format_bytes(*size))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                cx.open_with_system(&p)
                            }),
                    );
                }
                d.child(files)
            })
            .when(matches!(status, JobStatus::Failed | JobStatus::Canceled | JobStatus::Done), |d| {
                d.child(
                    h_flex().child(
                        Button::new(ElementId::Name(format!("retry-{id}").into()))
                            .ghost()
                            .xsmall()
                            .label(if status == JobStatus::Done { "Render ulang" } else { "Coba lagi" })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.render_one(&id_retry, window, cx)
                            })),
                    ),
                )
            });

        enter.apply(row)
    }

    fn render_queue_footer(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let pending = self.pending_count();
        let done = self.jobs.iter().filter(|j| j.status == JobStatus::Done).count();
        let out_dir = self.settings.output_dir.clone();
        let out_label = out_dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| out_dir.display().to_string());

        v_flex()
            .p_3()
            .gap_2()
            .border_t_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .id("out-dir")
                    .gap_2()
                    .items_center()
                    .px_2()
                    .h(px(30.))
                    .rounded(theme.radius)
                    .cursor_pointer()
                    .hover(|d| d.bg(theme.muted))
                    .text_xs()
                    .child(icon("folder-open").small().text_color(theme.muted_foreground))
                    .child(div().text_color(theme.muted_foreground).child("Simpan ke"))
                    .child(div().flex_1().min_w_0().truncate().child(out_label))
                    .tooltip_text(out_dir.display().to_string())
                    .on_click(cx.listener(|this, _, window, cx| this.pick_output_dir(window, cx))),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(if self.batch_running {
                        Button::new("stop")
                            .danger()
                            .flex_1()
                            .icon(icon("stop"))
                            .label("Hentikan")
                            .on_click(cx.listener(|this, _, _, cx| this.stop_all(cx)))
                            .into_any_element()
                    } else {
                        Button::new("render")
                            .primary()
                            .flex_1()
                            .icon(icon("export"))
                            .label(if pending > 0 { format!("Render {pending} video") } else { "Render".into() })
                            .disabled(pending == 0)
                            .on_click(cx.listener(|this, _, window, cx| this.render_all(window, cx)))
                            .into_any_element()
                    })
                    .child(
                        Button::new("open-out")
                            .outline()
                            .icon(icon("folder-open"))
                            .tooltip("Buka folder hasil")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let _ = std::fs::create_dir_all(&this.settings.output_dir);
                                cx.open_with_system(&this.settings.output_dir.clone())
                            })),
                    )
                    .when(done > 0 && !self.batch_running, |d| {
                        d.child(
                            Button::new("clear-done")
                                .outline()
                                .icon(icon("check"))
                                .tooltip("Bersihkan yang sudah selesai")
                                .on_click(cx.listener(|this, _, window, cx| this.clear_done(window, cx))),
                        )
                    }),
            )
    }
}

trait TooltipText {
    fn tooltip_text(self, text: String) -> Self;
}
impl TooltipText for Stateful<Div> {
    fn tooltip_text(self, text: String) -> Self {
        self.tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(text.clone()).build(window, cx))
    }
}
