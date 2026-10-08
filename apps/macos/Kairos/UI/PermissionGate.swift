import AppKit
import SwiftUI

@Observable
final class PermissionGateState {
    var granted = false
    var openedSettings = false
}

@MainActor
final class PermissionGateController {
    private weak var model: AppModel?
    private var window: NSWindow?
    private let gateState = PermissionGateState()

    static let settingsURL = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")!

    init(model: AppModel) {
        self.model = model
    }

    func sync() {
        guard let model else { return }
        let store = model.store
        guard store.connected, let state = store.state else { return }
        if state.permission == .denied {
            gateState.granted = false
            show()
        } else if window != nil {
            gateState.granted = true
        }
    }

    private func show() {
        guard window == nil, let model else { return }
        let view = PermissionGateView(
            gate: gateState,
            openSettings: { [weak self] in
                self?.gateState.openedSettings = true
                NSWorkspace.shared.open(Self.settingsURL)
            },
            quit: { model.quit() },
            restart: { model.restartKairos() },
            dismiss: { [weak self] in self?.close() }
        )
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 460, height: 260),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.title = "Kairos needs Screen Recording"
        window.level = .floating
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: view)
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window
    }

    private func close() {
        window?.orderOut(nil)
        window = nil
        gateState.openedSettings = false
    }
}

struct PermissionGateView: View {
    let gate: PermissionGateState
    let openSettings: () -> Void
    let quit: () -> Void
    let restart: () -> Void
    let dismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label(gate.granted ? "Permission granted" : "Screen Recording is required", systemImage: gate.granted ? "checkmark.seal" : "rectangle.dashed.badge.record")
                .font(.headline)
            Text("Kairos looks at how much of your screen changes once per second to tell when you are watching a video or on a call without touching the keyboard. Frames stay in memory, are never saved and never leave your Mac.")
                .fixedSize(horizontal: false, vertical: true)
            if gate.granted {
                Text("Restart Kairos so the change takes effect.")
                    .foregroundStyle(.secondary)
            } else {
                Text("Allow “kairosd” (Kairos) in System Settings → Privacy & Security → Screen & System Audio Recording. Kairos does not track anything until it is allowed.")
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            HStack {
                Button("Quit", action: quit)
                Spacer()
                if gate.granted {
                    Button("Later", action: dismiss)
                    Button("Restart Kairos", action: restart)
                        .keyboardShortcut(.defaultAction)
                } else {
                    if gate.openedSettings {
                        Button("Restart Kairos", action: restart)
                    }
                    Button("Open System Settings", action: openSettings)
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(20)
        .frame(width: 460, height: 260)
    }
}
