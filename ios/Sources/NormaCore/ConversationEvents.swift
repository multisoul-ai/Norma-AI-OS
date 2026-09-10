import Foundation

struct ConversationEvent: Decodable, Sendable {
    let streamID: String
    let sequence: UInt64
    let request: ConversationRequest
    let page: ConversationPage?
    enum CodingKeys: String, CodingKey { case streamID = "stream_id", sequence, request, page }
}

struct ConversationEventSequence: Sendable {
    let request: ConversationRequest
    private var streamID: String?
    private var sequence: UInt64 = 0
    private(set) var page: ConversationPage?
    init(request: ConversationRequest) { self.request = request }
    mutating func apply(_ event: ConversationEvent) throws -> ConversationPage? {
        do {
            guard validID(event.streamID), event.request == request else { throw MobileError.sessionChanged }
            if let streamID {
                guard streamID == event.streamID else { throw MobileError.eventGap }
                if event.page == nil {
                    guard event.sequence == sequence, page != nil else { throw MobileError.eventGap }
                    return nil
                }
                guard sequence < UInt64.max, event.sequence == sequence + 1 else { throw MobileError.eventGap }
            } else { guard event.sequence == 0, event.page != nil else { throw MobileError.eventGap } }
            guard let next = event.page else { throw MobileError.eventGap }
            try ConversationResponse(request: request, page: next).validate(for: request)
            if let page, page.direction != next.direction { throw MobileError.invalidResponse }
            streamID = event.streamID; sequence = event.sequence; page = next
            return next
        } catch { streamID = nil; page = nil; sequence = 0; throw error }
    }
}

public enum ConversationWatchUpdate: Sendable {
    case current(ConversationPage)
    /// Validated keepalive with no content change; consumers must not re-merge cached history.
    case heartbeat
    case recovering(String)
}

extension MobileClient {
    public func conversationUpdates(_ machine: PairedMachine, request: ConversationRequest) -> AsyncThrowingStream<ConversationWatchUpdate, any Error> {
        let reader = ConversationWatchReader(machine: machine, request: request, vault: vault)
        // Native unfolding reads only as the consumer advances; no queue can silently drop Claude pages.
        return AsyncThrowingStream(unfolding: { try await reader.next() })
    }
}

private actor ConversationWatchReader {
    let machine: PairedMachine
    let request: ConversationRequest
    let vault: CredentialVault
    var connection: PinnedConnection?
    var requestID = ""
    var state: ConversationEventSequence
    var attempts = 0
    var reading = false
    var finished = false
    init(machine: PairedMachine, request: ConversationRequest, vault: CredentialVault) {
        self.machine = machine; self.request = request; self.vault = vault
        state = ConversationEventSequence(request: request)
    }
    deinit { connection?.close() }
    func next() async throws -> ConversationWatchUpdate? {
        guard !finished else { return nil }
        guard !reading else { throw MobileError.invalidResponse }
        reading = true
        defer { reading = false }
        do {
            try Task.checkCancellation(); try machine.validate(); try request.validate()
            guard request.cursor == nil else { throw MobileError.invalidResponse }
            var data: Data
            if let connection { data = try await connection.receive() }
            else {
                // At most three read-only reconnects, after1+2+4=7s total backoff.
                if attempts > 0 { try await Task.sleep(for: .seconds(1 << (attempts - 1))) }
                guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
                let opened = try await PinnedConnection.open(machine, streaming: true)
                connection = opened; requestID = UUID().uuidString
                let target = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
                let body = try JSONSerialization.data(withJSONObject: ["version": 1, "request_id": requestID, "machine_id": machine.id,
                    "action": ["type": "watch_conversation", "device_id": credential.deviceID, "token": credential.token, "request": target]])
                data = try await opened.exchange(body)
            }
            var transfer = ConversationTransfer()
            var fragmented = false
            let event: ConversationEvent
            while true {
                let reply = try WireReply.decode(data, expectedID: requestID)
                if !fragmented, reply.result?.status == "conversation_event", let value = reply.result?.conversationEvent { event = value; break }
                guard reply.result?.status == "conversation_fragment", let part = reply.result?.fragment else { throw MobileError.invalidResponse }
                fragmented = true
                if let value = try transfer.append(part, as: ConversationEvent.self) { event = value; break }
                guard let connection else { throw MobileError.connectionFailed }
                data = try await connection.receive()
            }
            if let page = try state.apply(event) { return .current(page) }
            return .heartbeat
        } catch {
            connection?.close(); connection = nil
            state = ConversationEventSequence(request: request)
            if Task.isCancelled { finished = true; throw CancellationError() }
            let retryable: [MobileError] = [.connectionFailed, .timedOut, .unavailable, .eventGap, .grantChanged, .historyUnavailable]
            guard attempts < 3, (error as? MobileError).map(retryable.contains) ?? (error is URLError) else { finished = true; throw error }
            attempts += 1
            return .recovering(error.localizedDescription)
        }
    }
}
