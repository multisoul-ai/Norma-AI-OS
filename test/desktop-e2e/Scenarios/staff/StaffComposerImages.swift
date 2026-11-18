import AppKit
import E2EKit
import Fixtures
import Foundation

/// 真实粘贴、移除和发送，检查预览与用户气泡在宽窄窗口中的内容边界。
@MainActor
public enum StaffComposerImages {
    public nonisolated static let name = "staff-composer-images"
    public nonisolated static let englishName = "staff-composer-images-en"

    public static func run(_ session: Session, english: Bool = false) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            for key in keys {
                if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) }
            }
            for path in [root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("CLAUDE_CONFIG_DIR", root.appendingPathComponent("claude-config").path, 1)
        let language = english ? "en" : "zh-Hans"
        try session.launch(language: language)
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Composer Images")
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: english ? "sapphire-night" : "ink-black")
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Composer Images")
        try session.launch(language: language)

        let icon = VisualCriterion(id: "attach-icon", text: "中央消息输入框左下角蓝色圆形添加图片按钮内有清晰白色加号，不能只有纯蓝圆；右侧仍为发送上箭头。")
        let hint = VisualCriterion(id: "shortcut", text: english ? "输入框下方保留 Enter to send 与 Shift + Enter for a new line 提示。" : "输入框下方保留 Enter 发送与 Shift + Enter 换行提示。")
        try await session.step("空草稿显示添加图片的加号") {
            try await session.visualClick("点击 Composer Images 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击底栏最左侧的浅紫发圆形头像进入幕僚模式")
            try session.save(try await session.visualCheck([icon, hint]), "01-empty.png")
            return "空草稿左侧加号可见，发送箭头和快捷键提示保留"
        }
        try await session.step("粘贴图片后仍显示加号且有缩略图") {
            try await session.visualClick("点击中央底部消息输入框的文字输入区域")
            try await pasteImage(session, width: 120, height: 240)
            try session.save(try await session.visualCheck([
                icon, hint,
                VisualCriterion(id: "thumbnail", text: "图片预览完整在输入框内部：橙色竖图四边的白框完整可见、中央蓝块可见，右上角有移除叉号；预览与下方占位文字分开、不重叠。没有裁切、撑出输入框或图片路径文字。如果仍在上传，判 wait。")
            ]), "02-image.png")
            return "真实粘贴产生图片缩略图，左下角保持白色加号"
        }
        try await session.step("移除图片后回到空草稿") {
            try await session.visualClick("点击输入框内图片缩略图右上角的叉号移除图片")
            try session.save(try await session.visualCheck([
                icon,
                VisualCriterion(id: "removed", text: "输入框内已没有橙蓝图片缩略图或移除叉号，恢复空草稿；左侧添加图片按钮仍有加号，未发送消息。")
            ]), "03-removed.png")
            return "移除图片后加号继续可见"
        }
        let text = "再开发一个坦克。STREAM-BETA " + String(repeating: "保留完整用户文字 TANK，不要超出自己的气泡。", count: 3)
        try await session.step("横竖图片与输入文字分开，缩窗仍完整") {
            try await session.visualClick("点击中央底部消息输入框的文字输入区域")
            try await pasteImage(session, width: 120, height: 240)
            _ = try await session.visualCheck([VisualCriterion(id: "first-image", text: "输入框内已出现橙色竖图预览；仍在上传判wait。")])
            try await pasteImage(session, width: 320, height: 80)
            try await session.type(text)
            for width in [900, 1280] {
                try await session.resize(width: CGFloat(width), height: 860)
                try session.save(try await session.visualCheck([
                    icon, hint,
                    VisualCriterion(id: "draft-contained", text: "输入框内有两张预览，一张竖图、一张横图；每张橙图的白色四边完整可见（叉号仅覆盖右上小角），图片下方单独排列以再开发一个坦克开头的多行文字。两张图不被裁切、不撑出输入框、不覆盖文字；文字不覆盖下方加号或发送按钮。")
                ]), "04-draft-\(width).png")
            }
            return "横竖图均完整缩放，输入文字与图片分开"
        }
        try await session.step("回车发送后文字和图片留在用户气泡内") {
            try await session.shortcut(key: 36, modifiers: [])
            for _ in 0..<100 {
                let messages = try StaffFixture.tasks(root).first?["messages"] as? [[String: Any]] ?? []
                if messages.contains(where: { $0["User"] != nil }) { break }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            let messages = try StaffFixture.tasks(root).first?["messages"] as? [[String: Any]] ?? []
            let sent = messages.compactMap { ($0["User"] as? [String: Any])?["text"] as? String }
            guard sent.count == 1, let first = sent.first, first.hasPrefix(text + "\n\n[[norma-images]]") else {
                throw E2EError.product("回车必须发送一条包含原文和图片的用户消息")
            }
            for width in [1280, 900] {
                try await session.resize(width: CGFloat(width), height: 860)
                try await session.scroll(xFraction: 0.6, yFraction: 0.4, deltaY: 3000)
                try session.save(try await session.visualCheck([
                    icon, hint,
                    VisualCriterion(id: "sent-contained", text: "右对齐用户气泡内显示以再开发一个坦克开头的多行文字及两张橙蓝图片（竖图和横图），白色四边完整可见。文字与图片不互相重叠，均在同一个气泡背景及内边距以内；气泡不压住底部输入框。"),
                    VisualCriterion(id: "draft-cleared", text: "底部输入框已恢复占位文字，没有残留图片预览或已发送文字。")
                ]), "05-sent-\(width).png")
            }
            return "回车发送一条原文与双图消息，宽窄窗均无溢出"
        }
        try await session.step("发送箭头仍可点击发送") {
            try await session.visualClick("点击底部消息输入框的文字输入区域")
            try await session.type("再开发一个坦克 SEND-BUTTON STREAM-BETA")
            try await session.visualClick("点击输入框右下角的蓝色发送上箭头")
            try await session.scroll(xFraction: 0.6, yFraction: 0.5, deltaY: -3000)
            try session.save(try await session.visualCheck([
                icon,
                VisualCriterion(id: "button-sent", text: "对话底部出现包含再开发一个坦克 SEND-BUTTON STREAM-BETA的新用户气泡，文字完整在气泡内；上方前一条图文气泡的底边与新气泡之间留有间距，没有图片或文字跨出气泡覆盖下一条消息或底部输入框。输入框恢复占位文字；若发送尚未完成判wait。")
            ]), "06-button-sent.png")
            let messages = try StaffFixture.tasks(root).first?["messages"] as? [[String: Any]] ?? []
            guard messages.filter({ $0["User"] != nil }).count == 2 else { throw E2EError.product("发送按钮必须创建第二条用户消息") }
            return "发送按钮创建第二条用户消息"
        }
    }

    private static func pasteImage(_ session: Session, width: CGFloat, height: CGFloat) async throws {
        let image = NSImage(size: NSSize(width: width, height: height))
        image.lockFocus()
        NSColor.white.setFill()
        NSRect(x: 0, y: 0, width: width, height: height).fill()
        NSColor.systemOrange.setFill()
        NSRect(x: 6, y: 6, width: width - 12, height: height - 12).fill()
        NSColor.systemBlue.setFill()
        NSRect(x: width / 4, y: height / 4, width: width / 2, height: height / 2).fill()
        image.unlockFocus()
        guard let tiff = image.tiffRepresentation,
              let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else {
            throw E2EError.env("无法生成剪贴板 PNG 夹具")
        }
        let clipboard = NSPasteboard.general
        let saved = (clipboard.pasteboardItems ?? []).map { item in
            let copy = NSPasteboardItem()
            for type in item.types { if let data = item.data(forType: type) { copy.setData(data, forType: type) } }
            return copy
        }
        defer { clipboard.clearContents(); if !saved.isEmpty { clipboard.writeObjects(saved) } }
        clipboard.clearContents()
        clipboard.setData(png, forType: .png)
        try await session.shortcut(key: 9, modifiers: [0x37])
    }
}
