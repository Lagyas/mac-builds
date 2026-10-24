//! List window (XGworks List.hlp, compared with the original's List window,
//! `F:\rea\orig_shots\l_1.png` / `l_2.png`): one row per event — L1/L2/L3
//! location, Type, Value1..3 — for the blocks chosen in the Track View. Rows of
//! the block named in the toolbar can be edited; other blocks are grey.
//!
//! Editing as in the help: click / arrows move the cursor line (blue) and the
//! cell cursor (red); typing + Enter sets a value or location; PgUp/PgDn ±1,
//! Home/End ±10; Ctrl/Shift+click or drag select (green); Del deletes; Insert
//! copies the event (a note one gate later, Shift = same place); Ctrl+C/X/V with
//! a location dialog; right-click = edit menu with Insert Event; Esc clears the
//! selection; Ctrl+F / Ctrl+B jump a measure. Mono / poly monitor sounds notes.

use std::cell::RefCell;
use crate::platform::{Duration, Instant};

use eframe::egui::{self, pos2, vec2, Color32, FontFamily, FontId, Rect, Sense, Stroke};
use xgworks_core::listev::{self, Fmt, ListNames, NewKind, Row, RowText, TimeMap, FILTER_KINDS};
use xgworks_core::xgtbl::XgTable;
use xgworks_core::{EditNote, EditTrack, Event};

use crate::{clip, icons};

/// Row height at the original's font (MS Gothic, 18 px); other sizes scale it.
const ROW_H: f32 = 17.0;
const HEAD_H: f32 = 19.0;
const BAR_H: f32 = 30.0;
const STATUS_H: f32 = 20.0;
const LOC_BG: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
const CUR_BG: Color32 = Color32::from_rgb(0x00, 0x00, 0xFF);
const CELL_BG: Color32 = Color32::from_rgb(0xFF, 0x00, 0x00);
const SEL_BG: Color32 = Color32::from_rgb(0x00, 0xFF, 0x00);
const GREY_TXT: Color32 = Color32::from_rgb(0xA8, 0xA8, 0xA8);
const FACE: Color32 = Color32::from_rgb(0xC0, 0xC0, 0xC0);
/// Column titles; default widths from `ws.ini [List Hdr]` of the original.
const COLS: [&str; 7] = ["L1", "L2", "L3", "Type", "Value1", "Value2", "Value3"];
const DEF_W: [f32; 7] = [63.0, 30.0, 46.0, 239.0, 252.0, 336.0, 170.0];

/// A block shown in the list: track and block index (None = the whole track).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub track: usize,
    pub block: Option<usize>,
}

pub struct ListCtx<'a> {
    pub names: &'a ListNames,
    /// List font (name, size px), shared by all list windows; the app applies
    /// a change (`font_changed`).
    pub font: &'a RefCell<(String, f32)>,
    pub font_changed: &'a std::cell::Cell<bool>,
    /// Fonts that can be chosen (name, file).
    pub fonts: &'a [(String, String)],
    /// "Display XG" settings, shared by all list windows.
    pub xg: &'a RefCell<listev::XgDisp>,
    pub tbl: Option<&'a XgTable>,
    pub ppqn: u32,
    /// Shared event clipboard of all list windows: (tick relative to the first, event).
    pub clipboard: &'a RefCell<Vec<(u32, Event)>>,
    /// Screen rectangles of the list windows' event areas this frame (for
    /// dragging events from one list window onto another).
    pub areas: &'a RefCell<Vec<(u64, Rect)>>,
    /// Events dropped onto list window `.0` (it then asks where to insert them).
    pub dropped: &'a RefCell<Option<(u64, Vec<(u32, Event)>)>>,
}

/// What the window asks the app to do.
pub enum ListOut {
    /// MIDI to send through the output of `track`.
    Send(usize, Vec<u8>),
}

struct InsertDlg {
    kind: NewKind,
    loc: [u32; 3],
    vals: Vec<i32>,
    text: String,
    auto: bool,
    step: u32,
}

type Snap = Vec<(usize, Vec<EditNote>, Vec<Event>)>;

pub struct ListState {
    pub spans: Vec<Span>,
    pub target: usize,
    cursor: usize,
    col: usize,
    sel: Vec<usize>,
    anchor: Option<usize>,
    typing: Option<String>,
    first: usize,
    widths: [f32; 7],
    time_mode: bool,
    hidden: [bool; 9],
    mono: bool,
    poly: bool,
    insert: Option<InsertDlg>,
    paste_at: Option<[u32; 3]>,
    /// Events the Paste dialog inserts instead of the clipboard (a drop).
    paste_src: Option<Vec<(u32, Event)>>,
    /// The window's id among the list windows (drag & drop between them).
    pub id: u64,
    /// The selection is being dragged (List.hlp "複数のイベントの移動").
    move_drag: bool,
    /// Move dialog: destination of the selected events.
    move_at: Option<[u32; 3]>,
    filter_dlg: Option<[bool; 9]>,
    /// Font dialog being edited: (font name, size px).
    font_dlg: Option<(String, f32)>,
    /// "XG Settings" dialog being edited.
    xg_dlg: Option<listev::XgDisp>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    offs: Vec<(Instant, usize, [u8; 3])>,
    drag: Option<usize>,
    resize: Option<(usize, f32, f32)>,
    /// After an edit: re-find the cursor / selection by event identity.
    want: Option<(Option<(usize, Event)>, Vec<(usize, Event)>)>,
    status: String,
}

fn span_range(t: &EditTrack, s: Span) -> (u32, u32) {
    match s.block {
        Some(bi) if bi < t.blocks.len() => {
            let lo = if bi == 0 { 0 } else { t.blocks[bi].start };
            (lo, t.blocks.get(bi + 1).map_or(u32::MAX, |b| b.start))
        }
        _ => (0, u32::MAX),
    }
}

fn span_name(tracks: &[EditTrack], s: Span) -> String {
    let Some(t) = tracks.get(s.track) else { return String::new() };
    match s.block.and_then(|b| t.blocks.get(b)) {
        Some(b) if !b.name.is_empty() => b.name.clone(),
        _ => t.name.clone(),
    }
}

impl ListState {
    pub fn new(spans: Vec<Span>) -> Self {
        ListState {
            spans,
            target: 0,
            cursor: 0,
            col: 0,
            sel: Vec::new(),
            anchor: None,
            typing: None,
            first: 0,
            widths: DEF_W,
            time_mode: false,
            hidden: [false; 9],
            mono: true,
            poly: false,
            insert: None,
            paste_at: None,
            paste_src: None,
            id: 0,
            move_drag: false,
            move_at: None,
            filter_dlg: None,
            xg_dlg: None,
            font_dlg: None,
            undo: Vec::new(),
            redo: Vec::new(),
            offs: Vec::new(),
            drag: None,
            resize: None,
            want: None,
            status: String::new(),
        }
    }

    pub fn title(&self, tracks: &[EditTrack]) -> String {
        self.spans.get(self.target).map(|&s| span_name(tracks, s)).unwrap_or_default()
    }

    fn target_span(&self) -> Option<Span> {
        self.spans.get(self.target).copied()
    }

    fn in_target(&self, tracks: &[EditTrack], r: &Row) -> bool {
        let Some(s) = self.target_span() else { return false };
        let Some(t) = tracks.get(s.track) else { return false };
        let (lo, hi) = span_range(t, s);
        r.track == s.track && r.tick >= lo && r.tick < hi
    }

