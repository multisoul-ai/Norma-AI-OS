# 助手模式右侧内容工作台

一期：项目文件与 Git、普通浏览器、图片/视频、Markdown/HTML。实现沿用现有 workspace 内容标签、Wry/WebKit 与编辑输入组件。用户授权本地编辑、暂存/取消暂存、撤销未暂存修改和提交；远程同步、分支切换、合并及 IDE 语言服务不在一期。

## 行为

- 文件标签跟随当前展开项目，允许可搜索项目选择；具体文件和网页标签继续保留自己的来源。
- 文件工作台右侧按需目录树和文件名筛选；窄抽屉在目录与正文间切换，避免嵌套窄栏。
- 本地变更区分已暂存/未暂存/未跟踪；分支比较为 merge-base 到 HEAD，不包含工作树编辑。
- 文件级暂存、批量暂存、取消暂存、明确确认后的已跟踪文件撤销；提交只使用索引，正常运行 Git hooks。
- 文本上限 1 MiB，UTF-8；保存比较读取版本并采用原子交换，在最后替换处发现冲突时恢复磁盘版本并保留恢复副本。
- Markdown/HTML 支持源码编辑与预览，保存后重建预览；图片支持适应/实际尺寸/缩放/平移，视频使用系统 WebView 控件。
- 本地内容经 token-scoped `norma-content` 协议读取，资源限制在来源项目，Git 内部和越界链接拒绝读取；关闭撤销 token。
- 文件和 Git IO 均后台执行，刷新单飞；工作台实例 epoch、根目录、相对路径共同拒绝迟到结果。
- 未保存离开动作保留原意图；打开抽屉提示保存/放弃/取消，完成后继续关闭、切换或退出。

## 分层与约束

`workspace::content` 提供目录、文本版本保存、Git 读取与本地操作；`app` 持有工作台状态、后台任务、预览和 GPUI 组件；`storage` 增加 `project_files` 标签类型和工作台 JSON 状态，旧标签原样迁移。

参考 `apple-design`：内容优先，复用现有抽屉动效，高频点击不加新动画；选择器复用 gpui-component 可搜索 Select，目录/变更列表使用虚拟化，窄抽屉切换目录避免遮挡正文。`ponytail`：不引入新的浏览器引擎或编辑器依赖。安装环境没有检索到 `gpui-*` skills，API 已按钉死 GPUI 0.2.2、gpui-component 0.5.1 源码与邻近实现验证。

当前边界：Git 功能要求来源为工作树根目录（包括 worktree）；普通目录仍可浏览编辑。图像格式及视频编解码支持受 WebKit/macOS 限制。不支持的文件提供系统打开入口。不会自动安装编解码器或改变用户 Git 配置。

## 验收映射

| 行为 | 证据入口 |
|---|---|
| 目录选项、越界/.git 别名、外部保存冲突、重命名、首次提交前取消暂存、删除父目录恢复、分支比较、hooks 拒绝 | `cargo test -p workspace` |
| 新标签与旧标签兼容、状态存储 | `cargo test -p storage` |
| 迟到内容拒绝、Markdown 保存后渲染更新 | `cargo test -p app --lib workbench::tests` |
| 本地资源关闭后失效、视频 Range | `cargo test -p app --lib local_content::tests` |
| 真窗口目录/编辑/Git/Markdown/HTML/图片/视频 | `bash scripts/desktop-e2e.sh content-workbench` |
| 发布编译与机械边界 | app check/build、文件行数/no-allow/storage/performance/video 护栏 |

当前完成状态以本任务最终验收记录为准。未执行的真实窗口、登录、播放生命周期与主题检查不能以测试存在或代码编译代替。

## 实施位置与回归记录

实现及下列原始验收产物位于独立工作树 `../Norma-AI-OS-content-workbench`，分支 `feat/assistant-content-workbench`，开发基线 `6d593506`。集成时保留 `dev` 的 `c19e69f5` 并行提交，包括太空壁纸、抽屉动效、Safari 身份拆分和 GPT Live 文档；浏览器导航与最终预览尺寸修复采用本任务的已验收版本。

- `cargo test -p workspace -p storage`：248 项通过。
- 最终 App 单元测试：1130 项通过，1 项失败，9 项忽略。失败为 `voice_redesign_task_layout_and_mode_persistence` 的 517/543px 宽度断言；单独运行也失败，相关语音源文件和用例未修改。
- `cargo check -p app`、debug 构建、最终 `cargo build -p app --release` 通过。
- 文件行数、no-allow、storage/performance/video 护栏、指南行数、格式与 diff 检查通过。
- 全仓 `cargo test --workspace --no-fail-fast -- --test-threads=1` 未完成绿灯：先出现上述语音失败，后续因磁盘不足出现 `StorageFull` / `No space left on device`。已回收本任务新增的全仓构建缓存，保留最终程序和日志；不把这轮环境失败记为功能通过。

