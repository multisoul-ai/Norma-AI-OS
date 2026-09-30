import Foundation

public struct ConversationDetailCard: Sendable, Equatable, Identifiable {
    public let id: Int
    public let title: String
    public let symbol: String
    public let status: String?
    public let isFailure: Bool
    public let summary: String?
    public let sections: [Section]
    public let raw: String

    public struct Section: Sendable, Equatable {
        public let title: String
        public let text: String
    }

    static func parse(_ details: String) -> [Self] {
        guard let value = try? JSONSerialization.jsonObject(with: Data(details.utf8), options: .fragmentsAllowed) else {
            return [record(0, raw: details)]
        }
        let values = value as? [Any] ?? [value]
        return values.enumerated().map { index, value in
            let raw = formatted(value)
            guard let object = value as? [String: Any], let type = object["type"] as? String else {
                return record(index, raw: raw)
            }
            var title = "工具与附件", symbol = "doc.text", status: String?
            var failure = false, summary: String?, sections: [Section] = []
            func add(_ title: String, _ value: Any?) {
                guard let value, !(value is NSNull) else { return }
                sections.append(Section(title: title, text: formatted(value)))
            }
            switch type {
            case "tool_use", "server_tool_use":
                title = object["name"] as? String ?? "工具调用"
                let input = object["input"] as? [String: Any] ?? [:]
                summary = ["command", "file_path", "path", "pattern", "query", "description"].compactMap { input[$0] as? String }.first
                symbol = toolSymbol(title); status = "已请求"
                add("输入", object["input"])
            case "tool_result":
                title = "工具结果"; symbol = "text.alignleft"
                failure = object["is_error"] as? Bool == true
                status = failure ? "失败" : "已返回"
                add("输出", object["content"])
                summary = sections.first?.text
            case "commandExecution":
                title = "运行命令"; symbol = "terminal"
                summary = object["command"] as? String
                add("命令", object["command"]); add("目录", object["cwd"])
                add("输出", object["aggregatedOutput"])
                add("退出码", object["exitCode"])
                if let code = object["exitCode"] as? NSNumber, String(cString: code.objCType) != "c" {
                    failure = code.intValue != 0
                }
            case "fileChange":
                title = "修改文件"; symbol = "doc.badge.gearshape"
                if let changes = object["changes"] as? [[String: Any]] {
                    let paths = changes.compactMap { $0["path"] as? String }
                    summary = paths.count > 1 ? "\(paths[0]) · 共 \(paths.count) 个文件" : paths.first
                    for change in changes {
                        add(change["path"] as? String ?? "文件变更", change["diff"] ?? change)
                    }
                }
            case "mcpToolCall", "dynamicToolCall":
                title = (object["tool"] as? String) ?? (object["toolName"] as? String) ?? "工具调用"
                symbol = "wrench.and.screwdriver"
                summary = object["server"] as? String
                add("输入", object["arguments"]); add("输出", object["result"] ?? object["contentItems"])
                if let error = object["error"], !(error is NSNull) { failure = true; add("错误", error) }
                let result = object["result"] as? [String: Any]
                if result?["isError"] as? Bool == true || object["success"] as? Bool == false { failure = true }
            case "webSearch":
                title = "搜索网页"; symbol = "magnifyingglass"
                summary = object["query"] as? String
                add("搜索", object["query"]); add("详情", object["action"])
            default:
                return record(index, raw: raw)
            }
            if type != "tool_use", type != "server_tool_use", type != "tool_result" {
                switch object["status"] as? String {
                case "inProgress", "in_progress": status = "执行中"
                case "completed": status = "已完成"
                case "failed": failure = true
                case "declined": status = "已拒绝"
                case "interrupted": status = "已中断"
                default: break
                }
                if failure { status = "失败" }
            }
            return Self(id: index, title: title, symbol: symbol, status: status, isFailure: failure,
                        summary: summary.flatMap(compact), sections: sections, raw: raw)
        }
    }

    private static func record(_ id: Int, raw: String) -> Self {
        Self(id: id, title: "工具与附件", symbol: "doc.text", status: nil, isFailure: false,
             summary: nil, sections: [], raw: raw)
    }

    private static func toolSymbol(_ name: String) -> String {
        switch name.lowercased() {
        case "bash", "shell", "exec_command": "terminal"
        case "read", "read_file": "doc.text"
        case "edit", "write", "multiedit", "apply_patch": "square.and.pencil"
        case "glob", "grep", "websearch", "web_search": "magnifyingglass"
        default: "wrench.and.screwdriver"
        }
    }

    private static func compact(_ text: String) -> String? {
        // Only the two-line summary is bounded; full inputs/outputs remain in the expanded sections.
        let preview = text.prefix(240).split(whereSeparator: \.isWhitespace).joined(separator: " ")
        guard !preview.isEmpty else { return nil }
        return String(preview.prefix(120)) + (preview.count > 120 || text.prefix(241).count > 240 ? "…" : "")
    }

    private static func formatted(_ value: Any) -> String {
        if let text = value as? String { return text }
        if let blocks = value as? [[String: Any]], !blocks.isEmpty,
           blocks.allSatisfy({ $0["type"] as? String == "text" && $0["text"] is String }) {
            return blocks.compactMap { $0["text"] as? String }.joined(separator: "\n\n")
        }
        guard let data = try? JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys, .fragmentsAllowed, .withoutEscapingSlashes]) else { return String(describing: value) }
        return String(decoding: data, as: UTF8.self)
    }
}
