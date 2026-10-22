// Standalone HS2 – Hypersynth 2: hosts the Audio Unit in-process
// (AVAudioEngine), takes MIDI from all CoreMIDI sources, and is the place
// where the Hypersonic 2 content and the SoundFont are imported for the plug-in.

import AppKit
import SwiftUI

@main
struct HS2App: App {
    @StateObject private var host = HostAudio()

    var body: some Scene {
        WindowGroup("HS2 – Hypersynth 2") {
            Group {
                if let model = host.model {
                    HS2PanelView(model: model)
                } else {
                    VStack(spacing: 12) {
                        Text("HS2 – Hypersynth 2").font(.title2.bold())
                        Text(host.error ?? "Starting audio…").foregroundColor(.secondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(minWidth: 900, minHeight: 560)
            .navigationTitle("HS2 – Hypersynth 2 \(HS2Model.appVersion)")
        }
        .commands {
            CommandGroup(replacing: .appInfo) {
                Button("About HS2 – Hypersynth 2") {
                    NSApplication.shared.orderFrontStandardAboutPanel(options: [
                        .applicationName: "HS2 – Hypersynth 2",
                        .applicationVersion: HS2Model.appVersion,
                        .credits: NSAttributedString(string: SynthEngine.versionText),
                    ])
                }
            }
        }
    }
}
