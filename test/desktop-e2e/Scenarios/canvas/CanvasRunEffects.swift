import CoreGraphics
import E2EKit
import Foundation

/// 场景：Agent 卡片运行特效。四种风格 × 浅深两种主题，六张卡片分别处于
/// 准备 / 思考 / 工具调用 / 等待审批 / 完成（未查看）/ 失败。
/// 构造：隔离数据库预置 `effects.run_style` 与主题，启动后经控制通道 `run_effects_fixture` 摆出卡片。
/// 预期：思考卡片在动、准备卡片静止；新风格在失败卡片周围与经典明显不同；
/// 点击完成卡片（标记已查看）1.6s 后，卡片外缘与标题分隔线恢复到经典已查看的样子，不留残留。
@MainActor
public enum CanvasRunEffects {
    public nonisolated static let name = "canvas-run-effects"
    private static let styles = ["classic", "underline", "aura", "ferrofluid"]
    private static let themes = [("paper-blue", "light"), ("sapphire-night", "dark")]

    /// 与 `app/run_effects_fixture.rs` 的排布一致：3 × 2，每张 360 × 230，画布缩放 1。
    private static func card(_ index: Int) -> CGRect {
        CGRect(x: 70 + Double(index % 3) * 400, y: 90 + Double(index / 3) * 300, width: 360, height: 230)
    }

