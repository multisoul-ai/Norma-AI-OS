import CoreGraphics
import E2EKit
import Foundation

/// 虚拟屏高 900，窗口内容区最高 870。1440×870 上设置面板外框为 1008×609，与其他设置页相同。
/// 旧逻辑只放大「语音指令」，豆包与 GPT-Live 会缩回 1008×609。
/// 侧栏「语音」在隔离屏上经常被 Vision 漏掉，所以定位走模型看图。
@MainActor
public enum SettingsVoiceModePanelSize {
    public nonisolated static let name = "settings-voice-mode-panel-size"

    public static func run(_ session: Session) async throws {
        guard session.visualReview else { throw E2EError.env("此场景使用 --visual 模型看图") }
        defer { session.stop() }
        let boxes = PanelBoxes()
        try await session.step("在 1440×870 打开语音设置") {
            try session.launch(language: "zh-Hans")
            // 1440×900 虚拟屏扣掉 30pt 后，窗口内容区到不了 900。
            try await session.resize(width: 1440, height: 870)
            _ = try await session.visualCheck([
                VisualCriterion(id: "boot", text: "Norma 主界面已显示（工作空间起始页），不是启动动画或空白窗口；若仍在加载请回答 wait")
            ])
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await session.visualClick("点击设置左侧栏 Norma 分组中的「语音」一行。不要点内容区顶部的标题，也不要点「人格」「记忆」或「幕僚」")
            let page = try await session.visualCheck([
                VisualCriterion(id: "mode", text: "当前是语音设置：能看到「对话模式」，三张卡片为语音指令、豆包 2.0 语音、GPT-Live，且语音指令为选中态")
            ])
            boxes.legacy = try measure(page, name: "01-legacy-panel.png", session: session)
            return "窗口 1440×870，语音指令 \(boxes.legacy!)"
        }
        try await session.step("切到豆包并测量同一外框") {
            try await session.visualClick("点击「对话模式」里标题为「豆包 2.0 语音」的卡片中心，不要点侧栏")
            let page = try await session.visualCheck([
                VisualCriterion(id: "doubao", text: "豆包 2.0 语音卡片为选中态，其下出现豆包连接相关内容；设置面板仍是居中的大圆角窗口")
            ])
            boxes.doubao = try measure(page, name: "02-doubao-panel.png", session: session)
            return "豆包 \(boxes.doubao!)"
        }
        try await session.step("切到 GPT-Live 并测量同一外框") {
            try await session.visualClick("点击「对话模式」里标题为「GPT-Live」的卡片中心，不要点侧栏")
            let page = try await session.visualCheck([
                VisualCriterion(id: "live", text: "GPT-Live 卡片为选中态，其下出现对话偏好；设置面板仍是居中的大圆角窗口")
            ])
            boxes.live = try measure(page, name: "03-live-panel.png", session: session)
            return "GPT-Live \(boxes.live!)"
        }
        try await session.step("三种对话模式的面板外框一致且与其他设置页同为七成尺寸") {
            guard let legacy = boxes.legacy, let doubao = boxes.doubao, let live = boxes.live else {
                throw E2EError.product("三种模式没有全部量到外框")
            }
            for box in [legacy, doubao, live] {
                // 1440×870 的七成框是 1008×609。旧的语音页加宽框 1280×783 会被宽高上限拒绝。
                guard box.width >= 960, box.width <= 1060, box.height >= 570, box.height <= 650 else {
                    throw E2EError.product("面板应接近 1008×609，实测 \(box)。语音指令 \(legacy)，豆包 \(doubao)，GPT-Live \(live)")
                }
            }
            guard abs(legacy.width - doubao.width) <= 8, abs(legacy.height - doubao.height) <= 8,
                  abs(legacy.width - live.width) <= 8, abs(legacy.height - live.height) <= 8,
                  abs(legacy.left - doubao.left) <= 8, abs(legacy.top - doubao.top) <= 8,
                  abs(legacy.left - live.left) <= 8, abs(legacy.top - live.top) <= 8 else {
                throw E2EError.product("切换对话模式后面板外框变了：语音指令 \(legacy)，豆包 \(doubao)，GPT-Live \(live)")
            }
            return "三种模式同为 \(legacy)"
        }
    }

    private static func measure(_ frame: Frame, name: String, session: Session) throws -> PanelBox {
        try session.save(frame, name)
        guard let box = PanelBox.measure(frame.image, window: frame.windowFrame.size) else {
            throw E2EError.product("无法从 \(name) 量出设置面板外框")
        }
        return box
    }
}

final class PanelBoxes {
    var legacy: PanelBox?
    var doubao: PanelBox?
    var live: PanelBox?
}

struct PanelBox: CustomStringConvertible {
    var left: CGFloat
    var top: CGFloat
    var width: CGFloat
    var height: CGFloat

    var description: String {
        String(format: "x=%.0f y=%.0f %.0f×%.0f", left, top, width, height)
    }

    /// 面板填充是低彩度深灰，壁纸是偏色的。某一列里这种灰超过约四分之一，就落在面板内。
    static func measure(_ image: CGImage, window: CGSize) -> PanelBox? {
        let width = image.width
        let height = image.height
        guard width > 0, height > 0, window.width > 1, window.height > 1 else { return nil }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        guard let context = CGContext(
            data: &pixels,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: 1, y: -1)
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))

        let step = max(2, width / 720)
        func fill(_ x: Int, _ y: Int) -> Bool {
            let (r, g, b) = rgb(pixels, width, x, y)
            let chroma = max(r, g, b) - min(r, g, b)
            return chroma <= 12 && (r + g + b) / 3 >= 14 && (r + g + b) / 3 <= 55
        }

        var columns: [Int] = []
        var x = 0
        while x < width {
            var hits = 0
            var samples = 0
            var y = 0
            while y < height {
                samples += 1
                if fill(x, y) { hits += 1 }
                y += step
            }
            if samples > 0, hits * 4 >= samples { columns.append(x) }
            x += step
        }
        guard let firstColumn = columns.first, let lastColumn = columns.last, lastColumn > firstColumn else { return nil }

        var rows: [Int] = []
        var y = 0
        while y < height {
            var hits = 0
            var samples = 0
            var column = firstColumn
            while column <= lastColumn {
                samples += 1
                if fill(column, y) { hits += 1 }
                column += step
            }
            if samples > 0, hits * 10 >= samples * 3 { rows.append(y) }
            y += step
        }
        guard let firstRow = rows.first, let lastRow = rows.last, lastRow > firstRow else { return nil }
        let scale = CGFloat(width) / window.width
        return PanelBox(
            left: CGFloat(firstColumn) / scale,
            top: CGFloat(firstRow) / scale,
            width: CGFloat(lastColumn - firstColumn) / scale,
            height: CGFloat(lastRow - firstRow) / scale
        )
    }
}

private func rgb(_ pixels: [UInt8], _ width: Int, _ x: Int, _ y: Int) -> (Int, Int, Int) {
    let index = (y * width + x) * 4
    return (Int(pixels[index]), Int(pixels[index + 1]), Int(pixels[index + 2]))
}