    fn rows(&self, tracks: &[EditTrack]) -> Vec<Row> {
        let spans: Vec<(usize, u32, u32)> = self
            .spans
            .iter()
            .filter_map(|&s| tracks.get(s.track).map(|t| {
                let (lo, hi) = span_range(t, s);
                (s.track, lo, hi)
            }))
            .collect();
        let rows = listev::collect_rows(tracks, &spans);
        rows.into_iter()
            .filter(|r| listev::row_event(tracks, r).map_or(false, |e| !self.hidden[listev::filter_kind(&e)]))
            .collect()
    }

    fn snapshot(&mut self, tracks: &[EditTrack]) {
        let mut ts: Vec<usize> = self.spans.iter().map(|s| s.track).collect();
        ts.sort_unstable();
        ts.dedup();
        self.undo.push(ts.into_iter().filter_map(|ti| tracks.get(ti).map(|t| (ti, t.notes.clone(), t.others.clone()))).collect());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(tracks: &mut [EditTrack], snap: Snap) -> Snap {
        snap.into_iter()
            .filter_map(|(ti, n, o)| {
                let t = tracks.get_mut(ti)?;
                Some((ti, std::mem::replace(&mut t.notes, n), std::mem::replace(&mut t.others, o)))
            })
            .collect()
    }

    fn ident(tracks: &[EditTrack], rows: &[Row], i: usize) -> Option<(usize, Event)> {
        let r = rows.get(i)?;
        Some((r.track, listev::row_event(tracks, r)?))
    }

    fn remember(&mut self, tracks: &[EditTrack], rows: &[Row]) {
        let cur = Self::ident(tracks, rows, self.cursor);
        let sel = self.sel.iter().filter_map(|&i| Self::ident(tracks, rows, i)).collect();
        self.want = Some((cur, sel));
    }

    fn locate(tracks: &[EditTrack], rows: &[Row], id: &(usize, Event)) -> Option<usize> {
        rows.iter().position(|r| r.track == id.0 && listev::row_event(tracks, r).as_ref() == Some(&id.1))
    }

    /// Draw the window; returns true when the song changed.
    pub fn ui(&mut self, ui: &mut egui::Ui, tracks: &mut [EditTrack], cx: &ListCtx, out: &mut Vec<ListOut>) -> bool {
        let mut edited = false;
        self.spans.retain(|s| s.track < tracks.len());
        if self.spans.is_empty() {
            ui.label("No block.");
            return false;
        }
        self.target = self.target.min(self.spans.len() - 1);
        let tm = TimeMap::new(tracks, cx.ppqn as u16);
        let mut rows = self.rows(tracks);
        if let Some((cur, sel)) = self.want.take() {
            if let Some(i) = cur.as_ref().and_then(|c| Self::locate(tracks, &rows, c)) {
                self.cursor = i;
            }
            self.sel = sel.iter().filter_map(|s| Self::locate(tracks, &rows, s)).collect();
        }
        self.cursor = self.cursor.min(rows.len().saturating_sub(1));
        let fmt = Fmt { names: cx.names, xg: *cx.xg.borrow(), tbl: cx.tbl, ppqn: cx.ppqn, fx_hist: listev::song_fx_types(tracks) };
        let texts = listev::format_rows(tracks, &rows, &fmt);

        // note-offs of the monitor
        let now = Instant::now();
        self.offs.retain(|(at, ti, b)| {
            if *at <= now {
                out.push(ListOut::Send(*ti, b.to_vec()));
                false
            } else {
                true
            }
        });
        if !self.offs.is_empty() {
            ui.ctx().request_repaint_after(Duration::from_millis(20));
        }

        let full = ui.max_rect();
        ui.allocate_rect(full, Sense::hover());
        let font = list_font(ui.ctx());
        let row_h = list_row_h(ui.ctx(), &font);
        let p = ui.painter().clone();
        p.rect_filled(full, 0.0, FACE);

        // ---- toolbar: block name, font, mono / poly / wave / MCI monitor ----
        let bar = Rect::from_min_size(full.min, vec2(full.width(), BAR_H));
        let mut bar_ui = ui.child_ui(bar.shrink2(vec2(4.0, 3.0)), egui::Layout::left_to_right(egui::Align::Center), None);
        let names: Vec<String> = self.spans.iter().map(|&s| span_name(tracks, s)).collect();
        let mut tgt = self.target;
        egui::ComboBox::from_id_source("list_block").width(190.0).selected_text(names[tgt].clone()).show_ui(&mut bar_ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(&mut tgt, i, n);
            }
        });
        if tgt != self.target {
            self.target = tgt;
            self.sel.clear();
        }
        bar_ui.add_space(10.0);
        if icons::tool_button(&mut bar_ui, icons::LIST_BAR, 0, self.font_dlg.is_some(), "A", "Font").clicked() {
            // the font in use: the chosen one, else MS Gothic / the built-in one
            let (mut name, _) = cx.font.borrow().clone();
            if name.is_empty() {
                let gothic = ui.ctx().fonts(|f| f.families().contains(&FontFamily::Name("list".into())));
                name = if gothic { "MS Gothic".into() } else { BUILTIN_FONT.into() };
            }
            self.font_dlg = Some((name, font.size));
        }
        bar_ui.add_space(8.0);
        if icons::tool_button(&mut bar_ui, icons::LIST_BAR, 1, self.mono, "Mono", "Mono monitor").clicked() {
            self.mono = !self.mono;
            self.poly &= !self.mono;
        }
        if icons::tool_button(&mut bar_ui, icons::LIST_BAR, 2, self.poly, "Poly", "Poly monitor").clicked() {
            self.poly = !self.poly;
            self.mono &= !self.poly;
        }
        bar_ui.add_space(8.0);
        bar_ui.add_enabled_ui(false, |ui| {
            icons::tool_button(ui, icons::LIST_BAR, 3, false, "Wave", "Wave monitor (no wave blocks)");
            icons::tool_button(ui, icons::LIST_BAR, 4, false, "MCI", "MCI monitor");
        });

