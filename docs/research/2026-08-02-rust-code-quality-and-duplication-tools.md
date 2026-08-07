# Rust 代码质量与重复代码扫描工具调研

> 调研日期：2026-08-02
> 范围：Rust 源码重复块/重复率，以及与其相邻的格式、Lint、依赖、安全、`unsafe`、未使用依赖和 SemVer 检查
> 资料口径：只采用项目官方文档、官方仓库与官方产品文档；本文没有修改源码或 CI。

## 1. 结论先行

Rust 官方工具链目前**没有“项目重复率扫描器”**：

- `cargo clippy` 检查正确性、可疑写法、风格、复杂度和性能等 lint，但不统计跨文件复制粘贴块，也不提供项目重复率。
- `rustfmt` 只负责格式化；统一格式可以让文本型重复检测更稳定，但它本身不检测重复。
- `cargo-deny` 文档中的 `duplicates` 指“依赖图中同一 crate 的多个版本”或 workspace 依赖声明重复，不是源码重复。

对当前 Rust workspace，推荐路线是：

1. **P0：用 `jscpd` v5 建立重复率基线。**它原生识别 `.rs`，能同时给出重复块、重复行和重复百分比，支持 JSON/HTML/Markdown/SARIF，能按百分比让 CI 失败；v5 还是自包含的 Rust 实现，可在 macOS、Linux 和 Windows 运行。第一次只报告、不阻断。
2. **P1：若希望一个工具统一重复率、复杂度、Clippy、rustfmt 和安全扫描，评估 Qlty CLI。**它明确支持 Rust 的 identical/similar duplication 和 duplication density，但使用 BSL 1.1/Fair Source，而非当前即可获得的 OSI 开源许可证，需要先完成许可证评审。
3. **P1 备选：PMD CPD。**它从 PMD 7.9.0 开始正式支持 Rust，成熟、BSD-style 许可，适合已有 JVM/PMD 基础设施的团队；它擅长列出重复块，但没有像 jscpd/Sonar 那样直接提供可用的仓库“重复率质量门”。
4. **已有 SonarQube/SonarCloud 时优先复用 Sonar。**2025.3 起官方 Rust 分析包含代码重复，平台直接提供 `duplicated_lines_density` 和新代码质量门；若团队尚未使用 Sonar，不建议只为重复率引入服务端平台。
5. `cargo-duplicated` 是 Rust 专用候选，但项目截至目前只有少量提交、0.2.0 发布和极少生态采用，先放实验层，不作为默认质量门。

## 2. 与当前仓库的适配判断

本仓库是 18 个成员的 Rust 2021 workspace；现有 [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) 已执行：

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

因此当前 CI 已覆盖格式和主要 Rust lint，但没有以下能力：

- 跨 crate、跨文件的重复代码块；
- 重复行数和重复率趋势；
- “新代码重复率”质量门；
- 依赖漏洞、许可证、重复版本、未使用依赖或公开 API SemVer 破坏（这些属于其他工具的职责）。

重复扫描应覆盖 `crates/**/*.rs`，并排除至少：

- `target/**`；
- `vendor/**`（第三方代码不应计入本项目重复率）；
- 生成代码、fixtures、快照和明确需要重复的测试样板。

测试代码不应一刀切排除：本仓库重视测试，但测试中的场景数据和逐项断言经常是有意重复。建议先分别展示 production 与 tests 指标，再决定是否设门禁。

## 3. 专门的重复代码与重复率工具

