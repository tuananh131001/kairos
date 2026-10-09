import AppKit
import SwiftUI

final class OverlayWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    override func cancelOperation(_ sender: Any?) {}

    override func performClose(_ sender: Any?) {}

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.modifierFlags.contains(.command) {
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {
            return
        }
        super.keyDown(with: event)
    }
}

@MainActor
final class BreakOverlayController {
    private weak var model: AppModel?
    private var windows: [OverlayWindow] = []
    private var screenObserver: NSObjectProtocol?

    init(model: AppModel) {
        self.model = model
        screenObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, !self.windows.isEmpty else { return }
                self.hide()
                self.show()
            }
        }
    }

    var isVisible: Bool { !windows.isEmpty }

    func sync(visible: Bool) {
        if visible && windows.isEmpty {
            show()
        } else if !visible && !windows.isEmpty {
            hide()
        }
    }

    private func show() {
        guard let model else { return }
        for screen in NSScreen.screens {
            let window = OverlayWindow(
                contentRect: screen.frame,
                styleMask: [.borderless],
                backing: .buffered,
                defer: false
            )
            window.setFrame(screen.frame, display: true)
            window.level = .screenSaver
            window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
            window.isOpaque = false
            window.backgroundColor = .clear
            window.hasShadow = false
            window.isReleasedWhenClosed = false
            window.animationBehavior = .none
            window.contentView = NSHostingView(rootView: BreakOverlayView(store: model.store) { [weak model] in
                model?.postpone()
            })
            window.orderFrontRegardless()
            windows.append(window)
        }
        NSApp.activate(ignoringOtherApps: true)
        windows.first?.makeKey()
    }

    private func hide() {
        for window in windows {
            window.orderOut(nil)
            window.contentView = nil
        }
        windows.removeAll()
    }
}

struct BreakOverlayView: View {
    let store: DaemonStore
    let onPostpone: () -> Void

    var body: some View {
        ZStack {
            Rectangle()
                .fill(.ultraThinMaterial)
            Color.black.opacity(0.55)
            VStack(spacing: 28) {
                Text(store.breakText)
                    .font(.system(size: 40, weight: .semibold, design: .rounded))
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.white)
                    .frame(maxWidth: 900)
                Text(Formatting.countdown(store.overlayRemaining))
                    .font(.system(size: 96, weight: .light, design: .rounded).monospacedDigit())
                    .foregroundStyle(.white)
                Button(action: onPostpone) {
                    Text("Postpone 5 min")
                        .font(.title3)
                        .padding(.horizontal, 20)
                        .padding(.vertical, 8)
                }
                .buttonStyle(.bordered)
                .tint(.white)
            }
            .padding(60)
        }
        .ignoresSafeArea()
    }
}
