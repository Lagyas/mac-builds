//! Mixer window, rebuilt after XGworks' own Mixer (Mixer.hlp, dialog 3400,
//! bitmaps 3200–3299 and 3507 of XGworks.exe; reference shots in
//! F:\rea\orig_shots\05_mixer.png, m_ms.png).
//!
//! Layout is the original's pixel layout: a 630×357 panel bitmap with 16
//! channel strips 37 px apart plus the master unit; the live parts (meters,
//! Pgm boxes, knobs, mute/solo, faders, Port, Hi/Lo) are drawn over it at the
//! positions of the original child windows. The artwork is read from the
//! user's XGworks.exe at run time; without it a plain vector look is used.
//!
//! Behaviour (Mixer.hlp): moving a control only sends it to the device (Port);
//! the song changes only via "Insert Mixer Parameter" (or Record). With Play
//! Monitor on, the song's own channel events move the controls while playing.

use eframe::egui::{self, pos2, vec2, Color32, Key, Pos2, Rect, Sense, Stroke};
use std::collections::HashMap;
use xgworks_core::mixer::{self, ChanMix, ChanTracker, NRPN_EQ_HI, NRPN_EQ_LO};
use xgworks_core::xgtbl::Voice;
use xgworks_core::{EditTrack, Event};

use crate::peres;

// ---- geometry (panel-bitmap coordinates; the panel sits at client (2,32)) ----
const PANEL: Pos2 = pos2(2.0, 32.0);
pub const CLIENT: [f32; 2] = [634.0, 392.0];
const PITCH: f32 = 37.0;
const METER: (f32, f32) = (14.0, 4.0);
const PGM_LABEL: (f32, f32) = (2.0, 62.0);
const PGM_BOX: (f32, f32, f32, f32) = (4.0, 72.0, 27.0, 11.0);
const KNOB_Y: [f32; 4] = [86.0, 117.0, 146.0, 175.0]; // Eq, Rev, Cho, Var (32×29, centre 16,18)
const MUTE: (f32, f32) = (5.0, 207.0);
const SOLO: (f32, f32) = (22.0, 207.0);
const PAN_Y: f32 = 227.0; // 32×32, centre 16,20
const FADER: (f32, f32) = (8.0, 264.0); // 18×84, cap 18×14 travels 70 px
const MASTER_X: f32 = 602.0;
const PORT_BOX: (f32, f32, f32, f32) = (602.0, 33.0, 18.0, 12.0);
const HILO: (f32, f32) = (596.0, 91.0); // 14×22: Hi top, Lo bottom
const TOOL_X: [f32; 4] = [5.0, 36.0, 67.0, 90.0];

// ---- bitmap ids in XGworks.exe ----
const BM_PANEL: u32 = 3200;
const BM_METER: u32 = 3201; // +0..16
const BM_FADER: u32 = 3218; // +ch
const BM_FADER_MUTED: u32 = 3234; // +ch
const BM_MUTE_ON: u32 = 3250;
const BM_MUTE_OFF: u32 = 3251;
const BM_MUTE_SOLO: u32 = 3252;
const BM_CAP: u32 = 3255;
const BM_CAP_MUTED: u32 = 3256;
const BM_CAP_SEL: u32 = 3257;
const BM_SOLO_OFF: u32 = 3259;
const BM_SOLO_ON: u32 = 3260;
const BM_MASTER: u32 = 3264;
const BM_EQ: u32 = 3272; // +1 = highlighted
const BM_PAN: u32 = 3276;
const BM_PGM: u32 = 3288;
const BM_HI: u32 = 3292;
const BM_LO: u32 = 3293;
const BM_REV: u32 = 3294;
const BM_CHO: u32 = 3296;
const BM_VAR: u32 = 3298;
const BM_TOOLS: u32 = 3507; // 4 icons 16×15

const C_EQ_PTR: Color32 = Color32::from_rgb(0, 255, 255);
const C_FX_PTR: Color32 = Color32::from_rgb(204, 153, 255);
const C_PAN_PTR: Color32 = Color32::from_rgb(255, 0, 0);
const C_PANEL: Color32 = Color32::from_rgb(85, 85, 85);
const C_FACE: Color32 = Color32::from_rgb(240, 240, 240);

/// Mixer Setup's "Assignable Control" list = CC 0..126 (as in dialog 3406).
const AC_NAMES: [&str; 127] = [
    "Bank Select MSB", "Modulation", "Breath Controller", "----", "Foot Controller", "Portamento Time",
    "Data Entry MSB", "Volume", "Balance", "----", "Pan", "Expression", "----", "----", "----", "----",
    "Gen.Purpose Cntrl1", "Gen.Purpose Cntrl2", "Gen.Purpose Cntrl3", "Gen.Purpose Cntrl4", "----", "----",
    "----", "----", "----", "----", "----", "----", "----", "----", "----", "----", "Bank Select LSB", "----",
    "----", "----", "----", "----", "Data Entry LSB", "----", "----", "----", "----", "----", "----", "----",
    "----", "----", "----", "----", "----", "----", "----", "----", "----", "----", "----", "----", "----",
    "----", "----", "----", "----", "----", "Hold1", "Portamento", "Sostenuto", "Soft Pedal", "----", "Hold2",
    "----", "Harmonic Content", "Release Time", "Attack Time", "Brightness", "Decay Time", "Vibrate Rate",
    "Vibrate Depth", "Vibrate Delay", "----", "Gen.Purpose Cntrl5", "Gen.Purpose Cntrl6", "Gen.Purpose Cntrl7",
    "Gen.Purpose Cntrl8", "Portamento Control", "----", "----", "----", "----", "----", "----", "Reverb Send",
    "----", "Chorus Send", "Variation Send", "----", "RPN Increment", "RPN Decrement", "NRPN LSB", "NRPN MSB",
    "RPN LSB", "RPN MSB", "----", "----", "----", "----", "----", "----", "----", "----", "----", "----", "----",
    "----", "----", "----", "----", "----", "----", "----", "All Sound Off", "Reset All Controllers",
    "Local Control", "All Note Off", "Omni Mode Off", "Omni Mode On", "Mono",
];

/// A selectable control ("highlight" in Mixer.hlp). Row order is the
/// keyboard order top to bottom; mute/solo/Hi-Lo are not selectable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ctl {
    Pgm(usize),
    Eq(usize),
    Rev(usize),
    Cho(usize),
    Var(usize),
    Pan(usize),
    Fader(usize),
    Master,
}

