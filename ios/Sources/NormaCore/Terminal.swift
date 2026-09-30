import Foundation

public struct TerminalRequest: Codable, Equatable, Sendable {
    public let workspaceID: String
    public let routeID: String
    public let nodeID: UInt64
    public let runID: String
    public let sessionID: String?
    public var before: Int?
    public var revision: UInt64?
    public init(workspaceID: String, routeID: String, nodeID: UInt64, runID: String, sessionID: String?) {
        self.workspaceID = workspaceID; self.routeID = routeID; self.nodeID = nodeID; self.runID = runID; self.sessionID = sessionID
    }
    enum CodingKeys: String, CodingKey { case workspaceID = "workspace_id", routeID = "route_id", nodeID = "node_id", runID = "run_id", sessionID = "session_id", before, revision }
    func validate() throws {
        guard nodeID > 0, [workspaceID, routeID, runID].allSatisfy(validID), sessionID.map(validID) ?? true,
              (before == nil) == (revision == nil), before.map({ (1...100_000).contains($0) }) ?? true else { throw MobileError.invalidResponse }
    }
}

public struct TerminalPage: Codable, Equatable, Sendable {
    public let card: CardSnapshot
    public let revision: UInt64
    public let screenSignature: UInt64
    public let inputSequence: UInt64
    public let startLine: Int
    public let totalLines: Int
    public let cols: Int
    public let running: Bool
    public let inputReady: Bool
    public var rows: [String]
    public let softWrapped: [Bool]?
    public var cursor: Cursor? = nil
    public struct Cursor: Codable, Equatable, Sendable {
        public let line: Int
        public let column: Int
    }
    enum CodingKeys: String, CodingKey { case cursor, card, revision, screenSignature = "screen_signature", inputSequence = "input_sequence", startLine = "start_line", totalLines = "total_lines", cols, running, inputReady = "input_ready", rows, softWrapped = "soft_wrapped" }

    /// Reflow only confirmed soft boundaries; a legacy screen keeps its original line breaks.
    public func text(lastRows: Int = 120) -> String {
        let start = rows.count - min(rows.count, max(0, lastRows))
        var text = ""
        for index in start..<rows.count {
            text += rows[index]
            if index < rows.count - 1 && !(softWrapped?.count == rows.count && softWrapped?[index] == true) { text += "\n" }
        }
        return text
    }
    func validate(for request: TerminalRequest) throws {
        try request.validate(); try card.validate()
        guard card.nodeID == request.nodeID, card.runID == request.runID,
              request.sessionID == nil || card.sessionID == request.sessionID else { throw MobileError.sessionChanged }
        if let cursor {
            guard request.before == nil, cursor.line >= startLine, cursor.line < totalLines,
                  cursor.column >= 0, cursor.column < cols else { throw MobileError.invalidResponse }
        }
        let end = request.before ?? totalLines
        guard (1...100_000).contains(totalLines), (1...512).contains(cols), end <= totalLines, end > 0,
              startLine == max(0, end - 120), rows.count == end - startLine,
              softWrapped == nil || softWrapped?.isEmpty == true || softWrapped?.count == rows.count,
              request.revision == nil || request.revision == revision,
              !inputReady || (running && request.before == nil && inputSequence < UInt64.max),
              rows.allSatisfy({ $0.unicodeScalars.count <= cols && !$0.unicodeScalars.contains(where: { $0.value < 32 || (127...159).contains($0.value) }) }) else { throw MobileError.invalidResponse }
    }
}
struct TerminalResponse: Decodable, Sendable { let request: TerminalRequest; let page: TerminalPage }

public enum TerminalInput: Codable, Sendable, Equatable {
    case text(String), enter, tab, escape, backspace, up, down, left, right, ctrlC
    enum CodingKeys: String, CodingKey { case type, text }
    public var label: String {
        switch self { case .text: "输入文字"; case .enter: "Enter"; case .tab: "Tab"; case .escape: "Esc"; case .backspace: "退格"; case .up: "↑"; case .down: "↓"; case .left: "←"; case .right: "→"; case .ctrlC: "Ctrl-C" }
    }
    public func validate() throws {
        if case let .text(text) = self {
            guard !text.isEmpty, text.utf8.count <= 4096, !text.unicodeScalars.contains(where: {
                ($0.value < 32 || (127...159).contains($0.value)) && $0.value != 9 && $0.value != 10
            }) else { throw MobileError.invalidResponse }
        }
    }
    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        switch try values.decode(String.self, forKey: .type) {
        case "text": self = .text(try values.decode(String.self, forKey: .text))
        case "enter": self = .enter
        case "tab": self = .tab
        case "escape": self = .escape
        case "backspace": self = .backspace
        case "up": self = .up
        case "down": self = .down
        case "left": self = .left
        case "right": self = .right
        case "ctrl_c": self = .ctrlC
        default: throw MobileError.invalidResponse
        }
        try validate()
    }
    public func encode(to encoder: any Encoder) throws {
        try validate()
        var values = encoder.container(keyedBy: CodingKeys.self)
        let type: String
        switch self {
        case let .text(text): type = "text"; try values.encode(text, forKey: .text)
        case .enter: type = "enter"
        case .tab: type = "tab"
        case .escape: type = "escape"
        case .backspace: type = "backspace"
        case .up: type = "up"
        case .down: type = "down"
        case .left: type = "left"
        case .right: type = "right"
        case .ctrlC: type = "ctrl_c"
        }
        try values.encode(type, forKey: .type)
    }
}

extension PendingMobileCommand {
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64,
                terminal: TerminalPage, input: TerminalInput, operationID: String = UUID().uuidString) throws {
        self.machineID = machineID; self.deviceID = deviceID
        guard terminal.inputReady else { throw MobileError.terminalChanged }
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision,
                          command: .terminalInput(expected: terminal.card, screenSignature: terminal.screenSignature, inputSequence: terminal.inputSequence, input: input))
        try validate()
    }
}
extension MobileClient {
    public func prepareTerminalInput(_ machine: PairedMachine, workspaceID: String, routeID: String,
                                     revision: UInt64, page: TerminalPage, input: TerminalInput) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID,
                                        routeID: routeID, revision: revision, terminal: page, input: input)
    }
    public func terminal(_ machine: PairedMachine, request: TerminalRequest) async throws -> TerminalPage {
        try request.validate(); try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let scope = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
        let response = try await exchange(machine, action: ["type": "terminal", "device_id": credential.deviceID,
                                                           "token": credential.token, "request": scope])
        guard response.result?.status == "terminal", let terminal = response.result?.terminal, terminal.request == request else { throw MobileError.invalidResponse }
        try terminal.page.validate(for: request)
        return terminal.page
    }
}
