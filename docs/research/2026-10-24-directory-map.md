# 目录结构与小目录清单（清理前快照）

日期：2026-10-24。统计受跟踪文件及未被忽略的未跟踪文件；忽略构建缓存内部、Git 内部及其他 worktree。文件数包含所有子目录。

后续状态：用户已确认并删除 spikes 和旧 planner，以下目录树保留删除前快照；现行目录中不再包含它们。根目录的 `workflows/` 已按用户最新决定恢复并保留。旧材料可从 Git 提交 `0aed07c2` 查阅。

打包工具也已归入 `packaging/cargo-packager-wrapper/`，现与 `packaging/macos/`、`packaging/media/` 并列；空的根目录 `tools/` 已移除。

## 总览

Rust 源码目录共 170 个，其中 58 个目录的整个子树只有 1–2 个文件。App 源码目录共 109 个，其中 38 个属于上述小目录。49 个小目录旁还有同名父模块文件；其中 34 组直接合并后的原始行数超过 500。目录小不等于模块小。

## 根目录

| 目录 | 职责 |
| --- | --- |
| `crates/` | 21 个 Rust crate，桌面产品主体 |
| `ios/` | iOS App、共享逻辑、测试与配对桥接 |
| `services/` | 云服务，目前只有 norma-cloud |
| `test/` | 真实桌面 E2E 驱动 |
| `scripts/` | 构建、发布、护栏与专项验证 |
| `tools/` | Cargo 打包包装工具 |
| `sidecars/` | 旧 OpenAI card planner |
| `assets/` | 图标源文件、模型相关素材与壁纸来源副本 |
| `packaging/` | macOS 配置、媒体清单和预览 |
| `vendor/` | GPUI 与 Sherpa 底层依赖补丁 |
| `docs/` | 规格、设计、计划、研究、原型与历史记录 |
| `spikes/` | 独立 Keygate 和 Web/VR 实验 |
| `workflows/` | Agent 工作流说明，当前一份文档 |
| `.agents/` | 仓库 skills |
| `.cargo/` | Cargo 配置 |
| `.github/` | CI 与发布工作流 |
| `.githooks/` | Git hooks |
| `.claude/` | 本机 Agent 配置，当前未跟踪 |
| `.playwright-mcp/` | 历史浏览器快照 |
| `.superpowers/` | 历史施工记录和工具材料 |
| `.worktrees/` | 本地工作树相关目录，已忽略 |
| `build/` | 本机构建/发布产物，已忽略 |
| `artifacts/` | 本地验收证据，已忽略 |
| `.git/` | Git 内部数据库 |

## Rust crate 目录

每个 crate 的 Cargo 配置、src 与 tests 是独立构建和验证的单位。src 文件数包括子树的资源文件。

| crate 目录 | src 文件数 | tests 文件数 |
| --- | ---: | ---: |
| `crates/agent/` | 34 | 36 |
| `crates/analytics/` | 13 | 2 |
| `crates/app/` | 720 | 408 |
| `crates/asr/` | 13 | 9 |
| `crates/audio/` | 2 | 1 |
| `crates/canvas/` | 15 | 13 |
| `crates/dialogue/` | 8 | 6 |
| `crates/license/` | 4 | 2 |
| `crates/live_voice/` | 14 | 21 |
| `crates/media_store/` | 5 | 3 |
| `crates/mobile_pairing/` | 2 | 1 |
| `crates/model_store/` | 2 | 1 |
| `crates/norma/` | 76 | 62 |
| `crates/runtime/` | 57 | 67 |
| `crates/storage/` | 65 | 72 |
| `crates/terminal/` | 17 | 6 |
| `crates/tts/` | 18 | 8 |
| `crates/ui/` | 4 | 4 |
| `crates/voice_input/` | 9 | 9 |
| `crates/voice_sherpa/` | 9 | 10 |
| `crates/workspace/` | 21 | 14 |

## App 源码目录树

每一行是实际目录及其子树文件数。同名父模块可能在目录旁边。

