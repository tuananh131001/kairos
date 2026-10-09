import SwiftUI

@Observable
final class SettingsDraft {
    var settings = KairosSettings.defaults
    var breakMinutes = 10
    var breakSeconds = 0
    var loaded = false
    var saving = false
    var status: String?
    var launchAtLogin = LoginItems.launchAtLoginEnabled
}

struct SettingsView: View {
    let model: AppModel
    let store: DaemonStore
    @Bindable var draft: SettingsDraft

    private var reminderCount: Int { draft.settings.reminderText.count }

    var body: some View {
        Form {
            Section("Schedule") {
                Stepper(value: $draft.settings.workMinutes, in: 1...240) {
                    LabeledContent("Work duration", value: "\(draft.settings.workMinutes) min")
                }
                fieldError("work_minutes")
                LabeledContent("Break duration") {
                    HStack(spacing: 4) {
                        Picker("Minutes", selection: $draft.breakMinutes) {
                            ForEach(0...60, id: \.self) { Text("\($0) min").tag($0) }
                        }
                        .labelsHidden()
                        .frame(width: 90)
                        Picker("Seconds", selection: $draft.breakSeconds) {
                            ForEach(0..<60, id: \.self) { Text(String(format: "%02d s", $0)).tag($0) }
                        }
                        .labelsHidden()
                        .frame(width: 80)
                    }
                }
                fieldError("break_seconds")
            }
            Section("Break reminder") {
                TextField("Reminder text", text: $draft.settings.reminderText, axis: .vertical)
                    .lineLimit(2...4)
                HStack {
                    Spacer()
                    Text("\(reminderCount)/200")
                        .font(.caption)
                        .foregroundStyle(reminderCount > 200 || reminderCount == 0 ? Color.red : Color.secondary)
                }
                fieldError("reminder_text")
            }
            Section("Meetings") {
                Toggle("Offer a pause when the mic or camera is in use", isOn: $draft.settings.meetingDetectionEnabled)
                LabeledContent("Notify after mic/camera is on for", value: Formatting.delayLabel(draft.settings.meetingNotifyDelaySeconds))
                Slider(
                    value: Binding(
                        get: { Double(draft.settings.meetingNotifyDelaySeconds) },
                        set: { draft.settings.meetingNotifyDelaySeconds = Int($0) }
                    ),
                    in: 10...300,
                    step: 5
                )
                .disabled(!draft.settings.meetingDetectionEnabled)
                fieldError("meeting_notify_delay_seconds")
            }
            Section("General") {
                Toggle("Launch Kairos at login", isOn: $draft.launchAtLogin)
                    .disabled(!LoginItems.isBundled)
                    .onChange(of: draft.launchAtLogin) { _, enabled in
                        LoginItems.setLaunchAtLogin(enabled)
                    }
            }
            if let message = store.settingsErrors["form"] {
                Text(message).foregroundStyle(.red)
            }
            HStack {
                if let status = draft.status {
                    Text(status).foregroundStyle(.secondary)
                }
                if !store.connected {
                    Text("Daemon disconnected").foregroundStyle(.red)
                }
                Spacer()
                Button("Revert") { load(force: true) }
                    .disabled(store.settings == nil)
                Button("Save") { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!store.connected || draft.saving)
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
        .onAppear {
            load(force: true)
            NSApp.activate(ignoringOtherApps: true)
        }
        .onChange(of: store.settings) { _, _ in
            load(force: false)
        }
    }

    @ViewBuilder
    private func fieldError(_ field: String) -> some View {
        if let message = store.settingsErrors[field] {
            Text(message)
                .font(.caption)
                .foregroundStyle(.red)
        }
    }

    private func load(force: Bool) {
        guard let settings = store.settings, force || !draft.loaded else { return }
        draft.settings = settings
        draft.breakMinutes = settings.breakSeconds / 60
        draft.breakSeconds = settings.breakSeconds % 60
        draft.loaded = true
        draft.status = nil
        store.settingsErrors = [:]
    }

    private func save() {
        var candidate = draft.settings
        candidate.breakSeconds = draft.breakMinutes * 60 + draft.breakSeconds
        draft.saving = true
        draft.status = nil
        model.save(candidate) { ok in
            draft.saving = false
            draft.status = ok ? "Saved" : nil
        }
    }
}
