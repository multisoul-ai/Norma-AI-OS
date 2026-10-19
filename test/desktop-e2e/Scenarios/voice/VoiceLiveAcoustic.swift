import E2EKit

/// 原声学场景依赖悬浮字幕；用户移除该UI后，连接前停止，避免产生无效云端用量。
@MainActor
public enum VoiceLiveAcoustic {
    public nonisolated static let name = "voice-live-acoustic"

    public static func run(_: Session) async throws {
        throw E2EError.env("GPT-Live悬浮转写框已按用户要求移除；旧声学场景依赖字幕OCR，已在启动和连接前阻断。真人听说、插话及回声按统一验收清单执行，不能据此记为通过。")
    }
}
