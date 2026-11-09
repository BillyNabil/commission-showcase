//! Small filesystem helpers.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::model::VIDEO_EXTENSIONS;

/// Default font for the credit text when no library font is chosen.
pub fn default_font() -> PathBuf {
    let windir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:/Windows"));
    let dir = windir.join("Fonts");
    for f in ["arialbd.ttf", "arial.ttf", "segoeui.ttf"] {
        let p = dir.join(f);
        if p.exists() {
            return p;
        }
    }
    dir.join("arial.ttf")
}

fn is_video(p: &Path) -> bool {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.as_str()))
}

/// Expand dropped/selected files and folders (up to depth 3) into video files.
/// Sorted and de-duplicated.
pub fn expand_video_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(p: &Path, depth: u32, out: &mut BTreeSet<PathBuf>) {
        let Ok(meta) = std::fs::metadata(p) else { return };
        if meta.is_dir() {
            if depth > 3 {
                return;
            }
            let Ok(rd) = std::fs::read_dir(p) else { return };
            let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
            entries.sort();
            for e in entries {
                walk(&e, depth + 1, out);
            }
        } else if is_video(p) {
            out.insert(p.to_path_buf());
        }
    }
    let mut out = BTreeSet::new();
    for p in paths {
        walk(p, 0, &mut out);
    }
    out.into_iter().collect()
}

/// %APPDATA%\CommissionShowcase (shared with the earlier Electron version).
pub fn app_data_root() -> PathBuf {
    // Portable / demo mode: keep library, styles and settings in another folder.
    if let Some(dir) = std::env::var_os("CS_DATA_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join(".config")
    });
    base.join("CommissionShowcase")
}

/// %USERPROFILE%\Videos\Commission Showcase
pub fn default_output_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Videos").join("Commission Showcase")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_folders_and_filters() {
        let root = std::env::temp_dir().join(format!("showcase-core-expand-{}", crate::store::new_id()));
        let sub = root.join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        for f in [root.join("x.webm"), root.join("note.txt"), sub.join("y.MOV")] {
            std::fs::write(f, b"").unwrap();
        }
        let got = expand_video_paths(&[root.clone(), root.join("x.webm")]);
        assert_eq!(got, vec![sub.join("y.MOV"), root.join("x.webm")]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
