import E2EKit
import Fixtures
import Foundation

extension WorkspaceHomeDesign {
    static func runHomeLifecycle(_ session: Session) async throws {
        try await session.step("首页菜单重命名与删除持久化") {
            try HomeDesignFixture.install(database: session.databaseURL(), count: 1, theme: "sapphire-night", alternate: false)
            try session.launch(language: "zh-Hans")
            try await session.resize(width: 1440, height: 840)
            _ = try await session.wait(Expectation(allOf: ["最近访问", "Norma OS"], timeout: 25))
            try await openSingleMenu(session)
            try await session.click(matching: ["重命名"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["重命名工作空间"], timeout: 5))
            try await session.selectAll()
            try await session.type("首页验收重命名")
            try await session.click(matching: ["保存"], pick: .bottom)
            _ = try await session.waitCatalog(timeout: 8) { $0.names == ["首页验收重命名"] }
            let renamed = try await session.wait(Expectation(allOf: ["首页验收重命名", "最近访问"], absent: ["重命名工作空间"], timeout: 8))
            try session.save(renamed, "lifecycle-01-renamed.png")
            try await openSingleMenu(session)
            try await session.click(matching: ["删除"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["删除工作空间"], timeout: 5))
            try await session.click(matching: ["删除"], pick: .bottom)
            _ = try await session.waitCatalog(timeout: 8) { $0.names.isEmpty && $0.areas.isEmpty }
            let welcome = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], absent: ["最近访问"], timeout: 8))
            try session.save(welcome, "lifecycle-02-welcome.png")
            session.stop()
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], absent: ["首页验收重命名"], timeout: 25))
            guard try session.catalog().names.isEmpty else { throw E2EError.product("重启不能恢复已删除空间") }
            session.stop()
            return "单卡菜单真实重命名/确认删除；重启后目录仍为空"
        }
    }

    private static func openSingleMenu(_ session: Session) async throws {
        let frame = try await session.capture()
        // 单空间加新建槽：总宽520，左边460；第一卡菜单中心688，y607。
        try await session.hover(local: CGPoint(x: 688, y: 607), on: frame)
        try await session.click(local: CGPoint(x: 688, y: 607), on: try await session.capture())
        _ = try await session.wait(Expectation(allOf: ["重命名", "删除"], timeout: 5))
    }
}
