import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// Spike X1/X2: only the browser steps of content-workbench, reached via the shortest setup.
@MainActor
public enum ContentWebviewSpike {
    public nonisolated static let name = "content-webview-spike"
    public nonisolated static let videoName = "content-video-spike"

    public static func run(_ session: Session) async throws {
        let workspace = "ContentWorkbenchCheck"
        var root: URL?
        defer { session.stop(); if let root { try? FileManager.default.removeItem(at: root) } }
        root = try await openWorkbench(session, workspace: workspace)
        try await ContentWorkbench.browser(session, workspace: workspace)
    }

    /// Shared setup: isolated project + right drawer. Returns the fixture root.
    static func openWorkbench(_ session: Session, workspace: String) async throws -> URL? {
        var root: URL?
        try await session.step("建立隔离项目并打开右抽屉") {
            try session.launch()
            let database = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 15) { _ in FileManager.default.fileExists(atPath: database.path) }
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            try Catalog.seedWorkspace(database: data.appendingPathComponent("app.sqlite"), name: workspace)
            root = try ContentFixture.install(dataRoot: data, workspace: workspace)
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            let frame = try await session.capture()
            let open = frame.pick(["打开", "Open"], .bottom)?.center
                ?? CGPoint(x: frame.windowFrame.width - 112, y: 540)
            try await session.click(local: open, on: frame)
            try await Task.sleep(nanoseconds: 950_000_000)
            _ = try await session.act("展开文件工作台", expect: Expectation(allOf: ["ClipE2E.mp4", "NoteE2E.txt", "PlanE2E.md"], timeout: 6)) {
                let frame = try await session.capture()
                try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) - 28, y: 26), on: frame)
            }
            try await Task.sleep(nanoseconds: 450_000_000)
            return "右抽屉文件工作台已展开"
        }
        return root
    }

    /// Spike X4: the video must really decode and advance after an AppKit click on play.
    public static func runVideo(_ session: Session) async throws {
        var root: URL?
        defer { session.stop(); if let root { try? FileManager.default.removeItem(at: root) } }
        root = try await openWorkbench(session, workspace: "ContentWorkbenchCheck")
        try await session.step("点击播放后视频时间持续推进") {
            try await ContentWorkbench.openFile(session, "ClipE2E.mp4")
            // A paused player shows only the centre play button, so give it time to load.
            try await Task.sleep(nanoseconds: 2_000_000_000)
            let video = try await session.capture()
            try session.save(video, "X4-0-before-play.png")
            // Negative control: a paused player shows no clock yet.
            if let clock = playbackSeconds(video) {
                throw E2EError.product("点击前播放器已显示计时 \(clock)s，无法作为静止对照。可见：\(video.summary)")
            }
            // The play button sits at the centre of the preview pane (calibrated: 322px from the right edge, y=443).
            try await session.click(local: CGPoint(x: video.windowFrame.width - 322, y: 443), on: video)
            try await Task.sleep(nanoseconds: 1_300_000_000)
            let first = try await session.capture()
            try session.save(first, "X4-1-playing-a.png")
            try await Task.sleep(nanoseconds: 1_300_000_000)
            let second = try await session.capture()
            try session.save(second, "X4-2-playing-b.png")
            guard let a = playbackSeconds(first), let b = playbackSeconds(second) else {
                throw E2EError.product("点击后读不到播放时间。可见：\(second.summary)")
            }
            let summary = "播放前无计时，点击后 \(a)s → \(b)s"
            guard a >= 1, b > a else { throw E2EError.product("点击播放后时间没有推进：\(summary)") }
            return summary
        }
    }

    /// Current playback second read from the player's "mm:ss  …  mm:ss" labels (current is the smaller one).
    static func playbackSeconds(_ frame: Frame) -> Int? {
        let values = frame.texts.flatMap { text -> [Int] in
            text.value.split(whereSeparator: { !$0.isNumber && $0 != ":" }).compactMap { token in
                let parts = token.split(separator: ":")
                guard parts.count == 2, parts[0].count == 2, parts[1].count == 2,
                      let minutes = Int(parts[0]), let seconds = Int(parts[1]) else { return nil }
                return minutes * 60 + seconds
            }
        }
        return values.min()
    }

    /// Fraction of pixels in `region` (window points) whose RGB differs noticeably between frames.
    static func changedFraction(_ a: Frame, _ b: Frame, region: CGRect) -> Double {
        func pixels(_ frame: Frame) -> (data: [UInt8], width: Int, height: Int)? {
            let scale = CGFloat(frame.image.width) / frame.windowFrame.width
            let rect = CGRect(x: region.minX * scale, y: region.minY * scale, width: region.width * scale, height: region.height * scale).integral
            guard let crop = frame.image.cropping(to: rect) else { return nil }
            var data = [UInt8](repeating: 0, count: crop.width * crop.height * 4)
            guard let context = CGContext(data: &data, width: crop.width, height: crop.height, bitsPerComponent: 8, bytesPerRow: crop.width * 4, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
            context.draw(crop, in: CGRect(x: 0, y: 0, width: crop.width, height: crop.height))
            return (data, crop.width, crop.height)
        }
        guard let left = pixels(a), let right = pixels(b), left.data.count == right.data.count, !left.data.isEmpty else { return 1 }
        var changed = 0
        for index in stride(from: 0, to: left.data.count, by: 4) {
            let delta = abs(Int(left.data[index]) - Int(right.data[index])) + abs(Int(left.data[index + 1]) - Int(right.data[index + 1])) + abs(Int(left.data[index + 2]) - Int(right.data[index + 2]))
            if delta > 24 { changed += 1 }
        }
        return Double(changed) / Double(left.data.count / 4)
    }
}
