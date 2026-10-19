# voice-live-acoustic

现状：用户已要求移除GPT-Live悬浮字幕。本场景在App启动和云端连接前返回ENV（退出码3），以下清单仅解释历史证据，不能作为新版本通过依据。真人验收见 `docs/quality/2026-10-18-gpt-live-user-acceptance.md`。

- `01-acoustic-ready.png`：真实隔离画布，Live通话及完整字幕可见；无Key明文。
- `02-acoustic-reply.png`：用户固定语句和Norma的“苹果香蕉”回复分别可见，不把用户原话当助手回复。
- `03-acoustic-closed.png`：已挂断，显示最终服务端用量；检查app.log资源释放无错误。
- 若只有`02-acoustic-input-missing.png`，如实记ENV，不能声称声学通过。固定激励可能被系统AEC当远端音频消除，需真人语句复验。
- 本场景不保存麦克风录音，不证明耳机、全部时延样本或长时声学稳定性。
