# 远程自助打印开源项目调研

> 调研日期：2026-08-18
> 范围：面向纸张/文档打印的自托管开源软件；额外列出 3D 打印的独立候选，避免把两类需求混淆。
> 证据原则：只采用项目官方仓库、项目官方文档或 OpenPrinting/CUPS 官方文档；功能以文档明确记载为准，不以第三方测评、论坛或搜索摘要推断。

## 结论

**有，而且最接近“远程上传 + 自助选机/取件 + 用户权限 + 计费”的成熟开源方案是 [SavaPage](https://www.savapage.org/)。** 它是一个完整的 Open Print Portal：用户可在 Web 上传文件、预览和路由到真实打印机；支持私有 Web 队列、受认证后的安全释放、用户/组/共享账户计费、充值支付插件、POS 与 Print Site（自助打印点）Web 应用。其源代码使用 AGPLv3+，需要接受网络服务场景下的 copyleft 义务。[官方自由软件说明](https://www.savapage.org/docs/manual/ch-freepriv.html) · [官方功能页](https://www.savapage.org/?from=explinks.com)

若第一期只需“网页上传并远程下发到一两台打印机”，可选 [hanxi/cups-web](https://github.com/hanxi/cups-web)：MIT、中文界面、CUPS 容器化、多用户和打印记录都有，但没有内置余额、计价、支付或现场取件认证。若需要自建专用产品/API，可用 [CUPS](https://github.com/OpenPrinting/cups) 作为稳定的 IPP/队列内核；它提供远程 IPP、队列/作业管理、基础按用户配额和页面日志，但不提供终端用户上传门户或支付。

## 筛选维度

| 项目 | 网页远程提交 | 队列/状态 | 身份与权限 | 计费/余额/投币 | 自助终端/现场释放 | 推荐定位 |
|---|---|---|---|---|---|---|
| **SavaPage** | 是，Web Print 上传 | 是，私有 Web 队列、打印路由 | 是，用户角色与认证 | **是**，Pay-per-Print、账户/充值/支付插件、POS | **是**，Print Site、刷卡安全释放 | 完整自助打印平台首选 |
| **cups-web** | 是，浏览器上传、预览和下发 CUPS | 是，保存打印记录和状态 | 是，`admin`/`user` | 否（需二开） | 否（需二开） | 小型场景快速落地 |
| **CUPS** | IPP 客户端/API 可提交；内置 Web 为管理界面 | **是** | 系统认证、操作策略；可限制用户 | 基础页数/大小配额及页日志，非支付 | 否 | 底层打印服务/API 基座 |
| **PyKota** | 否，只有只读 Web 管理界面 | 经 CUPS 队列 | 有用户/组配额概念 | 账号余额和配额扣减，**非在线支付** | 否 | 旧 CUPS 部署补充计费，不建议新项目主用 |

## 1. SavaPage：完整候选（首选）

- **开源与部署**：官方说明确认其为 AGPLv3+ 自由/开源软件；官方 GitLab 的 [server 源码](https://gitlab.com/savapage/savapage-server) 与 [构建仓库](https://gitlab.com/savapage/savapage-make/-/tree/develop) 均可获得。后者的许可证说明也明确为 AGPLv3+。注意：现有 `savapage-server` GitLab 项目页标为 archived，官方 Issue Roadmap 记录了迁往 Forgejo 的计划；采购/生产前应先确认当前发布包、源代码镜像与维护支持渠道。[开源说明](https://www.savapage.org/docs/manual/ch-freepriv.html) · [迁移路线图](https://issues.savapage.org/roadmap_page.php)
- **网页远程提交**：`Web Print` 是无驱动文件传输打印，用户在 User Web App 上传文件；启用后映射到保留队列 `/webprint`，可配置单作业大小、允许来源 IP、拖拽上传，以及文件转换器。[Web Print 管理配置](https://www.savapage.org/docs/manual/ch-webapp-admin-options.html) · [上传流程](https://www.savapage.org/docs/manual/ch-webapp-user-main-upload.html)
- **队列与远程操控**：官方描述为中央打印门户，文档先聚合到私人的预览/队列，再从 Web App 路由到已安装的真实代理打印机；用户可预览、调整并删除页面后打印。它同时支持 IPP、AirPrint、Android Print、邮件和上传收集文档。[产品简介](https://www.savapage.org/docs/manual/ch-intro.html) · [官网能力页](https://www.savapage.org/?from=explinks.com)
- **权限、计费与支付**：官方明确提供按人、组或共享账户扣费的 Pay-per-Print；官网列出信用卡、PayPal、SEPA 等支付插件可向内部账户转入余额。用户手册还包含 POS Web App、Payment Web App、Vouchers（凭券）与财务设置章节。[产品简介](https://www.savapage.org/docs/manual/ch-intro.html) · [官网支付能力](https://www.savapage.org/?from=explinks.com) · [手册目录](https://www.savapage.org/docs/manual/)
- **自助终端与安全取件**：`Print Site Web App` 专用于自助打印地点；典型流程是授权用户选择打印机、作业保持，随后在打印机旁认证后释放。`Direct Print Release` 可要求在打印机旁刷卡，构成 pull-print 方案。官方把 Print Site 标为 *Feature Preview*，所以这部分应该先做现场 PoC，而非把它当作完全成熟承诺。[Print Site 文档](https://www.savapage.org/docs/manual/ch-webapp-printsite.html) · [刷卡释放文档](https://www.savapage.org/docs/manual/ch-webapp-user-main-print.html)
- **适合的产品形态**：校园、图书馆、共享办公、打印店/自助打印柜等多用户、多台打印机、需要账本与取件保密的场景。建议优先验证：目标打印机驱动/IPP 兼容性、支付网关在目标地区的可用性、卡读器型号、AGPL 合规方案及公网部署的身份/网络安全。

## 2. CUPS Web：轻量网页上传门户

[hanxi/cups-web](https://github.com/hanxi/cups-web) 是基于 CUPS 的 MIT 许可证 Web 管理工具，仓库明确支持浏览器上传、多用户管理和打印记录追踪。它可以处理 PDF、图片、Office、OFD 与文本，支持预览、份数、单双面、彩色、纸张、页码等选项，并在确认后落库、下发至 CUPS。[README：项目与许可证](https://github.com/hanxi/cups-web#readme) · [打印流程](https://github.com/hanxi/cups-web#%E6%89%93%E5%8D%B0%E6%B5%81%E7%A8%8B)

- 有 `admin`/`user` 两种角色，管理端可管理用户，并可按用户名和时间筛选记录；会保存文件、页数、份数、单双面/彩色和状态。[用户与权限](https://github.com/hanxi/cups-web#%E7%94%A8%E6%88%B7%E4%B8%8E%E6%9D%83%E9%99%90)
- 支持 Docker 部署，Web 端口与 CUPS 管理端口可分别暴露；项目要求把 CUPS 打印机设为 Shared 才能被 Web 端发现。[部署说明](https://github.com/hanxi/cups-web#docker-%E9%83%A8%E7%BD%B2)
- **缺口**：官方 README 未声明余额、按页计价、支付/投币、验证码/刷卡释放或多租户隔离。它适合家庭、小办公室或作为自研支付/订单/取件模块前的“远程提交层”，不适合直接充当无人值守收费系统。

## 3. CUPS：IPP、队列与基础配额的底层基座

[OpenPrinting CUPS](https://github.com/OpenPrinting/cups) 是标准化开源打印系统，Apache-2.0（带例外）许可。它适合作为任意自助打印方案的打印服务层，而不是完整业务门户。

- CUPS scheduler 是 HTTP/IPP 服务器，管理打印机、类、作业、订阅与通知；CUPS 的 IPP 实现公开了 `Print-Job`、`Create-Job`、`Send-Document`、`Get-Jobs`、`Cancel-Job`、`Hold-Job`、`Release-Job` 等操作，适合由自建 Web 后端调用。[架构说明](https://www.cups.org/doc/spec-design.html) · [IPP 操作清单](https://www.cups.org/doc/spec-ipp)
- 有管理 Web UI，可管理打印机、类和作业；支持 IPP Everywhere/AirPrint 和网络/USB 打印机。[官方仓库 README](https://github.com/OpenPrinting/cups#readme)
- 自带按每个用户统计的页数/作业大小配额，以及 `page_log` 页面日志；但每台打印机只有一套统一限制，不能按用户单独配置例外，也不能直接查询用户当前配额。因此它不等于“可收费自助平台”。[官方 Accounting 文档](https://openprinting.github.io/cups/doc/accounting.html)

**实现建议**：若业务需要自定义的订单、支付、核销码和收银机，采用“自有 Web/支付服务 → CUPS IPP → 网络打印机”的分层最可控。支付成功后应由服务端创建或释放 CUPS 作业；不要把未认证的 CUPS `:631` 管理界面直接暴露到公网。

## 4. PyKota：老牌 CUPS 计费/配额扩展（谨慎）

[PyKota](https://github.com/Jeparre/pykota) 是 GPLv2+ 的 CUPS/LPRng 配额与打印计费软件。它可通过硬件页计数、软件页数分析或墨水覆盖率估算，扣减用户账户余额或增加其配额使用量；其 `cupspykota` 后端可包裹 CUPS 实际设备 URI。[官方 README](https://github.com/Jeparre/pykota#readme)

- 官方功能页明确有 accounting 与 quota enforcement，并列出只读 Web Interface。[官方功能页](https://www.pykota.com/software/pykota/features)
- **不推荐新建项目以它为核心**：仓库 README 标注的版权/版本历史主要停留在 2007，依赖的 Python/系统组件年代久远；并且它不提供现代自助上传、移动端、在线支付或刷卡释放。适合只能维护已有 CUPS+PyKota 系统时的参考或迁移过渡。

## 5. 自研门户时的接入组件

以下项目不是自助打印产品，但能降低“网页/小程序 → 自建业务服务 → 本地打印机”的实现成本。

- [pycups](https://github.com/OpenPrinting/pycups) 是 CUPS 的 Python API 封装（GPL-2.0），依赖运行中的 CUPS。它已暴露提交文件、查询作业、查询属性、取消作业、启停打印机等接口，适合把“上传 → 创建作业 → 查询状态 → 仅本人取消”封成业务 API。[官方 README](https://github.com/OpenPrinting/pycups#readme) · [提交作业 API](https://github.com/OpenPrinting/pycups/blob/master/cupsconnection.c#L5480-L5505) · [作业 API](https://github.com/OpenPrinting/pycups/blob/master/cupsconnection.c#L4898-L4945)
- [ipp-usb](https://github.com/OpenPrinting/ipp-usb) 是 BSD-2-Clause 的 USB 接入层：将支持 IPP-over-USB 的设备转为 HTTP 反向代理，同时可服务 IPP 打印、eSCL 扫描和设备 Web 管理。它不管理队列、用户或计费，适合放在打印节点上让 CUPS 发现/转发。默认仅 loopback 监听；改为全网暴露时应评估网络风险。[官方 README 与暴露说明](https://github.com/OpenPrinting/ipp-usb#avahi-notes-exposing-printer-to-localhost)

可行的自研分层：

```text
网页 / 小程序 / 自助终端
        │ 认证、订单、支付、取件码
        ▼
业务 API（文件校验、页数/价格计算、审计）
        │ IPP 或 pycups
        ▼
CUPS（队列、状态、策略、基础配额）
        │ IPP；若本机 USB 则可经 ipp-usb
        ▼
物理打印机
```

业务服务需要自行完成登录授权、恶意文件/格式/大小校验、文件生命周期、订单幂等、余额账本和“付款后释放”；不要把 CUPS 管理权限交给普通提交用户。CUPS 官方还说明远程共享与管理默认未启用，开放会增加未授权访问与 DoS 风险，应以 TLS、可信网段/防火墙、VPN 或受认证的反向代理保护，而不是把 `631` 端口裸露公网。[CUPS 安全文档](https://openprinting.github.io/cups/doc/security.html) · [远程共享文档](https://openprinting.github.io/cups/doc/sharing.html) · [操作策略](https://openprinting.github.io/cups/doc/policies.html)

## 6. 若“打印机”指 3D 打印机：另选 OctoPrint / OctoFarm

3D 打印是 G-code 传输、机床状态与摄像头控制问题，不应与文档打印的 CUPS/IPP 方案混用。

- [OctoPrint](https://github.com/OctoPrint/OctoPrint) 提供单台 3D 打印机的 Web 控制和插件生态（项目官方仓库为 AGPL-3.0）。
- [OctoFarm](https://github.com/OctoFarm/OctoFarm) 是 AGPL-3.0 的多 OctoPrint 实例控制台，可统一监控/管理多个实例、文件系统、历史和日志。[官方仓库](https://github.com/OctoFarm/OctoFarm#readme)
- 这两个项目没有在官方材料中呈现出与 SavaPage 对等的用户余额、支付/投币与公众自助取件模型；它们适合创客空间/生产农场的设备操控后台，不适合直接做文档打印收费柜。

## 不纳入推荐的发现

- [FBK Pullprinting](https://github.com/stfbk/pullprinting) 是一个概念上很贴近“安全取件”的学术/原型项目：它以用户认证与靠近打印机为释放条件，并使用 OpenID Connect 与电子身份证二次认证。**不宜新建部署**：官方 README 的安装链路依赖已停止的 Google Cloud Print 与其 connector，要求 Android NFC/eID 和特定 OAuth2/身份认证基础设施；仓库页面也未列出许可证文件或 SPDX 许可证。因此可借鉴 pull-print 的认证释放思路，不能作为当前可上线的开源依赖。[官方仓库 README](https://github.com/stfbk/pullprinting#readme)
- [OpenPrint](https://github.com/yahorse/openprint) 提供 MIT 的 HTTP/REST、拖拽 PDF Web dashboard、实时队列、打印机发现与直接 IPP；其官方 README 未声明认证或计费能力，且项目规模/生产验证仍需自行评估。可关注为轻量 API/协议实验，不作为首选生产平台。[官方 README](https://github.com/yahorse/openprint#readme)
- [PrintRelay](https://github.com/schappim/print-relay) 的架构很贴合“云端 API 经 WebSocket 到本地客户端，再调用 CUPS”的跨网络需求，并有作业状态与 API key；但其许可证含“不得提供竞争性托管服务”条款，**不符合 OSI 开源定义的常规判断**。所以不把它列入“开源项目”推荐，只可作为架构参考。[项目架构与 API](https://github.com/schappim/print-relay#architecture) · [许可证说明](https://github.com/schappim/print-relay#license)

## 落地建议

1. **要尽快做收费自助打印 MVP**：先部署 SavaPage + 一台 IPP/CUPS 可用的真实打印机，验证“上传 → 价格显示 → 支付/余额 → 到机认证释放”闭环；Print Site 处于预览状态，必须做现场测试。
2. **只需内部远程打印**：选 cups-web；若需要已有身份系统/订单系统，直接在 CUPS IPP 前加自建服务，避免把收费逻辑硬改进 CUPS。
3. **必须跨公网控制门店/家庭打印机**：把打印节点通过 VPN、反向代理或受认证的 outbound agent 接入中心服务；CUPS 本身负责 IPP/队列，不应直接裸露在公网。对公网开放 SavaPage 的 Internet Print 时，官方也明确提示 Web App 会变为公网可访问，需额外保护访问。[Internet Print 提示](https://www.savapage.org/docs/manual/ch-webapp-admin-options.html)
