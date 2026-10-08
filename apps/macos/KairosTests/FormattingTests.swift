import XCTest
@testable import Kairos

final class FormattingTests: XCTestCase {
    func testMenuBarTitle() {
        XCTAssertEqual(Formatting.menuBarTitle(state: makeState(load: 50), connected: true), "50%")
        XCTAssertEqual(Formatting.menuBarTitle(state: makeState(load: 49.9999), connected: true), "50%")
        XCTAssertEqual(Formatting.menuBarTitle(state: makeState(load: 99.97), connected: true), "99%")
        XCTAssertEqual(Formatting.menuBarTitle(state: makeState(phase: .paused, load: 80), connected: true), "⏸ 80%")
        XCTAssertEqual(Formatting.menuBarTitle(state: makeState(), connected: false), "–%")
    }

    func testRedOnlyWhenPostponedAtFull() {
        XCTAssertTrue(Formatting.isAlert(state: makeState(load: 100, postponed: true)))
        XCTAssertFalse(Formatting.isAlert(state: makeState(load: 40, postponed: true)))
        XCTAssertFalse(Formatting.isAlert(state: makeState(load: 100)))
    }

    func testCountdowns() {
        XCTAssertEqual(Formatting.countdown(600), "10:00")
        XCTAssertEqual(Formatting.countdown(59), "00:59")
        XCTAssertEqual(Formatting.countdown(1500), "25:00")
        XCTAssertEqual(Formatting.shortCountdown(300), "5:00")
        XCTAssertEqual(Formatting.hoursMinutes(3_900), "1:05")
        XCTAssertEqual(Formatting.hoursMinutes(59), "0:00")
    }

    func testStatusLines() {
        let utc = TimeZone(identifier: "UTC")!
        XCTAssertEqual(Formatting.statusLine(state: makeState(breakIn: 1500), connected: true), "Break in 25:00")
        XCTAssertEqual(Formatting.statusLine(state: makeState(load: 100, breakIn: 300, postponed: true), connected: true), "Break postponed · 5:00")
        XCTAssertEqual(Formatting.statusLine(state: makeState(phase: .paused, breakIn: nil, pausedUntil: 1_767_603_600), connected: true, timeZone: utc), "Paused until 09:00")
        XCTAssertEqual(Formatting.statusLine(state: makeState(phase: .onBreak, breakIn: nil, remaining: 240), connected: true), "On break · 04:00 left")
        XCTAssertEqual(Formatting.statusLine(state: nil, connected: false), "Disconnected — reconnecting…")
        XCTAssertEqual(Formatting.todayLine(makeState()), "Today 1:05")
    }
}
