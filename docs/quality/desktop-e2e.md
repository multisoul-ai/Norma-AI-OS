# 桌面 E2E

真实 Norma 窗口上的用户流程由 `scripts/desktop-e2e.sh` 驱动。页面、布局和样式验收先走这条路径；场景表达不了的交互再降级 Computer Use。headless GPUI、单测和“截图文件存在”不能代替这条路径的结果。

## 跑什么

```bash
cargo build -p app
bash scripts/desktop-e2e.sh workspace
bash scripts/desktop-e2e.sh workspace-lifecycle --repeat 5
```

被测二进制是 `target/debug/norma-os`。`production-data` 构建会忽略 `NORMA_TEST_DATA_ROOT`，不要拿它做这套验收。驱动装在固定 bundle `target/desktop-workspace-e2e/NormaE2E.app`（标识 `ai.norma.desktop-e2e`），辅助功能和屏幕录制授权都挂在这个 bundle 上。

旧命令 `scripts/desktop-workspace-e2e.sh` 只转发到 `workspace-lifecycle`。

## 一次运行

1. 前置检查：未锁屏、辅助功能、屏幕录制、测试二进制存在。发现其他 Norma 只记录，不结束；`owned.json` 里上次由本脚手架留下的 PID，且可执行文件就是本次测试二进制时，才会被结束。
2. 用独立的 `NORMA_TEST_DATA_ROOT` 和 `NORMA_SOCKET` 启动，避免碰到用户数据和默认 socket。键鼠使用用户临时目录里的 `norma-desktop-e2e.lock`：多个 git worktree 会排队，不会同时点击。点击前确认该坐标最上面的窗口属于本次 PID；否则取消点击。
3. 场景步骤都是“动作 → 期望的后置状态 → 有限次重试”。点击前重取窗口 frame 并重新 OCR，先移动鼠标再按下。
4. 硬断言同时看画面文字、`desktop_workspaces` / `desktop_project_areas`，以及 `keep-me.txt`。
5. 证据写到 `artifacts/desktop-workspace-e2e/<时间戳>/`，含截图、`report.json` 和 `app.log`。该目录被仓库根 `.gitignore` 的 `artifacts/` 忽略。

退出码：`0` 通过，`1` 产品断言失败，`3` 环境失败。环境失败应停下来处理权限或锁屏，不要换一种输入方式绕过。

## 谁判什么

| 层 | 判据 |
| --- | --- |
| 画面 | OCR 的 present / absent，只负责复现点击和文字是否出现 |
| 数据 | 隔离库 `app.sqlite` 的工作空间名称和项目区行 |
| 文件 | 项目目录里的 `keep-me.txt` 在删除和重启后仍在 |
| 视觉 | Agent 按 `checks/<模块>/*.checks.md` 逐张看图，结论为符合、不符合或不确定 |

P0 不改产品代码，也没有 App 侧测试钩子。项目区通过停掉隔离进程后写入 SQLite 来准备。可访问性树和测试钩子留到 P1，需另行确认。

## 场景按模块放

场景源码在 `test/desktop-e2e/Scenarios/<模块>/`，视觉清单在 `test/desktop-e2e/checks/<模块>/`，夹具在 `test/desktop-e2e/Fixtures/<模块>/`。模块里有哪些场景，写在该目录的 `<模块>Module.swift`。`Scenarios/Catalog.swift` 只保留模块名单。边界说明在 `.agents/skills/norma-desktop-e2e/references/modules.md`。

`bash scripts/desktop-e2e.sh workspace` 跑完 workspace 模块；`bash scripts/desktop-e2e.sh workspace-lifecycle` 只跑这一条。

`workspace` 模块的 `workspace-lifecycle` 覆盖创建、目录列出两个工作空间、搜索框按名称过滤、重命名、取消删除、确认删除、项目目录保留、重启后不恢复已删除工作空间。每步都核对两张表。

新增场景、前置失败和分诊步骤见 `.agents/skills/norma-desktop-e2e/`。
