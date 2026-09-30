import Foundation

public enum TaskListFilter: String, CaseIterable, Sendable {
    case all = "全部", running = "进行中", attention = "待处理", completed = "已完成"
}

extension CardSnapshot {
    /// Filters only the current authenticated snapshot; never requests or caches other projects.
    public func matchesSearch(_ query: String, state: String? = nil, profile: String? = nil) -> Bool {
        (state == nil || self.state == state) && (profile == nil || self.profile == profile)
            && matchesTerms(query, fields: [title, taskTitle ?? "", self.profile])
    }

    /// Empty, ready Agent cards are not tasks. Only observed work enters the task feed.
    public func matchesTaskSearch(_ query: String, projectName: String, filter: TaskListFilter) -> Bool {
        guard taskTitle?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
                || turnID != nil || isBusy else { return false }
        let matchesState: Bool = switch filter {
        case .all: true
        case .running: ["preparing", "thinking", "running_tool", "interrupting", "collecting"].contains(state)
        case .attention: ["waiting_approval", "failed", "ended", "unknown"].contains(state)
        case .completed: state == "done"
        }
        return matchesState && matchesTerms(query, fields: [title, taskTitle ?? "", profile, projectName])
    }
}

extension ProjectSnapshot {
    public func matchesSearch(_ query: String, state: String? = nil) -> Bool {
        (state == nil || self.state == state) && matchesTerms(query, fields: [displayName])
    }
}

private func matchesTerms(_ query: String, fields: [String]) -> Bool {
    query.split(whereSeparator: \.isWhitespace).allSatisfy { term in
        fields.contains { $0.localizedStandardContains(String(term)) }
    }
}
