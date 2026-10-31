# Codex 等待与回合结束

真实安装的 Codex、真实 Norma debug 窗口、生产 Hook/原生 relay/幕僚事件链路；本机 Responses 服务控制模型响应，幕僚 ACP 使用只记录输入的脚本。提前 Stop 明确为测试注入，不声称来自此版本 Codex 的自然 Hook。

| 图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `00-codex-ready.png` | 真实 Codex 版本、模型及提示符 | 假 CLI 横幅、认证或信任弹窗 |
| `01-api-wait.png` | 卡片运行中，当前回合尚无回复 | Done |
| `02-thinking.png` | 思考正文与运行中 | Done |
| `03-background-terminal-banner.png` | 原生 Waiting for background terminal 与运行中 | Done |
| `04-premature-stop-rejected.png` | 提前 Stop 后仍运行中 | 提前完成状态 |
| `05-final-api-wait.png` | 终端已结束、最终 API 未返回，卡片仍运行 | Done |
| `06-native-turn-completed.png` | 最终 CODEX_WAIT_COMPLETE 与 Done | 运行中 |
| `07-brain-completion.png` | 幕僚活动对应完成记录 | 提前 Stop sentinel 作为完成结果 |

各阶段 `*-motion-0.png` 至 `*-motion-3.png`：先实际查看确定卡片外缘 ROI，再比较排除终端正文/光标/计时器的边缘像素。等待阶段应持续变化；Done 阶段同一外缘应停止变化。仅卡片文字或终端 spinner 不能证明运行动画持续。

硬断言：每段等待结束前幕僚完成事件数为0；注入 Stop 的生产 endpoint 回执200；原生结束后恰好1次、包含最终 marker 且无提前 sentinel。视觉结论与动画 ROI/数值另存证据目录，不以 OCR 或截图存在替代看图。
