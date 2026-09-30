import XCTest
import SwiftUI
import NormaCore
@testable import Norma

@MainActor final class LoadingLayoutTests: XCTestCase {
    /// 项目首屏加载在可用内容区域中央，不跟随顶部标题挤在左上方。
    /// 数据构造：一台仅内存中的测试电脑，无配对凭据；inactive阻止网络副作用；系统窗口提供真实安全区。
    /// 执行过程：挂载生产ProjectCardsView → 等待一次原生布局 → 读取实际系统spinner位置 → 回收窗口。
    /// 预期结果：只有一个spinner，水平居中、垂直靠近内容中心；不能留在内容顶部35%区域。
    func testProjectLoadingUsesViewportCenter() async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id": "mac-" + String(repeating: "0", count: 64), "name": "加载布局验收",
            "certificateSHA256": String(repeating: "0", count: 64), "endpoints": ["wss://127.0.0.1:1"]
        ]))
        let window = UIWindow(windowScene: scene)
        let host = UIHostingController(rootView: ProjectCardsView(machine: machine, workspaceID: "loading-layout", name: "项目加载", model: ComputerModel(), active: false)
            .environment(\.scenePhase, .inactive).preferredColorScheme(.dark))
        window.rootViewController = host; window.makeKeyAndVisible()
        defer { window.isHidden = true; window.rootViewController = nil }
        // 300ms允许SwiftUI完成首次布局，不作为接口延迟阈值。
        try await Task.sleep(for: .milliseconds(300))
        let spinners = descendants(host.view).compactMap { $0 as? UIActivityIndicatorView }
        XCTAssertEqual(spinners.count, 1, "首次加载必须只显示一个系统进度指示器")
        let spinner = try XCTUnwrap(spinners.first, "必须真正渲染系统spinner")
        let frame = spinner.convert(spinner.bounds, to: host.view)
        let viewport = host.view.bounds.inset(by: host.view.safeAreaInsets)
        XCTAssertEqual(frame.midX, viewport.midX, accuracy: 2, "加载不能靠左，2pt为像素取整容差")
        // 32pt涵盖spinner下方一行辅助文字的一半高度及间距；不放宽到顶部占位行。
        XCTAssertEqual(frame.midY, viewport.midY, accuracy: 32, "加载必须位于内容区域中央")
        XCTAssertGreaterThan(frame.midY, viewport.minY + viewport.height * 0.35, "spinner不能残留在顶部35%的区域")
        print("PROJECT_LOADING_FRAME \(frame) VIEWPORT \(viewport)")
        if ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "loading-layout-ui" {
            // 45s仅用于Computer Use观察已挂载的生产页面，不等待网络或执行Agent。
            print("LOADING_LAYOUT_UI_READY")
            try await Task.sleep(for: .seconds(45))
        }
    }
    /// List首次读取使用可见区域中央，最大辅助字号与读取完成都不能残留左上角spinner。
    /// 数据构造：真实List的一行来源说明；系统最大辅助字号；pending先为true再false，无网络/存储。
    /// 执行过程：挂载生产加载组件与List → 核验系统spinner中心 → 切换已读列表 → 核验spinner移除。
    /// 预期结果：读取中仅有一个居中的spinner；读取完成后没有旧loading，也不需要重建导航页面。
    func testListLoadingCentersAndDisappearsAfterRead() async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        let host = UIHostingController(rootView: LoadingTestList(loading: true))
        window.rootViewController = host; window.makeKeyAndVisible()
        defer { window.isHidden = true; window.rootViewController = nil }
        // 300ms允许原生List与大字号完成布局；不改变生产网络超时。
        try await Task.sleep(for: .milliseconds(300))
        let spinners = descendants(host.view).compactMap { $0 as? UIActivityIndicatorView }
        XCTAssertEqual(spinners.count, 1, "列表只能有一个首屏加载指示器")
        let spinner = try XCTUnwrap(spinners.first, "列表首屏必须有真实系统spinner")
        let frame = spinner.convert(spinner.bounds, to: host.view)
        let viewport = host.view.bounds.inset(by: host.view.safeAreaInsets)
        XCTAssertEqual(frame.midX, viewport.midX, accuracy: 2, "列表loading不能跟随左对齐的来源行")
        // 最大辅助字号会把提示折为两行；2×56pt行高+16pt间距=128pt，spinner离整组中心最多128/2=64pt。
        XCTAssertEqual(frame.midY, viewport.midY, accuracy: 64, "辅助字号下整组loading仍须在可用区域中央")
        host.rootView = LoadingTestList(loading: false)
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(descendants(host.view).filter { $0 is UIActivityIndicatorView }.count, 0, "读取完成后必须移除旧loading")
    }

    /// 任务同步不继承LazyVStack的左对齐，而是在标题下面的滚动视口中央显示。
    /// 数据构造：一台仅内存的电脑，connecting=true且目录尚未返回；inactive阻止真实请求。
    /// 执行过程：挂载生产TasksView → 查找任务UIScrollView和系统spinner → 比较真实坐标。
    /// 预期结果：只有一个loading，位于滚动视口中央；不在搜索/筛选后的左上角。
    func testTasksLoadingCentersBelowHeader() async throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id": "mac-" + String(repeating: "0", count: 64), "name": "加载布局验收",
            "certificateSHA256": String(repeating: "0", count: 64), "endpoints": ["wss://127.0.0.1:1"]
        ]))
        let model = ComputerModel()
        model.machines = [machine]; model.selectedID = machine.id; model.connecting = true
        let window = UIWindow(windowScene: scene)
        let host = UIHostingController(rootView: TasksView(model: model, active: false, manageComputers: {})
            .environment(\.scenePhase, .inactive).preferredColorScheme(.dark))
        window.rootViewController = host; window.makeKeyAndVisible()
        defer { window.isHidden = true; window.rootViewController = nil }
        // 300ms仅让系统完成首次布局，不模拟接口延迟。
        try await Task.sleep(for: .milliseconds(300))
        let views = descendants(host.view)
        let spinners = views.compactMap { $0 as? UIActivityIndicatorView }
        XCTAssertEqual(spinners.count, 1, "任务同步不能同时显示行内和首屏两个spinner")
        let spinner = try XCTUnwrap(spinners.first, "必须有实际任务loading")
        let scroll = try XCTUnwrap(views.compactMap { $0 as? UIScrollView }.max { $0.bounds.height < $1.bounds.height }, "任务必须使用真实滚动视口")
        let viewport = scroll.convert(scroll.bounds, to: host.view)
        let frame = spinner.convert(spinner.bounds, to: host.view)
        XCTAssertEqual(frame.midX, viewport.midX, accuracy: 2, "任务loading不能继承列表左对齐")
        // 单行标签高度与间距不超过64pt，spinner离组合中心最多64/2=32pt。
        XCTAssertEqual(frame.midY, viewport.midY, accuracy: 32, "任务loading必须在标题下的内容区居中")
        XCTAssertGreaterThan(frame.midY, viewport.minY + viewport.height * 0.35, "loading不能残留在内容顶部35%区域")
        print("TASK_LOADING_FRAME \(frame) VIEWPORT \(viewport)")
        if ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "loading-task-ui" {
            // 45s只供Computer Use核对原生任务页，不连接真实电脑。
            print("TASK_LOADING_UI_READY")
            try await Task.sleep(for: .seconds(45))
        }
    }

    private func descendants(_ view: UIView) -> [UIView] { [view] + view.subviews.flatMap(descendants) }
}

private struct LoadingTestList: View {
    let loading: Bool
    var body: some View {
        List {
            Text("会话来源")
            if !loading { Text("已读取会话") }
        }.workspaceLoading(loading, "正在读取会话…")
            .environment(\.dynamicTypeSize, .accessibility5).preferredColorScheme(.dark)
    }
}
