# Norma Markdown MemoryStore

实现对应 [Norma：Markdown 记忆库与可靠版本发布](https://github.com/multisoul-ai/Norma-AI-OS/issues/82)。本票仅交付存储与纯数据协议；尚未接入对话、自动蒸馏、Dream、设置页或语音。

## 公共接口与范围

`agent` 导出 `MemoryScope`、`MemoryEntryId`、`MemoryRevision`、`MemoryMutation`、主题摘要、正文、来源引用与保存回执；`storage` 导出 `MemoryStore`、`MemorySnapshot`、`MemoryError`。没有新增反向 crate 依赖。

```rust
let store = storage::MemoryStore::open(&database_paths)?;
let snapshot = store.snapshot(&scope)?;
let entry = store.read(&snapshot, &entry_id)?;
let saved = store.commit(&scope, snapshot.revision(), &changes)?;
let restored = store.restore(&scope, saved.revision, published_revision)?;
```

这些是阻塞文件 API，app 必须在后台执行。scope 的个人/项目/工作树身份由宿主授予，本票不识别 Git 仓库或授权模型访问范围。项目 ID 与工作树 ID 哈希后分别定位目录，显示名、分支名和模型传入的路径不参与目录解析。

快照包含校验后的目录、摘要、原始 Markdown 与来源；在内存中固定同一 revision，后续提交不改变旧快照。对话工具接入后，每次操作须重新取得当前快照并校验授权和遗忘状态，不能继续用旧快照读取已遗忘内容。创建/恢复/重新激活会话加载目录，持续会话不每轮自动刷新，均属于接入票。

相同内容提交返回原版本与空变更回执；过期预期版本返回 `Conflict`。删除单个主题可用，首版拒绝将已有集合整个清空。完整“忘记”及备份/来源抑制规则尚未实现，不能把本票的删除或回滚当成产品遗忘。

对话写入现支持可选的 `supersedes` 主题 ID 列表：用户要求记住、纠正或清理时，先查找现有主题，完整读取保留主题及待合并主题，保留独有事实，再用 `memory_write` 将完整正文与同范围旧主题删除一起发布。保留的 ID 不得出现在删除列表；未完整读取、过期基线、重复或不存在的删除目标均不能部分发布。新会话只读取当前库，发起写入的会话在保存成功或结果待核验后刷新目录；确认合并与目录代次在同一事务提交，使所有活跃会话下一轮刷新目录，普通对话历史保留。目录代次与读取撤销代次独立，同一合并的回执重试不重复递增；回执恢复同时核验正文和旧主题已删除，并返回最早实际发布的版本。历史版本仍保留用于恢复，此操作不等同于彻底遗忘。

来源引用支持 `UserTurn`、`ToolReceipt`、`SettingsEdit`，随主题保存在 manifest。存储校验引用标识格式；真实来源绑定和证据核实由后续 app 接入完成。无来源的原始主题可保存，不能据此宣称事实已经验证。

## 文件格式与布局

根目录使用 `DatabasePaths` 提供的 App 数据根下 `norma/memory`；Production 与 Test 分别沿用 `Norma OS`、`Norma OS Test`。独立测试只使用临时显式数据根。

```text
norma/memory/
  personal/
  projects/<sha256(project-id)>/
    shared/
    worktrees/<sha256(worktree-id)>/

每个范围：
  LOCK
  CURRENT                         # revision + manifest SHA256
  revisions/<20位revision>/
    manifest.json                 # 范围、版本、父版本/校验值、主题来源与校验值
    MEMORY.md                     # 从主题名称/描述生成
    topics/<entry-id>.md           # 原始 UTF-8 正文
```

主题 ID 为 1–128 个小写 ASCII 字母、数字、下划线或横线，保留 `memory` 名称；中文名称写入 frontmatter。无 frontmatter 的文件逐字保留，以主题 ID 作为名称、描述为空。frontmatter 是明确的单行文本子集，不是完整 YAML 解析器：

```markdown
---
name: 交流偏好
description: 用户希望简洁中文
type: user
modified: 2026-10-03
---
请用简洁中文交流。
```

有 frontmatter 时 `name` 与 `description` 必需，`type`、`modified` 可选。类型支持 `user/feedback/project/reference`；字段值支持原样单行文本、JSON 双引号字符串、单引号字符串。未知/重复字段、控制字符、空正文和未完成 header 返回明确错误；原文不会被自动重写。`modified` 作为显示文本保留，尚未承担时序判断职责。

单主题上限 256,000 bytes、目录 25,000 bytes；当前集合正文上限 16 MiB、manifest 1 MiB。每次保存复制有界完整集合，旧版本暂不自动清理；容量和历史回收需求明确后再升级，避免先实现索引/压缩/复杂合并。

## 发布、失败与恢复

每个范围用 std `File::try_lock` 获得跨进程排他锁，忙时返回 `Busy`。读取不等待写锁：新集合在独立 revision 目录准备、同步并经读取路径复验后，才原子替换 CURRENT 并同步范围目录。

路径 IO 使用已安装的 libc POSIX `openat/mkdirat/renameat` 和目录 fd；逐组件 `O_NOFOLLOW`，拒绝路径分隔、符号链接、非普通文件和硬链接文件。app 数据根是宿主配置，范围/主题输入不能把 IO 引到任意路径。

准备或发布指针之前失败，CURRENT 保持旧集合；重开只认 CURRENT，不扫描并采纳最大编号目录。中断目录占用的版本号不会复用，临时指针按该唯一版本命名，旧进程/PID 的残留文件不会阻止新提交。指针已可见但最终目录同步失败，返回 `PublishedButUnsynced { revision, source }`，调用方必须重新检查当前快照再决定下一步。

回滚沿 CURRENT 的校验值及 manifest 父链查找已发布历史，未发布候选不能成为回滚来源；恢复旧内容再发布新版本，不把 CURRENT 指回旧编号，避免过期写入重新获得有效预期版本。损坏文件、目录或 manifest 均失败，不静默选择另一份内容。

## 验收映射

| 票中验收 | 公共边界测试 |
| --- | --- |
| 中文保存、目录生成、重开 | `chinese_memory_survives_reopening_with_its_generated_catalog` |
| 项目共享、项目/工作树隔离 | `project_shared_and_worktree_scopes_do_not_leak` |
| 目录与正文同版本 | `snapshots_conflicts_and_restore_preserve_complete_revisions` |
| 旧写入、跨进程冲突、路径逃逸 | `snapshots_conflicts_and_restore_preserve_complete_revisions`、`another_process_cannot_publish_while_scope_is_locked`、agent ID 测试与 symlink 测试 |
| 发布失败、中断恢复、空候选 | `publication_failure_keeps_previous_revision_and_allows_retry`、`interrupted_preparation_is_never_a_read_or_restore_source`、`stale_pointer_from_a_reused_process_id_does_not_block_publication`、`invalid_and_empty_candidates_preserve_live_memory` |
| 回滚、未发布候选不参与读取 | `snapshots_conflicts_and_restore_preserve_complete_revisions`、`interrupted_preparation_is_never_a_read_or_restore_source` |

2026-10-03 验证结果：15 个新增聚焦用例通过；`cargo test --workspace` 为 1,749 passed、31 ignored（158 suites）。`cargo check -p agent -p storage`、`cargo clippy -p agent -p storage --all-targets -- -D warnings`、`cargo fmt --check` 及存储边界/行数/禁止 allow 护栏均通过。正式规范与需求两轴审查均无遗留问题。

本票没有页面修改，不涉及真实 App 视觉验收；workspace 测试没有运行已禁用的旧 e2e。
