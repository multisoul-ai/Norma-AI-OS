# Norma Quest 开发边界

Quest 是空间客户端。Mac 持有 Agent、CLI、PTY、仓库、构建、凭据和任务的权威状态。

```text
Quest APK
  app：Spatial SDK 空间面板 / Compose UI / WebSocket 适配
    ↓
  core：纯 Kotlin 协议、快照版本处理（JVM 单测，不依赖 Android）
    ⇅ JSON over WebSocket
Mac
  dev-host：仅供脚手架联调的 Node mock 服务
  后续 Rust adapter：连接现有 agent / terminal / runtime
```

## 本次范围

- 可构建的 Quest 3 Android APK；三张独立空间面板；默认 passthrough。
- 面板直接通过 SDK 创建，UI 使用 Compose 与 Horizon 的空间主题。
- 连接、断开、错误提示、任务状态、终端样例、多项决策及确认反馈。
- 双客户端广播、重连快照、过期快照过滤、协议版本拒绝的测试。
- 独立 Gradle 工程、锁定版本、Gradle wrapper、Node lockfile、CI 和开发命令。

当前终端是 **mock transcript 阅读视图**，不是 ANSI grid 渲染器。没有启动真实 CLI，没有录音/ASR/TTS，没有远程桌面串流，也没有生产配对服务。不要把这些后续能力标记为已完成。

## Mac 接入方式

生产实现应在 Mac 壳层/独立服务中增加网络适配器，消费现有 `terminal` 的中性快照和 `agent` 的类型事件，并通过 `runtime` 的输入路由执行用户意图。不要把网络或 Quest UI 反向放入 `canvas/terminal`，也不要将 GPUI 引入 Quest 工程。

v1 开发协议先使用完整快照建立端到端闭环。真实终端接入时增加带序号的 grid delta、resize、input 与断线恢复；不能直接每帧传整屏截图，也不能让终端流量阻塞决策指令。语音/视频使用单独媒体通道。

开发服务固定绑定 `127.0.0.1`，Quest 通过 `adb reverse tcp:8787 tcp:8787` 连接。拒绝带浏览器 Origin 的升级请求，不提供真实命令执行入口。正式发布前需实现配对、设备身份、WSS 和权限校验；release manifest 禁止明文流量。不要将此 mock 服务开放到局域网或公网作为生产桥接。

## 设计与验收

参考已加载的 `apple-design`：内容优先、状态可见、系统字体、圆角与克制的半透明背景；没有循环装饰动效。使用标准 Compose 控件提供按下、焦点、禁用反馈；真正的手部和控制器行为以 Quest SDK 为准。

保留代码创建的三张窗口用于验证，未宣称是最终工作区布局。玻璃折射、自定义 shader、窗口舒适距离、字体视角大小、手势误触率和稳定 90 Hz 都需要 Quest 真机验证。

自动证据：Node WebSocket 集成测试；纯 Kotlin 协议/状态测试；APK 构建与 Android lint。真实头显验收：运行 `scripts/quest.sh run` 后执行 `docs/headset-checklist.md`，记录设备、系统版本和 OVR Metrics。普通浏览器、JVM 测试不能替代此验收。

## 官方依据

- [Meta Spatial SDK 官方示例](https://github.com/meta-quest/Meta-Spatial-SDK-Samples/tree/main/StarterSample)
- [面板创建与坐标约定](https://developers.meta.com/horizon/documentation/spatial-sdk/spatial-sdk-panel-tutorial/)
- [面板合成层](https://developers.meta.com/horizon/documentation/spatial-sdk/spatial-sdk-2dpanel-layers/)
- [UI Set](https://developers.meta.com/horizon/documentation/spatial-sdk/spatial-sdk-ui-overview/)
