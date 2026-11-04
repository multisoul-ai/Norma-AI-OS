# Windows storage 15 项失败修复

本次仅恢复用户授权的 15 项存储失败，来源为最新启动修复版 `7c8abb94`。测试、Clippy 和运行验证均使用 GitHub hosted runner，本地只编辑与分析证据，正常提交 hook 照常执行。全部数据库夹具位于 runner 的隔离临时目录，没有连接或修改用户现有数据库。不合并、不发布、不重打包；Win10 真机与真实 Claude/Codex 仍由用户验收。

## 来源与未修复基线

- 本轮独立分支 `codex/windows-storage-15-20261103`。未改生产的基线 `8a4090375ccde85c81ac9ed216b13ad171f03f28`，[41756328226](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41756328226)，attempt 1。
- Windows Server 2025 / 10.0.26100、image `20261030.294.1`、4 核、Rust 1.91.1：完整 storage **267 通过 / 15 失败 / 0 忽略**，与此前 7cf61b94 的清单逐项相同；三组并发完整文件各五轮均暴露原失败。macOS 26.6.2 arm64 完整 storage **283 / 0 / 0**，全部分组及并发五轮通过。
- 基线与 7cf 的 storage 源码逐字一致；6 个路径夹具的 Windows 差异没有被当作生产路径算法缺陷。
- 原始 report 的 `failed_names` 从并发输出的单行提取，漏了两条被 stderr 交错的行；没有改写原报告。`baseline-failure-names.json` 独立解析完整 libtest 最终失败列表，确认全部 15 项。完整日志、artifact 服务端 digest 与各次失败均保留。

## 15 项映射与根因

最终验证来源：`90522917d26fd2fec8f554d9765950d3c97eb4f0`，[41760987212](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41760987212)。Windows 全套 **286 通过 / 0 失败 / 0 忽略**，macOS 全套 **287 / 0 / 0**；两个 job 的完整分组、并发二十轮、Clippy/fmt 和边界检查均 PASS。

| 组别 | 测试名 | 已确认原因与最小改动 | Windows 修复证据 |
|---|---|---|---|
| 路径 | database_paths::tests::isolated_test_paths_stay_under_the_explicit_test_boundary | 测试 join 产生反斜杠，实际 SHA 输入与 Unix 常量不同；固定测试输入，保留原 SHA 常量、隔离断言和生产 key 算法 | PASS，分组与完整 storage 均通过 |
| 旧库 | legacy_database_is_imported_once_including_committed_wal_data | 真实失败发生于 persist 后 parent-open，Io5 且 published=true；复用原生目录能力同步，不吞 I/O 错误 | PASS，分组与完整 storage 均通过 |
| 旧库 | failed_import_does_not_publish_an_empty_database_and_can_retry | 同上；继续保留损坏来源不发布、修复后可重试与原库不变断言 | PASS，分组与完整 storage 均通过 |
| 旧库 | constructing_a_project_store_cannot_bypass_legacy_import | 同上；保留正式构造入口强制先导入 | PASS，分组与完整 storage 均通过 |
| 旧库 | test_mode_never_imports_the_production_legacy_database | 同上；保留 Production/Test 来源隔离 | PASS，分组与完整 storage 均通过 |
| 旧库 | concurrent_initialization_publishes_one_complete_database | 同上；保留 read-only SQLite backup、暂存迁移、file sync 与 persist_noclobber | PASS，分组与完整 storage 均通过 |
| 并发 | live_concurrent_claim_is_unique_and_unfinished_restart_is_unknown | 预留前冷 App schema 迁移多次独立写入，另一 opener 在建表处耗尽 busy 等待；冷库迁移整链复用一个 Immediate 事务 | PASS；默认并发 20/20 轮 |
| 并发 | mobile_command_is_reserved_once_and_receipt_is_immutable | 同一冷迁移根因，业务 reserve 原本已有 Immediate，未修改业务领取/回执状态机 | PASS；默认并发 20/20 轮 |
| 并发 | concurrent_creation_migrates_legacy_chat_time_once | 同一冷迁移根因；保留旧时间列迁移的锁内重查，内嵌迁移复用外层事务 | PASS；默认并发 20/20 轮 |
| 记忆 | repository_worktrees_share_only_project_memory | 原 Windows binding 明确拒绝目录身份；从同一 pinned handle 读取 volume/index/creation-time，保留原二元组 API 和 Unix 签名 | PASS，分组与完整 storage 均通过 |
| 会话 | session_index_is_durable_scoped_and_immutable | helper 的 /tmp/project-a 在 Windows 非绝对路径；改成用例自身临时目录下的原生绝对路径，生产 valid_path 不变 | PASS，分组与完整 storage 均通过 |
| 会话 | transcript_reference_uses_existing_writer_and_cannot_be_rebound | 同上；原 writer、不可重绑定、错会话与跨库负断言保留 | PASS，分组与完整 storage 均通过 |
| 会话 | cursor_session_persists_bound_transcript_reference | 同上；保留只保存原生路径和不复制正文的契约 | PASS，分组与完整 storage 均通过 |
| 会话 | session_history_pages_without_duplicates | 同上；原 21=20+1 分页、幂等与不重复断言保留 | PASS，分组与完整 storage 均通过 |
| 会话 | native_events_preserve_session_identity_and_record_exact_turn | 同上；原会话/回合/项目归属与历史标题不变断言保留 | PASS，分组与完整 storage 均通过 |

