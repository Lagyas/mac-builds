//! Job ▸ Demix by Channel / Demix by Note (not in XGworks; as XGW Next's
//! Track menu): split tracks with `xgworks_core::demix`, undo-able. The new
//! tracks go right after the track they came from; open editors, the
//! selection and Mute / Solo follow.

use super::App;
use eframe::egui;
use xgworks_core::demix;

pub struct DemixDlg {
    by_note: bool,
    all_tracks: bool,
    /// Demix by Note: bit k = MIDI channel k+1.
    channels: u16,
}

impl DemixDlg {
    /// The dialog for the selected track (Demix by Note: its channel ticked).
    pub fn new(by_note: bool, ch: u8) -> DemixDlg {
        DemixDlg { by_note, all_tracks: false, channels: if by_note { 1 << (ch & 0x0F) } else { 0xFFFF } }
    }
}

impl App {
    /// Job menu entries.
    pub(super) fn demix_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("Demix by Channel...").on_hover_text("One track per MIDI channel (format 0 / mixed tracks)").clicked() {
            self.demix_dlg = Some(DemixDlg { by_note: false, all_tracks: false, channels: 0xFFFF });
            ui.close_menu();
        }
        if ui.button("Demix by Note...").on_hover_text("One track per note, e.g. each drum instrument").clicked() {
            let ch = self.edit.get(self.sel_track).map_or(9, |t| t.channel & 0x0F);
            self.demix_dlg = Some(DemixDlg { by_note: true, all_tracks: false, channels: 1 << ch });
            ui.close_menu();
        }
    }

    pub(super) fn demix_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.demix_dlg.take() else { return };
        let title = if d.by_note { "Demix by Note" } else { "Demix by Channel" };
        let vis = self.visible_tracks();
        let sel = vis.iter().position(|&v| v == self.sel_track).map(|k| (k + 1, self.edit[self.sel_track].name.clone()));
        let mut act = 0; // 1 ok, 2 cancel
        let open = super::sub_window(ctx, ("demix_dlg", title), title, [330.0, if d.by_note { 230.0 } else { 150.0 }], |ui| {
            match &sel {
                Some((n, name)) => ui.radio_value(&mut d.all_tracks, false, format!("Selected track: {n} {name}")),
                None => ui.add_enabled(false, egui::RadioButton::new(false, "Selected track")),
            };
            ui.radio_value(&mut d.all_tracks, true, "All tracks");
            if sel.is_none() {
                d.all_tracks = true;
            }
            if d.by_note {
                ui.add_space(6.0);
                ui.label("MIDI channels:");
                egui::Grid::new("demix_ch").spacing([4.0, 2.0]).show(ui, |ui| {
                    for c in 0..16u16 {
                        let mut on = d.channels & (1 << c) != 0;
                        if ui.checkbox(&mut on, format!("{:2}", c + 1)).changed() {
                            d.channels ^= 1 << c;
                        }
                        if c % 8 == 7 {
                            ui.end_row();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        d.channels = 0xFFFF;
                    }
                    if ui.small_button("None").clicked() {
                        d.channels = 0;
                    }
                });
            } else {
                ui.add_space(4.0);
                ui.weak("Tracks with notes or controllers on several channels\nare split into one track per channel.");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!d.by_note || d.channels != 0, egui::Button::new(egui::RichText::new("  OK  ").strong())).clicked() {
                    act = 1;
                }
                if ui.button("Cancel").clicked() {
                    act = 2;
                }
            });
        });
        if act == 1 {
            self.run_demix(&d);
        } else if open && act == 0 {
            self.demix_dlg = Some(d);
        }
    }

    fn run_demix(&mut self, d: &DemixDlg) {
        let which: Vec<usize> = if d.all_tracks { (0..self.edit.len()).collect() } else { vec![self.sel_track] };
        let res = if d.by_note {
            // drum names of the track's kit, as in the Drum window
            let label = |ti: usize, ch: u8, note: u8| {
                let (msb, prog) = self.mix.get(ti).map_or((0, 0), |m| (m.bank_msb, m.program));
                let msb = if msb == 0 && ch == 9 { 127 } else { msb };
                self.list_names.drum(msb, prog, note).map(str::to_string)
            };
            demix::by_note(&self.edit, &which, d.channels, label)
        } else {
            demix::by_channel(&self.edit, &which)
        };
        let r = match res {
            Ok(r) => r,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if r.count == 0 {
            self.status = if d.by_note { "No notes on those channels.".into() } else { "The tracks are already one channel each.".into() };
            return;
        }
        self.tv_snapshot();
        // old index -> first new track made from it; flags follow their track
        let mut first = vec![0usize; self.edit.len()];
        for (n, &o) in r.origin.iter().enumerate().rev() {
            first[o] = n;
        }
        self.flags = r.origin.iter().map(|&o| self.flags.get(o).copied().unwrap_or((false, false))).collect();
        let map = |i: usize| first.get(i).copied().unwrap_or(i);
        self.sel_track = map(self.sel_track);
        for (_, st) in &mut self.open_rolls {
            st.track = map(st.track);
        }
        for (_, st) in &mut self.open_lists {
            for s in &mut st.spans {
                s.track = map(s.track);
            }
        }
        for t in self.rec_tracks.iter_mut().flatten() {
            *t = map(*t);
        }
        self.edit = r.tracks;
        self.blk_sel.clear();
        self.song_edited();
        self.status = if d.by_note {
            format!("Demix by Note: {} note tracks created.", r.count)
        } else {
            format!("Demix by Channel: {} track(s) split.", r.count)
        };
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use xgworks_core::{edit_model, write_mid, EditTrack, Event};

    /// The sample song merged into one format-0 track.
    fn format0() -> Vec<u8> {
        let mid = std::fs::read("../xgworks-core/tests/fixtures/dream.mid").unwrap();
        let (tracks, ppqn) = edit_model(&mid);
        let mut one = EditTrack { name: "All".into(), ..Default::default() };
        for t in &tracks {
            for n in &t.notes {
                one.others.push(Event::Note { t: n.tick, ch: t.channel, note: n.pitch, vel: n.vel, gate: n.dur });
            }
            one.others.extend(t.others.iter().cloned());
        }
        write_mid(&[one], ppqn)
    }

    #[test]
    fn demix_channel_then_note_and_undo() {
        let mut app = App::new();
        app.load(&format0(), "f0.mid");
        let n0 = app.edit.len();
        app.sel_track = 0;
        app.run_demix(&DemixDlg { by_note: false, all_tracks: true, channels: 0xFFFF });
        let n1 = app.edit.len();
        assert!(n1 > n0 + 1, "{n0} -> {n1}");
        assert_eq!(app.flags.len(), n1);
        // the drum channel track, split by note
        let drums = app.edit.iter().position(|t| t.name.ends_with("[ch 10]")).expect("a ch 10 track");
        app.sel_track = drums;
        app.run_demix(&DemixDlg { by_note: true, all_tracks: false, channels: 1 << 9 });
        assert!(app.edit.len() > n1);
        assert!(app.edit.iter().any(|t| t.name.starts_with("CH 10 - ")), "{:?}", app.status);
        app.tv_undo_redo(true);
        assert_eq!(app.edit.len(), n1);
        app.tv_undo_redo(true);
        assert_eq!(app.edit.len(), n0);
    }
}
