// SwiftUI panel of HS2 – Hypersynth 2 (shared by the app and the Audio Unit).

import SwiftUI
#if os(macOS)
import AppKit
#endif
import UniformTypeIdentifiers

enum HS2Style {
    static let bg = Color(red: 0.11, green: 0.12, blue: 0.14)
    static let panel = Color(red: 0.17, green: 0.18, blue: 0.21)
    static let lcd = Color(red: 0.55, green: 0.78, blue: 0.95)
    static let accent = Color(red: 0.36, green: 0.62, blue: 1.0)
}

public struct HS2PanelView: View {
    @ObservedObject var model: HS2Model
    @State private var showAbout = false

    public init(model: HS2Model) {
        self.model = model
    }

    public var body: some View {
        VStack(spacing: 8) {
            header
            HStack(alignment: .top, spacing: 8) {
                PartListView(model: model).frame(width: 230)
                PatchBrowserView(model: model)
                controls.frame(width: 210)
            }
            KeyboardView(low: 12 * model.octave, octaves: 4, noteOn: { model.noteOn($0) }, noteOff: { model.noteOff($0) })
                .frame(height: 90)
            footer
        }
        .padding(10)
        .background(HS2Style.bg)
        .foregroundColor(.white)
        .sheet(isPresented: $showAbout) { AboutView(text: model.versionText) }
    }

    private var header: some View {
        HStack(alignment: .center, spacing: 12) {
            Text("HS2").font(.system(size: 26, weight: .black, design: .rounded)).foregroundColor(HS2Style.accent)
            Text("HYPERSYNTH 2").font(.system(size: 15, weight: .semibold)).tracking(3)
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                Text("Part \(model.selectedPart + 1)").font(.caption).foregroundColor(.gray)
                Text(model.partNames[model.selectedPart].isEmpty ? "—" : model.partNames[model.selectedPart])
                    .font(.system(size: 17, weight: .medium, design: .monospaced)).foregroundColor(HS2Style.lcd)
            }
            .padding(.horizontal, 12).padding(.vertical, 4)
            .background(RoundedRectangle(cornerRadius: 6).fill(Color.black.opacity(0.6)))
            MeterView(left: model.meterL, right: model.meterR).frame(width: 16, height: 40)
            Text("\(model.voices)").font(.caption.monospacedDigit()).frame(width: 28)
        }
    }

    private var controls: some View {
        VStack(alignment: .leading, spacing: 10) {
            Group {
                Text("Master").font(.caption)
                Slider(value: $model.master, in: 0...1)
                Text("Output Hype").font(.caption)
                Slider(value: $model.hype, in: 0...1)
                Text("Master Tune \(String(format: "%+.2f", model.tune))").font(.caption)
                Slider(value: $model.tune, in: -1...1)
            }
            HStack {
                Button("◀︎") { model.step(-1) }.help("Previous patch")
                Button("▶︎") { model.step(1) }.help("Next patch")
                Spacer()
                Button("Panic") { model.panic() }
            }
            Stepper("Keyboard C\(model.octave - 2)", value: $model.octave, in: 1...7)
            Divider()
            if model.standalone {
                Button("Import HS2 content folder…") { pickFolder() }
                Button("SoundFont / WAV…") { pickSoundFont() }
            } else {
                Text("Content is imported in the HS2 app.").font(.caption).foregroundColor(.gray)
                Button("Reload content") { model.reloadContent() }
            }
            Spacer()
            Button("About…") { showAbout = true }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 6).fill(HS2Style.panel))
    }

    private var footer: some View {
        HStack {
            Text(model.status).font(.caption).foregroundColor(.gray).lineLimit(1)
            Spacer()
            Text("HS2 – Hypersynth 2 \(HS2Model.appVersion)").font(.caption2).foregroundColor(.gray)
        }
    }

    private func pickFolder() {
        #if os(macOS)
        let p = NSOpenPanel()
        p.canChooseDirectories = true
        p.canChooseFiles = false
        p.message = "Choose your Hypersonic 2 content folder (patch, phrase and tuning banks)"
        if p.runModal() == .OK, let u = p.url {
            model.importContent(from: u)
        }
        #endif
    }

    private func pickSoundFont() {
        #if os(macOS)
        let p = NSOpenPanel()
        p.allowedContentTypes = [UTType(filenameExtension: "sf2") ?? .data, .wav]
        p.message = "Choose a SoundFont (.sf2) or WAV for the Sample elements"
        if p.runModal() == .OK, let u = p.url {
            model.importSoundFont(from: u)
        }
        #endif
    }
}

struct PartListView: View {
    @ObservedObject var model: HS2Model

    var body: some View {
        ScrollView {
            VStack(spacing: 2) {
                ForEach(0..<16, id: \.self) { i in
                    HStack {
                        Text(String(format: "%2d", i + 1)).font(.system(.caption, design: .monospaced)).foregroundColor(.gray)
                        Text(model.partNames[i].isEmpty ? "—" : model.partNames[i]).lineLimit(1)
                        Spacer()
                    }
                    .padding(.horizontal, 6).padding(.vertical, 3)
                    .background(RoundedRectangle(cornerRadius: 4).fill(i == model.selectedPart ? HS2Style.accent.opacity(0.45) : Color.clear))
                    .contentShape(Rectangle())
                    .onTapGesture { model.selectPart(i) }
                }
            }
        }
        .padding(6)
        .background(RoundedRectangle(cornerRadius: 6).fill(HS2Style.panel))
    }
}

