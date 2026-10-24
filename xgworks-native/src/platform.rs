//! Platform layer: the original XGworks data files (EXE resources, ws.ini,
//! XGEditor tables …), song file dialogs and the clock.
//!
//! Native reads the XGworks installation on disk. The web build reads a small
//! in-memory file system that the page fills before the app starts (fetched
//! from the server next to the page, or taken from a folder the user picks) —
//! no Yamaha data is shipped with the app on either platform.

use std::path::{Path, PathBuf};

pub use web_time::{Duration, Instant};

/// The XGworks V4.0 installation folder (`XGworks.exe`, `ws.ini`, `XGEditor\` …).
#[cfg(not(target_arch = "wasm32"))]
pub const XG4_DIR: &str = r"F:\rea\XG4";
#[cfg(target_arch = "wasm32")]
pub const XG4_DIR: &str = "XG4";

/// MS Gothic, the List window's font.
#[cfg(not(target_arch = "wasm32"))]
pub const LIST_FONT: &str = r"C:\Windows\Fonts\msgothic.ttc";
#[cfg(target_arch = "wasm32")]
pub const LIST_FONT: &str = "fonts/msgothic.ttc";

/// Fonts a list window can use: (family name, file). Natively the Windows
/// fonts folder; on the web the fonts the page provided.
#[cfg(not(target_arch = "wasm32"))]
pub fn font_files() -> Vec<(String, String)> {
    let mut dirs = vec![std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("Fonts")];
    if cfg!(target_os = "macos") {
        dirs = vec![PathBuf::from("/System/Library/Fonts"), PathBuf::from("/Library/Fonts")];
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(h).join("Library/Fonts"));
        }
    }
    let mut out: Vec<(String, String)> = dirs
        .iter()
        .flat_map(|d| std::fs::read_dir(d).map(|rd| rd.flatten().map(|e| e.path()).collect::<Vec<_>>()).unwrap_or_default())
        .filter(|p| p.extension().is_some_and(|x| matches!(x.to_string_lossy().to_lowercase().as_str(), "ttf" | "ttc" | "otf")))
        .filter_map(|p| {
            let head = std::fs::read(&p).ok()?;
            Some((crate::listwin::font_family_name(&head)?, p.to_string_lossy().into_owned()))
        })
        .collect();
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

#[cfg(target_arch = "wasm32")]
pub fn font_files() -> Vec<(String, String)> {
    read(Path::new(LIST_FONT))
        .and_then(|b| crate::listwin::font_family_name(&b))
        .map(|n| vec![(n, LIST_FONT.to_string())])
        .unwrap_or_default()
}

/// The XGworks installation folder: `XGWORKS_DIR` if set, else the first
/// existing of XG4_DIR, ~/XG4, ~/Documents/XG4, XG4 next to the program /
/// the .app bundle (macOS), else XG4_DIR.
#[cfg(not(target_arch = "wasm32"))]
pub fn xg4_dir() -> PathBuf {
    use std::sync::OnceLock;
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("XGWORKS_DIR") {
            return PathBuf::from(d);
        }
        let mut c = Vec::new();
        if let Some(d) = settings_path().and_then(|p| std::fs::read_to_string(p.with_file_name("xg4_dir.txt")).ok()) {
            c.push(PathBuf::from(d.trim()));
        }
        c.push(PathBuf::from(XG4_DIR));
        if let Some(h) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let h = PathBuf::from(h);
            c.push(h.join("XG4"));
            c.push(h.join("Documents").join("XG4"));
        }
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            c.push(exe_dir.join("XG4"));
            c.push(exe_dir.join("../../../XG4")); // Foo.app/Contents/MacOS → next to the bundle
        }
        c.into_iter().find(|d| d.join("ws.ini").is_file()).unwrap_or_else(|| PathBuf::from(XG4_DIR))
    })
    .clone()
}

/// Whether the XGworks data folder was found (it has ws.ini).
pub fn xg4_found() -> bool {
    read(&xg4("ws.ini")).is_some()
}

/// Ask for the XGworks V4.0 folder (the one with ws.ini and XGEditor\) and
/// remember it; used from the next start.
#[cfg(not(target_arch = "wasm32"))]
pub fn pick_xg4_dir() -> Option<PathBuf> {
    let d = rfd::FileDialog::new().set_title("Choose the XGworks V4.0 folder (with ws.ini)").pick_folder()?;
    let p = settings_path()?.with_file_name("xg4_dir.txt");
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(p, d.to_string_lossy().as_bytes()).ok()?;
    Some(d)
}

#[cfg(target_arch = "wasm32")]
pub fn xg4_dir() -> PathBuf {
    PathBuf::from(XG4_DIR)
}

/// A file of the XGworks installation, e.g. `xg4("ws.ini")`.
pub fn xg4(rel: &str) -> PathBuf {
    xg4_dir().join(rel)
}

/// `XGworks.exe` (its bitmaps / tables are used at run time).
pub fn xg4_exe() -> PathBuf {
    xg4("XGworks.exe")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static VFS: std::cell::RefCell<std::collections::HashMap<String, std::rc::Rc<[u8]>>> = Default::default();
}

#[cfg(target_arch = "wasm32")]
fn vfs_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    s.trim_start_matches("./").trim_start_matches('/').to_string()
}

/// Web: put a file into the virtual file system (`"XG4/ws.ini"`, `"song/x.xws"`).
#[cfg(target_arch = "wasm32")]
pub fn add_file(path: &str, bytes: Vec<u8>) {
    VFS.with(|v| v.borrow_mut().insert(vfs_key(Path::new(path)), bytes.into()));
}

