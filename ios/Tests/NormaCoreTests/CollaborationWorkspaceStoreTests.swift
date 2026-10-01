import Foundation
import XCTest
@testable import NormaCore

final class CollaborationWorkspaceStoreTests: XCTestCase {
    /// 两个 Workspace 的本机配置独立，归档重开及重建 Store 后仍完整保留。
    /// 数据构造：临时数据库 1 个，Workspace 2 个，A 关联项目 1 个，B 关联项目 0 个。
    /// 执行过程：创建 A/B → 给 A 添加项目 → 归档 A → 重建 Store → 重开 A。
    /// 预期结果：A 的项目保留；B 不继承 A 的项目；归档不删除，重开恢复为活动状态。
    func testIndependentWorkspacesSurviveArchiveAndReopen() async throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: url)
        var a = try await store.create(name: "移动端体验")
        let b = try await store.create(name: "发布质量")
        a.projects = [WorkspaceProjectReference(machineID: "mac-a", projectID: "ios", name: "iOS App", machineName: "Mac A")]
        a.archived = true
        try await store.save(a)
        let reopened = CollaborationWorkspaceStore(databaseURL: url)
        let values = try await reopened.all()
        let savedA = try XCTUnwrap(values.first { $0.id == a.id }, "归档不能删除 A")
        let savedB = try XCTUnwrap(values.first { $0.id == b.id }, "A 的修改不能删除 B")
        XCTAssertEqual(savedA.projects.first?.projectID, "ios", "重启后 A 的项目引用必须保留")
        XCTAssertTrue(savedA.archived, "重启后 A 应保持归档")
        XCTAssertTrue(savedB.projects.isEmpty, "B 不能继承 A 的项目")
        XCTAssertFalse(savedB.archived, "归档 A 不能归档 B")
        a.archived = false
        try await reopened.save(a)
        let restored = try await reopened.all()
        XCTAssertFalse(try XCTUnwrap(restored.first { $0.id == a.id }).archived, "重开 A 应解除归档")
        XCTAssertEqual(restored.count, 2, "归档和重开不得复制 Workspace")
    }

    /// 拒绝重复项目及不属于关联项目的 Agent，并保留此前合法配置。
    /// 数据构造：Workspace 1 个，项目 ios 1 个；重复列表包含 ios 两次；非法 Agent 指向 api。
    /// 执行过程：保存 ios → 尝试重复项目 → 尝试跨未关联项目 Agent → 重新读取。
    /// 预期结果：两次非法写入均失败；ios 保留；api Agent 不存在。
    func testInvalidAssociationsDoNotOverwriteSavedConfiguration() async throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: url)
        var workspace = try await store.create(name: "移动端体验")
        let project = WorkspaceProjectReference(machineID: "mac-a", projectID: "ios", name: "iOS App", machineName: "Mac A")
        workspace.projects = [project]
        try await store.save(workspace)
        workspace.projects = [project, project]
        do { try await store.save(workspace); XCTFail("重复项目必须拒绝") }
        catch { XCTAssertTrue(error is CollaborationWorkspaceError, "重复引用应返回配置错误") }
        workspace.projects = [project]
        workspace.agents = [WorkspaceAgentReference(machineID: "mac-a", projectID: "api", definitionID: 1, name: "Codex", profile: "Codex")]
        do { try await store.save(workspace); XCTFail("Agent 不能引用未关联的项目") }
        catch { XCTAssertTrue(error is CollaborationWorkspaceError, "悬空 Agent 应返回配置错误") }
        let records = try await store.all()
        let saved = try XCTUnwrap(records.first)
        XCTAssertEqual(saved.projects, [project], "失败写入不能改变合法的 ios 关联")
        XCTAssertTrue(saved.agents.isEmpty, "非法 api Agent 不应落盘")
    }
}
