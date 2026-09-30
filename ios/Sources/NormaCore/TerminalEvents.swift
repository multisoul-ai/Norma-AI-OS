import Foundation

struct TerminalEvent: Decodable, Sendable {
    let streamID: String
    let sequence: UInt64
    let request: TerminalRequest
    let page: TerminalPage?
    let replaceFrom: Int
    let removeCount: Int
    enum CodingKeys: String, CodingKey { case streamID = "stream_id", sequence, request, page, replaceFrom = "replace_from", removeCount = "remove_count" }
}
struct TerminalTimeline: Sendable {
    let request: TerminalRequest
    private var streamID: String?
    private var sequence: UInt64 = 0
    private(set) var page: TerminalPage?
    init(request: TerminalRequest) { self.request = request }
    mutating func apply(_ event: TerminalEvent) throws -> TerminalPage? {
        do {
            guard validID(event.streamID), request == event.request else { throw MobileError.sessionChanged }
            if let streamID {
                guard streamID == event.streamID else { throw MobileError.eventGap }
                if event.page == nil {
                    guard event.sequence == sequence, page != nil, event.replaceFrom == 0, event.removeCount == 0 else { throw MobileError.eventGap }
                    return nil
                }
                guard sequence < UInt64.max, event.sequence == sequence + 1 else { throw MobileError.eventGap }
            } else {
                guard event.sequence == 0, event.replaceFrom == 0, event.removeCount == 0 else { throw MobileError.eventGap }
            }
            guard var next = event.page else { throw MobileError.eventGap }
            if let page {
                guard event.replaceFrom >= 0, event.replaceFrom <= page.rows.count,
                      event.removeCount >= 0, event.removeCount <= page.rows.count - event.replaceFrom,
                      next.rows.count <= 120, next.revision >= page.revision, next.inputSequence >= page.inputSequence else { throw MobileError.eventGap }
                var rows = page.rows
                rows.replaceSubrange(event.replaceFrom..<(event.replaceFrom + event.removeCount), with: next.rows)
                next.rows = rows
            }
            try next.validate(for: request)
            page = next; streamID = event.streamID; sequence = event.sequence
            return next
        } catch { page = nil; streamID = nil; sequence = 0; throw error }
    }
}
public enum TerminalWatchUpdate: Sendable {
    case current(TerminalPage), heartbeat, recovering
}
extension MobileClient {
    public func terminalUpdates(_ machine: PairedMachine, request: TerminalRequest) -> AsyncThrowingStream<TerminalWatchUpdate, any Error> {
        AsyncThrowingStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let work = Task {
                do {
                    try machine.validate(); try request.validate()
                    guard request.before == nil else { throw MobileError.invalidResponse }
                    var attempts = 0
                    while !Task.isCancelled {
                        do {
                            guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
                            let connection = try await PinnedConnection.open(machine, streaming: true)
                            defer { connection.close() }
                            let id = UUID().uuidString
                            let scope = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
                            let body = try JSONSerialization.data(withJSONObject: ["version": 1, "request_id": id, "machine_id": machine.id,
                                "action": ["type": "terminal", "watch": true, "device_id": credential.deviceID, "token": credential.token, "request": scope]])
                            var data = try await connection.exchange(body)
                            if WireReply.rejectsNewRead(data) { throw MobileError.terminalUnavailable }
                            var timeline = TerminalTimeline(request: request)
                            while !Task.isCancelled {
                                let reply = try WireReply.decode(data, expectedID: id)
                                guard reply.result?.status == "terminal_event", let event = reply.result?.terminalEvent else { throw MobileError.invalidResponse }
                                if let page = try timeline.apply(event) { continuation.yield(.current(page)) }
                                // Do not let a heartbeat replace a buffered full screen while UI is busy.
                                data = try await connection.receive()
                            }
                        } catch {
                            try Task.checkCancellation()
                            let retryable: [MobileError] = [.eventGap, .connectionFailed, .timedOut, .unavailable, .grantChanged]
                            guard attempts < 3, (error as? MobileError).map(retryable.contains) ?? (error is URLError) else { throw error }
                            continuation.yield(.recovering)
                            //Three bounded read reconnects after1+2+4=7s. Never resend terminal input.
                            try await Task.sleep(for: .seconds(1 << attempts)); attempts += 1
                        }
                    }
                    continuation.finish()
                } catch { continuation.finish(throwing: error) }
            }
            continuation.onTermination = { _ in work.cancel() }
        }
    }
}
