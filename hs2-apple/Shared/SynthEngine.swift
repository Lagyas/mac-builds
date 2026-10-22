// Swift wrapper of the Rust engine (hs2-ffi, include/hs2.h).
// All calls except the render path (HS2AudioUnit) are for the main thread.

import AudioToolbox
import Foundation
import HS2FFI

public enum HS2Param: AUParameterAddress {
    case master = 0, hype = 1, tune = 2
}

public struct PatchBank: Identifiable, Hashable {
    public let id: Int          // bank index
    public let name: String
    public let patches: [String]
    /// Patch numbers inside the bank (differ from 0..<n in search results).
    public let numbers: [Int]

    public init(id: Int, name: String, patches: [String], numbers: [Int]? = nil) {
        self.id = id
        self.name = name
        self.patches = patches
        self.numbers = numbers ?? Array(patches.indices)
    }
}

public final class SynthEngine {
    /// Opaque `HS2Engine *`.
    let handle: OpaquePointer

    public init(sampleRate: Double) {
        handle = hs2_engine_new(Float(sampleRate))!
    }

    deinit {
        hs2_engine_free(handle)
    }

    public static var versionText: String {
        String(cString: hs2_version()!)
    }

    public func setSampleRate(_ sr: Double) {
        hs2_set_sample_rate(handle, Float(sr))
    }

    // MARK: MIDI (UI thread)

    public func midi(_ bytes: [UInt8]) {
        bytes.withUnsafeBufferPointer { hs2_midi(handle, $0.baseAddress, UInt32($0.count)) }
    }

    public func noteOn(channel: Int, note: Int, velocity: Int = 100) {
        midi([0x90 | UInt8(channel & 15), UInt8(note & 127), UInt8(max(1, min(127, velocity)))])
    }

    public func noteOff(channel: Int, note: Int) {
        midi([0x80 | UInt8(channel & 15), UInt8(note & 127), 0])
    }

    public func panic() {
        hs2_panic(handle)
    }

    // MARK: library

    @discardableResult
    public func loadContent(at url: URL) -> Int {
        Int(hs2_load_content_dir(handle, url.path))
    }

    /// Copies a Hypersonic 2 content folder (banks, combis; sample banks only
    /// as their name headers) into `destination`. Returns files copied, -1 on error.
    public static func importContent(from src: URL, to dst: URL) -> Int {
        Int(hs2_import_content(src.path, dst.path))
    }

    @discardableResult
    public func loadSoundFont(at url: URL) -> Int {
        Int(hs2_load_soundfont(handle, url.path))
    }

    public var status: String {
        readString { hs2_status(handle, $0, $1) }
    }

    public func banks() -> [PatchBank] {
        (0..<hs2_bank_count(handle)).map { b in
            let name = readString { hs2_bank_name(handle, b, $0, $1) }
            let patches = (0..<hs2_patch_count(handle, b)).map { p in readString { hs2_patch_name(handle, b, p, $0, $1) } }
            return PatchBank(id: Int(b), name: name, patches: patches)
        }
    }

    public func combis() -> [String] {
        (0..<hs2_combi_count(handle)).map { i in readString { hs2_combi_name(handle, i, $0, $1) } }
    }

    @discardableResult
    public func loadPatch(part: Int, bank: Int, patch: Int) -> Bool {
        hs2_load_patch(handle, UInt32(part), UInt32(bank), UInt32(patch)) != 0
    }

    @discardableResult
    public func loadCombi(_ index: Int) -> Bool {
        hs2_load_combi(handle, UInt32(index)) != 0
    }

    public func partPatchName(_ part: Int) -> String {
        readString { hs2_part_patch_name(handle, UInt32(part), $0, $1) }
    }

    public func partSource(_ part: Int) -> (bank: Int, patch: Int)? {
        var b: UInt32 = 0, p: UInt32 = 0
        return hs2_part_source(handle, UInt32(part), &b, &p) != 0 ? (Int(b), Int(p)) : nil
    }

    // MARK: parameters, meters

    public func setParam(_ address: AUParameterAddress, _ value: Float) {
        hs2_set_param(handle, UInt32(address), value)
    }

    public func param(_ address: AUParameterAddress) -> Float {
        hs2_get_param(handle, UInt32(address))
    }

    public func meter() -> (left: Float, right: Float, voices: Int) {
        var l: Float = 0, r: Float = 0, v: UInt32 = 0
        hs2_meter(handle, &l, &r, &v)
        return (l, r, Int(v))
    }

    // MARK: state

    public func saveState() -> Data {
        var len: UInt32 = 0
        guard let p = hs2_state_save(handle, &len) else { return Data() }
        defer { hs2_bytes_free(p, len) }
        return Data(bytes: p, count: Int(len))
    }

    @discardableResult
    public func loadState(_ data: Data) -> Bool {
        data.withUnsafeBytes { raw in
            hs2_state_load(handle, raw.bindMemory(to: UInt8.self).baseAddress, UInt32(raw.count)) != 0
        }
    }
}

/// Calls a `*_name(…, buf, cap)` function, growing the buffer when needed.
func readString(_ f: (UnsafeMutablePointer<CChar>, UInt32) -> UInt32) -> String {
    var cap = 256
    while true {
        var buf = [CChar](repeating: 0, count: cap)
        let n = buf.withUnsafeMutableBufferPointer { f($0.baseAddress!, UInt32(cap)) }
        if Int(n) < cap {
            return buf.withUnsafeBufferPointer { String(cString: $0.baseAddress!) }
        }
        cap = Int(n) + 1
    }
}
