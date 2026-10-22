// HS2 – Hypersynth 2 as an AUv3 instrument (aumu / hs2s / Lgys).
// Used by the Audio Unit extension (hosts: Logic, GarageBand, Ableton, …)
// and in-process by the standalone app.

import AVFoundation
import AudioToolbox
import CoreAudioKit
import HS2FFI

public func fourCC(_ s: String) -> FourCharCode {
    s.utf8.reduce(0) { $0 << 8 | FourCharCode($1) }
}

public enum HS2Component {
    /// The extension's component (Info.plist AudioComponents).
    public static let description = AudioComponentDescription(
        componentType: kAudioUnitType_MusicDevice,
        componentSubType: fourCC("hs2s"),
        componentManufacturer: fourCC("Lgys"),
        componentFlags: 0,
        componentFlagsMask: 0)
    /// The app registers its in-process copy under its own subtype.
    public static let inApp = AudioComponentDescription(
        componentType: kAudioUnitType_MusicDevice,
        componentSubType: fourCC("hs2i"),
        componentManufacturer: fourCC("Lgys"),
        componentFlags: 0,
        componentFlagsMask: 0)
    /// 0.2.0 → 0x00000200 (Info.plist "version").
    public static let version: UInt32 = 0x0000_0200
}

/// Render-thread data, allocated in allocateRenderResources.
final class RenderContext {
    let maxEvents = 512
    let events: UnsafeMutablePointer<HS2Event>
    var scratch: [UnsafeMutablePointer<Float>] = []
    var frames: Int = 0
    var musicalContext: AUHostMusicalContextBlock?
    var transportState: AUHostTransportStateBlock?

    init() {
        events = .allocate(capacity: maxEvents)
        events.initialize(repeating: HS2Event(), count: maxEvents)
    }

    func allocate(frames n: Int) {
        free()
        frames = n
        scratch = (0..<2).map { _ in
            let p = UnsafeMutablePointer<Float>.allocate(capacity: n)
            p.initialize(repeating: 0, count: n)
            return p
        }
    }

    func free() {
        scratch.forEach { $0.deallocate() }
        scratch = []
        frames = 0
    }

    deinit {
        free()
        events.deallocate()
    }
}

public final class HS2AudioUnit: AUAudioUnit {
    public let engine: SynthEngine
    private let ctx = RenderContext()
    private var outputBus: AUAudioUnitBus!
    private var busses: AUAudioUnitBusArray!
    private var presetMap: [(bank: Int, patch: Int)] = []
    private var presets: [AUAudioUnitPreset] = []
    private var current: AUAudioUnitPreset?

    public override init(componentDescription: AudioComponentDescription, options: AudioComponentInstantiationOptions = []) throws {
        engine = SynthEngine(sampleRate: 44100)
        try super.init(componentDescription: componentDescription, options: options)
        let format = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2)!
        outputBus = try AUAudioUnitBus(format: format)
        outputBus.maximumChannelCount = 2
        busses = AUAudioUnitBusArray(audioUnit: self, busType: .output, busses: [outputBus])
        maximumFramesToRender = 4096

