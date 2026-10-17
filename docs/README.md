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

## 历史材料

被替代的 Ideas Vault、Realtime Card Task 方案以及旧 Rust `e2e` 套件已从当前树移除。历史细节仍可从删除前的 Git 提交 `366284b4` 查阅。保留在 `docs/` 的旧规格和执行记录只解释当时的决策、命令与验收，不作为当前实施指令；新需求使用受影响 crate 的测试和真实 App 验收。

实验的逐轮日志和候选图片也可在上述提交中查阅。当前树保留仍被规格、脚本或报告引用的素材与汇总数据。
