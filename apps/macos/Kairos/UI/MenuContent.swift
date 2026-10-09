import SwiftUI

struct MenuContent: View {
    let model: AppModel
    let store: DaemonStore

    private let meetingPresets: [PausePreset] = [.m30, .h2, .h4]

    var body: some View {
        let menu = store.menu
        if menu.meetingFallback {
            Text("🎙 Meeting detected")
            ForEach(meetingPresets, id: \.self) { preset in
                Button("Pause \(Formatting.presetTitle(preset))") {
                    model.pause(preset, source: .meetingNotification)
                }
            }
            Divider()
        }
        Text(menu.status)
        Text(menu.activity)
        Text(menu.today)
        Divider()
        Menu("Pause for…") {
            ForEach(PausePreset.allCases, id: \.self) { preset in
                Button(Formatting.presetTitle(preset)) {
                    model.pause(preset, source: .menu)
                }
            }
        }
        .disabled(!menu.connected)
        Button("Resume") {
            model.resume()
        }
        .disabled(!menu.canResume)
        Divider()
        SettingsLink {
            Text("Settings…")
        }
        .keyboardShortcut(",")
        Button("Quit Kairos") {
            model.quit()
        }
        .keyboardShortcut("q")
    }
}

struct MenuBarLabel: View {
    let store: DaemonStore

    var body: some View {
        let label = store.label
        Image(nsImage: MenuBarIcon.render(
            title: label.title,
            progress: label.progress,
            alert: label.alert,
            badge: label.badge
        ))
    }
}

enum MenuBarIcon {
    static func render(title: String, progress: Double, alert: Bool, badge: Bool) -> NSImage {
        let text = badge ? "🎙 \(title)" : title
        let color: NSColor = alert ? .systemRed : .black
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 13, weight: .medium),
            .foregroundColor: color,
        ]
        let string = NSAttributedString(string: text, attributes: attributes)
        let textSize = string.size()
        let ring: CGFloat = 14
        let gap: CGFloat = 4
        let height: CGFloat = 18
        let size = NSSize(width: ceil(ring + gap + textSize.width), height: height)
        let image = NSImage(size: size, flipped: false) { _ in
            let rect = NSRect(x: 1, y: (height - ring) / 2 + 1, width: ring - 2, height: ring - 2)
            color.withAlphaComponent(0.3).setStroke()
            let track = NSBezierPath(ovalIn: rect)
            track.lineWidth = 2
            track.stroke()
            let clamped = min(1, max(0, progress))
            if clamped > 0 {
                color.setStroke()
                let arc = NSBezierPath()
                let center = NSPoint(x: rect.midX, y: rect.midY)
                arc.appendArc(withCenter: center, radius: rect.width / 2, startAngle: 90, endAngle: 90 - 360 * clamped, clockwise: true)
                arc.lineWidth = 2
                arc.stroke()
            }
            string.draw(at: NSPoint(x: ring + gap, y: (height - textSize.height) / 2))
            return true
        }
        image.isTemplate = !alert
        return image
    }
}
