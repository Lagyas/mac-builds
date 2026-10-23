p = r'F:\rea\xgworks-native\src\pianoroll.rs'
s = open(p, encoding='utf-8').read()


def rep(a, b):
    global s
    assert s.count(a) == 1, (a[:70], s.count(a))
    s = s.replace(a, b)


rep('''    Move { dtick: i64, dpitch: i32, anchor: usize },''',
    '''    Move { dtick: i64, dpitch: i32, anchor: usize },
    Resize { anchor: usize, dlen: i64 },''')

rep('''    fitted: bool,
    ppqn: u32,
}''', '''    fitted: bool,
    ppqn: u32,
    /// Edit cursor (paste position), set by clicking the ruler or empty grid.
    cursor: u32,
    undo: Vec<(Vec<EditNote>, Vec<Event>)>,
    redo: Vec<(Vec<EditNote>, Vec<Event>)>,
}''')
rep('''            fitted: false,
            ppqn: 480,
        }''', '''            fitted: false,
            ppqn: 480,
            cursor: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        }''')
rep('''    pub play_tick: Option<f32>,
}''', '''    pub play_tick: Option<f32>,
    /// Shared note clipboard (all piano rolls): notes relative to tick 0.
    pub clipboard: &'a std::cell::RefCell<Vec<EditNote>>,
}

/// Sort notes by (tick, pitch) and remap the selection to the new order.
fn sort_notes(notes: &mut Vec<EditNote>, sel: &mut Vec<usize>) {
    let mut idx: Vec<usize> = (0..notes.len()).collect();
    idx.sort_by_key(|&i| (notes[i].tick, notes[i].pitch));
    let mut pos = vec![0usize; notes.len()];
    for (new, &old) in idx.iter().enumerate() {
        pos[old] = new;
    }
    let sorted: Vec<EditNote> = idx.iter().map(|&i| notes[i].clone()).collect();
    *notes = sorted;
    for s in sel.iter_mut() {
        *s = pos[*s];
    }
    sel.sort();
    sel.dedup();
}

impl RollState {
    fn snapshot(&mut self, tr: &EditTrack) {
        self.undo.push((tr.notes.clone(), tr.others.clone()));
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}''')

