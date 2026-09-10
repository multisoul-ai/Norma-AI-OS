import XCTest
import SwiftUI
import NormaCore
@testable import Norma

final class ReviewViewTests: XCTestCase {
    /// 冻结图片按真实字节解码，限制解压尺寸，不执行文件路径或符号链接。
    ///
    /// 数据构造：3000×1500=450万像素PNG；4097²=16785409超过4096²=16777216像素上限；
    ///   1MiB+1字节超过现有单版本传输上限；损坏图片只保留前24字节。
    /// 执行过程：解码正常PNG并检查缩略尺寸 → 解码超大/损坏/非图片/符号链接 → 逐项检查拒绝。
    /// 预期结果：图片保留2:1比例且最长边2048；无效或超限输入不能产生可显示图像。
    @MainActor func testImagePreviewBoundsAndBinaryValidation() throws {
        let data = png(width: 3000, height: 1500)
        let image = try ReviewImagePreview.decode(version(data))
        XCTAssertEqual(image.width, 2048, "大图片必须在解码时缩小，不能完整展开后再缩放")
        XCTAssertEqual(image.height, 1024, "缩略图应保留2:1原始比例")
        XCTAssertThrowsError(try ReviewImagePreview.decode(version(png(width: 4097, height: 4097)))) {
            XCTAssertEqual($0 as? ReviewImageError, .tooLarge, "超过像素上限必须在解码前拒绝")
        }
        XCTAssertThrowsError(try ReviewImagePreview.decode(version(Data(repeating: 0, count: 1024 * 1024 + 1)))) {
            XCTAssertEqual($0 as? ReviewImageError, .tooLarge, "编码字节也应受现有1MiB边界约束")
        }
        XCTAssertThrowsError(try ReviewImagePreview.decode(version(Data(data.prefix(24)))), "损坏图片不能显示为有效成果")
        XCTAssertThrowsError(try ReviewImagePreview.decode(version(Data("<svg onload='fetch(1)'/>".utf8))), "不渲染主动内容或非栅格格式")
        XCTAssertThrowsError(try ReviewImagePreview.decode(version(data, kind: "symlink")), "符号链接不可作为图片解码，更不能跟随目标")
    }

