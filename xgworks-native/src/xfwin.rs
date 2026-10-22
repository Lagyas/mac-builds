//! XF Information Editor (the original's xfh.dll plug-in): a property sheet
//! with Karaoke Display / Song information / Other, laid out like its three
//! dialog pages. The data is `xgworks_core::xfinfo`.

use eframe::egui;
use xgworks_core::xfinfo::{self, XfInfo, BEATS, CATEGORIES, COUNTRIES, LANGUAGES, VOCALS};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Karaoke,
    Song,
    Other,
}

pub struct XfDialog {
    page: Page,
    pub info: XfInfo,
}

impl XfDialog {
    pub fn new(info: XfInfo) -> Self {
        XfDialog { page: Page::Karaoke, info }
    }
}

/// A combo over `items` storing the item's code (the part before ':'),
/// or the whole text when `whole`; the first entry is empty.
fn code_combo(ui: &mut egui::Ui, id: &str, v: &mut String, items: &[&str], whole: bool, width: f32) {
    let shown = items.iter().find(|e| if whole { **e == v.as_str() } else { xfinfo::code(e) == v.as_str() }).copied().unwrap_or(v.as_str()).to_string();
    egui::ComboBox::from_id_source(id).width(width).selected_text(shown).show_ui(ui, |ui| {
        ui.selectable_value(v, String::new(), " ");
        for e in items {
            let val = if whole { e.to_string() } else { xfinfo::code(e).to_string() };
            ui.selectable_value(v, val, *e);
        }
    });
}

fn ch_combo(ui: &mut egui::Ui, id: &str, v: &mut Option<u8>) {
    let shown = v.map_or("-".to_string(), |c| c.to_string());
    egui::ComboBox::from_id_source(id).width(40.0).selected_text(shown).show_ui(ui, |ui| {
        ui.selectable_value(v, None, "-");
        for c in 1..=16u8 {
            ui.selectable_value(v, Some(c), c.to_string());
        }
    });
}

fn row(ui: &mut egui::Ui, label: &str, v: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(v).desired_width(260.0));
    ui.end_row();
}

/// Show the dialog; Some(true) = OK, Some(false) = Cancel.
pub fn ui(ctx: &egui::Context, d: &mut XfDialog) -> Option<bool> {
    let mut result = None;
    let mut open = true;
    egui::Window::new("XF Information Editor").id(egui::Id::new("xf_dialog")).collapsible(false).resizable(false)
        .pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center()).open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut d.page, Page::Karaoke, "Karaoke Display");
                ui.selectable_value(&mut d.page, Page::Song, "Song information");
                ui.selectable_value(&mut d.page, Page::Other, "Other");
            });
            ui.separator();
            let x = &mut d.info;
            ui.set_min_size(egui::vec2(360.0, 330.0));
            match d.page {
                Page::Karaoke => {
                    egui::Grid::new("xf_k").num_columns(2).spacing([8.0, 10.0]).show(ui, |ui| {
                        ui.label("Song");
                        ui.add(egui::TextEdit::multiline(&mut x.k_song).desired_width(260.0).desired_rows(2));
                        ui.end_row();
                        row(ui, "Composer", &mut x.k_composer);
                        row(ui, "Lyricist", &mut x.k_lyricist);
                        row(ui, "Arranger", &mut x.k_arranger);
                        row(ui, "Performer", &mut x.k_performer);
                        row(ui, "Programmer", &mut x.k_programmer);
                        ui.label("Language");
                        code_combo(ui, "xf_klang", &mut x.k_language, &LANGUAGES, false, 250.0);
                        ui.end_row();
                    });
                }
                Page::Song => {
                    egui::Grid::new("xf_s").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
                        ui.label("Year/Month/Date");
                        ui.add(egui::TextEdit::singleline(&mut x.date).desired_width(110.0).hint_text("1999/12/31"));
                        ui.end_row();
                        let countries: Vec<&str> = COUNTRIES.split('|').collect();
                        ui.label("Country");
                        code_combo(ui, "xf_country", &mut x.country, &countries, false, 250.0);
                        ui.end_row();
                        ui.label("Category");
                        code_combo(ui, "xf_cat", &mut x.category, &CATEGORIES, true, 250.0);
                        ui.end_row();
                        ui.label("Beat");
                        code_combo(ui, "xf_beat", &mut x.beat, &BEATS, true, 250.0);
                        ui.end_row();
                        ui.label("Instrument ( GM )");
                        ui.add(egui::DragValue::new(&mut x.instrument).range(1..=128));
                        ui.end_row();
                        ui.label("Vocal Type");
                        code_combo(ui, "xf_vocal", &mut x.vocal, &VOCALS, false, 250.0);
                        ui.end_row();
                        row(ui, "Composer", &mut x.composer);
                        row(ui, "Lyricist", &mut x.lyricist);
                        row(ui, "Arranger", &mut x.arranger);
                        row(ui, "Performer", &mut x.performer);
                        row(ui, "Programmer", &mut x.programmer);
                    });
                }
                Page::Other => {
                    egui::Grid::new("xf_o").num_columns(2).spacing([8.0, 10.0]).show(ui, |ui| {
                        row(ui, "Sequence Name", &mut x.seq_name);
                        row(ui, "Copyright", &mut x.copyright);
                    });
                    ui.add_space(6.0);
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.label("Time / Tempo");
                        ui.horizontal(|ui| {
                            ui.label("Time");
                            ui.add(egui::DragValue::new(&mut x.time_sig.0).range(1..=255));
                            ui.label("/");
                            egui::ComboBox::from_id_source("xf_den").width(40.0).selected_text(x.time_sig.1.to_string()).show_ui(ui, |ui| {
                                for dd in [1u8, 2, 4, 8, 16, 32] {
                                    ui.selectable_value(&mut x.time_sig.1, dd, dd.to_string());
                                }
                            });
                            ui.add_space(30.0);
                            ui.label("Tempo");
                            ui.add(egui::DragValue::new(&mut x.tempo).range(10..=400));
                        });
                    });
                    ui.add_space(6.0);
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.label("Lyrics information");
                        ui.horizontal(|ui| {
                            ui.label("Melody I ch.");
                            ch_combo(ui, "xf_m1", &mut x.melody1);
                            ui.add_space(20.0);
                            ui.label("Melody II ch.");
                            ch_combo(ui, "xf_m2", &mut x.melody2);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Lyrics display offset");
                            ui.add(egui::DragValue::new(&mut x.lyrics_offset).range(-9999..=9999));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Language (lyrics)");
                            code_combo(ui, "xf_llang", &mut x.lyrics_language, &LANGUAGES, false, 230.0);
                        });
                    });
                }
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_sized([80.0, 22.0], egui::Button::new("Cancel")).clicked() {
                    result = Some(false);
                }
                if ui.add_sized([80.0, 22.0], egui::Button::new("OK")).clicked() {
                    result = Some(true);
                }
            });
        });
    if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        result = Some(false);
    }
    result
}