        // ---- header ----
        let head = Rect::from_min_size(pos2(full.left(), bar.bottom()), vec2(full.width() - 16.0, HEAD_H));
        let mut xs = [0f32; 8];
        xs[0] = head.left();
        for k in 0..7 {
            xs[k + 1] = xs[k] + self.widths[k];
        }
        let head_resp = ui.interact(head, ui.id().with("list_head"), Sense::click_and_drag());
        for k in 0..7 {
            let r = Rect::from_min_max(pos2(xs[k], head.top()), pos2(xs[k + 1], head.bottom()));
            bevel(&p, r, false);
            p.text(pos2(r.left() + 4.0, r.center().y), egui::Align2::LEFT_CENTER, COLS[k], font.clone(), Color32::BLACK);
        }
        bevel(&p, Rect::from_min_max(pos2(xs[7], head.top()), head.max), false);
        // column resize at the header borders
        if let Some(pos) = head_resp.hover_pos().or(head_resp.interact_pointer_pos()) {
            if let Some(k) = (0..7).find(|&k| (pos.x - xs[k + 1]).abs() <= 3.0) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                if head_resp.drag_started() {
                    self.resize = Some((k, pos.x, self.widths[k]));
                }
                if head_resp.double_clicked() {
                    let w = texts.iter().zip(&rows).map(|(t, r)| cell_text(&tm, r.tick, t, k, self.time_mode).chars().count()).max().unwrap_or(4).max(COLS[k].len());
                    self.widths[k] = w as f32 * 8.6 + 10.0;
                }
            }
        }
        if let (Some((k, x0, w0)), Some(pos)) = (self.resize, head_resp.interact_pointer_pos()) {
            self.widths[k] = (w0 + pos.x - x0).max(16.0);
        }
        if !head_resp.dragged() {
            self.resize = None;
        }
        let mut head_col = None;
        if head_resp.secondary_clicked() {
            if let Some(pos) = head_resp.interact_pointer_pos() {
                head_col = (0..7).find(|&k| pos.x >= xs[k] && pos.x < xs[k + 1]);
            }
        }
        if head_col.is_some() {
            ui.memory_mut(|m| m.data.insert_temp(ui.id().with("head_col"), head_col.unwrap()));
        }
        let menu_col: usize = ui.memory(|m| m.data.get_temp(ui.id().with("head_col"))).unwrap_or(0);
        head_resp.context_menu(|ui| {
            if menu_col < 3 {
                if ui.radio(!self.time_mode, "Measure").clicked() {
                    self.time_mode = false;
                    ui.close_menu();
                }
                if ui.radio(self.time_mode, "Time").clicked() {
                    self.time_mode = true;
                    ui.close_menu();
                }
            } else if menu_col == 3 {
                let x = *cx.xg.borrow();
                let xg_on = x.sysex || x.note || x.cc || x.prog;
                if ui.add(egui::SelectableLabel::new(xg_on, "Display XG...")).clicked() {
                    self.xg_dlg = Some(x);
                    ui.close_menu();
                }
                let on = self.hidden.iter().any(|h| *h);
                if ui.add(egui::SelectableLabel::new(on, "Display Filter...")).clicked() {
                    self.filter_dlg = Some(self.hidden);
                    ui.close_menu();
                }
            } else {
                ui.close_menu();
            }
        });

        // ---- rows ----
        let status_r = Rect::from_min_max(pos2(full.left(), full.bottom() - STATUS_H), full.max);
        let body = Rect::from_min_max(pos2(full.left(), head.bottom()), pos2(full.right() - 16.0, status_r.top()));
        {
            let off = ui.ctx().input(|i| i.viewport().inner_rect.map_or(egui::Vec2::ZERO, |r| r.min.to_vec2()));
            cx.areas.borrow_mut().push((self.id, body.translate(off)));
            let mine = cx.dropped.borrow().as_ref().is_some_and(|d| d.0 == self.id);
            if mine {
                if let Some((_, evs)) = cx.dropped.borrow_mut().take() {
                    let tick = rows.get(self.cursor).map_or(0, |r| r.tick);
                    self.paste_at = Some(loc_values(&tm, tick, false));
                    self.paste_src = Some(evs);
                }
            }
        }
        let sb = Rect::from_min_max(pos2(body.right(), body.top() - HEAD_H), pos2(full.right(), body.bottom()));
        let fit = ((body.height() / row_h).floor() as usize).max(1);
        let resp = ui.interact(body, ui.id().with("list_body"), Sense::click_and_drag());
        let wheel = if resp.hovered() { ui.input(|i| i.raw_scroll_delta.y) } else { 0.0 };
        if wheel != 0.0 {
            let d = (wheel / row_h * 1.0).round() as i64;
            self.first = (self.first as i64 - d.signum() * 3).clamp(0, rows.len().saturating_sub(fit) as i64) as usize;
        }
        self.first = self.first.min(rows.len().saturating_sub(fit.min(rows.len())));
        p.rect_filled(body, 0.0, Color32::WHITE);
        let grey: Vec<bool> = rows.iter().map(|r| !self.in_target(tracks, r)).collect();
        for (k, i) in (self.first..rows.len().min(self.first + fit + 1)).enumerate() {
            let y = body.top() + k as f32 * row_h;
            if y > body.bottom() {
                break;
            }
            let is_cur = i == self.cursor;
            let is_sel = self.sel.contains(&i);
            for c in 0..7 {
                let r = Rect::from_min_max(pos2(xs[c], y), pos2(xs[c + 1], y + row_h)).intersect(body);
                let (bg, fg) = if is_cur && c == self.col {
                    (CELL_BG, Color32::WHITE)
                } else if c < 3 {
                    (LOC_BG, if grey[i] { Color32::from_gray(0xD0) } else { Color32::WHITE })
                } else if is_cur {
                    (CUR_BG, Color32::WHITE)
                } else if is_sel {
                    (SEL_BG, Color32::BLACK)
                } else {
                    (Color32::WHITE, if grey[i] { GREY_TXT } else { Color32::BLACK })
                };
                p.rect_filled(r, 0.0, bg);
                let txt = if is_cur && c == self.col && self.typing.is_some() {
                    format!("{}_", self.typing.as_deref().unwrap_or(""))
                } else {
                    cell_text(&tm, rows[i].tick, &texts[i], c, self.time_mode)
                };
                let clip = p.with_clip_rect(r);
                // value-cell separator bar, as the original draws it
                clip.vline(r.left() + 2.5, (y + 2.0)..=(y + row_h - 2.0), Stroke::new(1.0_f32, if c < 3 { Color32::from_gray(0x40) } else { Color32::BLACK }));
                let mut tx = r.left() + 7.0;
                if c == 5 {
                    // Value2 keeps room for the note-length symbol
                    if let Some(Event::Note { gate, .. }) = listev::row_event(tracks, &rows[i]) {
                        if let Some(sym) = listev::gate_symbol(gate, cx.ppqn) {
                            draw_note_symbol(&clip, pos2(tx + 4.0, y + row_h - 4.0), sym, fg);
                        }
                    }
                    tx += 20.0;
                }
                if c < 3 {
                    clip.text(pos2(r.right() - 4.0, y + row_h / 2.0), egui::Align2::RIGHT_CENTER, txt, font.clone(), fg);
                } else {
                    let txt = fit_text(ui, &txt, &font, r.right() - tx - 2.0);
                    clip.text(pos2(tx, y + row_h / 2.0), egui::Align2::LEFT_CENTER, txt, font.clone(), fg);
                }
            }
            // the rest of the row
            let rest = Rect::from_min_max(pos2(xs[7], y), pos2(body.right(), y + row_h)).intersect(body);
            p.rect_filled(rest, 0.0, if is_cur { CUR_BG } else if is_sel { SEL_BG } else { Color32::WHITE });
            p.vline(xs[7] + 2.5, (y + 2.0)..=(y + row_h - 2.0), Stroke::new(1.0_f32, Color32::BLACK));
            p.hline(body.left()..=body.right(), y + row_h - 0.5, Stroke::new(1.0_f32, Color32::BLACK));
        }
        // scrollbar
        p.rect_filled(sb, 0.0, Color32::from_gray(0xE8));
        if rows.len() > fit {
            let track_r = sb.shrink2(vec2(1.0, 16.0));
            let h = (track_r.height() * fit as f32 / rows.len() as f32).max(12.0);
            let y = track_r.top() + (track_r.height() - h) * self.first as f32 / (rows.len() - fit) as f32;
            let thumb = Rect::from_min_size(pos2(track_r.left(), y), vec2(track_r.width(), h));
            bevel(&p, thumb, false);
            let sresp = ui.interact(sb, ui.id().with("list_sb"), Sense::click_and_drag());
            if let Some(pos) = sresp.interact_pointer_pos() {
                if sresp.dragged() || sresp.clicked() {
                    let f = ((pos.y - track_r.top() - h / 2.0) / (track_r.height() - h)).clamp(0.0, 1.0);
                    self.first = (f * (rows.len() - fit) as f32).round() as usize;
                }
            }
        }

        // ---- mouse ----
        let nrows = rows.len();
        let row_at = |first: usize, pos: egui::Pos2| -> Option<usize> {
            let k = ((pos.y - body.top()) / row_h).floor();
            (k >= 0.0).then(|| first + k as usize).filter(|&i| i < nrows)
        };
        let col_at = |x: f32| (0..7).find(|&c| x >= xs[c] && x < xs[c + 1]).unwrap_or(6);
        let mods = ui.input(|i| i.modifiers);
        let mut sound: Vec<usize> = Vec::new();
        // dragging the green selection moves it (or copies it onto another list window)
        if resp.drag_started() && !mods.shift && !mods.command && !self.sel.is_empty() {
            let press = ui.input(|i| i.pointer.press_origin());
            if let Some(i) = press.and_then(|p| row_at(self.first, p)) {
                if self.sel.contains(&i) {
                    self.move_drag = true;
                }
            }
        }
        if self.move_drag {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            if resp.drag_stopped() {
                self.move_drag = false;
                let ids = self.chosen(tracks, &rows);
                let evs: Vec<Event> = ids.iter().filter_map(|&i| listev::row_event(tracks, &rows[i])).collect();
                let t0 = evs.iter().map(|e| e.t()).min().unwrap_or(0);
                let payload: Vec<(u32, Event)> = evs.iter().map(|e| (e.t() - t0, e.clone())).collect();
                let pos = ui.input(|i| i.pointer.latest_pos());
                let off = ui.ctx().input(|i| i.viewport().inner_rect.map_or(egui::Vec2::ZERO, |r| r.min.to_vec2()));
                match pos {
                    Some(p) if body.contains(p) => {
                        if !payload.is_empty() {
                            self.move_at = Some(loc_values(&tm, t0, false));
                        }
                    }
                    Some(p) => {
                        // released over another list window: copy the events there
                        let at = p + off;
                        let target = cx.areas.borrow().iter().find(|(id, r)| *id != self.id && r.contains(at)).map(|x| x.0);
                        if let (Some(t), false) = (target, payload.is_empty()) {
                            *cx.dropped.borrow_mut() = Some((t, payload));
                        }
                    }
                    None => {}
                }
            }
        }
        if !self.move_drag && (resp.drag_started() || (resp.clicked() && self.drag.is_none())) {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.commit_typing(tracks, &rows, &tm, cx, &mut edited);
                let mut continue_cursor = true;
                match row_at(self.first, pos) {
                    Some(i) => {
                        if !mods.command {
                            self.col = col_at(pos.x);
                        }
                        if mods.shift {
                            let a = self.anchor.unwrap_or(self.cursor);
                            self.sel = (a.min(i)..=a.max(i)).collect();
                        } else if mods.command {
                            // Ctrl+click toggles a row; the cursor line stays where it is
                            if let Some(k) = self.sel.iter().position(|&s| s == i) {
                                self.sel.remove(k);
                            } else {
                                self.sel.push(i);
                            }
                            self.anchor = Some(i);
                            sound.push(i);
                            continue_cursor = false;
                        } else {
                            self.sel.clear();
                            self.anchor = Some(i);
                            self.drag = Some(i);
                        }
                        if continue_cursor {
                            self.cursor = i;
                            sound.push(i);
                        }
                    }
                    None => {
                        // the empty area below the last event clears the selection
                        self.sel.clear();
                    }
                }
            }
        }
        if resp.dragged() {
            if let (Some(a), Some(pos)) = (self.drag, resp.interact_pointer_pos()) {
                // auto-scroll at the edges
                if pos.y > body.bottom() - 4.0 && self.first + fit < rows.len() {
                    self.first += 1;
                } else if pos.y < body.top() + 4.0 && self.first > 0 {
                    self.first -= 1;
                }
                if let Some(i) = row_at(self.first, pos.clamp(body.min, body.max - vec2(0.0, 1.0))) {
                    if i != self.cursor {
                        self.sel = (a.min(i)..=a.max(i)).collect();
                        self.cursor = i;
                        if self.poly {
                            sound.push(i);
                        }
                    }
                }
            }
        }
        if !resp.dragged() && !resp.drag_started() {
            self.drag = None;
        }
        if resp.secondary_clicked() {
            if let Some(i) = resp.interact_pointer_pos().and_then(|p| row_at(self.first, p)) {
                if !self.sel.contains(&i) {
                    self.cursor = i;
                }
                if let Some(pos) = resp.interact_pointer_pos() {
                    self.col = col_at(pos.x);
                }
            }
        }
        if resp.clicked() || resp.drag_started() {
            // take the keyboard back from any text field / combo
            ui.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }

        // ---- edit menu (right click) ----
        let mut cmd: Option<Cmd> = None;
        resp.context_menu(|ui| {
            let item = |ui: &mut egui::Ui, label: &str, sc: &str, c: Cmd| -> Option<Cmd> {
                let r = ui.add(egui::Button::new(label).shortcut_text(sc)).clicked().then_some(c);
                if r.is_some() {
                    ui.close_menu();
                }
                r
            };
            for (l, sc, c) in [("Cut", "Ctrl+X", Cmd::Cut), ("Copy", "Ctrl+C", Cmd::Copy), ("Paste", "Ctrl+V", Cmd::Paste), ("Delete", "Del", Cmd::Delete), ("Select All", "Ctrl+A", Cmd::All)] {
                cmd = item(ui, l, sc, c).or(cmd);
            }
            ui.separator();
            ui.menu_button("Insert Event", |ui| {
                for k in NewKind::ALL {
                    if ui.button(k.label()).clicked() {
                        cmd = Some(Cmd::Insert(k));
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            for (l, c) in [("Select Same Event", Cmd::Same), ("Select Similar Event", Cmd::Similar), ("Invert Selection", Cmd::Invert)] {
                cmd = item(ui, l, "", c).or(cmd);
            }
            ui.separator();
            cmd = item(ui, "Make Edit Target", "", Cmd::Target).or(cmd);
        });

        // ---- keys ----
        let focused = ui.ctx().input(|i| i.focused) && !ui.ctx().wants_keyboard_input() && self.insert.is_none() && self.paste_at.is_none() && self.filter_dlg.is_none() && self.xg_dlg.is_none() && self.move_at.is_none() && self.font_dlg.is_none();
        if focused {
            let ek = clip::edit_keys(ui.ctx());
            if ek.copy {
                cmd = Some(Cmd::Copy);
            } else if ek.cut {
                cmd = Some(Cmd::Cut);
            } else if ek.paste {
                cmd = Some(Cmd::Paste);
            } else if ek.all {
                cmd = Some(Cmd::All);
            } else if ek.undo {
                cmd = Some(Cmd::Undo);
            } else if ek.redo {
                cmd = Some(Cmd::Redo);
            } else if ek.del && self.typing.is_none() {
                cmd = Some(Cmd::Delete);
            }
            let (keys, text): (Vec<(egui::Key, egui::Modifiers)>, String) = ui.input(|i| {
                let mut k = Vec::new();
                let mut s = String::new();
                for e in &i.events {
                    match e {
                        egui::Event::Key { key, pressed: true, modifiers, .. } => k.push((*key, *modifiers)),
                        egui::Event::Text(t) => s.push_str(t),
                        _ => {}
                    }
                }
                (k, s)
            });
            if !text.is_empty() && !mods.command {
                let t = self.typing.get_or_insert_with(String::new);
                t.push_str(&text);
            }
            for (key, m) in keys {
                use egui::Key::*;
                let n = rows.len();
                match key {
                    ArrowUp | ArrowDown if n > 0 => {
                        self.commit_typing(tracks, &rows, &tm, cx, &mut edited);
                        let i = if key == ArrowUp { self.cursor.saturating_sub(1) } else { (self.cursor + 1).min(n - 1) };
                        if m.shift {
                            let a = self.anchor.get_or_insert(self.cursor);
                            let a = *a;
                            self.sel = (a.min(i)..=a.max(i)).collect();
                        } else {
                            self.anchor = Some(i);
                        }
                        if i != self.cursor {
                            self.cursor = i;
                            sound.push(i);
                        }
                    }
                    ArrowLeft | ArrowRight | Tab => {
                        self.commit_typing(tracks, &rows, &tm, cx, &mut edited);
                        let left = key == ArrowLeft || (key == Tab && m.shift);
                        self.col = if left { self.col.saturating_sub(1) } else { (self.col + 1).min(6) };
                    }
                    Enter => self.commit_typing(tracks, &rows, &tm, cx, &mut edited),
                    Escape => {
                        if self.typing.take().is_none() {
                            self.sel.clear();
                        }
                    }
                    Backspace => {
                        if let Some(t) = self.typing.as_mut() {
                            t.pop();
                        }
                    }
                    PageUp | PageDown | Home | End if self.typing.is_none() => {
                        let d = match key {
                            PageUp => 1,
                            PageDown => -1,
                            Home => 10,
                            _ => -10,
                        };
                        self.bump(tracks, &rows, &tm, cx, d, &mut edited);
                    }
                    Insert => cmd = Some(if m.shift { Cmd::DupSame } else if m.command { Cmd::Rest } else { Cmd::Dup }),
                    F if m.command => cmd = Some(Cmd::Measure(1)),
                    B if m.command => cmd = Some(Cmd::Measure(-1)),
                    _ => {}
                }
            }
            // keep the cursor line visible
            if self.cursor < self.first {
                self.first = self.cursor;
            } else if self.cursor >= self.first + fit {
                self.first = self.cursor + 1 - fit;
            }
        }
        if let Some(c) = cmd {
            self.command(c, tracks, &mut rows, &tm, cx, &mut edited, ui.ctx());
        }

        // ---- monitor ----
        if self.mono || self.poly {
            for i in sound {
                if let Some(Event::Note { t, ch, note, vel, gate }) = rows.get(i).and_then(|r| listev::row_event(tracks, r)) {
                    let ti = rows[i].track;
                    if self.mono {
                        for (_, tj, b) in self.offs.drain(..) {
                            out.push(ListOut::Send(tj, b.to_vec()));
                        }
                    }
                    out.push(ListOut::Send(ti, vec![0x90 | ch, note, vel]));
                    let ms = (tm.to_ms(t + gate) - tm.to_ms(t)).clamp(80.0, 2000.0);
                    self.offs.push((Instant::now() + Duration::from_millis(ms as u64), ti, [0x80 | ch, note, 0]));
                    ui.ctx().request_repaint_after(Duration::from_millis(20));
                }
            }
        }

        // ---- status bar (selected event / block start) ----
        p.rect_filled(status_r, 0.0, FACE);
        p.hline(status_r.x_range(), status_r.top() + 0.5, Stroke::new(1.0_f32, Color32::WHITE));
        let left = if !self.status.is_empty() {
            self.status.clone()
        } else if self.sel.len() > 1 {
            "Selecting Event".into()
        } else {
            texts.get(self.cursor).map(|t| format!("{}  {}  {}  {}", t.ty, t.v[0], t.v[1], t.v[2])).unwrap_or_default()
        };
        p.text(pos2(status_r.left() + 6.0, status_r.center().y), egui::Align2::LEFT_CENTER, left, FontId::proportional(12.0), Color32::BLACK);
        if let Some(s) = self.target_span() {
            let start = tracks.get(s.track).map(|t| span_range(t, s).0).unwrap_or(0);
            let (m, b, c) = tm.to_mbc(start);
            p.text(pos2(status_r.right() - 8.0, status_r.center().y), egui::Align2::RIGHT_CENTER, format!("Measure   {m:04}:{b:02}:{c:03}"), FontId::proportional(12.0), Color32::BLACK);
        }

        // ---- dialogs ----
        self.dialogs(ui.ctx(), tracks, &rows, &tm, cx, &mut edited);
        if edited {
            self.status.clear();
        }
        edited
    }

    /// Commit the typed text into the cursor cell.
    fn commit_typing(&mut self, tracks: &mut [EditTrack], rows: &[Row], tm: &TimeMap, cx: &ListCtx, edited: &mut bool) {
        let Some(text) = self.typing.take() else { return };
        let Some(r) = rows.get(self.cursor).copied() else { return };
        if !self.in_target(tracks, &r) {
            self.status = "Only the block in the toolbar can be edited.".into();
            return;
        }
        let Some(e) = listev::row_event(tracks, &r) else { return };
        let new = if self.col < 3 {
            let Ok(v) = text.trim().parse::<u32>() else { return };
            let mut loc = loc_values(tm, r.tick, self.time_mode);
            loc[self.col] = v;
            Some(listev::with_tick(&e, tm.loc_tick(loc, self.time_mode)))
        } else if self.col > 3 && listev::editable(&e, self.col - 4) {
            listev::set_value(&e, self.col - 4, &text, cx.ppqn)
        } else {
            None
        };
        match new {
            Some(n) if n != e => self.apply(tracks, rows, &r, n, edited),
            Some(_) => {}
            None => self.status = format!("\"{}\" cannot be set here.", text.trim()),
        }
    }

    /// PgUp / PgDn / Home / End on the cursor cell.
    fn bump(&mut self, tracks: &mut [EditTrack], rows: &[Row], tm: &TimeMap, cx: &ListCtx, d: i32, edited: &mut bool) {
        let Some(r) = rows.get(self.cursor).copied() else { return };
        if !self.in_target(tracks, &r) {
            return;
        }
        let Some(e) = listev::row_event(tracks, &r) else { return };
        let new = if self.col < 3 {
            let mut loc = loc_values(tm, r.tick, self.time_mode);
            loc[self.col] = (loc[self.col] as i64 + d as i64).max(if self.col < 2 && !self.time_mode { 1 } else { 0 }) as u32;
            Some(listev::with_tick(&e, tm.loc_tick(loc, self.time_mode)))
        } else if self.col > 3 {
            listev::bump_value(&e, self.col - 4, d, cx.ppqn)
        } else {
            None
        };
        if let Some(n) = new.filter(|n| *n != e) {
            self.apply(tracks, rows, &r, n, edited);
        }
    }

    fn apply(&mut self, tracks: &mut [EditTrack], rows: &[Row], r: &Row, e: Event, edited: &mut bool) {
        self.snapshot(tracks);
        let cur_ident = Some((r.track, e.clone()));
        self.remember(tracks, rows);
        listev::set_row_event(tracks, r, e);
        if let Some(w) = self.want.as_mut() {
            w.0 = cur_ident;
            w.1.clear();
        }
        *edited = true;
    }

    /// Rows a command works on: the selection, else the cursor line (target block only).
    fn chosen(&self, tracks: &[EditTrack], rows: &[Row]) -> Vec<usize> {
        let mut v: Vec<usize> = if self.sel.is_empty() { vec![self.cursor] } else { self.sel.clone() };
        v.retain(|&i| rows.get(i).map_or(false, |r| self.in_target(tracks, r)));
        v.sort_unstable();
        v.dedup();
        v
    }

    #[allow(clippy::too_many_arguments)]
    fn command(&mut self, c: Cmd, tracks: &mut [EditTrack], rows: &mut Vec<Row>, tm: &TimeMap, cx: &ListCtx, edited: &mut bool, ctx: &egui::Context) {
        self.typing = None;
        let tgt = self.target_span();
        let ch = tgt.and_then(|s| tracks.get(s.track)).map_or(0, |t| t.channel & 0x0F);
        match c {
            Cmd::Copy | Cmd::Cut => {
                let ids = self.chosen(tracks, rows);
                let evs: Vec<Event> = ids.iter().filter_map(|&i| listev::row_event(tracks, &rows[i])).collect();
                let t0 = evs.iter().map(|e| e.t()).min().unwrap_or(0);
                *cx.clipboard.borrow_mut() = evs.iter().map(|e| (e.t() - t0, e.clone())).collect();
                clip::announce(ctx, "events");
                self.status = format!("{} event(s) copied.", evs.len());
                if c == Cmd::Cut && !ids.is_empty() {
                    self.snapshot(tracks);
                    let rs: Vec<Row> = ids.iter().map(|&i| rows[i]).collect();
                    listev::remove_rows(tracks, &rs);
                    self.sel.clear();
                    *edited = true;
                }
            }
            Cmd::Paste => {
                if cx.clipboard.borrow().is_empty() {
                    self.status = "The clipboard is empty.".into();
                } else {
                    let tick = rows.get(self.cursor).map_or(0, |r| r.tick);
                    self.paste_at = Some(loc_values(tm, tick, false));
                }
            }
            Cmd::Delete => {
                let ids = self.chosen(tracks, rows);
                if !ids.is_empty() {
                    self.snapshot(tracks);
                    let rs: Vec<Row> = ids.iter().map(|&i| rows[i]).collect();
                    listev::remove_rows(tracks, &rs);
                    self.sel.clear();
                    self.want = Some((None, Vec::new()));
                    *edited = true;
                }
            }
            Cmd::All => self.sel = (0..rows.len()).filter(|&i| self.in_target(tracks, &rows[i])).collect(),
            Cmd::Invert => self.sel = (0..rows.len()).filter(|&i| self.in_target(tracks, &rows[i]) && !self.sel.contains(&i)).collect(),
            Cmd::Same | Cmd::Similar => {
                let Some(base) = rows.get(self.cursor).and_then(|r| listev::row_event(tracks, r)) else { return };
                let key = |e: &Event| -> (usize, i32) {
                    let k = listev::filter_kind(e);
                    let v = match e {
                        Event::Note { note, .. } => *note as i32,
                        Event::Chan { data, .. } if data[0] & 0xF0 == 0xB0 || data[0] & 0xF0 == 0xA0 => data[1] as i32,
                        Event::Meta { mtype, .. } => *mtype as i32,
                        _ => 0,
                    };
                    (k, v)
                };
                let (bk, bv) = key(&base);
                self.sel = (0..rows.len())
                    .filter(|&i| self.in_target(tracks, &rows[i]))
                    .filter(|&i| {
                        let Some(e) = listev::row_event(tracks, &rows[i]) else { return false };
                        let (k, v) = key(&e);
                        k == bk && (v == bv || (c == Cmd::Similar && k == 0 && v % 12 == bv % 12))
                    })
                    .collect();
            }
            Cmd::Target => {
                if let Some(r) = rows.get(self.cursor) {
                    if let Some(k) = self.spans.iter().position(|&s| {
                        let (lo, hi) = span_range(&tracks[s.track], s);
                        s.track == r.track && r.tick >= lo && r.tick < hi
                    }) {
                        self.target = k;
                        self.sel.clear();
                    }
                }
            }
            Cmd::Undo | Cmd::Redo => {
                let (from, to) = if c == Cmd::Undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
                if let Some(s) = from.pop() {
                    to.push(Self::restore(tracks, s));
                    self.sel.clear();
                    *edited = true;
                }
            }
            Cmd::Dup | Cmd::DupSame => {
                let Some(r) = rows.get(self.cursor).copied() else { return };
                if !self.in_target(tracks, &r) {
                    return;
                }
                let Some(e) = listev::row_event(tracks, &r) else { return };
                let at = match (&e, c) {
                    (Event::Note { gate, .. }, Cmd::Dup) => r.tick + gate,
                    _ => r.tick,
                };
                self.snapshot(tracks);
                let n = listev::with_tick(&e, at);
                listev::push_event(&mut tracks[r.track], n.clone());
                self.want = Some((Some((r.track, n)), Vec::new()));
                *edited = true;
            }
            Cmd::Rest => {
                // Ctrl+Insert: a rest the length of the note — the cursor moves on
                let Some(Event::Note { t, gate, .. }) = rows.get(self.cursor).and_then(|r| listev::row_event(tracks, r)) else { return };
                if let Some(i) = rows.iter().position(|r| r.tick >= t + gate) {
                    self.cursor = i;
                }
            }
            Cmd::Measure(d) => {
                let tick = rows.get(self.cursor).map_or(0, |r| r.tick);
                let (m, _, _) = tm.to_mbc(tick);
                let m2 = (m as i32 + d).max(1) as u32;
                let t2 = tm.from_mbc(m2, 1, 0);
                let i = if d > 0 { rows.iter().position(|r| r.tick >= t2) } else { rows.iter().position(|r| r.tick >= t2) };
                if let Some(i) = i {
                    self.cursor = i;
                }
            }
            Cmd::Insert(k) => {
                let tick = rows.get(self.cursor).map_or(0, |r| r.tick);
                self.insert = Some(InsertDlg {
                    kind: k,
                    loc: loc_values(tm, tick, false),
                    vals: k.fields().iter().map(|f| f.3).collect(),
                    text: String::new(),
                    auto: true,
                    step: 0,
                });
            }
        }
        let _ = ch;
    }

    fn dialogs(&mut self, ctx: &egui::Context, tracks: &mut [EditTrack], rows: &[Row], tm: &TimeMap, cx: &ListCtx, edited: &mut bool) {
        let tgt = self.target_span();
        // Paste: location dialog
        if let Some(mut loc) = self.paste_at {
            let mut open = true;
            let mut done = None;
            egui::Window::new("Paste").collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center()).open(&mut open).show(ctx, |ui| {
                loc_fields(ui, &mut loc);
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
            self.paste_at = Some(loc);
            if !open || done == Some(false) {
                self.paste_at = None;
                self.paste_src = None;
            } else if done == Some(true) {
                self.paste_at = None;
                let src = self.paste_src.take().unwrap_or_else(|| cx.clipboard.borrow().clone());
                if let Some(s) = tgt {
                    let at = tm.from_mbc(loc[0], loc[1], loc[2]);
                    self.snapshot(tracks);
                    let ch = tracks[s.track].channel & 0x0F;
                    let mut ids = Vec::new();
                    for (rel, e) in src.iter() {
                        let mut n = listev::with_tick(e, at + rel);
                        if let Event::Chan { data, .. } = &mut n {
                            data[0] = data[0] & 0xF0 | ch;
                        }
                        listev::push_event(&mut tracks[s.track], n.clone());
                        ids.push((s.track, n));
                    }
                    self.want = Some((ids.first().cloned(), ids));
                    *edited = true;
                }
            }
        }
        // Move: the selected events go to the location given ("Insert" dialog)
        if let Some(mut loc) = self.move_at {
            let mut open = true;
            let mut done = None;
            egui::Window::new("Insert").collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center()).open(&mut open).show(ctx, |ui| {
                loc_fields(ui, &mut loc);
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
            self.move_at = Some(loc);
            if !open || done == Some(false) {
                self.move_at = None;
            } else if done == Some(true) {
                self.move_at = None;
                let ids = self.chosen(tracks, rows);
                let rs: Vec<Row> = ids.iter().map(|&i| rows[i]).collect();
                let evs: Vec<(usize, Event)> = rs.iter().filter_map(|r| listev::row_event(tracks, r).map(|e| (r.track, e))).collect();
                if let Some(t0) = evs.iter().map(|e| e.1.t()).min() {
                    let at = tm.from_mbc(loc[0], loc[1], loc[2]);
                    self.snapshot(tracks);
                    listev::remove_rows(tracks, &rs);
                    let mut moved = Vec::new();
                    for (ti, e) in evs {
                        let n = listev::with_tick(&e, at + (e.t() - t0));
                        listev::push_event(&mut tracks[ti], n.clone());
                        moved.push((ti, n));
                    }
                    self.sel.clear();
                    self.want = Some((moved.first().cloned(), moved));
                    *edited = true;
                }
            }
        }
        // Insert Event dialog ("Enter" adds and stays open; Auto Step moves on)
        if let Some(mut d) = self.insert.take() {
            let mut open = true;
            let mut enter = false;
            let mut exit = false;
            egui::Window::new(d.kind.label()).collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center()).open(&mut open).show(ctx, |ui| {
                loc_fields(ui, &mut d.loc);
                egui::Grid::new("ins_vals").num_columns(2).show(ui, |ui| {
                    for (k, &(label, lo, hi, _)) in d.kind.fields().iter().enumerate() {
                        ui.label(label);
                        ui.add(egui::DragValue::new(&mut d.vals[k]).range(lo..=hi));
                        if d.kind == NewKind::Note && k == 0 {
                            ui.label(listev::note_name(d.vals[0].clamp(0, 127) as u8));
                        }
                        ui.end_row();
                    }
                });
                if d.kind.has_text() {
                    ui.label(if d.kind == NewKind::SysEx { "Hex (F0 … F7)" } else { "Text" });
                    ui.text_edit_singleline(&mut d.text);
                }
                if d.kind == NewKind::Note {
                    ui.checkbox(&mut d.auto, "Auto Step");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Auto Step");
                        ui.add(egui::DragValue::new(&mut d.step).range(0..=7680).suffix(" clk"));
                    });
                }
                ui.horizontal(|ui| {
                    enter = ui.button("Enter").clicked();
                    exit = ui.button("Exit").clicked();
                });
            });
            if enter {
                if let Some(s) = tgt {
                    let tick = tm.from_mbc(d.loc[0], d.loc[1], d.loc[2]);
                    let ch = tracks[s.track].channel & 0x0F;
                    let evs = d.kind.build(tick, ch, &d.vals, &d.text);
                    if evs.is_empty() {
                        self.status = "Enter the bytes as hex, e.g. 43 10 4C 00 00 7E 00 F7.".into();
                    } else {
                        self.snapshot(tracks);
                        let mut last = None;
                        for e in evs {
                            listev::push_event(&mut tracks[s.track], e.clone());
                            last = Some((s.track, e));
                        }
                        self.want = Some((last, Vec::new()));
                        *edited = true;
                        let step = if d.kind == NewKind::Note { if d.auto { d.vals[2].max(1) as u32 } else { 0 } } else { d.step };
                        let (m, b, c) = tm.to_mbc(tick + step);
                        d.loc = [m, b, c];
                    }
                }
            }
            if open && !exit {
                self.insert = Some(d);
            }
        }
        // Display filter: checked kinds are hidden
        if let Some(mut f) = self.filter_dlg {
            let mut open = true;
            let mut done = None;
            egui::Window::new("Filter").collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center()).open(&mut open).show(ctx, |ui| {
                for (k, name) in FILTER_KINDS.iter().enumerate() {
                    ui.checkbox(&mut f[k], *name);
                }
                ui.horizontal(|ui| {
                    if ui.button("Set All").clicked() {
                        f = [true; 9];
                    }
                    if ui.button("Clear All").clicked() {
                        f = [false; 9];
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
            self.filter_dlg = Some(f);
            if !open || done.is_some() {
                if done == Some(true) {
                    self.remember(tracks, rows);
                    self.hidden = f;
                }
                self.filter_dlg = None;
            }
        }
        // Font (the "A" button): name, size and a sample, like the Windows font dialog
        if let Some((mut name, mut px)) = self.font_dlg.clone() {
            let mut open = true;
            let mut done = None;
            egui::Window::new("Font").collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center()).open(&mut open).show(ctx, |ui| {
                egui::Grid::new("font_grid").num_columns(2).show(ui, |ui| {
                    ui.label("Font name");
                    egui::ComboBox::from_id_source("list_font_name").width(220.0).selected_text(&name).show_ui(ui, |ui| {
                        ui.selectable_value(&mut name, BUILTIN_FONT.to_string(), BUILTIN_FONT);
                        for (n, _) in cx.fonts {
                            ui.selectable_value(&mut name, n.clone(), n);
                        }
                    });
                    ui.end_row();
                    ui.label("Size");
                    ui.add(egui::DragValue::new(&mut px).range(8.0..=40.0).speed(0.2).suffix(" px"));
                    ui.end_row();
                });
                ui.label("Sample");
                let sample = egui::RichText::new("AaBbYyZz  D  4 [ 74]  ff [104]").size(px);
                egui::Frame::canvas(ui.style()).show(ui, |ui| ui.label(sample));
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
            self.font_dlg = Some((name.clone(), px));
            if !open || done.is_some() {
                if done == Some(true) {
                    *cx.font.borrow_mut() = (name, px);
                    cx.font_changed.set(true);
                }
                self.font_dlg = None;
            }
        }
        // Display XG ("XG Settings", dialog 1209): events shown with XG names
        if let Some(mut x) = self.xg_dlg {
            let mut open = true;
            let mut done = None;
            egui::Window::new("XG Settings").collapsible(false).resizable(false).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center()).open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.checkbox(&mut x.sysex, "System Exclusive");
                        ui.checkbox(&mut x.note, "Note");
                        ui.checkbox(&mut x.cc, "Control Change");
                        ui.checkbox(&mut x.prog, "Program Change");
                    });
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        if ui.add_sized([60.0, 20.0], egui::Button::new("OK")).clicked() {
                            done = Some(true);
                        }
                        if ui.add_sized([60.0, 20.0], egui::Button::new("Cancel")).clicked() {
                            done = Some(false);
                        }
                    });
                });
            });
            self.xg_dlg = Some(x);
            if !open || done.is_some() {
                if done == Some(true) {
                    *cx.xg.borrow_mut() = x;
                }
                self.xg_dlg = None;
            }
        }
        let _ = (rows, cx);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Cmd {
    Copy,
    Cut,
    Paste,
    Delete,
    All,
    Invert,
    Same,
    Similar,
    Target,
    Undo,
    Redo,
    Dup,
    DupSame,
    Rest,
    Measure(i32),
    Insert(NewKind),
}

