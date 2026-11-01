import E2EKit
import Fixtures
import Foundation

/// 真实 App + 生产 Hook；脚本化 Agent 和幕僚，不需真实模型。
@MainActor
public enum BrainApprovals {
    public nonisolated static let name = "brain-approvals-ui"

    public static func run(_ session: Session) async throws {
        var shell: URL?
        var brain: URL?
        var project: URL?
        let english = ProcessInfo.processInfo.environment["NORMA_APPROVAL_UI_ENGLISH"] == "1"
        let allow = english ? "Allow" : "允许"
        let deny = english ? "Deny" : "拒绝"
        let scope = english ? "This request only" : "仅处理本次请求"
        let submitting = english ? "Submitting" : "提交中"
        let unconfirmed = english ? "Approval is not confirmed" : "审批结果尚未确认"
        defer {
            session.stop()
            if let brain, let output = session.output {
                for name in ["approval-decisions.jsonl", "approval-hook.log", "prompt.log"] {
                    if let data = try? Data(contentsOf: brain.appendingPathComponent(name)) {
                        try? data.write(to: output.appendingPathComponent(name))
                    }
                }
                let transcripts = (FileManager.default.enumerator(at: brain.appendingPathComponent("claude-config/projects"), includingPropertiesForKeys: nil)?.allObjects as? [URL] ?? []).filter { $0.pathExtension == "jsonl" }
                if let path = transcripts.first, let data = try? Data(contentsOf: path) {
                    try? data.write(to: output.appendingPathComponent("approval-transcript.jsonl"))
                }
            }
            for path in [shell, brain, project].compactMap({ $0 }) {
                try? FileManager.default.removeItem(at: path)
            }
        }
        try await session.step("建立隔离项目和原生权限菜单") {
            let fixture = try BrainApprovalFixture.install()
            shell = fixture.shell; brain = fixture.brain
            setenv("ZDOTDIR", fixture.shell.path, 1)
            setenv("NORMA_BRAIN_DIR", fixture.brain.path, 1)
            setenv("CLAUDE_CONFIG_DIR", fixture.brain.appendingPathComponent("claude-config").path, 1)
            setenv("NORMA_E2E_LANGUAGE", english ? "en" : "zh-Hans", 1)
            try session.launch()
            try await session.resize(width: 1440, height: 840)
            try await BrainSupport.createWorkspace(session, name: "Approvals")
            session.stop()
            guard let root = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            project = try CanvasProjectFixture.install(dataRoot: root, workspaceName: "Approvals")
            try Catalog.seedSetting(database: try session.databaseURL(), key: "appearance.theme.v1", value: english ? "ink-black" : "sapphire-night")
            try session.launch()
            try await session.resize(width: 1440, height: 840)
            try await BrainSupport.openProjectWithAgents(session, count: 1)
            try await BrainSupport.openOverview(session)
            return "原卡片等待审批请求"
        }
        guard let brain else { throw E2EError.env("审批夹具目录丢失") }
        try await session.step("收起态直接允许，原生 CLI 仅收到一次 allow") {
            try Data().write(to: brain.appendingPathComponent("approval-1"))
            let frame = try await session.wait(Expectation(allOf: [allow, deny], absent: [scope], timeout: 45))
            try session.save(frame, "01-approval-collapsed.png")
            guard try records(brain).isEmpty else { throw E2EError.product("操作前已放行审批") }
            try await session.click(matching: [allow], pick: .top)
            try await waitForDecision(brain, cycle: 1, behavior: "allow")
            let after = try await session.wait(Expectation(allOf: ["Decision received 1"], absent: [scope, deny, submitting, unconfirmed], timeout: 15))
            try session.save(after, "02-approval-allowed.png")
            return "折叠态完成审批，未展开详情"
        }
        try await session.step("展开核对完整命令，再收起直接拒绝") {
            try Data().write(to: brain.appendingPathComponent("approval-2"))
            _ = try await session.wait(Expectation(allOf: [allow, deny], absent: [scope], timeout: 45))
            try await session.click(matching: ["Check the decision copy"], pick: .top)
            let expanded = try await session.wait(Expectation(allOf: [scope, "brain_decision_copy", allow, deny], timeout: 8))
            try session.save(expanded, "03-approval-expanded.png")
            try await session.click(matching: ["Check the decision copy"], pick: .top)
            _ = try await session.wait(Expectation(allOf: [allow, deny], absent: [scope], timeout: 8))
            try await session.click(matching: [deny], pick: .top)
            try await waitForDecision(brain, cycle: 2, behavior: "deny")
            let after = try await session.wait(Expectation(anyOf: ["已结束", "Ended"], allOf: ["Decision received 2"], absent: [scope, deny, submitting, unconfirmed, "Do you want to proceed"], timeout: 15))
            try session.save(after, "04-approval-denied.png")
            guard try records(brain).count == 2 else { throw E2EError.product("审批重复消费") }
            return "允许/拒绝分别送达原请求一次"
        }
    }

    private static func records(_ root: URL) throws -> [[String: Any]] {
        let path = root.appendingPathComponent("approval-decisions.jsonl")
        guard FileManager.default.fileExists(atPath: path.path) else { return [] }
        return try String(contentsOf: path, encoding: .utf8).split(separator: "\n").map {
            guard let value = try JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] else {
                throw E2EError.product("审批消费记录不是 JSON")
            }
            return value
        }
    }

    private static func waitForDecision(_ root: URL, cycle: Int, behavior: String) async throws {
        let deadline = Date().addingTimeInterval(15)
        while Date() < deadline {
            let items = try records(root).filter { $0["cycle"] as? Int == cycle }
            if let item = items.first {
                guard items.count == 1, item["tool_use_id"] as? String == "approval-ui-tool-\(cycle)",
                      item["behavior"] as? String == behavior else {
                    throw E2EError.product("审批重复、错请求或决定错误：\(items)")
                }
                return
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("原生 CLI 未消费审批")
    }
}
