import Foundation

/// 电脑端工作空间想法的只读视图。手机不能新增、编辑或指派；与本机“个人想法”（IdeaStore）是两套数据。
public struct WorkspaceIdea: Decodable, Equatable, Identifiable, Sendable {
    public enum State: String, Decodable, Sendable { case todo, assigned, done }
    public let id: String
    public let title: String
    public let state: State
    public let createdAt: Date

    enum CodingKeys: String, CodingKey { case id, title, state, createdAt = "created_at_unix" }
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        title = try container.decode(String.self, forKey: .title)
        state = try container.decode(State.self, forKey: .state)
        createdAt = Date(timeIntervalSince1970: TimeInterval(try container.decode(UInt64.self, forKey: .createdAt)))
    }
}

struct WorkspaceIdeaList: Decodable, Sendable {
    let ideas: [WorkspaceIdea]
    /// 电脑端最多下发 100 条、标题最多 80 个字符；超出或带控制字符的回复一律视为无效。
    func validate() throws {
        guard ideas.count <= 100,
              ideas.allSatisfy({ validID($0.id) && !$0.title.isEmpty && $0.title.count <= 81 && !hasControlScalars($0.title) })
        else { throw MobileError.invalidResponse }
    }
}

private func hasControlScalars(_ text: String) -> Bool {
    text.unicodeScalars.contains { $0.value < 32 || (127...159).contains($0.value) }
}

extension MobileClient {
    /// 读取当前电脑端工作空间的想法清单（只读）。
    public func workspaceIdeas(_ machine: PairedMachine, workspaceID: String) async throws -> [WorkspaceIdea] {
        try machine.validate()
        guard validID(workspaceID) else { throw MobileError.invalidResponse }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let action: [String: Any] = ["type": "workspace_ideas", "device_id": credential.deviceID, "token": credential.token, "workspace_id": workspaceID]
        let reply = try await exchange(machine, action: action)
        guard reply.result?.status == "workspace_ideas", reply.result?.workspaceID == workspaceID,
              let ideas = reply.result?.ideas else { throw MobileError.invalidResponse }
        let list = WorkspaceIdeaList(ideas: ideas)
        try list.validate()
        return list.ideas
    }
}
