# jscpd v5 GitHub Actions 集成调研

> 日期：2026-08-02
> 范围：Rust 重复代码扫描、jscpd v5、GitHub Actions、SARIF 与产物管理
> 约束：本报告只给出方案，不修改源码或现有 CI。

## 1. 结论

推荐在现有 Rust CI 之外增加独立的 Ubuntu duplication workflow，并直接运行固定版本：

```bash
npx --yes jscpd@5.0.14 ...
```

不建议把 `kucherenko/jscpd` composite Action 作为首选 gate。直接调用 CLI 有三个实际优势：

1. `threshold` 触发的非零退出码会自然传递给 GitHub Actions，不需要二次检查 Action output。
2. SARIF、JSON、HTML 的生成和上传可分别配置，避免“开启上传但没有生成文件”的隐式行为。
3. 避开官方 Action 运行时下载 `https://jscpd.dev/install.sh`、内部嵌套浮动 Action tag，以及报告缺失被解析成全零等问题。

推荐按以下方式分层：

- `production-duplication`：扫描各业务 crate 的 `src/**/*.rs`，显式排除 `crates/e2e/**`、测试目录和测试辅助模块；先只采集基线，再对全量 production source 重复率设阈值并阻断。
- `test-duplication-report`：扫描 `crates/*/tests/**/*.rs` 与 `crates/e2e/src/**/*.rs`，只输出报告，不因重复率阻断。
- 使用 `ubuntu-latest`，不安装 Rust toolchain、不运行 Cargo，也不恢复 `target` 缓存。
- 不宣称 jscpd 原生支持“仅新增代码”或 diff gate；它的 threshold 针对本次扫描集合的总体重复率。

## 2. 当前仓库约束

当前 [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) 只有一个 `macos-latest` 的 `rust-checks` job，承担 guard script、`cargo check`、`rustfmt` 和 `clippy`。重复率扫描不依赖 macOS、Cargo 构建或 GPUI，因此应拆到独立 Ubuntu workflow，与现有昂贵的 macOS job 并行执行。

本仓库测试规范强制要求：

- 测试包含完整的场景、数据构造、执行过程和预期结果注释；
- 数值预算必须写推导过程；
- 每个断言必须有说明，并显式覆盖正、负验收标准。

这些要求会合理地产生重复的测试说明和断言骨架。因此测试重复率适合做趋势报告，不适合作为合并门禁。若为消除测试报告中的重复而抽象掉逐项断言，反而可能破坏仓库的可审计性规范。

## 3. 为什么首选直接 CLI

### 3.1 固定版本与退出码

