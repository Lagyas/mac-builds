p = r'F:\rea\xgworks-native\src\main.rs'
s = open(p, encoding='utf-8').read()

a = s.index('// ---------------------------------------------------------------------------\n// Track View (main window)')
b = s.index('    fn draw_playhead(&self')
new = r'''// ---------------------------------------------------------------------------
// Track View (main window), drawn like XGworks' Track View (compared against
// F:\rea\orig_shots\04_trackview_max.png): purple header, grey rows with white
// text, dark-blue arrangement area with grid, blocks as blue bars with a note
// icon and the block name (selected track red), markers on the ruler, and the
// Chord / Style lanes at the bottom. The unnamed conductor track is hidden.
// ---------------------------------------------------------------------------
const ROW_H: f32 = 20.0;
const TV_HEAD: Color32 = Color32::from_rgb(0x66, 0x00, 0x66);
const TV_ROW: Color32 = Color32::from_rgb(0x82, 0x82, 0x82);
const TV_LANE: Color32 = Color32::from_rgb(0x00, 0x40, 0x80);
const TV_GRID: Color32 = Color32::from_rgb(0x20, 0x60, 0xA0);
const TV_BLOCK: Color32 = Color32::from_rgb(0x66, 0x66, 0xFF);
const TV_BLOCK_SEL: Color32 = Color32::from_rgb(0xFF, 0x00, 0x00);
const TV_RULER_TXT: Color32 = Color32::from_rgb(0xFF, 0xCC, 0x66);
const TV_MARKER: Color32 = Color32::from_rgb(0x00, 0xDD, 0xEE);
const TV_CHORD_BG: Color32 = Color32::from_rgb(0xC8, 0xC8, 0xFF);
use egui::Color32;

/// (title, width) of the Track View columns, as in the original.
const TV_COLS: [(&str, f32); 10] = [
    ("Trk", 26.0), ("Port", 54.0), ("Ch/Pan", 56.0), ("Rec", 26.0), ("Name", 92.0),
    ("S", 24.0), ("M", 24.0), ("Trns/Vol", 60.0), ("Type", 52.0), ("Ins", 40.0),
];

const CHORD_TYPES: [&str; 35] = [
    "", "6", "M7", "M7(#11)", "add9", "M7(9)", "6(9)", "aug", "m", "m6", "m7", "m7b5", "madd9", "m7(9)",
    "m7(11)", "mM7", "mM7(9)", "dim", "dim7", "7", "7sus4", "7b5", "7(9)", "7(#11)", "7(13)", "7(b9)",
    "7(b13)", "7(#9)", "M7aug", "7aug", "1+8", "1+5", "sus4", "1+2+5", "cc",
];

/// XF chord root byte: low nibble 1..7 = C..B, high nibble 3 = natural (lower = flats).
fn xf_root(b: u8) -> String {
    let n = ["", "C", "D", "E", "F", "G", "A", "B"].get((b & 0x0F) as usize).copied().unwrap_or("");
    let acc = match b >> 4 {
        0 => "bbb",
        1 => "bb",
        2 => "b",
        4 => "#",
        5 => "##",
        6 => "###",
        _ => "",
    };
    format!("{n}{acc}")
}

/// XF chord meta (FF 7F 43 7B 01 cr ct bn bt) → "Am7/D".
fn xf_chord(data: &[u8]) -> Option<String> {
    if data.len() < 7 || data[0..3] != [0x43, 0x7B, 0x01] {
        return None;
    }
    let mut s = format!("{}{}", xf_root(data[3]), CHORD_TYPES.get(data[4] as usize).copied().unwrap_or(""));
    if data[5] != 0x7F {
        s = format!("{s}/{}", xf_root(data[5]));
    }
    Some(s)
}

impl App {
    /// Indices of the tracks shown in the Track View (conductor hidden).
    fn visible_tracks(&self) -> Vec<usize> {
        (0..self.edit.len()).filter(|&i| !self.edit[i].conductor).collect()
    }

    fn track_view(&mut self, ui: &mut egui::Ui) {
        if self.edit.is_empty() {
            ui.centered_and_justified(|ui| ui.weak("Open an .XWS or .MID file."));
            return;
        }
        let aud = self.audible();
        let vis = self.visible_tracks();
        let bar = self.ppqn as u32 * 4;
        let full = ui.available_rect_before_wrap();
        let hdr_w: f32 = TV_COLS.iter().map(|c| c.1).sum();
        let tl = egui::Rect::from_min_max(egui::pos2(full.left() + hdr_w + 4.0, full.top()), full.max);
        if self.px_per_tick <= 0.0 {
            self.px_per_tick = (tl.width() - 20.0) / self.total_ticks.max(1) as f32; // fit
        }
        // wheel: Ctrl = zoom, Shift / horizontal = scroll time, plain = scroll rows
        if ui.rect_contains_pointer(full) {
            let (scroll, zoom, ctrl, shift, mouse) =
                ui.input(|i| (i.raw_scroll_delta, i.zoom_delta(), i.modifiers.ctrl, i.modifiers.shift, i.pointer.hover_pos()));
            if ctrl && zoom != 1.0 {
                let mx = mouse.map_or(0.0, |p| (p.x - tl.left()).max(0.0));
                let at = self.view_t0 + mx / self.px_per_tick;
                self.px_per_tick = (self.px_per_tick * zoom).clamp(0.002, 2.0);
                self.view_t0 = (at - mx / self.px_per_tick).max(0.0);
            } else if shift || scroll.x != 0.0 {
                let d = if scroll.x != 0.0 { scroll.x } else { scroll.y };
                self.view_t0 = (self.view_t0 - d / self.px_per_tick).clamp(0.0, self.total_ticks as f32);
            } else if scroll.y != 0.0 {
                self.row_scroll = (self.row_scroll - scroll.y / ROW_H).max(0.0);
            }
        }
        let (t0, ppt) = (self.view_t0, self.px_per_tick);
        let x_of = |t: u32| tl.left() + (t as f32 - t0) * ppt;
        let p = ui.painter_at(full);
        let font = egui::FontId::monospace(11.0);
        let bold = |s: &str| egui::RichText::new(s);
        let _ = bold;

        // ---- header row + ruler
        let head = egui::Rect::from_min_size(full.min, egui::vec2(hdr_w, ROW_H));
        p.rect_filled(head, 0.0, TV_HEAD);
        let mut x = full.left();
        for (title, w) in TV_COLS {
            p.text(egui::pos2(x + 3.0, head.center().y), egui::Align2::LEFT_CENTER, title, font.clone(), Color32::WHITE);
            x += w;
            p.line_segment([egui::pos2(x, head.top()), egui::pos2(x, head.bottom())], egui::Stroke::new(1.0_f32, Color32::BLACK));
        }
        let ruler = egui::Rect::from_min_max(egui::pos2(tl.left(), full.top()), egui::pos2(tl.right(), full.top() + ROW_H));
        p.rect_filled(ruler, 0.0, TV_LANE);
        let step = ((36.0 / (bar as f32 * ppt)).ceil() as u32).max(1);
        let markers = self.markers();
        let mut b = (t0 as u32) / bar;
        while x_of(b * bar) <= ruler.right() {
            let xx = x_of(b * bar);
            if b % step == 0 && xx >= ruler.left() {
                p.text(egui::pos2(xx + 2.0, ruler.bottom() - 1.0), egui::Align2::LEFT_BOTTOM, format!("{}", b + 1), font.clone(), TV_RULER_TXT);
            }
            b += 1;
        }
        for (t, name) in &markers {
            let xx = x_of(*t);
            if xx >= ruler.left() && xx <= ruler.right() {
                p.add(egui::Shape::convex_polygon(
                    vec![egui::pos2(xx - 4.0, ruler.bottom() - 6.0), egui::pos2(xx + 4.0, ruler.bottom() - 6.0), egui::pos2(xx, ruler.bottom())],
                    TV_MARKER, egui::Stroke::NONE));
                p.text(egui::pos2(xx, ruler.top()), egui::Align2::CENTER_TOP, name, egui::FontId::monospace(10.0), TV_RULER_TXT);
            }
        }
        self.draw_playhead(&p, ruler, t0, ppt);
        let ruler_resp = ui.interact(ruler, ui.id().with("tv_ruler"), egui::Sense::click());
        let _ = ruler_resp;

        // ---- track rows (pool rows after the used tracks, like the original)
        let bottom_lanes = 2.0 * ROW_H * 2.0;
        let rows_area = egui::Rect::from_min_max(egui::pos2(full.left(), head.bottom()), egui::pos2(full.right(), full.bottom() - bottom_lanes));
        let rows_fit = (rows_area.height() / ROW_H).floor() as usize;
        let total_rows = (vis.len() + 30).max(rows_fit);
        self.row_scroll = self.row_scroll.min(total_rows.saturating_sub(rows_fit) as f32);
        let first = self.row_scroll as usize;
        let rp = ui.painter_at(rows_area);
        rp.rect_filled(egui::Rect::from_min_max(egui::pos2(tl.left(), rows_area.top()), rows_area.max), 0.0, TV_LANE);
        // vertical grid every bar (thinned)
        let gstep = ((10.0 / (bar as f32 * ppt)).ceil() as u32).max(1);
        let mut b = (t0 as u32) / bar;
        while x_of(b * bar) <= rows_area.right() {
            if b % gstep == 0 {
                let xx = x_of(b * bar);
                rp.line_segment([egui::pos2(xx, rows_area.top()), egui::pos2(xx, rows_area.bottom())], egui::Stroke::new(1.0_f32, TV_GRID));
            }
            b += 1;
        }
        for r in 0..rows_fit.min(total_rows - first) {
            let row = first + r;
            let y = rows_area.top() + r as f32 * ROW_H;
            let hdr = egui::Rect::from_min_size(egui::pos2(full.left(), y), egui::vec2(hdr_w, ROW_H));
            let lane = egui::Rect::from_min_max(egui::pos2(tl.left(), y), egui::pos2(tl.right(), y + ROW_H));
            rp.line_segment([lane.left_bottom(), lane.right_bottom()], egui::Stroke::new(1.0_f32, TV_GRID));
            let ti = vis.get(row).copied();
            self.tv_row(&rp, ui, hdr, lane, row, ti, &aud, &x_of, bar);
        }

        // ---- Chord / Style lanes
        let cl = egui::Rect::from_min_max(egui::pos2(full.left(), full.bottom() - bottom_lanes), full.max);
        let lane_h = bottom_lanes / 2.0;
        for (k, label, kind) in [(0, "C", "CHORD"), (1, "S", "STYLE")] {
            let y = cl.top() + k as f32 * lane_h;
            let hdr = egui::Rect::from_min_size(egui::pos2(full.left(), y), egui::vec2(hdr_w, lane_h));
            p.rect_filled(hdr, 0.0, Color32::from_rgb(0x60, 0x60, 0xA0));
            p.text(egui::pos2(hdr.left() + 10.0, hdr.center().y), egui::Align2::CENTER_CENTER, label, font.clone(), Color32::WHITE);
            p.text(egui::pos2(hdr.left() + TV_COLS[..8].iter().map(|c| c.1).sum::<f32>() + 4.0, hdr.center().y),
                egui::Align2::LEFT_CENTER, kind, font.clone(), Color32::WHITE);
            let lane = egui::Rect::from_min_max(egui::pos2(tl.left(), y), egui::pos2(tl.right(), y + lane_h));
            p.rect_filled(lane, 0.0, TV_CHORD_BG);
            p.rect_stroke(lane, 0.0, egui::Stroke::new(1.0_f32, Color32::GRAY));
        }
        let chord_lane = egui::Rect::from_min_max(egui::pos2(tl.left(), cl.top()), egui::pos2(tl.right(), cl.top() + lane_h));
        let cp = ui.painter_at(chord_lane);
        let mut chords: Vec<(u32, String)> = self.edit.iter().flat_map(|t| t.others.iter()).filter_map(|e| match e {
            Event::Meta { t, mtype: 0x7F, data } => xf_chord(data).map(|c| (*t, c)),
            _ => None,
        }).collect();
        chords.sort_by_key(|c| c.0);
        for (t, c) in &chords {
            let xx = x_of(*t);
            if xx >= chord_lane.left() - 40.0 && xx <= chord_lane.right() {
                cp.line_segment([egui::pos2(xx, chord_lane.top() + 4.0), egui::pos2(xx, chord_lane.bottom() - 4.0)], egui::Stroke::new(1.0_f32, Color32::BLACK));
                cp.text(egui::pos2(xx + 2.0, chord_lane.center().y), egui::Align2::LEFT_CENTER, c, egui::FontId::monospace(10.0), Color32::BLACK);
            }
        }
        ui.allocate_rect(full, egui::Sense::hover());
    }

    #[allow(clippy::too_many_arguments)]
    fn tv_row(&mut self, p: &egui::Painter, ui: &egui::Ui, hdr: egui::Rect, lane: egui::Rect, row: usize, ti: Option<usize>,
              aud: &[bool], x_of: &dyn Fn(u32) -> f32, bar: u32) {
        let font = egui::FontId::monospace(11.0);
        let sel = ti == Some(self.sel_track);
        // header cells
        p.rect_filled(hdr, 0.0, TV_ROW);
        let mut x = hdr.left();
        let mut cells: Vec<egui::Rect> = Vec::new();
        for (_, w) in TV_COLS {
            cells.push(egui::Rect::from_min_size(egui::pos2(x, hdr.top()), egui::vec2(w, ROW_H)));
            x += w;
        }
        p.rect_filled(cells[0].shrink(1.0), 0.0, Color32::from_gray(0xF0));
        p.text(cells[0].center(), egui::Align2::CENTER_CENTER, format!("{}", row + 1), font.clone(), Color32::BLACK);
        let port = self.out_names.get(self.sel_out).map(|n| n.chars().take(6).collect::<String>()).unwrap_or_default();
        p.text(cells[1].left_center() + egui::vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, &port, font.clone(), Color32::WHITE);
        for c in &cells[1..] {
            p.line_segment([c.left_top(), c.left_bottom()], egui::Stroke::new(1.0_f32, Color32::from_gray(0x40)));
        }
        p.line_segment([hdr.left_bottom(), hdr.right_bottom()], egui::Stroke::new(1.0_f32, Color32::from_gray(0x40)));
        let Some(ti) = ti else {
            p.text(cells[2].center(), egui::Align2::CENTER_CENTER, format!("{}", (row % 16) + 1), font.clone(), Color32::WHITE);
            p.text(cells[7].center(), egui::Align2::CENTER_CENTER, "0", font.clone(), Color32::WHITE);
            return;
        };
        let t = &self.edit[ti];
        let drum = t.channel & 0x0F == 9;
        p.text(cells[2].center(), egui::Align2::CENTER_CENTER, format!("{}", (t.channel & 0x0F) + 1), font.clone(), Color32::WHITE);
        let name_col = if aud[ti] { Color32::WHITE } else { Color32::from_gray(0xB0) };
        p.text(cells[4].left_center() + egui::vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, &t.name, font.clone(), name_col);
        let (muted, solo) = self.flags[ti];
        if solo {
            p.rect_filled(cells[5].shrink(2.0), 0.0, Color32::from_rgb(214, 160, 40));
            p.text(cells[5].center(), egui::Align2::CENTER_CENTER, "S", font.clone(), Color32::BLACK);
        }
        if muted {
            p.rect_filled(cells[6].shrink(2.0), 0.0, Color32::from_rgb(200, 64, 56));
            p.text(cells[6].center(), egui::Align2::CENTER_CENTER, "M", font.clone(), Color32::WHITE);
        }
        p.text(cells[7].center(), egui::Align2::CENTER_CENTER, if drum { "Off" } else { "0" }, font.clone(), Color32::WHITE);
        p.text(cells[8].center(), egui::Align2::CENTER_CENTER, "MIDI", font.clone(), Color32::WHITE);
        let m = &self.mix[ti];
        let ins = self.xg.voice_category(m.bank_msb, m.bank_lsb, m.program).map(|c| c.chars().take(3).collect::<String>())
            .unwrap_or_else(|| "Non".into());
        p.text(cells[9].left_center() + egui::vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, &ins, font.clone(), Color32::WHITE);
        if sel {
            p.rect_stroke(cells[1].shrink(1.0), 0.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 220, 220)));
        }

        // blocks
        let base = if sel { TV_BLOCK_SEL } else { TV_BLOCK };
        let blocks: Vec<xgworks_core::EditBlock> = if t.blocks.is_empty() {
            let end = t.notes.iter().map(|n| n.tick + n.dur).chain(t.others.iter().map(|e| e.t())).max().unwrap_or(0);
            vec![xgworks_core::EditBlock { name: t.name.clone(), start: 0, end: (end + bar - 1) / bar * bar, raw: Vec::new() }]
        } else {
            t.blocks.clone()
        };
        let lp = ui.painter_at(lane);
        for blk in &blocks {
            let (x0, x1) = (x_of(blk.start), x_of(blk.end.max(blk.start + 1)).max(x_of(blk.start) + 12.0));
            if x1 < lane.left() || x0 > lane.right() {
                continue;
            }
            let r = egui::Rect::from_min_max(egui::pos2(x0, lane.top() + 4.0), egui::pos2(x1, lane.bottom() - 4.0));
            let col = if aud[ti] { base } else { base.linear_multiply(0.45) };
            lp.rect_filled(r, 0.0, col);
            lp.line_segment([r.left_top() + egui::vec2(0.0, 2.0), r.right_top() + egui::vec2(0.0, 2.0)], egui::Stroke::new(1.0_f32, Color32::WHITE.linear_multiply(0.6)));
            lp.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(0x0C)));
            lp.text(r.left_center() + egui::vec2(3.0, 0.0), egui::Align2::LEFT_CENTER,
                if blk.name.is_empty() || x1 - x0 < 40.0 { "♪".to_string() } else { format!("♪ {}", blk.name) },
                egui::FontId::monospace(11.0), Color32::WHITE);
        }
        self.draw_playhead(&lp, lane, self.view_t0, self.px_per_tick);

        // interaction: click selects, double-click opens the Piano Roll, S/M toggle
        let row_rect = egui::Rect::from_min_max(hdr.min, lane.max);
        let resp = ui.interact(row_rect, ui.id().with(("tv_row", ti)), egui::Sense::click());
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if cells[5].contains(pos) {
                    self.flags[ti].1 = !self.flags[ti].1;
                } else if cells[6].contains(pos) {
                    self.flags[ti].0 = !self.flags[ti].0;
                } else {
                    self.sel_track = ti;
                }
            }
        }
        if resp.double_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if !cells[5].contains(pos) && !cells[6].contains(pos) {
                    self.open_roll(ti);
                }
            }
        }
    }

'''
s = s[:a] + new + s[b:]

