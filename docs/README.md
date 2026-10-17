# 文档入口

优先从当前规格进入，再按需要查设计、执行记录和决策背景。日期表示编写时间，不代表文档仍适用；每份文档开头的状态与其引用的后续决策共同决定适用范围。

| 需要了解 | 入口 | 用法 |
| --- | --- | --- |
| 产品边界与当前方向 | [项目上下文](../CONTEXT.md)、[产品规格](product-specs/) | 先核对状态与替代关系；桌面统一场景仍在实施中 |
| 系统结构 | [架构](../ARCHITECTURE.md)、[ADR](adr/) | ADR 保留当时决策及演进原因 |
| 界面规范 | [iOS 样式](design-docs/norma-ios-style-guide.md)、[菜单](design-docs/material-menus.md)、[交互反馈](quality/ui-interaction-feedback.md) | 现行规范优先于旧概念图 |
| 正在实施的工作 | [执行计划](exec-plans/) | 根据状态和关联 Issue 判断是否仍在执行 |
| 工程操作 | [工程手册](../CLAUDE.md)、[参考](references/)、[质量规则](quality/) | 命令以当前脚本和 Cargo 配置为准 |
| 可交互原型 | [原型入口](prototypes/README.md) | 浏览器 runner 随原型保存；归档版本只作历史参考 |
| 研究与验证记录 | [研究](research/)、[验证](verification/) | 历史测量不自动代表当前版本 |
| 品牌 | [品牌资料](brand/README.md) | 使用前核对当前视觉方向 |

<a id="history"></a>

## 历史材料

被替代的 Ideas Vault、Realtime Card Task 方案以及旧 Rust `e2e` 套件已从当前树移除。这些材料以及 2026-06-28 的 e2e 规格与实施计划，仍可从 Git 提交 `366284b4` 及其后的删除提交查阅。2026-10-17 的文档整合还删除了 `docs/superpowers/` 下的旧施工计划、与规格重复的执行步骤，以及已被统一云服务取代的部署记录。清单见 [文档整合计划](exec-plans/2026-10-17-docs-consolidation.md)。

保留在 `docs/` 的旧规格和执行记录只解释当时的决策、命令与验收，不作为当前实施指令；新需求使用受影响 crate 的测试和真实 App 验收。

仍留在文档根目录、且继续被脚本或规格引用的页面：

- [产品设计系统](design.md) 与评审页 [design.html](design.html)。`design.md` 自标为 V2 / Current Direction（2026-08-08）。
- [Core UI 交互原型](product-ui.html)，由 `scripts/check-product-ui-prototype.sh` 与 `scripts/render-product-ui-assets.sh` 使用。
- [派工粒子视觉基准](ion-fireworks.html)，由 `scripts/check-ion-fireworks-prototype.sh` 与派工粒子规格引用。

`exec-plans/archive/` 与 `prototypes/archive/` 仍被现行规格引用，本次保留。`research/assets/` 下的测量 JSON 本次未动。

实验的逐轮日志和候选图片也可在 `366284b4` 查阅。当前树保留仍被规格、脚本或报告引用的素材与汇总数据。
