# 脚手架验收记录

日期：2026-09-09。执行环境：macOS arm64，Temurin JDK 17、Android API 34。

## 已执行

- `npm run test:host`：2 项测试，含真实 loopback WebSocket 双客户端广播、非法选项拒绝、重连恢复与单任务决策隔离。
- `:core:test`：2 项 JVM 测试，消费与 Node 相同的 JSON fixture，验证乱序快照过滤、缺失版本拒绝与错误不污染会话。
- `:app:assembleDebug`：生成 arm64-v8a APK。
- `:app:lintDebug`：不关闭 lint、不创建忽略基线。保留固定 SDK/依赖版本和只支持 Quest ARM 的提示；这些是当前平台选择。
- `scripts/quest.sh doctor`：Java、Node、Android SDK 与 adb 可用。

首次 lint 发现 UI Set 的 `androidx.core:core:1.13.1-patched` 未发布依赖，已按照 Meta StarterSample 显式依赖 `core-ktx:1.13.1`，使运行与 lint/test classpath 对齐。没有跳过 lint。

## 尚未执行

`adb devices -l` 没有设备，因此未执行安装/启动、真实头显 Computer Use、手势与窗口舒适度测试。APK 编译成功不代表这些验收通过；应按 `headset-checklist.md` 补齐。

没有调用真实 Codex/Claude、麦克风、LLM 或仓库。此轮验收仅覆盖原生开发脚手架和明确标记的 mock 联调流程。
