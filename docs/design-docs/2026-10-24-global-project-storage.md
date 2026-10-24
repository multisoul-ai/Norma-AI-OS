# 项目数据库统一进入全局数据目录

2026-10-24：用户确认数据库统一放到 Norma 全局数据目录，保留各项目独立存储，不再在项目目录创建数据库。本决策取代旧数据库环境隔离规范中的项目本地存储位置；Production/Test 的隔离要求继续有效。

## 路径与边界

- Production：`~/Library/Application Support/Norma OS/projects/<key>/project.sqlite`。
- 本地 Test：`~/Library/Application Support/Norma OS Test/projects/<key>/project.test.sqlite`。
- 隔离测试：显式 App 数据库所在目录下的 `projects/<key>/project.test.sqlite`。
- `key` 为规范化项目目录原始路径字节的 SHA256。同一项目的符号链接别名使用同一存档，同名但路径不同的项目分别存储。
- 项目初始化统一通过 `ProjectStore::initialize_for_database_paths`。路径解析本身不打开数据库、不扫描目录；旧项目加载、手机恢复及想法任务查找继续共用 `DatabasePaths`。
- 已有的桌面区域画布与共享 Agent 定义继续使用全局数据目录中的现行存储位置。
- 用户项目窗口先在后台准备数据库，再在原窗口仍有效时打开项目；手机历史查询与想法对账在已有后台路径中兼容导入，避免主线程复制旧库。
- 手机历史查询绑定实际 `AppStore` 的数据根，同时保留 Production/Test 模式与对应文件名；项目离线或旧库损坏时，想法清单仍可读取，无法查询的派发记录保持未知状态。

## 旧数据迁移

首次打开项目，若目标全局库不存在，读取对应模式的旧库：Production 只读取 `.multisoul/project.sqlite`，Test 只读取 `.multisoul/project.test.sqlite`。旧文件不存在则直接初始化全局空库。

迁移以只读连接打开来源，使用 SQLite backup 在全局目标目录生成临时快照，包含已提交的 WAL 数据；完成 schema 迁移并同步文件后，以不可覆盖的原子发布安装到目标路径，再同步目标目录。并发初始化不会覆盖已发布的库。失败不留下正式目标，修复来源后可重试。

新全局库存在时以其为准，不重新导入旧值。原项目库保留作备份，不自动删除或修改；后续写入只使用全局库。存储失败会报告对应模式和全局目标路径，不退回项目本地库。

## 验收映射

| 验收行为 | 证据 |
| --- | --- |
| 打开新项目不创建本地数据库，重开复用全局存档 | `crates/storage/tests/global_project_storage.rs` |
| 旧会话及已提交 WAL 可恢复，新数据不被旧库覆盖 | `crates/storage/tests/global_project_storage.rs` |
| 同名项目、符号链接与 Production/Test 隔离 | `crates/storage/tests/global_project_storage.rs` |
| 损坏旧库失败不发布空库，修复后可重试 | `crates/storage/tests/global_project_storage.rs` |
| 各运行模式路径与初始化失败报告 | `crates/storage/tests/unit/database_paths_tests.rs` |
| App 项目绑定与保存使用隔离全局库 | `crates/app/tests/unit/database_isolation_tests.rs` |
| 真实 App 打开与重启后项目中不生成数据库 | `test/desktop-e2e/Scenarios/canvas/CanvasContentTabs.swift` |
