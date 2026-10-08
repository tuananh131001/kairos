import Foundation
import ServiceManagement

enum LoginItems {
    static let agentPlistName = "com.kairos.daemon.plist"
    static let agentLabel = "com.kairos.daemon"
    private static let registeredKey = "didRegisterMainAppLoginItem"

    static var isBundled: Bool {
        Bundle.main.bundleURL.pathExtension == "app"
            && ProcessInfo.processInfo.environment["KAIROS_SKIP_LOGIN_ITEMS"] == nil
    }

    static func onLaunch() {
        guard isBundled else { return }
        let agent = SMAppService.agent(plistName: agentPlistName)
        if agent.status != .enabled {
            do {
                try agent.register()
            } catch {
                NSLog("Kairos: agent registration failed: \(error.localizedDescription)")
            }
        }
        if agent.status == .requiresApproval {
            SMAppService.openSystemSettingsLoginItems()
        }
        if !UserDefaults.standard.bool(forKey: registeredKey) {
            try? SMAppService.mainApp.register()
            UserDefaults.standard.set(true, forKey: registeredKey)
        }
        kickstartDaemon(restart: false)
    }

    static func kickstartDaemon(restart: Bool) {
        guard isBundled else { return }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        var arguments = ["kickstart"]
        if restart {
            arguments.append("-k")
        }
        arguments.append("gui/\(getuid())/\(agentLabel)")
        process.arguments = arguments
        try? process.run()
    }

    static var launchAtLoginEnabled: Bool {
        SMAppService.mainApp.status == .enabled
    }

    static func setLaunchAtLogin(_ enabled: Bool) {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
        } catch {
            NSLog("Kairos: login item update failed: \(error.localizedDescription)")
        }
    }
}
