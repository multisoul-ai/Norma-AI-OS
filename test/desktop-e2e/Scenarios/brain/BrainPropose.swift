import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 大脑只提议下一步：脚本化的大脑在 Alan 完成后提两条（给 Bella、给 Alan），
/// 概览“需要你处理”里出现两行；忽略一条、派发一条，派发后幕僚活动面板里出现“你批准”的记录。
@MainActor
public enum BrainPropose {
    public nonisolated static let name = "brain-propose"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        var brainDir: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
            if let brainDir { try? FileManager.default.removeItem(at: brainDir) }
        }
        let workspace = "Proposals"

        try await session.step("建立工作空间与项目，开启只提议的脚本化大脑") {
            fakeShell = try BrainFixture.installFakeAgents()
            brainDir = try BrainFixture.installScriptedBrainDirectory(mode: .propose)
            setenv("ZDOTDIR", fakeShell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brainDir!.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch()
            try await session.resize(width: 1440, height: 840)
            try await BrainSupport.createWorkspace(session, name: workspace)
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try session.launch()
            try await session.resize(width: 1440, height: 840)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入项目，创建两个智能体（Alan 会完成任务，Bella 待命）") {
            try await BrainSupport.openProjectWithAgents(session, count: 2)
            return try await session.capture().summary
        }

        try await session.step("⌘⇧E 打开概览，“需要你处理”里出现两条下一步，提议已写入文件") {
            try await BrainSupport.openOverview(session)
            var frame = try await session.capture()
            let deadline = Date().addingTimeInterval(120)
            while Date() < deadline {
                frame = try await session.capture()
                if frame.contains("写登录接口文档") && frame.contains("给 Alan") { break }
                try await Task.sleep(nanoseconds: 2_000_000_000)
            }
            try session.save(frame, "01-proposals.png")
            // 栏有高度上限，第二条在栏内下方；它是否存在看文件，忽略第一条后再看界面。
            // 小号的“下一步”标签常被识别成“一步”。
            for needle in ["需要你处理", "一步", "写登录接口文档", "给 Alan", "拒绝", "采纳"] {
                guard frame.contains(needle) else {
                    let out = brainDir.flatMap { try? String(contentsOf: $0.appendingPathComponent("propose.out"), encoding: .utf8) } ?? ""
                    throw E2EError.product("概览里缺少「\(needle)」：\(frame.summary)；propose 输出：\(out)")
                }
            }
            let saved = try await savedProposals(brainDir, expect: 2)
            guard saved == 2 else {
                throw E2EError.product("提议文件里应有 2 条，实际 \(saved)")
            }
            return "提议出现在“需要你处理”，文件里有 2 条"
        }

        try await session.step("重启 App 后两条提议仍在，确认前没有任务发给 Agent") {
            session.stop()
            try session.launch()
            try await session.resize(width: 1440, height: 840)
            try await BrainSupport.openProjectWithAgents(session, count: 0)
            try await BrainSupport.openOverview(session)
            _ = try await session.wait(Expectation(allOf: ["写登录接口文档"], timeout: 20))
            _ = try await savedProposals(brainDir, expect: 2)
            guard try received(brainDir).isEmpty else {
                throw E2EError.product("用户确认前 Agent 已收到任务")
            }
            return "提议已从磁盘恢复，未提前派发"
        }

        try await session.step("展开“写登录接口文档”，点“拒绝”，这一行消失") {
            try await session.click(matching: ["写登录接口文档"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["完整任务", "接口刚改过"], timeout: 6))
            try await session.click(matching: ["完整任务"], pick: .top)
            let expanded = try await session.wait(Expectation(allOf: ["拒绝", "为登录接口写使用文档"], timeout: 6))
            try session.save(expanded, "02-expanded.png")
            try await session.click(matching: ["拒绝"], pick: .top)
            let after = try await session.wait(Expectation(absent: ["写登录接口文档"], timeout: 6))
            try session.save(after, "03-ignored.png")
            // 中文标题偶尔被识别错字（“页”认成“贡”），用“给 Bella”判断。
            guard after.contains("给 Bella") else {
                throw E2EError.product("忽略一条后另一条也不见了：\(after.summary)")
            }
            return "忽略生效"
        }

        try await session.step("目标等待确认时，下一步解释原因且不能派发；处理后恢复可用") {
            guard let brainDir else { throw E2EError.env("大脑夹具目录丢失") }
            let marker = brainDir.appendingPathComponent("waiting-2")
            try Data().write(to: marker)
            let blocked = try await session.wait(
                Expectation(allOf: ["先处理卡片中的问题或审批", "等待确认"], timeout: 6))
            try session.save(blocked, "04a-waiting-confirmation.png")
            try await session.click(matching: ["等待确认"], pick: .top)
            guard try received(brainDir).isEmpty,
                  try await savedProposals(brainDir, expect: 1) == 1 else {
                throw E2EError.product("等待确认时不得派发、覆盖问题或消费提议")
            }
            try FileManager.default.removeItem(at: marker)
            _ = try await session.wait(
                Expectation(allOf: ["采纳"], absent: ["先处理卡片中的问题或审批"], timeout: 10))
            return "阻塞原因在提议旁可见，未派发；解除后采纳恢复"
        }

        try await session.step("“补登录页测试”收起时已有采纳按钮且未展开，直接点“采纳”") {
            let collapsed = try await session.wait(
                Expectation(allOf: ["给 Bella", "采纳", "拒绝"], absent: ["登录页还没有测试"], timeout: 6))
            try session.save(collapsed, "04-dispatch-ready.png")
            try await session.click(matching: ["采纳"], pick: .bottom)
            let deadline = Date().addingTimeInterval(10)
            var frame = try await session.capture()
            while Date() < deadline {
                frame = try await session.capture()
                if !frame.contains("给 Bella") && frame.contains("补上登录") { break }
                try await Task.sleep(nanoseconds: 500_000_000)
            }
            try session.save(frame, "05-dispatched.png")
            guard !frame.contains("给 Bella") else {
                throw E2EError.product("派发后提议仍在“需要你处理”里：\(frame.summary)")
            }
            // 派发记录现在在幕僚活动面板里：由用户批准的标“你批准”，没有“以后这类先问我”。
            try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
            let panel = try await session.wait(Expectation(allOf: ["保留最近", "你批准", "补上登录"], timeout: 15))
            try session.save(panel, "05b-dispatched-panel.png")
            try await session.click(matching: ["补上登录"], pick: .top)
            let detail = try await session.wait(Expectation(allOf: ["理由"], absent: ["以后这类先问我"], timeout: 8))
            try session.save(detail, "05c-dispatched-detail.png")
            try await session.shortcut(key: 53, modifiers: [])
            _ = try await session.wait(Expectation(absent: ["保留最近"], timeout: 6))
            guard try await savedProposals(brainDir, expect: 0) == 0 else {
                throw E2EError.product("处理完后提议文件里仍有内容")
            }
            let expected = "补上登录页的测试，cargo test 通过即完成"
            var messages = try received(brainDir)
            while Date() < deadline && messages.isEmpty {
                try await Task.sleep(nanoseconds: 200_000_000)
                messages = try received(brainDir)
            }
            guard messages.count == 1, messages[0]["node"] as? Int == 2,
                  messages[0]["task"] as? String == expected else {
                throw E2EError.product("目标没有且仅收到原文，或忽略的任务仍被发送：\(messages)")
            }
            guard let brainDir, let output = session.output else { throw E2EError.env("验收证据目录丢失") }
            try Data(contentsOf: brainDir.appendingPathComponent("received.jsonl"))
                .write(to: output.appendingPathComponent("received.jsonl"))
            let files = BrainSupport.stateFiles(brainDir.appendingPathComponent("projects"), named: "proposals.json")
            guard files.count == 1 else { throw E2EError.product("提议文件数量改变") }
            try Data(contentsOf: files[0]).write(to: output.appendingPathComponent("proposals.json"))
            return "Bella 收到完整原文；忽略的任务未发送；提议文件是有效空 JSON"
        }
    }

    /// 读提议文件里的条数；文件在后台写入，等到条数为 `expect` 或 3 秒到期。
    private static func savedProposals(_ brainDir: URL?, expect: Int) async throws -> Int {
        guard let brainDir else { throw E2EError.env("大脑目录丢失") }
        let dir = brainDir.appendingPathComponent("projects", isDirectory: true)
        var count = -1
        let deadline = Date().addingTimeInterval(3)
        repeat {
            let files = (FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil)?.allObjects as? [URL] ?? [])
                .filter { $0.lastPathComponent == "proposals.json" }
            if files.count == 1,
               let data = try? Data(contentsOf: files[0]),
               let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let items = object["items"] as? [[String: Any]] {
                count = items.count
                if count == expect { return count }
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        } while Date() < deadline && count != expect
        throw E2EError.product("提议文件必须存在、可解析且含 \(expect) 条，实际 \(count)")
    }

    private static func received(_ brainDir: URL?) throws -> [[String: Any]] {
        guard let brainDir else { throw E2EError.env("大脑目录丢失") }
        let path = brainDir.appendingPathComponent("received.jsonl")
        guard FileManager.default.fileExists(atPath: path.path) else { return [] }
        return try String(contentsOf: path, encoding: .utf8).split(separator: "\n").map {
            guard let value = try JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] else {
                throw E2EError.product("目标 Agent 的接收记录不是有效 JSON")
            }
            return value
        }
    }
}
