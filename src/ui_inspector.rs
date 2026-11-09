//! Right panel. Edits the *style* (preset) used by the selected video; every
//! video using the same style updates together. Per-video things (client name,
//! file name, thumbnail frame) live on the stage and the filmstrip.

use crate::app::{AppView, InputId, SliderId, Tab};
use crate::assets::icon;
use crate::widgets::*;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_ce_motion::{Motion, Target};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::Input;
use gpui_component::scroll::ScrollableElement;
use gpui_component::switch::Switch;
use gpui_component::tab::{Tab as UiTab, TabBar};
use gpui_component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex, v_flex};
use showcase_core::layout::compute_layout;
use showcase_core::model::*;

pub const INSPECTOR_WIDTH: f32 = 360.;

const TABS: [(Tab, &str); 6] = [
    (Tab::Background, "Latar"),
    (Tab::Frame, "Frame"),
    (Tab::Thumbnail, "Thumbnail"),
    (Tab::Text, "Teks"),
    (Tab::Audio, "Audio"),
    (Tab::Export, "Ekspor"),
];

impl AppView {
    pub fn render_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let Some(preset) = self.current_preset().cloned() else {
            return v_flex().w(px(INSPECTOR_WIDTH)).into_any_element();
        };
        let s = preset.settings.clone();
        let tab_index = TABS.iter().position(|(t, _)| *t == self.tab).unwrap_or(0);

        let content = match self.tab {
            Tab::Background => self.tab_background(&s, cx).into_any_element(),
            Tab::Frame => self.tab_frame(&s, cx).into_any_element(),
            Tab::Thumbnail => self.tab_thumbnail(&s, cx).into_any_element(),
            Tab::Text => self.tab_text(&s, cx).into_any_element(),
            Tab::Audio => self.tab_audio(&s, cx).into_any_element(),
            Tab::Export => self.tab_export(&s, cx).into_any_element(),
        };
        let tab = self.tab;
        let slide = self.motion(format!("tab-{tab:?}")).frame(
            Motion::new(Target::default().x(0.0).opacity(1.0)).initial(Target::default().x(8.0).opacity(0.0)),
            window,
            cx,
        );

