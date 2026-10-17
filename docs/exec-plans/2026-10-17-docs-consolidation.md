# 文档整合实施计划

日期：2026-10-17。范围只限 `docs/` 的内容合并与删除。不调整目录结构，不改 Rust 代码，文档不使用 Mermaid。拿不准是否过期的文件标为待确认，不删。

每项都已打开文件核对状态、结论和引用，不按文件名推断。工作在独立分支 `docs/consolidation-2026-10-17` 上进行，起点为 `dev`（`63ce3b6e`），不带入工作区里与本任务无关的未提交改动。

## 范围

纳入：

- `docs/superpowers/` 全部 18 份计划与设计
- `docs/` 根目录散件
- iOS 规格、启动提示、验收记录、发布记录
- 已核对的同题规格 / 设计 / 执行计划
- 部署与 macOS 发布记录
- 2026-06-28 两份旧 e2e 文档
- `docs/README.md` 的历史说明，以及删除后的全库链接

不纳入：

- `docs/research/assets/` 下约 80 个 JSON（后续建议：按引用再决定是否归档测量原始数据）
- `docs/exec-plans/archive/` 与 `docs/prototypes/archive/`（仍被现行规格引用，保留）
- 工作区未提交的 `docs/prototypes/norma-assistant/`、`docs/exec-plans/2026-10-17-norma-assistant-goal.*`、`docs/references/developer-id-certificate-setup.md`、`scripts/desktop-workspace-e2e.*`
- Rust、脚本行为与目录重组

链接修复：删除前用检索覆盖 Markdown、HTML、Rust 注释、脚本和 `AGENTS.md` / `CLAUDE.md` / `ARCHITECTURE.md` / `docs/README.md`。相对链接按所在文件解析。正本顶部写「历史：见 git 提交」并列出原路径。完成后扫描死链。`AGENTS.md` 与 `CLAUDE.md` 若无需改链接则不动，并运行 `bash scripts/check-agent-guide-lines.sh`。

验证：

- 全库不再出现已删路径
- `docs/` 内相对链接指向仍存在的文件
- 上述行数检查通过
- 提交走仓库 pre-commit，不使用 `--no-verify`

风险：历史验收日志很长，合并后单文件变长，但步骤清单不再冒充当前施工入口。状态互相冲突时，在合并文顶部写明以较新日期的段落为准，不改写当时的验收句子。

## 1. superpowers

`Cargo.toml` 中已无 `realtime` crate。这 18 份是旧施工步骤，不是当前入口。

| 原文件 | 处置 | 正本 |
| --- | --- | --- |
| `plans/2026-06-27-voice-pipeline-optimization.md` | 结论已在规格中（松键后再等 500ms，`cancel()` 不等待，解析前做最小 LLM 修正）。删计划 | `product-specs/voice-pipeline-optimization.md` |
| `plans/2026-07-01-card-task-identity.md` | 首条 prompt 截取 60 字并锁定等结论已在规格中。删计划 | `product-specs/card-task-identity.md` |
| `plans/2026-06-28-wallpaper-settings.md` | 规格已含 `surface()`、`core-video =0.4.3`、10 个内置素材。删计划。规格仍写「待实施」，不改成已上线 | `product-specs/2026-06-28-SPEC-wallpaper-settings.md` |
| `specs/2026-06-27-canvas-session-persistence-design.md` 与同名 plan | 并入仍被引用的会话规格：视图恢复、`node_sessions` 与 `session_checkpoints` 分开、失败则保持原行为、`proc_pidinfo` 取 cwd、Claude `--resume`。然后删除两份 | `product-specs/2026-07-31-SPEC-agent-card-fresh-session-and-context-isolation.md` |
| `specs/2026-07-01-project-agent-catalog-design.md` 与同名 plan | 并入仍有效的目录决策：定义在项目库 `agent_definitions`，不进全局 settings；首版复制 name/profile，不把 `workflow_description` 送进 CLI，不给 `AgentSpec` 加 `definition_id`。然后删除两份 | `product-specs/2026-07-14-SPEC-voice-addressable-agents.md` |
| `plans/2026-07-09-local-voice-pipeline-01-speech-skeleton.md`、`02-llm-and-card-tools.md` | 步骤清单删除。规格仍写「待实施」，是否被后续语音规格取代标为待确认，不改状态 | `product-specs/2026-07-09-SPEC-local-voice-pipeline.md` |
| `plans/2026-07-01-voice-input-device-settings.md` | 并入三条仍未见他处写明的规则：空选择才是系统默认；显式设备缺失必须报错；切换不热切当前录音；当时不做输出设备 | `product-specs/2026-07-19-SPEC-multi-provider-asr-and-azure-speech.md` |
| `plans/2026-07-05-cef-browser-engine-switch.md` | 默认 Wry、CEF 仅 `chromium-browser` 已写在 Browser 规格验收表。删计划 | `product-specs/2026-07-16-SPEC-browser-chrome-auth.md` |
| `plans/2026-07-01-auto-release-version-bump.md` | 该计划要求打包前改版本；2026-08-05 发布计划已改为打包不改版本、成功后再递增。以后者为准，记入 macOS 发布文 | `references/macos-release-and-notarization.md` |
| `plans/2026-07-01-canvas-performance-fixes.md` | 热路径禁令已在 `CLAUDE.md`。删计划，链到 `docs/README.md` 历史说明 | 历史说明 |
| `plans/2026-06-27-realtime-partials-async-channel.md`、`voice-sherpa-streaming.md`、三份 realtime card task 计划 | 引用已不存在的 `realtime` / `card_task` 或旧语音 crate。作为历史删除 | 历史说明 |

