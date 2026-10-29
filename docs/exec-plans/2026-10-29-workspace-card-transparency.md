# 工作空间与项目卡片透明度统一

用户要求：⌘⇧↑工作空间总览、进入工作空间后的项目卡片，与左右抽屉采用相同的60%不透明度（40%透明）。基于 `f382a807`，在独立工作树实施，主工作区保留其他任务现场。

## 实施与测试映射

1. 先补失败测试：检查真实工作空间卡片、项目卡片根元素的背景alpha与抽屉一致；嵌套项目预览不得再次覆盖背景。
2. 抽屉、工作空间与独立项目卡片共用60%alpha；嵌套预览透明，保留边框、标题和节点内容。按下状态保留反馈但不恢复接近不透明的底色。
3. 扩展现有 `workspace-home-preview` 场景，在Sapphire Night/Graphite和两张壁纸下保存工作空间目录、项目总览、悬停/按下/键盘选中及抽屉对照图。
4. 跑聚焦App测试、`cargo check -p app`、debug构建、行数/no-allow/性能护栏及该E2E；只看本次材质相关图。保持原快捷键和布局，已有无关验收证据继续复用。

## 设计与验收

- 已读 apple-design、emil-design-eng、gpui-styling/testing、norma-desktop-e2e；采用材质层级与可读性原则，不新增动效，不把整个卡片（含文字）设为半透明。
- 60%对应 `153/255=0.6`。独立两类卡片与抽屉一致；嵌套预览不再造成84%或99.4%的重复遮盖。
- 默认、悬停、按下、焦点和选中状态仍可辨；方向键与⌘↓仍导航至正确目标；项目数量、终端示意数量和布局保持一致。
- 远端当前可选主题为六个深色主题；真实E2E选其中两种并配两张壁纸，旧浅色仅作单测兼容夹具，不声称真实浅色主题验收。
- 当前磁盘约3GiB，复用现有Cargo缓存与固定E2E路径，不清理共享target或其他任务缓存。日志与截图存入artifacts，不操作用户数据库，不使用Computer Use。

## 本轮结果

- 默认工作空间与独立项目卡片共用抽屉的153/255背景alpha；嵌套项目预览移除重复背景。工作空间hover/pressed为8%/12%轻状态层，按下不再切回94%实心底色；文字根元素opacity仍为1。
- 失败测试实际读到三种生产元素的0.92156863背景alpha，红灯0通过/1失败；修复后包含抽屉材质、主题、键盘和目录导航的聚焦测试10通过/0失败（3.30s）。
- `cargo check -p app`通过（15.67s）；debug构建首次因磁盘满失败。核对query-cache中的工作树路径、修改时间和lsof后，仅回收本轮红绿单测生成的两份增量会话缓存（清单在 `incremental-cleanup.json`），保留二进制/日志/其他任务缓存，未运行共享target的cargo clean。重试debug构建通过（1m43s）。
- max-file-lines、no-allow、performance护栏及diff空白检查均通过。i18n仍报告既有Effects空值及GPT-Live硬编码，本轮没有新增产品文案。上游设置页重复Effects匹配警告保留。
- `bash scripts/desktop-e2e.sh workspace-home-preview`退出3，0/1，3.90s；初始化阶段为ENV：`E2E frame barrier: window unavailable`。尚未进入卡片页面，无截图，未完成视觉验收。遵循E2E技能停止后续验收，未用Computer Use替代。
- 本次未跑release/全仓测试；未启动新的验收预览，因为真实E2E尚未通过。保留用户App及两个原有预览进程。
- 用户随后授权提交代码。本次提交位于独立分支 `fix/workspace-card-transparency`，仅包含这次材质调整、定向测试、E2E场景与本计划；运行正常提交钩子，不推送。主工作区的其他任务内容保持原样。命令和失败报告归档到 `artifacts/workspace-card-transparency-2026-10-29/`。
