//! Small building blocks shared by the panels. Everything uses theme tokens so
//! the app follows the Windows light/dark setting.

use crate::app::{AppView, SliderId};
use crate::assets::icon;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::slider::Slider;
use gpui_component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use showcase_core::model::{AssetKind, RenderSettings};

/// A titled group inside the inspector.
pub fn group(title: impl Into<SharedString>, cx: &App) -> Div {
    v_flex().gap_2().child(
        div().text_xs().font_medium().text_color(cx.theme().muted_foreground).child(title.into()),
    )
}

pub fn hint(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text.into())
}

/// Segmented control. `on_pick` receives the chosen value.
pub fn segmented<T: Copy + PartialEq + 'static>(
    id: &'static str,
    options: &[(T, &'static str)],
    value: T,
    cx: &mut Context<AppView>,
    on_pick: impl Fn(&mut AppView, T, &mut Window, &mut Context<AppView>) + 'static + Clone,
) -> Div {
    let theme = cx.theme().clone();
    let mut row = h_flex().p(px(2.)).gap(px(2.)).rounded(theme.radius).bg(theme.muted).w_full();
    for (i, (v, label)) in options.iter().enumerate() {
        let v = *v;
        let selected = v == value;
        let f = on_pick.clone();
        row = row.child(
            div()
                .id((id, i))
                .flex_1()
                .h(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radius - px(2.))
                .text_xs()
                .cursor_pointer()
                .when(selected, |d| d.bg(theme.background).text_color(theme.foreground).shadow_xs())
                .when(!selected, |d| d.text_color(theme.muted_foreground).hover(|d| d.text_color(theme.foreground)))
                .child(*label)
                .on_click(cx.listener(move |this, _, window, cx| f(this, v, window, cx))),
        );
    }
    row
}

/// Label + slider + numeric value.
pub fn slider_row(this: &AppView, label: &'static str, id: SliderId, s: &RenderSettings, unit: &'static str, cx: &App) -> Div {
    let v = id.get(s);
    let shown = if v.abs() >= 10.0 || v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") };
    h_flex()
        .gap_3()
        .items_center()
        .child(div().w(px(76.)).flex_shrink_0().text_sm().text_color(cx.theme().muted_foreground).child(label))
        .child(div().flex_1().child(Slider::new(&this.sliders[&id])))
        .child(
            div()
                .w(px(44.))
                .flex_shrink_0()
                .text_right()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{shown}{unit}")),
        )
}

/// Label on the left, control on the right.
pub fn field(label: &'static str, control: impl IntoElement, cx: &App) -> Div {
    h_flex()
        .gap_3()
        .items_center()
        .child(div().w(px(76.)).flex_shrink_0().text_sm().text_color(cx.theme().muted_foreground).child(label))
        .child(div().flex_1().min_w_0().child(control))
}

/// Grid of image assets with a "none" tile and an "add" tile.
pub fn image_grid(
    this: &AppView,
    kind: AssetKind,
    value: Option<&str>,
    none_label: Option<&'static str>,
    cx: &mut Context<AppView>,
    on_pick: impl Fn(&mut AppView, Option<String>, &mut Context<AppView>) + 'static + Clone,
) -> Div {
    let theme = cx.theme().clone();
    let tile = |id: ElementId, selected: bool| {
        div()
            .id(id)
            .w(px(98.))
            .h(px(56.))
            .flex_shrink_0()
            .rounded(theme.radius)
            .overflow_hidden()
            .border_2()
            .border_color(if selected { theme.primary } else { theme.border })
            .bg(theme.muted)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
    };
    let mut grid = h_flex().flex_wrap().gap_2();
    if let Some(label) = none_label {
        let f = on_pick.clone();
        grid = grid.child(
            tile(ElementId::Name(format!("none-{kind:?}").into()), value.is_none())
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| f(this, None, cx))),
        );
    }
    for a in this.assets_of(kind) {
        let id = a.id.clone();
        let f = on_pick.clone();
        let fit = if kind == AssetKind::Background { ObjectFit::Cover } else { ObjectFit::Contain };
        let confirm = this.confirm_delete.as_deref() == Some(a.id.as_str());
        let del_id = a.id.clone();
        grid = grid.child(
            tile(ElementId::Name(format!("asset-{}", a.id).into()), value == Some(a.id.as_str()))
                .relative()
                .group("tile")
                .child(img(a.file.clone()).size_full().object_fit(fit))
                .child(
                    div()
                        .id(ElementId::Name(format!("del-{}", a.id).into()))
                        .absolute()
                        .top(px(3.))
                        .right(px(3.))
                        .size(px(18.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(if confirm { theme.danger } else { theme.background.opacity(0.85) })
                        .text_color(if confirm { theme.danger_foreground } else { theme.foreground })
                        .when(!confirm, |d| d.invisible().group_hover("tile", |d| d.visible()))
                        .child(icon(if confirm { "trash" } else { "x" }).xsmall())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if this.confirm_delete.as_deref() == Some(del_id.as_str()) {
                                this.remove_asset(&del_id.clone(), cx);
                            } else {
                                this.confirm_delete = Some(del_id.clone());
                                cx.notify();
                            }
                        })),
                )
                .on_click(cx.listener(move |this, _, _, cx| f(this, Some(id.clone()), cx))),
        );
    }
    grid.child(
        tile(ElementId::Name(format!("add-{kind:?}").into()), false)
            .border_dashed()
            .text_color(theme.muted_foreground)
            .hover(|d| d.text_color(theme.foreground))
            .child(icon("plus").small())
            .on_click(cx.listener(move |this, _, window, cx| this.pick_assets(kind, window, cx))),
    )
}

