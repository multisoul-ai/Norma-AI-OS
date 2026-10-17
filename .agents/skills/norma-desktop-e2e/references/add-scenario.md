# 新增场景

先读 [`modules.md`](modules.md)，选定模块。没有对应模块时，先在该文件的预留表里把它改成现有模块，再写代码。

1. 在 `test/desktop-e2e/Scenarios/<模块>/` 加 `<场景>.swift`。类型提供 `nonisolated static let name` 和 `run(_:)`。只写本模块的业务步骤；点击、等待、SQLite 走 `E2EKit`。
2. 在 `test/desktop-e2e/checks/<模块>/` 加同名 `<场景>.checks.md`。不要把 `.md` 放进 `Scenarios/`，否则 SwiftPM 会拒绝编译。
3. 每个用户可见步骤用 `session.step`。一步里要有动作、期望的后置状态，以及这一步该核对的数据或文件。
4. 点击用 `click(matching:)`，或在动作闭包里重新 `capture()`。不要缓存上一张截图的坐标。中英是二选一，放 `anyOf`；`allOf` 表示这些词必须同时出现。
5. 需要消失的界面（确认框、对话框）放进 `Expectation.absent`。菜单项和确认按钮若共用一个词，只点菜单里那个位置，重试时不要改点确认按钮。
6. 夹具放在 `test/desktop-e2e/Fixtures/<模块>/`。写 SQLite 前先 `session.stop()`，写完再 `launch()`。
7. 在该模块的 `<模块>Module.swift` 登记：`manifest` 的 `scenarioNames` 加上场景名，`run` 加上对应分支。文件名必须带模块前缀，同一 Swift 目标里不能再出现第二个 `Module.swift`。不要把单条场景写进 `Catalog.swift`。
8. 全新模块还要在 `Scenarios/Catalog.swift` 的 `modules` 加上该模块的 `manifest`，并在 [`modules.md`](modules.md) 从预留表挪到现有模块表。
9. 在 [`modules.md`](modules.md) 补上场景名和验收范围。

跑 `bash scripts/desktop-e2e.sh <场景名>`，或 `bash scripts/desktop-e2e.sh <模块名>` 跑完该模块全部场景。退出码 0，且清单里的图都按 [`visual-checks.md`](visual-checks.md) 看过，这个场景才算可用。
