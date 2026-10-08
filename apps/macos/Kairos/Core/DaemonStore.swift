import Foundation
import Observation

@Observable
final class DaemonStore {
    var connected = false
    var state: DaemonState?
    var settings: KairosSettings?
    var settingsErrors: [String: String] = [:]
    var breakText = ""
    var overlayVisible = false
    var meetingNudge = false
    var notificationsDenied = false

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
}
