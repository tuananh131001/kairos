import AppKit
import Foundation

@MainActor
final class AppModel {
    static let shared = AppModel()

    let store = DaemonStore()
    let client = IPCClient()
    let notifier = MeetingNotifier()
    let settingsDraft = SettingsDraft()
    private(set) lazy var overlay = BreakOverlayController(model: self)
    private(set) lazy var gate = PermissionGateController(model: self)
    private var started = false
    private var trackingMenus = 0
    private var menuObservers: [NSObjectProtocol] = []

    func start() {
        guard !started else { return }
        started = true
        LoginItems.onLaunch()
        client.onMessage = { [weak self] event in
            MainActor.assumeIsolated { self?.handle(event) }
        }
        client.onConnectionChange = { [weak self] connected in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.store.setConnected(connected)
                self.syncWindows()
            }
        }
        notifier.onPause = { [weak self] preset in
            self?.pause(preset, source: .meetingNotification)
        }
        notifier.onAuthorizationChange = { [weak self] authorized in
            self?.store.notificationsDenied = !authorized
        }
        notifier.setup()
        observeMenuTracking()
        client.start()
    }

    private func observeMenuTracking() {
        let center = NotificationCenter.default
        menuObservers = [
            center.addObserver(forName: NSMenu.didBeginTrackingNotification, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.menuTrackingChanged(by: 1) }
            },
            center.addObserver(forName: NSMenu.didEndTrackingNotification, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.menuTrackingChanged(by: -1) }
            },
        ]
    }

    private func menuTrackingChanged(by delta: Int) {
        trackingMenus = max(0, trackingMenus + delta)
        store.menuOpen = trackingMenus > 0
    }

    func handle(_ event: ServerEvent) {
        store.apply(event)
        switch event {
        case .meetingDetected:
            notifier.meetingDetected()
        case .meetingEnded:
            notifier.meetingEnded()
        default:
            break
        }
        syncWindows()
    }

    func syncWindows() {
        overlay.sync(visible: store.overlayVisible)
        gate.sync()
    }

    func pause(_ preset: PausePreset, source: PauseSource) {
        store.overlayVisible = false
        syncWindows()
        client.send(.pause(preset: preset, source: source)) { [weak self] reply in
            MainActor.assumeIsolated { self?.handle(.reply(reply)) }
        }
    }

    func resume() {
        client.send(.resume) { [weak self] reply in
            MainActor.assumeIsolated { self?.handle(.reply(reply)) }
        }
    }

    func postpone() {
        store.overlayVisible = false
        overlay.sync(visible: false)
        client.send(.postponeBreak) { [weak self] reply in
            MainActor.assumeIsolated { self?.handle(.reply(reply)) }
        }
    }

    func save(_ settings: KairosSettings, completion: @escaping (Bool) -> Void) {
        client.send(.updateSettings(settings)) { [weak self] reply in
            MainActor.assumeIsolated {
                completion(self?.store.applySettingsReply(reply) ?? false)
            }
        }
    }

    func quit() {
        var finished = false
        let terminate = {
            guard !finished else { return }
            finished = true
            NSApp.terminate(nil)
        }
        client.send(.shutdown) { _ in terminate() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { terminate() }
    }

    func restartKairos() {
        LoginItems.kickstartDaemon(restart: true)
        client.send(.restartCapture)
        guard LoginItems.isBundled else { return }
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.createsNewApplicationInstance = true
        NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL, configuration: configuration) { _, _ in
            DispatchQueue.main.async { NSApp.terminate(nil) }
        }
    }
}
