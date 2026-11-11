import E2EKit
import Fixtures
import Foundation

/// 语音「更多设置」折叠分组：默认全部收起，单开一组，识别服务只显示当前来源。
/// 不保存 Key、不打开麦克风、不发起通话。
@MainActor
public enum VoiceAdvancedGroups {
    public nonisolated static let name = "voice-advanced-groups"

    public static func run(_ session: Session) async throws {
        guard session.visualReview else { throw E2EError.env("此场景使用 --visual 模型看图") }
        defer { session.stop() }
        try await session.step("打开语音指令并展开更多设置") {
            session.stop()
            try session.launch(language: "zh-Hans")
            _ = try await session.capturePixels()
            session.stop()
            let database = try session.databaseURL()
            try Catalog.suppressFeatureGuide(database: database)
            try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: "sapphire-night")
            try Catalog.seedSetting(database: database, key: "voice.desktop.mode", value: "legacy")
            try session.launch(language: "zh-Hans")
            try await session.resize(width: 1280, height: 850)
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await session.visualClick("点击设置左侧栏 Norma 分组中的「语音」一行。不要点「快捷键」或「音效」")
            try await session.scroll(xFraction: 0.62, yFraction: 0.55, deltaY: -640)
            try await session.visualClick("点击标题为「更多设置」的一行，展开它")
            try await session.scroll(xFraction: 0.62, yFraction: 0.55, deltaY: -900)
            let page = try await session.visualCheck([
                VisualCriterion(id: "groups", text: "能看到「理解模型」「语音识别服务」「Jev 快速判断」「回复偏好」四到五个分组标题行，每行有一句摘要，且全部是收起状态（没有展开的表单）"),
                VisualCriterion(id: "badge", text: "「理解模型」「语音识别服务」行右侧有状态标签（如待连接、待设置、已就绪），文字可读"),
                VisualCriterion(id: "mic", text: "分组列表里没有重复的「麦克风权限」行"),
                VisualCriterion(id: "secret", text: "画面上没有明文 API Key")
            ])
            try session.save(page, "00-groups-collapsed.png")
            return "分组默认收起已看图"
        }
        try await session.step("展开语音识别服务，再切到理解模型") {
            try await session.visualClick("点击「语音识别服务」分组标题行，展开它")
            let recognition = try await session.visualCheck([
                VisualCriterion(id: "open", text: "「语音识别服务」已展开，只显示本地 Sherpa 相关设置（如本地识别模型），没有 Azure 区域、Azure Key 或 A1 安装项"),
                VisualCriterion(id: "single", text: "其他分组仍是收起的")
            ])
            try session.save(recognition, "01-recognition-open.png")
            try await session.visualClick("点击「理解模型」分组标题行")
            let model = try await session.visualCheck([
                VisualCriterion(id: "open", text: "「理解模型」已展开，显示供应商、基础地址、API Key 等设置"),
                VisualCriterion(id: "secret", text: "API Key 输入框没有回显任何真实 Key")
            ])
            try session.save(model, "02-model-open.png")
            try await session.scroll(xFraction: 0.62, yFraction: 0.55, deltaY: -900)
            let swapped = try await session.visualCheck([
                VisualCriterion(id: "swap", text: "能看到「语音识别服务」行，它是收起的（没有「识别模型」内容），说明同一时刻只展开一组")
            ])
            try session.save(swapped, "03-recognition-closed.png")
            try await session.scroll(xFraction: 0.62, yFraction: 0.55, deltaY: 900)
            try await session.visualClick("再次点击已展开的「理解模型」分组标题行，把它收起")
            let closed = try await session.visualCheck([
                VisualCriterion(id: "closed", text: "所有分组都已收起，列表回到只有标题行和摘要的状态")
            ])
            try session.save(closed, "04-groups-closed-again.png")
            return "分组单开与收起已看图"
        }
    }
}
