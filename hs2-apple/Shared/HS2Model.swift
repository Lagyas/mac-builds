// View model of the SwiftUI panel (app window and the Audio Unit's view).

import AVFoundation
import Combine
import Foundation

public final class HS2Model: ObservableObject {
    public let unit: HS2AudioUnit
    public let standalone: Bool
    var engine: SynthEngine { unit.engine }

    @Published public var banks: [PatchBank] = []
    @Published public var combis: [String] = []
    @Published public var partNames: [String] = Array(repeating: "", count: 16)
    @Published public var selectedPart = 0
    @Published public var search = ""
    @Published public var meterL: Float = 0
    @Published public var meterR: Float = 0
    @Published public var voices = 0
    @Published public var status = ""
    @Published public var master: Float = 0.7 { didSet { setParam(.master, master) } }
    @Published public var hype: Float = 0 { didSet { setParam(.hype, hype) } }
    @Published public var tune: Float = 0 { didSet { setParam(.tune, tune) } }
    @Published public var selection: (bank: Int, patch: Int)?
    @Published public var octave = 3

    private var timer: Timer?
    private var syncing = false
    private var paramObserver: AUParameterObserverToken?

    public init(unit: HS2AudioUnit, standalone: Bool) {
        self.unit = unit
        self.standalone = standalone
        reloadLibrary()
        pullParameters()
        // host automation / state loads → sliders
        paramObserver = unit.parameterTree?.token(byAddingParameterObserver: { [weak self] _, _ in
            DispatchQueue.main.async { self?.pullParameters() }
        })
        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 15.0, repeats: true) { [weak self] _ in self?.tick() }
    }

    deinit {
        timer?.invalidate()
        if let t = paramObserver {
            unit.parameterTree?.removeParameterObserver(t)
        }
    }

    // MARK: version

    public static var appVersion: String {
        let info = Bundle.main.infoDictionary ?? [:]
        let v = info["CFBundleShortVersionString"] as? String ?? "?"
        let b = info["CFBundleVersion"] as? String ?? "?"
        let commit = info["HS2GitCommit"] as? String ?? "unknown"
        return "\(v) (build \(b), \(commit))"
    }

    public var versionText: String {
        "HS2 – Hypersynth 2 \(Self.appVersion)\n\(standalone ? "Standalone app" : "Audio Unit")\n\(SynthEngine.versionText)"
    }

    // MARK: library

    public func reloadLibrary() {
        banks = engine.banks()
        combis = engine.combis()
        status = engine.status
        refreshParts()
    }

    public var filteredBanks: [PatchBank] {
        let q = search.trimmingCharacters(in: .whitespaces).lowercased()
        if q.isEmpty { return banks }
        return banks.compactMap { b in
            if b.name.lowercased().contains(q) { return b }
            let hits = b.patches.indices.filter { b.patches[$0].lowercased().contains(q) }
            return hits.isEmpty ? nil : PatchBank(id: b.id, name: b.name, patches: hits.map { b.patches[$0] }, numbers: hits.map { b.numbers[$0] })
        }
    }

    /// Load patch number `patch` of bank `bank` into the selected part.
    public func load(bank: Int, patch: Int) {
        engine.loadPatch(part: selectedPart, bank: bank, patch: patch)
        selection = (bank, patch)
        refreshParts()
    }

    public func loadCombi(_ i: Int) {
        engine.loadCombi(i)
        refreshParts()
    }

    public func step(_ dir: Int) {
        let flat = banks.flatMap { b in b.patches.indices.map { (b.id, $0) } }
        guard !flat.isEmpty else { return }
        let cur = engine.partSource(selectedPart).flatMap { s in flat.firstIndex { $0 == (s.bank, s.patch) } } ?? -1
        let next = flat[(cur + dir + flat.count) % flat.count]
        engine.loadPatch(part: selectedPart, bank: next.0, patch: next.1)
        selection = next
        refreshParts()
    }

    func refreshParts() {
        partNames = (0..<16).map { engine.partPatchName($0) }
        if let s = engine.partSource(selectedPart) {
            selection = s
        }
    }

    public func selectPart(_ p: Int) {
        selectedPart = p
        selection = engine.partSource(p)
    }

    // MARK: content (the app imports, the Audio Unit only reloads)

    public func importContent(from url: URL) {
        let n = SharedContent.importContent(from: url)
        if n < 0 {
            status = "Could not copy the content folder."
            return
        }
        unit.reloadContent()
        reloadLibrary()
        if !SharedContent.usesAppGroup {
            status += "  (no app group: the Audio Unit will not see it — set the Team in Xcode)"
        }
    }

    public func importSoundFont(from url: URL) {
        do {
            let dst = try SharedContent.importSoundFont(from: url)
            let n = engine.loadSoundFont(at: dst)
            status = n < 0 ? "\(url.lastPathComponent): not a SoundFont / WAV" : engine.status
        } catch {
            status = "SoundFont: \(error.localizedDescription)"
        }
    }

    public func reloadContent() {
        unit.reloadContent()
        reloadLibrary()
    }

    // MARK: playing

    public func noteOn(_ note: Int, velocity: Int = 100) {
        engine.noteOn(channel: selectedPart, note: note, velocity: velocity)
    }

    public func noteOff(_ note: Int) {
        engine.noteOff(channel: selectedPart, note: note)
    }

    public func panic() {
        engine.panic()
    }

    // MARK: parameters, meters

    private func setParam(_ p: HS2Param, _ v: Float) {
        guard !syncing else { return }
        // through the tree so the host sees the change (automation recording)
        if let param = unit.parameterTree?.parameter(withAddress: p.rawValue) {
            param.setValue(v, originator: paramObserver)
        } else {
            engine.setParam(p.rawValue, v)
        }
    }

    func pullParameters() {
        syncing = true
        master = engine.param(HS2Param.master.rawValue)
        hype = engine.param(HS2Param.hype.rawValue)
        tune = engine.param(HS2Param.tune.rawValue)
        syncing = false
    }

    private func tick() {
        let m = engine.meter()
        meterL = max(m.left, meterL * 0.8)
        meterR = max(m.right, meterR * 0.8)
        voices = m.voices
        let names = (0..<16).map { engine.partPatchName($0) }
        if names != partNames {
            partNames = names // MIDI program change / host state
        }
    }
}
