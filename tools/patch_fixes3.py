def patch(p, pairs):
    s = open(p, encoding='utf-8').read()
    for a, b in pairs:
        assert s.count(a) == 1, (p, a[:70], s.count(a))
        s = s.replace(a, b)
    open(p, 'w', encoding='utf-8').write(s)


patch(r'F:\rea\xgworks-native\src\main.rs', [
    ('''        for (t, c) in &chords {
            let xx = x_of(*t);
            if xx >= chord_lane.left() - 40.0 && xx <= chord_lane.right() {
                cp.line_segment([egui::pos2(xx, chord_lane.top() + 4.0), egui::pos2(xx, chord_lane.bottom() - 4.0)], egui::Stroke::new(1.0_f32, Color32::BLACK));
                cp.text(egui::pos2(xx + 2.0, chord_lane.center().y), egui::Align2::LEFT_CENTER, c, egui::FontId::monospace(10.0), Color32::BLACK);
            }
        }''', '''        // labels only where they fit (the original thins them out when zoomed out)
        let mut free_x = f32::MIN;
        for (t, c) in &chords {
            let xx = x_of(*t);
            if xx >= chord_lane.left() - 40.0 && xx <= chord_lane.right() && xx >= free_x {
                cp.line_segment([egui::pos2(xx, chord_lane.top() + 4.0), egui::pos2(xx, chord_lane.bottom() - 4.0)], egui::Stroke::new(1.0_f32, Color32::BLACK));
                let r = cp.text(egui::pos2(xx + 2.0, chord_lane.center().y), egui::Align2::LEFT_CENTER, c, egui::FontId::monospace(10.0), Color32::BLACK);
                free_x = r.right() + 4.0;
            }
        }'''),
    ('''            let cx = pianoroll::RollCtx {
                ppqn: self.ppqn as u32,''', '''            let visible = self.visible_tracks();
            let cx = pianoroll::RollCtx {
                visible: &visible,
                ppqn: self.ppqn as u32,'''),
    ('''            "Loaded {name} — {} tracks, {} notes. Select a track and edit on the roll.",
            self.edit.len(),''', '''            "Loaded {name} — {} tracks, {} notes.",
            self.edit.iter().filter(|t| !t.conductor).count(),'''),
])

patch(r'F:\rea\xgworks-native\src\pianoroll.rs', [
    ('''pub struct RollCtx<'a> {
    pub ppqn: u32,''', '''pub struct RollCtx<'a> {
    /// Track indices listed in the selector (conductor hidden), display number = position + 1.
    pub visible: &'a [usize],
    pub ppqn: u32,'''),
    ('''            let name = cx.track_names.get(self.track).cloned().unwrap_or_default();
            egui::ComboBox::from_id_source(("roll_trk", self as *const _ as usize))
                .width(220.0)
                .selected_text(format!("{} {}", self.track + 1, name))
                .show_ui(ui, |ui| {
                    for (i, n) in cx.track_names.iter().enumerate() {
                        if ui.selectable_label(i == self.track, format!("{} {}", i + 1, n)).clicked() {''', '''            let name = cx.track_names.get(self.track).cloned().unwrap_or_default();
            let num = cx.visible.iter().position(|&v| v == self.track).map_or(0, |k| k + 1);
            egui::ComboBox::from_id_source(("roll_trk", self as *const _ as usize))
                .width(220.0)
                .selected_text(format!("{num} {name}"))
                .show_ui(ui, |ui| {
                    for (k, &i) in cx.visible.iter().enumerate() {
                        let n = &cx.track_names[i];
                        if ui.selectable_label(i == self.track, format!("{} {}", k + 1, n)).clicked() {'''),
])
print('ok')
