import Foundation

public enum WorkspaceResultBlock: Equatable, Sendable {
    case text(String)
    case code(String)
}

/// Splits fenced code for native rendering while leaving inline Markdown to AttributedString.
public func workspaceResultBlocks(_ source: String) -> [WorkspaceResultBlock] {
    var blocks: [WorkspaceResultBlock] = []
    var lines: [String] = []
    var code = false
    func flush() {
        guard !lines.isEmpty else { return }
        let body = lines.joined(separator: "\n")
        if !body.isEmpty { blocks.append(code ? .code(body) : .text(body)) }
        lines = []
    }
    for line in source.components(separatedBy: "\n") {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("```") {
            flush(); code.toggle()
        } else { lines.append(line) }
    }
    flush()
    return blocks
}