/// List of non-visual assets (music, fonts).
pub fn asset_list(
    this: &AppView,
    kind: AssetKind,
    value: Option<&str>,
    none_label: &'static str,
    cx: &mut Context<AppView>,
    on_pick: impl Fn(&mut AppView, Option<String>, &mut Context<AppView>) + 'static + Clone,
) -> Div {
    let theme = cx.theme().clone();
    let row = |id: ElementId, selected: bool| {
        h_flex()
            .id(id)
            .h(px(30.))
            .px_2()
            .gap_2()
            .rounded(theme.radius)
            .cursor_pointer()
            .text_sm()
            .when(selected, |d| d.bg(theme.accent).text_color(theme.accent_foreground))
            .when(!selected, |d| d.hover(|d| d.bg(theme.muted)))
    };
    let mut list = v_flex().gap(px(2.));
    let f = on_pick.clone();
    list = list.child(
        row(ElementId::Name(format!("none-{kind:?}").into()), value.is_none())
            .child(div().flex_1().text_color(theme.muted_foreground).child(none_label))
            .on_click(cx.listener(move |this, _, _, cx| f(this, None, cx))),
    );
    for a in this.assets_of(kind) {
        let id = a.id.clone();
        let f = on_pick.clone();
        let selected = value == Some(a.id.as_str());
        let family = this.font_families.get(&a.id).cloned();
        let meta = a.duration.map(|d| format!("{:.0}:{:02.0}", (d / 60.0).floor(), d % 60.0)).unwrap_or_default();
        let confirm = this.confirm_delete.as_deref() == Some(a.id.as_str());
        let del_id = a.id.clone();
        list = list.child(
            row(ElementId::Name(format!("asset-{}", a.id).into()), selected)
                .group("row")
                .child(icon(if kind == AssetKind::Audio { "music" } else { "type" }).small())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .when_some(family, |d, fam| d.font_family(fam))
                        .child(a.name.clone()),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(meta))
                .child(
                    div()
                        .id(ElementId::Name(format!("del-{}", a.id).into()))
                        .px_1()
                        .rounded_sm()
                        .text_xs()
                        .when(confirm, |d| d.text_color(theme.danger).child("Hapus?"))
                        .when(!confirm, |d| d.invisible().group_hover("row", |d| d.visible()).child(icon("x").xsmall()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if this.confirm_delete.as_deref() == Some(del_id.as_str()) {
                                this.remove_asset(&del_id.clone(), cx);
                            } else {
                                this.confirm_delete = Some(del_id.clone());
                                cx.notify();
                            }
                        })),
                )
                .on_click(cx.listener(move |this, _, _, cx| f(this, Some(id.clone()), cx))),
        );
    }
    list.child(
        row(ElementId::Name(format!("add-{kind:?}").into()), false)
            .text_color(theme.muted_foreground)
            .child(icon("plus").small())
            .child(if kind == AssetKind::Audio { "Tambah musik…" } else { "Tambah font…" })
            .on_click(cx.listener(move |this, _, window, cx| this.pick_assets(kind, window, cx))),
    )
}

/// Row of colour swatches + hex input.
pub fn swatches(
    current: &str,
    input: &Entity<gpui_component::input::InputState>,
    cx: &mut Context<AppView>,
    on_pick: impl Fn(&mut AppView, String, &mut Window, &mut Context<AppView>) + 'static + Clone,
) -> Div {
    const COLORS: [&str; 8] = ["#ffffff", "#1d1b26", "#000000", "#f4ede4", "#ffd6e7", "#cfe8ff", "#d7f5d0", "#ffe9a8"];
    let theme = cx.theme().clone();
    let mut row = h_flex().gap_1p5().items_center();
    for (i, c) in COLORS.iter().enumerate() {
        let rgb = showcase_core::layout::parse_hex_color(c).unwrap_or((0, 0, 0));
        let selected = current.eq_ignore_ascii_case(c);
        let f = on_pick.clone();
        let c = c.to_string();
        row = row.child(
            div()
                .id(("swatch", i))
                .size(px(20.))
                .rounded_full()
                .cursor_pointer()
                .border_1()
                .border_color(theme.border)
                .when(selected, |d| d.border_2().border_color(theme.primary))
                .bg(rgb_color(rgb))
                .on_click(cx.listener(move |this, _, window, cx| f(this, c.clone(), window, cx))),
        );
    }
    row.child(div().w(px(92.)).child(gpui_component::input::Input::new(input).small()))
}

pub fn rgb_color((r, g, b): (u8, u8, u8)) -> Hsla {
    gpui::rgb_to_hsla(gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32))
}

pub fn format_bytes(n: u64) -> String {
    if n < 1024 * 1024 {
        format!("{} KB", n / 1024)
    } else {
        format!("{:.1} MB", n as f64 / 1024.0 / 1024.0)
    }
}

pub fn format_time(t: f64) -> String {
    let m = (t / 60.0).floor();
    format!("{}:{:04.1}", m as u32, t - m * 60.0)
}