## 2. docs 根目录散件

已检索全库：

| 文件 | 引用 | 处置 |
| --- | --- | --- |
| `design_github_clone.md` | 无外部引用。Phase 1 自称已实现，Phase 2/3 未做。正文只留在 git 历史 | 删除 |
| `swiftui-style-spec.md` | 无外部引用。适用对象是已不存在的 `apple/Sources/MultisoulApp` | 删除 |
| `landing.html` | 仅自指 | 删除 |
| `design.md`、`design.html` | `core-ui-v2/README.md`、`product-ui` 检查脚本、`exec-plans/2026-07-15-product-ui-native-refresh.md`、`crates/app/src/settings/typography.rs` 注释。`design.md` 自标 V2 / Current Direction（2026-08-08），`design.html` 是评审页 | 保留。在 `docs/README.md` 说明去向，不改路径 |
| `product-ui.html` | `core-ui-v2/README.md`、`scripts/check-product-ui-prototype.sh`、`scripts/render-product-ui-assets.sh` | 保留。README 说明它是 Core UI 交互原型 |
| `ion-fireworks.html` | `scripts/check-ion-fireworks-prototype.sh`、`product-specs/2026-08-01-SPEC-dispatch-receipt-particle-effect.md` | 保留。README 说明它是派工粒子效果的视觉基准 |

## 3. iOS 文档簇

正本与并入：

| 正本 | 并入后删除 |
| --- | --- |
| 新建 `product-specs/2026-09-28-norma-ios-issues-spec.md` | `issue-54` 至 `issue-60` 七份。保留各票原文与状态，用锚点 `#issue-54` … `#issue-60`。#59 文件同时写了 #57 送达，不拆开改写 |
| `product-specs/2026-09-06-norma-ios-companion.md` | `2026-09-26-norma-ios-workspace-feature-list.md`、`2026-09-28-norma-ios-data-storage-matrix.md`、`2026-09-28-norma-ios-prototype-fidelity-acceptance.md` 作为附录。实时指挥台仍以 `2026-09-13-norma-ios-realtime-companion.md` 为现行产品方向，不并入这份历史提案 |
| `exec-plans/2026-09-28-norma-ios-issues-01-05-native-goal.md` 文末 | `2026-09-28-norma-ios-issues-01-05-start-prompt.md` |
| `exec-plans/2026-09-29-norma-ios-single-mac-goal.md` 文末 | `2026-09-29-norma-ios-single-mac-start-prompt.md` |
| 新建 `exec-plans/2026-09-29-norma-ios-acceptance.md` | `new-shell-acceptance`、`thread-acceptance`、`local-test`。顶部说明较新日期优先 |
| 新建 `exec-plans/2026-09-08-norma-ios-release-records.md` | `2026-09-08-norma-ios-testflight.md`、`2026-09-30-norma-ios-testflight-dispatch.md`、`2026-10-05-workspace-chat-self-test-release.md`、`2026-10-06-mobile-self-test-release.md`。新记录在前 |

`norma-ios-style-guide.md` 已写明 HTML 是视觉速览、冲突以 Markdown 为准。已入库树没有成对的 `norma-assistant` Markdown/HTML；未提交的那对不纳入。

## 4. 同题文档

下表是打开后确认的跨目录同题组。用户举例里的 21 组包含其中若干对；没有第二份文件的主题不另造分组。

| 主题 | 正本 | 并入后删除 | 说明 |
| --- | --- | --- | --- |
| asr-model-framework | `product-specs/2026-07-11-SPEC-asr-model-framework.md` | 同名 design-doc 与 exec-plan | 规格已含实现决策。补 2026-07-11 阶段状态，以及设计文 2026-07-18 的 Zipformer Bilingual 非推荐说明。阶段 4 仍未推荐，见 `quality/asr-model-release-gate.md` |
| browser-chrome-auth | `product-specs/2026-07-16-SPEC-browser-chrome-auth.md` | 同名 design-doc 与 exec-plan | 规格已含 MV3、禁止直接复用 Profile、不把 Cookie 写入 SQLite。执行计划六段均已勾选 |
| software-licensing | `product-specs/2026-07-14-SPEC-software-licensing.md` | `design-docs/2026-07-14-software-licensing.md` | 补「一期不控设备、人工发证」。规格仍是 ready-for-agent，不写成已上线 |
| database-environment-isolation | `product-specs/2026-07-25-SPEC-database-environment-isolation.md` | 同名 design-doc | 规格已是「已实施」。把设计文中的识别方式补进规格：`production-data` 只由打包打开；不用 `debug_assertions`；不用环境变量切到生产；`NORMA_TEST_DATA_ROOT` 只改测试路径；禁止失败后落到当前目录 `app.sqlite` |
| launchpad-static-visual | 产品规格（已标 2026-07-15 被专用首页视频替代） | exec-plan | 计划同样已标替代 |
| agent-card-corner-glow | 产品规格（已标历史，现行见完成态边框） | exec-plan | 735 行步骤清单删除 |
| single-voice-command | 产品规格，决策亦见 ADR 0013 | exec-plan | 代码已有 `SingleVoiceCommand` |
| core-ui-v2-theme-switching | `verification/2026-07-16-core-ui-v2-theme-switching.md` | exec-plan | 验收报告保留六套主题与 Computer Use 记录 |
| developer-id-notarize | 见第 5 节 | design-doc 与 exec-plan | 2026-07-05 写明证书阶段当时挂起；现行命令以 `references/packaging.md` 为准 |

