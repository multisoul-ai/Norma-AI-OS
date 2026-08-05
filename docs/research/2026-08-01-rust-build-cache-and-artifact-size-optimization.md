# Rust 编译缓存与中间产物体积优化调研

日期：2026-08-01
范围：本地开发、Cargo 全局缓存、workspace、Git worktree、CI/CD
结论性质：只读调研与方案评估；本报告未实施任何配置或代码变更

## 0. 明确推荐方案（决策版）

### 0.1 一句话决策

**不把任何第三方工具作为根治依赖。** 推荐方案是：经用户明确授权后先用 Cargo 自带的 `cargo clean` 一次性释放旧 `target`，随后采用 Cargo 官方 dev 调试信息配方作为永久配置；本地第一轮保留 incremental，只有代表性工作流仍超过磁盘预算时才关闭；`sccache` 仅作为限额 5 GB 的可选重建加速器。Cargo 官方明确说明 `line-tables-only`、依赖 `debug = false` 和按需完整 debugging profile 会同时减少 codegen、链接时间与 `target` 占用，这与当前约 40.74 GiB 调试对象的主矛盾直接对应。[Cargo 官方构建性能指南](https://doc.rust-lang.org/cargo/guide/build-performance.html#reduce-amount-of-generated-debug-information)

### 0.2 P0：紧急释放空间，先清旧产物

当前只剩约 799 MiB，可用空间不足以安全建立第二份独立 target 做 A/B。实施阶段应先：

1. 保存本报告中的 71/63/46/40.74/16/7.2 GiB 基线。
2. 先执行只读预览：`cargo clean --dry-run --verbose`。
3. **取得用户对删除整个 workspace `target` 的明确授权后**，执行一次 `cargo clean`；按当前基线预计释放约 71 GB，代价是下一次构建为冷构建。Cargo 官方定义 `cargo clean` 为删除 Cargo 在 target 目录生成的产物。[cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)
4. 不动 `~/.cargo/bin`、配置、凭据或整个 `CARGO_HOME`；其当前约 2.5 GB，不是主要矛盾。Cargo 1.88+ 已对 Cargo Home 下载/可重建缓存提供稳定自动 GC，但不会清 workspace target。[Cargo 1.88 changelog](https://doc.rust-lang.org/cargo/CHANGELOG.html#cargo-188-2025-06-26)

永久 profile 变更会生成不同构建指纹；在磁盘只剩约 799 MiB 时直接改配置并重建，会让旧 71 GB 的保留价值很低、峰值风险很高。因此 P0 的完整 clean 是结构性配置落地前的必要止血，但本报告不执行删除。

### 0.3 P1：永久配置只采用 Cargo 官方配方

建议后续变更为以下 profile；这是本报告的**主方案**，不是试验性第三方方案：

```toml
[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false

[profile.debugging]
inherits = "dev"
debug = true
```

含义是：日常 workspace crate 只生成能够让 panic backtrace 带文件/行号的最小调试信息，第三方依赖不生成调试信息；需要 LLDB 高质量变量和参数信息时，显式使用 `--profile debugging` 完整重建。Cargo 官方列出的收益是更快 codegen、更快链接和更小 target，明确代价是高质量 debugger 体验需要完整重建。[Cargo 官方构建性能指南](https://doc.rust-lang.org/cargo/guide/build-performance.html#reduce-amount-of-generated-debug-information) `line-tables-only` 不包含变量或函数参数信息；完整含义见 [Cargo profile `debug`](https://doc.rust-lang.org/cargo/reference/profiles.html#debug)。

本地 incremental 第一轮**保持默认开启**。它只用于 workspace member 和 path dependency，是用磁盘换编辑后重编译速度的缓存；当前约 16 GB 虽然显著，但应先观察调试信息主问题解决后的稳态，再决定是否牺牲本地反馈速度。[Cargo profile `incremental`](https://doc.rust-lang.org/cargo/reference/profiles.html#incremental) CI 继续关闭 incremental；现有 `Swatinem/rust-cache@v2` 会设置 `CARGO_INCREMENTAL=0` 并在保存前移除 incremental。[Swatinem/rust-cache 官方仓库](https://github.com/Swatinem/rust-cache)

### 0.4 macOS `split-debuginfo`：先理解，暂不设正式默认

`split-debuginfo` 决定**已生成调试信息放在哪里**，`debug` 才决定**生成多少调试信息**。因此它不是主方案的替代品。[Cargo Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#split-debuginfo)

| 值 | macOS 精确行为 | 磁盘/速度权衡 | 本项目决策 |
|---|---|---|---|
| `unpacked` | Cargo 在开启 debug info 的 profile 上默认使用；调试信息留在各 codegen unit 的原始 object 中，rustc 为调试保留这些 object | 会保留大量散落的 `*.rcgu.o`，与当前 40.74 GiB 现象一致；避免 `dsymutil` 打包步骤 | P1 先在官方减量配方下维持默认 |
| `packed` | 调用 `dsymutil` 把调试信息收集到独立 `*.dSYM`，随后原始 object 可删除 | 可能降低散落 object 占用，但新增打包时间和 dSYM；总占用是否更小必须按本项目 A/B | 仅作为后续 A/B 候选 |
| `off` | macOS 上不运行 `dsymutil`，rustc 也不为 split debug info 保留 object | 磁盘最激进，但 macOS 上没有可依赖的 packed 完整调试信息，不能保证日常 LLDB 质量 | 不作为未经验证的正式默认 |

三种值的官方语义见 [rustc codegen options](https://doc.rust-lang.org/rustc/codegen-options/index.html#split-debuginfo)；rustc 实现明确显示 macOS `packed` 运行 `dsymutil` 后清理 object，而 `off`/`packed` 不为 split debug info 保留 object、`unpacked` 保留，见 [rustc linker source](https://doc.rust-lang.org/beta/nightly-rustc/src/rustc_codegen_ssa/back/link.rs.html#1657-1680)。这里能确定文件布局和对象保留行为，但不能从语义直接推出本项目 packed 后的精确 GiB，所以不在没有 A/B 的情况下写死 `off` 或 `packed`。

### 0.5 P2：第三方只做可选增强

- **`sccache`：可选，容量硬上限 5 GB。** 它缓存 rustc 编译结果并用于跨 clean、worktree 或 CI checkout 加快重建；Cargo 官方把它定位为跨 workspace 的 shared cache，而不是 target 清理器。[Cargo Build Cache](https://doc.rust-lang.org/cargo/reference/build-cache.html#shared-cache) 它不会删除或缩小 target，还会新增自己的缓存。Rust 官方工具文档说明：开启 incremental 的 rustc 调用不可缓存，调用系统链接器的 `bin`、`dylib`、`cdylib` 和 `proc-macro` crate 也不可缓存。[sccache Rust caveats](https://github.com/mozilla/sccache/blob/main/docs/Rust.md) 只有在 `sccache --show-stats` 显示可缓存请求命中率达到 60%，且“缓存读取+解压”确实快于本地重编译时才保留。
- **`cargo-cache`：只作 Cargo Home 观测/人工清理。** 它管理 registry、git checkout、crate archive 等 `CARGO_HOME` 内容，不处理当前 71 GB target；不能作为本问题主解。[cargo-cache 官方仓库](https://github.com/matthiaskrgr/cargo-cache)
- **`cargo-sweep`：不进入生产默认或无人值守定时任务。** 其 `--time` 依据 Cargo fingerprint 的访问时间，`--installed`/`--toolchains` 读取 fingerprint 中的 rustc 哈希，stamp/file 工作流用时间标记选择旧产物；这些能力适合 dry-run 后的人工选择性清理，见 [cargo-sweep 0.8.0 源码](https://docs.rs/crate/cargo-sweep/0.8.0/source/src/fingerprint.rs) 和 [官方 README](https://github.com/holmgr/cargo-sweep#timestamp-workflow)。但项目官方已经明确标记 **currently unmaintained**、存在已知问题且没有专职维护者，因此不能承担生产默认清理责任。[cargo-sweep 官方仓库维护状态](https://github.com/holmgr/cargo-sweep#readme) 当前 target 中超过 7 天的文件仅约 0.09 GiB，按时龄清理对本轮近期活跃产物也几乎无效。

### 0.6 验收指标、升级条件与回滚条件

P0/P1 实施后，用同一工具链、同一 feature/target/profile 和同一组代表性命令记录 clean build、一次 workspace crate 单文件修改后的 rebuild、target 组成、backtrace 与 LLDB；不要拿不同命令集的目录大小直接比较。

| 指标 | 通过线 | 未通过时动作 |
|---|---|---|
| 首轮代表性 dev 工作流后的 `target` | ≤ 25 GiB，或相对当前 71 GB 基线下降 ≥ 60% | 先核对是否混入 release、全 feature、交叉 target 或旧 profile；再评估 incremental off A/B |
| 连续 7 个开发日后的稳态 `target` | ≤ 35 GiB | 建立按 profile/工作流的占用归因；不得先靠 cargo-sweep 掩盖结构性增长 |
| `*.rcgu.o` 逻辑总量 | ≤ 10 GiB，较当前 40.74 GiB 下降 ≥ 75% | 核对依赖 `debug=false` 是否生效；再 A/B `packed`，不直接写死 |
| `target/debug/incremental` | ≤ 12 GiB | 若超过 12 GiB，进入 incremental on/off A/B；off 后单文件 rebuild p50 变慢 ≤ 20% 才接受关闭 |
| 日常 backtrace | 保留 workspace 源文件与行号 | 回滚 `line-tables-only` 变更并定位平台/构建差异 |
| `--profile debugging` | LLDB 断点、栈帧、workspace 变量可检查 | 回滚该 profile，恢复原 dev debug 设置 |
| 日常单文件 rebuild | 相同测量条件下 p50 不比变更前慢 > 10% | 保留体积数据，回滚 P1 后重新选择折中；不要用单次样本决策 |
| 可选 sccache | 可缓存请求命中率 ≥ 60%，缓存 ≤ 5 GB | 未达标即停用并移除该额外缓存，不影响 P1 主方案 |

这些是本项目的首轮工程验收阈值，不是 Cargo 官方保证值。若 P1 已满足 25/35 GiB 目标，就不再增加 `split-debuginfo`、incremental 或第三方工具复杂度；若不满足，升级顺序固定为“归因 → incremental A/B → packed A/B → sccache 加速冷重建”。

### 0.7 明确不选什么

- 不选择“安装一个第三方清理工具”作为根治；清理不能阻止 40.74 GiB 全量调试信息再次生成。
- 不把 `split-debuginfo = "off"` 或 `"packed"` 未经 A/B 写成团队默认；它们改变布局与调试/链接权衡，不决定调试信息总量。
- 不在 P1 第一轮同时关闭本地 incremental；一次只改变主变量，才能知道体积和速度收益来自哪里。
- 不拆分 debug/release 到两个 target 根目录来追求瘦身；Cargo 本来已按 profile 分目录，额外 target 只改善生命周期隔离，可能复制依赖产物。[Cargo Build Cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)
- 不把 Cargo 1.91 `build.build-dir` 当压缩、去重或自动 GC。1.91 只稳定了“中间产物放到哪里”；新布局仍是 nightly，稳定 Cargo 也没有 target 自动 GC 或 `cargo prune`。[Cargo 1.91 changelog](https://doc.rust-lang.org/cargo/CHANGELOG.html#cargo-191-2025-10-30)
- 不在 Cargo 1.91 使用 `cargo clean --workspace`；该 flag 到 Cargo 1.93 才加入。[Cargo 1.93 changelog](https://doc.rust-lang.org/cargo/CHANGELOG.html#cargo-193-2026-01-22)
- 不清整个 `CARGO_HOME` 来解决 target 问题，也不删除 credentials。

## 1. 执行摘要

Norma OS 当前的主要问题不是 `~/.cargo` 下载缓存，而是 macOS dev profile 的调试对象、增量编译数据，以及同一 workspace 在不同构建组合下留下的多份产物。

本机只读测量结果：

| 项目 | 当前占用 | 判断 |
|---|---:|---|
| `target/` | 约 71 GB | 主要矛盾 |
| `target/debug/` | 约 63 GB | 占 `target` 的绝大部分 |
| `target/debug/deps/` | 约 46 GB | 含大量拆分调试对象与依赖产物 |
| `target/debug/incremental/` | 约 16 GB | 用磁盘换本地增量编译速度 |
| `target/release/` | 约 7.2 GB | 可按 release 使用频率单独管理 |
| `~/.cargo/` | 约 2.5 GB | 次要矛盾；其中 registry 约 2.3 GB |

`target/debug/deps/` 中检测到约 56,571 个 `*.rcgu.o`，逻辑大小约 40.74 GiB。Cargo 官方文档说明：macOS 在生成调试信息的 profile 中，`split-debuginfo` 默认是 `unpacked`。结合当前 `profile.dev` 未显式降低 debug 信息，这些文件高度可能是 `debug/deps` 膨胀的首要来源。该结论属于基于文件类型与官方默认值的高置信度推断，最终应通过独立 target 的 A/B 构建确认。[Cargo Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#split-debuginfo)

建议顺序：

1. 磁盘告急时，经用户授权先完整 `cargo clean`，释放旧 target 并避免 profile 切换时的峰值风险。
2. 落地 Cargo 官方推荐的 dev 调试信息缩减方案，并保留按需使用的完整 debugging profile。
3. 本地第一轮保留 incremental；CI 关闭。只有代表性工作流仍超过预算时，才做 incremental on/off A/B。
4. 多 worktree、跨目录或 CI 冷构建确有收益时再引入限额 5 GB 的 `sccache`；它主要减少重编译时间，不会自动缩小 `target`。
5. `~/.cargo` 继续依赖 Cargo 1.88+ 自动 GC；`cargo-cache` 主要用于观测和精细清理。
6. workspace 继续共享 `Cargo.lock` 与 `target`，再用 `cargo tree -d` 定位确实可以合并的重复版本；不要把 `[workspace.dependencies]` 当成自动去重开关。
7. CI 保留现有 `Swatinem/rust-cache@v2` 路线，限制缓存写入来源并监控 GitHub 10 GB 默认配额，避免缓存抖动。

## 2. 关键事实纠正：不存在 Rust 1.79+ 的官方 `cargo prune` 清理命令

“Rust 1.79+ 提供 `cargo prune` 清缓存”这一前提不成立。

| 版本/能力 | 实际含义 |
|---|---|
| Cargo 1.54 | 加入 `cargo tree --prune <SPEC>`，只裁剪依赖树的显示内容，不删除任何文件 |
| Cargo 1.78 | 开始记录 Cargo 全局缓存的最近使用信息，为后续 GC 提供数据 |
| Cargo 1.79 | 没有新增 `cargo prune` 清理命令 |
| Cargo 1.88 | 全局缓存自动 GC 稳定；默认定期清理长期未用的下载/可重建缓存 |
| Cargo 1.91 | `build.build-dir` 稳定，可分离中间产物和最终产物的位置 |
| 本项目固定的 stable 1.91.1 | `cargo clean gc` 仍是 nightly/`-Zgc` 不稳定命令 |

本机 Cargo 1.91.1 实测 `cargo prune --help` 返回 `no such command: prune`；`cargo tree --help` 中的 `--prune` 明确写着“从依赖树显示中裁剪”。Cargo 官方 changelog 也没有 1.79 清理命令记录。[Cargo Changelog](https://doc.rust-lang.org/cargo/CHANGELOG.html#cargo-179-2024-06-13)

因此报告后续把以下三类能力分开：

- `cargo tree --prune`：依赖图展示过滤，不释放磁盘。
- Cargo 1.88+ 自动 GC：只管理 `CARGO_HOME` 全局下载/源码缓存，不清 workspace 的 `target`。
- 第三方 `cargo-cache` / `cargo-sweep`：分别管理 `CARGO_HOME` 和项目构建产物。

## 3. 方案决策矩阵

| 方案 | 本地磁盘 | 编译速度 | 最适合 | 主要代价/风险 | Norma OS 判断 |
|---|---|---|---|---|---|
| 降低 dev debug info | 大幅改善潜力 | 通常也更快 | macOS、大型 GUI workspace | 日常 LLDB 信息变少 | **P1，永久主方案** |
| 完整 `cargo clean` | 立即释放最多 | 下一次全量冷编译 | 紧急腾盘、配置切换后重建 | 只治标 | P0 止血手段，非长期方案 |
| 选择性 clean / `cargo-sweep` | 中等，取决于陈旧产物 | 保留部分热缓存 | 长期活跃仓库、多工具链 | 项目当前无人维护；误删后需重编译 | 不进生产默认；当前按时龄收益极低 |
| 关闭 incremental | 可直接消除该类新增数据 | 本地小改可能变慢 | CI、磁盘紧张、配合 sccache | 失去增量编译 | CI 应关闭；本地先 A/B |
| `sccache` | 不直接缩小 target；新增独立缓存 | 跨 clean/worktree/runner 可明显受益 | 多 worktree、频繁冷构建 | incremental、链接型 crate 不可缓存 | P2 可选，硬上限 5 GB |
| Cargo 1.88+ 自动 GC | 改善 `~/.cargo` | 可能产生偶发重下载 | 所有稳定工具链用户 | 不处理 `target` | 已具备，维持默认即可 |
| `cargo-cache` | 精细管理 `~/.cargo` | 清理后可能重下载/解压 | 诊断全局缓存组成 | 第三方工具；不处理编译产物 | P2 辅助工具 |
| workspace 共享 target | 避免同 workspace 成员重复目录 | 提升复用 | 单 workspace | 不消除 profile/feature/toolchain 组合 | 当前已具备 |
| 多 worktree 共享 target | 可能减少重复依赖产物 | 串行时可能受益 | 同时仅一个 Cargo 构建 | 目录锁竞争、路径差异导致 miss | 并行 Agent 不建议作为默认 |
| 为 debug/release 再拆 target 根目录 | 通常不减反增 | 复用下降 | 需要不同生命周期/磁盘卷 | 重复依赖和构建脚本产物 | 仅用于隔离，不作为瘦身方案 |
| Cargo 1.91 `build.build-dir` | 便于集中治理 | 基本中性 | 想把中间产物集中到可清理位置 | 只是移动；不自动共享/GC | P2 管理性优化 |
| 对齐依赖版本和 feature | 可减少部分重复编译 | 冷构建更快 | `cargo tree -d` 确认的可合并项 | API/feature 不兼容，升级风险 | P2，逐项处理 |
| CI `Swatinem/rust-cache` | 使用远端缓存额度 | 依赖多时收益高 | GitHub Actions | 上传下载开销、10 GB 配额 | 当前方案正确，需收紧写入 |

## 4. Rust 编译缓存：`sccache` 与 `cargo-cache`

### 4.1 两者不是同一类工具

- `sccache` 是 rustc 编译结果缓存。命中时可跳过一部分编译，用于加速；支持本地磁盘和多种远端存储。[sccache README](https://github.com/mozilla/sccache)
- `cargo-cache` 是 `CARGO_HOME` 管理工具，用来查看和删除 registry、git checkout、crate archive 等内容；它不是编译缓存。[cargo-cache README](https://github.com/matthiaskrgr/cargo-cache)

### 4.2 `sccache` 的配置与使用

macOS 可使用 Homebrew 或官方 release 的预编译包。官方明确提示 `cargo install sccache --locked` 会从源码编译且资源消耗较大，CI 应优先预编译包。[sccache installation](https://github.com/mozilla/sccache#installation)

最小配置有两种：

```bash
# 仅对单次 shell / CI 生效
export RUSTC_WRAPPER=sccache
cargo build
```

```toml
# Cargo 全局或项目 config 示例；本报告未写入此配置
[build]
rustc-wrapper = "sccache"
```

常用容量和路径设置：

```bash
export SCCACHE_DIR=/absolute/path/to/sccache
export SCCACHE_CACHE_SIZE=5G
export SCCACHE_BASEDIRS=/absolute/path/to/projects
```

`SCCACHE_CACHE_SIZE` 的默认本地上限是 10 GB。示例中的 5 GB 只是说明必须显式设预算，不是本项目尚未验证的最终值。[sccache configuration](https://github.com/mozilla/sccache/blob/main/docs/Configuration.md)

运行后用以下只读统计判断实际收益：

```bash
sccache --show-stats
```

重点观察 cache hits、cache misses、non-cacheable compilations，以及缓存读写耗时。不要只看第二次构建总时间，因为 Cargo 自身 target cache 也会让第二次构建变快。

### 4.3 `sccache` 的限制

官方列出的 Rust 限制包括：[sccache Rust caveats](https://github.com/mozilla/sccache/blob/main/docs/Rust.md)

- incremental compilation 必须关闭，该次 rustc 调用才能进入 sccache。
- 调用系统链接器的 `bin`、`dylib`、`cdylib`、`proc-macro` crate 不能缓存。
- 默认要求绝对路径一致；多 worktree 或不同 CI checkout 路径需要 `SCCACHE_BASEDIRS` 归一化。
- 远端 cache 的网络延迟、压缩和上传成本可能高于重新编译，必须以统计数据决定是否保留。

对 Norma OS 的含义：

- 保持本地 incremental 时，sccache 主要覆盖非增量的第三方依赖；不会解决当前 16 GB `incremental`。
- 若本地关闭 incremental，workspace 中可缓存的 library 有机会进入 sccache，但最终二进制和链接仍要发生。
- sccache 不会主动删除 `target`；它的价值是让团队可以更积极地清理 target，而不必每次承担完整冷编译成本。
- 在当前 71 GB target 未先治理前直接启用默认 10 GB sccache，只会先把总占用继续抬高。

### 4.4 `cargo-cache` 的配置与使用

典型命令：

```bash
cargo install cargo-cache
cargo cache
cargo cache --top-cache-items 20
cargo cache --dry-run --autoclean
```

前三个命令用于安装或只读观测；`--dry-run --autoclean` 预览清理。确认后执行 `cargo cache --autoclean` 会删除已解压 crate source 和 git checkout，同时保留可用于重建它们的 crate archive 和 bare git repo。[cargo-cache usage](https://github.com/matthiaskrgr/cargo-cache#usage)

风险分级：

- 低风险：`cargo cache`、`--top-cache-items`、带 `--dry-run` 的命令。
- 中风险：`--autoclean`，之后会重新解压或 checkout，首次构建变慢。
- 高影响：删除整个 registry/git cache，之后必须重新下载；离线环境可能无法构建。

当前 `~/.cargo` 总量只有约 2.5 GB，即使全部清掉，收益也远小于 target。`registry/src` 约 1.9 GB，适合在极端磁盘压力下作为次级回收点，但活跃依赖会很快重新解压。

## 5. `target` 目录优化

### 5.1 debug/release 已经默认分离

Cargo 默认布局已经是：

- `target/debug/`：dev 与 test profile。
- `target/release/`：release 与 bench profile。
- `target/<custom-profile>/`：自定义 profile。
- `target/<target-triple>/<profile>/`：显式 `--target` 的交叉编译产物。

因此，再把 debug 和 release 放进两个不同的 `CARGO_TARGET_DIR`，主要价值是隔离生命周期或放到不同磁盘卷，不会自动减少总量，反而可能让两边重复保存 build script、proc-macro 和依赖产物。[Cargo Build Cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)

适用的分离场景：

- CI 临时 target 与本地开发 target 生命周期完全不同。
- release 产物需要落在高速/大容量独立磁盘。
- 多个并行任务必须避免争用同一 build directory lock。

不适用的场景：单一 workspace 仅为了“看起来更整齐”而拆目录。

### 5.2 当前最高收益项：减少 dev 调试信息

Cargo 官方的 build performance 指南推荐：[Optimizing Build Performance](https://doc.rust-lang.org/cargo/guide/build-performance.html#reduce-amount-of-generated-debug-information)

```toml
[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false

[profile.debugging]
inherits = "dev"
debug = true
```

作用：

- workspace crate 仅保留文件名/行号级 backtrace 信息。
- 第三方依赖不生成 debug info。
- 真正使用 LLDB 时通过 `cargo build --profile debugging` 做一次完整调试构建。
- 官方预期是 codegen 更快、link 更快、target 更小；代价是高质量调试需要单独完整重建。

对 Norma OS 的预期效果很高，因为当前约 40.74 GiB 逻辑大小来自 macOS `unpacked` 形态的 `rcgu.o`。但不能直接把 40.74 GiB 等同于最终可回收量；需要在独立 target 中对比：

1. clean build 后的总占用与 `deps/*.rcgu.o` 占用。
2. 单文件修改后的增量构建时间。
3. panic backtrace 是否仍含文件和行号。
4. `--profile debugging` 下 LLDB 的变量、断点和栈帧体验。

`split-debuginfo = "off"` 或 `"packed"` 也可以作为 macOS A/B 候选，但不应先于官方 debug 信息缩减配方直接全局启用；不同取值会在链接耗时、LLDB 体验和最终文件分布之间重新权衡。

### 5.3 incremental：明确用磁盘换反馈速度

Cargo 官方说明 incremental 只用于 workspace member 和 path dependency，额外数据存储在 target 中。[Cargo Profiles: incremental](https://doc.rust-lang.org/cargo/reference/profiles.html#incremental)

当前 `target/debug/incremental` 约 16 GB。选择：

- 本地默认保留：适合频繁编辑大型 workspace crate、磁盘充裕、重视最短 edit-compile-run。
- 本地关闭：适合磁盘紧张、经常切换 worktree、准备配合 sccache；必须 A/B 验证编辑反馈退化。
- CI 关闭：ephemeral runner 上几乎总是合理；现有 `Swatinem/rust-cache` 已自动设置 `CARGO_INCREMENTAL=0`。

单次试验可通过环境变量完成，不需要先改 manifest：

```bash
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/absolute/path/to/isolated-target cargo check --workspace
```

必须使用独立 target 做 A/B；否则开关 incremental 会在原 target 中产生另一组 fingerprint，短期内反而增加占用。

### 5.4 清理：先 dry-run，再按用途删除

Cargo 官方稳定命令：[cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)

```bash
# 只读预览
cargo clean --dry-run --verbose
cargo clean --release --dry-run

# 以下会真实删除生成物
cargo clean
cargo clean --release
cargo clean -p app
```

- `cargo clean`：释放最多，但下一次是完整冷构建。
- `cargo clean --release`：当前约可释放 7.2 GB，适合 release 构建不频繁的开发机。
- `cargo clean -p`：只清指定 package 的已知产物，依赖仍保留；适合局部问题，但不一定能清掉所有历史组合。

第三方 `cargo-sweep` 支持按 fingerprint 访问时龄、rustc 工具链哈希和 stamp 时间标记清理，并提供 `--dry-run`。[cargo-sweep 0.8.0 源码](https://docs.rs/crate/cargo-sweep/0.8.0/source/src/fingerprint.rs)

```bash
cargo sweep --dry-run --time 30
cargo sweep --dry-run --installed
```

`--stamp` 写入时间标记，`--file` 后续删除早于该标记的产物；`--installed`/`--toolchains` 则读取 Cargo fingerprint 中的 rustc 哈希并与 rustup/rustc 结果对照。这些是启发式的选择性回收机制，不是 Cargo 官方的依赖可达性 GC。[cargo-sweep 官方 README](https://github.com/holmgr/cargo-sweep#timestamp-workflow) 更关键的是，官方仓库已明确标记项目 currently unmaintained、存在已知问题且缺少专职维护者，因此不应进入生产默认或无人值守定时任务。[cargo-sweep 维护状态](https://github.com/holmgr/cargo-sweep#readme)

当前只读扫描显示：mtime 超过 7 天的 target 文件逻辑大小仅约 0.09 GiB。这意味着目前的 71 GB 主要是近期活跃构建产生的，简单的“删除 30 天前文件”几乎不会解决本轮问题；必须先处理 debug info 的结构性体积，incremental 则按验收阈值决定是否关闭。

### 5.5 Cargo 1.91 `build.build-dir`

Cargo 1.91 稳定了 `build.build-dir`，允许把中间产物和最终产物放到不同位置。[Cargo 1.91 Changelog](https://doc.rust-lang.org/cargo/CHANGELOG.html#cargo-191-2025-10-30)

示例：

```toml
[build]
build-dir = "{cargo-cache-home}/build/{workspace-path-hash}"
```

用途：

- 最终 binary/doc/timing 留在 `target-dir`。
- `deps`、`incremental`、build script output 等中间产物集中到统一根目录，便于设置磁盘卷、配额和清理任务。
- `{workspace-path-hash}` 隔离不同 workspace，降低碰撞风险。

边界：

- 它移动数据，不压缩、不去重，也不自动 GC。
- 当前稳定布局不等于“多个 workspace 可安全共享同一份中间产物”。
- 2026 年正在测试的 build-dir layout v2 仍是 nightly 的 `-Zbuild-dir-new-layout`，不应写入生产默认配置。[Build Dir Layout v2](https://blog.rust-lang.org/2026/03/13/call-for-testing-build-dir-layout-v2/)

## 6. Cargo 全局缓存管理

### 6.1 Cargo Home 组成

Cargo 官方列出的主要目录：[Cargo Home](https://doc.rust-lang.org/cargo/guide/cargo-home.html)

- `registry/cache`：压缩 `.crate` archive。
- `registry/src`：从 archive 解压后的源码。
- `registry/index`：registry 元数据。
- `git/db`：bare git repo。
- `git/checkouts`：实际 checkout。
- `bin`：`cargo install` 的工具。
- `credentials.toml`：registry 凭据，不得进入 CI cache 或随意删除/暴露。

CI 不应直接缓存整个 Cargo Home，因为 `registry/cache` 和 `registry/src` 会重复保存压缩包与解压源码。Cargo 官方建议主要缓存：

- `.crates.toml`、`.crates2.json`、`bin/`（仅需要已安装工具时）。
- `registry/index/`、`registry/cache/`、`git/db/`。
- 不缓存 `registry/src/` 和 `git/checkouts/`，让 Cargo 从压缩包/bare repo 重建。

### 6.2 Cargo 1.88+ 自动 GC

Cargo 1.88 起，稳定版默认周期性清理全局缓存：[Cargo cache configuration](https://doc.rust-lang.org/cargo/reference/config.html#cache)

- 默认最多每天检查一次。
- 必须重新下载才能恢复的文件，默认未使用 3 个月后清理。
- 可由本地已有数据重建的文件，默认未使用 1 个月后清理。
- `--offline` 或 `--frozen` 时不自动清理，避免长期离线后失去缓存。
- 当前只追踪 Cargo Home，全局 GC 尚不管理 workspace target。

稳定版可配置检查频率：

```toml
[cache]
auto-clean-frequency = "1 day"
```

该值控制“多久检查一次”，不控制具体保留时长。Norma OS 已使用 Cargo 1.91.1，默认行为已经足够；把频率从一天改得更短不会处理 71 GB target。

`cargo clean gc -Zgc` 可以手动按 age/size 清全局缓存，但在本项目固定的 stable 1.91.1 上仍不可用，不应纳入稳定开发流程。[Cargo unstable GC](https://doc.rust-lang.org/cargo/reference/unstable.html#gc)

### 6.3 本机配置优先级风险

本机同时存在 `~/.cargo/config` 与 `~/.cargo/config.toml`，Cargo 1.91.1 每次均警告并选择无扩展名的 `config`。Cargo 官方也说明两者同时存在时旧文件优先。[Cargo Configuration](https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure)

这意味着未来若只把 `rustc-wrapper`、`cache` 或 `build-dir` 写进 `config.toml`，可能完全不生效。实施任何方案前，应先人工合并并确认唯一的全局配置文件；本报告没有改动这两个文件。

## 7. workspace 级共享依赖与重复编译

### 7.1 workspace 已经提供什么

Cargo workspace 默认：[Cargo Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)

- 所有成员共享根 `Cargo.lock`。
- 所有成员共享根 `target`。
- 根 manifest 统一定义 profile。
- `[workspace.dependencies]` 可以让成员继承同一个依赖声明。

Norma OS 已经是一个 18-member virtual workspace，并共享根 target，所以“把 crate 放入 workspace”这一基础优化已完成。

### 7.2 `[workspace.dependencies]` 不会自动去重所有编译

即使不使用 `[workspace.dependencies]`，Cargo resolver 也会尽量统一兼容的版本。`[workspace.dependencies]` 的主要价值是避免各成员手写版本/source/default-features 时发生漂移；只有当它实际消除了不同版本、不同 source 或不必要的 feature 组合时，才会减少编译产物。[Workspace dependency inheritance](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#inheriting-a-dependency-from-a-workspace)

本机 `cargo tree --workspace --duplicates --depth 0` 显示约 50 个依赖名存在多个 package ID/版本，包括 `dirs`、`bzip2`、`rand`、`syn`、`thiserror`、`ureq` 等。这里大部分可能来自传递依赖，不能仅靠在根 manifest 写一个版本强行合并。

建议流程：

```bash
cargo tree --workspace --duplicates
cargo tree --workspace --invert <crate>@<version>
```

然后逐项判断：

1. workspace 成员是否直接声明了不同版本或 source；可安全时统一到 `[workspace.dependencies]`。
2. 是否由某个上游旧版本带入；优先升级上游，而不是用 `[patch]` 强压不兼容 API。
3. 是否是不同 major version；通常不能直接合并。
4. 是否是 build/proc-macro/dev/target-specific feature 的有意隔离；不要为了少一次编译破坏语义。

### 7.3 resolver 与 feature 组合

Norma OS 使用 `resolver = "2"`。Resolver 2 会有意避免在以下场景统一 feature：[Cargo Dependency Resolution](https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2)

- build dependency / proc-macro 与 normal dependency。
- 当前未构建平台的 target-specific dependency。
- 非测试构建中的 dev dependency。

这可能让同一版本以不同 feature 集编译多次，但换回 resolver 1 会重新引入 feature 污染和跨平台/no_std 问题，不是合理的磁盘优化。

更安全的做法：

- 对 CI 和常用本地任务标准化 feature 集与 target 集，避免每个脚本都生成一种新组合。
- 仅在验收需要时使用 `--all-features` / `--all-targets`。
- 尽量在一次 Cargo invocation 中构建需要的多个 workspace package，使兼容 feature 合并后只构建一次。
- 工具链、RUSTFLAGS、profile 或 target 改变都会产生不同 fingerprint；避免无意义地频繁切换。

### 7.4 多 worktree 的 target 策略

Cargo 官方支持 `CARGO_TARGET_DIR`、`build.target-dir` 和 `--target-dir` 改变位置，但没有承诺多个并行 workspace 在同一 target 下获得无锁、完全复用的共享缓存。[Cargo Build Cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)

两条可行路线：

- 磁盘优先、构建基本串行：同一仓库的 worktree 指向一个共享 target，减少相同 fingerprint 依赖的重复副本；接受 build directory lock 竞争与路径变化造成的 miss。
- 并行优先：每个 worktree 独立 target，使用 `sccache` 复用可缓存编译，再在 worktree 结束时清理其 target。磁盘峰值较高，但 Agent 不会因共享锁互相等待。

Norma OS 存在多 Agent 并行工作场景，默认更适合第二条；共享 target 只适合确认 Cargo 构建不会并发的机器。

## 8. CI/CD 缓存优化

### 8.1 当前 GitHub Actions 基线

当前 `.github/workflows/ci.yml` 已使用：

```yaml
- uses: Swatinem/rust-cache@v2
```

这是合理起点。该 action 会：[Swatinem/rust-cache](https://github.com/Swatinem/rust-cache)

- 用 rustc release/host/hash、Cargo manifests/lockfile、toolchain 文件、Cargo config 和相关环境变量构造 key。
- 缓存 Cargo registry/git 与 target 中的依赖产物。
- 保存前清理 incremental、陈旧产物、workspace 自身产物和已不用依赖。
- 自动设置 `CARGO_INCREMENTAL=0`。
- 不缓存 `registry/src`，由 `registry/cache` 重新解压。

现有单个 macOS job 顺序执行 `cargo check` 和 `cargo clippy`，同一 runner 内可以直接复用 target，比拆成多个并行 job 后各自恢复/上传大型 cache 更节省。

### 8.2 建议保留并优化的参数

1. **只让可信主分支保存**：PR 只恢复 main cache，不写入一次性 merge-ref cache；可用 `save-if` 限制为 `refs/heads/main`。代价是同一 PR 后续提交不能保存自己的增量 cache。
2. **保持稳定工具链精确 pin**：现有 1.91.1 有利于 key 稳定；nightly 每日变化会频繁失效。
3. **不要缓存 workspace crate 与 incremental**：action 当前默认已经这样做。
4. **矩阵 key 必须包含 OS、架构、toolchain、target、profile、RUSTFLAGS 和会改变产物的 feature 维度**；不能让不兼容产物共用 key。
5. **最终 `.app` / `.dmg` 用 artifact，不用 dependency cache**：cache 是可丢弃的性能优化，artifact 才是 job/stage 间交付物。
6. **比较传输成本**：若恢复+解压+保存时间接近或高于重编译时间，应减小 cache 范围，而不是继续加目录。

GitHub Actions 默认每仓库 cache 上限为 10 GB，未访问超过 7 天的条目会被清理；超限后按最近访问时间逐出，过多 key 会产生 cache thrashing。[GitHub dependency caching reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy)

### 8.3 是否在 CI 叠加 sccache

不建议一开始同时启用 `Swatinem/rust-cache` 的 target cache和远端 sccache：

- 两层都可能下载/上传相似编译结果。
- 故障定位和 cache key 设计复杂度上升。
- GitHub cache 后端更适合较大 archive，sccache 是细粒度对象缓存，网络延迟可能抵消收益。

先测现有 rust-cache 的 restore/save 时间和依赖命中效果。只有在大量矩阵、worktree 或跨分支冷构建仍占主要 CI 时间时，再做一条独立实验分支，将“target dependency cache”和“sccache remote”进行 A/B，而不是默认叠加。

### 8.4 GitLab CI 的对应策略

若迁移到 GitLab：[GitLab CI caching](https://docs.gitlab.com/ci/caching/)

- `cache:key:files` 使用 `Cargo.lock` 与工具链/config 输入生成 key。
- feature branch 使用 fallback key 恢复默认分支 cache。
- 默认分支 `pull-push`，普通分支/PR 尽量 `pull`，减少并发覆盖和上传。
- 多 runner 需要 S3 distributed cache 或共享网络 cache，并在对象存储设置 lifecycle。
- cache 用于依赖，artifacts 用于 stage 间传递最终构建结果。

## 9. 建议的分阶段落地顺序

本报告不实施变更；以下是后续执行建议。

### Phase 0：保留基线和安全止血

- 保存本报告中的 71/63/46/16/7.2/2.5 GB 基线。
- 所有删除先运行 `cargo clean --dry-run --verbose` 或对应工具的 `--dry-run`。
- 当前只剩约 799 MiB；经用户明确授权后完整 `cargo clean`，释放旧 target，并明确接受下一次冷构建。
- 不直接删除 `~/.cargo/bin`、配置或 credentials。

### Phase 1：独立 target 验证 dev profile

空间释放后对比三组；正式默认先只采用第 2 组：

1. 当前 profile。
2. 官方 `line-tables-only` + dependency `debug=false`。
3. 仅当第 2 组仍未达到 25/35 GiB 验收线时，再分别尝试本地 incremental on/off；macOS `split-debuginfo` 仅作为附加 A/B。

记录：clean build、单文件 rebuild、link、target 总量、`rcgu.o` 总量、panic backtrace、LLDB 可用性。选出默认轻量 dev profile，并保留 opt-in 完整 debugging profile。

### Phase 2：缓存与清理政策

- 本地只在多 worktree/频繁 clean 场景试点 sccache，设置 5 GB 硬上限并看 `--show-stats`。
- `cargo-sweep` 当前无人维护且本项目按时龄收益很低，不设为生产默认或无人值守任务；最多在 dry-run 后人工使用。
- 依赖 Cargo 1.88+ 自动 GC 管理 Cargo Home；`cargo-cache` 作为人工诊断工具。
- 若需要统一治理路径，再评估 Cargo 1.91 `build.build-dir`，但不要把它计入“节省容量”。

### Phase 3：workspace 与 CI 精修

- 逐项审查 `cargo tree -d` 的重复来源；只合并可兼容且真实减少 package ID/feature 组合的条目。
- 保持 resolver 2。
- GitHub Actions 限制 cache 写入来源，监控总量、命中率与 restore/save 时间。
- 仅当现有 rust-cache 仍不足，再把 sccache remote 作为替代方案 A/B。

## 10. 最终结论

对当前 Norma OS，优化优先级不能按工具知名度排序，而应按实际占用排序：

1. **macOS dev 调试对象约 40.74 GiB**：先减少日常 debug info，并保留完整调试 profile。
2. **incremental 约 16 GB**：CI 关闭；本地用 A/B 决定是否值得。
3. **历史 target / release 约 7.2 GB**：通过 dry-run、选择性 clean 和后续策略回收。
4. **sccache**：是“清理后仍能较快重建”的加速器，不是 target 瘦身器；必须限额。
5. **Cargo Home 约 2.5 GB**：Cargo 1.88+ 自动 GC 已覆盖主要场景，`cargo-cache` 只做补充。
6. **workspace 和 CI**：当前方向基本正确；收益来自减少不必要的版本/feature/profile/key 组合，而不是简单增加更多缓存目录。

最重要的错误前提也已排除：Rust 1.79 没有官方 `cargo prune` 清缓存功能。当前稳定路线是 `cargo clean` 管项目 target、Cargo 1.88+ 自动 GC 管 Cargo Home；需要更细控制时再引入 `cargo-cache`、`cargo-sweep` 或 `sccache`。