| 工具 | Rust 是否真实支持 | 输出 | 接入方式 | 维护与许可证 | 主要限制 | 结论 |
|---|---|---|---|---|---|---|
| **jscpd v5** | 是；官方格式表把 `.rs` 映射为 `rust` | 重复块、行数、统计表、百分比；JSON/XML/CSV/HTML/Markdown/SARIF | `cargo install jscpd`、Homebrew、npm 预编译二进制、CLI、GitHub Action/SARIF | 活跃，MIT；v5 为 Rust 重写 | token/归一化型克隆检测，不理解业务语义；阈值过低会把样板代码当问题 | **默认推荐** |
| **Qlty CLI** | 是；官方 Rust 页面明确支持 identical/similar duplication | duplicated lines、blocks、duplication density、代码异味及趋势 | 本地 CLI、Git hooks、GitHub Action、VS Code、Qlty Cloud | 活跃；BSL 1.1/Fair Source，延迟转开源 | 许可证不是当前即 OSI 开源；平台能力会增加治理面 | 统一质量平台候选 |
| **PMD CPD 7.9+** | 是；7.9.0 新增 `pmd-rust` | 每个重复块的 tokens、行数、出现位置；text/XML/CSV/VS/Markdown | Java CLI、脚本/CI；可转换 HTML | 非常活跃，BSD-style | 需要 JVM；Rust 仅 CPD，不是 PMD Rust 规则集；官方报表以块为主，仓库百分比需另算 | 成熟备选 |
| **SonarQube/SonarCloud** | 是；2025.3 文档明确 Rust 重复分析 | duplicated lines/blocks/files、`duplicated_lines_density`、新代码重复率 | SonarScanner、PR decoration、质量门、平台 UI | 商业/社区产品，持续维护 | 需要服务或 SaaS；默认非 Java 判定至少 100 个连续 token 且通常至少 10 行，短重复不会报 | 已有 Sonar 时复用 |
| **cargo-duplicated 0.2** | 是；Rust 专用，0.2 加入 AST 语义归一化 | 重复块、人类可读/JSON；退出码可阻断 | `cargo install cargo-duplicated`、CLI | 2025-12 发布，MIT，但只有少量提交/采用 | 不输出成熟的仓库重复率趋势；生态和回归样本不足 | 仅实验对照 |
| **rust-code-analysis** | 支持分析 Rust，但**没有重复指标** | ABC、复杂度、Halstead、MI、LOC 等 | Rust library、CLI、JSON/TOML/YAML、REST | Mozilla 项目，MPL-2.0 | 官方 metrics 列表没有 duplication/clone/重复率 | 不用于重复扫描 |
| **CodeQL for Rust** | 是；2025-10 已 GA，支持 Rust 2021/2024 | 安全和部分质量 query 的 code scanning alerts | GitHub 默认/高级设置、CodeQL CLI、SARIF | GitHub 产品；CLI 有专门使用条款 | 官方 Rust query 集没有通用复制粘贴/重复率度量；私有仓库授权取决于 GitHub 计划 | 安全工具，不替代重复检测 |

### 3.1 为什么首选 jscpd

jscpd 的官方文档给出了当前需求所需的完整闭环：

- `.rs` 自动识别；
- 默认最小 50 tokens、5 行，可调整；
- `console` 直接给 clone list 和 statistics；
- JSON/HTML/Markdown 用于审阅，SARIF 可进入 GitHub Code Scanning；
- `threshold` reporter 能在重复百分比超过阈值时返回失败；
- `.jscpd.json` 支持路径、排除项、阈值和检测模式；
- v5 提供 macOS arm64/x64 自包含二进制，并可用 `cargo install jscpd` 安装。

建议的试运行口径（这是本项目治理建议，不是 jscpd 官方默认值）：

```text
扫描：crates/**/*.rs
排除：target/**, vendor/**, generated/**, fixtures/**, snapshots/**
初始检测：minTokens=70, minLines=8, mode=mild
输出：console + json + html（CI 可改为 SARIF）
第一阶段：只采集全量基线，不失败
第二阶段：只阻止新增重复块或新代码重复率回退
长期参考线：新代码重复率 <= 3%，全量重复率不高于已确认基线
```

不建议第一天就把“全仓重复率 > 3%”设为硬失败：历史重复和合理样板会让团队先忙于豁免。正确顺序是先看重复块、排除 vendor/生成物和合理测试样板，再对新增代码设门。

### 3.2 误报与解释边界

所有通用重复检测器都不能判断两段相似代码是否应该抽象：

- macro 展开前的声明、协议适配、平台分支、迁移代码和测试表格可能有意重复；
- 把共享逻辑强行抽象，有时会增加跨 crate 耦合，违反本仓库依赖方向；
- 文本/token 检测容易漏掉结构相同但变量、控制流变化较大的“语义重复”；
- Qlty 的 Tree-sitter AST fingerprint 会丢弃变量名等 literal 信息，能找到“相似重复”，同时也更需要人工判断；
- Sonar 对非 Java 默认至少 100 个连续 token、通常至少 10 行，偏向高置信度大块重复；
- PMD 文档提醒 lexer 错误会跳过整个文件并形成 false negative；Rust 当前也不具备 Java 那样完整的局部 CPD suppression 能力。

