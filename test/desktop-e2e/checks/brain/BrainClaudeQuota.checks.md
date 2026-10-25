# Claude 额度错误

- quota-first-panel-identity.png、quota-final-panel-identity.png、quota-reopened-panel-identity.png：逐张核对目标轮次号及原始回放 session id。点击后截图若仍是旧展开行，必须标注不作为目标身份凭证；不能因为文件名是 final 就判定为新轮。以同次真实 UI 复制全文、完整数据库键和重开身份截图交叉核验，防止同文错误串轮假通过。
- quota-first-panel.png：额度正文、重置时间、rate_limit 与 stopReason=error 可读，没有永久进行中。
- quota-settings-restart.png：策略已改为质量优先；这是实际设置触发重启，不是直接写配置。
- quota-final-panel.png：新轮额度错误正文清楚，不被旧轮取消替换。
- quota-reopened-panel.png：重开真实工作树 App 后持久化错误仍可读。

数据硬断言：三轮唯一，停止原因 error / cancelled / error。中间轮只收到额度正文，故保存 cancelled，不声称收到尚未发出的终止错误。
全部额度输入来自模拟 SDK/ACP 回放，不实际耗尽额度；speak=false，不验语音。
