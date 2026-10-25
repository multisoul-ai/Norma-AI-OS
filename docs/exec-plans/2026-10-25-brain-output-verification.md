# 幕僚输出修复与额度错误验证

状态：实现与定向验收完成；历史 Claude 第 43 轮的具体丢失位置未能确定。

本次提交包含 Codex 命令封装兼容、ACP 轮次结束与会话身份、设置重启后的旧接收通道保留，以及它们依赖的幕僚活动存储和面板。Claude 额度专项新增测试和回放夹具，未重复修改已覆盖该路径的产品逻辑。画布效果、共享上下文和离线语音等并行任务不在本次提交内。

## 已验证行为

- Codex ACP 2.2.2 适配保存原始命令边界，合法单／双引号封装与含引号正文可执行；其它程序、复合命令、动态展开和未知封装继续拒绝。
- ACP 每轮关联 backend/session_id，失败与取消保留已收到正文，并生成唯一结束；进程内轮次不因服务重建而复用。
- 设置重启立即失效旧 token，旧状态通道排空前保留，迟到错误和权限拒绝仍归原轮，不覆盖新 host 状态。
- 幕僚活动经后台写入 SQLite，使用 launch/area/lane/turn 完整键，面板读回原正文、原因与结束状态。
- 可控额度回放在真实工作树 App 中得到 error/cancelled/error 三轮；重开后的 3 条轮次、16 条正文及第 3 轮复制全文完全一致。

## 验证证据

- Brain：79 单测、15 服务集成通过；Codex 安装版 bundle 专项单独通过。
- App：幕僚相关 58 项回归通过，包含 retire 后迟到额度错误与真实 SQLite 重开；App build/check 通过。
- 真实 App：Codex 前后同场景权限结果从全部拒绝变为合法汇报放行、两项危险命令仍拒绝；Claude 额度场景 5/5 步通过。
- 合并时保留正常 pre-commit，并在不含并行任务的独立工作树验证提交内容。

详细原件保留在开发工作区：

- `artifacts/brain-codex-20261025/acceptance.md`
- `artifacts/brain-codex-20261025/before-after-correlation.json`
- `artifacts/brain-claude-quota-20261025/acceptance.md`
- `artifacts/brain-claude-quota-20261025/correlation.json`
- `artifacts/brain-merge-20261025/`

这些本机证据不随源码提交上传；包含会话输入、SQLite 与截图。

## 结论边界

历史目标为 Claude session `d30307c0-4f0c-425a-a8f7-b8f2acea4680`、Main 第 43 轮。其完整输入含“未采纳提议”段落，Claude 本地历史明确记录 usage_limit_reached / rate_limit / 429；缺少当时 ACP stdout 和持久化活动，不能确定具体丢失位置，也不能把后续 Codex 相似输入当作同轮。

额度验收使用安装版 Claude ACP 0.94.0 consumer 加人工 SDK 迭代器与终止 result，不调用在线模型，不耗尽账户。真实 App 验证收到正文后设置取消、新轮与重开；退休之后完整迟到 Error 的精确时序由独立状态通道／SQLite 测试覆盖。

正式 App 未替换，发布构建及部署后验收待完成。speak=false，未验 TTS 合成或实际可听见播放；面板通过不等于播放通过。