独立审查发现并修复了地址栏把带空格的 `javascript:`、`data:`、`file:` 内容发送到搜索服务的问题，新增大小写/空白变体测试先红后绿。真实视频验收还发现 Git 回执使预览申请 638px、实际仅有 622px；新增 scene 测试先复现，再让右栏预览跟随实际 flex viewport，原画布固定逻辑尺寸保持不变，两条 scene 回归均通过。

原始构建、测试及失败复现日志保存在 `artifacts/content-workbench-validation/`。桌面场景使用停机写入的隔离空间/项目夹具，创建空间交互由 workspace 模块独立验收。保存/暂存/提交等待真实文件与 Git 状态；媒体状态使用播放进度辅助识别；正文、控件边界和主题仍需逐图验收。

## 最终真实 App 验收

`bash scripts/desktop-e2e.sh content-workbench`：退出码 **0**，7/7 步通过，40.13 秒，最终场景单跑 **1/1**，不代表稳定性连跑。证据目录：`artifacts/desktop-workspace-e2e/2026-10-18T19-10-47Z/`，含 `report.json`、`app.log`、隔离 SQLite 和下列 17 张截图。

数据库复查确认：`project_files` 标签选中、浏览器位置以 `/two` 结尾、主题为 `paper-blue`。场景内另以磁盘文件、真实 Git 索引与 HEAD 检查了保存、暂存和提交。

| 截图 | 逐图结论 |
|---|---|
| 01-file-tree | 符合：项目选择紧凑；本地 1 个修改和全部 5 个文件可见，没有横向截断。嵌套目录由域测试覆盖，此夹具为平铺目录。 |
| 02-text-preview | 符合：正文、目录、编辑和系统打开入口可读且互不重叠。 |
| 03-file-saved | 符合：新正文、保存回执、输入焦点与已禁用保存按钮一致；磁盘断言通过。 |
| 03b-git-committed | 符合：Git 完成回执、0 个变更及禁用写操作一致；索引/HEAD 断言通过。 |
| 04-markdown-preview | 符合：标题、两列表格为富文本，原生层不覆盖工具栏。 |
| 05-html-preview | 符合：HTML 标题正常显示，正文贴齐内容区域。 |
| 06-image-preview | 符合：320×180 图像完整可见，适应/实际尺寸/缩放入口清晰。 |
| 07-image-zoom | 符合：图像扩大为约 400×225，即 1.25 倍；不遮挡标题。此图不证明平移手势。 |
| 08-video-preview | 符合：中央播放入口位于预览区域内。 |
| 08a-video-playing、08b-video-progress | 符合：画面颜色与位置变化，时间由 00:00 推进到 00:01；底部完整控件保留在窗口内。 |
| 08c-video-returned | 符合：返回后为播放三角而非暂停图标，保持在约 00:02，离开期间没有继续播放。 |
| 09-browser-first | 符合：HTTP 地址、正文与导航栏布局正确，输入 RETAINED E2E 可见。 |
| 10-browser-retained | 符合：切换文件再返回，表单输入仍保留。 |
| 11-browser-second | 符合：地址 /two 与第二页正文一致；脚本另执行后退、前进与刷新，检查最终地址。 |
| 12-light-document、13-light-file-tree | 符合：Paper Blue 已应用，右栏沿用现有深色材质，正文、目录和控件仍清楚可辨。 |

图片平移、视频拖动进度、外部站点登录以及全部键盘/hover 状态未在本场景穷举；不把它们写成已完成验收。早期运行存在 OCR 漏字/乱码和定位失败，已保留各次报告；最终通过记录仅指上述单次完整流程。


## dev 集成验证（2026-10-19）

合并保留 `c19e69f5` 的并行改动。独立审查发现新允许的 185px 右栏会把浏览器地址输入压到 0px；聚焦测试先复现，再让窄栏导航分为两行，185/474px 的输入、打开入口与历史按钮边界均通过。`cargo check -p app`、正常提交钩子、248 项 workspace/storage 测试、226 项相关 App 回归通过（1 项忽略）；本轮未重跑全仓测试或 release 构建，既有全仓限制见上文。

合并后的 debug 构建与 `content-workbench` 真实 App 场景通过：退出 0、7/7 步、65.92 秒、单跑 1/1。证据在独立工作树的 `artifacts/desktop-workspace-e2e/2026-10-19T01-17-14Z/`。18 张关键截图均已逐张检查：01–13 的结论与上表一致；新增 `14-narrow-browser.png` 符合，右栏约 190px，第一行五个入口与第二行地址/打开按钮完整可见，正文从工具栏下方开始。场景进一步实际编辑窄栏地址并点击打开，验证首页正文及精确持久化地址，再恢复宽度并导航到第二页；最终标签仍为 `/two`。
