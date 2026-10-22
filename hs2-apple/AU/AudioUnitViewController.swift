// Principal class of the Audio Unit extension: creates the HS2AudioUnit for
// the host and shows the SwiftUI panel as its plug-in window.

import CoreAudioKit
import AppKit
import SwiftUI

public class AudioUnitViewController: AUViewController, AUAudioUnitFactory {
    private var audioUnit: HS2AudioUnit?
    private var hosting: NSHostingView<HS2PanelView>?

    public override func loadView() {
        view = NSView(frame: NSRect(x: 0, y: 0, width: 960, height: 600))
        preferredContentSize = NSSize(width: 960, height: 600)
    }

    public override func viewDidLoad() {
        super.viewDidLoad()
        attachUI()
    }

    public func createAudioUnit(with componentDescription: AudioComponentDescription) throws -> AUAudioUnit {
        let au = try HS2AudioUnit(componentDescription: componentDescription, options: [])
        audioUnit = au
        DispatchQueue.main.async { [weak self] in self?.attachUI() }
        return au
    }

    private func attachUI() {
        guard let au = audioUnit, isViewLoaded, hosting == nil else { return }
        let h = NSHostingView(rootView: HS2PanelView(model: HS2Model(unit: au, standalone: false)))
        h.frame = view.bounds
        h.autoresizingMask = [.width, .height]
        view.addSubview(h)
        hosting = h
    }
}
