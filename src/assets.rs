//! App icon set (simple line icons) + the component library's bundled icons.
use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

const ICONS: &[(&str, &[u8])] = &[
    ("app/alert.svg", include_bytes!("../assets/icons/alert.svg")),
    ("app/check.svg", include_bytes!("../assets/icons/check.svg")),
    ("app/clock.svg", include_bytes!("../assets/icons/clock.svg")),
    ("app/copy.svg", include_bytes!("../assets/icons/copy.svg")),
    ("app/export.svg", include_bytes!("../assets/icons/export.svg")),
    ("app/film.svg", include_bytes!("../assets/icons/film.svg")),
    ("app/folder-open.svg", include_bytes!("../assets/icons/folder-open.svg")),
    ("app/folder-plus.svg", include_bytes!("../assets/icons/folder-plus.svg")),
    ("app/frame-border.svg", include_bytes!("../assets/icons/frame-border.svg")),
    ("app/image.svg", include_bytes!("../assets/icons/image.svg")),
    ("app/layers.svg", include_bytes!("../assets/icons/layers.svg")),
    ("app/music.svg", include_bytes!("../assets/icons/music.svg")),
    ("app/mute.svg", include_bytes!("../assets/icons/mute.svg")),
    ("app/pause.svg", include_bytes!("../assets/icons/pause.svg")),
    ("app/pencil.svg", include_bytes!("../assets/icons/pencil.svg")),
    ("app/pin.svg", include_bytes!("../assets/icons/pin.svg")),
    ("app/play.svg", include_bytes!("../assets/icons/play.svg")),
    ("app/plus.svg", include_bytes!("../assets/icons/plus.svg")),
    ("app/repeat.svg", include_bytes!("../assets/icons/repeat.svg")),
    ("app/settings.svg", include_bytes!("../assets/icons/settings.svg")),
    ("app/sliders.svg", include_bytes!("../assets/icons/sliders.svg")),
    ("app/stop.svg", include_bytes!("../assets/icons/stop.svg")),
    ("app/trash.svg", include_bytes!("../assets/icons/trash.svg")),
    ("app/type.svg", include_bytes!("../assets/icons/type.svg")),
    ("app/upload.svg", include_bytes!("../assets/icons/upload.svg")),
    ("app/video-plus.svg", include_bytes!("../assets/icons/video-plus.svg")),
    ("app/volume.svg", include_bytes!("../assets/icons/volume.svg")),
    ("app/x.svg", include_bytes!("../assets/icons/x.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().find(|(p, _)| *p == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out: Vec<SharedString> = ICONS
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(*p))
            .collect();
        out.extend(gpui_component_assets::Assets.list(path)?);
        Ok(out)
    }
}

/// Icon from the app set, e.g. `icon("image")`.
pub fn icon(name: &str) -> gpui_component::Icon {
    gpui_component::Icon::empty().path(SharedString::from(format!("app/{name}.svg")))
}
