// Audio + MIDI for the standalone app.

import AVFoundation
import CoreMIDI
import SwiftUI

final class HostAudio: ObservableObject {
    @Published var model: HS2Model?
    @Published var error: String?

    private let engine = AVAudioEngine()
    private var avUnit: AVAudioUnit?
    private var midiClient = MIDIClientRef()
    private var midiPort = MIDIPortRef()

    init() {
        AUAudioUnit.registerSubclass(HS2AudioUnit.self, as: HS2Component.inApp,
                                     name: "Lagyas: HS2 – Hypersynth 2 (app)", version: HS2Component.version)
        AVAudioUnit.instantiate(with: HS2Component.inApp, options: .loadInProcess) { [weak self] unit, err in
            DispatchQueue.main.async { self?.started(unit, err) }
        }
    }

    private func started(_ unit: AVAudioUnit?, _ err: Error?) {
        guard let unit = unit, let au = unit.auAudioUnit as? HS2AudioUnit else {
            error = "Could not create the synth: \(err?.localizedDescription ?? "unknown error")"
            return
        }
        avUnit = unit
        engine.attach(unit)
        let format = engine.outputNode.outputFormat(forBus: 0)
        let stereo = AVAudioFormat(standardFormatWithSampleRate: format.sampleRate > 0 ? format.sampleRate : 48000, channels: 2)
        engine.connect(unit, to: engine.mainMixerNode, format: stereo)
        do {
            try engine.start()
        } catch {
            self.error = "Audio output: \(error.localizedDescription)"
            return
        }
        model = HS2Model(unit: au, standalone: true)
        startMIDI(au)
    }

    // MARK: CoreMIDI: every source → the synth (sample-accurate via the AU's MIDI block)

    private func startMIDI(_ au: HS2AudioUnit) {
        guard let send = au.scheduleMIDIEventBlock else { return }
        MIDIClientCreateWithBlock("HS2" as CFString, &midiClient) { [weak self] note in
            if note.pointee.messageID == .msgSetupChanged {
                DispatchQueue.main.async { self?.connectSources() }
            }
        }
        MIDIInputPortCreateWithProtocol(midiClient, "HS2 In" as CFString, ._1_0, &midiPort) { list, _ in
            for packet in list.unsafeSequence() {
                let count = Int(packet.pointee.wordCount)
                withUnsafeBytes(of: packet.pointee.words) { raw in
                    let words = raw.bindMemory(to: UInt32.self)
                    var i = 0
                    while i < count {
                        let w = words[i]
                        let type = w >> 28
                        if type == 0x2 { // MIDI 1.0 channel voice
                            var bytes: (UInt8, UInt8, UInt8) = (UInt8((w >> 16) & 0xFF), UInt8((w >> 8) & 0x7F), UInt8(w & 0x7F))
                            let len = (bytes.0 & 0xF0) == 0xC0 || (bytes.0 & 0xF0) == 0xD0 ? 2 : 3
                            withUnsafePointer(to: &bytes) {
                                $0.withMemoryRebound(to: UInt8.self, capacity: 3) { send(AUEventSampleTimeImmediate, 0, len, $0) }
                            }
                        }
                        // message size in words by UMP type
                        i += [1, 1, 1, 2, 2, 4, 1, 1, 2, 2, 2, 3, 3, 4, 4, 4][Int(type)]
                    }
                }
            }
        }
        connectSources()
    }

    private func connectSources() {
        for i in 0..<MIDIGetNumberOfSources() {
            MIDIPortConnectSource(midiPort, MIDIGetSource(i), nil)
        }
    }
}