impl Ctl {
    fn row(self) -> usize {
        match self {
            Ctl::Pgm(_) => 0,
            Ctl::Eq(_) => 1,
            Ctl::Rev(_) => 2,
            Ctl::Cho(_) => 3,
            Ctl::Var(_) => 4,
            Ctl::Pan(_) => 5,
            Ctl::Fader(_) | Ctl::Master => 6,
        }
    }
    fn ch(self) -> Option<usize> {
        match self {
            Ctl::Pgm(c) | Ctl::Eq(c) | Ctl::Rev(c) | Ctl::Cho(c) | Ctl::Var(c) | Ctl::Pan(c) | Ctl::Fader(c) => Some(c),
            Ctl::Master => None,
        }
    }
    fn at(row: usize, ch: usize) -> Ctl {
        match row {
            0 => Ctl::Pgm(ch),
            1 => Ctl::Eq(ch),
            2 => Ctl::Rev(ch),
            3 => Ctl::Cho(ch),
            4 => Ctl::Var(ch),
            5 => Ctl::Pan(ch),
            _ => Ctl::Fader(ch),
        }
    }
}

/// What the app has to do after a frame of the mixer.
pub enum MixAction {
    /// Send to the mixer's port (and record it if Record is on).
    Send(Vec<u8>),
    SetPort(usize),
    /// Insert Mixer Parameter starting at this Track View track number (1-based).
    Insert(usize),
    Status(String),
}

enum Dialog {
    Setup { port: usize, ac: u8, monitor: bool },
    Chan { ch: usize, m: ChanMix, mute: bool, solo: bool },
    Master { v: u8 },
    Insert { track: usize },
    Voices { ch: usize, filter: String },
    Port,
}

pub struct Mixer {
    pub chans: [ChanMix; 16],
    pub mute: [bool; 16],
    pub solo: [bool; 16],
    pub master: u8,
    /// EQ knobs edit Hi (true) or Lo (false, the default).
    pub eq_hi: bool,
    /// Controller on the faders (Mixer Setup "Assignable Control").
    pub ac_cc: u8,
    pub play_monitor: bool,
    pub recording: bool,
    /// The mixer's output port (song port number, 0 = port 1), shown in the
    /// master unit; the `port` passed to `ui` is that port's device.
    pub port_no: u8,
    sel: Ctl,
    /// Nothing is highlighted until a control is clicked or keys are used.
    sel_on: bool,
    /// Strip control to return to with Tab from the Master fader.
    last_sel: Option<Ctl>,
    pgm_edit: Option<(usize, String)>,
    meters: [f32; 16],
    drag: Option<(Ctl, f32, u8)>,
    dialog: Option<Dialog>,
    // play monitor
    events: Vec<(u32, Vec<u8>)>,
    notes: Vec<(u32, u8, u8)>, // tick, channel, velocity
    tracker: ChanTracker,
    mon_tick: f32,
    ev_pos: usize,
    note_pos: usize,
    // artwork
    sprites: Option<HashMap<u32, egui::TextureHandle>>,
    sprites_tried: bool,
    exe_path: std::path::PathBuf,
}

impl Mixer {
    pub fn new(exe_path: std::path::PathBuf) -> Self {
        Mixer {
            chans: [ChanMix::initial(7); 16],
            mute: [false; 16],
            solo: [false; 16],
            master: 127,
            eq_hi: false,
            ac_cc: 7,
            play_monitor: true,
            recording: false,
            sel: Ctl::Pgm(0),
            sel_on: false,
            last_sel: None,
            pgm_edit: None,
            meters: [0.0; 16],
            drag: None,
            dialog: None,
            events: Vec::new(),
            notes: Vec::new(),
            tracker: ChanTracker::default(),
            mon_tick: -1.0,
            ev_pos: 0,
            note_pos: 0,
            sprites: None,
            sprites_tried: false,
            exe_path,
            port_no: 0,
        }
    }

    /// New song: strips as the song sets them up, events for the Play Monitor.
    pub fn load_song(&mut self, tracks: &[EditTrack]) {
        self.chans = mixer::song_chan_mix(tracks, self.ac_cc);
        self.song_events(tracks);
        self.mute = [false; 16];
        self.solo = [false; 16];
        self.recording = false;
    }

    /// The song was edited: refresh what the Play Monitor and meters follow.
    pub fn song_events(&mut self, tracks: &[EditTrack]) {
        self.events = mixer::song_chan_events(tracks);
        let mut notes = Vec::new();
        for t in tracks {
            let ch = t.channel & 0x0F;
            notes.extend(t.notes.iter().map(|n| (n.tick, ch, n.vel)));
            for e in &t.others {
                if let Event::Note { t, ch, vel, .. } = e {
                    notes.push((*t, ch & 0x0F, *vel));
                }
            }
        }
        notes.sort_by_key(|n| n.0);
        self.notes = notes;
        self.mon_tick = -1.0;
    }

    /// Whether a channel sounds (mixer mute / solo).
    pub fn chan_audible(&self) -> [bool; 16] {
        let any = self.solo.iter().any(|&s| s);
        std::array::from_fn(|c| if any { self.solo[c] } else { !self.mute[c] })
    }

    /// Advance the Play Monitor and level meters to `play_tick`.
    pub fn follow(&mut self, play_tick: Option<f32>, dt: f32) {
        for m in &mut self.meters {
            *m = (*m - dt * 40.0).max(0.0); // falls ~full scale in 0.4 s
        }
        let Some(now) = play_tick else {
            self.mon_tick = -1.0;
            return;
        };
        if now < self.mon_tick || self.mon_tick < 0.0 {
            // (re)started: replay from the top so the strips match
            self.tracker = ChanTracker::default();
            self.ev_pos = 0;
            self.note_pos = self.notes.partition_point(|n| (n.0 as f32) < now - 1.0);
        }
        while self.ev_pos < self.events.len() && self.events[self.ev_pos].0 as f32 <= now {
            if self.play_monitor {
                let data = self.events[self.ev_pos].1.clone();
                self.tracker.apply(&mut self.chans, self.ac_cc, &data);
            }
            self.ev_pos += 1;
        }
        let aud = self.chan_audible();
        while self.note_pos < self.notes.len() && self.notes[self.note_pos].0 as f32 <= now {
            let (_, ch, vel) = self.notes[self.note_pos];
            if aud[ch as usize] {
                let lvl = vel as f32 / 127.0 * 16.0;
                self.meters[ch as usize] = self.meters[ch as usize].max(lvl);
            }
            self.note_pos += 1;
        }
        self.mon_tick = now;
    }