# remove old helpers cell / cell_sense (no longer used)
a = s.index('fn cell(ui: &mut egui::Ui, w: f32, text: egui::RichText) {')
b = s.index('// ---------------------------------------------------------------------------\n// Mixer window')
s = s[:a] + s[b:]


def rep(x, y):
    global s
    assert s.count(x) == 1, x[:60]
    s = s.replace(x, y)


rep('    px_per_tick: f32,\n', '    px_per_tick: f32,\n    row_scroll: f32,\n')
rep('            px_per_tick: 0.0,\n', '            px_per_tick: 0.0,\n            row_scroll: 0.0,\n')
# --roll N refers to the visible track number (1-based, conductor hidden)
rep('''                        app.open_roll(n.saturating_sub(1));''', '''                        if let Some(&ti) = app.visible_tracks().get(n.saturating_sub(1)) {
                            app.open_roll(ti);
                        }''')
open(p, 'w', encoding='utf-8').write(s)

p = r'F:\rea\xgworks-native\src\xged.rs'
s = open(p, encoding='utf-8').read()
rep('''    /// Reload the parameter memory''', '''    /// Voice-list category of a voice ("Bass", "Guitar", …) for the Ins column.
    pub fn voice_category(&self, msb: u8, lsb: u8, prg: u8) -> Option<&str> {
        self.voices.iter().find(|v| v.msb == msb && v.lsb == lsb && v.program == prg).map(|v| v.category.as_str())
    }

    /// Reload the parameter memory''')
open(p, 'w', encoding='utf-8').write(s)
print('ok')
