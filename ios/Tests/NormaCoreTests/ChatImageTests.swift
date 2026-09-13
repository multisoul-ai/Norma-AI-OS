import XCTest
@testable import NormaCore

@MainActor final class ChatImageTests: XCTestCase {
    /// 附件引用使用JSON数组避免Claude把裸路径改写为Image占位符，原生Codex trim后仍可恢复纯图片。
    /// 数据构造：64位SHA命名PNG与含空格目录，一段含Tab的正文；对照为不完整/非图片标记。
    /// 执行过程：compose→检查JSON路径→parse；再模拟纯图片被trim；拒绝非法文件引用。
    /// 预期结果：正文和路径逐字往返；无裸图片路径行，不能丢失首条纯图片或隐藏普通文字。
    func testImageReferenceSurvivesNativeEditors() throws {
        let path = "/tmp/Norma Data/app.mobile-images/project/" + String(repeating: "a", count: 64) + ".png"
        let text = "检查\t这张图片"
        let message = try ChatImageMessage.compose(text: text, paths: [path])
        XCTAssertTrue(message.contains("[\"/tmp/Norma Data/"), "路径必须在JSON数组中，不能被CLI当作图片粘贴替换")
        XCTAssertFalse(message.components(separatedBy: "\n").contains(path), "不能留下Claude自动转换的裸路径行")
        XCTAssertEqual(ChatImageMessage.parse(message)?.text, text, "附件处理不能改正文")
        XCTAssertEqual(ChatImageMessage.parse(message)?.paths, [path], "路径含空格也须完整")
        let imageOnly = try ChatImageMessage.compose(text: "", paths: [path]).trimmingCharacters(in: .whitespacesAndNewlines)
        XCTAssertEqual(ChatImageMessage.parse(imageOnly)?.paths, [path], "Codex去首尾空白后仍须恢复图片")
        XCTAssertEqual(ChatImageMessage.parse(imageOnly)?.text, "", "纯图片不能显示协议正文")
        XCTAssertNil(ChatImageMessage.parse("普通消息 [[norma-images]]"), "普通正文不能被隐藏")
        XCTAssertThrowsError(try ChatImageMessage.compose(text: "", paths: ["/etc/passwd"]), "只能引用上传目录内的图片")
    }

    /// 相同图片删除重选后，旧回执不能清除新版本；只有准确成功回执才清理对应文件。
    /// 数据构造：card7/run-a/session-a，一张68字节PNG，两个相同SHA但不同本地分配的草稿版本。
    /// 执行过程：保存首版→绑定发送→移除重选→旧成功与新未知回执→新成功→重读磁盘。
    /// 预期结果：新图片字节保留；旧版本和未知回执不清草稿，准确成功后无草稿/无图片文件。
    func testReceiptKeepsReaddedImageUntilExactSuccess() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = ChatDraftStore(url: folder.appendingPathComponent("drafts.json"))
        let book = ChatDraftBook(store: store); await book.load()
        let scope = ChatDraft.Scope(machineID: "mac-" + String(repeating: "a", count: 64), workspaceID: "project", routeID: "route", nodeID: 7, runID: "run-a", sessionID: "session-a")
        let data = try XCTUnwrap(Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="))
        let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(#"{"node_id":7,"profile":"Claude","title":"Agent","mobile_control":true,"state":"ready","run_id":"run-a","session_id":"session-a","turn_id":null,"task_title":null}"#.utf8))
        func command(_ draft: ChatDraft) throws -> PendingMobileCommand {
            let image = try XCTUnwrap(draft.imageAttachments.first)
            let text = try ChatImageMessage.compose(text: draft.text, paths: ["/tmp/app.mobile-images/project/" + image.id + ".png"])
            return try PendingMobileCommand(machineID: scope.machineID, deviceID: "phone", workspaceID: "project", routeID: "route", revision: 1, sending: card, text: text, draftID: draft.id, operationID: "photo-message")
        }
        let success = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(#"{"device_id":"phone","operation_id":"photo-message","workspace_id":"project","route_id":"route","state":"succeeded","result":{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-a"},"error":null}"#.utf8))
        let unknown = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(#"{"device_id":"phone","operation_id":"photo-message","workspace_id":"project","route_id":"route","state":"outcome_unknown","result":null,"error":"outcome_unknown"}"#.utf8))
        try await book.addImage(data, for: scope)
        let first = try XCTUnwrap(book.draft(for: scope))
        let firstImage = try XCTUnwrap(first.imageAttachments.first)
        book.removeImage(firstImage.id, for: scope)
        try await book.addImage(data, for: scope)
        let next = try XCTUnwrap(book.draft(for: scope))
        let nextImage = try XCTUnwrap(next.imageAttachments.first)
        XCTAssertNotEqual(nextImage.fileKey, firstImage.fileKey, "延迟删除不能指向重选的新文件")
        try await book.confirm(command(first), receipt: success)
        try await book.confirm(command(next), receipt: unknown)
        XCTAssertEqual(book.draft(for: scope)?.id, next.id, "旧成功和未知回执不能误删新图")
        let bytes = try await book.imageData(nextImage)
        XCTAssertEqual(bytes, data, "重选图片必须仍可读")
        try await book.confirm(command(next), receipt: success)
        XCTAssertNil(book.draft(for: scope), "准确成功才清除纯图片草稿")
        let disk = try await store.load()
        XCTAssertTrue(disk.isEmpty, "已发送附件不能重启复活")
        do { _ = try await book.imageData(nextImage); XCTFail("准确成功后应清理文件") } catch {}
    }

    /// 图片草稿可跨页面和重启恢复；文字编辑不丢附件，移除不会影响文字或其他草稿。
    /// 数据构造：68字节的1×1 PNG，一台隔离Mac的card7；同一图片只保存一份原始数据。
    /// 执行过程：添加图片→编辑文字→落盘重开→读取校验→移除图片→再次读取。
    /// 预期结果：原字节和文字完整；删除后附件与不再使用的本地文件消失，文字仍存在。
    func testImageDraftPersistsAndRemovalKeepsText() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = ChatDraftStore(url: folder.appendingPathComponent("drafts.json"))
        let book = ChatDraftBook(store: store); await book.load()
        let scope = ChatDraft.Scope(machineID: "mac-" + String(repeating: "a", count: 64), workspaceID: "project", routeID: "route", nodeID: 7, runID: nil, sessionID: nil)
        let data = try XCTUnwrap(Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="))
        try await book.addImage(data, for: scope, label: "图片草稿")
        book.update("请检查这张图", for: scope)
        try await book.flush()
        let reopened = ChatDraftBook(store: store); await reopened.load()
        let draft = try XCTUnwrap(reopened.draft(for: scope))
        let image = try XCTUnwrap(draft.imageAttachments.first)
        let loaded = try await reopened.imageData(image)
        XCTAssertEqual(loaded, data, "草稿图片不能压坏或丢失")
        XCTAssertEqual(draft.text, "请检查这张图", "编辑文字不能删除图片")
        XCTAssertEqual(draft.imageAttachments.count, 1, "只能恢复所选图片")
        reopened.removeImage(image.id, for: scope)
        try await reopened.flush()
        XCTAssertTrue(reopened.draft(for: scope)?.imageAttachments.isEmpty == true, "移除后不能再次发送旧图")
        XCTAssertEqual(reopened.draft(for: scope)?.text, "请检查这张图", "移除图片不能清空文字")
        do { _ = try await reopened.imageData(image); XCTFail("无草稿引用的文件应清理") } catch {}
    }
}