所以重复率是**维护风险信号**，不是越低越好，也不应自动驱动跨域抽象。

## 4. Rust 代码质量工具矩阵

这些工具都值得使用，但不能回答“源码重复率”：

| 工具 | 真正检查什么 | 重复相关的常见误解 | CLI/CI/IDE | 状态、许可与限制 | 本仓库建议 |
|---|---|---|---|---|---|
| `cargo clippy` | 800+ Rust lint：correctness、suspicious、style、complexity、perf 等 | `complexity` 是写法复杂度 lint，不是复制粘贴率 | rustup component、Cargo、CI；编辑器通常经 rust-analyzer 调用 | 随 Rust 稳定版发布，MIT/Apache-2.0；pedantic/nursery 可能误报 | **已启用，保留** |
| `cargo fmt` / rustfmt | 统一 Rust 格式 | 统一文本不等于找重复，只能改善 detector 稳定性 | rustup component、Cargo、CI、IDE | 随 Rust 工具链维护，MIT/Apache-2.0；部分配置仅 nightly | **已启用，保留** |
| `cargo-deny` | 漏洞、许可证、来源、禁止 crate、同一 crate 多版本 | `multiple-versions`/workspace `duplicates` 是依赖重复，不是源码重复 | Cargo CLI、官方 Action、pre-commit、SARIF | 活跃，MIT/Apache-2.0；许可证判断需人工处理例外 | 建议另行评估依赖治理 |
| `cargo-audit` | 根据 RustSec 审计 `Cargo.lock`/可审计二进制 | 不扫描自有源码，更不算重复率 | Cargo CLI、`rustsec/audit-check` Action | 活跃，MIT/Apache-2.0；只覆盖已进入 RustSec 的公告，忽略项需复核 | 安全基线候选；与 cargo-deny advisories 二选一主入口 |
| `cargo-geiger` | 统计 workspace 与依赖中的 `unsafe` 使用 | “不安全代码比例”不是漏洞率，也不是重复率 | Cargo CLI、可序列化报告 | 0.13.0 修复 Rust 1.89 构建，MIT/Apache-2.0；官方明确说不能直接判断代码是否不安全 | 安全审计时按需，不设简单比例门 |
| `cargo-udeps` | 编译驱动的未使用依赖检测 | 不找重复 crate 版本/源码 | `cargo +nightly udeps`、pre-commit/CI | 0.1.60（2025-11），MIT/Apache-2.0；运行必须 nightly；doc-test 等会漏报，按名称处理多版本有缺陷 | 定期审计，不进入每次 PR 主链路 |
| `cargo-machete` | 快速、静态近似地找未使用依赖 | 快但“不精确”，不是源码相似度 | Cargo CLI、JSON、Action、pre-commit | 活跃，MIT；build.rs 生成引用、renamed crate 会误报，`--with-metadata` 可能改 lockfile | 比 udeps 更适合 PR 提示，先非阻断 |
| `cargo-semver-checks` | 比较基线与当前 rustdoc JSON，找公开 API SemVer 破坏 | 不检查实现重复或一般代码质量 | Cargo CLI、官方 GitHub Action、workspace 配置 | 活跃，MIT/Apache-2.0；rustdoc JSON 会变；官方说明既有误报也不能抓完全部 SemVer 违规 | 仅发布公共 crate 时启用 |

## 5. 分层落地建议

### P0：无需改现有 Rust lint，先建立重复率基线

1. 保留现有 `cargo fmt --check` 和 `cargo clippy --workspace --all-targets -- -D warnings`。
2. 在开发机或临时 CI 分支试跑 jscpd v5，只输出报告。
3. 人工分类前 20 个最大重复块：真实可抽象、合理样板、测试重复、vendor/生成物误入。
4. 固化排除范围与阈值后，记录全仓和 production-only 两套基线。

验收证据：

- 每个报告都能定位两个或以上源码区间；
- 同时有 duplicated lines、blocks 和 percentage；
- `vendor/**`、`target/**` 不参与指标；
- 基线运行不修改源码、`Cargo.lock` 或 `target`；
- 同一 commit 连续运行结果稳定。

### P1：只管新增，不要求一次性清零历史

