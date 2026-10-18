# GitHub Rust 编译缓存方案复核（2026-10-18）

本轮通过 GitHub 搜索、仓库 README、源码、提交与 release API 重新核对，不安装工具、不执行清理、不构建项目。结论是：**本项目适合保留每个并行 worktree 的独立 target，研究 APFS 克隆预热，并在任务结束时回收；swarfr 可作为压缩/去重试点，sccache 作为可选重建加速器。** 永久共享可写 target 的建议需要收紧。

## 1. 证据边界与现状

- 上一轮本机只读统计：主仓库 `target` 约 5.4 GiB，`~/.cache/norma-*` 约 59.8 GiB，GPT 独立目录约 6.6 GiB；本轮未重新测量。incremental 已包含在这些目录中，不能再相加。
- 当前 dev 已减少调试信息，release 已有 ThinLTO/strip；这些既有措施的收益不能再次记为新增方案收益。
- 所有下表中的工具收益都是机制判断或上游自测，**没有本项目的安装后/A/B 实测**。APFS clone 的共享块也不能简单按多个目录的 `du` 之和计算可释放容量。
- GitHub 个别请求出现临时 EOF；关键 README/源码均已重试或改用公开 raw 获取。`releases/latest` 的 404 表示没有该端点可返回的正式 release，不代表仓库不存在；预发布另查 releases 列表。

## 2. 适配与收益表

| 方案 | 对 Norma OS 的收益 | 成本、限制 | 建议 |
|---|---|---|---|
| APFS 克隆预热独立 target，任务结束回收 | 新 worktree 从暖缓存开始；相同文件初始共享物理块，后续写入独立；完成任务的缓存不再长期累积 | 克隆不是永久去重，分叉后会增大；donor 必须停止写入并持有 Cargo 锁；需验证路径依赖与最终运行程序 | **优先小范围验证**，参考 cargo-shared-target 的锁与 staging 流程；本仓库 release 基准脚本已有 `cp -cR` 先例 |
| swarfr：APFS 透明压缩 + 内容去重 | 压缩稳定依赖对象；把相同文件换为 CoW clone；不必先完整删除 target | v0.1.1 为预发布，仓库创建于 2026-09；需扫描、hash、压缩并短时持锁；本项目收益未知 | **第二优先级试点候选**，先限定一个停止构建的可重建目录，做状态/计划/构建前后验证 |
| sccache，设置有界本地缓存 | 清 target 后可以复用可缓存编译结果，降低回收后重建代价 | 额外占磁盘；不清 target；Rust 增量、linking 与普通 check 有限制；不同 worktree 的 cwd 会限制命中 | 可选加速，不能计为直接瘦身；先测 `cargo build/test` 命中率，再决定是否采用 |
| cargo-clean-all | 批量发现并删除已停用项目的整个 target，支持交互/时龄/大小/dry-run | 直接删 `<project>/target`，没有核实到 Cargo 构建锁保护；不能覆盖本项目大量外置自定义 target；冷重建 | 人工盘点普通仓库可用，**不是本项目外置缓存治理主方案** |
| cargo-sweep / gauteoh fork | 按时龄、工具链或容量选择性回收历史产物 | 官方明确无人维护；atime 是启发式；选择性模式跳过 incremental，不能覆盖本轮大热点 | 不设为默认定时清理器 |
| cargo-mark-sweep | 用 Cargo JSON 构建消息标记保留集，再删未引用 hash；可清旧 incremental | 初期项目，只在作者单一大型 macOS 工作区验证；dry-run 也会运行 mark 构建；未标记配置以后重建 | 可研究机制，暂不引入日常自动化 |
| cargo-cache | 盘点/回收 `~/.cargo/registry`、git source 等 | 主要不是 target；默认分支最后提交为 2023-06；不解决本轮数十 GiB 多工作树产物 | 次级诊断工具 |
| cargo-green（supergreen） | BuildKit/OCI 细粒度远程编译缓存，可减少重复计算 | README 最低环境是 Ubuntu/BuildKit；未验证原生 macOS/Metal/GPUI；增加镜像/构建引擎缓存，不自动缩小现有 target | 当前不直接采用 |
| Cargo 原生 artifact 去重 / target GC | 将来由 Cargo 统一处理共享缓存与生命周期 | 对应 PR 尚未合并，需要实验开关；不是当前 stable 已有能力 | 跟踪，暂不写进默认配置 |

## 3. 最相关的新增路线

