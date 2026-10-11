# T04 工作空间目录与目标解析验收记录

日期：2026-10-11。关联：工作空间派工规格 issue #96 的 T04。状态：实现与范围内自动验证完成，待整体集成验收。

## 实施边界

`norma::workspace_dispatch` 定义授权后的完整目录、原文目标证据、结构化提及、纯解析结果和冻结目标。
`runtime::workspace_resolution` 纯解析完整空间候选，不读取数据库、不加载实例、不创建卡片、不发送输入。
`agent::workspace_plan` 从不可变来源原文构造目标 manifest，校验整组计划，并提供可信用户选择能力。
`storage::WorkspaceDirectoryStore` 后台只读持久目录；App 在读取结束后仅投影同来源空间的已保留实例。

目录包含全部授权画布的普通无定义卡、停用角色及未加载卡片。停用角色保留名称占用，不能成为有效派工候选。
同目录的多个画布共享持久项目键，但卡片仍使用 workspace／area／node／card UUID 精确寻址。
目录 revision 表示角色登记版本；`catalog_fingerprint` 另覆盖区域成员、项目键、显示名和路径，不能因角色 revision 相同而省略目录复核。

只读路径不迁移、不登记、不恢复 Document、不确保会话、不启动 PTY。缺数据库、schema、画布快照、卡片身份、登记一致性或配置启停证据时，返回明确缺口；不能把缺口当作空项目或唯一候选。
手机目录先按可信授权区域过滤项目、角色与失效键；接收前仍由共享服务重新校验授权。

## 目标与原文合同

- 有效主名和语音别名使用相同规范化键；失效旧键直接拒绝，未知或旧名称不能回退到项目唯一候选。
- 明确项目按全部授权画布计算零／一／多候选；不同项目同显示名、明确项目与卡片不匹配均要求补全。
- `FrozenWorkspaceTarget` 保存原持久卡片、项目及已存在的执行指纹。改名只改变展示；删除重建、停用、项目变化或执行指纹变化拒绝原绑定，不重选同名卡。
- `build_original_target_manifest()` 保守支持明确收件人、并列名单、重复“同时让／并让”、字面项目和结构化 `@`。引用正文不作为收件人，无法证明完整名单时要求用户补全或编辑。
- Discussion／Query、原文讨论／假设／否定／问题前缀及普通寒暄不能被模型 Execute 分类升级为执行。单独角色提及不是命令。
- `validate_workspace_task_plan()` 校验原文字节证据、正文及整组目标全集。遗漏、追加、重复目标或改变来源正文均不能接收；该 API 必须使用代码解析的预期结果，不能使用模型自报的目标。

## 可信用户补全

公开入口为 `confirm_workspace_target_selection()` 与 `apply_workspace_target_selections()`。
选择 token 字段私有，无 serde／Clone；仅可信 UI/controller 在用户明确选卡后创建，模型工具参数不能创建该能力。
token 深绑定原 manifest 与代码构造的发送 scope，不能跨消息、编辑正文或重复覆盖一个收件人。

未知／失效称呼可选择精确卡片，原明确项目仍受约束。项目多候选及同名项目歧义也可选择；选中的精确卡片同时确定原字面项目集合中的具体项目。
当前有效点名不能通过补全擅自改投。补全保留原文、字面证据和其他收件人，`original()` 保存审计，`effective()` 供重新解析和整组校验。
选卡不能解除讨论／假设／否定 veto 或无法证明完整目标名单的语法缺口；此时入口应保留正文，允许用户编辑成新的明确命令，作为新的可信来源操作继续。

## T06／T07 接线约束

`BoundWorkspaceOperation` 必须在初始完整目录、原 manifest 和原冻结目标校验成功时建立。
规划或提交期间发生改名，后续按原持久卡片复核；不能重新用失效旧名解析，也不能更新原执行指纹来掩盖会话替换。
最新完整目录 revision 可进入接收事务，但完整 catalog、授权和原身份仍须复核。
需要准备冷卡时只准备选中的原卡；准备后捕获真实执行指纹，不能为纯 Document 卡虚构运行实例。

