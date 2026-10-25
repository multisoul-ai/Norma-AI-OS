# workspace-run-overview-titles

中文界面，一个项目、三个智能体（两个 `claude`、一个 `codex`）。假 CLI 经真实 Hook 接口上报：提交提示词后写出会话名，再上报 `Stop`。

- `01-overview-titles.png`：左栏运行概览里
  - 第一个 Claude 行：主标题是`Refactor permission gate`（transcript 里的 `ai-title`），第二行是首条提示词 `Alpha prompt`。
  - Codex 行：主标题是`Write self introduction`（`session_index.jsonl` 的 `thread_name`），第二行是 `Gamma prompt`。
  - 第二个 Claude 行没有会话名：主标题回退为 `Beta prompt`，不重复出现第二遍。
  - 三行的项目标签、状态芯片、行高与其他智能体行一致，标题单行截断、不与右侧芯片重叠。