### APFS 独立克隆比永久共享可写 target 更适合并行任务

[ynishi/cargo-shared-target](https://github.com/ynishi/cargo-shared-target) 的当前实现使用 reflink，保留独立 target，通过 staging 与原子 rename 提升，并锁 donor、拒绝忙碌构建。仓库创建于 2026-09-04，最近提交为 [651ad2a（2026-09-09）](https://github.com/ynishi/cargo-shared-target/commit/651ad2afdeac9bd0ad81bfc6e4f2e8e3211e4fe5)，无 release；CI 仅有 Linux 文件系统证据，README 声称 APFS 支持，不能视为已在本项目 macOS 验收。[seed 源码](https://github.com/ynishi/cargo-shared-target/blob/651ad2afdeac9bd0ad81bfc6e4f2e8e3211e4fe5/crates/shared-target-core/src/seed.rs)

本仓库 [scripts/benchmark-release-size.py](../../scripts/benchmark-release-size.py) 已有 `cp -cR` 克隆 release 缓存的实现先例；本轮未运行它。可借鉴“锁住静止 donor → 克隆至临时目录 → 验证 → 原子提升”的流程，不能只把命令放进 worktree hook 就认定安全。

此前“多个 worktree 共用同一 target”的建议需要收紧。[Cargo 官方 issue #12516](https://github.com/rust-lang/cargo/issues/12516) 仍未关闭；维护者 [epage 的说明](https://github.com/rust-lang/cargo/issues/12516#issuecomment-5194388630) 明确没有背书这种工作流，并列出缓存冲突、锁竞争和缓存污染。相对路径依赖与 mtime 的组合可能错误判定 fresh。[第三方复现](https://github.com/structwafel/cargo-shared-target-repro) 报告了静默链接错误 path dependency。本项目未复现该错误；这是方案选择风险，不是已发生的产品 bug。

另外两个相近方案目前不能直接用于本机：

- [cargo-pool](https://github.com/louisponet/cargo-pool)：创建于 2026-10-16，最近 [c4dedbd/10-17](https://github.com/louisponet/cargo-pool/commit/c4dedbdd5a68ff423f42c4d1af1ff37776646ff0)，无 release；池化 slot、租约和 GC 有价值，但 README 明确 Linux only，并接管 hooks/wrapper、调整源码 mtime。
- [rust-dependency-seed](https://github.com/benthecarman/rust-dependency-seed)：[bf030b7/2026-09-15](https://github.com/benthecarman/rust-dependency-seed/commit/bf030b7977c1c67b0d5fc1a96913bbad175a0a69)，Linux/Paseo/Rust 1.97；依赖 rlib/rmeta 硬链接，metadata/native 私有，跳过 active donor 与 workspace/path crates，不带 incremental。本机 macOS/Rust 1.91.1 不匹配。上游节省比例不可转作本项目预测。

### swarfr 的机制符合本项目问题，但成熟度不足

[swarfr README](https://github.com/pyrlyn/swarfr) 提供 `compress`、`dedupe`、`seed` 和显式启用的 `orphans/evict/incremental/doc`；`evict` 可按全局容量上限回收。最近默认分支提交 [61cdaf2/2026-10-18](https://github.com/pyrlyn/swarfr/commit/61cdaf2e14ffdb71c340738b7ca8c4684dbd3431)，[v0.1.1/2026-10-14](https://github.com/pyrlyn/swarfr/releases/tag/v0.1.1) 是 prerelease，并有 Apple Silicon 二进制。正式 `latest` 仍指旧名 dunnage 的 v0.1.0，不能据此误读为没有新版本。

核对到的当前源码保护：

- [engine.rs](https://github.com/pyrlyn/swarfr/blob/61cdaf2e14ffdb71c340738b7ca8c4684dbd3431/src/engine.rs) 对 Cargo profile 锁使用非阻塞 `try_lock`，忙碌目录跳过；锁持有期间修改，跨目录按顺序拿锁。[session.rs](https://github.com/pyrlyn/swarfr/blob/61cdaf2e14ffdb71c340738b7ca8c4684dbd3431/src/session.rs) 另有 run lock，避免 daemon 与人工执行互撞。
- [macos.rs](https://github.com/pyrlyn/swarfr/blob/61cdaf2e14ffdb71c340738b7ca8c4684dbd3431/src/sys/macos.rs) 使用 `fclonefileat` 与 applesauce/LZFSE；压缩后读回校验。按 [DESIGN](https://github.com/pyrlyn/swarfr/blob/61cdaf2e14ffdb71c340738b7ca8c4684dbd3431/DESIGN.md)，在临时文件上修改、恢复 mtime/mode、再替换，处理同一 inode 的 hardlink 组。
- [平台说明](https://github.com/pyrlyn/swarfr/blob/61cdaf2e14ffdb71c340738b7ca8c4684dbd3431/docs/platforms.md) 区分 CoW 与硬链接：build artifact 的硬链接可能让 rustc truncate 影响其他 target，因此无 CoW 时只有显式 `--link-artifacts` 才启用。这种硬链接不能作为默认替代方案。
- `status/advise` 可盘点，`run --dry-run` 可计划；dry-run 会维护工具自身 hash index，并非完全零写入。真正删除必须显式选 lossy pass；README 声称忙锁返回退出码 2。[使用说明](https://github.com/pyrlyn/swarfr/blob/61cdaf2e14ffdb71c340738b7ca8c4684dbd3431/docs/usage.md)

README 的“两份 target 3.63 GiB → 0.93 GiB，约 74%”是作者单机/APFS/587-crate workspace 的 [上游基准](https://github.com/pyrlyn/swarfr/blob/61cdaf2e14ffdb71c340738b7ca8c4684dbd3431/docs/bench.md)，**不能填入 Norma OS 收益栏**。压缩 clone 若顺序不当会解除共享、反而增加磁盘，因此不建议自行串联通用压缩器与去重器；该工具也仍在征集测试志愿者。本轮核对的是当前 commit，发布版是否具有全部相同保护仍需实施前按 tag 再验。

另外，尚未验证扫描 root 能否自动发现本项目 `~/.cache/norma-*` 等外置、自定义名称的 target，并正确关联所属 worktree。试点前必须用 `status` 检查发现清单；不能假定扫描仓库根目录就覆盖这约 59.8 GiB。

## 4. sccache：需要纠正的跨 worktree 预期

[sccache v0.18.0](https://github.com/mozilla/sccache/releases/tag/v0.18.0) 发布于 2026-09-14，包含 ARM64/x86_64 macOS 二进制；当前源码持续维护。它保存额外编译结果，在 Cargo 需要重建时恢复输出，**不会替用户删除 target**。[本地缓存文档](https://github.com/mozilla/sccache/blob/v0.18.0/docs/Local.md) 默认上限 10 GB，可用 `SCCACHE_CACHE_SIZE` 收紧；同一个本地 cache 只能有一个 server，多个 server 会竞争并可能失败。

本项目应先验证以下限制：

1. [Rust 文档](https://github.com/mozilla/sccache/blob/v0.18.0/docs/Rust.md) 要求 `--emit` 含 `link`；普通 `cargo check` 的 metadata-only 编译不在这一支持范围。不能为了 check 任务就承诺明显命中收益。
2. 增量编译 crate 不缓存；调用系统 linker 的 bin/dylib/cdylib/proc-macro 不缓存。关闭 incremental 后磁盘下降来自关闭 incremental，本身不是 sccache 的去重收益。
3. [README](https://github.com/mozilla/sccache/blob/v0.18.0/README.md#normalizing-paths-with-sccache_basedirs) 泛称 `SCCACHE_BASEDIRS` 可归一化路径，但同一 tag 的 [Rust 源码](https://github.com/mozilla/sccache/blob/v0.18.0/src/compiler/rust.rs#L1503) 仍注明 full paths 归一化 TODO，并在 [L1594](https://github.com/mozilla/sccache/blob/v0.18.0/src/compiler/rust.rs#L1594) 无条件将 cwd 纳入 key；`--remap-path-prefix` 是 [PassThrough](https://github.com/mozilla/sccache/blob/v0.18.0/src/compiler/rust.rs#L1047)，参数自身也参与 hash。**不能承诺仅设置 BASEDIRS/remap 就能让不同路径 worktree 的 Rust 成员命中。** 当前 main 也保留该 cwd hash。

因此更可信的试点是：固定工具链/flags，复用同一 Cargo Home 的 registry 依赖或同路径冷重建；用 `--show-stats` 记录 cacheable、hit、miss 与不可缓存原因，测 target + sccache 的总物理占用。只有总空间与重建耗时都符合目标，才把 sccache 纳入日常任务。

## 5. 清理工具核对

| 工具及版本状态 | 已核实能力 | 关键限制/源码依据 |
|---|---|---|
| [cargo-clean-all v0.6.5，2026-08-30](https://github.com/dnlmlr/cargo-clean-all/releases/tag/v0.6.5) | `--dry-run`、交互选择、`--keep-days/--keep-size`、保留 executable | [main.rs:378](https://github.com/dnlmlr/cargo-clean-all/blob/f4660ceee200dd086e797a71a3aa71aaa6b58676/src/main.rs#L378) 直接删 `project_path.join("target")`；没有经 Cargo metadata 解析自定义目录，也未见构建锁；应在停止构建时人工使用 |
| [cargo-sweep v0.8.0，2025-10-11](https://github.com/holmgr/cargo-sweep/releases/tag/v0.8.0)；README 2026-05-26 更新 | 时龄、已安装工具链、maxsize、stamp、dry-run | [官方 README](https://github.com/holmgr/cargo-sweep) 明确 unmaintained；[fingerprint.rs](https://github.com/holmgr/cargo-sweep/blob/94a7cf012d40d314896c0fa6986132b0a1e931ba/src/fingerprint.rs#L278) 选择性清理跳过 incremental；删除逻辑未见 Cargo 锁保护 |
| [gauteoh/cargo-sweep，2026-10-02](https://github.com/gauteoh/cargo-sweep/commit/8e3e21b1ecd24c83d9c55701fad0ad4826669606)，无 release | fork 增加显式 target、ownership marker、去重目标与失败报告 | [README](https://github.com/gauteoh/cargo-sweep) 仍保留 unmaintained 警告；承认 atime 不精确、no-op build 可不更新，可能删仍需产物；不是已接棒的稳定新上游 |
| [cargo-mark-sweep v0.2.0，2026-08-03](https://github.com/JoshKneale/cargo-mark-sweep/releases/tag/v0.2.0) | Cargo JSON 标记、失败则中止、清理后 shakedown；macOS/Linux，macOS launchd | [README](https://github.com/JoshKneale/cargo-mark-sweep) 自述单环境验证；[gc.rs](https://github.com/JoshKneale/cargo-mark-sweep/blob/fc33dd07e547dd87674b3b5693b073734e42a730/src/gc.rs) 用 `.cargo-lock`/flock；默认清空 incremental，支持 `--keep-incremental`；dry-run 会执行构建，不是单纯目录扫描 |
| [cargo-cache](https://github.com/matthiaskrgr/cargo-cache)，默认提交 [2023-06-04](https://github.com/matthiaskrgr/cargo-cache/commit/2f4497beeab989becd2ecdae8a42601956e822b0)；tag 0.8.3 为 2022-09-11 | Cargo Home 容量分析、dry-run、autoclean、trim；历史 CI 有 macOS | `local` 主要分析 target；autoclean 主要删解压源码/git checkout，保留 archive/bare repo；不替代 target 生命周期治理，当前 Cargo 的内置全局缓存 GC 也已覆盖部分需求 |

## 6. 暂不直接采用和未来能力

- [cargo-green/supergreen](https://github.com/fenollp/supergreen)：最近默认提交 [0ada7be/2026-10-16](https://github.com/fenollp/supergreen/commit/0ada7be7c7c7d6c02641f9336bc15afde5350c20)，[v0.27.0/2026-07-24](https://github.com/fenollp/supergreen/releases/tag/v0.27.0)。README 描述以 BuildKit/OCI 包装 rustc，最低环境列 Ubuntu 22.04、Docker/Podman、BuildKit；没有找到当前项目原生 macOS/Metal 可用性的证据。营销站还描述 Nix，但仓库 README 的稳定事实是 BuildKit；不按营销站外推能力。
- [manaforged/artificer](https://github.com/manaforged/artificer) 使用 per-digest 锁、不可变发布、容量上限/LRU，声称 macOS ARM64 支持；但要求 Rust ≥1.98，本项目固定 1.91.1，且为新 0.x 工具。本轮没有本项目兼容验证，不能为了缓存工具直接升级整套编译链。
- [Cargo PR #17512](https://github.com/rust-lang/cargo/pull/17512) 提议 CAS blob、reflink/hardlink 去重及 GC；截至本轮仍 open、未合并。上游 tokio 五 worktree 的 690 MiB → 360 MiB、大测试集 10.9 GB → 4.8 GB 是试验结果，不是 stable 功能或本项目收益。
- [Cargo PR #17524](https://github.com/rust-lang/cargo/pull/17524) 追踪 target/build 目录并按时龄 GC，最后更新 2026-10-16，仍 open、未合并，涉及 `-Zgc`。现在不能把自动 target GC 当成 stable 已有能力。

## 7. 下一步验证范围

保留现有工程配置。先给外置构建目录建立任务归属与结束回收规则，再在一份可重建 target 上验证 APFS 克隆预热；如果需要进一步减少仍活跃缓存的物理占用，单独试点 swarfr 的压缩/去重。两条路线都测“磁盘实际占用、首次预热、单文件重建、依赖 fresh 状态、实际运行/测试产物”。sccache 单列为可选加速试验，不与前两者的节省比例相加。

任何工具上游宣称的 35%、74%、90% 都没有转写为 Norma OS 的预期收益。本轮只新增本报告，没有安装、删除、设置全局 wrapper、启用 daemon 或更改 Cargo 配置。

## 8. 成熟度复核（2026-10-18）

**直接影响选择的结论：sccache 已成熟，但解决的是重建加速；这轮找到的 APFS 预热/去重工具大多还很早，不适合直接接管所有 Agent 构建。** 本项目目前更稳妥的是明确构建目录归属、任务结束回收，再隔离试验 APFS 克隆。APFS CoW clone 本身属于 macOS 文件系统提供的基础能力，本仓库已有调用先例；donor 选择、Cargo 锁、路径依赖 freshness、崩溃恢复、容量回收是工具另行实现的工程，不能继承基础能力的成熟度。

以下评级考察发布连续性、维护集中度、平台测试及已知限制，不按星数排序。GitHub release 数只统计非 draft 的 release；git tag、crates.io 发布是不同证据。贡献者 API 按默认分支归属统计，包含 bot，也不等于实际维护者人数。零公开问题不代表没有缺陷。

| 方案 | 证据评级 | 历史与发布 | 维护及 macOS 验证 | 对本项目的判断 |
|---|---|---|---|---|
| sccache | **成熟可用** | 2016-04-26 创建；2026 年 1/2/4/6/7/9 月连续正式发布；v0.18.0/9-14 有双架构 macOS binary | contributor API 第一页即 100 条记录（含 bot）；10-17 main 实际 ARM/Intel macOS 测试成功 | 可以作为有界的可选编译加速器；不能把成熟度转译成能回收 target 或跨路径必命中 |
| cargo-clean-all | **人工辅助** | 2022-02-25 创建；2023–2026 多次发布，最新 v0.6.5/8-30 | 6 名贡献者，主要作者 64 commits、第二位 8；未找到 GitHub Actions/macOS CI 证据 | CLI 有使用历史，但 issue #27 明确不处理 `CARGO_TARGET_DIR`，本项目主占用范围不匹配 |
| cargo-sweep | **人工辅助，维护中断** | 2018 年创建，有历史版本；README 明确 unmaintained | 历史 26 名贡献者不等于仍有人维护；5-26 最近 main CI 为失败，不据此推断具体产品 bug | 不用于自动回收活跃构建；已有替代路线，不优先引入 |
| swarfr | **早期试验** | 2026-09-18 创建；2 个 GitHub release：v0.1.0 正式版/9-23（旧名 dunnage）、v0.1.1 预发布/10-14 | API 记录 1 名人类贡献者 55 commits，另 1 bot；10-18 实际 Apple Silicon macOS job 成功；最新两次 CI 查询时仍 queued | 有真实 macOS 测试和可核对代码，适合单目录试点；尚缺长期、多用户生产证据 |
| cargo-shared-target | **未发布，源码试验** | 2026-09-04 创建；0 GitHub release，只有 v0.1.0 git tag；最近默认提交 9-09 | 1 人、5 commits；9-09 CI 成功，但全部是 Ubuntu 上的 ext4/btrfs/xfs 与 lint，没有 macOS job | 核心流程可借鉴，APFS 支持仍需本机验证；不直接装成全局 worktree hook |
| cargo-pool | **未发布，源码试验** | 2026-10-16 创建；0 GitHub release、0 tag；最后更新 10-17 | 1 人、15 commits；未找到 `.github` workflow，Actions runs 为空；README 明确 Linux only | 公开历史约 2 天，且平台不匹配，当前排除 |
| Artificer | **早期试验** | 2026-09-29 创建；GitHub release/tag 均为 0；README 描述 0.x，crates.io 安装状态本轮未成功在线核验 | 1 人、100 commits；10-15 实际 macos-latest 测试成功，并在 POSIX job 验证安装→shim 构建夹具→卸载 | 测试投入比“只有 README”更充分，但主干仍快速修正确性；要求 Rust ≥1.98，与本项目不匹配 |
| Cargo 原生去重/target GC | **未发布** | PR #17512、#17524 均 open、merged=false | 有 Cargo 上游讨论与试验，尚不属于 stable 的可用保证 | 跟踪即可，不把未合并 PR 当成现成方案 |

### 新工具的发布内容与当前 README 是否一致

- **swarfr**：[v0.1.1 与本轮 main 的比较](https://github.com/pyrlyn/swarfr/compare/v0.1.1...main) 相差 5 commits，修改集中在 CI、README 和文档，没有 `src/` 变化。因此当前核对的压缩、CoW 去重和锁核心代码在 v0.1.1 tag 已有对应；文档最新表达和 CI 配置不能全部当成发布时状态。其 [10-18 CI](https://github.com/pyrlyn/swarfr/actions/runs/39098451074) 中 `rust / aarch64-apple-darwin` 在 `xcode-27` runner 实际 success。查询时 06:39 和 06:53 UTC 两次更新的 CI 仍排队，未用旧绿灯替代最新结论。当前 open 列表的 3 条均为 PR，没有由此获得广泛使用无 bug 的证据。[release 列表](https://github.com/pyrlyn/swarfr/releases)、[贡献者 API](https://api.github.com/repos/pyrlyn/swarfr/contributors)
- **cargo-shared-target**：[v0.1.0 tag README](https://github.com/ynishi/cargo-shared-target/blob/v0.1.0/README.md) 已有 donor 锁、reflink、staging rename；[main 比 tag 多 1 commit](https://github.com/ynishi/cargo-shared-target/compare/v0.1.0...main)，新增 staging marker、残留 `--prune` 与回收锁，明确写在 [CHANGELOG 的 Unreleased](https://github.com/ynishi/cargo-shared-target/blob/main/CHANGELOG.md)。不能把该恢复能力归到 v0.1.0。其 [最近成功 CI](https://github.com/ynishi/cargo-shared-target/actions/runs/34310588318) 只有 Linux 文件系统测试；零 open issue 不补足 APFS 证据。
- **cargo-pool**：没有 tag/release 可用于冻结已发布机制；[README](https://github.com/louisponet/cargo-pool) 的 slot、锁、GC、mtime 恢复仅有当前源码和本仓库测试文件，缺公开 Actions 运行记录。其 [tag 列表](https://api.github.com/repos/louisponet/cargo-pool/tags)、[release 列表](https://api.github.com/repos/louisponet/cargo-pool/releases)、[Actions](https://github.com/louisponet/cargo-pool/actions) 查询均未提供已发布、已跨平台验收的证据。
- **Artificer**：[10-15 成功 CI](https://github.com/manaforged/artificer/actions/runs/38818248781) 有真实 macOS、Linux、Windows 测试；PR 模式只跑 Linux，push/main 才跑三平台，不能把每次 PR 绿灯都当成 macOS 通过。[CHANGELOG Unreleased](https://github.com/manaforged/artificer/blob/main/CHANGELOG.md) 仍记录并行构建相互等待、metadata/link 时序、incremental 开关切换后找不到 rlib/rmeta 等关键修复，说明兼容性仍在快速收敛；没有 GitHub tag 可证明 README 当前机制全部属于某个已发布版本。crates.io 查询失败，本文只断言“GitHub 无 release/tag”，不声称它从未发布。[workflow](https://github.com/manaforged/artificer/blob/main/.github/workflows/ci.yml)

成熟工具的补充依据：[sccache ARM macOS 成功 job](https://github.com/mozilla/sccache/actions/runs/39024071220/job/117371478204)、[Intel macOS 成功 job](https://github.com/mozilla/sccache/actions/runs/39024071220/job/117371478323)、[integration-tests](https://github.com/mozilla/sccache/actions/runs/39024071161)；[cargo-clean-all #27](https://github.com/dnlmlr/cargo-clean-all/issues/27)；[cargo-sweep 维护声明](https://github.com/holmgr/cargo-sweep#readme)。这些证据不替代本项目对外置缓存、钉死 Cargo 1.91.1、并行 worktree 和实际程序产物的验证。
