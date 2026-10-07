# 2026-10-06 双端自测包

用户授权：重新本地打包并安装 Mac，发布 iOS，由用户进行蜂窝任务与断线恢复验收。

## 已交付

- Mac：0.1.73（20261006.131442），已替换 `/Applications/Norma OS.app` 并通过 Computer Use 启动。正常退出旧版后安装，旧包备份于 `/Users/alan/Documents/mybook/norma-ai-os/build/release-backups/local-test-20261006/Original installed Norma OS.app`。
- iOS：0.1.0（53），Apple 回读 `VALID / IN_BETA_TESTING`，加入原「Norma 内测」组，中文测试说明逐字核对。Build / Delivery ID：`1b72c9d2-358b-46fc-87cd-70b87ef9a472`。没有新增测试邀请或公开 App Store 上架。
- Mac 手机服务已开启，原 iPhone 配对保持「全部项目 · 控制 · 10 个项目 · 权限版本 1」。新版进程建立到 `47.251.242.24:443` 的出站连接。未修改数据目录或安装证书。

## 产物与验证

Mac DMG：`/Users/alan/Documents/mybook/norma-ai-os/build/Norma OS_0.1.73_aarch64.dmg`，197449962 字节（188.30 MiB），SHA-256 `dc1003dd2369f26ad4edf93d6a394f9e0318b1f5942fd371ecae05044393e1f9`。Developer ID 团队 `2BF8G9AN4L`；严格签名、DMG 布局与完整性、安装后的 4 个可执行文件哈希均通过。此本地自测包未公证，Gatekeeper 评估不通过且无 stapler 票据；未将其描述为正式公共分发包。按现行打包脚本保留 manifest 版本 0.1.73，更新 bundle 构建号。

iOS IPA：`/Users/alan/Documents/mybook/norma-ai-os/build/local-test-release-20261006/Norma_0.1.0_53.ipa`，91179046 字节，SHA-256 `d3e5c113d95f2bbf8cf3716ba314746cfcf2d1cda6db660f14f299e3a1cfc85a`。App 与 Widget 均为 build 53；严格签名、production APNs、禁止调试、无测试 bundle、无隔离网关配置、Apple 校验与单次上传通过。沿用既有相同加密用途与分发范围。

Swift 核心：XCTest 111 项（2 项条件跳过，零失败），Swift Testing 26 项通过。Rust 文件行数、禁止 allow、Agent 指南行数检查通过。Mac Release 构建通过。Mac 1197 项、iOS 195 项生产输入在构建前后没有漂移。真实 App 已观察到原 5 个工作空间、已开启手机服务与原 iPhone 配对。

发布原始证据、哈希清单及 Apple 回读：`/Users/alan/Documents/mybook/norma-ai-os/build/local-test-release-20261006`。iOS 版本在 `ios/project.yml` 从 52 更新为 53，通过 xcodegen 重新生成 Xcode 项目；没有提交或推送代码。

## 用户自测

1. 在 TestFlight 更新到 0.1.0（53），Mac 保持在线和唤醒。
2. 若手机仍显示「启用云端聊天」，先与 Mac 连接同一 Wi-Fi，在「Norma → 输入文字」点击启用；现行初次注册需通过已配对的 Mac 直连取得证明。已有云端凭据可直接继续。
3. 关闭手机 Wi-Fi，使用 4G，在工作空间群聊通过 @ 选择一张空闲 Agent 卡片，对隔离测试项目发送创建唯一标记文件的任务，检查真实文件和手机结果。
4. 执行中断开手机网络并恢复，检查原任务继续、回执与历史补齐、唯一标记只出现一次。结果未知时先查原回执，不重复派工。

蜂窝任务与断线恢复本轮由用户验收；构建、上传、TCP 连接和模拟器记录均不替代真机验收。iPhone 11 支持 4G，5G 尚未验证。