fn loc_values(tm: &TimeMap, tick: u32, time_mode: bool) -> [u32; 3] {
    if time_mode {
        let ms = tm.to_ms(tick).round() as u32;
        [ms / 60_000, ms / 1000 % 60, ms % 1000]
    } else {
        let (m, b, c) = tm.to_mbc(tick);
        [m, b, c]
    }
}

fn loc_fields(ui: &mut egui::Ui, loc: &mut [u32; 3]) {
    ui.horizontal(|ui| {
        ui.label("Meas");
        ui.add(egui::DragValue::new(&mut loc[0]).range(1..=9999));
        ui.label("Beat");
        ui.add(egui::DragValue::new(&mut loc[1]).range(1..=16));
        ui.label("Clock");
        ui.add(egui::DragValue::new(&mut loc[2]).range(0..=1919));
    });
}

fn cell_text(tm: &TimeMap, tick: u32, t: &RowText, c: usize, time_mode: bool) -> String {
    match c {
        0..=2 => tm.loc_text(tick, time_mode)[c].clone(),
        3 => t.ty.clone(),
        _ => t.v[c - 4].clone(),
    }
}

/// Cut a text that does not fit and end it with "..", like the original.
fn fit_text(ui: &egui::Ui, s: &str, font: &FontId, max_w: f32) -> String {
    let w = |t: &str| ui.fonts(|f| f.layout_no_wrap(t.to_string(), font.clone(), Color32::BLACK).size().x);
    if w(s) <= max_w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    for n in (0..chars.len()).rev() {
        let t: String = chars[..n].iter().collect::<String>() + "..";
        if w(&t) <= max_w {
            return t;
        }
    }
    String::new()
}

