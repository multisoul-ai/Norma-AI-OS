# Windows 存储 15 项失败

仅恢复用户列出的 15 项存储测试；其他暂停范围保持不变。Win10 真机和真实 Claude/Codex 由用户验收。全部测试、Clippy、构建在 GitHub runner，本地仅编辑和证据分析；正常提交 hook 照常执行。不改用户已有数据库，不合并、不发布。

## 基准与隔离

- 新工作树 `/private/tmp/norma-windows-storage-15-20261103`，分支 `codex/windows-storage-15-20261103`，基于最新修复包及证据提交 `7c8abb94c131319d6990ba66a3868d841213b743`。
- 该来源保留已通过的 81 项 norma 验证（Server 2022 / 2025，run 41675679325）；仅当相关实现、依赖或条件改变才补增量验证。
- 历史 storage：run 41617015436，源码 7cf61b94，267 通过 / 15 失败 / 0 忽略。清单来自 `docs/quality/evidence/windows-workspace-create-20261103/storage-comparison.json`。
- 旧手机并发工作树有未完成的临时探针和未提交证据，保持原状；本次只读取并复用已有失败记录，不覆盖或误提交。
- storage 独立运行只编译 norma 库，不主动运行被既有 Unix 权限 API 阻塞的 norma 全套或扩大暂停范围；若实际受阻，记录依赖。

## 验收映射

| 组别 | 原失败与入口 | 验收 |
|---|---|---|
| 路径 1 | `database_paths::tests::isolated_test_paths_stay_under_the_explicit_test_boundary` | 保留隔离、模式及明确路径断言；查明路径编码/哈希差异 |
| 旧库初始化 5 | `global_project_storage` 中 legacy_database_is_imported_once_including_committed_wal_data、failed_import_does_not_publish_an_empty_database_and_can_retry、constructing_a_project_store_cannot_bypass_legacy_import、test_mode_never_imports_the_production_legacy_database、concurrent_initialization_publishes_one_complete_database | 原公共 ProjectStore 契约，WAL 数据与重试/并发不丢失，不修改原库 |
| SQLite 并发 3 | live_concurrent_claim_is_unique_and_unfinished_restart_is_unknown、mobile_command_is_reserved_once_and_receipt_is_immutable、concurrent_creation_migrates_legacy_chat_time_once | 原测试文件默认并行重复；唯一拥有者、不可覆盖回执、时间迁移状态正确 |
| 共享记忆 1 | repository_worktrees_share_only_project_memory | 同仓库 worktree 共享项目记忆，私有记忆隔离，安全目录身份校验不放宽 |
| 手机会话引用 5 | session_index_is_durable_scoped_and_immutable、transcript_reference_uses_existing_writer_and_cannot_be_rebound、cursor_session_persists_bound_transcript_reference、session_history_pages_without_duplicates、native_events_preserve_session_identity_and_record_exact_turn | 原公共存取 API，合法文件引用可用且越界、重绑定、重复分页仍被拒绝 |

手机命令回执是并发 3 项之一，并非第 16 项。旧同机诊断 run 41655531581：基线 2/10 通过，观察来源 3/10 通过，均实际执行完整 3 测试/0 忽略；不能认定由 7cf 直接引入。当时具体 SQL 根因未确认，本轮诊断结果见下方“结果”。

## 执行

1. 在未改生产代码的来源上运行完整 storage、各组和并发文件重复，保存每次真实失败与平台信息，作为本轮 RED。
2. 逐个根因先复用/补能失败的公共 seam 测试，再最小修复；不跳过测试、不放宽断言、不删除数据。必要诊断使用独立前缀并在确认后移除。
3. Windows / macOS 受影响测试与存储全套、边界和必要检查；逐项比较 267/15。对未复现项目明确证据缺口，不把单跑或串行绿当作修复。
4. 独立对抗式审查，正常提交及推送源码与证据；每项以通过或明确阻塞收口，手机回执单列状态。

## 结果

- 未修复基线 [41756328226](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41756328226) 在 Windows 精确复现267/15/0，原15清单不变；Mac283/0/0。
- 路径/会话6项是原生路径夹具错误，保留生产key算法、valid_path及原断言；生产修复包括原生目录sync、同句柄目录身份、冷App迁移整链Immediate并复用内嵌事务。暖库原有guarded检查与user_version1不变。
- 原子回归 [41759894115](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41759894115) 在Win/Mac真实RED：迁移失败后留下半套schema；已修复且完整通过。临时探针全部移除。
- 最终源码 `90522917d26fd2fec8f554d9765950d3c97eb4f0`，[41760987212](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41760987212)：Win286/0/0、Mac287/0/0，原15逐项具名通过，无新增存储失败。每组并发完整文件默认并行重复20轮均通过，Clippy/fmt与边界通过，Windows workspace原生40项通过。
- 手机回执属于原15之一，原失败定位于reserve前的冷迁移；完整文件20/20轮通过，唯一领取、重开Unknown与不可覆盖终态原断言保留。没有把它归为7cf直接代码回归。
- 独立只读审查未发现阻塞；新增负例的版本冲突/NUL路径假阳性均修正。81项norma证据有效复用；未修暂停的Unix权限测试，不重打包、不合并、不发布。
- 完整逐项记录与原始失败见 `docs/quality/evidence/windows-storage-fifteen-20261103/README.md`，机器比较见同目录 `storage-comparison.json`。所有实际DB写入仅发生于runner临时夹具。
