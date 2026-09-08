# Norma Quest

Norma AI OS 的 Quest 3 原生客户端脚手架。**Agent、Codex / Claude CLI 和仓库始终运行在 Mac；Quest 负责空间呈现和交互。**

技术栈：Kotlin 2.1.0、Meta Spatial SDK 0.13.2、Jetpack Compose、Android API 34、AGP 8.11.1、Gradle 9.4.1、JDK 17。

## 开始开发

1. 安装 Android Studio Narwhal 2025.1.1 或更新版本，JDK 17、Node.js 22.13+。
2. 在 Android SDK Manager 安装 Android API 34、Build Tools 35.0.0、Platform Tools。
3. 配置 `JAVA_HOME`、`ANDROID_HOME`，或在本目录创建不提交的 `local.properties`，写入 `sdk.dir=/你的/Android/sdk`。
4. Android Studio 打开本目录，等待 Gradle Sync。空间窗口由代码创建，不需要 Spatial Editor。

```sh
# 当前目录 apps/quest，或独立 Norma-Quest 仓库根目录
bash scripts/quest.sh setup
bash scripts/quest.sh test
bash scripts/quest.sh build
```

在 Norma-AI-OS monorepo 根目录也可以直接执行同名命令：`bash scripts/quest.sh build`。

## 连接 Quest 与 Mac mock 服务

在 Quest 开启开发者模式，通过 USB 连接 Mac，并在头显中允许 USB 调试。

```sh
# 终端一：启动只监听 loopback 的 mock Mac 服务
bash scripts/quest.sh mock

# 终端二：构建、安装、转发端口、启动 Quest 应用
bash scripts/quest.sh run
```

在头显中点击「连接 Mac」，默认地址为 `ws://127.0.0.1:8787/session`。`run` 会执行 `adb reverse` 将此端口映射到 Mac。若有多台 Android 设备，设置 `ANDROID_SERIAL` 选择目标。自定义 `NORMA_QUEST_PORT` 时需自行同步端口转发与应用地址。

`mock` 仅提供两项模拟任务、终端样例和多个待决策事项。决定经 Mac mock 服务确认后广播给所有客户端；重连恢复同一个服务进程的最新状态。重启 mock 服务会重置数据。

## 命令

| 命令 | 作用 |
|---|---|
| `setup` | 安装锁定的 Node 依赖并检查工具链 |
| `doctor` | 检查 Java、Node、Android SDK 与 adb |
| `mock` | 启动 Mac mock 会话服务 |
| `test` | Node WebSocket 测试 + Kotlin JVM 测试 |
| `check` | 测试 + APK 构建 + Android lint |
| `build` | 生成 debug APK |
| `run` | 构建、安装、USB 端口转发并启动 |
| `logs` | 查看 AndroidRuntime / NormaQuest 日志 |

APK：`app/build/outputs/apk/debug/app-debug.apk`。CI 也会保留 debug APK 与测试报告。

## 项目结构

```text
app/          Quest 原生入口、空间面板、Compose UI、连接适配
core/         零 Android 的协议类型与会话状态
protocol/     共享 JSON fixture 与协议说明
dev-host/     Node mock Mac 服务及真实 WebSocket 集成测试
scripts/      独立工程和 monorepo 共用的开发命令
docs/         架构、真机验收与仓库同步约定
```

当前不包含真实 CLI 接入、ANSI 网格渲染、语音、媒体串流、生产配对和权限服务。release 构建禁止明文连接；不要把开发 mock 服务作为生产远程入口。

阅读 [架构与边界](docs/architecture.md)、[协议](protocol/README.md)、[真机验收](docs/headset-checklist.md)、[monorepo 同步](docs/repository.md)。

## 上游说明

SDK 与构建配置依据 [Meta 官方 StarterSample](https://github.com/meta-quest/Meta-Spatial-SDK-Samples/tree/main/StarterSample)；原生应用代码为本项目实现。Gradle wrapper 来自官方示例中的标准 Gradle wrapper，受 Gradle 的 Apache-2.0 许可证约束。Meta Spatial SDK 二进制受其官方 SDK 条款约束。本项目不复制示例场景、美术、字体或媒体资源。
