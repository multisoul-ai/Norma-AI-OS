import E2EKit
import Foundation

/// 首页焦点回归：复用首页场景的12空间夹具，始终通过原始输入和最终可见状态验收。
@MainActor
func verifyHomeFocus(_ session: Session, prefix: String) async throws {
    try await session.step("\(prefix)：首页不抢命令输入") {
        try await session.shortcut(key: 40, modifiers: [0x37])
        try await session.type("/help")
        _ = try await session.wait(Expectation(allOf: ["/help", "最近访问"], absent: ["搜索结果", "没有名为"], timeout: 5))
        try await Task.sleep(nanoseconds: 350_000_000)
        let frame = try await session.capture()
        try session.save(frame, "\(prefix)-12-command-input.png")
        guard frame.contains("/help"), !frame.contains("搜索结果"), !frame.contains("没有名为") else {
            throw E2EError.product("命令文本必须留在Dock，不能进入首页搜索：\(frame.summary)")
        }
        try await session.selectAll()
        try await session.deleteSelection()
        try await session.shortcut(key: 53, modifiers: [])
        return "⌘K后完整/help留在底部输入框，首页搜索保持关闭"
    }
    for (tabs, name) in [(1, "close"), (2, "older")] {
        try await session.step("\(prefix)：\(name)按钮聚焦后Esc保持关闭") {
            try await session.click(matching: ["全部 12", "全部12"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["更早", "显示全部"], timeout: 5))
            for _ in 0..<tabs { try await session.shortcut(key: 48, modifiers: []) }
            // 先让按钮焦点真正绘制，再关闭，避免合并焦点事件掩盖回归。
            try await Task.sleep(nanoseconds: 150_000_000)
            try await session.shortcut(key: 53, modifiers: [])
            let closed = try await stableHomeClosed(session)
            try session.save(closed, "\(prefix)-13-\(name)-escape.png")
            return "后续帧仍为搜索胶囊，无分组/遮罩反弹"
        }
    }
    try await session.step("\(prefix)：关闭按钮点击与搜索重新展开") {
        try await session.click(matching: ["全部 12", "全部12"], pick: .top)
        _ = try await session.wait(Expectation(allOf: ["更早"], timeout: 5))
        let frame = try await session.capture()
        // 1440×840：首页搜索top166+108=274，关闭按钮中心right1040-8-20=1012、y274+7+20=301。
        try await session.click(local: CGPoint(x: 1012, y: 301), on: frame)
        _ = try await stableHomeClosed(session)
        try await session.type("插件")
        _ = try await session.wait(Expectation(allOf: ["插件市场", "搜索结果"], timeout: 5))
        try await Task.sleep(nanoseconds: 350_000_000)
        try session.save(try await session.capture(), "\(prefix)-14-reopened.png")
        try await session.shortcut(key: 53, modifiers: [])
        _ = try await stableHomeClosed(session)
        return "点击关闭稳定；随后输入能重新展开并筛选，没有永久压制Focus"
    }
}

@MainActor
private func stableHomeClosed(_ session: Session) async throws -> Frame {
    // 等待超过260ms面板动效，并跨过Focus事件所在帧，再连续检查最终状态。
    try await Task.sleep(nanoseconds: 350_000_000)
    var frame = try await session.capture()
    for _ in 0..<2 {
        guard frame.contains("最近访问"),
              !frame.contains("更早"), !frame.contains("搜索结果"),
              !frame.contains("没有名为"), !frame.contains("要找哪一个") else {
            throw E2EError.product("搜索关闭后不能在后续帧重新展开：\(frame.summary)")
        }
        try await Task.sleep(nanoseconds: 100_000_000)
        frame = try await session.capture()
    }
    guard frame.contains("最近访问"), !frame.contains("更早"),
          !frame.contains("搜索结果"), !frame.contains("没有名为"), !frame.contains("要找哪一个") else {
        throw E2EError.product("搜索关闭最终帧发生反弹：\(frame.summary)")
    }
    return frame
}