截至调研日，官方最新版本是 [`v5.0.14`](https://github.com/kucherenko/jscpd/releases/tag/v5.0.14)，tag commit 为：

```text
2a6a1a406a9b960547adcb11ec6409ded079d96e
```

`npx --yes jscpd@5.0.14` 固定了 CLI 版本。CLI 的 threshold/运行错误会直接成为 step 的退出码；只需在同一步验证 JSON 报告确实生成，即可避免“扫描器未产出报告但任务误绿”。

如果未来追求更强的供应链复现性，可将 jscpd 声明为锁文件依赖并用 `npm ci` 安装；这会引入仓库文件变更，不属于本次报告的实施范围。

### 3.2 推荐配置骨架

建议新增一个 `.jscpd.json`，让扫描口径集中在一个地方。基线阶段必须省略 `threshold`，不要写成 `0`；`0` 表示只要重复率大于 0 就失败。

```json
{
  "format": ["rust"],
  "pattern": "**/src/**/*.rs",
  "minTokens": 70,
  "minLines": 8,
  "mode": "mild",
  "ignore": [
    "**/target/**",
    "**/vendor/**",
    "**/generated/**",
    "**/fixtures/**",
    "**/snapshots/**",
    "**/*.snap",
    "**/tests/**",
    "e2e/**",
    "crates/e2e/**",
    "**/e2e/**",
    "**/e2e.rs"
  ],
  "reporters": ["console", "json", "html", "sarif"],
  "output": "report/production",
  "gitignore": true,
  "absolute": false
}
```

这里排除 `**/e2e/**` 与 `**/e2e.rs`，是因为本仓库既有独立的 `crates/e2e` 测试 crate，也有 `crates/app/src/app/e2e*` 测试辅助模块。它们应进入测试趋势报告，而不是 production gate。

### 3.3 推荐命令骨架

下面是设计示意，不是对仓库的实际修改。基线阶段不传 `--threshold`：

```yaml
- name: Scan production duplication
  shell: bash
  run: |
    set +e
    npx --yes jscpd@5.0.14 --config .jscpd.json crates
    scan_exit=$?
    set -e
    test -f report/production/jscpd-report.json || {
      echo "::error::jscpd JSON report was not generated"
      exit 2
    }
    exit "$scan_exit"
```

阈值执行阶段在 `.jscpd.json` 中增加经基线批准的数值，例如：

```json
{
  "threshold": 4.3
}
```

不要同时用 `--exit-code` 实现“发现任何 clone 就失败”。`--threshold` 更适合控制总体重复率，避免第一次接入就被历史存量完全阻塞。

## 4. 官方 Action v5.0.14 的真实接口

以下内容来自 tag `v5.0.14` 的官方 [`action.yml`](https://github.com/kucherenko/jscpd/blob/v5.0.14/action.yml)，而不是 Marketplace 摘要。

| input | 默认值 | 作用 |
|---|---:|---|
| `path` | `.` | 空格分隔的扫描路径 |
| `config` | 空 | `.jscpd.json` 路径 |
| `min-tokens` | `50` | 最小重复 token 数 |
| `min-lines` | `5` | 最小重复行数 |
| `max-lines` | 空 | 单块最大行数 |
| `mode` | `mild` | `mild`、`weak` 或 `strict` |
| `format` | 空 | 逗号分隔的格式 |
| `ignore` | 空 | 逗号分隔的文件 glob |
| `ignore-pattern` | 空 | 跳过匹配 token 的正则，不是文件 glob |
| `reporters` | `console` | reporter 列表 |
| `output` | `report` | 文件报告目录 |
| `threshold` | 空 | 重复率阈值 |
| `blame` | `false` | 增加 Git blame 信息 |
| `exit-code` | 空 | 发现重复时使用默认或指定退出码 |
| `pattern` | 空 | 文件搜索 glob |
| `max-size` | 空 | 跳过超过给定大小的文件 |
| `skip-local` | `false` | 忽略同目录内的 clone |
| `ignore-case` | `false` | 忽略符号大小写，实验性 |
| `follow-symlinks` | `false` | 跟随符号链接 |
| `no-gitignore` | `false` | 不遵守 `.gitignore`；默认会遵守 |
| `absolute` | `false` | 报告使用绝对路径 |
| `formats-exts` | 空 | 自定义格式与扩展名映射 |
| `formats-names` | 空 | 自定义格式与文件名映射 |
| `version` | `latest` | 安装的 jscpd 版本 |
| `install-prefix` | 空 | 安装目录 |
| `skip-install` | `false` | 使用已有 `jscpd`/`cpd` |
| `extra-args` | 空 | 直接附加 CLI 参数 |
| `upload-report` | `false` | 上传报告 Artifact |
| `upload-sarif` | `true` | 尝试上传 SARIF |

Action outputs 为：`duplication-percentage`、`clones-found`、`duplicated-lines`、`total-lines`、`files-count`、`report-path`、`sarif-path` 和 `exit-code`。

### 4.1 安装行为

在 Linux/macOS 上，Action 最多三次执行远程安装脚本 `https://jscpd.dev/install.sh`，失败后 fallback 到全局 npm 安装；Windows 使用 npm。即使 Action 本身固定到 commit，安装脚本仍是运行时下载，因此并未完全固定供应链。至少要同时设置 `version: "5.0.14"`。

### 4.2 threshold 不会自动阻断 Action

CLI 对 threshold 的非零退出是有效的，但 composite Action 实际执行：

```bash
jscpd "${ARGS[@]}" || EXIT_CODE=$?
echo "exit-code=${EXIT_CODE:-0}" >> "$GITHUB_OUTPUT"
```

它捕获退出码后没有再次 `exit`。因此，单纯填写 `threshold` 或 `exit-code` 不会可靠地使 workflow 失败。这与官方 CI 文档所写的“超过 threshold workflow 失败”不一致。

若仍选择 Action，必须增加显式 gate：

```yaml
- name: Run jscpd
  id: jscpd
  uses: kucherenko/jscpd@2a6a1a406a9b960547adcb11ec6409ded079d96e
  with:
    version: "5.0.14"
    path: crates
    format: rust
    pattern: "**/src/**/*.rs"
    reporters: console,json,html,sarif
    output: report/production
    threshold: "<approved-threshold>"

- name: Verify report and enforce jscpd exit code
  if: always()
  shell: bash
  env:
    JSCPD_EXIT_CODE: ${{ steps.jscpd.outputs.exit-code }}
  run: |
    test -f report/production/jscpd-report.json || exit 2
    case "${JSCPD_EXIT_CODE:-}" in
      0) ;;
      ''|*[!0-9]*) exit 2 ;;
      *) exit "$JSCPD_EXIT_CODE" ;;
    esac
```

### 4.3 SARIF 并非默认真正上传

虽然 `upload-sarif` 默认是 `true`，但 `reporters` 默认只有 `console`。Action 只会强制补 `json`，不会补 `sarif`。因此默认配置不会生成 SARIF，自然也不会上传。

要生成 SARIF，必须显式把 `sarif` 放入 `reporters`。v5.0.14 的实际文件名是：

- `<output>/jscpd-report.json`
- `<output>/jscpd-report.sarif`

官方 SARIF 页面仍展示过时文件名 `jscpd-sarif.json`，实施时应以 [`parse-output.js`](https://github.com/kucherenko/jscpd/blob/v5.0.14/.github/actions/jscpd/parse-output.js) 和 reporter 源码为准。

更严重的是：报告不存在、JSON 无 `statistics.total` 或解析失败时，parser 会把所有统计 output 写成 `0` 并成功退出。因此必须独立检查 `jscpd-report.json` 存在，不能把 `duplication-percentage == 0` 单独当成成功证据。

Action 内部 SARIF 上传固定使用 `github/codeql-action/upload-sarif@v3`，且 `continue-on-error: true`；报告 Artifact 固定命名为 `jscpd-report`，使用 `actions/upload-artifact@v4`，同样 `continue-on-error: true`。当前 [GitHub SARIF 官方示例](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/integrate-with-existing-tools/upload-sarif-file) 已使用 `upload-sarif@v4`。这也是推荐在外层自行管理上传步骤的原因。

### 4.4 config 与 ignore 的边界

- Action input 使用 kebab-case，如 `min-tokens`、`ignore-pattern`；`.jscpd.json` 使用 camelCase，如 `minTokens`、`ignorePattern`。
- `ignore` 是文件 glob；`ignore-pattern` 是代码 token 正则，二者不能互换。
- jscpd 默认遵守 `.gitignore`，只有 `no-gitignore: true` 才关闭。
- Action 总会显式传 `--reporters` 和 `--output`，默认 `path` 也是 `.`。这些 CLI 参数可能覆盖 config 中同名值。

因此，如果选 composite Action，建议把 path、format、pattern、ignore、reporters、output、threshold 全部明确写在 workflow 的 `with` 中，避免 config 与 Action defaults 双重管理。直接 CLI 方案也应只保留一个配置权威来源。

## 5. 推荐 GitHub Actions 架构

### 5.1 独立 workflow

建议新增独立的 `Duplication` workflow，而不是塞入当前 `rust-checks`：

```text
pull_request / push(main, dev)
              │
              ├── production-duplication  [ubuntu, baseline 后 gate]
              └── test-duplication-report [ubuntu, 永久 report-only]
```

两类 job 都只 checkout 源码并运行 jscpd，不需要 Rust toolchain、`cargo build`、`sccache` 或 `target` 缓存。这既降低 macOS runner 成本，也不会增加本次要治理的 Rust 编译中间产物。

建议沿用当前 CI 的 concurrency/cancel-in-progress 语义，避免同一 PR 的旧扫描继续消耗资源。

若阶段 B 将 `production-duplication` 设为 required check，不要在 workflow 的 `pull_request` 触发器上增加 `paths`/`paths-ignore`。GitHub 明确说明：workflow 因路径过滤被跳过时，关联检查可能一直保持 Pending，进而阻塞合并。jscpd v5 扫描很轻，required check 每个 PR 都运行更稳妥；参考 [GitHub required status check 排障文档](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/troubleshooting-required-status-checks)。

### 5.2 production gate 的范围

第一版扫描集合建议为各业务 crate 的 `src/**/*.rs`，排除：

- `target`、vendor、生成代码；
- fixtures、snapshots；
- 独立的 `tests` 目录。
- `crates/e2e/**`、`**/e2e/**` 与 `**/e2e.rs` 测试辅助代码。

需要明确：jscpd 是文件/token 扫描器，不理解 Rust 的 `#[cfg(test)]` 语义。写在 `src/*.rs` 内的 inline test 仍会进入 production 文件集合。因此这里的 production-only 是“文件路径级 production source”，不是 AST 级编译条件过滤。若 inline test 对结果影响明显，应人工审阅 clone，或把该路径列为例外；不能声称工具已经自动剔除。

`min-tokens: 70`、`min-lines: 8` 是建议起点，用于降低 Rust trait 实现、builder、match 分支等短模板噪声，不应直接视为最终标准。最终参数要以仓库基线和误报抽样为依据。

### 5.3 tests 永久 report-only

测试扫描覆盖 `crates/*/tests/**/*.rs`、`crates/e2e/src/**/*.rs`，并按需补充其他明确的测试辅助路径；不传 `threshold` 和 `exit-code`。扫描器安装失败、命令崩溃或报告缺失仍应使 job 失败；发现重复本身不阻断。

测试报告重点用于发现：

- 可以复用但不会隐藏验收语义的数据构造 helper；
- 大段完全相同的 fixture；
- 复制后已经漂移的测试逻辑。

不要根据重复率自动合并文档注释、预算推导或正负断言。任何测试抽象都必须继续满足仓库的逐项可读、逐项可审计要求。

### 5.4 Artifact 与 SARIF

- production：生成 `console,json,html,sarif`；trusted push/内部 PR 可上传 SARIF。
- tests：生成 `console,json,html`；不上传 SARIF，避免 Security 页面被非阻断的测试重复污染。
- 两个 job 若都保存 Artifact，必须在 workflow 外层用不同名字，如 `jscpd-production-report` 与 `jscpd-tests-report`。不要同时依赖 composite Action 固定的 `jscpd-report` 名称。
- 建议保留 14 天用于基线观察；官方 composite Action 没有暴露 `retention-days`，需自行调用并固定 `actions/upload-artifact` 的完整 commit SHA。

## 6. GitHub 权限、fork PR 与供应链

### 6.1 最小权限

普通扫描和 Artifact 只需：

```yaml
permissions:
  contents: read
```

需要上传 SARIF 时增加：

```yaml
permissions:
  contents: read
  security-events: write
  # private repository 按 GitHub 文档还需要 actions: read
```

private/internal repository 还必须具备 GitHub Code Security。权限要求以 [GitHub SARIF 文档](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/integrate-with-existing-tools/upload-sarif-file) 为准。

### 6.2 fork 与 Dependabot

fork PR 和 Dependabot PR 的 `GITHUB_TOKEN` 按只读模型运行，不能把 `security-events: write` 或 SARIF 上传当作可靠 gate；首次外部贡献者还可能等待 maintainer 批准。正确策略是：

- 仍在 `pull_request` 上执行无 secrets 的扫描和控制台/Artifact 报告；
- SARIF 上传失败不能掩盖 CLI 扫描结果；
- trusted `push` 或内部 PR 再上传 SARIF；
- 不使用 `pull_request_target` checkout 并执行不可信 fork 代码，避免 pwn-request 风险。

参考：[fork workflow 行为](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflows-in-forked-repositories)、[安全使用 `pull_request_target`](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target)。

### 6.3 固定 Action

GitHub 官方指出，完整 commit SHA 是第三方 Action 唯一不可变的固定方式。若采用 composite Action，应使用：

```yaml
uses: kucherenko/jscpd@2a6a1a406a9b960547adcb11ec6409ded079d96e # v5.0.14
```

checkout、SARIF upload、Artifact upload 也应固定到审核过的 release commit，而不是只写 `@v4`。参考 [GitHub Secure use reference](https://docs.github.com/en/actions/reference/security/secure-use#using-third-party-actions)。

但要注意，固定 jscpd Action 顶层 SHA 并不能固定其内部的远程安装脚本和 `@v3`/`@v4` 嵌套引用；这进一步支持“直接固定 CLI + 外层固定上传 Action”的主方案。

## 7. 两阶段落地

### 阶段 A：baseline，建议 2–4 周

- production 与 tests 都不设 threshold。
- 每次扫描保留 JSON/HTML，记录 `percentage`、`clones`、`duplicatedLines`、`lines`、`sources`。
- 抽样检查 Rust 宏、trait impl、协议映射、测试模板等误报。
- 固定扫描范围、ignore、`minTokens`、`minLines` 后再比较趋势；参数变化会让历史曲线失去可比性。

阶段 A 的成功标准不是“零重复”，而是：报告稳定生成、范围可解释、误报可接受、主干基线有足够样本。

### 阶段 B：enforce

- 只给 production source 设置经评审批准的全量重复率 threshold。
- tests 继续 report-only。
- 将 production job 设为 branch protection required check。
- 通过显式 JSON 文件存在性验证，区分“真实 0%”和“报告没有生成”。
- 阈值应从稳定基线推导，不能在没有数据时武断采用 0% 或任意行业数字。

### 非目标：new-code diff gate

jscpd v5 的 path、pattern、ignore 和 threshold 都作用于本次扫描集合。官方 Action 没有 base/head 比较 input，也没有“只统计本 PR 新增行”的门禁能力。

未来若要“新增代码不得提高重复率”，需要外部逻辑，例如分别扫描 merge-base 与 head、读取两个 JSON 并比较，或根据 Git diff 构造待扫描文件集合。这是仓库自建 gate，不是 jscpd 原生能力；第一阶段不建议把这种额外复杂度与基础接入同时引入。

## 8. 验收清单

实施时应逐项验证：

- workflow 在 `ubuntu-latest` 独立运行，不触发 Cargo build，也不缓存 `target`。
- production 与 tests 使用不同扫描集合和不同报告目录。
- production 基线期不阻断，批准 threshold 后非零 CLI 退出确实使 job 失败。
- tests 发现重复时 job 仍成功，但扫描失败或 JSON 缺失时 job 失败。
- JSON 实际路径为 `<output>/jscpd-report.json`；SARIF 为 `<output>/jscpd-report.sarif`。
- SARIF reporter 被显式启用，不依赖 `upload-sarif: true` 的默认值。
- fork PR 不依赖 secrets 或 SARIF write permission 完成 duplication gate。
- 所有第三方 Actions 使用审核过的完整 commit SHA。
- branch protection 只把 production duplication 设为 required check。
- production duplication 成为 required check 后，workflow 触发器不使用会让整条 workflow 被跳过的路径过滤。
- 文档明确写出：当前门禁是全量 production source threshold，不是 new-code diff gate。

## 9. 官方来源

- [jscpd v5.0.14 Release](https://github.com/kucherenko/jscpd/releases/tag/v5.0.14)
- [jscpd v5.0.14 action.yml](https://github.com/kucherenko/jscpd/blob/v5.0.14/action.yml)
- [jscpd v5.0.14 parse-output.js](https://github.com/kucherenko/jscpd/blob/v5.0.14/.github/actions/jscpd/parse-output.js)
- [jscpd Rust 使用文档](https://jscpd.dev/getting-started/rust)
- [jscpd 配置文档](https://jscpd.dev/getting-started/configuration)
- [jscpd CI 文档](https://jscpd.dev/ci-and-hooks/ci)
- [jscpd SARIF 文档](https://jscpd.dev/reporters/sarif)
- [GitHub：上传 SARIF](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/integrate-with-existing-tools/upload-sarif-file)
- [GitHub：安全使用第三方 Actions](https://docs.github.com/en/actions/reference/security/secure-use#using-third-party-actions)
- [GitHub：fork workflow 行为](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflows-in-forked-repositories)
- [GitHub：安全使用 pull_request_target](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target)
- [GitHub：required status check 与被跳过的 workflow](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/troubleshooting-required-status-checks)
