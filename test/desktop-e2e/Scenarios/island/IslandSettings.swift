import E2EKit
import Foundation

/// Real Settings controls, compositor visibility, and bilingual restart persistence.
@MainActor
public enum IslandSettings {
    public nonisolated static let name = "island-settings"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try session.launch(language: "zh-Hans")
        try await session.resize(width: 1280, height: 840)
        let island = try await waitForIsland(session)
        var main = 0
        try await session.step("中文设置默认显示岛并开启三类提醒") {
            main = try await openSettings(session, english: false)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "defaults", text: "灵动岛设置区可见：开启灵动岛已打开，审批时主动弹出、提问时主动弹出、完成时主动弹出全部开启；中文标签可读，无裁切或错误提示")
            ]), "01-defaults-zh.png")
            guard try session.nativeWindowIsOnScreen(island) else { throw E2EError.product("缺省岛窗口不在系统可见窗口列表") }
            return "四个真实设置开关可见，岛原生窗口实际显示"
        }
        try await session.step("独立关闭审批和完成弹出，保留提问") {
            try await session.visualClick("点击审批时主动弹出右侧的开关，将它关闭；不要修改其他开关")
            try await session.visualClick("点击完成时主动弹出右侧的开关，将它关闭；不要修改其他开关")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "independent", text: "审批时主动弹出与完成时主动弹出关闭，提问时主动弹出仍开启，开启灵动岛仍打开")
            ]), "02-independent.png")
            try await verifySaved(session, hidden: false)
            return "后台保存两项关闭值，提问仍为true"
        }
        try await session.step("隐藏开关撤下系统窗口，主窗口仍可操作") {
            try await session.visualClick("点击开启灵动岛右侧开关，将它关闭")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "hidden", text: "开启灵动岛已关闭；审批和完成关闭、提问开启；没有保存失败提示")
            ]), "03-hidden.png")
            try await verifySaved(session, hidden: true)
            guard try session.nativeWindowIsOnScreen(main), try !session.nativeWindowIsOnScreen(island) else {
                throw E2EError.product("隐藏后主窗口必须仍显示，岛必须从系统可见窗口列表消失")
            }
            return "macOS合成器确认岛不再显示，主窗口保留"
        }
        try await session.step("英文重启保持隐藏及独立偏好") {
            session.stop()
            try session.launch(language: "en")
            try await session.resize(width: 1280, height: 840)
            main = try await openSettings(session, english: true)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "restored", text: "英文灵动岛设置可见：Enable Dynamic Island已关闭，Show approval popups和Show completion popups关闭，Show question popups开启；没有中文残留或错误提示")
            ]), "04-restored-en.png")
            guard try await !session.nativeWindows().contains(where: isIsland) else { throw E2EError.product("隐藏状态重启仍创建了岛窗口") }
            try await verifySaved(session, hidden: true)
            return "重启未创建岛窗口，英文设置保留最后选择"
        }
        try await session.step("恢复显示并手动展开原有三页签") {
            try await session.visualClick("点击Enable Dynamic Island右侧开关，将它重新打开")
            try await verifySaved(session, hidden: false)
            let restored = try await waitForIsland(session)
            guard try session.nativeWindowIsOnScreen(restored), try session.nativeWindowIsOnScreen(main) else { throw E2EError.product("恢复显示后缺少岛或主窗口") }
            try await session.selectNativeWindow(restored)
            let frame = try await session.capturePixels()
            try await session.hover(local: CGPoint(x: frame.windowFrame.width / 2, y: 17), on: frame, dwell: 250_000_000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "expanded", text: "岛已恢复显示并展开，英文Needs you、Running、Staff三个页签可见，布局没有旧左抽屉嵌入岛中")
            ]), "05-shown-en.png")
            return "取消隐藏后真实岛可悬停展开"
        }
    }

    private static func isIsland(_ window: NativeWindowInfo) -> Bool { window.width >= 300 && window.width < 700 }

    private static func waitForIsland(_ session: Session) async throws -> Int {
        let deadline = Date().addingTimeInterval(10)
        while Date() < deadline {
            if let island = try await session.nativeWindows().first(where: isIsland) { return island.number }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw E2EError.product("未出现独立岛窗口")
    }

    private static func openSettings(_ session: Session, english: Bool) async throws -> Int {
        guard let main = try await session.nativeWindows().first(where: { $0.width > 900 }) else { throw E2EError.product("缺少主窗口") }
        try await session.selectNativeWindow(main.number)
        try await session.shortcut(key: 43, modifiers: [0x37])
        try await session.visualClick(english ? "点击设置左侧Norma分组里的Island菜单项" : "点击设置左侧Norma分组里的灵动岛菜单项")
        return main.number
    }

    private static func verifySaved(_ session: Session, hidden: Bool) async throws {
        let expected = ["hidden": hidden, "approvals": false, "questions": true, "completions": false, "native_questions": false]
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if let raw = try session.setting("island.preferences.v1"),
               let data = raw.data(using: .utf8),
               let saved = try JSONSerialization.jsonObject(with: data) as? [String: Bool], saved == expected { return }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw E2EError.product("未保存精确偏好：\(expected)")
    }
}