- `crates/app/src/` — 720 个文件
  - `crates/app/src/app/` — 223 个文件
    - `crates/app/src/app/agent_announcements/` — 8 个文件
    - `crates/app/src/app/agent_runtime/` — 31 个文件
      - `crates/app/src/app/agent_runtime/coordination/` — 1 个文件；小目录
      - `crates/app/src/app/agent_runtime/pending/` — 1 个文件；小目录
    - `crates/app/src/app/bootstrap/` — 3 个文件
      - `crates/app/src/app/bootstrap/visual_preview/` — 1 个文件；小目录
    - `crates/app/src/app/browser/` — 17 个文件
      - `crates/app/src/app/browser/address_input/` — 1 个文件；小目录
      - `crates/app/src/app/browser/store/` — 1 个文件；小目录
    - `crates/app/src/app/chrome/` — 2 个文件；小目录
    - `crates/app/src/app/command_dock/` — 5 个文件
    - `crates/app/src/app/dialogue/` — 18 个文件
      - `crates/app/src/app/dialogue/reply_stream/` — 6 个文件
    - `crates/app/src/app/doubao/` — 13 个文件
      - `crates/app/src/app/doubao/plan/` — 6 个文件
        - `crates/app/src/app/doubao/plan/asr/` — 2 个文件；小目录
      - `crates/app/src/app/doubao/playback/` — 1 个文件；小目录
    - `crates/app/src/app/e2e/` — 17 个文件
      - `crates/app/src/app/e2e/visuals/` — 1 个文件；小目录
    - `crates/app/src/app/ego_preview/` — 7 个文件
    - `crates/app/src/app/feature_guide/` — 13 个文件
      - `crates/app/src/app/feature_guide/view/` — 1 个文件；小目录
    - `crates/app/src/app/frame_loop/` — 1 个文件；小目录
    - `crates/app/src/app/gpt_live/` — 14 个文件
    - `crates/app/src/app/keyboard/` — 5 个文件
    - `crates/app/src/app/voice/` — 12 个文件
      - `crates/app/src/app/voice/control/` — 1 个文件；小目录
      - `crates/app/src/app/voice/playback/` — 1 个文件；小目录
    - `crates/app/src/app/voice_window_inset/` — 5 个文件
    - `crates/app/src/app/wallpaper_actions/` — 2 个文件；小目录
  - `crates/app/src/assets/` — 1 个文件；小目录
  - `crates/app/src/bin/` — 4 个文件
  - `crates/app/src/canvas/` — 17 个文件
    - `crates/app/src/canvas/events/` — 1 个文件；小目录
  - `crates/app/src/cards/` — 38 个文件
    - `crates/app/src/cards/agent/` — 7 个文件
      - `crates/app/src/cards/agent/queue/` — 4 个文件
    - `crates/app/src/cards/browser/` — 5 个文件
      - `crates/app/src/cards/browser/card/` — 1 个文件；小目录
    - `crates/app/src/cards/intro/` — 2 个文件；小目录
    - `crates/app/src/cards/preview/` — 4 个文件
    - `crates/app/src/cards/snake/` — 1 个文件；小目录
    - `crates/app/src/cards/term/` — 14 个文件
      - `crates/app/src/cards/term/paint/` — 4 个文件
      - `crates/app/src/cards/term/view/` — 1 个文件；小目录
  - `crates/app/src/e2e_control/` — 5 个文件
  - `crates/app/src/effects/` — 7 个文件
    - `crates/app/src/effects/dispatch_receipt/` — 5 个文件
  - `crates/app/src/license_gate/` — 12 个文件
    - `crates/app/src/license_gate/embed/` — 3 个文件
    - `crates/app/src/license_gate/gate_root/` — 1 个文件；小目录
  - `crates/app/src/media/` — 5 个文件
  - `crates/app/src/mobile/` — 68 个文件
    - `crates/app/src/mobile/activity/` — 2 个文件；小目录
    - `crates/app/src/mobile/command/` — 1 个文件；小目录
    - `crates/app/src/mobile/controller/` — 2 个文件；小目录
    - `crates/app/src/mobile/push/` — 5 个文件
    - `crates/app/src/mobile/queue/` — 1 个文件；小目录
    - `crates/app/src/mobile/server/` — 26 个文件
      - `crates/app/src/mobile/server/connection/` — 23 个文件
        - `crates/app/src/mobile/server/connection/realtime/` — 4 个文件
    - `crates/app/src/mobile/workspace_live/` — 5 个文件
  - `crates/app/src/motion/` — 15 个文件
  - `crates/app/src/norma/` — 24 个文件
    - `crates/app/src/norma/endpoint/` — 12 个文件
      - `crates/app/src/norma/endpoint/mobile_approval/` — 1 个文件；小目录
    - `crates/app/src/norma/server/` — 7 个文件
  - `crates/app/src/performance_monitor/` — 2 个文件；小目录
  - `crates/app/src/preview/` — 9 个文件
  - `crates/app/src/settings/` — 151 个文件
    - `crates/app/src/settings/agent_hook_ingress/` — 2 个文件；小目录
    - `crates/app/src/settings/agents/` — 15 个文件
      - `crates/app/src/settings/agents/state/` — 6 个文件
      - `crates/app/src/settings/agents/view/` — 3 个文件
    - `crates/app/src/settings/general/` — 1 个文件；小目录
    - `crates/app/src/settings/mobile/` — 3 个文件
    - `crates/app/src/settings/navigation/` — 1 个文件；小目录
    - `crates/app/src/settings/norma/` — 9 个文件
      - `crates/app/src/settings/norma/persona/` — 2 个文件；小目录
    - `crates/app/src/settings/orchestration/` — 5 个文件
    - `crates/app/src/settings/typography/` — 1 个文件；小目录
    - `crates/app/src/settings/voice/` — 80 个文件
      - `crates/app/src/settings/voice/state/` — 27 个文件
        - `crates/app/src/settings/voice/state/values/` — 4 个文件
      - `crates/app/src/settings/voice/view/` — 46 个文件
        - `crates/app/src/settings/voice/view/doubao/` — 4 个文件
        - `crates/app/src/settings/voice/view/editable/` — 6 个文件
        - `crates/app/src/settings/voice/view/gpt_live/` — 2 个文件；小目录
        - `crates/app/src/settings/voice/view/provider/` — 2 个文件；小目录
        - `crates/app/src/settings/voice/view/setup/` — 3 个文件
        - `crates/app/src/settings/voice/view/tts_catalog/` — 6 个文件
          - `crates/app/src/settings/voice/view/tts_catalog/controller/` — 4 个文件
        - `crates/app/src/settings/voice/view/tts_provider/` — 6 个文件
          - `crates/app/src/settings/voice/view/tts_provider/controller/` — 2 个文件；小目录
          - `crates/app/src/settings/voice/view/tts_provider/picker/` — 1 个文件；小目录
    - `crates/app/src/settings/wallpaper/` — 13 个文件
  - `crates/app/src/shortcuts/` — 2 个文件；小目录
  - `crates/app/src/workspace/` — 122 个文件
    - `crates/app/src/workspace/clone_project/` — 5 个文件
    - `crates/app/src/workspace/desktop/` — 102 个文件
      - `crates/app/src/workspace/desktop/drawers/` — 51 个文件
        - `crates/app/src/workspace/desktop/drawers/content/` — 26 个文件
          - `crates/app/src/workspace/desktop/drawers/content/workbench/` — 16 个文件
        - `crates/app/src/workspace/desktop/drawers/float/` — 3 个文件
      - `crates/app/src/workspace/desktop/ideas/` — 5 个文件
    - `crates/app/src/workspace/flow/` — 3 个文件
    - `crates/app/src/workspace/windowing/` — 1 个文件；小目录

