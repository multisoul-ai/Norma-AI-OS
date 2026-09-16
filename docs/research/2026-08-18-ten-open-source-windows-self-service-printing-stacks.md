# 十个可使用 Windows 本机打印机的开源打印项目/技术栈

> 调研日期：2026-08-18
> 筛选口径：项目本身或其明确支持的运行方式，能通过 Windows Print Spooler、已安装的 Windows 驱动或 Windows 本地打印代理输出到物理打印机。并非每一项都包含远程任务队列或自助收费；这类能力可由调用方扩展。

## 优先结论

1. **远程下发任务**：优先试 [Pridge](https://github.com/sayehava/Pridge-Client)。
2. **网页静默打印**：优先试 [QZ Tray](https://github.com/qzind/tray)；若只打印 RAW 小票/标签，可试 [Swift Print](https://makhijahemant.github.io/swift-print.html)。
3. **自建服务且以 PDF 为主**：使用 `Windows 后台服务 + SumatraPDF CLI` 最稳妥；在更严格的开源许可可接受时可用 Ghostscript。

| # | 项目/技术栈 | 许可证 | Windows 打印路径 | 适用边界 |
|---:|---|---|---|---|
| 1 | Pridge | GPL-3.0 | Client 拉取远程作业，再用 Windows RAW 或 GDI 系统驱动输出 | 远程打印首选；账务/取件需自建 |
| 2 | QZ Tray | LGPL-2.1 | 网页 JavaScript → 本机 QZ Tray → 已安装打印机 | 网页静默打印；非远程队列系统 |
| 3 | print-agent | GPL-3.0 | WebSocket 服务端 → Go Windows Agent → Windows Spooler | 远程热敏/ESC-POS；项目小，先 PoC |
| 4 | OpenPrint | MIT | Windows CLI/Bridge → IPP 打印机或桥接服务 | PDF REST API；Alpha，不用于生产核心 |
| 5 | Swift Print | MIT | localhost HTTP API → 原生 Windows RAW 打印 | 单机网页小票、标签和 ESC/POS |
| 6 | SumatraPDF CLI | GPL-3.0 | `-print-to` → Windows 已安装打印机 | 后端/守护进程打印 PDF/XPS；无队列 API |
| 7 | Ghostscript | AGPL-3.0 或商业许可 | `mswinpr2` 设备 → Windows 打印机驱动 | 高级 PDF/PS 渲染；注意 AGPL |
| 8 | `@lastapp/node-printer` | MIT | Node/Electron → Winspool | 自建 Node API/桌面代理；PDF 需先转 EMF |
| 9 | `pywin32` + `win32print` | PSF（源码含混合许可，按文件核对） | Python 直接调用 Windows Print Spooler | 自建 Python Windows Agent/服务 |
| 10 | `winrawprinter` | MIT | Node 原生 C++ addon → `StartDocPrinter`/RAW 或 XPS_PASS | ZPL/PCL/ESC-P 等 RAW 打印，不负责 PDF 渲染 |

## 项目说明

### 1. Pridge

[Pridge Client](https://github.com/sayehava/Pridge-Client) 提供 Windows x64 安装包和无界面模式。客户端以 token 认证连接服务器、轮询/保留作业、回报 `printing`、`printed`、`failed`；Windows 可将 RAW 内容交给 `StartDocPrinter`，或以 PDFium/GDI 走已安装的系统驱动。搭配 [Pridge Server](https://github.com/sayehava/Pridge-Server) 可形成最小远程打印链路。适合把支付、订单、权限放入自己的 Web API，再把“已放行”的任务交给 Pridge。

### 2. QZ Tray

[QZ Tray](https://github.com/qzind/tray) 是浏览器与本机打印机/外设之间的开源桥接器。它提供 Windows 包和 [Windows Service 模式](https://qz.io/docs/windows-service)，适合 React/Vue/普通网页静默打印 PDF、HTML、图片、ESC/POS/ZPL 等。应配置数字签名与允许来源；它本身不提供跨公网任务队列、账户或作业持久化。

### 3. print-agent

[jordankzf/print-agent](https://github.com/jordankzf/print-agent) 是 GPL-3.0 的“服务端 WebSocket → Windows Agent → Windows Spooler”实现，断线时服务器可保存任务并在重连后发送。其定位是远程办公室的热敏打印机。它很贴合远程下发，但仓库规模较小，建议只作为参考实现或先做实际打印机 PoC。

### 4. OpenPrint

[OpenPrint](https://github.com/yahorse/openprint) 提供 Windows 安装路径、REST 提交 PDF、作业查询/取消、SSE 状态与 Web Dashboard，能直接用 IPP 与支持 PDF 的打印机通讯。项目为 MIT，但 README 标为 `0.1.0 Alpha`，也缺少用户、权限与计价，不应作为无人值守生产系统的唯一核心。

### 5. Swift Print

[Swift Print](https://makhijahemant.github.io/swift-print.html) 是 Windows-only 的开源 localhost API：常驻托盘，列出已安装打印机，并以 API Key 接收 Base64 RAW 数据，转交原生 Windows 打印路径。适合小票、标签、ESC/POS；不适合通用 PDF 文档，且默认 token 必须替换、localhost API 不能被不可信网页调用。

### 6. SumatraPDF 命令行

[SumatraPDF](https://github.com/sumatrapdfreader/sumatrapdf) 是 Windows 开源阅读器。它官方支持 `-print-to`、`-print-to-default`、`-print-settings` 与 `-silent`，可从后台服务打印 PDF 到指定的 Windows 队列。[官方打印文档](https://github.com/sumatrapdfreader/sumatrapdf/blob/master/docs/md/Printing.md) 给出了无人值守示例。可配合自己的 HTTP 任务服务和文件清理策略使用。

### 7. Ghostscript

[Ghostscript/GhostPDL](https://github.com/ArtifexSoftware/ghostpdl) 支持 Windows，并可用 `mswinpr2` 输出设备调用 Windows 系统打印机，适合作为 PDF/PostScript 的转码/渲染执行器。官方提供 AGPL 与商业双许可；若你的闭源服务把它作为分发组件或整合进产品，应先进行许可评估。[官方下载与许可说明](https://ghostscript.com/download/)

### 8. `@lastapp/node-printer`

[`@lastapp/node-printer`](https://github.com/last-tech/node-printer) 是持续维护的 MIT Node/Electron 原生绑定，在 Windows 上通过 Winspool 枚举打印机、查询/控制作业，以及发送 RAW 或文件作业；并提供 Windows 预编译二进制。它适合把 Node/Express、NestJS 或 Electron 做成 Windows 本地打印代理。其 README 明确提醒：Windows 打印 PDF 时需先转换为 EMF。

### 9. `pywin32` + `win32print`

[pywin32](https://github.com/mhammond/pywin32) 是 Python 对 Windows API 的成熟封装，含 `win32print`。它适合实现 Windows Service：列出打印机、提交 RAW 作业、查询和管理队列。PyPI 对其标记为 Production/Stable，支持 Windows 和现代 Python；具体使用的源码文件应按仓库许可证核对。[PyPI 项目说明](https://pypi.org/project/pywin32/)

### 10. `winrawprinter`

[winrawprinter](https://github.com/steve-krisjanovs/winrawprinter) 是 MIT 的 Node 原生 C++ addon，面向 Windows 打印机提交 RAW 数据；针对 Type 4/XPS 驱动会使用 `XPS_PASS`。适合 ZPL、PCL、PostScript、ESC/P/ESC-POS 等已由打印机理解的字节流。它不渲染 PDF，也不实现远程调度。

## 共同安全边界

- 不直接把 Windows Print Spooler、RAW 9100 或本机代理端口暴露到公网。
- 远程任务优先采用客户端 outbound WebSocket/轮询，令牌与任务必须绑定到特定打印节点。
- 文件类任务先在服务端做格式、大小与恶意内容检查；PDF 渲染后再入 Windows 队列。
- API 接收 RAW 时，必须限制目标打印机、调用来源、命令格式与最大长度，防止任意设备命令或无限打印。
