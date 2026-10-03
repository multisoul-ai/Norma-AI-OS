# Norma Master 分阶段实施票

日期：2026-10-03。状态：已生成七张施工票；第一张 MemoryStore 票已实施，后续票尚未实施。完整范围和验收标准保存在各 GitHub 票，本文件只作本地导航。存储接口与验收映射见 [MemoryStore 接入说明](../references/norma-memory-store.md)。

实施入口：[Norma Master：Mac 本体、Markdown 记忆与成长实施路线](https://github.com/multisoul-ai/Norma-AI-OS/issues/81)。

当前实施切片：[正常记忆读写与 Norma 设置页实施方案](2026-10-03-norma-memory-access-and-settings.md)。用户已确认先完成对话按需读取、明确记住时写入、记忆管理和 Voice 迁移；自动学习放到下一步。本方案覆盖后两张票的部分范围，不代表完整票已完成，也不要求先实现自动蒸馏才能交付设置页。

## 已确认方向

以 Mac 为主体，统一人格和个人偏好；个人对话与项目会话分开。项目默认隔离，同仓库工作树共享约定与通用经验、隔离分支/任务事实。允许自主学习，新增能力启用或扩权需用户确认。

记忆与做梦的实施基线按用户要求参考 AgentConnect 源码：主题 Markdown + 生成 MEMORY.md；回复后蒸馏；快照、完整 staging、差异校验、采纳与备份。自动采用等具体政策仍由现有决策票记录。

设置入口已确认：Settings 中新增 Norma 一级页，记忆设置与查看统一放入；现有语音设置整体迁入，包括麦克风、识别、声音、语音交互、模式、模型/Provider、Setup 与模型管理。复用原配置和凭据引用，旧入口定位到 Norma 对应分区。页面布局仍由既有原型票确认，范围已更新到设置页实施票。

## 施工顺序

| 顺序 | 实施票 | 前置 |
| --- | --- | --- |
| 1 | [Norma：Markdown 记忆库与可靠版本发布](https://github.com/multisoul-ai/Norma-AI-OS/issues/82) | 已实施存储基础，验收记录见接入说明 |
| 2 | [Norma：持久经历、记忆工具与回合后蒸馏](https://github.com/multisoul-ai/Norma-AI-OS/issues/83) | [Norma：Markdown 记忆库与可靠版本发布](https://github.com/multisoul-ai/Norma-AI-OS/issues/82)；[Norma 的记忆与做梦如何保持可信和项目隔离？](https://github.com/multisoul-ai/Norma-AI-OS/issues/78) |
| 3 | [Norma：独立设置页与统一人格配置](https://github.com/multisoul-ai/Norma-AI-OS/issues/84) | [Norma：持久经历、记忆工具与回合后蒸馏](https://github.com/multisoul-ai/Norma-AI-OS/issues/83)；[Norma 独立设置页如何呈现记忆、梦境与成长？](https://github.com/multisoul-ai/Norma-AI-OS/issues/80) |
| 4 | [Norma：手动 Dream、完整暂存库与差异采纳](https://github.com/multisoul-ai/Norma-AI-OS/issues/85) | [Norma：独立设置页与统一人格配置](https://github.com/multisoul-ai/Norma-AI-OS/issues/84)；[Norma：持久经历、记忆工具与回合后蒸馏](https://github.com/multisoul-ai/Norma-AI-OS/issues/83) |
| 5 | [Norma：Skill 成长候选、验证与版本回滚](https://github.com/multisoul-ai/Norma-AI-OS/issues/86) | [Norma：手动 Dream、完整暂存库与差异采纳](https://github.com/multisoul-ai/Norma-AI-OS/issues/85)；[Norma 的人格、主动性与自进化权限如何定义？](https://github.com/multisoul-ai/Norma-AI-OS/issues/79) |
| 6 | [Norma：自动做梦与主动关心策略](https://github.com/multisoul-ai/Norma-AI-OS/issues/87) | [Norma：手动 Dream、完整暂存库与差异采纳](https://github.com/multisoul-ai/Norma-AI-OS/issues/85)；[Norma：Skill 成长候选、验证与版本回滚](https://github.com/multisoul-ai/Norma-AI-OS/issues/86)；[Norma 的记忆与做梦如何保持可信和项目隔离？](https://github.com/multisoul-ai/Norma-AI-OS/issues/78)；[Norma 的人格、主动性与自进化权限如何定义？](https://github.com/multisoul-ai/Norma-AI-OS/issues/79) |
| 7 | [Norma：首个受控插件能力包](https://github.com/multisoul-ai/Norma-AI-OS/issues/88) | [Norma：Skill 成长候选、验证与版本回滚](https://github.com/multisoul-ai/Norma-AI-OS/issues/86)；[Norma 的人格、主动性与自进化权限如何定义？](https://github.com/multisoul-ai/Norma-AI-OS/issues/79) |

## 剩余讨论如何安排

- 自动记忆、私密经历、纠正/忘记和 Dream 自动采纳：沿用 [Norma 的记忆与做梦如何保持可信和项目隔离？](https://github.com/multisoul-ai/Norma-AI-OS/issues/78)，在记忆接入与自动成长票开始前对齐。
- 相处方式、主动提醒和 Skill 更新权限：沿用 [Norma 的人格、主动性与自进化权限如何定义？](https://github.com/multisoul-ai/Norma-AI-OS/issues/79)。首个插件的具体用途、来源和验收场景在插件票开工前记录。
- 页面先通过已有 [Norma 独立设置页如何呈现记忆、梦境与成长？](https://github.com/multisoul-ai/Norma-AI-OS/issues/80) 原型验证，再实施；不重复创建原型决策票。

第一张票只交付受管 Markdown 存储和可靠版本发布。已确认会话激活时加载目录、人工编辑通过设置页或导入生效、首版 Dream 人工采纳且冲突后重跑；这些接入行为在后续票实施。其余票有原生 blocked_by；前置满足、必要决策形成后，再标为可交给开发 Agent。

## 后续范围

手机连接同一个 Mac Norma 的目标保留。现有 iPhone 云端 Norma、手机历史与 Mac 记忆权威之间的兼容/迁移另行明确，不修改现有 iOS 实施票，也不自动导入旧私聊。只有具体需求证明后再考虑语义索引、复杂 rebase 或更大的插件体系。

## 验证要求

每票先聚焦失败测试，再实现，并按受影响 crate 运行 AGENTS.md 指定检查。测试遵守完整注释与带说明的正负断言。所有页面/视觉状态必须用 Computer Use 操作真实 .app 验收；不使用已禁用的 crates/e2e。文件发布、来源、真实工具回执与作用域隔离不得因分期省略。

建票时已检查链接、子票归属与原生依赖；本次实现以公共 MemoryStore 测试覆盖第一票验收，具体测试映射见接入说明。后续页面仍须真实 App 验收。
