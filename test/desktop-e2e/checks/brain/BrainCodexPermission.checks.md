# Codex 权限回放

- `codex-activity.png`：真实活动面板出现本次项目轮次，输入、权限、输出、结束有清晰计数。
- `codex-permission-expanded.png`：展开后权限拒绝可见，能读到被拒绝的程序或组合命令及拒绝原因。
- 结合同目录 `replay.jsonl` 与 `app.log` 核对：before 合法汇报误拒绝；after 合法汇报 CLI 返回 0 且 App 接收到文字。两侧非允许程序、复合命令均拒绝。
- 这是固定适配器权限回放，不是历史 Claude 额度错误轮次，也不代表在线模型的全部输出问题已解决。