## 其他产品与文档目录

- `ios/` — 347 个文件
  - `ios/App/` — 106 个文件
    - `ios/App/Assets.xcassets/` — 13 个文件
    - `ios/App/CommandCenter/` — 22 个文件
  - `ios/DeviceTests/` — 29 个文件
  - `ios/Norma.xcodeproj/` — 4 个文件
    - `ios/Norma.xcodeproj/project.xcworkspace/` — 2 个文件
    - `ios/Norma.xcodeproj/xcshareddata/` — 1 个文件
  - `ios/PairingBridge/` — 2 个文件
    - `ios/PairingBridge/include/` — 2 个文件
  - `ios/Resources/` — 8 个文件
    - `ios/Resources/Avatar/` — 2 个文件
    - `ios/Resources/Motions/` — 4 个文件
  - `ios/Sources/` — 78 个文件
    - `ios/Sources/NormaCore/` — 78 个文件
  - `ios/Tests/` — 86 个文件
    - `ios/Tests/Fixtures/` — 1 个文件
    - `ios/Tests/NormaCoreTests/` — 63 个文件
    - `ios/Tests/PairingProbe/` — 22 个文件
  - `ios/UITests/` — 26 个文件
  - `ios/Widget/` — 2 个文件
  - `ios/scripts/` — 1 个文件