    fn load_sprites(&mut self, ctx: &egui::Context) {
        self.sprites_tried = true;
        let Some(exe) = crate::platform::read(&self.exe_path) else { return };
        let bms = peres::load_bitmaps(&exe, |id| (3200..=3299).contains(&id) || id == BM_TOOLS);
        if !bms.contains_key(&BM_PANEL) {
            return;
        }
        let mut map = HashMap::new();
        for (id, mut b) in bms {
            if id == BM_TOOLS {
                peres::face_to_alpha(&mut b);
            }
            let img = egui::ColorImage::from_rgba_unmultiplied([b.w, b.h], &b.rgba);
            map.insert(id, ctx.load_texture(format!("mix{id}"), img, egui::TextureOptions::NEAREST));
        }
        self.sprites = Some(map);
    }

    /// Draw sprite `id` with its top-left at panel coordinate `at`; false if missing.
    fn spr(&self, p: &egui::Painter, o: Pos2, id: u32, at: (f32, f32)) -> bool {
        self.spr_uv(p, o, id, at, None)
    }
    fn spr_uv(&self, p: &egui::Painter, o: Pos2, id: u32, at: (f32, f32), part: Option<Rect>) -> bool {
        let Some(t) = self.sprites.as_ref().and_then(|s| s.get(&id)) else { return false };
        let [w, h] = t.size();
        let src = part.unwrap_or(Rect::from_min_size(Pos2::ZERO, vec2(w as f32, h as f32)));
        let uv = Rect::from_min_max(pos2(src.min.x / w as f32, src.min.y / h as f32), pos2(src.max.x / w as f32, src.max.y / h as f32));
        p.image(t.id(), Rect::from_min_size(o + vec2(at.0, at.1), src.size()), uv, Color32::WHITE);
        true
    }

    fn value(&self, c: Ctl) -> u8 {
        match c {
            Ctl::Pgm(ch) => self.chans[ch].program,
            Ctl::Eq(ch) => if self.eq_hi { self.chans[ch].eq_hi } else { self.chans[ch].eq_lo },
            Ctl::Rev(ch) => self.chans[ch].rev,
            Ctl::Cho(ch) => self.chans[ch].cho,
            Ctl::Var(ch) => self.chans[ch].var,
            Ctl::Pan(ch) => self.chans[ch].pan,
            Ctl::Fader(ch) => self.chans[ch].ac,
            Ctl::Master => self.master,
        }
    }

    /// Set a control and emit what it sends.
    fn set(&mut self, c: Ctl, v: u8, out: &mut Vec<MixAction>) {
        let v = v.min(127);
        if self.value(c) == v {
            return;
        }
        let ac = self.ac_cc;
        let send = |out: &mut Vec<MixAction>, b: Vec<u8>| out.push(MixAction::Send(b));
        match c {
            Ctl::Master => {
                self.master = v;
                send(out, mixer::universal_master_volume(v));
            }
            Ctl::Pgm(ch) => {
                self.chans[ch].program = v;
                send(out, vec![0xC0 | ch as u8, v]);
            }
            Ctl::Eq(ch) => {
                let lsb = if self.eq_hi { NRPN_EQ_HI } else { NRPN_EQ_LO };
                if self.eq_hi { self.chans[ch].eq_hi = v } else { self.chans[ch].eq_lo = v }
                for b in mixer::nrpn(ch as u8, 0x01, lsb, v) {
                    send(out, b.to_vec());
                }
            }
            Ctl::Rev(ch) | Ctl::Cho(ch) | Ctl::Var(ch) | Ctl::Pan(ch) | Ctl::Fader(ch) => {
                let m = &mut self.chans[ch];
                let cc = match c {
                    Ctl::Rev(_) => { m.rev = v; 91 }
                    Ctl::Cho(_) => { m.cho = v; 93 }
                    Ctl::Var(_) => { m.var = v; 94 }
                    Ctl::Pan(_) => { m.pan = v; 10 }
                    _ => { m.ac = v; ac }
                };
                send(out, vec![0xB0 | ch as u8, cc, v]);
            }
        }
    }

    /// Send a whole strip (Transmit, Properties, voice change).
    fn send_chan(&self, ch: usize, out: &mut Vec<MixAction>) {
        for b in mixer::chan_messages(ch as u8, &self.chans[ch], self.ac_cc) {
            out.push(MixAction::Send(b));
        }
    }

    fn describe(&self, c: Ctl, voice: &dyn Fn(u8, u8, u8) -> Option<String>) -> String {
        let v = self.value(c) as i32;
        let (name, val) = match c {
            Ctl::Pgm(_) => ("Program", format!("{}", v + 1)),
            Ctl::Eq(_) => (if self.eq_hi { "EQ Gain Hi" } else { "EQ Gain Lo" }, format!("{}", v - 64)),
            Ctl::Rev(_) => ("Reverb", v.to_string()),
            Ctl::Cho(_) => ("Chorus", v.to_string()),
            Ctl::Var(_) => ("Variation", v.to_string()),
            Ctl::Pan(_) => ("Pan", format!("{}", v - 64)),
            Ctl::Fader(_) => (AC_NAMES[self.ac_cc as usize], v.to_string()),
            Ctl::Master => ("Master Volume", v.to_string()),
        };
        match c.ch() {
            Some(ch) => {
                let m = &self.chans[ch];
                let vn = voice(m.bank_msb, m.bank_lsb, m.program).unwrap_or_default();
                format!("Ch.{} {name}   {val}   {vn}", ch + 1)
            }
            None => format!("{name}   {val}"),
        }
    }

    fn ctl_rect(c: Ctl) -> Rect {
        let sx = |ch: usize| ch as f32 * PITCH;
        let r = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(pos2(x, y), vec2(w, h));
        match c {
            Ctl::Pgm(ch) => r(sx(ch) + PGM_BOX.0, PGM_BOX.1, PGM_BOX.2, PGM_BOX.3),
            Ctl::Eq(ch) => r(sx(ch) + 2.0, KNOB_Y[0], 32.0, 29.0),
            Ctl::Rev(ch) => r(sx(ch) + 2.0, KNOB_Y[1], 32.0, 29.0),
            Ctl::Cho(ch) => r(sx(ch) + 2.0, KNOB_Y[2], 32.0, 29.0),
            Ctl::Var(ch) => r(sx(ch) + 2.0, KNOB_Y[3], 32.0, 29.0),
            Ctl::Pan(ch) => r(sx(ch) + 2.0, PAN_Y, 32.0, 32.0),
            Ctl::Fader(ch) => r(sx(ch) + FADER.0, FADER.1, 18.0, 84.0),
            Ctl::Master => r(MASTER_X, FADER.1, 18.0, 84.0),
        }
    }