待确认，不删：

| 主题 | 原因 |
| --- | --- |
| desktop-unified-scene 规格与 Goal | Goal 状态为 ACTIVE，统一场景验收未完成 |
| desktop-multi-project-workspaces 规格与 exec-plan | 计划写明 #65–#71 任务拆分仍有效，不因统一场景而自动关闭 |
| onboarding 规格与 exec-plan | 规格写待实施；代码中没有 `OnboardingState`。只把计划中的修正写进规格：不新增 `AppScreen::Onboarding`，用 overlay；锚点在没有元素 bounds 查询时用画布到屏幕的投影 |
| settings-visual-refresh 规格与 exec-plan | 计划没有完成记录；当前 Settings 源码中没有该计划里的 `Input & Controls` 分组文案 |
| voice-addressable-agents exec-plan | 规格自标 Partially superseded，不能把步骤清单当成已完成而删除 |

## 5. 部署与发布

当前云部署以 2026-10-06 统一服务为准：`services/norma-cloud` 的 `norma-cloud` 进程，`47.251.242.24`，公网仍是 `soulvoiceai.com` 上的 `/norma/voice/*`、`/norma/healthz`、`/norma/v1/notifications`。原文在 `exec-plans/2026-10-06-norma-cloud-unified-deployment.md`。

并入新文件 `exec-plans/2026-10-06-norma-cloud-deployment.md` 后删除：

| 文件 | 核对结论 |
| --- | --- |
| `2026-09-14-voice-vercel-deployment.md` | 曾部署 `norma-voice.vercel.app`，随后被自有服务器取代 |
| `2026-09-15-voice-server-deployment.md` | 曾用 `norma-voice` 进程，已被统一服务取代 |
| `2026-10-06-norma-cloud-relay-deployment.md` | 同日较早的中继部署，统一服务文已写明替代旧 voice/push |
| `2026-09-29-norma-cloud-text-release.md` | 当日明确未上传、未重启生产。不把「不要部署」当成当前禁令 |
| `2026-10-06-norma-cloud-unified-deployment.md` | 作为新文件的「当前生效」正文 |

macOS 发布与公证并入 `references/macos-release-and-notarization.md`，操作命令仍以 `references/packaging.md` 为准，两边互链。并入后删除：

| 文件 | 保留的结论 |
| --- | --- |
| `2026-08-05-github-sparkle-auto-update-release.md` | Sparkle 2 feed、Draft 核验后再公开、打包不改版本。文中「尚未做首次真实发布」是 2026-08-05 的状态，不改写成今天的发布结论 |
| `2026-07-05-developer-id-notarize-design.md` 与 exec-plan | Developer ID 签名、公证、staple；不走 Mac App Store Sandbox。Team ID `2BF8G9AN4L` |
| `2026-10-11-cloud-media-publish.md` | `media-v1` 的 12 个原始媒体与验收证据。这是 GitHub Release 媒体，不是云节点部署 |

`design-docs/2026-09-28-norma-cloud-service-architecture.md` 是产品拓扑，不是部署记录，保留。

## 6. 其他

| 项 | 处置 |
| --- | --- |
| `exec-plans/archive/2026-09-30-desktop-workspace-canvas-rebuild.md` | 保留。同名短文件只是指向归档与统一场景的入口 |
| `prototypes/archive/` | 保留。companion 规格与原型 README 仍引用其中 HTML 与截图 |
| `product-specs/2026-06-28-SPEC-e2e-testing.md`、`exec-plans/2026-06-28-e2e-testing-infra.md` | 删除。二者描述已移除的 `crates/e2e` 与 headless GPUI 套件。`docs/README.md` 已说明旧套件移除 |
| `design-docs/norma-ios-style-guide.md` 与 `.html` | Markdown 已说明关系，复核后不改写 |
| `research/assets` JSON | 不动。后续建议：先列出零引用 JSON，再单独决定是否移出当前树 |

## 执行顺序

1. 提交本计划。
2. 写正本中的短结论，生成合并文件，修正链接，再删除被并入文件。
3. 更新 `docs/README.md`。
4. 扫描死链与已删路径，运行指南行数检查。
5. 按主题分次提交，经 pre-commit 后向 `dev` 提 PR。