# ---- interaction block
a = s.index('        let hit = |pos: Pos2, notes: &[EditNote]| notes.iter().rposition(|n| note_rect(n).contains(pos));')
b = s.index('        if let Some(t) = cx.play_tick {\n            let x = x_of(t as u32);')
s = s[:a] + '''        let hit = |pos: Pos2, notes: &[EditNote]| notes.iter().rposition(|n| note_rect(n).contains(pos));
        let near_end = |pos: Pos2, n: &EditNote| (note_rect(n).right() - pos.x).abs() <= 4.0;
        let tr = &mut tracks[self.track];
        let (shift, ctrl_m) = ui.input(|i| (i.modifiers.shift, i.modifiers.ctrl || i.modifiers.command));
        // where the button went down (drag starts are detected only after some
        // movement, by then the pointer may already have left the note)
        let press = ui.input(|i| i.pointer.press_origin());

        if let Some(pos) = resp.hover_pos() {
            if let Some(i) = hit(pos, &tr.notes) {
                if near_end(pos, &tr.notes[i]) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                } else if self.tool != Tool::Eraser {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }

        // clicks
        if resp.secondary_clicked() || (resp.clicked() && self.tool == Tool::Eraser) {
            if let Some(pos) = resp.interact_pointer_pos() {
                if let Some(i) = hit(pos, &tr.notes) {
                    self.snapshot(tr);
                    tr.notes.remove(i);
                    self.sel.clear();
                    changed = true;
                }
            }
        } else if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                match hit(pos, &tr.notes) {
                    Some(i) if shift || ctrl_m => {
                        if let Some(k) = self.sel.iter().position(|&s| s == i) {
                            self.sel.remove(k);
                        } else {
                            self.sel.push(i);
                        }
                    }
                    Some(i) => self.sel = vec![i],
                    None if self.tool == Tool::Pencil => {
                        // click on empty grid with the pencil: one note of the quantize length
                        self.snapshot(tr);
                        let raw = tick_of(pos.x);
                        let st = if step <= 1 { raw } else { raw / step * step };
                        tr.notes.push(EditNote { tick: st, dur: step.max(ppqn / 4), pitch: pitch_of(pos.y), vel: self.new_vel });
                        self.sel = vec![tr.notes.len() - 1];
                        self.cursor = st;
                        changed = true;
                    }
                    None => {
                        self.sel.clear();
                        self.cursor = snap(tick_of(pos.x));
                    }
                }
            }
        }

        // drags
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = press.or(resp.interact_pointer_pos()) {
                let raw = tick_of(pos.x);
                let pi = pitch_of(pos.y);
                match (self.tool, hit(pos, &tr.notes)) {
                    (Tool::Eraser, _) => self.drag = Drag::Rubber { from: pos },
                    (_, Some(i)) => {
                        if !self.sel.contains(&i) {
                            self.sel = if shift || ctrl_m { let mut v = self.sel.clone(); v.push(i); v } else { vec![i] };
                        }
                        self.snapshot(tr);
                        let n = &tr.notes[i];
                        self.drag = if near_end(pos, n) {
                            Drag::Resize { anchor: i, dlen: (n.tick + n.dur) as i64 - raw as i64 }
                        } else {
                            Drag::Move { dtick: n.tick as i64 - raw as i64, dpitch: n.pitch as i32 - pi as i32, anchor: i }
                        };
                    }
                    (Tool::Pencil, None) => {
                        let st = if step <= 1 { raw } else { raw / step * step };
                        self.drag = Drag::Create { pitch: pi, start: st, cur: st + step.max(ppqn / 4) };
                    }
                    (Tool::Select, None) => self.drag = Drag::Rubber { from: pos },
                }
            }
        }
        if resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                match self.drag {
                    Drag::Move { dtick, dpitch, anchor } if anchor < tr.notes.len() => {
                        let nt = snap(((tick_of(pos.x) as i64 + dtick).max(0)) as u32) as i64;
                        let np = (pitch_of(pos.y) as i32 + dpitch).clamp(0, 127);
                        let (dt, dp) = (nt - tr.notes[anchor].tick as i64, np - tr.notes[anchor].pitch as i32);
                        if dt != 0 || dp != 0 {
                            for &i in &self.sel {
                                if let Some(n) = tr.notes.get_mut(i) {
                                    n.tick = (n.tick as i64 + dt).max(0) as u32;
                                    n.pitch = (n.pitch as i32 + dp).clamp(0, 127) as u8;
                                }
                            }
                            changed = true;
                        }
                    }
                    Drag::Resize { anchor, dlen } if anchor < tr.notes.len() => {
                        let end = snap(((tick_of(pos.x) as i64 + dlen).max(0)) as u32) as i64;
                        let d = end - (tr.notes[anchor].tick + tr.notes[anchor].dur) as i64;
                        if d != 0 {
                            for &i in &self.sel {
                                if let Some(n) = tr.notes.get_mut(i) {
                                    n.dur = (n.dur as i64 + d).max(step.max(1) as i64) as u32;
                                }
                            }
                            changed = true;
                        }
                    }
                    Drag::Create { pitch, start, .. } => {
                        let cur = snap(tick_of(pos.x)).max(start + step.max(1));
                        self.drag = Drag::Create { pitch, start, cur };
                    }
                    Drag::Rubber { from } => {
                        let r = Rect::from_two_pos(from, pos);
                        p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::BLACK));
                    }
                    _ => {}
                }
            }
        }
        if let Drag::Create { pitch, start, cur } = self.drag {
            let r = note_rect(&EditNote { tick: start, dur: cur - start, pitch, vel: self.new_vel });
            p.rect_stroke(r, 2.0, Stroke::new(1.5_f32, BLUE_NOTE));
        }
        if resp.drag_stopped() {
            match self.drag {
                Drag::Create { pitch, start, cur } => {
                    self.snapshot(tr);
                    tr.notes.push(EditNote { tick: start, dur: cur - start, pitch, vel: self.new_vel });
                    self.sel = vec![tr.notes.len() - 1];
                    changed = true;
                }
                Drag::Rubber { from } => {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let r = Rect::from_two_pos(from, pos);
                        let hits: Vec<usize> = (0..tr.notes.len()).filter(|&i| note_rect(&tr.notes[i]).intersects(r)).collect();
                        if self.tool == Tool::Eraser {
                            if !hits.is_empty() {
                                self.snapshot(tr);
                                let mut k = 0;
                                tr.notes.retain(|_| {
                                    k += 1;
                                    !hits.contains(&(k - 1))
                                });
                                changed = true;
                            }
                            self.sel.clear();
                        } else if shift || ctrl_m {
                            self.sel.extend(hits);
                        } else {
                            self.sel = hits;
                        }
                    }
                }
                Drag::Move { .. } | Drag::Resize { .. } => changed = true,
                _ => {}
            }
            self.drag = Drag::None;
            if changed {
                sort_notes(&mut tr.notes, &mut self.sel);
            }
        }

        // keyboard: Del, Ctrl+A/C/X/V/Z/Y, arrows (only for the focused window)
        if ui.ctx().input(|i| i.focused) && !ui.ctx().wants_keyboard_input() && matches!(self.drag, Drag::None) {
            let keys = ui.input(|i| {
                let k = |key| i.key_pressed(key);
                (k(egui::Key::Delete) || k(egui::Key::Backspace), k(egui::Key::A), k(egui::Key::C), k(egui::Key::X),
                 k(egui::Key::V), k(egui::Key::Z), k(egui::Key::Y), k(egui::Key::ArrowUp), k(egui::Key::ArrowDown),
                 k(egui::Key::ArrowLeft), k(egui::Key::ArrowRight))
            });
            let (del, ka, kc, kx, kv, kz, ky, up, down, left, right) = keys;
            // egui turns Ctrl+C/X/V into Copy/Cut/Paste events
            let (ev_copy, ev_cut, ev_paste) = ui.input(|i| {
                let mut r = (false, false, false);
                for e in &i.events {
                    match e {
                        egui::Event::Copy => r.0 = true,
                        egui::Event::Cut => r.1 = true,
                        egui::Event::Paste(_) => r.2 = true,
                        _ => {}
                    }
                }
                r
            });
            let sel_notes = |tr: &EditTrack, sel: &[usize]| -> Vec<EditNote> {
                let v: Vec<EditNote> = sel.iter().filter_map(|&i| tr.notes.get(i).cloned()).collect();
                let t0 = v.iter().map(|n| n.tick).min().unwrap_or(0);
                v.into_iter().map(|mut n| { n.tick -= t0; n }).collect()
            };
            if ctrl_m && ka {
                self.sel = (0..tr.notes.len()).collect();
            }
            if (ctrl_m && kc) || ev_copy || (ctrl_m && kx) || ev_cut {
                if !self.sel.is_empty() {
                    *cx.clipboard.borrow_mut() = sel_notes(tr, &self.sel);
                }
            }
            if ((ctrl_m && kx) || ev_cut || del) && !self.sel.is_empty() {
                self.snapshot(tr);
                let sel = std::mem::take(&mut self.sel);
                let mut k = 0;
                tr.notes.retain(|_| {
                    k += 1;
                    !sel.contains(&(k - 1))
                });
                changed = true;
            }
            if ((ctrl_m && kv) || ev_paste) && !cx.clipboard.borrow().is_empty() {
                self.snapshot(tr);
                let base = tr.notes.len();
                let clip = cx.clipboard.borrow().clone();
                let len = clip.iter().map(|n| n.tick + n.dur).max().unwrap_or(0);
                for mut n in clip {
                    n.tick += self.cursor;
                    tr.notes.push(n);
                }
                self.sel = (base..tr.notes.len()).collect();
                self.cursor += len; // repeated paste lays copies end to end
                changed = true;
            }
            if ctrl_m && kz {
                if let Some((n, o)) = self.undo.pop() {
                    self.redo.push((std::mem::replace(&mut tr.notes, n), std::mem::replace(&mut tr.others, o)));
                    self.sel.clear();
                    changed = true;
                }
            }
            if ctrl_m && ky {
                if let Some((n, o)) = self.redo.pop() {
                    self.undo.push((std::mem::replace(&mut tr.notes, n), std::mem::replace(&mut tr.others, o)));
                    self.sel.clear();
                    changed = true;
                }
            }
            if !self.sel.is_empty() && (up || down || left || right) {
                self.snapshot(tr);
                let dp: i32 = if up { 1 } else if down { -1 } else { 0 } * if shift { 12 } else { 1 };
                let dt: i64 = if right { step.max(1) as i64 } else if left { -(step.max(1) as i64) } else { 0 };
                for &i in &self.sel {
                    if let Some(n) = tr.notes.get_mut(i) {
                        n.pitch = (n.pitch as i32 + dp).clamp(0, 127) as u8;
                        n.tick = (n.tick as i64 + dt).max(0) as u32;
                    }
                }
                changed = true;
            }
            if changed {
                sort_notes(&mut tr.notes, &mut self.sel);
            }
        }
        // edit cursor
        let cx_x = x_of(self.cursor);
        if cx_x >= gx0 && cx_x <= notes.right() {
            p.line_segment([Pos2::new(cx_x, notes.top()), Pos2::new(cx_x, notes.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(0, 150, 0)));
        }
''' + s[b:]

