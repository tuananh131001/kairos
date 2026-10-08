import XCTest
@testable import Kairos

func makeState(
    phase: EnginePhase = .running,
    load: Double = 40,
    breakIn: Int? = 1800,
    remaining: Int? = nil,
    postponed: Bool = false,
    pausedUntil: Int64? = nil,
    meeting: Bool = false,
    permission: PermissionStatus = .granted
) -> DaemonState {
    DaemonState(
        phase: phase,
        activity: phase == .paused ? .paused : .active,
        loadPercent: load,
        breakInS: breakIn,
        breakRemainingS: remaining,
        postponed: postponed,
        pausedUntil: pausedUntil,
        todayWorkedS: 3_900,
        permission: permission,
        meetingActive: meeting
    )
}

final class DaemonStoreTests: XCTestCase {
    func testBreakStartedShowsOverlayAndEndedHidesIt() {
        let store = DaemonStore()
        store.apply(.state(makeState()))
        store.apply(.breakStarted(text: "Drink water 💧", remainingS: 600))
        XCTAssertTrue(store.overlayVisible)
        XCTAssertEqual(store.breakText, "Drink water 💧")
        XCTAssertEqual(store.overlayRemaining, 600)
        store.apply(.state(makeState(phase: .onBreak, load: 90, breakIn: nil, remaining: 540)))
        XCTAssertEqual(store.overlayRemaining, 540)
        store.apply(.breakEnded(reason: .completed))
        XCTAssertFalse(store.overlayVisible)
    }

    func testStateLeavingBreakHidesOverlay() {
        let store = DaemonStore()
        store.apply(.breakStarted(text: "Rest", remainingS: 600))
        store.apply(.state(makeState(postponed: true)))
        XCTAssertFalse(store.overlayVisible)
    }

    func testMeetingFallbackNeedsDeniedNotifications() {
        let store = DaemonStore()
        store.apply(.state(makeState(meeting: true)))
        XCTAssertTrue(store.meetingNudge)
        XCTAssertFalse(store.showMeetingBadge)
        store.notificationsDenied = true
        XCTAssertTrue(store.showMeetingBadge)
        XCTAssertTrue(store.showMeetingFallback)
        store.apply(.state(makeState(phase: .paused, meeting: true)))
        XCTAssertTrue(store.showMeetingBadge)
        XCTAssertFalse(store.showMeetingFallback)
        store.apply(.meetingEnded)
        XCTAssertFalse(store.showMeetingBadge)
    }

    func testSettingsReplyErrorsAreMappedToFields() {
        let store = DaemonStore()
        let failed = Reply(id: 1, ok: false, error: ErrorBody(code: "validation", message: "Invalid settings.", fields: [FieldError(field: "work_minutes", message: "bad")]), state: nil, settings: nil)
        XCTAssertFalse(store.applySettingsReply(failed))
        XCTAssertEqual(store.settingsErrors["work_minutes"], "bad")
        let ok = Reply(id: 2, ok: true, error: nil, state: nil, settings: .defaults)
        XCTAssertTrue(store.applySettingsReply(ok))
        XCTAssertTrue(store.settingsErrors.isEmpty)
        XCTAssertEqual(store.settings, .defaults)
    }

    func testDisconnectHidesOverlay() {
        let store = DaemonStore()
        store.apply(.breakStarted(text: "Rest", remainingS: 600))
        store.setConnected(false)
        XCTAssertFalse(store.overlayVisible)
    }

    func testMenuSnapshotFollowsStateWhileClosed() {
        let store = DaemonStore()
        store.setConnected(true)
        store.apply(.state(makeState(breakIn: 1800)))
        XCTAssertEqual(store.menu.status, "Break in 30:00")
        store.apply(.state(makeState(breakIn: 1799)))
        XCTAssertEqual(store.menu.status, "Break in 29:59")
        XCTAssertEqual(store.label.title, "40%")
    }

    func testMenuSnapshotFreezesWhileOpenAndCatchesUpOnClose() {
        let store = DaemonStore()
        store.setConnected(true)
        store.apply(.state(makeState(load: 40, breakIn: 1800)))
        store.menuOpen = true
        store.apply(.state(makeState(phase: .paused, load: 41, breakIn: nil, pausedUntil: 0)))
        XCTAssertEqual(store.menu.status, "Break in 30:00")
        XCTAssertFalse(store.menu.canResume)
        XCTAssertEqual(store.label.title, "40%")
        store.menuOpen = false
        XCTAssertTrue(store.menu.canResume)
        XCTAssertEqual(store.label.title, "⏸ 41%")
    }

    func testSnapshotsIgnoreSubPercentLoadChanges() {
        let store = DaemonStore()
        store.setConnected(true)
        store.apply(.state(makeState(load: 40.2)))
        let label = store.label
        store.apply(.state(makeState(load: 40.7)))
        XCTAssertEqual(store.label, label)
        XCTAssertEqual(store.label.progress, 0.4, accuracy: 0.0001)
    }

    func testBackoffDoublesToFiveSeconds() {
        var backoff = Backoff()
        XCTAssertEqual((0..<6).map { _ in backoff.next() }, [0.5, 1, 2, 4, 5, 5])
        backoff.reset()
        XCTAssertEqual(backoff.next(), 0.5)
    }
}
