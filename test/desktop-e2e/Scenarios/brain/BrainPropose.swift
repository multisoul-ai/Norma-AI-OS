import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 大脑只提议下一步：脚本化的大脑在 Alan 完成后提两条（给 Bella、给 Alan），
/// 概览“需要你处理”里出现两行；忽略一条、派发一条，派发后“Norma 自主处理”出现记录。
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
            try await BrainSupport.createWorkspace(session, name: workspace)
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try session.launch()
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
            for needle in ["需要你处理", "一步", "写登录接口文档", "给 Alan"] {
                guard frame.contains(needle) else {
                    let out = brainDir.flatMap { try? String(contentsOf: $0.appendingPathComponent("propose.out"), encoding: .utf8) } ?? ""
                    throw E2EError.product("概览里缺少「\(needle)」：\(frame.summary)；propose 输出：\(out)")
                }
            }
            let saved = try savedProposals(brainDir, expect: 2)
            guard saved == 2 else {
                throw E2EError.product("提议文件里应有 2 条，实际 \(saved)")
            }
            return "提议出现在“需要你处理”，文件里有 2 条"
        }

        try await session.step("展开“写登录接口文档”，点“忽略”，这一行消失") {
            try await session.click(matching: ["写登录接口文档"], pick: .top)
            let expanded = try await session.wait(Expectation(allOf: ["忽略", "为登录接口写使用文档"], timeout: 6))
            try session.save(expanded, "02-expanded.png")
            try await session.click(matching: ["忽略"], pick: .bottom)
            let after = try await session.wait(Expectation(absent: ["写登录接口文档"], timeout: 6))
            try session.save(after, "03-ignored.png")
            // 中文标题偶尔被识别错字（“页”认成“贡”），用“给 Bella”判断。
            guard after.contains("给 Bella") else {
                throw E2EError.product("忽略一条后另一条也不见了：\(after.summary)")
            }
            return "忽略生效"
        }

        try await session.step("展开“补登录页测试”，看到完整指令与理由，点“派发”") {
            try await session.click(matching: ["给 Bella"], pick: .top)
            let expanded = try await session.wait(
                Expectation(allOf: ["cargo test", "登录页还没有测试", "派发"], timeout: 6))
            try session.save(expanded, "04-dispatch-ready.png")
            try await session.click(matching: ["派发"], pick: .bottom)
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
            guard frame.contains("Norma 自主处理"), frame.contains("补上登录") else {
                throw E2EError.product("派发后“Norma 自主处理”没有出现记录：\(frame.summary)")
            }
            guard try savedProposals(brainDir, expect: 0) == 0 else {
                throw E2EError.product("处理完后提议文件里仍有内容")
            }
            return "派发生效，文件已清空"
        }
    }

    /// 读提议文件里的条数；文件在后台写入，等到条数为 `expect` 或 3 秒到期。
    private static func savedProposals(_ brainDir: URL?, expect: Int) throws -> Int {
        guard let brainDir else { throw E2EError.env("大脑目录丢失") }
        let dir = brainDir.appendingPathComponent("proposals", isDirectory: true)
        var count = -1
        let deadline = Date().addingTimeInterval(3)
        repeat {
            let files = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? []
            count = files.compactMap { try? Data(contentsOf: $0) }
                .compactMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
                .map { ($0["items"] as? [Any])?.count ?? 0 }
                .reduce(0, +)
            Thread.sleep(forTimeInterval: 0.2)
        } while Date() < deadline && count != expect
        return count
    }
}
