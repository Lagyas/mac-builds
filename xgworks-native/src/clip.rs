//! Edit shortcuts shared by the editors (Track View, Piano Roll).
//!
//! egui-winit turns Ctrl+C / Ctrl+X into `Event::Copy` / `Event::Cut` and
//! swallows the key, and Ctrl+V only becomes `Event::Paste` when the system
//! clipboard holds text (otherwise nothing at all arrives). So a copy also
//! puts a short text marker on the system clipboard, which makes the next
//! Ctrl+V reach us; the data itself stays in the app's own clipboard.

use eframe::egui;

#[derive(Default, Clone, Copy)]
pub struct EditKeys {
    pub copy: bool,
    pub cut: bool,
    pub paste: bool,
    pub del: bool,
    pub all: bool,
    pub undo: bool,
    pub redo: bool,
}

pub fn edit_keys(ctx: &egui::Context) -> EditKeys {
    ctx.input(|i| {
        let mut e = EditKeys::default();
        for ev in &i.events {
            match ev {
                // the modifiers of the key event itself: a quick Ctrl+A can
                // release Ctrl within the same frame
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    let ctrl = modifiers.command;
                    match key {
                        egui::Key::C if ctrl => e.copy = true,
                        egui::Key::X if ctrl => e.cut = true,
                        egui::Key::V if ctrl => e.paste = true,
                        egui::Key::A if ctrl => e.all = true,
                        egui::Key::Z if ctrl => e.undo = true,
                        egui::Key::Y if ctrl => e.redo = true,
                        egui::Key::Delete => e.del = true,
                        _ => {}
                    }
                }
                egui::Event::Copy => e.copy = true,
                egui::Event::Cut => e.cut = true,
                egui::Event::Paste(_) => e.paste = true,
                _ => {}
            }
        }
        e
    })
}

/// Put a text marker on the system clipboard so that Ctrl+V gets delivered.
pub fn announce(ctx: &egui::Context, what: &str) {
    ctx.copy_text(format!("XGworks {what}"));
}
