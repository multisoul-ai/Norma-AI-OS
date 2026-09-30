import XCTest
@testable import NormaCore

final class ConversationTransferTests: XCTestCase {
    /// 长响应只在全部分片到齐后解码；字节缺口/混入另一传输必须拒绝且清空。
    ///
    /// 数据构造：中文3字节+emoji4字节，offset使用UTF-8长度；32MiB上限另用元数据触发。
    /// 执行过程：首片 → 尾片 → 错offset → 换ID → 超限声明。
    /// 预期结果：首片不产生假完整响应；最终正文准确；缺口不能靠下一片补成无效会话。
    func testCompleteTransferAndRejectMixedFragments() throws {
        let request = ConversationRequest(workspaceID: "p", routeID: "r", nodeID: 1, runID: "run", sessionID: "session", cursor: nil)
        let prefix = #"{"request":{"workspace_id":"p","route_id":"r","node_id":1,"run_id":"run","session_id":"session","cursor":null},"page":{"items":[{"id":"one","role":"assistant","text":"中"#
        let suffix = #"🖤","details":null}],"cursor":null,"has_more":false,"pending_tail":false,"direction":"older"}}"#
        let bytes = prefix.utf8.count + suffix.utf8.count
        func part(_ text: String, offset: Int, id: String = "transfer-a", total: Int? = nil) -> ConversationFragment {
            ConversationFragment(transferID: id, offset: offset, totalBytes: total ?? bytes, text: text)
        }
        var transfer = ConversationTransfer()
        XCTAssertNil(try transfer.append(part(prefix, offset: 0), as: ConversationResponse.self), "首片不完整，不能提前提交历史游标")
        let complete = try XCTUnwrap(try transfer.append(part(suffix, offset: prefix.utf8.count), as: ConversationResponse.self), "尾片完成后才能解码")
        try complete.validate(for: request)
        XCTAssertEqual(complete.page.items.first?.text, "中🖤", "UTF-8字节偏移不能截坏Unicode正文")
        XCTAssertFalse(complete.page.hasMore, "完整末页不能伪造后续游标")
        transfer = ConversationTransfer()
        _ = try transfer.append(part(prefix, offset: 0), as: ConversationResponse.self)
        XCTAssertThrowsError(try transfer.append(part(suffix, offset: prefix.utf8.count + 1), as: ConversationResponse.self), "缺一个字节也必须失败")
        XCTAssertThrowsError(try transfer.append(part(suffix, offset: prefix.utf8.count), as: ConversationResponse.self), "缺口后旧缓存已清空，不能续拼")
        transfer = ConversationTransfer(); _ = try transfer.append(part(prefix, offset: 0), as: ConversationResponse.self)
        XCTAssertThrowsError(try transfer.append(part(suffix, offset: prefix.utf8.count, id: "transfer-b"), as: ConversationResponse.self), "不同冻结读取不能拼接")
        transfer = ConversationTransfer()
        XCTAssertThrowsError(try transfer.append(part(prefix, offset: 0, total: 32 * 1024 * 1024 + 1), as: ConversationResponse.self), "超过32MiB预算的声明应在分配前拒绝")
    }
}
