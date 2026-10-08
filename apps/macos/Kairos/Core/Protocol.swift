import Foundation

let protocolVersion = 1

struct KairosSettings: Codable, Equatable {
    var workMinutes: Int
    var breakSeconds: Int
    var reminderText: String
    var idleThresholdSeconds: Int
    var meetingDetectionEnabled: Bool
    var meetingNotifyDelaySeconds: Int

    static let defaults = KairosSettings(
        workMinutes: 50,
        breakSeconds: 600,
        reminderText: "Time to rest your eyes. Stand up and stretch.",
        idleThresholdSeconds: 60,
        meetingDetectionEnabled: true,
        meetingNotifyDelaySeconds: 30
    )

    enum CodingKeys: String, CodingKey {
        case workMinutes = "work_minutes"
        case breakSeconds = "break_seconds"
        case reminderText = "reminder_text"
        case idleThresholdSeconds = "idle_threshold_seconds"
        case meetingDetectionEnabled = "meeting_detection_enabled"
        case meetingNotifyDelaySeconds = "meeting_notify_delay_seconds"
    }
}

enum EnginePhase: String, Codable {
    case running
    case onBreak = "on_break"
    case paused
}

enum Activity: String, Codable {
    case active, watching, away, paused
}

enum PermissionStatus: String, Codable {
    case granted, denied
}

enum BreakEndReason: String, Codable {
    case completed, postponed
    case cancelledByPause = "cancelled_by_pause"
}

enum PausePreset: String, Codable, CaseIterable {
    case m30 = "30m"
    case h2 = "2h"
    case h4 = "4h"
    case d1 = "1d"
}

enum PauseSource: String, Codable {
    case menu
    case meetingNotification = "meeting_notification"
}

struct DaemonState: Codable, Equatable {
    var phase: EnginePhase
    var activity: Activity
    var loadPercent: Double
    var breakInS: Int?
    var breakRemainingS: Int?
    var postponed: Bool
    var pausedUntil: Int64?
    var todayWorkedS: Int
    var permission: PermissionStatus
    var meetingActive: Bool

    enum CodingKeys: String, CodingKey {
        case phase, activity, postponed, permission
        case loadPercent = "load_percent"
        case breakInS = "break_in_s"
        case breakRemainingS = "break_remaining_s"
        case pausedUntil = "paused_until"
        case todayWorkedS = "today_worked_s"
        case meetingActive = "meeting_active"
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(phase, forKey: .phase)
        try c.encode(activity, forKey: .activity)
        try c.encode(loadPercent, forKey: .loadPercent)
        try c.encode(breakInS, forKey: .breakInS)
        try c.encode(breakRemainingS, forKey: .breakRemainingS)
        try c.encode(postponed, forKey: .postponed)
        try c.encode(pausedUntil, forKey: .pausedUntil)
        try c.encode(todayWorkedS, forKey: .todayWorkedS)
        try c.encode(permission, forKey: .permission)
        try c.encode(meetingActive, forKey: .meetingActive)
    }
}

struct FieldError: Codable, Equatable {
    var field: String
    var message: String
}

struct ErrorBody: Codable, Equatable {
    var code: String
    var message: String
    var fields: [FieldError]?
}

struct Reply: Equatable {
    var id: UInt64?
    var ok: Bool
    var error: ErrorBody?
    var state: DaemonState?
    var settings: KairosSettings?
}

enum ServerEvent: Equatable {
    case reply(Reply)
    case state(DaemonState)
    case breakStarted(text: String, remainingS: Int)
    case breakEnded(reason: BreakEndReason)
    case meetingDetected
    case meetingEnded
}

struct ServerMessage: Codable, Equatable {
    var v: Int = protocolVersion
    var event: ServerEvent

    enum CodingKeys: String, CodingKey {
        case v, type, id, ok, error, state, settings, text, reason
        case remainingS = "remaining_s"
    }