- `services/` — 45 个文件
  - `services/norma-cloud/` — 45 个文件
    - `services/norma-cloud/api/` — 1 个文件
    - `services/norma-cloud/cloud/` — 2 个文件
    - `services/norma-cloud/collaboration/` — 1 个文件
    - `services/norma-cloud/deploy/` — 3 个文件
    - `services/norma-cloud/push/` — 3 个文件
    - `services/norma-cloud/scripts/` — 3 个文件
    - `services/norma-cloud/tests/` — 17 个文件
    - `services/norma-cloud/text/` — 2 个文件
    - `services/norma-cloud/voice/` — 4 个文件
- `test/` — 92 个文件
  - `test/desktop-e2e/` — 92 个文件
    - `test/desktop-e2e/Fixtures/` — 9 个文件
    - `test/desktop-e2e/Scenarios/` — 39 个文件
    - `test/desktop-e2e/Sources/` — 14 个文件
    - `test/desktop-e2e/Tests/` — 1 个文件
    - `test/desktop-e2e/checks/` — 28 个文件
- `spikes/` — 132 个文件
  - `spikes/keygate-mvp/` — 13 个文件
    - `spikes/keygate-mvp/scripts/` — 7 个文件
    - `spikes/keygate-mvp/tests/` — 1 个文件
  - `spikes/norma-spatial-prototype/` — 119 个文件
    - `spikes/norma-spatial-prototype/.openai/` — 1 个文件
    - `spikes/norma-spatial-prototype/app/` — 10 个文件
    - `spikes/norma-spatial-prototype/components/` — 60 个文件
    - `spikes/norma-spatial-prototype/hooks/` — 2 个文件
    - `spikes/norma-spatial-prototype/lib/` — 17 个文件
    - `spikes/norma-spatial-prototype/public/` — 2 个文件
    - `spikes/norma-spatial-prototype/tests/` — 16 个文件
