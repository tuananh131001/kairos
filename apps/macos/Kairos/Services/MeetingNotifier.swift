import Foundation
import UserNotifications

final class MeetingNotifier: NSObject, UNUserNotificationCenterDelegate {
    static let category = "KAIROS_MEETING"
    static let notificationId = "kairos.meeting"

    var onPause: ((PausePreset) -> Void)?
    var onAuthorizationChange: ((Bool) -> Void)?
    private(set) var authorized = false

    private var center: UNUserNotificationCenter? {
        Bundle.main.bundleIdentifier == nil ? nil : UNUserNotificationCenter.current()
    }

    private static let actions: [(String, String, PausePreset)] = [
        ("PAUSE_30M", "Pause 30 min", .m30),
        ("PAUSE_2H", "Pause 2 h", .h2),
        ("PAUSE_4H", "Pause 4 h", .h4),
    ]

    func setup() {
        guard let center else {
            report(false)
            return
        }
        center.delegate = self
        let actions = Self.actions.map { UNNotificationAction(identifier: $0.0, title: $0.1, options: []) }
        let category = UNNotificationCategory(identifier: Self.category, actions: actions, intentIdentifiers: [], options: [])
        center.setNotificationCategories([category])
        center.requestAuthorization(options: [.alert, .sound]) { [weak self] _, _ in
            self?.refresh()
        }
    }

    func refresh(then action: (() -> Void)? = nil) {
        guard let center else {
            report(false)
            action?()
            return
        }
        center.getNotificationSettings { [weak self] settings in
            let ok = settings.authorizationStatus == .authorized || settings.authorizationStatus == .provisional
            DispatchQueue.main.async {
                self?.report(ok)
                action?()
            }
        }
    }

    func meetingDetected() {
        refresh { [weak self] in
            guard let self, self.authorized, let center = self.center else { return }
            let content = UNMutableNotificationContent()
            content.title = "Meeting detected"
            content.body = "Your mic or camera is on. Pause break reminders?"
            content.categoryIdentifier = Self.category
            let request = UNNotificationRequest(identifier: Self.notificationId, content: content, trigger: nil)
            center.add(request)
        }
    }

    func meetingEnded() {
        center?.removeDeliveredNotifications(withIdentifiers: [Self.notificationId])
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping () -> Void) {
        if let preset = Self.actions.first(where: { $0.0 == response.actionIdentifier })?.2 {
            DispatchQueue.main.async { self.onPause?(preset) }
        }
        completionHandler()
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        completionHandler([.banner, .list])
    }

    private func report(_ value: Bool) {
        authorized = value
        onAuthorizationChange?(value)
    }
}
