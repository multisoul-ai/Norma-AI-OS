import XCTest
@testable import NormaCore

final class ProjectReviewTests: XCTestCase {
    /// 成果预览绑定准确项目窗口，并保留原始字节，错误范围、重复路径和超限均不显示。
    ///
    /// 数据构造：未跟踪的A.txt含UTF8“中”3字节；无HEAD的新仓库；128+1条超过文件数预算。
    /// 执行过程：解码完整预览 → 核验原文 → 篡改窗口/路径/文件数量 → 拒绝错误响应。
    /// 预期结果：正文准确且before为空；路径注入、重复、旧窗口与超限不能当成完整成果。
    func testScopedPreviewKeepsBytesAndRejectsMalformedResults() throws {
        let request = ProjectReviewRequest(workspaceID: "project-a", routeID: "route-a")
        let file: [String: Any] = ["path": "A.txt", "status": "??", "after": ["kind": "text", "executable": false, "bytes": [228, 184, 173]], "diff": "+中"]
        func result(_ files: [[String: Any]], route: String = "route-a") throws -> ProjectReviewResponse {
            let data = try JSONSerialization.data(withJSONObject: ["request": ["workspace_id": "project-a", "route_id": route], "review": ["files": files, "conflicted": false]])
            return try JSONDecoder().decode(ProjectReviewResponse.self, from: data)
        }
        let valid = try result([file]); try valid.validate(for: request)
        XCTAssertEqual(valid.review.files[0].after?.text, "中", "UTF8正文不能按字符数截断")
        XCTAssertNil(valid.review.files[0].before, "新文件不伪造原文")
        XCTAssertThrowsError(try result([file], route: "old-route").validate(for: request), "旧窗口成果不可冒充新窗口")
        var escape = file; escape["path"] = "../secret"
        XCTAssertThrowsError(try result([escape]).validate(for: request), "路径不得逃逸项目")
        XCTAssertThrowsError(try result([file, file]).validate(for: request), "重复路径不能覆盖另一版本")
        XCTAssertThrowsError(try result((0..<129).map { i in var f = file; f["path"] = "\(i).txt"; return f }).validate(for: request), "128+1超过文件数量预算")
        var binary = file; binary["after"] = ["kind": "text", "executable": false, "bytes": [255]]
        XCTAssertThrowsError(try result([binary]).validate(for: request), "无效UTF8不能伪装成文本")
    }
}