- `docs/` — 887 个文件
  - `docs/adr/` — 28 个文件
  - `docs/agents/` — 3 个文件
  - `docs/assets/` — 1 个文件
  - `docs/brand/` — 13 个文件
    - `docs/brand/assets/` — 10 个文件
  - `docs/design-docs/` — 160 个文件
    - `docs/design-docs/2026-07-27-agent-card-queue-ui/` — 2 个文件
    - `docs/design-docs/2026-08-04-ideas-vault-canvas-native/` — 5 个文件
    - `docs/design-docs/2026-09-29-desktop-workbench-concepts/` — 8 个文件
    - `docs/design-docs/2026-09-30-desktop-unified-scene/` — 1 个文件
    - `docs/design-docs/assets/` — 85 个文件
    - `docs/design-docs/core-ui-v2/` — 10 个文件
    - `docs/design-docs/ion-fireworks/` — 3 个文件
  - `docs/exec-plans/` — 75 个文件
    - `docs/exec-plans/archive/` — 1 个文件
    - `docs/exec-plans/evidence/` — 9 个文件
    - `docs/exec-plans/ios-terminal-acceptance-2026-10-05/` — 9 个文件
  - `docs/product-specs/` — 83 个文件
    - `docs/product-specs/assets/` — 6 个文件
    - `docs/product-specs/norma-ios-ideas-workspaces-2026-09-28/` — 6 个文件
    - `docs/product-specs/norma-ios-lockscreen-prototypes-2026-10-05/` — 8 个文件
    - `docs/product-specs/norma-ios-prototypes-2026-09-26/` — 14 个文件
  - `docs/prototypes/` — 180 个文件
    - `docs/prototypes/archive/` — 35 个文件
    - `docs/prototypes/assets/` — 25 个文件
    - `docs/prototypes/dispatch-effects/` — 1 个文件
    - `docs/prototypes/norma-assistant/` — 78 个文件
    - `docs/prototypes/norma-command-dock-2026-10-01/` — 26 个文件
    - `docs/prototypes/settings-effects/` — 1 个文件
    - `docs/prototypes/tests/` — 4 个文件
  - `docs/quality/` — 15 个文件
  - `docs/references/` — 19 个文件
    - `docs/references/assets/` — 4 个文件
  - `docs/research/` — 287 个文件
    - `docs/research/assets/` — 196 个文件
    - `docs/research/evidence/` — 44 个文件
  - `docs/screenshots/` — 1 个文件
  - `docs/verification/` — 17 个文件
    - `docs/verification/launchpad-hd-benchmark-data/` — 7 个文件

## 全部 1–2 文件 Rust 源码目录

下表包含独立 crate 的 src、平台适配、测试辅助及资源目录，不是删除清单。父模块行数仅指目录旁同名 Rust 文件；合计是物理行数估算，实际重组仍需处理声明、可见性与测试挂载。