#[cfg(target_arch = "wasm32")]
pub fn read(path: &Path) -> Option<Vec<u8>> {
    VFS.with(|v| v.borrow().get(&vfs_key(path)).map(|b| b.to_vec()))
}

/// The app's own settings file (INI text): natively
/// `%APPDATA%\XGworks Port\settings.ini`, on the web the browser's localStorage.
#[cfg(not(target_arch = "wasm32"))]
fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("XGworks Port").join("settings.ini"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_settings() -> Option<String> {
    std::fs::read_to_string(settings_path()?).ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_settings(text: &str) {
    if let Some(p) = settings_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, text);
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(target_arch = "wasm32")]
pub fn load_settings() -> Option<String> {
    storage()?.get_item("xgworks-settings").ok().flatten()
}

#[cfg(target_arch = "wasm32")]
pub fn save_settings(text: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item("xgworks-settings", text);
    }
}

/// A song picked in the Open dialog: (file name, bytes). Natively the dialog
/// is modal and fills it at once; on the web it arrives a few frames later.
pub type Picked = std::rc::Rc<std::cell::RefCell<Option<(String, Vec<u8>)>>>;

#[cfg(not(target_arch = "wasm32"))]
pub fn open_song(slot: &Picked, _ctx: &eframe::egui::Context) {
    if let Some(path) = rfd::FileDialog::new().add_filter("XGworks / MIDI", &["xws", "XWS", "mid", "MID"]).pick_file() {
        if let Ok(bytes) = std::fs::read(&path) {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
            *slot.borrow_mut() = Some((name, bytes));
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn open_song(slot: &Picked, ctx: &eframe::egui::Context) {
    let (slot, ctx) = (slot.clone(), ctx.clone());
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(f) = rfd::AsyncFileDialog::new().add_filter("XGworks / MIDI", &["xws", "XWS", "mid", "MID"]).pick_file().await {
            let bytes = f.read().await;
            *slot.borrow_mut() = Some((f.file_name(), bytes));
            ctx.request_repaint();
        }
    });
}

/// A file picked in a multi-file dialog: natively its path (read when used),
/// on the web its bytes.
#[derive(Clone)]
pub struct PickedFile {
    pub name: String,
    pub path: Option<String>,
    pub bytes: Option<std::rc::Rc<Vec<u8>>>,
}

impl PickedFile {
    pub fn read(&self) -> Option<Vec<u8>> {
        match (&self.bytes, &self.path) {
            (Some(b), _) => Some(b.to_vec()),
            (None, Some(p)) => read(Path::new(p)),
            _ => None,
        }
    }

    /// What lists show and save: the path, else the name.
    pub fn label(&self) -> &str {
        self.path.as_deref().unwrap_or(&self.name)
    }
}

pub type PickedFiles = std::rc::Rc<std::cell::RefCell<Option<Vec<PickedFile>>>>;

/// Pick several files (`exts` without dots) into `slot`.
#[cfg(not(target_arch = "wasm32"))]
pub fn pick_files(slot: &PickedFiles, _ctx: &eframe::egui::Context, filter: &str, exts: &[&str]) {
    if let Some(paths) = rfd::FileDialog::new().add_filter(filter, exts).pick_files() {
        let v = paths
            .into_iter()
            .map(|p| PickedFile {
                name: p.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string(),
                path: Some(p.to_string_lossy().into_owned()),
                bytes: None,
            })
            .collect();
        *slot.borrow_mut() = Some(v);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn pick_files(slot: &PickedFiles, ctx: &eframe::egui::Context, filter: &str, exts: &[&str]) {
    let (slot, ctx) = (slot.clone(), ctx.clone());
    let exts: Vec<String> = exts.iter().map(|s| s.to_string()).collect();
    let filter = filter.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(files) = rfd::AsyncFileDialog::new().add_filter(&filter, &exts).pick_files().await {
            let mut v = Vec::new();
            for f in files {
                let b = f.read().await;
                v.push(PickedFile { name: f.file_name(), path: None, bytes: Some(std::rc::Rc::new(b)) });
            }
            *slot.borrow_mut() = Some(v);
            ctx.request_repaint();
        }
    });
}

/// Save a song: a save dialog natively, a download on the web.
/// Returns the status line text.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_song(default_name: &str, ext: &str, filter: &str, bytes: &[u8]) -> Option<String> {
    let path = rfd::FileDialog::new().set_file_name(default_name).add_filter(filter, &[ext]).save_file()?;
    Some(match std::fs::write(&path, bytes) {
        Ok(_) => format!("Saved {} ({} bytes).", path.display(), bytes.len()),
        Err(e) => format!("Save failed: {e}"),
    })
}

#[cfg(target_arch = "wasm32")]
pub fn save_song(default_name: &str, _ext: &str, _filter: &str, bytes: &[u8]) -> Option<String> {
    use wasm_bindgen::JsCast;
    let fail = |e: &str| Some(format!("Save failed: {e}"));
    let arr = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("application/octet-stream");
    let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &opts) else { return fail("blob") };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return fail("url") };
    let doc = web_sys::window()?.document()?;
    let Some(a) = doc.create_element("a").ok().and_then(|e| e.dyn_into::<web_sys::HtmlAnchorElement>().ok()) else { return fail("link") };
    a.set_href(&url);
    a.set_download(default_name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Some(format!("Downloaded {default_name} ({} bytes).", bytes.len()))
}