    /// 新增与删除的冻结图片默认显示存在的一侧，切换文件后不保留前一张图片。
    ///
    /// 数据构造：新增2×1红图与已删除1×2蓝图，390×844为常规iPhone视口。
    /// 执行过程：挂载真实SwiftUI页面 → 等待后台解码 → 检查新增图 → 切换为删除文件 → 检查旧版本图。
    /// 预期结果：默认无需寻找版本菜单；图片可缩放，有可访问名称；删除图不能显示旧的红图。
    @MainActor func testBinaryFileViewShowsExistingVersionAndResetsImage() async throws {
        func file(_ path: String, side: String, data: Data) throws -> ReviewFileChange {
            try JSONDecoder().decode(ReviewFileChange.self, from: JSONSerialization.data(withJSONObject:
                ["path": path, "status": side == "after" ? "??" : " D", side: ["kind": "binary", "executable": false, "bytes": Array(data)]]))
        }
        let added = try file("added.png", side: "after", data: png(width: 2, height: 1))
        let deleted = try file("deleted.png", side: "before", data: png(width: 1, height: 2, color: .blue))
        let host = UIHostingController(rootView: ReviewFileView(file: added).id(added.id))
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene, "测试需要原生scene")
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        window.rootViewController = host; window.isHidden = false
        defer { window.isHidden = true; window.rootViewController = nil }
        func imageView(_ view: UIView) -> UIImageView? {
            if let image = view as? UIImageView, image.accessibilityIdentifier == "review-image" { return image }
            return view.subviews.lazy.compactMap(imageView).first
        }
        func waitForImage(named name: String) async throws -> UIImageView {
            // 3s=150×20ms，等待异步解码和SwiftUI挂载，超时必须失败。
            for _ in 0..<150 {
                host.view.layoutIfNeeded()
                if let image = imageView(host.view), image.accessibilityLabel == name { return image }
                try await Task.sleep(for: .milliseconds(20))
            }
            return try XCTUnwrap(imageView(host.view), "图片预览必须真实挂载")
        }
        let newImage = try await waitForImage(named: "added.png")
        XCTAssertEqual(newImage.image?.size, CGSize(width: 2, height: 1), "新增文件默认显示交付版本")
        XCTAssertEqual(newImage.accessibilityLabel, "added.png", "VoiceOver应能识别当前文件")
        let scroll = try XCTUnwrap(newImage.superview as? UIScrollView, "图片应复用系统缩放滚动")
        XCTAssertGreaterThan(scroll.maximumZoomScale, scroll.minimumZoomScale, "图片必须允许原生捏合放大")
        XCTAssertGreaterThan(scroll.bounds.height, 200, "图片需有足够视口，不能被上方控件挤压")
        host.rootView = ReviewFileView(file: deleted).id(deleted.id)
        let oldImage = try await waitForImage(named: "deleted.png")
        XCTAssertEqual(oldImage.image?.size, CGSize(width: 1, height: 2), "删除文件默认显示任务起点的旧图")
        XCTAssertNotEqual(oldImage.image?.size, CGSize(width: 2, height: 1), "切换文件不能残留前一张图")
        XCTAssertEqual(oldImage.accessibilityLabel, "deleted.png", "图像与文件身份必须一致")
    }

    @MainActor private func png(width: Int, height: Int, color: UIColor = .red) -> Data {
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: width, height: height), format: format).pngData { context in
            color.setFill(); context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        }
    }
    private func version(_ data: Data, kind: String = "binary") throws -> ReviewFileVersion {
        try JSONDecoder().decode(ReviewFileVersion.self, from: JSONSerialization.data(withJSONObject:
            ["kind": kind, "executable": false, "bytes": Array(data)]))
    }

    /// 原生文件预览完整呈现大正文，保留选择与动态字体，不能出现编辑或提交入口。
    ///
    /// 数据构造：20000行“+中\n”=5字节×20000=100000字节；390×844模拟普通iPhone可用区域。
    /// 执行过程：真实SwiftUI HostingView布局 → 找到TextKit正文 → 校验全文、选择、编辑与字体设置。
    /// 预期结果：正文完整可选择且不可编辑；支持Dynamic Type；独立正文视口有剩余高度，无截断字符串。
    @MainActor func testFullTextPreviewUsesReadOnlyNativeTextKit() async throws {
        let body = String(repeating: "+中\n", count: 20_000)
        let data = try JSONSerialization.data(withJSONObject: ["path": "result.txt", "status": "??", "diff": body])
        let file = try JSONDecoder().decode(ReviewFileChange.self, from: data)
        let host = UIHostingController(rootView: ReviewFileView(file: file).preferredColorScheme(.dark))
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene, "原生测试宿主需要有效scene")
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        window.rootViewController = host; window.isHidden = false
        defer { window.isHidden = true; window.rootViewController = nil }
        host.loadViewIfNeeded()
        host.view.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        host.view.setNeedsLayout(); host.view.layoutIfNeeded()
        func textView(_ view: UIView) -> UITextView? {
            if let text = view as? UITextView { return text }
            return view.subviews.lazy.compactMap(textView).first
        }
        // 2s = 100×20ms，只等待SwiftUI挂载到真实测试窗口；不将空渲染树视为通过。
        for _ in 0..<100 {
            if textView(host.view) != nil { break }
            try await Task.sleep(for: .milliseconds(20))
            host.view.layoutIfNeeded()
        }
        let text = try XCTUnwrap(textView(host.view), "SwiftUI必须创建原生可滚动正文")
        XCTAssertEqual(text.text, body, "100000字节正文不得截断")
        XCTAssertTrue(text.isSelectable, "正文应支持选择与复制")
        XCTAssertFalse(text.isEditable, "手机预览不能修改项目文件")
        XCTAssertTrue(text.adjustsFontForContentSizeCategory, "系统大字号必须可用")
        XCTAssertGreaterThan(text.bounds.height, 200, "844pt页面应为正文留下至少200pt视口，而非被控件挤压")
    }
}
