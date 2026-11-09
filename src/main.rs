#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod player;
mod ui_inspector;
mod ui_queue;
mod ui_stage;
mod widgets;

use app::{AppView, InputId};
use assets::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_ce_motion::{Motion, Target};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::Input;
use gpui_component::{ActiveTheme, Root, Sizable, StyledExt, Theme, TitleBar, h_flex, v_flex};

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.release_retired(window);
        let theme = cx.theme().clone();
        let ffmpeg_missing = matches!(self.ffmpeg, Some(Err(_)));

        let title = TitleBar::new().child(
            h_flex()
                .w_full()
                .pr_2()
                .justify_between()
                .child(
                    h_flex()
                        .gap_2()
                        .child(icon("layers").small().text_color(theme.primary))
                        .child(div().text_sm().font_medium().child("Commission Showcase")),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .when(ffmpeg_missing, |d| {
                            d.child(
                                Button::new("ffmpeg-warn")
                                    .warning()
                                    .xsmall()
                                    .label("ffmpeg tidak ditemukan")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_settings = true;
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            Button::new("settings")
                                .ghost()
                                .small()
                                .icon(icon("settings"))
                                .tooltip("Pengaturan")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_settings = !this.show_settings;
                                    cx.notify();
                                })),
                        ),
                ),
        );

        let main = h_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_queue(window, cx))
            .child(self.render_stage(window, cx))
            .child(self.render_inspector(window, cx));

        v_flex()
            .id("app")
            .size_full()
            .relative()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .child(title)
            .child(main)
            .drag_over::<ExternalPaths>(|style, _, _, cx| style.bg(cx.theme().drop_target))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.on_drop_paths(paths.paths().to_vec(), window, cx);
            }))
            .when(self.show_settings, |d| d.child(self.render_settings(window, cx)))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

impl AppView {
    fn render_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let status = match &self.ffmpeg {
            None => ("Memeriksa ffmpeg…".to_string(), theme.muted_foreground),
            Some(Ok(v)) => (v.clone(), theme.success),
            Some(Err(e)) => (format!("Tidak ditemukan: {e}"), theme.danger),
        };
        let concurrency = self.settings.concurrency;
        let panel = self.motion("settings-panel").frame(
            Motion::new(Target::default().y(0.0).opacity(1.0)).initial(Target::default().y(-8.0).opacity(0.0)),
            window,
            cx,
        );
        let label = |t: &'static str| div().text_xs().font_medium().text_color(theme.muted_foreground).child(t);

        div()
            .id("settings-scrim")
            .absolute()
            .inset_0()
            .bg(black().opacity(0.25))
            .flex()
            .justify_end()
            .pt(px(40.))
            .pr_3()
            .on_click(cx.listener(|this, _, _, cx| {
                this.show_settings = false;
                cx.notify();
            }))
            .child(
                panel.apply(
                    v_flex()
                        .id("settings-panel")
                        .w(px(380.))
                        .h_auto()
                        .max_h(px(560.))
                        .p_4()
                        .gap_4()
                        .rounded(theme.radius * 2.)
                        .bg(theme.popover)
                        .text_color(theme.popover_foreground)
                        .border_1()
                        .border_color(theme.border)
                        .shadow_xl()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(div().text_sm().font_semibold().child("Pengaturan"))
                        .child(
                            v_flex()
                                .gap_1p5()
                                .child(label("ffmpeg"))
                                .child(Input::new(&self.inputs[&InputId::FfmpegPath]).small())
                                .child(label("ffprobe"))
                                .child(Input::new(&self.inputs[&InputId::FfprobePath]).small())
                                .child(div().text_xs().text_color(status.1).line_clamp(2).child(status.0)),
                        )
                        .child(
                            v_flex()
                                .gap_1p5()
                                .child(label("Nama file hasil"))
                                .child(Input::new(&self.inputs[&InputId::Template]).small())
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("{name} {profile} {date} → …_social.mp4, …_porto.mp4, …_thumb.jpg"),
                                ),
                        )
                        .child(
                            v_flex().gap_1p5().child(label("Render bersamaan")).child(widgets::segmented(
                                "concurrency",
                                &[(1u32, "1 video"), (2, "2 video"), (3, "3 video")],
                                concurrency,
                                cx,
                                |this, v, _, cx| {
                                    this.settings.concurrency = v;
                                    let _ = this.store.set_settings(&this.settings);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            h_flex()
                                .justify_end()
                                .gap_2()
                                .child(Button::new("settings-close").ghost().small().label("Tutup").on_click(cx.listener(|this, _, _, cx| {
                                    this.show_settings = false;
                                    cx.notify();
                                })))
                                .child(Button::new("settings-save").primary().small().label("Simpan").on_click(cx.listener(|this, _, window, cx| {
                                    this.save_app_settings(window, cx);
                                }))),
                        ),
                ),
            )
    }
}

fn main() {
    gpui_platform::application().with_assets(assets::Assets).run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1100.), px(700.))),
                    titlebar: Some(TitleBar::title_bar_options()),
                    app_id: Some("commission-showcase".into()),
                    ..Default::default()
                },
                |window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                    window.set_window_title("Commission Showcase");
                    let view = cx.new(|cx| AppView::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("open window");
        })
        .detach();
    });
}
