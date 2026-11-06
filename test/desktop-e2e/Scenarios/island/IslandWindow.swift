import E2EKit
import Foundation

/// 真正独立的NSPanel：缩窗、悬停、页签、固定与Esc；模型看图核对实际界面。
/// 不把进程内原始输入当作系统热键或物理键盘证据。
@MainActor
public enum IslandWindow {
    public nonisolated static let name = "island-window"

    public static func run(_ session: Session) async throws {
        defer { session.stop(); unsetenv("NORMA_E2E_NO_ACTIVATE") }
        setenv("NORMA_E2E_NO_ACTIVATE", "1", 1)
        var islandNumber = 0
        try await session.step("启动真实App并选择独立岛窗口") {
            try session.launch(language: "zh-Hans")
            _ = try await session.capturePixels()
            let windows = try await session.nativeWindows()
            guard let island = windows.first(where: { $0.width >= 300 && $0.width < 700 && $0.height <= 60 }) else {
                throw E2EError.product("没有真实的收起岛窗口：\(windows)")
            }
            islandNumber = island.number
            guard windows.contains(where: { $0.width > 900 && $0.number != island.number }) else {
                throw E2EError.product("岛必须独立于主工作窗口")
            }
            try await session.selectNativeWindow(island.number)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "compact", text: "窗口是一条黑色圆角胶囊，左侧为空闲，右侧有绿色对勾；没有伪造的摄像头/刘海图案，也没有主画布内容")
            ]), "01-compact.png")
            return "独立窗口号 \(island.number)，尺寸 \(island.width)×\(island.height)"
        }
        try await session.step("悬停展开三页签并固定") {
            let compact = try await session.capturePixels()
            try await session.hover(local: CGPoint(x: compact.windowFrame.width / 2, y: 17), on: compact, dwell: 250_000_000)
            let expanded = try await session.capturePixels()
            guard expanded.windowFrame.width >= 600, expanded.windowFrame.height > 100, expanded.windowFrame.height < 330 else {
                throw E2EError.product("悬停后原生窗口未随形态放大")
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "tabs", text: "与原型一致：顶部标题为运行概览并显示工作空间数量，图钉在标题行右侧；下方只有需要你处理、运行中、幕僚三个页签，置于同一深灰分段底座，没有第四个固定大按钮。运行中选中，页签文字约12.5pt、未选中为灰色，空态居中而非左上角")
            ]), "02-expanded.png")
            try await session.visualClick("定位并点击顶部标题行右侧的图钉图标以固定展开")
            try await session.visualClick("定位并点击幕僚页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "staff", text: "幕僚页签被选中，面板显示紫色幕僚标志和启用引导；标题行右侧的图钉为青色选中状态；面板高度贴合内容，无大面积空白")
            ]), "03-staff-pinned.png")
            return "悬停展开、固定与页签切换均已实际操作"
        }
        try await session.step("Esc收起后释放展开区域") {
            try await session.shortcut(key: 53, modifiers: [])
            // AppKit frame callbacks run after the GPUI event lease; observe the native result.
            for _ in 0..<20 {
                if try await session.nativeWindows().contains(where: { $0.number == islandNumber && $0.height <= 60 }) { break }
                try await Task.sleep(nanoseconds: 50_000_000)
            }
            let compact = try await session.capturePixels()
            guard compact.windowID == UInt32(islandNumber), compact.windowFrame.width < 400, compact.windowFrame.height <= 60 else {
                throw E2EError.product("Esc后仍保留大面积透明原生窗口")
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "collapsed", text: "恢复为小型黑色胶囊；三个页签、幕僚内容和固定按钮都已收起")
            ]), "04-collapsed.png")
            return "同一个原生窗口已缩回胶囊尺寸"
        }
    }
}