        SharedContent.load(into: engine)
        buildPresets()
        setupParameters()
    }

    // MARK: parameters

    private func setupParameters() {
        let flags: AudioUnitParameterOptions = [.flag_IsReadable, .flag_IsWritable, .flag_CanRamp]
        let master = AUParameterTree.createParameter(withIdentifier: "master", name: "Master", address: HS2Param.master.rawValue,
                                                     min: 0, max: 1, unit: .generic, unitName: nil, flags: flags,
                                                     valueStrings: nil, dependentParameters: nil)
        let hype = AUParameterTree.createParameter(withIdentifier: "hype", name: "Output Hype", address: HS2Param.hype.rawValue,
                                                   min: 0, max: 1, unit: .generic, unitName: nil, flags: flags,
                                                   valueStrings: nil, dependentParameters: nil)
        let tune = AUParameterTree.createParameter(withIdentifier: "tune", name: "Master Tune", address: HS2Param.tune.rawValue,
                                                   min: -1, max: 1, unit: .relativeSemiTones, unitName: nil, flags: flags,
                                                   valueStrings: nil, dependentParameters: nil)
        let tree = AUParameterTree.createTree(withChildren: [master, hype, tune])
        let engine = self.engine
        tree.implementorValueObserver = { p, v in engine.setParam(p.address, v) }
        tree.implementorValueProvider = { p in engine.param(p.address) }
        parameterTree = tree
        syncParameters()
    }

    /// Push the engine's values to the tree (after state / preset loads).
    func syncParameters() {
        guard let tree = parameterTree else { return }
        for p in tree.allParameters {
            p.setValue(engine.param(p.address), originator: nil)
        }
    }

    // MARK: busses, render

    public override var outputBusses: AUAudioUnitBusArray { busses }

    public override func allocateRenderResources() throws {
        try super.allocateRenderResources()
        engine.setSampleRate(outputBus.format.sampleRate)
        ctx.allocate(frames: Int(maximumFramesToRender))
        ctx.musicalContext = musicalContextBlock
        ctx.transportState = transportStateBlock
    }

    public override func deallocateRenderResources() {
        ctx.musicalContext = nil
        ctx.transportState = nil
        super.deallocateRenderResources()
    }

    public override var internalRenderBlock: AUInternalRenderBlock {
        let handle = engine.handle
        let ctx = self.ctx
        return { _, timestamp, frameCount, _, outputData, eventList, _ in
            let abl = UnsafeMutableAudioBufferListPointer(outputData)
            guard abl.count > 0, Int(frameCount) <= ctx.frames, ctx.scratch.count == 2 else {
                return kAudioUnitErr_TooManyFramesToProcess
            }
            for i in 0..<abl.count {
                if abl[i].mData == nil {
                    abl[i].mData = UnsafeMutableRawPointer(ctx.scratch[min(i, 1)])
                }
                abl[i].mDataByteSize = frameCount * UInt32(MemoryLayout<Float>.size)
            }
            let left = abl[0].mData!.assumingMemoryBound(to: Float.self)
            let right = abl.count > 1 ? abl[1].mData!.assumingMemoryBound(to: Float.self) : ctx.scratch[1]

            // events → HS2Event array (sorted by time, as the host delivers them)
            let start = AUEventSampleTime(timestamp.pointee.mSampleTime)
            var n = 0
            var ev = eventList
            while let e = ev, n < ctx.maxEvents {
                let head = e.pointee.head
                let off = UInt32(max(0, min(Int64(frameCount), head.eventSampleTime - start)))
                switch head.eventType {
                case .MIDI:
                    let m = e.pointee.MIDI
                    ctx.events[n] = HS2Event(offset: off, param: 0, value: 0, kind: UInt8(HS2_EVENT_MIDI),
                                             len: UInt8(min(3, Int(m.length))), data: (m.data.0, m.data.1, m.data.2))
                    n += 1
                case .parameter, .parameterRamp:
                    let p = e.pointee.parameter
                    ctx.events[n] = HS2Event(offset: off, param: UInt32(p.parameterAddress), value: p.value,
                                             kind: UInt8(HS2_EVENT_PARAM), len: 0, data: (0, 0, 0))
                    n += 1
                default:
                    break
                }
                ev = head.next.map { UnsafePointer($0) }
            }

            var t = HS2Transport(tempo: 0, beat: 0, playing: 0, valid: 0)
            if let mc = ctx.musicalContext {
                var tempo = 0.0, beat = 0.0
                if mc(&tempo, nil, nil, &beat, nil, nil) {
                    t.tempo = tempo
                    t.beat = beat
                    t.valid = 1
                }
            }
            if let ts = ctx.transportState {
                var flags = AUHostTransportStateFlags()
                if ts(&flags, nil, nil, nil) {
                    t.playing = flags.contains(.moving) ? 1 : 0
                }
            }
            withUnsafePointer(to: &t) { tp in
                hs2_process(handle, left, right, frameCount, ctx.events, UInt32(n), tp)
            }
            for i in 2..<max(2, abl.count) {
                memset(abl[i].mData, 0, Int(abl[i].mDataByteSize))
            }
            return noErr
        }
    }

    // MARK: presets and state

    private func buildPresets() {
        presetMap = []
        presets = []
        for bank in engine.banks() {
            for (i, name) in bank.patches.enumerated() {
                let p = AUAudioUnitPreset()
                p.number = presets.count
                p.name = "\(bank.name) – \(name)"
                presets.append(p)
                presetMap.append((bank.id, i))
            }
        }
    }

    /// Re-read the shared content (after the app imported new banks).
    public func reloadContent() {
        SharedContent.load(into: engine)
        buildPresets()
    }

    public override var factoryPresets: [AUAudioUnitPreset]? { presets }

    public override var supportsUserPresets: Bool { true }

    public override var currentPreset: AUAudioUnitPreset? {
        get { current }
        set {
            guard let p = newValue else {
                current = nil
                return
            }
            if p.number >= 0 {
                if p.number < presetMap.count {
                    let m = presetMap[p.number]
                    engine.loadPatch(part: 0, bank: m.bank, patch: m.patch)
                    current = p
                }
            } else if let state = try? presetState(for: p) {
                fullState = state
                current = p
            }
        }
    }

    public override var fullState: [String: Any]? {
        get {
            var s = super.fullState ?? [:]
            s["hs2"] = engine.saveState()
            return s
        }
        set {
            super.fullState = newValue
            if let d = newValue?["hs2"] as? Data {
                engine.loadState(d)
                syncParameters()
            }
        }
    }
}
