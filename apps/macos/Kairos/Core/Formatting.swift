import Foundation

enum Formatting {
    static func percent(_ load: Double) -> Int {
        Int(min(100, max(0, load + 0.001)).rounded(.down))
    }

    static func menuBarTitle(state: DaemonState?, connected: Bool) -> String {
        guard connected, let state else { return "–%" }
        let value = "\(percent(state.loadPercent))%"
        return state.phase == .paused ? "⏸ \(value)" : value
    }

    static func isAlert(state: DaemonState?) -> Bool {
        guard let state else { return false }
        return state.postponed && percent(state.loadPercent) >= 100
    }

    static func countdown(_ seconds: Int) -> String {
        let s = max(0, seconds)
        return String(format: "%02d:%02d", s / 60, s % 60)
    }

    static func shortCountdown(_ seconds: Int) -> String {
        let s = max(0, seconds)
        return String(format: "%d:%02d", s / 60, s % 60)
    }

    static func hoursMinutes(_ seconds: Int) -> String {
        let minutes = max(0, seconds) / 60
        return String(format: "%d:%02d", minutes / 60, minutes % 60)
    }

    static func clockTime(_ epochSeconds: Int64, timeZone: TimeZone = .current) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "HH:mm"
        return formatter.string(from: Date(timeIntervalSince1970: TimeInterval(epochSeconds)))
    }

    static func statusLine(state: DaemonState?, connected: Bool, timeZone: TimeZone = .current) -> String {
        guard connected else { return "Disconnected — reconnecting…" }
        guard let state else { return "Connecting…" }
        switch state.phase {
        case .paused:
            if let until = state.pausedUntil {
                return "Paused until \(clockTime(until, timeZone: timeZone))"
            }
            return "Paused"
        case .onBreak:
            return "On break · \(countdown(state.breakRemainingS ?? 0)) left"
        case .running:
            let seconds = state.breakInS ?? 0
            if state.postponed {
                return "Break postponed · \(shortCountdown(seconds))"
            }
            return "Break in \(countdown(seconds))"
        }
    }

    static func activityLine(_ state: DaemonState?) -> String {
        switch state?.activity {
        case .active: return "Active"
        case .watching: return "Watching"
        case .away: return "Away"
        case .paused: return "Paused"
        case nil: return "—"
        }
    }

    static func todayLine(_ state: DaemonState?) -> String {
        "Today \(hoursMinutes(state?.todayWorkedS ?? 0))"
    }

    static func presetTitle(_ preset: PausePreset) -> String {
        switch preset {
        case .m30: return "30 minutes"
        case .h2: return "2 hours"
        case .h4: return "4 hours"
        case .d1: return "1 day"
        }
    }

    static func delayLabel(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds) s" }
        let m = seconds / 60
        let s = seconds % 60
        return s == 0 ? "\(m) min" : "\(m) min \(s) s"
    }
}
