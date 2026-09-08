# Norma Quest 导航

Quest 是空间客户端；Agent、PTY、代码仓库与构建在 Mac。见 `docs/architecture.md`。

- 中文回复与文档，不使用 Mermaid。
- `app/` 放 Android/Spatial SDK 适配；`core/` 为纯 Kotlin，不依赖 Android。
- 不把 CLI、shell 或仓库执行代码放进 Quest。生产 Mac 适配接现有 Rust 领域模块。
- 公共协议在 `protocol/`，共享 fixture 必须由 Kotlin 和 Node 两端测试消费。
- 修改行为先写失败测试；测试覆盖外部可观察状态、版本处理与多任务决策的独立性。
- 不自动确认决策；以 Mac 确认及权威快照为准。断开后禁用提交。
- UI 内容优先，使用系统字体、空间合成层、明确的连接/按下/焦点/禁用状态；先使用 SDK 标准交互。
- `bash scripts/quest.sh check` 为提交前检查；不能跳过 Gradle/CI 或提交 hook。
- 真机验收按 `docs/headset-checklist.md`，没有真实设备时明确记录未验证，不把 JVM、浏览器或 APK 构建当作 VR 验收。
- 不提交 SDK/JDK、`local.properties`、签名密钥、配对令牌、私人日志、`node_modules` 或构建产物。
- monorepo 是唯一开发源；独立私有库是 `apps/quest` 的 subtree 导出。不要同时维护两个代码副本，见 `docs/repository.md`。
