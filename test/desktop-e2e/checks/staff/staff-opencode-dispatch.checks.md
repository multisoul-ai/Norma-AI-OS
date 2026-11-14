# OpenCode 幕僚投递

- 01-needs-you：只有一张 Worker，状态是需要你；有重新投递和在画布中查看。不是执行失败，也不是执行中。
- 02-running：重新投递后仍是同一张卡，状态为执行中，没有第二张 Worker。

无固定文件名的条目按 visual 请求中的 criterion id 查找原 PNG。不得仅凭 exit 0 或 fixture 日志判视觉通过。
