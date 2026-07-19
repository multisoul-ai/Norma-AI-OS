# 默认日志不记录完整用户与模型内容

我们决定让默认关键日志只包含指令 TraceId、稳定 ID、阶段、状态、计数和错误类型，不写入完整语音识别文本、任务正文、模型回复或 Terminal 内容。用户显式开启“完整诊断内容”后，这些可见内容才能在 `DEBUG` 级别记录；内部 `<think>`、API Key 和 Authorization Header 在任何模式下都不记录。PCM chunk、fingerprint 和其他高频 TTS 细节归入 `TRACE`，避免在 `INFO` 中淹没可用 TraceId 定位的任务状态链。