        v_flex()
            .w(px(INSPECTOR_WIDTH))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(self.render_style_bar(&preset, cx))
            .child(
                div().px_2().border_b_1().border_color(theme.border).child(
                    TabBar::new("inspector-tabs")
                        .underline()
                        .small()
                        .selected_index(tab_index)
                        .children(TABS.iter().map(|(_, label)| UiTab::new().label(*label)))
                        .on_click(cx.listener(|this, ix: &usize, _, cx| {
                            this.tab = TABS[*ix].0;
                            this.confirm_delete = None;
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(v_flex().p_4().gap_5().child(slide.apply(v_flex().gap_5().child(content))).overflow_y_scrollbar()),
            )
            .into_any_element()
    }

    /// Style (preset) chips: which saved look this video uses.
    fn render_style_bar(&mut self, preset: &Preset, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current = preset.id.clone();
        let usage = self.preset_usage(&current);
        let is_default = self.settings.default_profile_id.as_deref() == Some(current.as_str());
        let confirm = self.confirm_delete.as_deref() == Some(current.as_str());
        let can_delete = self.presets.len() > 1;

        let mut chips = h_flex().flex_wrap().gap_1p5();
        for (i, p) in self.presets.iter().enumerate() {
            let selected = p.id == current;
            let id = p.id.clone();
            chips = chips.child(
                div()
                    .id(("preset", i))
                    .px_2p5()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .text_xs()
                    .cursor_pointer()
                    .border_1()
                    .when(selected, |d| d.bg(theme.primary).border_color(theme.primary).text_color(theme.primary_foreground))
                    .when(!selected, |d| d.border_color(theme.border).hover(|d| d.bg(theme.muted)))
                    .child(p.name.clone())
                    .on_click(cx.listener(move |this, _, window, cx| this.assign_preset(&id, false, window, cx))),
            );
        }
        chips = chips.child(
            div()
                .id("preset-new")
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_dashed()
                .border_color(theme.border)
                .cursor_pointer()
                .text_color(theme.muted_foreground)
                .hover(|d| d.text_color(theme.foreground))
                .child(icon("plus").xsmall())
                .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new("Gaya baru dari gaya ini").build(window, cx))
                .on_click(cx.listener(|this, _, window, cx| this.duplicate_preset(window, cx))),
        );

        let usage_text = match (self.selected.is_some(), usage) {
            (_, 0) => "Belum dipakai video mana pun".to_string(),
            (_, 1) => "Dipakai 1 video".to_string(),
            (_, n) => format!("Dipakai {n} video — perubahan berlaku untuk semuanya"),
        };

        v_flex()
            .p_4()
            .gap_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .justify_between()
                    .child(div().text_sm().font_semibold().child("Gaya"))
                    .when(self.jobs.len() > 1, |d| {
                        let id = current.clone();
                        d.child(
                            Button::new("apply-all")
                                .ghost()
                                .xsmall()
                                .label("Pakai untuk semua video")
                                .on_click(cx.listener(move |this, _, window, cx| this.assign_preset(&id, true, window, cx))),
                        )
                    }),
            )
            .child(chips)
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&self.inputs[&InputId::PresetName]).small()))
                    .child(
                        Button::new("make-default")
                            .ghost()
                            .xsmall()
                            .label(if is_default { "Default" } else { "Jadikan default" })
                            .disabled(is_default)
                            .tooltip("Video baru otomatis memakai gaya ini")
                            .on_click(cx.listener(|this, _, _, cx| this.set_default_preset(cx))),
                    )
                    .when(can_delete, |d| {
                        let id = current.clone();
                        d.child(
                            Button::new("delete-preset")
                                .ghost()
                                .xsmall()
                                .when(confirm, |b| b.danger().label("Hapus?"))
                                .when(!confirm, |b| b.icon(icon("trash")).tooltip("Hapus gaya ini"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this.confirm_delete.as_deref() == Some(id.as_str()) {
                                        this.delete_preset(&id.clone(), window, cx);
                                    } else {
                                        this.confirm_delete = Some(id.clone());
                                        cx.notify();
                                    }
                                })),
                        )
                    }),
            )
            .child(hint(usage_text, cx))
    }

    fn tab_background(&mut self, s: &RenderSettings, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = s.background.mode;
        let mut col = v_flex().gap_5().child(
            group("Latar belakang", cx).child(segmented(
                "bg-mode",
                &[(BgMode::Image, "Gambar"), (BgMode::Color, "Warna"), (BgMode::Blur, "Blur video")],
                mode,
                cx,
                |this, v, _, cx| this.edit_settings(cx, |s| s.background.mode = v),
            )),
        );
        match mode {
            BgMode::Image => {
                col = col.child(
                    group("Pilih gambar", cx)
                        .child(image_grid(self, AssetKind::Background, s.background.asset_id.as_deref(), None, cx, |this, id, cx| {
                            this.edit_settings(cx, |s| s.background.asset_id = id)
                        }))
                        .child(segmented(
                            "bg-fit",
                            &[(Fit::Cover, "Penuhi layar"), (Fit::Contain, "Tampilkan utuh")],
                            s.background.fit,
                            cx,
                            |this, v, _, cx| this.edit_settings(cx, |s| s.background.fit = v),
                        ))
                        .child(hint("Tip: tarik gambar dari Explorer ke jendela ini untuk menambah.", cx)),
                );
            }
            BgMode::Color => {
                col = col.child(group("Warna", cx).child(swatches(&s.background.color, &self.inputs[&InputId::BgHex], cx, |this, c, window, cx| {
                    this.edit_settings(cx, |s| s.background.color = c);
                    this.sync_controls(window, cx);
                })));
            }
            BgMode::Blur => {
                col = col.child(
                    group("Blur", cx)
                        .child(slider_row(self, "Kekuatan", SliderId::Blur, s, "", cx))
                        .child(hint("Bagian video yang buram dipakai sebagai latar. Untuk video transparan hasilnya seperti glow.", cx)),
                );
            }
        }
        col.child(
            group("Posisi video", cx)
                .child(slider_row(self, "Ukuran", SliderId::Scale, s, "%", cx))
                .child(slider_row(self, "Horizontal", SliderId::OffsetX, s, "%", cx))
                .child(slider_row(self, "Vertikal", SliderId::OffsetY, s, "%", cx))
                .child(
                    h_flex().child(
                        Button::new("reset-pos")
                            .ghost()
                            .xsmall()
                            .label("Kembalikan ke tengah")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.edit_settings(cx, |s| s.video = VideoPlacement::default());
                                this.sync_controls(window, cx);
                            })),
                    ),
                ),
        )
    }

    fn tab_frame(&mut self, s: &RenderSettings, cx: &mut Context<Self>) -> impl IntoElement {
        let (social, porto) = (s.frame.social, s.frame.porto);
        v_flex()
            .gap_5()
            .child(
                group("Frame / watermark", cx)
                    .child(image_grid(self, AssetKind::Frame, s.frame.asset_id.as_deref(), Some("Tanpa frame"), cx, |this, id, cx| {
                        this.edit_settings(cx, |s| s.frame.asset_id = id)
                    }))
                    .child(hint("PNG transparan seukuran hasil (mis. 1920×1080), ditumpuk paling atas.", cx)),
            )
            .child(
                group("Tampilkan di", cx)
                    .child(Checkbox::new("frame-social").small().label("Versi sosmed").checked(social).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.frame.social = v)
                    })))
                    .child(Checkbox::new("frame-porto").small().label("Versi porto").checked(porto).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.frame.porto = v)
                    }))),
            )
    }

    fn tab_thumbnail(&mut self, s: &RenderSettings, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let enabled = s.intro.enabled;
        let frames = self
            .job()
            .and_then(|j| j.probe.as_ref().map(|p| compute_layout(s, p).intro_frames))
            .unwrap_or(0);
        let custom = self.job().and_then(|j| j.thumb_image.clone());
        v_flex()
            .gap_5()
            .child(
                h_flex()
                    .gap_3()
                    .justify_between()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().font_medium().child("Thumbnail di awal video"))
                            .child(hint("Frame diam di depan video, untuk sosmed yang tidak bisa memilih cover.", cx)),
                    )
                    .child(Switch::new("intro-on").checked(enabled).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.intro.enabled = v)
                    }))),
            )
            .child(
                group("Lama tampil", cx)
                    .child(slider_row(self, "Durasi", SliderId::IntroDuration, s, " dtk", cx))
                    .when(frames > 0, |d| d.child(hint(format!("= {frames} frame diam"), cx))),
            )
            .child(
                group("Gambar thumbnail (video ini)", cx)
                    .child(hint("Klik salah satu frame di bawah preview, atau pakai gambar sendiri.", cx))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("thumb-image")
                                    .outline()
                                    .small()
                                    .icon(icon("image"))
                                    .label("Pakai gambar sendiri…")
                                    .disabled(self.selected.is_none())
                                    .on_click(cx.listener(|this, _, window, cx| this.pick_thumb_image(window, cx))),
                            )
                            .when(custom.is_some(), |d| {
                                d.child(
                                    Button::new("thumb-clear")
                                        .ghost()
                                        .small()
                                        .label("Pakai frame video")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.edit_job(cx, |j| j.thumb_image = None);
                                            this.refresh_preview(cx);
                                        })),
                                )
                            }),
                    )
                    .when_some(custom, |d, p| {
                        d.child(
                            div()
                                .w(px(160.))
                                .h(px(90.))
                                .rounded(theme.radius)
                                .overflow_hidden()
                                .border_1()
                                .border_color(theme.border)
                                .child(img(p).size_full().object_fit(ObjectFit::Cover)),
                        )
                    }),
            )
            .child(
                group("Tempelan (sticker / judul)", cx).child(image_grid(
                    self,
                    AssetKind::Overlay,
                    s.intro.overlay_asset_id.as_deref(),
                    Some("Tanpa tempelan"),
                    cx,
                    |this, id, cx| this.edit_settings(cx, |s| s.intro.overlay_asset_id = id),
                )),
            )
            .child(
                group("Tampilkan di", cx)
                    .child(Checkbox::new("intro-social").small().label("Versi sosmed").checked(s.intro.social).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.intro.social = v)
                    })))
                    .child(Checkbox::new("intro-porto").small().label("Versi porto").checked(s.intro.porto).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.intro.porto = v)
                    }))),
            )
    }

    fn tab_text(&mut self, s: &RenderSettings, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let t = s.text.clone();
        let mut grid = v_flex().gap_1();
        let positions = [
            [Some(TextPosition::TopLeft), Some(TextPosition::Top), Some(TextPosition::TopRight)],
            [None, Some(TextPosition::Center), None],
            [Some(TextPosition::BottomLeft), Some(TextPosition::Bottom), Some(TextPosition::BottomRight)],
        ];
        for (r, row) in positions.iter().enumerate() {
            let mut line = h_flex().gap_1();
            for (c, p) in row.iter().enumerate() {
                line = line.child(match p {
                    Some(p) => {
                        let p = *p;
                        let on = p == t.position;
                        div()
                            .id(("pos", r * 3 + c))
                            .w(px(28.))
                            .h(px(18.))
                            .rounded_sm()
                            .cursor_pointer()
                            .border_1()
                            .border_color(if on { theme.primary } else { theme.border })
                            .bg(if on { theme.primary } else { theme.muted })
                            .on_click(cx.listener(move |this, _, _, cx| this.edit_settings(cx, |s| s.text.position = p)))
                            .into_any_element()
                    }
                    None => div().w(px(28.)).h(px(18.)).into_any_element(),
                });
            }
            grid = grid.child(line);
        }

        let mut placeholders = h_flex().gap_1();
        for (i, ph) in ["{client}", "{filename}", "{date}"].iter().enumerate() {
            let ph = ph.to_string();
            placeholders = placeholders.child(
                div()
                    .id(("ph", i))
                    .px_1p5()
                    .rounded_sm()
                    .bg(theme.muted)
                    .text_xs()
                    .cursor_pointer()
                    .hover(|d| d.bg(theme.accent))
                    .child(ph.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let mut v = this.inputs[&InputId::Text].read(cx).value().to_string();
                        if !v.is_empty() && !v.ends_with(' ') {
                            v.push(' ');
                        }
                        v.push_str(&ph);
                        this.edit_settings(cx, |s| s.text.text = v);
                        this.sync_controls(window, cx);
                    })),
            );
        }

        v_flex()
            .gap_5()
            .child(
                h_flex()
                    .gap_3()
                    .justify_between()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().font_medium().child("Teks credit"))
                            .child(hint("Mis. nama klien atau platform kamu.", cx)),
                    )
                    .child(Switch::new("text-on").checked(t.enabled).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.text.enabled = v)
                    }))),
            )
            .child(
                group("Isi", cx)
                    .child(Input::new(&self.inputs[&InputId::Text]))
                    .child(h_flex().gap_2().child(hint("Sisipkan:", cx)).child(placeholders)),
            )
            .child(
                group("Font", cx).child(asset_list(self, AssetKind::Font, t.font_asset_id.as_deref(), "Arial Bold (bawaan)", cx, |this, id, cx| {
                    this.edit_settings(cx, |s| s.text.font_asset_id = id)
                })),
            )
            .child(
                group("Tampilan", cx)
                    .child(slider_row(self, "Ukuran", SliderId::TextSize, s, "%", cx))
                    .child(slider_row(self, "Outline", SliderId::Outline, s, "", cx))
                    .child(swatches(&t.color, &self.inputs[&InputId::TextHex], cx, |this, c, window, cx| {
                        this.edit_settings(cx, |s| s.text.color = c);
                        this.sync_controls(window, cx);
                    }))
                    .child(Checkbox::new("text-shadow").small().label("Bayangan").checked(t.shadow).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.text.shadow = v)
                    }))),
            )
            .child(
                group("Posisi", cx)
                    .child(field("Letak", grid, cx))
                    .child(slider_row(self, "Jarak tepi", SliderId::TextMargin, s, "%", cx)),
            )
            .child(
                group("Tampilkan di", cx)
                    .child(Checkbox::new("text-social").small().label("Versi sosmed").checked(t.social).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.text.social = v)
                    })))
                    .child(Checkbox::new("text-porto").small().label("Versi porto").checked(t.porto).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.text.porto = v)
                    }))),
            )
    }

    fn tab_audio(&mut self, s: &RenderSettings, cx: &mut Context<Self>) -> impl IntoElement {
        let a = s.audio.clone();
        let no_audio = self.job().and_then(|j| j.probe.as_ref()).map(|p| !p.has_audio).unwrap_or(false);
        v_flex()
            .gap_5()
            .child(
                group("Suara", cx)
                    .child(segmented(
                        "audio-mode",
                        &[(AudioMode::None, "Tanpa"), (AudioMode::Original, "Asli"), (AudioMode::Replace, "Musik"), (AudioMode::Mix, "Campur")],
                        a.mode,
                        cx,
                        |this, v, _, cx| this.edit_settings(cx, |s| s.audio.mode = v),
                    ))
                    .when(no_audio && matches!(a.mode, AudioMode::Original | AudioMode::Mix), |d| {
                        d.child(hint("Video ini tidak punya suara.", cx))
                    }),
            )
            .when(matches!(a.mode, AudioMode::Replace | AudioMode::Mix), |d| {
                d.child(
                    group("Musik", cx)
                        .child(asset_list(self, AssetKind::Audio, a.asset_id.as_deref(), "Belum dipilih", cx, |this, id, cx| {
                            this.edit_settings(cx, |s| s.audio.asset_id = id)
                        }))
                        .child(slider_row(self, "Volume", SliderId::Volume, s, "×", cx)),
                )
            })
            .when(a.mode != AudioMode::None, |d| {
                d.child(
                    group("Fade", cx)
                        .child(slider_row(self, "Masuk", SliderId::FadeIn, s, " dtk", cx))
                        .child(slider_row(self, "Keluar", SliderId::FadeOut, s, " dtk", cx)),
                )
                .child(Checkbox::new("audio-porto").small().label("Versi porto juga bersuara").checked(a.porto).on_click(cx.listener(|this, v: &bool, _, cx| {
                    let v = *v;
                    this.edit_settings(cx, |s| s.audio.porto = v)
                })))
            })
    }

    fn tab_export(&mut self, s: &RenderSettings, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let o = s.outputs.clone();
        let layout = self.job().and_then(|j| j.probe.as_ref().map(|p| compute_layout(s, p)));

        let mut sizes = h_flex().flex_wrap().gap_1p5();
        let current_size = match s.resolution.mode {
            ResolutionMode::Source => "source".to_string(),
            ResolutionMode::Custom => "custom".to_string(),
            ResolutionMode::Preset => s.resolution.preset.clone(),
        };
        let mut options: Vec<(String, String)> = RESOLUTION_PRESETS.iter().map(|p| (p.id.to_string(), p.label.to_string())).collect();
        options.push(("source".into(), "Ukuran asli video".into()));
        for (i, (id, label)) in options.into_iter().enumerate() {
            let on = id == current_size;
            sizes = sizes.child(
                div()
                    .id(("size", i))
                    .px_2()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .rounded(theme.radius)
                    .text_xs()
                    .cursor_pointer()
                    .border_1()
                    .when(on, |d| d.border_color(theme.primary).bg(theme.primary.opacity(0.12)))
                    .when(!on, |d| d.border_color(theme.border).hover(|d| d.bg(theme.muted)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let id = id.clone();
                        this.edit_settings(cx, |s| {
                            if id == "source" {
                                s.resolution.mode = ResolutionMode::Source;
                            } else {
                                s.resolution.mode = ResolutionMode::Preset;
                                s.resolution.preset = id;
                            }
                        })
                    })),
            );
        }

        let quality_hint = match s.quality.preset {
            QualityPreset::Small => "File paling kecil, tetap jernih untuk sosmed.",
            QualityPreset::Balanced => "Disarankan: tajam dan tetap ringan.",
            QualityPreset::High => "Detail maksimal untuk lineart halus; file lebih besar.",
        };
        let fps_value = if s.fps <= 0.0 { 0u32 } else { s.fps.round() as u32 };

        v_flex()
            .gap_5()
            .child(
                group("Hasil per video", cx)
                    .child(Checkbox::new("out-social").small().label("Versi sosmed (dengan frame & teks)").checked(o.social).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.outputs.social = v)
                    })))
                    .child(Checkbox::new("out-porto").small().label("Versi porto (bersih)").checked(o.porto).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.outputs.porto = v)
                    })))
                    .child(Checkbox::new("out-thumb").small().label("Gambar thumbnail (.jpg)").checked(o.thumbnail).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.outputs.thumbnail = v)
                    })))
                    .child(Checkbox::new("out-gif").small().label("Animasi GIF (.gif)").checked(o.gif).on_click(cx.listener(|this, v: &bool, _, cx| {
                        let v = *v;
                        this.edit_settings(cx, |s| s.outputs.gif = v)
                    }))),
            )
            .when(o.gif, |d| {
                d.child(
                    group("GIF", cx)
                        .child(field(
                            "Dari versi",
                            segmented(
                                "gif-source",
                                &[(GifSource::Social, "Sosmed"), (GifSource::Porto, "Porto")],
                                s.gif.source,
                                cx,
                                |this, v, _, cx| this.edit_settings(cx, |s| s.gif.source = v),
                            ),
                            cx,
                        ))
                        .child(field(
                            "Lebar",
                            segmented(
                                "gif-width",
                                &[(480u32, "480"), (640, "640"), (800, "800"), (1080, "1080")],
                                s.gif.width,
                                cx,
                                |this, v, _, cx| this.edit_settings(cx, |s| s.gif.width = v),
                            ),
                            cx,
                        ))
                        .child(field(
                            "FPS",
                            segmented(
                                "gif-fps",
                                &[(10u32, "10"), (12, "12"), (15, "15"), (20, "20")],
                                s.gif.fps,
                                cx,
                                |this, v, _, cx| this.edit_settings(cx, |s| s.gif.fps = v),
                            ),
                            cx,
                        ))
                        .child(hint("Dibuat dari video yang sudah jadi, dalam render yang sama. Lebar lebih kecil & FPS lebih rendah = file lebih ringan.", cx)),
                )
            })
            .child(group("Ukuran", cx).child(sizes))
            .child(group("FPS", cx).child(segmented(
                "fps",
                &[(0u32, "Asli"), (24, "24"), (30, "30"), (60, "60")],
                fps_value,
                cx,
                |this, v, _, cx| this.edit_settings(cx, |s| s.fps = v as f64),
            )))
            .child(
                group("Durasi", cx)
                    .child(
                        h_flex()
                            .justify_between()
                            .child(div().text_sm().child("Ulang animasi pendek"))
                            .child(Switch::new("loop-on").checked(s.loop_.enabled).on_click(cx.listener(|this, v: &bool, _, cx| {
                                let v = *v;
                                this.edit_settings(cx, |s| s.loop_.enabled = v)
                            }))),
                    )
                    .when(s.loop_.enabled, |d| d.child(slider_row(self, "Minimal", SliderId::MinLoop, s, " dtk", cx)))
                    .when_some(layout, |d, l| {
                        d.child(hint(format!("{:.1} dtk × {} = {:.1} dtk", l.source_duration, l.plays, l.main_duration), cx))
                    }),
            )
            .child(
                group("Kualitas", cx)
                    .child(segmented(
                        "quality",
                        &[(QualityPreset::Small, "Kecil"), (QualityPreset::Balanced, "Seimbang"), (QualityPreset::High, "Tinggi")],
                        s.quality.preset,
                        cx,
                        |this, v, _, cx| this.edit_settings(cx, |s| s.quality.preset = v),
                    ))
                    .child(hint(quality_hint, cx)),
            )
            .child(
                group("Format versi porto", cx)
                    .child(segmented(
                        "codec",
                        &[(Codec::H264, "H.264 · paling kompatibel"), (Codec::H265, "H.265 · lebih kecil")],
                        s.quality.porto_codec,
                        cx,
                        |this, v, _, cx| this.edit_settings(cx, |s| s.quality.porto_codec = v),
                    )),
            )
            .when_some(layout, |d, l| d.child(hint(format!("Hasil: {}×{} · {} fps", l.width, l.height, l.fps), cx)))
    }
}
