import Foundation

public struct WorkspaceTerminalRequest: Codable, Equatable, Sendable {
    public let workspaceID: String
    public let target: WorkspaceAgentTarget
    public var knownVersion: String?
    public var before: Int?
    public var historyVersion: String?
    public var snapshotID: String?
    public var offset: Int?
    public init(workspaceID: String, target: WorkspaceAgentTarget) { self.workspaceID = workspaceID; self.target = target }
    enum CodingKeys: String, CodingKey {
        case workspaceID = "desktop_workspace_id", target, before, offset
        case knownVersion = "known_version", historyVersion = "history_version", snapshotID = "snapshot_id"
    }
    func validate() throws {
        guard validID(workspaceID), validID(target.areaID), target.definitionID > 0, target.nodeID > 0,
              target.runID.map(validID) == true, target.sessionID.map(validID) ?? true,
              (before == nil) == (historyVersion == nil), before.map({ (1...100_000).contains($0) }) ?? true,
              (snapshotID == nil) == (offset == nil), offset.map({ (1..<120).contains($0) }) ?? true,
              [knownVersion, historyVersion, snapshotID].compactMap({ $0 }).allSatisfy(validID) else { throw MobileError.invalidResponse }
    }
}
public struct WorkspaceTerminalCell: Codable, Equatable, Sendable {
    public let text: String
    public let fg: UInt32
    public let bg: UInt32
    public let flags: UInt16
    public var spacer: Bool { flags & 32 != 0 }
    public var hidden: Bool { flags & 16 != 0 }
}
public struct WorkspaceTerminalFrame: Codable, Equatable, Sendable {
    public let version: String
    public let cols: Int
    public let screenLines: Int
    public let startLine: Int
    public let totalLines: Int
    public let running: Bool
    public let altScreen: Bool
    public let cursor: TerminalPage.Cursor?
    public let background: UInt32
    public let foreground: UInt32
    public var rows: [[WorkspaceTerminalCell]]
    enum CodingKeys: String, CodingKey {
        case version, cols, running, cursor, rows, background, foreground
        case screenLines = "screen_lines", startLine = "start_line", totalLines = "total_lines", altScreen = "alt_screen"
    }
    public func text() -> String {
        var result = ""
        for (index, row) in rows.enumerated() {
            let wrapped = row.last.map { $0.flags & 64 != 0 } ?? false
            let line = row.filter { !$0.spacer }.map { $0.hidden ? " " : $0.text }.joined()
            result += wrapped ? line : line.replacingOccurrences(of: " +$", with: "", options: .regularExpression)
            if index < rows.count - 1 && !wrapped { result += "\n" }
        }
        return result
    }
}
public struct WorkspaceTerminalFragment: Decodable, Sendable {
    public let snapshotID: String
    public let rowCount: Int
    public let offset: Int
    public let nextOffset: Int?
    public let frame: WorkspaceTerminalFrame
    enum CodingKeys: String, CodingKey { case snapshotID = "snapshot_id", rowCount = "row_count", offset, nextOffset = "next_offset" }
    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: CodingKeys.self)
        snapshotID = try fields.decode(String.self, forKey: .snapshotID)
        rowCount = try fields.decode(Int.self, forKey: .rowCount)
        offset = try fields.decode(Int.self, forKey: .offset)
        nextOffset = try fields.decodeIfPresent(Int.self, forKey: .nextOffset)
        frame = try WorkspaceTerminalFrame(from: decoder)
    }
}
public struct WorkspaceTerminalAssembly: Sendable {
    private var capture: String?
    private var partial: WorkspaceTerminalFrame?
    private var nextOffset = 0
    private var rowCount = 0
    public init() {}
    public mutating func apply(_ fragment: WorkspaceTerminalFragment) throws -> WorkspaceTerminalFrame? {
        do {
            let frame = fragment.frame
            guard validID(fragment.snapshotID), (1...120).contains(fragment.rowCount), validID(frame.version), (1...512).contains(frame.cols),
                  (1...120).contains(frame.screenLines), (1...100_000).contains(frame.totalLines),
                  frame.startLine >= 0, frame.startLine < frame.totalLines,
                  !frame.rows.isEmpty, frame.rows.count <= 120,
                  frame.background <= 0xFFFFFF, frame.foreground <= 0xFFFFFF,
                  frame.rows.allSatisfy({ row in row.count == frame.cols && row.allSatisfy {
                      $0.fg <= 0xFFFFFF && $0.bg <= 0xFFFFFF && $0.flags < 128 && !$0.text.isEmpty &&
                      $0.text.unicodeScalars.count <= 8 && !$0.text.unicodeScalars.contains { $0.value < 32 || (127...159).contains($0.value) }
                  } }) else { throw MobileError.invalidResponse }
            if fragment.offset == 0 {
                guard capture == nil else { throw MobileError.invalidResponse }
                capture = fragment.snapshotID; partial = frame; nextOffset = frame.rows.count; rowCount = fragment.rowCount
            }
            else {
                guard capture == fragment.snapshotID, fragment.rowCount == rowCount, fragment.offset == nextOffset, var previous = partial else { throw MobileError.invalidResponse }
                var metadata = frame; metadata.rows = previous.rows
                guard metadata == previous else { throw MobileError.invalidResponse }
                previous.rows.append(contentsOf: frame.rows); partial = previous; nextOffset += frame.rows.count
            }
            guard nextOffset <= rowCount else { throw MobileError.invalidResponse }
            if let next = fragment.nextOffset {
                guard next == nextOffset, next < rowCount else { throw MobileError.invalidResponse }
                return nil
            }
            guard nextOffset == rowCount, let completed = partial, completed.startLine + completed.rows.count <= completed.totalLines else { throw MobileError.invalidResponse }
            if let cursor = completed.cursor {
                guard cursor.line >= completed.startLine, cursor.line < completed.startLine + completed.rows.count,
                      cursor.column >= 0, cursor.column < completed.cols else { throw MobileError.invalidResponse }
            }
            capture = nil; partial = nil; nextOffset = 0
            return completed
        } catch { capture = nil; partial = nil; nextOffset = 0; throw error }
    }
}
struct WorkspaceTerminalUpdate: Decodable, Sendable { let version: String; let fragment: WorkspaceTerminalFragment? }
struct WorkspaceTerminalResponse: Decodable, Sendable { let request: WorkspaceTerminalRequest; let update: WorkspaceTerminalUpdate }
