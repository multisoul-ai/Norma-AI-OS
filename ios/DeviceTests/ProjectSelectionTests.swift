import XCTest
import NormaCore
@testable import Norma

final class ProjectSelectionTests: XCTestCase {
    /// 项目选择按电脑隔离，授权目录变化不能让失效项目继续成为操作目标。
    ///
    /// 数据构造：两台电脑共用a/b项目标识，模拟不同电脑克隆相同项目；两项均为关闭快照，不发网络请求。
    /// 执行过程：A选b → B默认a并选a → 回A保持b → 目录移除b → 断开。
    /// 预期结果：B不能继承A的b；A保留独立选择；失效项目回退到现有a，断线时不能保留可操作项目。
    @MainActor func testSelectionIsScopedToComputerAndCurrentCatalog() throws {
        let catalog = try JSONDecoder().decode([ProjectSnapshot].self, from: Data(#"[{"workspace_id":"a","display_name":"Alpha","state":"closed"},{"workspace_id":"b","display_name":"Beta","state":"closed"}]"#.utf8))
        let model = ComputerModel()
        model.selectedID = "machine-a"; model.projects = catalog
        model.selectProject("b")
        XCTAssertEqual(model.selectedProject?.id, "b", "A应选中Beta")
        model.selectedID = "machine-b"
        XCTAssertEqual(model.selectedProject?.id, "a", "B应使用自己的默认项目")
        XCTAssertNotEqual(model.selectedProject?.id, "b", "不能因项目ID相同串到A的选择")
        model.selectProject("a")
        model.selectedID = "machine-a"
        XCTAssertEqual(model.selectedProject?.id, "b", "回A应保留其独立选择")
        model.projects = [catalog[0]]
        XCTAssertEqual(model.selectedProject?.id, "a", "撤销b后只能选择仍获授权的a")
        model.selectProject("b")
        XCTAssertNotEqual(model.selectedProject?.id, "b", "不能选择不在当前目录的项目")
        model.disconnect()
        XCTAssertNil(model.selectedProject, "断线不能残留可操作项目")
    }
}
