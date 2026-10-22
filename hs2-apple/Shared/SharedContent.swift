// The folder the app and the Audio Unit share (app group container):
//   Content/    Hypersonic 2 banks imported by the app (sample banks: names only)
//   SoundFont/  the user's SoundFont (.sf2) or WAV for Sample elements

import Foundation

public enum SharedContent {
    /// App group from Info.plist (HS2AppGroup = $(TeamIdentifierPrefix)cz.lagyas.hs2).
    static var groupID: String? {
        Bundle.main.object(forInfoDictionaryKey: "HS2AppGroup") as? String
    }

    public static var root: URL {
        if let g = groupID, let u = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: g) {
            return u
        }
        // no app group (unsigned build): per-user folder, seen by the app only
        let u = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("HS2 Hypersynth 2")
        return u
    }

    public static var usesAppGroup: Bool {
        groupID.flatMap { FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: $0) } != nil
    }

    public static var contentDir: URL { root.appendingPathComponent("Content", isDirectory: true) }
    public static var soundFontDir: URL { root.appendingPathComponent("SoundFont", isDirectory: true) }

    public static var hasContent: Bool {
        let items = (try? FileManager.default.contentsOfDirectory(atPath: contentDir.path)) ?? []
        return !items.isEmpty
    }

    public static var soundFont: URL? {
        let items = (try? FileManager.default.contentsOfDirectory(at: soundFontDir, includingPropertiesForKeys: nil)) ?? []
        return items.first { ["sf2", "wav"].contains($0.pathExtension.lowercased()) }
    }

    /// Replace the shared content with a Hypersonic 2 content folder.
    public static func importContent(from src: URL) -> Int {
        let fm = FileManager.default
        try? fm.removeItem(at: contentDir)
        try? fm.createDirectory(at: contentDir, withIntermediateDirectories: true)
        let ok = src.startAccessingSecurityScopedResource()
        defer { if ok { src.stopAccessingSecurityScopedResource() } }
        return SynthEngine.importContent(from: src, to: contentDir)
    }

    /// Replace the shared SoundFont with a copy of `src`.
    public static func importSoundFont(from src: URL) throws -> URL {
        let fm = FileManager.default
        try? fm.removeItem(at: soundFontDir)
        try fm.createDirectory(at: soundFontDir, withIntermediateDirectories: true)
        let ok = src.startAccessingSecurityScopedResource()
        defer { if ok { src.stopAccessingSecurityScopedResource() } }
        let dst = soundFontDir.appendingPathComponent(src.lastPathComponent)
        try fm.copyItem(at: src, to: dst)
        return dst
    }

    /// Load the shared content and SoundFont into `engine`.
    public static func load(into engine: SynthEngine) {
        if hasContent {
            engine.loadContent(at: contentDir)
        }
        if let sf = soundFont {
            engine.loadSoundFont(at: sf)
        }
    }
}