    private struct Shots {
        var failed: CGImage
        var viewed: CGImage
    }

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("建立隔离数据库") {
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["Norma", "orkspace", "工作空间"], timeout: 25))
            session.stop()
            try Catalog.suppressFeatureGuide(database: session.databaseURL())
            return "首次启动生成数据库，已关闭功能引导"
        }
        for (theme, label) in themes {
            var classic: Shots?
            for style in styles {
                try await session.step("\(style) · \(label)") {
                    let shots = try await check(session, style: style, theme: theme, label: label, classic: classic)
                    if style == "classic" { classic = shots.0 }
                    return shots.1
                }
            }
        }
    }

    private static func check(
        _ session: Session, style: String, theme: String, label: String, classic: Shots?
    ) async throws -> (Shots, String) {
        let database = try session.databaseURL()
        try Catalog.seedSetting(database: database, key: "effects.run_style", value: style)
        try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: theme)
        try session.launch()
        defer {
            session.stop()
            // 每次启动会重写 app.log；留一份按风格命名的副本，供 MULTISOUL_FRAME_DIAGNOSTICS 统计绘制耗时。
            if let output = session.output {
                try? FileManager.default.copyItem(
                    at: output.appendingPathComponent("app.log"),
                    to: output.appendingPathComponent("\(label)-\(style).log")
                )
            }
        }
        _ = try await session.wait(Expectation(anyOf: ["Norma", "orkspace", "工作空间"], timeout: 25))
        try await session.installRunEffectsFixture()
        let frame = try await laidOut(session)
        let size = frame.windowFrame.size
        // 等进入动画结束：平滑时间常数最大 450ms，1.6s 后完成/失败已定格。
        try await Task.sleep(nanoseconds: 1_800_000_000)
        let first = try await session.snapshot()
        try await Task.sleep(nanoseconds: 350_000_000)
        let second = try await session.snapshot()
        try session.saveSnapshot(first, "\(label)-\(style)-1.png")
        try session.saveSnapshot(second, "\(label)-\(style)-2.png")

        // 只看特效区（卡片外 20px 到内 6px 的环带 + 标题分隔线），排除卡片正文的文字变化。
        let changed = { (a: CGImage, b: CGImage, index: Int) in
            zone(card(index)).map { changedPixels(a, b, in: $0, windowSize: size, threshold: 60) }.reduce(0, +)
        }
        let thinkingMotion = changed(first, second, 1)
        guard thinkingMotion > 20 else {
            throw E2EError.product("\(style)/\(label)：思考卡片 350ms 内只有 \(thinkingMotion) 个像素变化，特效没有在动")
        }
        let readyMotion = changed(first, second, 0)
        guard readyMotion <= 12 else {
            throw E2EError.product("\(style)/\(label)：准备卡片应静止，实际 \(readyMotion) 个像素变化")
        }
        var notes = ["思考 \(thinkingMotion)px 变化", "准备 \(readyMotion)px"]
        if let classic {
            let difference = changedPixels(classic.failed, second, in: card(5).insetBy(dx: -24, dy: -24), windowSize: size, threshold: 60)
            guard difference > 150 else {
                throw E2EError.product("\(style)/\(label)：失败卡片与经典几乎相同（\(difference)px），新风格没有生效")
            }
            notes.append("失败卡片与经典差 \(difference)px")
        }

        // 点击完成卡片标题栏 = 标记已查看。
        let done = card(4)
        try await session.click(local: CGPoint(x: done.midX, y: done.minY + 12), on: frame)
        try await Task.sleep(nanoseconds: 1_600_000_000)
        let viewed = try await session.snapshot()
        try session.saveSnapshot(viewed, "\(label)-\(style)-viewed.png")
        if style == "ferrofluid" {
            // 磁流体的膜在准备状态也包着卡片（设计如此）：查看后绿色提示退去，且画面定格。
            // 用“偏绿像素数”判断，不受点击选中引起的描边/阴影变化影响。
            let outline = ring(done, inner: 4, outer: 24)
            let greenBefore = greenish(second, in: outline, windowSize: size)
            let greenAfter = greenish(viewed, in: outline, windowSize: size)
            guard greenBefore > 60, greenAfter * 3 < greenBefore else {
                throw E2EError.product("ferrofluid/\(label)：查看前后偏绿像素 \(greenBefore) → \(greenAfter)，完成提示没有退去")
            }
            let cleared = greenBefore - greenAfter
            try await Task.sleep(nanoseconds: 400_000_000)
            let later = try await session.snapshot()
            let drift = outline.map { changedPixels(viewed, later, in: $0, windowSize: size, threshold: 60) }.reduce(0, +)
            guard drift <= 12 else {
                throw E2EError.product("ferrofluid/\(label)：查看后膜仍在动（\(drift)px），没有停帧")
            }
            notes.append("查看后提示退去 \(cleared)px、定格 \(drift)px")
        } else if let classic {
            // 外缘一圈（卡片外 4–20px）与标题分隔线必须回到经典已查看的样子。
            let regions = ring(done, inner: 4, outer: 20) + [CGRect(x: done.minX + 16, y: done.minY + 28, width: done.width - 32, height: 8)]
            let residual = regions.map { changedPixels(classic.viewed, viewed, in: $0, windowSize: size, threshold: 60) }.reduce(0, +)
            guard residual <= 40 else {
                throw E2EError.product("\(style)/\(label)：完成卡片查看后仍残留 \(residual) 个像素")
            }
            notes.append("查看后残留 \(residual)px")
        }
        return (Shots(failed: second, viewed: viewed), notes.joined(separator: "；"))
    }

    /// 等卡片出现并核对布局：卡片标题（别名）落在对应卡片的标题栏里。OCR 偶尔漏读，重试几次。
    private static func laidOut(_ session: Session) async throws -> Frame {
        var last: Error = E2EError.product("找不到卡片")
        for _ in 0..<8 {
            let frame = try await session.capture()
            do {
                try requireLayout(frame)
                return frame
            } catch {
                last = error
                try await Task.sleep(nanoseconds: 600_000_000)
            }
        }
        throw last
    }

    private static func requireLayout(_ frame: Frame) throws {
        for index in [0, 4] {
            guard let title = frame.pick(["Run \(index + 1)", "Run\(index + 1)"], .top) else {
                throw E2EError.product("找不到卡片 Run \(index + 1)：\(frame.summary)")
            }
            let bar = card(index)
            guard bar.contains(title.center), title.center.y < bar.minY + 40 else {
                throw E2EError.product("Run \(index + 1) 标题位置 \(title.center) 不在预期卡片 \(bar) 的标题栏内，排布已变化")
            }
        }
    }

    /// 特效区：卡片外 20px 到卡片内 6px 的环带，加标题栏下分隔线（一道光）。
    private static func zone(_ rect: CGRect) -> [CGRect] {
        ring(rect, inner: -6, outer: 20) + [CGRect(x: rect.minX, y: rect.minY + 28, width: rect.width, height: 8)]
    }

    /// 区域内绿色明显占优的像素数（完成色），用来判断完成提示是否还在。
    private static func greenish(_ image: CGImage, in rects: [CGRect], windowSize: CGSize) -> Int {
        var data = [UInt8](repeating: 0, count: image.width * image.height * 4)
        guard let context = CGContext(
            data: &data, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: image.width * 4,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return 0 }
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        let sx = CGFloat(image.width) / windowSize.width, sy = CGFloat(image.height) / windowSize.height
        var count = 0
        for rect in rects {
            let area = CGRect(x: rect.minX * sx, y: rect.minY * sy, width: rect.width * sx, height: rect.height * sy)
                .intersection(CGRect(x: 0, y: 0, width: image.width, height: image.height))
            guard !area.isNull else { continue }
            for y in Int(area.minY)..<Int(area.maxY) {
                for x in Int(area.minX)..<Int(area.maxX) {
                    let i = (y * image.width + x) * 4
                    let (r, g, b) = (Int(data[i]), Int(data[i + 1]), Int(data[i + 2]))
                    if g > r + 30 && g > b + 10 { count += 1 }
                }
            }
        }
        return count
    }

    private static func ring(_ rect: CGRect, inner: Double, outer: Double) -> [CGRect] {
        let o = rect.insetBy(dx: -outer, dy: -outer)
        let band = outer - inner
        return [
            CGRect(x: o.minX, y: o.minY, width: o.width, height: band),
            CGRect(x: o.minX, y: o.maxY - band, width: o.width, height: band),
            CGRect(x: o.minX, y: o.minY + band, width: band, height: o.height - 2 * band),
            CGRect(x: o.maxX - band, y: o.minY + band, width: band, height: o.height - 2 * band),
        ]
    }
}
