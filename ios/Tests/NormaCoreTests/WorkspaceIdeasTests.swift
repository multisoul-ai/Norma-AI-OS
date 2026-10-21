import XCTest
@testable import NormaCore

final class WorkspaceIdeasTests: XCTestCase {
    private func decode(_ json: String) throws -> WorkspaceIdeaList {
        try JSONDecoder().decode(WorkspaceIdeaList.self, from: Data(json.utf8))
    }

    /// 电脑端下发的想法能被解码并保持顺序，三种状态都认得。
    /// 数据构造：待办、已指派、已完成各一条，创建时间 1_800_000_000 秒。
    /// 执行过程：解码并校验。
    /// 预期结果：顺序不变；状态与标题正确；时间换算为同一时刻；校验通过。
    func testDecodesIdeasInOrderWithAllStates() throws {
        let list = try decode(#"{"ideas":[{"id":"a1","title":"登录页改版","state":"todo","created_at_unix":1800000000},{"id":"b2","title":"补快捷键截图","state":"assigned","created_at_unix":1800000001},{"id":"c3","title":"清理依赖","state":"done","created_at_unix":1800000002}]}"#)
        try list.validate()
        XCTAssertEqual(list.ideas.map(\.title), ["登录页改版", "补快捷键截图", "清理依赖"], "顺序必须与电脑端一致")
        XCTAssertEqual(list.ideas.map(\.state), [.todo, .assigned, .done], "三种状态都应被识别")
        XCTAssertEqual(list.ideas[0].createdAt, Date(timeIntervalSince1970: 1_800_000_000), "时间应按秒换算")
    }

    /// 不可信的回复一律拒绝：超限、空标题、过长标题、控制字符、非法 ID、未知状态。
    /// 数据构造：每种坏数据一份。
    /// 预期结果：未知状态在解码阶段失败；其余在校验阶段抛 invalidResponse；恰好 81 字符（80 加省略号）仍允许。
    func testRejectsMalformedOrOversizedLists() throws {
        func bad(_ json: String, file: StaticString = #filePath, line: UInt = #line) {
            XCTAssertThrowsError(try decode(json).validate(), "应拒绝：\(json.prefix(60))", file: file, line: line)
        }
        bad(#"{"ideas":[{"id":"a","title":"","state":"todo","created_at_unix":1}]}"#)
        bad(#"{"ideas":[{"id":"a","title":"\#(String(repeating: "长", count: 82))","state":"todo","created_at_unix":1}]}"#)
        bad(#"{"ideas":[{"id":"a","title":"含\u0007控制","state":"todo","created_at_unix":1}]}"#)
        bad(#"{"ideas":[{"id":"含 空格","title":"x","state":"todo","created_at_unix":1}]}"#)
        bad(#"{"ideas":[{"id":"a","title":"x","state":"weird","created_at_unix":1}]}"#)
        let many = (0..<101).map { #"{"id":"i\#($0)","title":"x","state":"todo","created_at_unix":1}"# }.joined(separator: ",")
        bad(#"{"ideas":[\#(many)]}"#)
        let edge = try decode(#"{"ideas":[{"id":"a","title":"\#(String(repeating: "长", count: 81))","state":"todo","created_at_unix":1}]}"#)
        XCTAssertNoThrow(try edge.validate(), "80 字符加省略号共 81 个字符，应允许")
    }
}