## 诊断与先失败证据

[阶段诊断 41757755342](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41757755342)，来源 `11a81b52`：仅修正路径夹具、增加失败时的阶段探针与负例，尚无目录身份或事务实现。

- Windows 路径 1/0、会话 6/0，原业务断言全部保留。新增负例最终已收紧为原生绝对路径加 NUL，避免被非 absolute 条件提前拒绝。
- 旧库 2/5；五项均在 `parent-open` 记录 Io5 / PermissionDenied、`published=true`。损坏来源先在 backup 阶段报 NotADatabase，修复后成功发布才遭遇目录打开错误；两种错误没有混为一谈。
- 共享记忆 0/2；原跨 worktree 与新增同路径普通目录负例均在 Windows unsupported binding 处失败。
- 并发：Live 4/5 轮失败、手机 5/5、chat time 5/5，均在业务预留前的迁移中失败，多为 `question_delivery_store::migrate` 首个 CREATE，另一次为 mobile_session 建表；`autocommit=true`，约 6.3–6.7 秒后 DatabaseBusy。不是业务 reserve 少了 Immediate。
- 临时 `[DEBUG-storage15]` 探针在生产修复中全部移除，原诊断日志不改写。

[原子迁移 RED 41759894115](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41759894115)，来源 `fdf5c60c`：Windows 与 macOS 均真实执行 2 条测试，1 通过 / 1 失败 / 0 忽略。末段索引因缺少 workspace_id 失败后，旧实现留下 49 张半迁移表；断言要求只保留最初旧表，因此正确失败。另一个 user_version=1 但缺后半 schema 的用例通过，防止拿旧版本号当完整标记。

修复使用独立 revision marker，只在冷/未标记库获取写事务，锁内二次检查，全链和最终 marker 一起提交；失败整体回滚。已有 marker 的暖库继续原有 guarded 检查，不增加外层写事务，避免遮蔽原有旧库回填语义。原 user_version=1 不变；marker 不降低较高 revision，这不代表旧代码兼容所有未来 schema。

## 手机命令回执

它是原 15 项中并发 3 项之一，并非额外第 16 项。此前同机历史 [41655531581](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41655531581) 已证明基线也可复现，不能归为 7cf 直接引入。此次阶段日志确认发生在冷迁移，尚未进入原 Immediate 业务领取。

修复版在 Windows 与 macOS 均以完整 `mobile_commands` 文件默认并行重复 **20/20 次通过**（每轮真实 3 测试 / 0 失败 / 0 忽略），保留唯一 New / 唯一 Existing、重开 Unknown、具体 node_id=7 的完成结果、拒绝 node_id=8 覆盖，以及不同手机/项目/正文不可认领的原断言。LiveOperation 与 chat time 完整文件也各二十轮通过。没有串行化测试或使用 skip/ignore；每轮命令、完整日志、退出码均保留。详见 `mobile-receipt-status.json`。

