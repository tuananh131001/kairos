import Foundation
import Observation

struct MenuSnapshot: Equatable {
    var status: String
    var activity: String
    var today: String
    var connected: Bool
    var canResume: Bool
    var meetingFallback: Bool
}

struct LabelSnapshot: Equatable {
    var title: String
    var progress: Double
    var alert: Bool
    var badge: Bool
}

@Observable
final class DaemonStore {
    var connected = false { didSet { refreshSnapshots() } }
    var state: DaemonState? { didSet { refreshSnapshots() } }
    var settings: KairosSettings?
    var settingsErrors: [String: String] = [:]
    var breakText = ""
    var overlayVisible = false
    var meetingNudge = false { didSet { refreshSnapshots() } }
    var notificationsDenied = false { didSet { refreshSnapshots() } }
    var menuOpen = false { didSet { refreshSnapshots() } }
    private(set) var menu = MenuSnapshot(
        status: Formatting.statusLine(state: nil, connected: false),
        activity: Formatting.activityLine(nil),
        today: Formatting.todayLine(nil),
        connected: false,
        canResume: false,
        meetingFallback: false
    )
    private(set) var label = LabelSnapshot(
        title: Formatting.menuBarTitle(state: nil, connected: false),
        progress: 0,
        alert: false,
        badge: false
    )

    var overlayRemaining: Int {
        state?.breakRemainingS ?? breakStartRemaining
    }

    private var breakStartRemaining = 0

    var showMeetingFallback: Bool {
        meetingNudge && notificationsDenied && state?.phase != .paused
    }

    var showMeetingBadge: Bool {
        meetingNudge && notificationsDenied
    }

    func apply(_ event: ServerEvent) {
        switch event {
        case .state(let newState):
            applyState(newState)
        case .reply(let reply):
            if let newState = reply.state {
                applyState(newState)
            }
            if let newSettings = reply.settings {
                settings = newSettings
            }
        case .breakStarted(let text, let remaining):
            breakText = text
            breakStartRemaining = remaining
            if var current = state {
                current.phase = .onBreak
                current.breakRemainingS = remaining
                state = current
            }
            overlayVisible = true
        case .breakEnded:
            overlayVisible = false
        case .meetingDetected:
            meetingNudge = true
        case .meetingEnded:
            meetingNudge = false
        }
    }

    func setConnected(_ value: Bool) {
        connected = value
        if !value {
            overlayVisible = false
        }
    }

    func applySettingsReply(_ reply: Reply) -> Bool {
        if reply.ok {
            settingsErrors = [:]
            if let newSettings = reply.settings {
                settings = newSettings
            }
            if let newState = reply.state {
                applyState(newState)
            }
            return true
        }
        var errors: [String: String] = [:]
        for field in reply.error?.fields ?? [] {
            errors[field.field] = field.message
        }
        if errors.isEmpty {
            errors["form"] = reply.error?.message ?? "Could not save settings."
        }
        settingsErrors = errors
        return false
    }

    private func applyState(_ newState: DaemonState) {
        state = newState
        if newState.phase != .onBreak {
            overlayVisible = false
        }
        meetingNudge = newState.meetingActive
    }

    private func refreshSnapshots() {
        guard !menuOpen else { return }
        let liveState = connected ? state : nil
        let nextLabel = LabelSnapshot(
            title: Formatting.menuBarTitle(state: state, connected: connected),
            progress: Double(Formatting.percent(liveState?.loadPercent ?? 0)) / 100,
            alert: Formatting.isAlert(state: liveState),
            badge: showMeetingBadge
        )
        if label != nextLabel {
            label = nextLabel
        }
        let nextMenu = MenuSnapshot(
            status: Formatting.statusLine(state: state, connected: connected),
            activity: Formatting.activityLine(state),
            today: Formatting.todayLine(state),
            connected: connected,
            canResume: connected && state?.phase == .paused,
            meetingFallback: showMeetingFallback
        )
        if menu != nextMenu {
            menu = nextMenu
        }
    }
}
