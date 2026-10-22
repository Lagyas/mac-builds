//! File and settings access: native file system / dialogs, or the browser
//! (picked files arrive asynchronously, saves become downloads, settings in
//! localStorage).

pub type Slot<T> = std::rc::Rc<std::cell::RefCell<Option<T>>>;

/// Files picked by the user: (relative name, bytes).
pub type Files = Vec<(String, Vec<u8>)>;

/// File name part of a picked file's name (natively a full path).
pub fn base(n: &str) -> &str {
    n.rsplit(['/', '\\']).next().unwrap_or(n)
}

/// The picked name is a real path that can be read again (native).
pub fn is_path(n: &str) -> bool {
    n.contains('/') || n.contains('\\')
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use std::path::{Path, PathBuf};

    fn settings_path() -> PathBuf {
        let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("hs2-native.ini")
    }

    pub fn load_settings() -> Option<String> {
        std::fs::read_to_string(settings_path()).ok()
    }

    pub fn save_settings(text: &str) {
        let _ = std::fs::write(settings_path(), text);
    }

    pub fn pick_files(slot: &Slot<Files>, _ctx: &eframe::egui::Context, filter: &str, exts: &[&str]) {
        if let Some(paths) = rfd::FileDialog::new().add_filter(filter, exts).pick_files() {
            let v = paths
                .into_iter()
                // natively the "name" is the full path (kept for settings)
                .filter_map(|p| Some((p.to_string_lossy().into_owned(), std::fs::read(&p).ok()?)))
                .collect();
            *slot.borrow_mut() = Some(v);
        }
    }

    /// Pick a folder: natively returns its path (content is read from disk).
    pub fn pick_folder(slot: &Slot<String>, _ctx: &eframe::egui::Context) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            *slot.borrow_mut() = Some(p.to_string_lossy().into_owned());
        }
    }

    /// All files under `dir` (relative path with '/', bytes), skipping big
    /// protected audio banks (Samples/Wavetables/Drawbars: names only, first 256 KB).
    pub fn read_tree(dir: &str) -> Files {
        fn walk(base: &Path, d: &Path, out: &mut Files) {
            let Ok(rd) = std::fs::read_dir(d) else { return };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(base, &p, out);
                    continue;
                }
                let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                let audio = ["Samples/", "Wavetables/", "Drawbars/"].iter().any(|k| rel.contains(k));
                let data = if audio {
                    use std::io::Read;
                    let mut b = vec![0u8; 256 * 1024];
                    let n = std::fs::File::open(&p).and_then(|mut f| f.read(&mut b)).unwrap_or(0);
                    b.truncate(n);
                    b
                } else {
                    match std::fs::metadata(&p) {
                        Ok(m) if m.len() < 64 * 1024 * 1024 => std::fs::read(&p).unwrap_or_default(),
                        _ => continue,
                    }
                };
                out.push((rel, data));
            }
        }
        let mut out = Vec::new();
        walk(Path::new(dir), Path::new(dir), &mut out);
        out
    }

    pub fn read(path: &str) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }

    pub fn take_added() -> Files {
        Vec::new()
    }

    pub fn save_file(default_name: &str, ext: &str, filter: &str, bytes: &[u8]) -> Option<String> {
        let path = rfd::FileDialog::new().set_file_name(default_name).add_filter(filter, &[ext]).save_file()?;
        Some(match std::fs::write(&path, bytes) {
            Ok(_) => format!("Saved {}", path.display()),
            Err(e) => format!("Save failed: {e}"),
        })
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn load_settings() -> Option<String> {
        storage()?.get_item("hs2-native.ini").ok()?
    }

    pub fn save_settings(text: &str) {
        if let Some(s) = storage() {
            let _ = s.set_item("hs2-native.ini", text);
        }
    }

    pub fn pick_files(slot: &Slot<Files>, ctx: &eframe::egui::Context, filter: &str, exts: &[&str]) {
        let (slot, ctx) = (slot.clone(), ctx.clone());
        let exts: Vec<String> = exts.iter().map(|s| s.to_string()).collect();
        let filter = filter.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(files) = rfd::AsyncFileDialog::new().add_filter(&filter, &exts).pick_files().await {
                let mut v = Vec::new();
                for f in files {
                    let b = f.read().await;
                    v.push((f.file_name(), b));
                }
                *slot.borrow_mut() = Some(v);
                ctx.request_repaint();
            }
        });
    }

    static ADDED: std::sync::Mutex<Vec<(String, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

    /// The page adds content files (chosen with a folder picker) here.
    pub fn add_file(path: &str, bytes: Vec<u8>) {
        ADDED.lock().unwrap().push((path.to_string(), bytes));
    }

    pub fn take_added() -> Files {
        std::mem::take(&mut *ADDED.lock().unwrap())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(inline_js = "export function pick_folder_js() { if (globalThis.hs2PickFolder) globalThis.hs2PickFolder(); }")]
    extern "C" {
        fn pick_folder_js();
    }

    /// Browser: the page opens its folder picker (webkitdirectory) and adds
    /// the files with `add_file`; see `index.html`.
    pub fn pick_folder(_slot: &Slot<String>, _ctx: &eframe::egui::Context) {
        pick_folder_js();
    }

    pub fn read_tree(_dir: &str) -> Files {
        Vec::new()
    }

    pub fn read(_path: &str) -> Option<Vec<u8>> {
        None
    }

    pub fn save_file(default_name: &str, _ext: &str, _filter: &str, bytes: &[u8]) -> Option<String> {
        use wasm_bindgen::JsCast;
        let arr = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type("application/octet-stream");
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &opts).ok()?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
        let doc = web_sys::window()?.document()?;
        let a = doc.create_element("a").ok()?.dyn_into::<web_sys::HtmlAnchorElement>().ok()?;
        a.set_href(&url);
        a.set_download(default_name);
        a.click();
        let _ = web_sys::Url::revoke_object_url(&url);
        Some(format!("Downloaded {default_name}"))
    }
}

pub use imp::*;