| 目录 | 子树文件 | 同名父模块行数 | 合计行数 |
| --- | --- | ---: | ---: |
| `crates/agent/src/master/` | `crates/agent/src/master/delegation.rs`、`crates/agent/src/master/memory_session.rs` | 494 | 742 |
| `crates/agent/src/rig_tools/` | `crates/agent/src/rig_tools/gpt_live.rs` | 220 | 260 |
| `crates/app/src/app/agent_runtime/coordination/` | `crates/app/src/app/agent_runtime/coordination/task_result.rs` | 415 | 513 |
| `crates/app/src/app/agent_runtime/pending/` | `crates/app/src/app/agent_runtime/pending/realtime.rs` | 329 | 579 |
| `crates/app/src/app/bootstrap/visual_preview/` | `crates/app/src/app/bootstrap/visual_preview/working_snake.rs` | 339 | 366 |
| `crates/app/src/app/browser/address_input/` | `crates/app/src/app/browser/address_input/render.rs` | 444 | 637 |
| `crates/app/src/app/browser/store/` | `crates/app/src/app/browser/store/view.rs` | 395 | 527 |
| `crates/app/src/app/chrome/` | `crates/app/src/app/chrome/dock.rs`、`crates/app/src/app/chrome/top_bar.rs` | 329 | 1045 |
| `crates/app/src/app/doubao/plan/asr/` | `crates/app/src/app/doubao/plan/asr/listen.rs`、`crates/app/src/app/doubao/plan/asr/protocol.rs` | 327 | 564 |
| `crates/app/src/app/doubao/playback/` | `crates/app/src/app/doubao/playback/budget.rs` | 289 | 347 |
| `crates/app/src/app/e2e/visuals/` | `crates/app/src/app/e2e/visuals/preview.rs` | 239 | 293 |
| `crates/app/src/app/feature_guide/view/` | `crates/app/src/app/feature_guide/view/header.rs` | 443 | 514 |
| `crates/app/src/app/frame_loop/` | `crates/app/src/app/frame_loop/animation_driver.rs` | 426 | 663 |
| `crates/app/src/app/voice/control/` | `crates/app/src/app/voice/control/error.rs` | 355 | 381 |
| `crates/app/src/app/voice/playback/` | `crates/app/src/app/voice/playback/macos.rs` | 148 | 369 |
| `crates/app/src/app/wallpaper_actions/` | `crates/app/src/app/wallpaper_actions/assets.rs`、`crates/app/src/app/wallpaper_actions/sync.rs` | 123 | 545 |
| `crates/app/src/assets/` | `crates/app/src/assets/orb_frames.rs` | 493 | 575 |
| `crates/app/src/canvas/events/` | `crates/app/src/canvas/events/mouse_up.rs` | 486 | 577 |
| `crates/app/src/cards/browser/card/` | `crates/app/src/cards/browser/card/status.rs` | 475 | 533 |
| `crates/app/src/cards/intro/` | `crates/app/src/cards/intro/mod.rs`、`crates/app/src/cards/intro/paint.rs` | — | 331 |
| `crates/app/src/cards/snake/` | `crates/app/src/cards/snake/view.rs` | 290 | 652 |
| `crates/app/src/cards/term/view/` | `crates/app/src/cards/term/view/render.rs` | 488 | 608 |
| `crates/app/src/license_gate/gate_root/` | `crates/app/src/license_gate/gate_root/workbench.rs` | 467 | 537 |
| `crates/app/src/mobile/activity/` | `crates/app/src/mobile/activity/detail.rs`、`crates/app/src/mobile/activity/overview.rs` | 335 | 623 |
| `crates/app/src/mobile/command/` | `crates/app/src/mobile/command/validation.rs` | 368 | 490 |
| `crates/app/src/mobile/controller/` | `crates/app/src/mobile/controller/sources.rs`、`crates/app/src/mobile/controller/work.rs` | 438 | 795 |
| `crates/app/src/mobile/queue/` | `crates/app/src/mobile/queue/dispatch.rs` | 176 | 461 |
| `crates/app/src/norma/endpoint/mobile_approval/` | `crates/app/src/norma/endpoint/mobile_approval/native.rs` | 107 | 216 |
| `crates/app/src/performance_monitor/` | `crates/app/src/performance_monitor/macos.rs`、`crates/app/src/performance_monitor/recording.rs` | 348 | 678 |
| `crates/app/src/settings/agent_hook_ingress/` | `crates/app/src/settings/agent_hook_ingress/approval_wait.rs`、`crates/app/src/settings/agent_hook_ingress/http.rs` | 387 | 607 |
| `crates/app/src/settings/general/` | `crates/app/src/settings/general/persistence.rs` | 333 | 442 |
| `crates/app/src/settings/navigation/` | `crates/app/src/settings/navigation/brand.rs` | 423 | 508 |
| `crates/app/src/settings/norma/persona/` | `crates/app/src/settings/norma/persona/mod.rs`、`crates/app/src/settings/norma/persona/view.rs` | — | 630 |
| `crates/app/src/settings/typography/` | `crates/app/src/settings/typography/scale.rs` | 57 | 154 |
| `crates/app/src/settings/voice/view/gpt_live/` | `crates/app/src/settings/voice/view/gpt_live/device.rs`、`crates/app/src/settings/voice/view/gpt_live/diagnostics.rs` | 450 | 586 |
| `crates/app/src/settings/voice/view/provider/` | `crates/app/src/settings/voice/view/provider/api_key.rs`、`crates/app/src/settings/voice/view/provider/controller.rs` | 491 | 664 |
| `crates/app/src/settings/voice/view/tts_provider/controller/` | `crates/app/src/settings/voice/view/tts_provider/controller/activation.rs`、`crates/app/src/settings/voice/view/tts_provider/controller/work.rs` | 430 | 615 |
| `crates/app/src/settings/voice/view/tts_provider/picker/` | `crates/app/src/settings/voice/view/tts_provider/picker/provider_list.rs` | 427 | 553 |
| `crates/app/src/shortcuts/` | `crates/app/src/shortcuts/catalog.rs`、`crates/app/src/shortcuts/idea_assign.rs` | 432 | 1047 |
| `crates/app/src/workspace/windowing/` | `crates/app/src/workspace/windowing/e2e.rs` | 483 | 514 |
| `crates/asr/src/providers/cloud/azure/` | `crates/asr/src/providers/cloud/azure/test_support.rs` | 476 | 505 |
| `crates/asr/src/providers/local/` | `crates/asr/src/providers/local/mod.rs`、`crates/asr/src/providers/local/sherpa.rs` | — | 313 |
| `crates/audio/src/` | `crates/audio/src/lib.rs`、`crates/audio/src/pcm.rs` | — | 53 |
| `crates/live_voice/src/connection/` | `crates/live_voice/src/connection/output.rs`、`crates/live_voice/src/connection/transmit.rs` | 403 | 517 |
| `crates/live_voice/src/context/` | `crates/live_voice/src/context/history.rs`、`crates/live_voice/src/context/observed.rs` | 304 | 406 |
| `crates/live_voice/src/providers/` | `crates/live_voice/src/providers/mod.rs`、`crates/live_voice/src/providers/openai.rs` | — | 82 |
| `crates/live_voice/src/session/` | `crates/live_voice/src/session/command.rs` | 312 | 409 |
| `crates/mobile_pairing/src/` | `crates/mobile_pairing/src/ffi.rs`、`crates/mobile_pairing/src/lib.rs` | — | 195 |
| `crates/model_store/src/` | `crates/model_store/src/installation.rs`、`crates/model_store/src/lib.rs` | — | 408 |
| `crates/norma/src/bin/` | `crates/norma/src/bin/multisoul_agent_hook.rs`、`crates/norma/src/bin/norma.rs` | — | 23 |
| `crates/norma/src/codex/` | `crates/norma/src/codex/approvals.rs` | 260 | 578 |
| `crates/norma/src/orchestration/installer/` | `crates/norma/src/orchestration/installer/reconcile.rs`、`crates/norma/src/orchestration/installer/skill_component.rs` | 479 | 760 |
| `crates/runtime/src/ego/` | `crates/runtime/src/ego/transition.rs` | 402 | 591 |
| `crates/storage/src/live_operation_store/` | `crates/storage/src/live_operation_store/task_facts.rs` | 270 | 413 |
| `crates/storage/src/migrations/` | `crates/storage/src/migrations/desktop.rs`、`crates/storage/src/migrations/desktop_canvas.rs` | 492 | 756 |
| `crates/terminal/src/process_tree/` | `crates/terminal/src/process_tree/observation.rs` | 302 | 473 |
| `crates/tts/src/providers/doubao/` | `crates/tts/src/providers/doubao/voices.json` | 46 | 2203 |
| `crates/voice_sherpa/src/tts/` | `crates/voice_sherpa/src/tts/stream.rs` | 445 | 578 |