/// Win95 3D face.
fn bevel(p: &egui::Painter, r: Rect, down: bool) {
    p.rect_filled(r, 0.0, FACE);
    let (tl, br) = if down { (Color32::from_gray(0x80), Color32::WHITE) } else { (Color32::WHITE, Color32::from_gray(0x80)) };
    p.hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0_f32, tl));
    p.vline(r.left() + 0.5, r.y_range(), Stroke::new(1.0_f32, tl));
    p.hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0_f32, br));
    p.vline(r.right() - 0.5, r.y_range(), Stroke::new(1.0_f32, br));
}

/// A small note symbol: head, stem and flags (dot / "3" for dotted / triplet).
fn draw_note_symbol(p: &egui::Painter, base: egui::Pos2, (val, dot, tri): (u32, bool, bool), col: Color32) {
    let head = Rect::from_center_size(base, vec2(6.0, 4.5));
    if val <= 2 {
        p.rect_stroke(head, 2.0, Stroke::new(1.0_f32, col));
    } else {
        p.rect_filled(head, 2.0, col);
    }
    if val >= 2 {
        let x = head.right() - 0.5;
        p.vline(x, (base.y - 11.0)..=base.y, Stroke::new(1.0_f32, col));
        let flags = match val {
            8 => 1,
            16 => 2,
            32 => 3,
            64 => 4,
            _ => 0,
        };
        for f in 0..flags {
            let y = base.y - 11.0 + f as f32 * 2.5;
            p.line_segment([pos2(x, y), pos2(x + 3.5, y + 3.0)], Stroke::new(1.0_f32, col));
        }
    }
    if dot {
        p.circle_filled(pos2(head.right() + 6.0, base.y), 1.0, col);
    }
    if tri {
        p.text(pos2(head.right() + 4.0, base.y + 1.0), egui::Align2::LEFT_BOTTOM, "3", FontId::proportional(8.0), col);
    }
}