App 生产入口 `read_workspace_dispatch_directory()` 显式接收冻结来源空间，SQLite 在后台执行；前台只查询同 AppDB 的 retained desktop sessions。
读取期间切到其他工作空间，结果仍属于原空间。多个实例 owner、原 UUID 缺失或替换、未登记卡、缺内存配置证据均标缺口。
仅现存 PTY 投影执行指纹；纯 Document 卡保持 `execution=None`，导航、焦点和输入不变。
捕获、复核与目录投影共用 route generation：已登记 `workspace_route_id` 的 SHA256 前八字节按 little-endian 转 u64，缺路由为 0。

## 验收映射

| 要求 | 证据 |
| --- | --- |
| 完整目录与授权覆盖缺口、非法角色范围 | `crates/norma/tests/workspace_directory.rs` |
| 有效名／语音别名、旧名失效、全区域零／一／多候选、目录缺口、项目歧义 | `crates/runtime/tests/workspace_resolution.rs` |
| 改名保持原绑定，停用／删除重建拒绝，不重选目标 | 同上 `frozen_binding_survives_rename_but_not_recreation_or_disable` |
| 原文目标全集、字面项目、引用不派工、讨论／寒暄 veto、重复第二命令与未知并列目标 | `crates/agent/tests/workspace_target_manifest.rs`，11 条 |
| 失效名选卡不恢复旧名、scope／原文深绑定、其他未知收件人保留、项目多候选／同名歧义 | `crates/agent/tests/workspace_target_completion.rs`，7 条 |
| 冷卡／无定义／停用卡可见，授权过滤、schema／文件不创建、缺快照／定义形成缺口 | `crates/storage/tests/workspace_directory_readonly.rs`，7 条 |
| catalog 变化不依赖角色 revision，已授权缺失区域不能静默消失 | 同上 `catalog_version_changes_without_role_revision_and_missing_area_is_not_empty` |
| 已有 PTY 精确投影且零输入／无新 PTY／焦点不变；替换原卡形成缺口；Document 不虚构执行实例 | `crates/app/tests/unit/workspace_directory_projection_tests.rs` |
| 真实隔离 AppDB 与 A/B retained sessions，读取 A 后切 B 仍只投影 A，所有卡零输入／无新 PTY，导航保持 B | 同上 `workspace_directory_reader_keeps_frozen_source_across_view_switch` |

## TDD 与验证记录

聚焦红灯已真实复现：缺原文／补全 API、项目选择不支持、生产 App reader 缺失，以及“同时让 Nobody”遗漏、`@Alan 讨论…` 越权、`@Alan 很不错` 被当成执行。
相应修复后，原文 manifest 11 条、可信补全 7 条与 App 目录投影 4 条均通过。
App 目录测试调用生产后台 reader，并检查所有卡片实际记录输入、Runtime 数量和导航；TestAppContext 仍不等于启动真实 `.app`。

最终范围验证已通过：

- `cargo test --quiet -p norma -p agent -p runtime -p storage`：732 passed，0 failed，4 个原有 ignored，exit 0。
- `cargo check -p app --quiet`：exit 0，无诊断。
- `cargo test -p app --lib workspace_directory`：生产目录投影 4 项通过，exit 0。
- fmt、diff、存储／性能边界、staged 源码行数／no-allow／指南行数护栏通过；提交使用原正常 pre-commit 检查。

持久 runner 日志与退出码位于 `/tmp/norma-workspace-dispatch/t04-final-domain.*`、`t04-final-app-check.*`、`t04-app-directory-green.*`，不把未完成的工具会话当成成功证据。
独立对抗式审查由主 Agent 在整体集成阶段协调。T04 没有新增页面或样式，不在此宣称 Dock、选择器、真实 Provider 完成证据或全空间进度汇报已验收。
真实 App／Computer Use、正式共享接收和输入副作用由 T06–T09 集成验证；录制 PTY 和只读目录测试不能替代完整派工与进度汇报闭环。
