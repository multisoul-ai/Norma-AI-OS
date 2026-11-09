# Hermes

- 2026-11-09 本机：`~/.local/bin/hermes` 的目标 `/Users/alan/.hermes/hermes-agent/venv/bin/hermes` 不存在。`~/.hermes` 只有 config.yaml、plugins、skills，没有 agent 安装树。
- 因此不能做「把答案回传给 clarify」的实机。
- 上游 `tools/human_input_hooks.py`（NousResearch/hermes-agent main，调研 docs/research/2026-10-18-hermes-agent.md 与本次核对的 docstring）写明：`on_human_input_request` / `on_human_input_resolved` 是 observer，返回值忽略，失败不影响 prompt，payload 不携带人类输入的内容。
- Norma 现有 `hermes_plugin.py` 只转发 kind==approval，fail-open。
- 结论：不可行。不留问答 stub。