/// The built-in fallback font's name in the font dialog.
pub const BUILTIN_FONT: &str = "(built-in monospace)";

/// The list font size in px (0 = the default for the font in use).
static FONT_PX: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The list font: MS Gothic like the original (`ws.ini [List] Font`), drawn
/// at 18 px; else monospace at the size whose glyphs are as wide (9 px); or
/// the font and size chosen in the Font dialog.
fn list_font(ctx: &egui::Context) -> FontId {
    let name = FontFamily::Name("list".into());
    let px = f32::from_bits(FONT_PX.load(std::sync::atomic::Ordering::Relaxed));
    if ctx.fonts(|f| f.families().contains(&name)) {
        FontId::new(if px > 0.0 { px } else { 18.0 }, name)
    } else {
        FontId::new(if px > 0.0 { px } else { 15.0 }, FontFamily::Monospace)
    }
}

/// Row height for a list font: the font's line height less one pixel — 17 px
/// for the original's MS Gothic at 18 px.
fn list_row_h(ctx: &egui::Context, f: &FontId) -> f32 {
    match f.family {
        FontFamily::Monospace if f.size == 15.0 => ROW_H,
        _ => (ctx.fonts(|x| x.row_height(f)) - 1.0).round().max(10.0),
    }
}

/// Use font `bytes` (None = MS Gothic if present, else built-in monospace)
/// at `px` (0 = default) for the list windows.
pub fn set_list_font(ctx: &egui::Context, bytes: Option<Vec<u8>>, px: f32) {
    FONT_PX.store(px.to_bits(), std::sync::atomic::Ordering::Relaxed);
    let mut defs = egui::FontDefinitions::default();
    let data = bytes.or_else(|| crate::platform::read(&crate::platform::list_font_path()));
    if let Some(b) = data {
        defs.font_data.insert("listfont".into(), egui::FontData { font: b.into(), index: 0, tweak: Default::default() });
        let mut fam = vec!["listfont".to_string()];
        fam.extend(defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default());
        defs.families.insert(FontFamily::Name("list".into()), fam);
    }
    ctx.set_fonts(defs);
}