- PR 先告警“新增重复块”；观察 2~4 周后再决定是否阻断。
- 建议新代码重复率上限从 3% 开始；全仓指标只允许持平或下降。
- 重复块低于 8 行/70 token 默认忽略，避免 getter、构造样板和断言噪音。
- 任何跨 crate 抽象都要同时检查 AGENTS.md 的依赖方向，不能为了降低指标制造反向依赖。

### P2：按实际治理诉求补工具

- 想统一维护性指标：评估 Qlty，先审 BSL/Fair Source。
- 已有 Sonar：直接开启 Rust analysis，利用 new-code quality gate。
- 想要完全宽松许可、可离线运行：PMD CPD 是稳妥备选，但需接受 JVM 和自行计算百分比。
- 供应链：`cargo-deny`；漏洞入口也可选 `cargo-audit`，避免重复配置两套相同阻断。
- 未使用依赖：PR 提示用 `cargo-machete`，周期深扫用 `cargo-udeps`。
- 公开发布 crate：再加 `cargo-semver-checks`。
- 安全 SAST：CodeQL；不要把它当重复率工具。

## 6. 一手来源

### Rust 官方工具

- [Clippy 介绍与 lint 分类](https://doc.rust-lang.org/stable/clippy/)
- [Clippy 用法、workspace 和 CI](https://doc.rust-lang.org/stable/clippy/usage.html)
- [cargo fmt 官方说明](https://doc.rust-lang.org/cargo/commands/cargo-fmt.html)

### 重复检测

- [jscpd 官方仓库与 v5 能力](https://github.com/kucherenko/jscpd)
- [jscpd v5 Rust engine CLI、reporter 与阈值](https://github.com/kucherenko/jscpd/blob/master/docs/rust.md)
- [jscpd 支持格式：Rust `.rs`](https://github.com/kucherenko/jscpd/blob/master/FORMATS.md)
- [Qlty 官方仓库、Rust duplication metrics 与许可证](https://github.com/qltysh/qlty)
- [Qlty duplication engine 原理](https://docs.qlty.sh/duplication)
- [Qlty Rust language support](https://docs.qlty.sh/languages/rust)
- [PMD 7.9.0：CPD 新增 Rust](https://pmd.github.io/2024/12/27/PMD-7.9.0/)
- [PMD CPD CLI、限制与退出码](https://pmd.github.io/pmd/pmd_userdocs_cpd.html)
- [PMD CPD 报告格式](https://pmd.github.io/pmd/pmd_userdocs_cpd_report_formats.html)
- [Sonar Rust 分析：包含 duplication](https://docs.sonarsource.com/sonarqube-server/2025.3/analyzing-source-code/languages/rust)
- [Sonar duplication metrics 与计算公式](https://docs.sonarsource.com/sonarqube-cloud/digging-deeper/metric-definitions)
- [cargo-duplicated 官方仓库](https://github.com/bircni/cargo-duplicated)
- [cargo-duplicated 0.2.0 变更](https://github.com/bircni/cargo-duplicated/blob/main/CHANGELOG.md)
- [rust-code-analysis 官方 metrics 列表](https://mozilla.github.io/rust-code-analysis/metrics.html)

### 其他 Rust 质量与安全工具

- [cargo-deny 官方仓库](https://github.com/EmbarkStudios/cargo-deny)
- [cargo-deny bans：依赖多版本与 workspace dependency duplicates](https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html)
- [cargo-audit 官方 README](https://github.com/rustsec/rustsec/tree/main/cargo-audit)
- [cargo-geiger 官方仓库](https://github.com/geiger-rs/cargo-geiger)
- [cargo-udeps 官方仓库和已知限制](https://github.com/est31/cargo-udeps)
- [cargo-machete 官方仓库、误报与 CI](https://github.com/bnjbvr/cargo-machete)
- [cargo-semver-checks 官方仓库与限制](https://github.com/obi1kenobi/cargo-semver-checks)
- [CodeQL Rust GA 公告](https://github.blog/changelog/2025-10-14-codeql-scanning-rust-and-c-c-without-builds-is-now-generally-available/)
- [CodeQL Rust 支持版本与框架](https://codeql.github.com/docs/codeql-overview/supported-languages-and-frameworks/)
- [CodeQL Rust 内置 query](https://docs.github.com/en/code-security/reference/code-scanning/codeql/codeql-queries/rust-built-in-queries)
