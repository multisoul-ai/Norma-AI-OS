# Norma 助手 · Google / Material 设计方向

日期：2026-10-04。五张静态概念图均由内置 `image_gen.imagegen` 生成，逐款独立提示词；完整生成及修订提示词见 [prompts.json](prompts.json)。

## 参考来源与转译

本轮通过公开网页读取核实了三份 Google 官方设计资料：

- [Ten things we know to be true](https://about.google/company-info/philosophy/)
- [Layout – Material Design 3](https://m3.material.io/foundations/layout/layout-overview/overview)
- [States – Material Design 3](https://m3.material.io/foundations/interaction/states/overview)

官方资料强调：以用户目标为中心、清晰简单、速度、稳定且可适配的布局骨架、区域与间距层级、一致的可识别交互状态，以及多种状态指示。

Norma 的具体交互提案：在画布旁保留可发现的添加、整理、撤销、缩放和停止入口；选中对象有边框、手柄或标签；回显识别文本和执行结果；提供键盘输入替代；按任务展开资料、编辑工具或终端。撤销模型、语音回执和视图切换是对官方原则的产品转译，不是对官方原文的引用。

## 五个任务视图

| 编号 | 视图 | 用途 | 图片 |
| --- | --- | --- | --- |
| 1 | 通用工作台 | 稳定导航、自由画布、直接操作和实时语音；适合作为默认界面。 | [图片](01-material-workbench.png) |
| 2 | 资料研究台 | 素材、阅读、对话各有位置，生成结论时能追溯来源。 | [图片](02-material-research.png) |
| 3 | 并排对比 | 让网页、文档或视频占据主屏，语音直接切换与比较。 | [图片](03-material-compare.png) |
| 4 | 内容编辑台 | 语音和鼠标共同编辑，选中对象后显示对应工具。 | [图片](04-material-compose.png) |
| 5 | 任务执行台 | 实时对话同时查看网页预览、终端与任务进度。 | [图片](05-material-execution.png) |

建议以通用工作台作为默认界面，其他四张作为研究、对比、编辑和执行任务时的视图参考。

## 检查情况

已逐张视觉检查内容层级、语音入口、可发现的主要操作、选中状态和执行回执。任务执行台已修正完成数与终端状态，使 2/3 已完成、最后一项验证进行中保持一致。五张 PNG 的头信息、尺寸、文件副本哈希已验证。

这些是视觉原型，截图中的网页、报告、任务和终端文字均为示例内容；没有执行对应业务操作或测试命令。本轮未修改现有 HTML 或 Rust。