    /// The whole window body. `ports` = output names, `port` = selected one.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ports: &[String],
        port: usize,
        voices: &[Voice],
        voice: &dyn Fn(u8, u8, u8) -> Option<String>,
        playing: bool,
    ) -> Vec<MixAction> {
        let ctx = ui.ctx().clone();
        if !self.sprites_tried {
            self.load_sprites(&ctx);
        }
        let mut out = Vec::new();
        let (resp, p) = ui.allocate_painter(vec2(CLIENT[0], CLIENT[1]), Sense::hover());
        let o0 = resp.rect.min;
        p.rect_filled(resp.rect, 0.0, C_FACE);
        self.toolbar(ui, &p, o0, playing, ports, port, &mut out);

        let o = o0 + PANEL.to_vec2();
        if !self.spr(&p, o, BM_PANEL, (0.0, 0.0)) {
            self.fallback_panel(&p, o);
        }

        // ---- channel strips ----
        for ch in 0..16 {
            let sx = ch as f32 * PITCH;
            // level meter
            let lvl = self.meters[ch].round().clamp(0.0, 16.0) as u32;
            if lvl > 0 && !self.spr(&p, o, BM_METER + lvl, (sx + METER.0, METER.1)) {
                let r = Rect::from_min_size(o + vec2(sx + METER.0, METER.1), vec2(8.0, 53.0));
                p.rect_filled(r, 0.0, Color32::BLACK);
                let h = 53.0 * lvl as f32 / 16.0;
                p.rect_filled(Rect::from_min_max(pos2(r.min.x, r.max.y - h), r.max), 0.0, Color32::from_rgb(0, 220, 0));
            }
            // Pgm
            let pgm_sel = self.is_sel(Ctl::Pgm(ch));
            if !self.spr(&p, o, BM_PGM + pgm_sel as u32, (sx + PGM_LABEL.0, PGM_LABEL.1)) {
                p.text(o + vec2(sx + 3.0, PGM_LABEL.1 - 1.0), egui::Align2::LEFT_TOP, "Pgm", egui::FontId::proportional(9.0),
                    if pgm_sel { C_EQ_PTR } else { Color32::WHITE });
            }
            let br = Self::ctl_rect(Ctl::Pgm(ch)).translate(o.to_vec2());
            p.rect_filled(br, 0.0, Color32::WHITE);
            if self.pgm_edit.as_ref().map_or(true, |e| e.0 != ch) {
                pixel_text(&p, br.left_center() + vec2(3.0, 0.5), &format!("{}", self.chans[ch].program as u32 + 1));
            }
            // knobs
            for (k, (bm, col)) in [(BM_EQ, C_EQ_PTR), (BM_REV, C_FX_PTR), (BM_CHO, C_FX_PTR), (BM_VAR, C_FX_PTR)].into_iter().enumerate() {
                let c = Ctl::at(k + 1, ch);
                self.knob(&p, o, bm, (sx + 2.0, KNOB_Y[k]), (16.0, 18.0), self.value(c), col, self.is_sel(c));
            }
            // mute / solo
            let (mb, sb) = if self.solo[ch] {
                (BM_MUTE_SOLO, BM_SOLO_ON)
            } else if self.mute[ch] {
                (BM_MUTE_ON, BM_SOLO_OFF)
            } else {
                (BM_MUTE_OFF, BM_SOLO_OFF)
            };
            if !self.spr(&p, o, mb, (sx + MUTE.0, MUTE.1)) {
                let r = Rect::from_min_size(o + vec2(sx + MUTE.0, MUTE.1), vec2(16.0, 17.0));
                p.rect_filled(r, 1.0, match mb { BM_MUTE_ON => Color32::from_gray(40), BM_MUTE_SOLO => Color32::from_rgb(255, 160, 160), _ => Color32::from_gray(200) });
                p.text(r.center(), egui::Align2::CENTER_CENTER, "M", egui::FontId::proportional(9.0), Color32::BLACK);
            }
            if !self.spr(&p, o, sb, (sx + SOLO.0, SOLO.1)) {
                let r = Rect::from_min_size(o + vec2(sx + SOLO.0, SOLO.1), vec2(9.0, 17.0));
                p.rect_filled(r, 1.0, if sb == BM_SOLO_ON { Color32::from_rgb(255, 160, 160) } else { Color32::from_gray(200) });
                p.text(r.center(), egui::Align2::CENTER_CENTER, "S", egui::FontId::proportional(8.0), Color32::BLACK);
            }
            // pan
            let c = Ctl::Pan(ch);
            self.knob(&p, o, BM_PAN, (sx + 2.0, PAN_Y), (16.0, 20.0), self.value(c), C_PAN_PTR, self.is_sel(c));
            // fader (grey when muted; a solo elsewhere does not grey it, as in the original)
            let muted = self.mute[ch] && !self.solo[ch];
            let c = Ctl::Fader(ch);
            let track = if muted { BM_FADER_MUTED } else { BM_FADER } + ch as u32;
            let cap = if muted { BM_CAP_MUTED } else if self.is_sel(c) { BM_CAP_SEL } else { BM_CAP };
            self.fader(&p, o, track, cap, sx + FADER.0, self.value(c), Some(ch));
        }
        // ---- master unit ----
        let cap = if self.is_sel(Ctl::Master) { BM_CAP_SEL } else { BM_CAP };
        self.fader(&p, o, BM_MASTER, cap, MASTER_X, self.master, None);
        let pr = Rect::from_min_size(o + vec2(PORT_BOX.0, PORT_BOX.1), vec2(PORT_BOX.2, PORT_BOX.3));
        p.rect_filled(pr.shrink(1.0), 0.0, Color32::WHITE);
        pixel_text(&p, pr.left_center() + vec2(6.0, 0.5), &format!("{}", self.port_no + 1));
        if !self.spr(&p, o, if self.eq_hi { BM_HI } else { BM_LO }, HILO) {
            for (k, on) in [(0.0, self.eq_hi), (12.0, !self.eq_hi)] {
                let r = Rect::from_min_size(o + vec2(HILO.0, HILO.1 + k), vec2(14.0, 10.0));
                p.rect_filled(r, 0.0, if on { Color32::from_rgb(150, 210, 210) } else { Color32::from_gray(70) });
                p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::BLACK));
            }
        }

        self.interact(ui, o, &mut out, voice, ports, port);
        self.keyboard(&ctx, &mut out, voice);
        self.dialogs(&ctx, &mut out, ports, port, voices, voice);
        out
    }

    fn knob(&self, p: &egui::Painter, o: Pos2, bm: u32, at: (f32, f32), c: (f32, f32), v: u8, col: Color32, sel: bool) {
        if !self.spr(p, o, bm + sel as u32, at) {
            let ctr = o + vec2(at.0 + c.0, at.1 + c.1);
            p.circle_filled(ctr + vec2(1.5, 1.5), 7.0, Color32::from_gray(50));
            p.circle_filled(ctr, 7.0, Color32::BLACK);
            p.circle_stroke(ctr, 7.5, Stroke::new(1.5_f32, Color32::WHITE));
            let lbl = match bm { BM_EQ => "Eq", BM_REV => "Rev", BM_CHO => "Cho", BM_VAR => "Var", _ => "Pan" };
            let tc = if sel { C_EQ_PTR } else if bm == BM_PAN { Color32::from_rgb(255, 153, 153) } else { Color32::from_rgb(204, 204, 255) };
            p.text(o + vec2(at.0 + 1.0, at.1 - 1.0), egui::Align2::LEFT_TOP, lbl, egui::FontId::proportional(9.0), tc);
        }
        // pointer: 7 px from the centre, -135°..+135° over 0..127 (measured on the original)
        let a = (-135.0 + 270.0 * v as f32 / 127.0).to_radians();
        let (x0, y0) = (at.0 + c.0, at.1 + c.1);
        let (x1, y1) = ((x0 + 7.0 * a.sin()).round(), (y0 - 7.0 * a.cos()).round());
        line_px(p, o, (x0 as i32, y0 as i32), (x1 as i32, y1 as i32), col);
    }

    fn fader(&self, p: &egui::Painter, o: Pos2, track: u32, cap: u32, x: f32, v: u8, ch: Option<usize>) {
        if !self.spr(p, o, track, (x, FADER.1)) {
            let r = Rect::from_min_size(o + vec2(x, FADER.1), vec2(18.0, 84.0));
            let fill = match track {
                BM_MASTER => Color32::from_rgb(255, 153, 153),
                t if t >= BM_FADER_MUTED => Color32::from_gray(100),
                _ => Color32::from_rgb(153, 153, 204),
            };
            p.rect_filled(r, 0.0, fill);
            if let Some(ch) = ch {
                p.text(r.center_bottom() - vec2(0.0, 2.0), egui::Align2::CENTER_BOTTOM, format!("{}", ch + 1),
                    egui::FontId::proportional(11.0), Color32::from_rgb(220, 220, 255));
            }
        }
        let y = FADER.1 + ((127 - v as u32) * 70 / 127) as f32;
        if !self.spr(p, o, cap, (x, y)) {
            let r = Rect::from_min_size(o + vec2(x, y), vec2(18.0, 14.0));
            p.rect_filled(r, 1.0, if cap == BM_CAP_SEL { Color32::from_rgb(120, 230, 230) } else { Color32::from_gray(200) });
            p.hline(r.x_range(), r.center().y, Stroke::new(2.0_f32, Color32::RED));
        }
    }

    fn fallback_panel(&self, p: &egui::Painter, o: Pos2) {
        p.rect_filled(Rect::from_min_size(o, vec2(630.0, 357.0)), 0.0, C_PANEL);
        for ch in 0..=16 {
            let x = o.x + ch as f32 * PITCH - 1.0;
            p.vline(x, o.y..=o.y + 357.0, Stroke::new(2.0_f32, Color32::from_gray(30)));
        }
        let eq = Rect::from_min_size(o + vec2(0.0, KNOB_Y[0] + 6.0), vec2(592.0, 23.0));
        p.rect_filled(eq, 0.0, Color32::from_gray(120));
        p.text(o + vec2(603.0, 22.0), egui::Align2::LEFT_TOP, "Port", egui::FontId::proportional(9.0), Color32::WHITE);
        p.text(o + vec2(612.0, 96.0), egui::Align2::LEFT_CENTER, "Hi", egui::FontId::proportional(9.0), Color32::from_rgb(204, 255, 204));
        p.text(o + vec2(612.0, 108.0), egui::Align2::LEFT_CENTER, "Lo", egui::FontId::proportional(9.0), Color32::from_rgb(204, 255, 204));
        p.text(o + vec2(611.0, 240.0), egui::Align2::CENTER_CENTER, "Master\nFader", egui::FontId::proportional(9.0), Color32::from_gray(200));
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, p: &egui::Painter, o: Pos2, playing: bool, ports: &[String], port: usize, out: &mut Vec<MixAction>) {
        let tips = ["Mixer Setup(F2)", "Transmit Mixer Parameter(F3)", "Insert Mixer Parameter(F4)", "Record Mixer Parameter(F5)"];
        for (k, x) in TOOL_X.into_iter().enumerate() {
            let r = Rect::from_min_size(o + vec2(x, 2.0), vec2(22.0, 22.0));
            let resp = ui.interact(r, ui.id().with(("tool", k)), Sense::click()).on_hover_text(tips[k]);
            let down = resp.is_pointer_button_down_on() || (k == 3 && self.recording);
            let (tl, br) = if down { (Color32::from_gray(100), Color32::WHITE) } else { (Color32::WHITE, Color32::from_gray(100)) };
            p.rect_filled(r, 0.0, if k == 3 && self.recording { Color32::from_gray(225) } else { C_FACE });
            p.hline(r.x_range(), r.top(), Stroke::new(1.0_f32, tl));
            p.vline(r.left(), r.y_range(), Stroke::new(1.0_f32, tl));
            p.hline(r.x_range(), r.bottom() - 1.0, Stroke::new(1.0_f32, br));
            p.vline(r.right() - 1.0, r.y_range(), Stroke::new(1.0_f32, br));
            let at = (x + 3.0 + down as u8 as f32 - PANEL.x, 5.0 + down as u8 as f32 - PANEL.y);
            let part = Rect::from_min_size(pos2(16.0 * k as f32, 0.0), vec2(16.0, 15.0));
            if !self.spr_uv(p, o + PANEL.to_vec2(), BM_TOOLS, at, Some(part)) {
                p.text(r.center(), egui::Align2::CENTER_CENTER, ["⚙", "⇪", "⤓", "⏺"][k], egui::FontId::proportional(13.0), Color32::BLACK);
            }
            let key = [Key::F2, Key::F3, Key::F4, Key::F5][k];
            if resp.clicked() || ui.ctx().input(|i| i.key_pressed(key)) {
                match k {
                    0 => self.dialog = Some(Dialog::Setup { port, ac: self.ac_cc, monitor: self.play_monitor }),
                    1 => {
                        out.push(MixAction::Send(mixer::universal_master_volume(self.master)));
                        for ch in 0..16 {
                            self.send_chan(ch, out);
                        }
                        out.push(MixAction::Status("Mixer parameters transmitted.".into()));
                    }
                    2 => self.dialog = Some(Dialog::Insert { track: 1 }),
                    _ => {
                        self.recording = !self.recording;
                        out.push(MixAction::Status(if self.recording {
                            if playing { "Recording mixer moves…" } else { "Record armed: mixer moves are recorded while the song plays." }
                        } else {
                            "Mixer recording off."
                        }.into()));
                    }
                }
            }
        }
        let _ = ports;
    }

    fn interact(&mut self, ui: &mut egui::Ui, o: Pos2, out: &mut Vec<MixAction>, voice: &dyn Fn(u8, u8, u8) -> Option<String>, ports: &[String], port: usize) {
        let mut ctls: Vec<Ctl> = (0..16).flat_map(|ch| (0..7).map(move |r| Ctl::at(r, ch))).collect();
        ctls.push(Ctl::Master);
        let mut hovered: Option<Ctl> = None;
        for c in ctls {
            let r = Self::ctl_rect(c).translate(o.to_vec2());
            let resp = ui.interact(r, ui.id().with(("ctl", format!("{c:?}"))), Sense::click_and_drag());
            if resp.hovered() {
                hovered = Some(c);
            }
            if resp.clicked() || resp.drag_started() {
                self.sel = c;
                self.sel_on = true;
                if !matches!(c, Ctl::Pgm(_)) {
                    self.pgm_edit = None;
                }
            }
            match c {
                Ctl::Pgm(ch) => {
                    if resp.double_clicked() {
                        self.pgm_edit = None;
                        self.dialog = Some(Dialog::Voices { ch, filter: String::new() });
                    } else if resp.clicked() && self.pgm_edit.is_none() {
                        self.pgm_edit = Some((ch, String::new()));
                    }
                }
                Ctl::Fader(_) | Ctl::Master => {
                    if resp.is_pointer_button_down_on() {
                        if let Some(pt) = resp.interact_pointer_pos() {
                            let y = (pt.y - r.top() - 7.0).clamp(0.0, 70.0);
                            let v = (127.0 - y * 127.0 / 70.0).round() as u8;
                            self.set(c, v, out);
                        }
                    }
                }
                _ => {
                    if resp.drag_started() {
                        self.drag = Some((c, 0.0, self.value(c)));
                    }
                    if let Some((dc, acc, v0)) = self.drag.as_mut() {
                        if *dc == c && resp.dragged() {
                            *acc -= resp.drag_delta().y;
                            let v = (*v0 as f32 + *acc).round().clamp(0.0, 127.0) as u8;
                            self.set(c, v, out);
                        }
                    }
                    if resp.drag_stopped() {
                        self.drag = None;
                    }
                }
            }
            if resp.hovered() {
                let wheel = ui.ctx().input(|i| i.raw_scroll_delta.y);
                if wheel != 0.0 {
                    let v = (self.value(c) as i32 + wheel.signum() as i32).clamp(0, 127) as u8;
                    self.set(c, v, out);
                }
            }
            // right click: channel / master popup (Mixer.hlp)
            resp.context_menu(|ui| match c.ch() {
                Some(ch) => {
                    if ui.button("Initialize Channel Data").clicked() {
                        self.chans[ch] = ChanMix::initial(self.ac_cc);
                        self.mute[ch] = false;
                        self.solo[ch] = false;
                        self.send_chan(ch, out);
                        ui.close_menu();
                    }
                    if ui.button("Properties").clicked() {
                        self.dialog = Some(Dialog::Chan { ch, m: self.chans[ch], mute: self.mute[ch], solo: self.solo[ch] });
                        ui.close_menu();
                    }
                }
                None => {
                    if ui.button("Initialize Master Channel").clicked() {
                        self.set(Ctl::Master, 127, out);
                        ui.close_menu();
                    }
                    if ui.button("Properties").clicked() {
                        self.dialog = Some(Dialog::Master { v: self.master });
                        ui.close_menu();
                    }
                }
            });
        }
        // Pgm typing box
        if let Some((ch, mut s)) = self.pgm_edit.take() {
            let r = Self::ctl_rect(Ctl::Pgm(ch)).translate(o.to_vec2());
            let te = ui.put(r, egui::TextEdit::singleline(&mut s).font(egui::FontId::monospace(10.0)).frame(false)
                .text_color(Color32::BLACK).char_limit(3).margin(vec2(2.0, 0.0)));
            te.request_focus();
            let dbl = te.double_clicked();
            if dbl {
                // the second click of a double-click lands on the typing box
                self.dialog = Some(Dialog::Voices { ch, filter: String::new() });
            }
            let enter = ui.ctx().input(|i| i.key_pressed(Key::Enter));
            let esc = ui.ctx().input(|i| i.key_pressed(Key::Escape));
            s.retain(|c| c.is_ascii_digit());
            if enter {
                if let Ok(n) = s.parse::<u32>() {
                    if (1..=128).contains(&n) {
                        let m = &mut self.chans[ch];
                        m.program = (n - 1) as u8;
                        let (msb, lsb, prg) = (m.bank_msb, m.bank_lsb, m.program);
                        // same bank, new program
                        out.push(MixAction::Send(vec![0xB0 | ch as u8, 0, msb]));
                        out.push(MixAction::Send(vec![0xB0 | ch as u8, 32, lsb]));
                        out.push(MixAction::Send(vec![0xC0 | ch as u8, prg]));
                    }
                }
            } else if !esc && !dbl && !te.lost_focus() {
                self.pgm_edit = Some((ch, s));
            }
        }
        // mute / solo / Hi-Lo / Port
        for ch in 0..16 {
            let sx = ch as f32 * PITCH;
            let mr = Rect::from_min_size(o + vec2(sx + MUTE.0, MUTE.1), vec2(16.0, 17.0));
            let sr = Rect::from_min_size(o + vec2(sx + SOLO.0, SOLO.1), vec2(9.0, 17.0));
            let m = ui.interact(mr, ui.id().with(("mute", ch)), Sense::click());
            let s = ui.interact(sr, ui.id().with(("solo", ch)), Sense::click());
            if m.clicked() {
                self.mute[ch] = !self.mute[ch];
            }
            if s.clicked() {
                self.solo[ch] = !self.solo[ch];
            }
            if m.hovered() {
                out.push(MixAction::Status(format!("Ch.{} Mute   {}", ch + 1, if self.mute[ch] { "ON" } else { "OFF" })));
            }
            if s.hovered() {
                out.push(MixAction::Status(format!("Ch.{} Solo   {}", ch + 1, if self.solo[ch] { "ON" } else { "OFF" })));
            }
        }
        let hl = ui.interact(Rect::from_min_size(o + vec2(HILO.0, HILO.1), vec2(14.0, 22.0)), ui.id().with("hilo"), Sense::click());
        if hl.clicked() {
            if let Some(pt) = hl.interact_pointer_pos() {
                self.eq_hi = pt.y < o.y + HILO.1 + 11.0;
            }
        }
        if hl.hovered() {
            out.push(MixAction::Status(format!("EQ Gain Select   {}", if self.eq_hi { "Hi" } else { "Lo" })));
        }
        let pr = Rect::from_min_size(o + vec2(PORT_BOX.0, PORT_BOX.1), vec2(PORT_BOX.2, PORT_BOX.3));
        let pp = ui.interact(pr, ui.id().with("port"), Sense::click());
        if pp.double_clicked() {
            self.dialog = Some(Dialog::Port);
        }
        if pp.hovered() {
            out.push(MixAction::Status(format!("Port {}   {}", self.port_no + 1, ports.get(port).cloned().unwrap_or_default())));
        }
        if let Some(c) = hovered.or(self.drag.map(|d| d.0)) {
            out.push(MixAction::Status(self.describe(c, voice)));
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context, out: &mut Vec<MixAction>, voice: &dyn Fn(u8, u8, u8) -> Option<String>) {
        if self.dialog.is_some() || self.pgm_edit.is_some() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let keys = ctx.input(|i| {
            [Key::ArrowLeft, Key::ArrowRight, Key::ArrowUp, Key::ArrowDown, Key::PageUp, Key::PageDown, Key::Home, Key::End, Key::Tab]
                .map(|k| i.key_pressed(k))
        });
        if !keys.iter().any(|&k| k) {
            return;
        }
        if !self.sel_on {
            self.sel_on = true; // first key just shows the highlight
            return;
        }
        let (row, col) = (self.sel.row(), self.sel.ch().unwrap_or(16));
        let mut nc = self.sel;
        if keys[0] && col > 0 {
            nc = if col == 16 { Ctl::Fader(15) } else { Ctl::at(row, col - 1) };
        }
        if keys[1] {
            nc = if col == 15 { Ctl::Master } else if col < 15 { Ctl::at(row, col + 1) } else { nc };
        }
        if keys[2] && row > 0 && col < 16 {
            nc = Ctl::at(row - 1, col);
        }
        if keys[3] && row < 6 && col < 16 {
            nc = Ctl::at(row + 1, col);
        }
        if keys[6] {
            nc = Ctl::Pgm(0);
        }
        if keys[7] {
            nc = Ctl::Master;
        }
        if keys[8] {
            nc = if col == 16 { self.last_strip_sel() } else { Ctl::Master };
        }
        if nc != self.sel {
            if col < 16 {
                self.last_sel = Some(self.sel);
            }
            self.sel = nc;
        }
        let v = self.value(self.sel) as i32;
        if keys[4] {
            self.set(self.sel, (v + 1).min(127) as u8, out);
        }
        if keys[5] {
            self.set(self.sel, (v - 1).max(0) as u8, out);
        }
        out.push(MixAction::Status(self.describe(self.sel, voice)));
    }

    fn is_sel(&self, c: Ctl) -> bool {
        self.sel_on && self.sel == c
    }

    fn last_strip_sel(&self) -> Ctl {
        self.last_sel.unwrap_or(Ctl::Pgm(0))
    }

    fn dialogs(&mut self, ctx: &egui::Context, out: &mut Vec<MixAction>, ports: &[String], port: usize, voices: &[Voice], voice: &dyn Fn(u8, u8, u8) -> Option<String>) {
        let Some(mut d) = self.dialog.take() else { return };
        let mut keep = true;
        let win = |t: &str| egui::Window::new(egui::RichText::new(t).size(13.0)).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
        match &mut d {
            Dialog::Setup { port: sp, ac, monitor } => {
                win("Mixer Setup").show(ctx, |ui| {
                    ui.group(|ui| {
                        ui.label("MIDI Output Port");
                        egui::ComboBox::from_id_source("mx_port").width(170.0)
                            .selected_text(ports.get(*sp).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (i, n) in ports.iter().enumerate() {
                                    ui.selectable_value(sp, i, n);
                                }
                            });
                    });
                    ui.group(|ui| {
                        ui.label("Assignable Control");
                        egui::ComboBox::from_id_source("mx_ac").width(170.0)
                            .selected_text(AC_NAMES[*ac as usize])
                            .show_ui(ui, |ui| {
                                for (i, n) in AC_NAMES.iter().enumerate() {
                                    ui.selectable_value(ac, i as u8, *n);
                                }
                            });
                    });
                    ui.checkbox(monitor, "Play Monitor");
                    ui.horizontal(|ui| {
                        if ui.button("   OK   ").clicked() {
                            if *sp != port {
                                out.push(MixAction::SetPort(*sp));
                            }
                            if *ac != self.ac_cc {
                                self.ac_cc = *ac;
                                for m in &mut self.chans {
                                    m.ac = mixer::ini_default(*ac);
                                }
                            }
                            self.play_monitor = *monitor;
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
            }
            Dialog::Chan { ch, m, mute, solo } => {
                let ch = *ch;
                let mut apply = false;
                win("Channel Properties").show(ctx, |ui| {
                    egui::Grid::new("chprop").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label("Ch.No");
                        ui.label(format!("{}", ch + 1));
                        ui.checkbox(mute, "Mute");
                        ui.checkbox(solo, "Solo");
                        ui.end_row();
                        ui.label("Program");
                        ui.label(voice(m.bank_msb, m.bank_lsb, m.program).unwrap_or_default());
                        ui.end_row();
                        ui.label("");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut m.bank_msb).range(0..=127u8).prefix("MSB "));
                            ui.add(egui::DragValue::new(&mut m.bank_lsb).range(0..=127u8).prefix("LSB "));
                            let mut pn = m.program as u32 + 1;
                            ui.add(egui::DragValue::new(&mut pn).range(1..=128u32).prefix("Program No. "));
                            m.program = (pn - 1) as u8;
                        });
                        ui.end_row();
                        ui.label("EQ Gain");
                        ui.horizontal(|ui| {
                            signed(ui, &mut m.eq_hi, "Hi ");
                            signed(ui, &mut m.eq_lo, "Lo ");
                        });
                        ui.end_row();
                        ui.label("Reverb");
                        ui.add(egui::DragValue::new(&mut m.rev).range(0..=127u8));
                        ui.label("Pan");
                        signed(ui, &mut m.pan, "");
                        ui.end_row();
                        ui.label("Chorus");
                        ui.add(egui::DragValue::new(&mut m.cho).range(0..=127u8));
                        ui.label("AC");
                        ui.add(egui::DragValue::new(&mut m.ac).range(0..=127u8));
                        ui.end_row();
                        ui.label("Variation");
                        ui.add(egui::DragValue::new(&mut m.var).range(0..=127u8));
                        ui.end_row();
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            apply = true;
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if ui.button("Apply").clicked() {
                            apply = true;
                        }
                    });
                });
                if apply {
                    self.chans[ch] = *m;
                    self.mute[ch] = *mute;
                    self.solo[ch] = *solo;
                    self.send_chan(ch, out);
                }
            }
            Dialog::Master { v } => {
                let mut apply = false;
                win("Master Channel Properties").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Master Volume");
                        ui.add(egui::DragValue::new(v).range(0..=127u8));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            apply = true;
                            keep = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if ui.button("Apply").clicked() {
                            apply = true;
                        }
                    });
                });
                if apply {
                    self.set(Ctl::Master, *v, out);
                }
            }
            Dialog::Insert { track } => {
                win("Insert Setup").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Track No.");
                        ui.add(egui::DragValue::new(track).range(1..=100usize));
                        ui.vertical(|ui| {
                            if ui.button("   OK   ").clicked() {
                                out.push(MixAction::Insert(*track));
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() {
                                keep = false;
                            }
                        });
                    });
                    ui.weak(format!("Ch.1 = track {}, Ch.2 = track {} ...", *track, *track + 1));
                });
            }
            Dialog::Voices { ch, filter } => {
                let ch = *ch;
                let cur = self.chans[ch];
                let mut pick: Option<(u8, u8, u8)> = None;
                win(&format!("Voice List — Ch.{}", ch + 1)).show(ctx, |ui| {
                    if voices.is_empty() {
                        ui.label("No Voice List file or section is found");
                    }
                    ui.horizontal(|ui| {
                        ui.label("🔍");
                        ui.text_edit_singleline(filter);
                    });
                    let f = filter.to_lowercase();
                    egui::ScrollArea::vertical().max_height(230.0).show(ui, |ui| {
                        let mut cat = "";
                        for v in voices.iter().filter(|v| f.is_empty() || v.name.to_lowercase().contains(&f) || v.category.to_lowercase().contains(&f)) {
                            if v.category != cat {
                                cat = &v.category;
                                ui.strong(cat);
                            }
                            let on = (v.msb, v.lsb, v.program) == (cur.bank_msb, cur.bank_lsb, cur.program);
                            let r = ui.selectable_label(on, format!("{:3}  {}   ({}/{})", v.program as u32 + 1, v.name, v.msb, v.lsb));
                            if r.clicked() {
                                pick = Some((v.msb, v.lsb, v.program));
                            }
                        }
                    });
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                });
                if let Some((msb, lsb, prg)) = pick {
                    let m = &mut self.chans[ch];
                    (m.bank_msb, m.bank_lsb, m.program) = (msb, lsb, prg);
                    out.push(MixAction::Send(vec![0xB0 | ch as u8, 0, msb]));
                    out.push(MixAction::Send(vec![0xB0 | ch as u8, 32, lsb]));
                    out.push(MixAction::Send(vec![0xC0 | ch as u8, prg]));
                }
            }
            Dialog::Port => {
                win("Port").show(ctx, |ui| {
                    for (i, n) in ports.iter().enumerate() {
                        if ui.selectable_label(i == port, n).clicked() {
                            out.push(MixAction::SetPort(i));
                            keep = false;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            }
        }
        if keep && self.dialog.is_none() {
            self.dialog = Some(d);
        }
    }
}

/// Raw 0..127 (64 = centre) edited as -64..63, like the original's spin boxes.
fn signed(ui: &mut egui::Ui, v: &mut u8, prefix: &str) {
    let mut s = *v as i32 - 64;
    ui.add(egui::DragValue::new(&mut s).range(-64..=63i32).prefix(prefix));
    *v = (s + 64) as u8;
}

/// 1-px Bresenham line in panel pixels (crisp like the original's knob pointers).
fn line_px(p: &egui::Painter, o: Pos2, a: (i32, i32), b: (i32, i32), col: Color32) {
    let (mut x, mut y) = a;
    let (dx, dy) = ((b.0 - x).abs(), -(b.1 - y).abs());
    let (sx, sy) = (if x < b.0 { 1 } else { -1 }, if y < b.1 { 1 } else { -1 });
    let mut err = dx + dy;
    loop {
        p.rect_filled(Rect::from_min_size(o + vec2(x as f32, y as f32), vec2(1.0, 1.0)), 0.0, col);
        if (x, y) == b {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Bold digits like the original's MS Sans Serif in the Pgm / Port boxes.
fn pixel_text(p: &egui::Painter, left_center: Pos2, s: &str) {
    let f = egui::FontId::proportional(11.0);
    for d in [0.0, 0.6] {
        p.text(left_center + vec2(d, 0.0), egui::Align2::LEFT_CENTER, s, f.clone(), Color32::BLACK);
    }
}
