# Windows 远程自助文档打印方案调研

> 调研日期：2026-08-18
> 证据范围：项目官方仓库、官方文档与厂商官方资料。

## 结论

若 **Windows 原生部署、网页远程提交、打印队列、现场自助释放与计费** 都是硬要求，未找到可与 Linux 上的 SavaPage 对等、成熟且完整开源的一体化项目。最合适的上线选择是闭源商业产品 **PaperCut NG/MF**；若必须开源，建议以 **Pridge** 作为 Windows 打印执行端并自建业务门户。

## 推荐顺序

| 方案 | Windows 原生 | Web/API 提交 | 自助释放/计费 | 开源 | 适合度 |
|---|---:|---:|---:|---:|---|
| PaperCut NG/MF | 是 | 是 | 是（Release Station、支付模块） | 否 | 生产首选 |
| Pridge Server + Client | 是（Client） | 是（自建/插件提交） | 需自建 | GPL-3.0 | 开源首选的执行层 |
| QZ Tray + 自建业务服务 | 是 | 浏览器到本机代理 | 需自建 | LGPL-2.1 | 单机/门店终端 |
| OpenPrint | Windows 可安装 | 是，REST/Web/SSE | 否 | MIT | 仅 PoC，不建议生产核心 |

## PaperCut NG/MF：Windows 上的完整生产方案（闭源）

PaperCut NG/MF 官方支持 Windows Server 2016/2019/2022/2025 以及 Windows 10/11，并要求安装在直接托管打印机的 Windows Print Server 上。[系统要求](https://www.papercut.com/products/ng/system-requirements/) · [Windows 安装](https://www.papercut.com/help/manuals/ng-mf/common/install-win/)

其 Web Print 可在同一 Windows Application Server 上作为服务运行；默认支持 PDF、图片上传，Office/XPS 需 Sandbox 模式。[Web Print 官方说明](https://www.papercut.com/help/manuals/ng-mf/applicationserver/web-print-setting-up-default-mode/)。产品定价包含 Release Station 与 Online Payment Gateway 加购项，适用于“付费后自助放行”。[定价与附加功能](https://www.papercut.com/products/ng/pricing/) · [支付网关安装](https://www.papercut.com/kb/Main/SettingupthePaymentGateway/)。它不是开源软件。

## Pridge：开源 Windows 打印执行端（推荐自建时使用）

[Pridge Client](https://github.com/sayehava/Pridge-Client) 是 GPL-3.0 的跨平台本地客户端，提供 Windows x64 安装包、无界面模式、客户端 token 认证、心跳、远端队列到本地打印机的映射，以及 `printing` / `printed` / `failed` 状态回报。Windows 打印走 `StartDocPrinter` RAW 或 PDFium/GDI 的系统驱动路径。[Client README](https://github.com/sayehava/Pridge-Client#readme)

配套 [Pridge Server](https://github.com/sayehava/Pridge-Server) 是 PHP + SQLite 的作业 broker。它适合将公网业务服务与门店/办公室的 Windows 打印机隔离：客户端主动向服务器拉取作业，不必把 Windows 打印服务端口暴露到公网。它没有内置的用户余额、支付、计价、取件 PIN/二维码或刷卡释放，需由业务服务实现。

## QZ Tray：浏览器直连 Windows 打印机的本地代理

[QZ Tray](https://github.com/qzind/tray) 是 LGPL-2.1 的浏览器到本地打印机/外设桥接器，官方提供 Windows 版及 Windows Service 运行方式；适合自研 Web 自助终端向同机或局域网 Windows 打印队列静默打印。[Windows Service 文档](https://qz.io/docs/windows-service) · [项目许可证](https://github.com/qzind/tray/blob/master/LICENSE.txt)

它不是服务器端打印管理系统：账号、订单、价格、支付、作业持有和到机释放均需自行实现。应为每台门店终端配可信签名证书并限制允许发起静默打印的网站，不能让任意网页调用本地代理。

## OpenPrint：可试验但尚不成熟

[OpenPrint](https://github.com/yahorse/openprint) 是 MIT 项目，提供 PDF 上传 REST API、网页控制台、作业查询/取消、SSE 状态和 TLS/Bearer Token。其仓库将版本标为 `0.1.0 Alpha`、几乎没有社区采用证据，且没有用户账户、计费或安全释放能力。官方说明的部署路径以 Linux/macOS 为主，因此即使它可在 Windows 安装，也只适合兼容性 PoC，不能作为无人值守收费打印的生产核心。[README](https://github.com/yahorse/openprint#readme)

## 建议架构

```text
用户网页/小程序 → 认证、上传、页数计价、支付、取件码与审计
                         ↓（已支付、待取件的受控作业）
                  Windows 门店节点：Pridge Client 或 QZ Tray
                         ↓
               Windows Print Spooler + 厂商驱动 → 物理打印机
```

若不接受自研自助流程，直接选 PaperCut NG/MF。若一定要开源，先针对一台真实 Windows 11/Server + 目标打印机，用 Pridge 进行“PDF 上传、断网重试、状态回传、付款后才下发”的小型 PoC；支付、取件码与文件安全是上层业务责任。