## 有针对性的收拢建议

- 可收拢：`crates/app/src/app/bootstrap/visual_preview/working_snake.rs` 的单个配置函数与父模块其他预览配置同类，可并入父文件。
- 可收拢：`crates/app/src/app/voice/control/error.rs` 的三个方法直接操作父模块状态，可与父文件的错误进入逻辑放在一起。
- 可收拢：`crates/agent/src/rig_tools/gpt_live.rs` 可并入父文件，保留其独立类型和 OpenAI schema 适配语义。
- 保留分离：`crates/app/src/settings/typography/scale.rs` 是零 GPUI 的字级模型；父文件负责 GPUI 样式适配。这是有意义的职责分工。
- shared audio、model_store、mobile_pairing 虽小，仍分别承载共享类型、安装原语和 iOS FFI，不能按文件数取消 crate。
- 许多单文件子目录是父模块接近 500 行后拆出的。可在必要时统一为目录内 mod.rs 加职责文件，但不要为了目录外观而扩大搬迁范围。
- App 的设置、语音、移动端、桌面工作空间和通用 app 子目录，需要按状态及生命周期所有权整理；压平目录不会自动解决职责分散。

独立复核已检查上述收拢候选及 typography 的分离理由；这些 Rust 模块的收拢建议尚未实施。

