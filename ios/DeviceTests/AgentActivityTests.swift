import XCTest
import SwiftUI
import Darwin
import NormaCore
@testable import Norma

@MainActor final class AgentActivityTests: XCTestCase {
    /// 6张运行卡在真实SwiftUI窗口中持续绘制，刷新标题不重启动画，非活跃时不循环。
    /// 数据构造：6张卡=3 Claude+3 Codex，每张图标/边框各1个合成动画；0.6s预热，6s CPU采样。
    /// 执行过程：显示卡片→测进程CPU→读取12个系统动画→更新标题→后台/结束→离屏。
    /// 预期结果：运行连续且相位不被刷新重置；非活跃/离屏无动画；CPU仅记录前后同环境对照，不作真机FPS结论。
    func testRenderingKeepsPhaseAndStopsInactiveMotion() async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        let host = UIHostingController(rootView: ActivityTestBoard())
        window.rootViewController = host; window.makeKeyAndVisible()
        defer { window.isHidden = true; window.rootViewController = nil }
        try await Task.sleep(for: .milliseconds(600))
        let cpu = cpuSeconds(), start = ContinuousClock.now
        try await Task.sleep(for: .seconds(6))
        print("CARD_ANIMATION_CPU seconds=\(cpuSeconds() - cpu) wall=\(start.duration(to: .now)) cards=6")
        let layers = allLayers(host.view.layer).filter { $0.name == "agent.activity.motion" }
        XCTAssertEqual(layers.count, 12, "6张运行卡应有图标和边框合成层，不能依赖SwiftUI每帧重绘")
        let first = try XCTUnwrap(layers.first?.animation(forKey: "activity"), "必须有连续合成动画")
        let border = try XCTUnwrap(layers.first { $0 is CAGradientLayer }, "必须保留边框光带")
        let before = try XCTUnwrap(border.presentation(), "边框必须已经实际上屏").transform
        host.rootView = ActivityTestBoard(revision: 1)
        try await Task.sleep(for: .milliseconds(100)) // 跨过一次主线程提交；不等待整个3s边框周期。
        XCTAssertEqual(layers.first?.animation(forKey: "activity")?.beginTime, first.beginTime, "内容刷新不能从头播放")
        let after = try XCTUnwrap(border.presentation(), "刷新后仍须在合成层播放").transform
        XCTAssertTrue(before.m11 != after.m11 || before.m12 != after.m12, "动画注册成功还不够，实际呈现角度必须推进")
        host.rootView = ActivityTestBoard(phase: .background)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertFalse(layers.contains { $0.animation(forKey: "activity") != nil }, "后台不能继续循环")
        host.rootView = ActivityTestBoard(working: false)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertFalse(layers.contains { $0.animation(forKey: "activity") != nil }, "任务结束后必须停止")
        host.rootView = ActivityTestBoard()
        try await Task.sleep(for: .milliseconds(100))
        let resumed = allLayers(host.view.layer).filter { $0.name == "agent.activity.motion" }
        XCTAssertEqual(resumed.filter { $0.animation(forKey: "activity") != nil }.count, 12, "回到前台运行状态应恢复12个动画")
        // 显式UI模式最多60s用于Computer Use观察，不触碰电脑配对或运行Agent。
        if ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "card-animation-ui" {
            print("CARD_ANIMATION_UI_READY")
            try await Task.sleep(for: .seconds(60))
        }
        window.rootViewController = nil
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertFalse(resumed.contains { $0.animation(forKey: "activity") != nil }, "离屏后不能保留动画")
    }

    private func allLayers(_ layer: CALayer) -> [CALayer] {
        [layer] + (layer.sublayers ?? []).flatMap(allLayers)
    }
    private func cpuSeconds() -> Double {
        var value = rusage(); getrusage(RUSAGE_SELF, &value)
        return Double(value.ru_utime.tv_sec + value.ru_stime.tv_sec)
            + Double(value.ru_utime.tv_usec + value.ru_stime.tv_usec) / 1_000_000
    }
}

private struct ActivityTestBoard: View {
    var revision = 0
    var phase = ScenePhase.active
    var working = true
    private var cards: [CardSnapshot] {
        (1...6).map { id in
            let object: [String: Any] = ["node_id":id, "profile":id % 2 == 0 ? "Claude" : "Codex", "title":"任务 \(id) · \(revision)",
                "mobile_control":true, "state":working ? "thinking" : "done"]
            return try! JSONDecoder().decode(CardSnapshot.self, from: JSONSerialization.data(withJSONObject: object))
        }
    }
    var body: some View {
        VStack(spacing: 14) {
            Text("运行中 · 动效验收").font(.headline).foregroundStyle(.white)
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 12) {
                ForEach(cards) { card in AgentSnapshotCard(card: card) }
            }
        }.padding(20).frame(maxWidth: .infinity, maxHeight: .infinity).background(.black)
            .environment(\.scenePhase, phase).preferredColorScheme(.dark)
    }
}