## 最终核对

- [41760987212](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41760987212)，attempt 1，源码 `90522917d26fd2fec8f554d9765950d3c97eb4f0`。Windows job `126348380558`、macOS job `126348380892` 均 SUCCESS。
- 原 15 项逐项在修复后的分组日志找到具名 `ok`，机器可核对映射和日志 SHA256 见 `storage-comparison.json`。没有剩余原失败或新增 storage 失败。
- 计数：Windows `267 + 15 已解决 + 4 新增 = 286`；macOS `283 + 4 新增 = 287`。新增四项分别为非法会话路径、同路径记忆身份替换、迁移失败原子回滚和旧 user_version=1 不完整 schema。
- 三组并发完整文件各 20 轮（Windows 共 60 个独立文件运行，macOS 同样 60 个），所有原测试默认并行、0 忽略；暖鉴权持写锁仍成功，原 data_version 断言通过。
- Windows 变更依赖的 workspace 库与 scoped_directory 共 40 项通过，storage all-targets Clippy、workspace lib/scoped_directory Clippy、格式、存储及性能边界均通过。未遇到 norma 的既有 Unix 权限测试编译阻塞。
- 前后 Windows image、平台、4 核数与完整 Rust 编译器字符串相同；不同 runner VM 的资源噪声仍可能不同。Server 2025 结果不是 Windows 10/11 实机结果。
- 最终 Windows artifact `13291500842`，SHA256 `ab02270d4a8c725473cd40c15daa14060c4996362612f9dabe2d8b9f0b142866`；macOS artifact `13290282865`，SHA256 `0ed86ecb0fcd0913807b87733a656780e844387c31123327a271d2b9a88c0b1d`。所有本轮 artifact ZIP 已在解压前核对服务端 digest。
- runner artifact 内的 cargo 日志与 report JSON 按原字节保存，包含 Windows CRLF 和原始尾部空白；GitHub 完整 job 文本另存作换行归一副本。旧失败轮次未重跑覆盖或删改。原创分析文档单独做空白检查，不格式化原始证据。
- 逐文件字节数、SHA256、源码提交与 run 对应关系见 `evidence-manifest.json`，清单排除自身；提交前常见密钥格式扫描未命中。

## 新增护栏与独立审查

- 会话非法路径拒绝后不持久化身份；absolute + NUL 独立检验 NUL 条件，另覆盖相对、父级、Windows rooted/drive-relative。
- 普通目录在完全同一路径替换，证明 project key/root 前后相等；迟到写使用真实原 revision 与独立 operation，必须精确得到身份 Rejected。恢复原目录到原路径后，原 revision 和原主题不变，late 主题不存在，新目录 scope 为空。
- native 目录三元组来自同一个 pinned File 的一次信息读取；不同句柄稳定，同路径替换变更。原二元组接口和 clone/delete 安全测试未改。
- 迁移失败回滚保留原表及原行；通过重命名保留原表修复夹具后可重试，没有删数据制造通过。
- 原 `authentication_does_not_write_or_wait_for_a_writer` 完整保留：另一连接持有 Immediate 写锁时鉴权仍成功，data_version 不变。
- 独立只读 reviewer 审查 `51d642ed` 未发现阻塞；确认内嵌事务覆盖、失败回滚、marker 提交时机、暖读和 legacy 不覆盖行为。发现的测试假阳性已修正，未运行本地测试或构建。

## 验证边界

本次没有修改 `Cargo.toml`、`Cargo.lock`、`crates/norma/` 或 `crates/mobile_pairing/`；复用 [81 项有效验证 41675679325](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41675679325)，不重复运行或声称 Windows norma 全套通过。存储独立编译未受那个既有 Unix 权限测试阻塞，也没有改其代码。

之前的启动修复 ZIP 不包含本轮存储变更。本轮只交付源码与证据；不自动合并、发布或重打包，也不代替用户进行 Windows 10 19045 真机和真实模型验收。