struct PatchBrowserView: View {
    @ObservedObject var model: HS2Model
    @State private var open: Set<Int> = []

    var body: some View {
        VStack(spacing: 6) {
            TextField("Search patches", text: $model.search).textFieldStyle(.roundedBorder)
            List {
                if !model.combis.isEmpty && model.search.isEmpty {
                    DisclosureGroup("Combis (\(model.combis.count))") {
                        ForEach(Array(model.combis.enumerated()), id: \.offset) { i, name in
                            Text(name).contentShape(Rectangle()).onTapGesture(count: 1) { model.loadCombi(i) }
                        }
                    }
                }
                ForEach(model.filteredBanks) { bank in
                    DisclosureGroup(isExpanded: Binding(
                        get: { open.contains(bank.id) || !model.search.isEmpty },
                        set: { if $0 { open.insert(bank.id) } else { open.remove(bank.id) } })
                    ) {
                        ForEach(Array(zip(bank.numbers, bank.patches)), id: \.0) { number, name in
                            let sel = model.selection.map { $0.bank == bank.id && $0.patch == number } ?? false
                            Text(name)
                                .foregroundColor(sel ? HS2Style.lcd : .primary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                                .onTapGesture { model.load(bank: bank.id, patch: number) }
                        }
                    } label: {
                        Text(bank.name).bold()
                    }
                }
            }
            .listStyle(.plain)
        }
        .padding(6)
        .background(RoundedRectangle(cornerRadius: 6).fill(HS2Style.panel))
    }
}

struct MeterView: View {
    var left: Float
    var right: Float

    var body: some View {
        GeometryReader { g in
            HStack(spacing: 2) {
                bar(left, g.size.height)
                bar(right, g.size.height)
            }
        }
    }

    private func bar(_ v: Float, _ h: CGFloat) -> some View {
        let x = CGFloat(min(1, max(0, v)))
        return ZStack(alignment: .bottom) {
            Rectangle().fill(Color.black.opacity(0.6))
            Rectangle().fill(v > 0.95 ? Color.red : Color.green).frame(height: h * x)
        }
    }
}

/// On-screen keyboard: click / drag to play.
struct KeyboardView: View {
    let low: Int
    let octaves: Int
    let noteOn: (Int) -> Void
    let noteOff: (Int) -> Void
    @State private var down: Int?

    private static let whiteSteps = [0, 2, 4, 5, 7, 9, 11]
    private static let blackSteps: [(step: Int, slot: Int)] = [(1, 0), (3, 1), (6, 3), (8, 4), (10, 5)]

    var body: some View {
        GeometryReader { g in
            let whites = octaves * 7
            let w = g.size.width / CGFloat(whites)
            let h = g.size.height
            ZStack(alignment: .topLeading) {
                ForEach(0..<whites, id: \.self) { i in
                    let note = low + (i / 7) * 12 + Self.whiteSteps[i % 7]
                    Rectangle()
                        .fill(down == note ? HS2Style.accent : Color(white: 0.93))
                        .overlay(Rectangle().stroke(Color.black.opacity(0.5), lineWidth: 0.5))
                        .frame(width: w, height: h)
                        .offset(x: CGFloat(i) * w)
                }
                ForEach(0..<octaves * 5, id: \.self) { i in
                    let b = Self.blackSteps[i % 5]
                    let note = low + (i / 5) * 12 + b.step
                    Rectangle()
                        .fill(down == note ? HS2Style.accent : Color.black)
                        .frame(width: w * 0.6, height: h * 0.6)
                        .offset(x: (CGFloat((i / 5) * 7 + b.slot) + 1) * w - w * 0.3)
                }
            }
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0)
                .onChanged { v in
                    let n = noteAt(v.location, w: w, h: h)
                    if n != down {
                        if let d = down { noteOff(d) }
                        if let n = n { noteOn(n) }
                        down = n
                    }
                }
                .onEnded { _ in
                    if let d = down { noteOff(d) }
                    down = nil
                })
        }
    }

    private func noteAt(_ p: CGPoint, w: CGFloat, h: CGFloat) -> Int? {
        guard p.x >= 0, p.y >= 0, p.y <= h else { return nil }
        if p.y < h * 0.6 {
            for i in 0..<octaves * 5 {
                let b = Self.blackSteps[i % 5]
                let x0 = (CGFloat((i / 5) * 7 + b.slot) + 1) * w - w * 0.3
                if p.x >= x0 && p.x < x0 + w * 0.6 {
                    return low + (i / 5) * 12 + b.step
                }
            }
        }
        let i = Int(p.x / w)
        guard i < octaves * 7 else { return nil }
        return low + (i / 7) * 12 + Self.whiteSteps[i % 7]
    }
}

struct AboutView: View {
    let text: String
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("HS2 – Hypersynth 2").font(.title2.bold())
            Text("A synthesizer in the style of Hypersonic 2. Reads Hypersonic 2 patch, phrase and tuning banks from your own content; Sample elements play your SoundFont / WAV samples. Not affiliated with Steinberg or Wizoo.")
                .fixedSize(horizontal: false, vertical: true)
            Text(text).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
            HStack {
                Button("Copy version info") {
                    #if os(macOS)
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                    #endif
                }
                Spacer()
                Button("Close") { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 460)
    }
}
