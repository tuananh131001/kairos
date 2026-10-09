import XCTest
@testable import Kairos

final class ProtocolFixtureTests: XCTestCase {
    private var fixturesURL: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .appendingPathComponent("../../../protocol/fixtures")
            .standardizedFileURL
    }

    private func fixtures() throws -> [(String, Data)] {
        let names = try FileManager.default.contentsOfDirectory(atPath: fixturesURL.path)
            .filter { $0.hasSuffix(".json") }
            .sorted()
        XCTAssertGreaterThanOrEqual(names.count, 18)
        return try names.map { ($0, try Data(contentsOf: fixturesURL.appendingPathComponent($0))) }
    }

    private func object(_ data: Data) throws -> NSDictionary {
        try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? NSDictionary)
    }

    func testFixturesRoundTrip() throws {
        for (name, data) in try fixtures() {
            let reencoded: Data
            if name.hasPrefix("request_") {
                reencoded = try JSONEncoder().encode(JSONDecoder().decode(ClientMessage.self, from: data))
            } else {
                reencoded = try JSONEncoder().encode(JSONDecoder().decode(ServerMessage.self, from: data))
            }
            XCTAssertEqual(try object(data), try object(reencoded), name)
        }
    }

    func testDecodesStateFixture() throws {
        let data = try Data(contentsOf: fixturesURL.appendingPathComponent("event_state_paused.json"))
        let message = try JSONDecoder().decode(ServerMessage.self, from: data)
        guard case .state(let state) = message.event else {
            return XCTFail("expected state")
        }
        XCTAssertEqual(state.phase, .paused)
        XCTAssertEqual(state.loadPercent, 80)
        XCTAssertEqual(state.pausedUntil, 1_767_603_600)
        XCTAssertEqual(state.permission, .denied)
        XCTAssertTrue(state.meetingActive)
    }

    func testNDJSONFraming() throws {
        var buffer = Data("{\"a\":1}\n{\"b\":2}\n{\"c\"".utf8)
        let lines = NDJSON.splitLines(&buffer)
        XCTAssertEqual(lines.count, 2)
        XCTAssertEqual(String(decoding: buffer, as: UTF8.self), "{\"c\"")
        let encoded = try NDJSON.encode(ClientMessage(id: 3, request: .resume))
        XCTAssertEqual(encoded.last, 0x0A)
    }
}
