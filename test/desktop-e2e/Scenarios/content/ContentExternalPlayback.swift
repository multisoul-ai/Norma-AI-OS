import CoreGraphics
import E2EKit
import Foundation

@MainActor
enum ContentExternalPlayback {
    static func verify(_ session: Session, language: String) async throws {
        try await session.step("\(language)：外部视频真实解码与播放推进") {
            // WKWebView 是原生子视图，GPUI dispatch_event 不会把点击送给播放器。
            // 仅此步骤使用现有 AppKit 原始输入通道，不改变产品或调用媒体脚本。
            let previousInput = ProcessInfo.processInfo.environment["NORMA_E2E_INPUT"]
            setenv("NORMA_E2E_INPUT", "ns", 1)
            defer {
                if let previousInput { setenv("NORMA_E2E_INPUT", previousInput, 1) }
                else { unsetenv("NORMA_E2E_INPUT") }
            }
            let play = try await session.visualLocate("定位右侧视频播放器中央的播放按钮，用于验证外部视频实际播放")
            // 视频中心右上方纯画面：避开中央播放按钮与底部控制条，防止控件淡出伪装成播放。
            let region = CGRect(x: play.point.x + 85, y: play.point.y - 95, width: 90, height: 40)
            let stillA = try await session.capturePixels()
            try await Task.sleep(nanoseconds: 700_000_000)
            let stillB = try await session.capturePixels()
            guard CGRect(origin: .zero, size: stillA.windowFrame.size).contains(region), stillA.windowFrame.size == stillB.windowFrame.size else {
                throw E2EError.product("视频比较区域必须在同尺寸窗口内")
            }
            let paused = ContentWebviewSpike.changedFraction(stillA, stillB, region: region)
            guard paused < 0.01 else { throw E2EError.product("播放前画面不是静止对照：\(paused)") }
            // 使用刚取得的截图派发真实鼠标事件；媒体帧采样不等待模型，防止短视频已经播完。
            try await session.click(local: play.point, on: stillB)
            // 首次解码允许有界预热，但每次都比较相隔 850ms 的真实纯画面。
            // 12 秒夹具，最多 6 组约 7 秒，既不把控件变化计入，也不无限等待。
            try session.save(stillB, "\(language)-video-paused.png")
            var moving = 0.0
            for attempt in 1...6 {
                try await Task.sleep(nanoseconds: 300_000_000)
                let a = try await session.capturePixels()
                try await Task.sleep(nanoseconds: 850_000_000)
                let b = try await session.capturePixels()
                try session.save(a, "\(language)-video-playing-\(attempt)-a.png")
                try session.save(b, "\(language)-video-playing-\(attempt)-b.png")
                guard a.windowFrame.size == b.windowFrame.size else { throw E2EError.product("视频采样期间窗口尺寸变化") }
                moving = ContentWebviewSpike.changedFraction(a, b, region: region)
                if moving > 0.05 { break }
            }
            guard moving > 0.05 else {
                throw E2EError.product("点击后视频画面未推进：paused=\(paused)，playing=\(moving)")
            }
            _ = try await session.visualCheck([VisualCriterion(id: "playing", text: "右侧视频已经播放，蓝紫背景/黄色块画面正常，播放器显示非零时间或已正常播放到结尾，没有解码错误")])
            return "同一区域：播放前变化率 \(paused) < 1%；点击后间隔 850ms 变化率 \(moving) > 5%，真实帧与播放状态均已验证"
        }
    }
}
