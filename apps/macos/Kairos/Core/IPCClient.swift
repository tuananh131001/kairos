import Foundation
import Network

final class IPCClient {
    static var defaultSocketPath: String {
        if let override = ProcessInfo.processInfo.environment["KAIROS_SOCKET"] {
            return override
        }
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return support.appendingPathComponent("Kairos/kairosd.sock").path
    }

    var onMessage: ((ServerEvent) -> Void)?
    var onConnectionChange: ((Bool) -> Void)?

    private let path: String
    private let queue: DispatchQueue
    private var connection: NWConnection?
    private var buffer = Data()
    private var nextId: UInt64 = 1
    private var pending: [UInt64: (Reply) -> Void] = [:]
    private var backoff = Backoff()
    private var stopped = true
    private var reconnectWork: DispatchWorkItem?
    private(set) var isConnected = false

    init(path: String = IPCClient.defaultSocketPath, queue: DispatchQueue = .main) {
        self.path = path
        self.queue = queue
    }

    func start() {
        queue.async {
            self.stopped = false
            self.connect()
        }
    }

    func stop() {
        queue.async {
            self.stopped = true
            self.reconnectWork?.cancel()
            self.connection?.cancel()
            self.connection = nil
        }
    }

    func send(_ request: ClientRequest, completion: ((Reply) -> Void)? = nil) {
        queue.async {
            guard let connection = self.connection, self.isConnected else {
                completion?(Reply(id: nil, ok: false, error: ErrorBody(code: "disconnected", message: "Kairos daemon is not running.", fields: nil)))
                return
            }
            let id = self.nextId
            self.nextId += 1
            if let completion {
                self.pending[id] = completion
            }
            do {
                let data = try NDJSON.encode(ClientMessage(id: id, request: request))
                connection.send(content: data, completion: .contentProcessed { _ in })
            } catch {
                self.pending.removeValue(forKey: id)?(Reply(id: id, ok: false, error: ErrorBody(code: "encode", message: error.localizedDescription, fields: nil)))
            }
        }
    }

    private func connect() {
        guard !stopped else { return }
        buffer.removeAll()
        let connection = NWConnection(to: .unix(path: path), using: .tcp)
        self.connection = connection
        connection.stateUpdateHandler = { [weak self, weak connection] state in
            guard let self, let connection, connection === self.connection else { return }
            switch state {
            case .ready:
                self.backoff.reset()
                self.setConnected(true)
                self.receive(on: connection)
                self.send(.subscribe) { reply in
                    if let state = reply.state {
                        self.onMessage?(.state(state))
                    }
                }
                self.send(.getSettings) { reply in
                    self.onMessage?(.reply(reply))
                }
            case .failed, .waiting:
                self.handleDisconnect(connection)
            case .cancelled:
                self.handleDisconnect(connection)
            default:
                break
            }
        }
        connection.start(queue: queue)
    }

    private func receive(on connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] data, _, isComplete, error in
            guard let self, connection === self.connection else { return }
            if let data, !data.isEmpty {
                self.buffer.append(data)
                for line in NDJSON.splitLines(&self.buffer) {
                    self.dispatch(line)
                }
            }
            if isComplete || error != nil {
                self.handleDisconnect(connection)
            } else {
                self.receive(on: connection)
            }
        }
    }

    private func dispatch(_ line: Data) {
        guard let message = try? JSONDecoder().decode(ServerMessage.self, from: line) else { return }
        if case .reply(let reply) = message.event, let id = reply.id, let handler = pending.removeValue(forKey: id) {
            handler(reply)
            return
        }
        onMessage?(message.event)
    }

    private func handleDisconnect(_ connection: NWConnection) {
        guard connection === self.connection else { return }
        connection.stateUpdateHandler = nil
        connection.cancel()
        self.connection = nil
        setConnected(false)
        let failed = pending
        pending.removeAll()
        for (id, handler) in failed {
            handler(Reply(id: id, ok: false, error: ErrorBody(code: "disconnected", message: "Connection lost.", fields: nil)))
        }
        guard !stopped else { return }
        let work = DispatchWorkItem { [weak self] in self?.connect() }
        reconnectWork = work
        queue.asyncAfter(deadline: .now() + backoff.next(), execute: work)
    }

    private func setConnected(_ connected: Bool) {
        guard connected != isConnected else { return }
        isConnected = connected
        onConnectionChange?(connected)
    }
}

struct Backoff {
    private(set) var current: TimeInterval = 0.5
    let initial: TimeInterval = 0.5
    let maximum: TimeInterval = 5

    mutating func next() -> TimeInterval {
        let value = current
        current = min(current * 2, maximum)
        return value
    }

    mutating func reset() {
        current = initial
    }
}