# the old end-of-ui sort would scramble selection indices -> remove it
rep('''        if changed {
            tr.notes.sort_by_key(|n| (n.tick, n.pitch));
            if !matches!(self.drag, Drag::Move { .. }) {
                self.sel.retain(|&i| i < tr.notes.len());
            }
        }
        changed
    }''', '''        self.sel.retain(|&i| i < tr.notes.len());
        changed
    }''')

# ruler click sets the edit cursor
rep('''        // ---------------- keyboard + grid''', '''        let rr = ui.interact(Rect::from_min_max(Pos2::new(gx0, ruler.top()), ruler.max), ui.id().with(("roll_ruler", self.track)), Sense::click());
        if let Some(pos) = rr.interact_pointer_pos().filter(|_| rr.clicked()) {
            self.cursor = snap(tick_of(pos.x));
        }

        // ---------------- keyboard + grid''')

# ASCII labels instead of glyphs the default font lacks
for a, b in [('"⇕+").on_hover_text("Zoom in (vertical)")', '"V+").on_hover_text("Zoom in (vertical)")'),
             ('"⇕−").on_hover_text("Zoom out (vertical)")', '"V-").on_hover_text("Zoom out (vertical)")'),
             ('ui.selectable_value(&mut self.quant, 4, "♩").on_hover_text("1/4");', 'ui.selectable_value(&mut self.quant, 4, "1/4");'),
             ('ui.selectable_value(&mut self.quant, 8, "♪").on_hover_text("1/8");', 'ui.selectable_value(&mut self.quant, 8, "1/8");'),
             ('ui.selectable_value(&mut self.quant, 16, "𝅘𝅥𝅯").on_hover_text("1/16");', 'ui.selectable_value(&mut self.quant, 16, "1/16");'),
             ('ui.toggle_value(&mut self.dotted, "•").on_hover_text("Dotted");', 'ui.toggle_value(&mut self.dotted, " . ").on_hover_text("Dotted");'),
             ('ui.toggle_value(&mut self.triplet, "3").on_hover_text("Triplet");', 'ui.toggle_value(&mut self.triplet, "3").on_hover_text("Triplet");'),
             ('ui.selectable_value(&mut self.tool, Tool::Select, "↖").on_hover_text("Select");', 'ui.selectable_value(&mut self.tool, Tool::Select, "Select");'),
             ('ui.selectable_value(&mut self.tool, Tool::Pencil, "✏").on_hover_text("Pencil");', 'ui.selectable_value(&mut self.tool, Tool::Pencil, "Pencil");'),
             ('ui.selectable_value(&mut self.tool, Tool::Eraser, "⌫").on_hover_text("Eraser");', 'ui.selectable_value(&mut self.tool, Tool::Eraser, "Eraser");'),
             ('ui.selectable_value(&mut self.shape, Shape::Line, "╲").on_hover_text("Line");', 'ui.selectable_value(&mut self.shape, Shape::Line, "Line");'),
             ('ui.selectable_value(&mut self.shape, Shape::Free, "∿").on_hover_text("Freehand");', 'ui.selectable_value(&mut self.shape, Shape::Free, "Free");')]:
    rep(a, b)

# controller edits are undoable too
rep('''            if ctl == Ctl::Velocity {
                for n in tr.notes.iter_mut() {''', '''            self.snapshot(tr);
            if ctl == Ctl::Velocity {
                for n in tr.notes.iter_mut() {''')
open(p, 'w', encoding='utf-8').write(s)

p = r'F:\rea\xgworks-native\src\main.rs'
s = open(p, encoding='utf-8').read()
rep('''            let cx = pianoroll::RollCtx { ppqn: self.ppqn as u32, total_ticks: self.total_ticks, markers: &markers, track_names: &names, play_tick };''',
    '''            let cx = pianoroll::RollCtx {
                ppqn: self.ppqn as u32,
                total_ticks: self.total_ticks,
                markers: &markers,
                track_names: &names,
                play_tick,
                clipboard: &self.clipboard,
            };''')
rep('''    next_roll_id: u64,''', '''    next_roll_id: u64,
    clipboard: std::cell::RefCell<Vec<xgworks_core::EditNote>>,''')
rep('''            next_roll_id: 0,''', '''            next_roll_id: 0,
            clipboard: std::cell::RefCell::new(Vec::new()),''')
open(p, 'w', encoding='utf-8').write(s)
print('ok')