    init(event: ServerEvent) {
        self.event = event
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        v = try c.decode(Int.self, forKey: .v)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "Reply":
            event = .reply(Reply(
                id: try c.decodeIfPresent(UInt64.self, forKey: .id),
                ok: try c.decode(Bool.self, forKey: .ok),
                error: try c.decodeIfPresent(ErrorBody.self, forKey: .error),
                state: try c.decodeIfPresent(DaemonState.self, forKey: .state),
                settings: try c.decodeIfPresent(KairosSettings.self, forKey: .settings)
            ))
        case "State":
            event = .state(try DaemonState(from: decoder))
        case "BreakStarted":
            event = .breakStarted(
                text: try c.decode(String.self, forKey: .text),
                remainingS: try c.decode(Int.self, forKey: .remainingS)
            )
        case "BreakEnded":
            event = .breakEnded(reason: try c.decode(BreakEndReason.self, forKey: .reason))
        case "MeetingDetected":
            event = .meetingDetected
        case "MeetingEnded":
            event = .meetingEnded
        default:
            throw DecodingError.dataCorruptedError(forKey: .type, in: c, debugDescription: "unknown event \(type)")
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(v, forKey: .v)
        switch event {
        case .reply(let reply):
            try c.encode("Reply", forKey: .type)
            try c.encode(reply.id, forKey: .id)
            try c.encode(reply.ok, forKey: .ok)
            try c.encodeIfPresent(reply.error, forKey: .error)
            try c.encodeIfPresent(reply.state, forKey: .state)
            try c.encodeIfPresent(reply.settings, forKey: .settings)
        case .state(let state):
            try c.encode("State", forKey: .type)
            try state.encode(to: encoder)
        case .breakStarted(let text, let remaining):
            try c.encode("BreakStarted", forKey: .type)
            try c.encode(text, forKey: .text)
            try c.encode(remaining, forKey: .remainingS)
        case .breakEnded(let reason):
            try c.encode("BreakEnded", forKey: .type)
            try c.encode(reason, forKey: .reason)
        case .meetingDetected:
            try c.encode("MeetingDetected", forKey: .type)
        case .meetingEnded:
            try c.encode("MeetingEnded", forKey: .type)
        }
    }
}

enum ClientRequest: Equatable {
    case getState
    case getSettings
    case subscribe
    case updateSettings(KairosSettings)
    case pause(preset: PausePreset, source: PauseSource)
    case resume
    case postponeBreak
    case restartCapture
    case shutdown
}

struct ClientMessage: Codable, Equatable {
    var v: Int = protocolVersion
    var id: UInt64?
    var request: ClientRequest

    enum CodingKeys: String, CodingKey {
        case v, id, type, settings, preset, source
    }

    init(id: UInt64?, request: ClientRequest) {
        self.id = id
        self.request = request
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        v = try c.decode(Int.self, forKey: .v)
        id = try c.decodeIfPresent(UInt64.self, forKey: .id)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "GetState": request = .getState
        case "GetSettings": request = .getSettings
        case "Subscribe": request = .subscribe
        case "UpdateSettings": request = .updateSettings(try c.decode(KairosSettings.self, forKey: .settings))
        case "Pause":
            request = .pause(
                preset: try c.decode(PausePreset.self, forKey: .preset),
                source: try c.decode(PauseSource.self, forKey: .source)
            )
        case "Resume": request = .resume
        case "PostponeBreak": request = .postponeBreak
        case "RestartCapture": request = .restartCapture
        case "Shutdown": request = .shutdown
        default:
            throw DecodingError.dataCorruptedError(forKey: .type, in: c, debugDescription: "unknown request \(type)")
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(v, forKey: .v)
        try c.encodeIfPresent(id, forKey: .id)
        switch request {
        case .getState: try c.encode("GetState", forKey: .type)
        case .getSettings: try c.encode("GetSettings", forKey: .type)
        case .subscribe: try c.encode("Subscribe", forKey: .type)
        case .updateSettings(let settings):
            try c.encode("UpdateSettings", forKey: .type)
            try c.encode(settings, forKey: .settings)
        case .pause(let preset, let source):
            try c.encode("Pause", forKey: .type)
            try c.encode(preset, forKey: .preset)
            try c.encode(source, forKey: .source)
        case .resume: try c.encode("Resume", forKey: .type)
        case .postponeBreak: try c.encode("PostponeBreak", forKey: .type)
        case .restartCapture: try c.encode("RestartCapture", forKey: .type)
        case .shutdown: try c.encode("Shutdown", forKey: .type)
        }
    }
}

enum NDJSON {
    static func encode<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        var data = try encoder.encode(value)
        data.append(0x0A)
        return data
    }

    static func splitLines(_ buffer: inout Data) -> [Data] {
        var lines: [Data] = []
        while let index = buffer.firstIndex(of: 0x0A) {
            let line = buffer[buffer.startIndex..<index]
            buffer.removeSubrange(buffer.startIndex...index)
            if !line.isEmpty {
                lines.append(Data(line))
            }
        }
        return lines
    }
}