/// The built-in monospace font for the list windows, at `px` (0 = default).
pub fn set_list_font_builtin(ctx: &egui::Context, px: f32) {
    FONT_PX.store(px.to_bits(), std::sync::atomic::Ordering::Relaxed);
    ctx.set_fonts(egui::FontDefinitions::default());
}

/// The family name of a TrueType / OpenType font (or the first of a
/// collection): the 'name' table's family (name id 1), Windows Unicode first.
pub fn font_family_name(b: &[u8]) -> Option<String> {
    let u32_at = |o: usize| b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]) as usize);
    let u16_at = |o: usize| b.get(o..o + 2).map(|x| u16::from_be_bytes([x[0], x[1]]) as usize);
    let font = if b.get(0..4) == Some(b"ttcf") { u32_at(12)? } else { 0 };
    let n = u16_at(font + 4)?;
    let name = (0..n).find_map(|i| {
        let r = font + 12 + 16 * i;
        (b.get(r..r + 4) == Some(b"name")).then(|| u32_at(r + 8)).flatten()
    })?;
    let count = u16_at(name + 2)?;
    let strings = name + u16_at(name + 4)?;
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let r = name + 6 + 12 * i;
        let (pid, eid, lang, nid, len, off) = (u16_at(r)?, u16_at(r + 2)?, u16_at(r + 4)?, u16_at(r + 6)?, u16_at(r + 8)?, u16_at(r + 10)?);
        if nid != 1 {
            continue;
        }
        let raw = b.get(strings + off..strings + off + len)?;
        let (rank, text) = match (pid, eid) {
            (3, 1) | (0, _) => {
                let w: Vec<u16> = raw.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                (if lang == 0x409 { 0 } else { 1 }, String::from_utf16_lossy(&w))
            }
            (1, 0) => (2, raw.iter().map(|&c| c as char).collect()),
            _ => continue,
        };
        if best.as_ref().map_or(true, |b| rank < b.0) {
            best = Some((rank, text));
        }
    }
    best.map(|b| b.1)
}

/// Register MS Gothic (from the Windows fonts) as the "list" family.
pub fn install_font(ctx: &egui::Context) {
    let Some(bytes) = crate::platform::read(&crate::platform::list_font_path()) else { return };
    let mut defs = egui::FontDefinitions::default();
    defs.font_data.insert("msgothic".into(), egui::FontData { font: bytes.into(), index: 0, tweak: Default::default() });
    let mut fam = vec!["msgothic".to_string()];
    fam.extend(defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default());
    defs.families.insert(FontFamily::Name("list".into()), fam);
    ctx.set_fonts(defs);
}
