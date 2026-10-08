import SwiftUI

@main
struct KairosApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        MenuBarExtra {
            MenuContent(model: AppModel.shared, store: AppModel.shared.store)
        } label: {
            MenuBarLabel(store: AppModel.shared.store)
        }
        .menuBarExtraStyle(.menu)

        Settings {
            SettingsView(model: AppModel.shared, store: AppModel.shared.store, draft: AppModel.shared.settingsDraft)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        MainActor.assumeIsolated {
            AppModel.shared.start()
        }
    }
}
