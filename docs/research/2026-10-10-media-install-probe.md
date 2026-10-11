# 真实大文件安装域层实验

使用资源清单中最大的原视频 `wallpapers/1406443.mp4`（30,692,231 字节），调用当前 `media_store` 的公开接口安装、离线校验、修复和取消。九次独立运行全部通过，成功路径逐块比较原文件与安装结果，每一个字节一致。

这份实验只测本地文件流经过安装域层的行为与资源使用。它没有经过 HTTP、App 下载 manager 或 GPUI，不能作为真实 App 下载、界面响应或发布程序的性能验收。

## 实测表格

| 场景 | 重复次数 | 处理文件 MB | 安装或修复耗时中位数 | 离线 SHA 校验中位数 | 进程峰值 RSS | 最大读取缓冲 | 最终缓存 |
|---|---:|---:|---:|---:|---:|---:|---|
| 原文件首次安装 | 3 | 30.692 | 0.112 秒 | 0.095 秒 | 2.30 MiB | 64 KiB | 1 份完整原文件，无 partial |
| 单字节损坏后重新安装 | 3 | 30.692 | 0.211 秒 | 0.095 秒 | 2.30 MiB | 64 KiB | 损坏被拒绝，修复后逐字节一致 |
| 读取 1 MiB 后取消 | 3 | 1.049 | 0.005 秒 | 不适用 | 2.30 MiB | 64 KiB | 0 文件，未 Ready，无 partial |

MB 为十进制，MiB/KiB 为二进制。取消场景的原文件仍为 30.692 MB，表中 1.049 MB 是取消前实际读取的部分。峰值 RSS 为三次运行中的最大值，即 2,408,448 字节；不是整个 App 的内存占用。

## 方法与限制

- 每次使用独立、原先不存在的受管缓存目录，避免命中旧缓存。源文件可能命中操作系统文件缓存，不将本表作为冷磁盘吞吐排名。
- 安装时间在公开 `MediaStore::install` 调用前后计时，包含写入、长度/SHA 校验、`sync_all` 和原子提升；不包括探针进程启动或逐字节对照。
- 修复场景先翻转安装副本的第一个字节，确认长度未变且 `find_verified` 返回 `HashMismatch`，然后测重新安装；原文件不作修改。
- 取消场景在第十六次 64 KiB 读取后发出信号，确认显式 `Cancelled`、不可用状态及空目录。
- 峰值 RSS 使用 macOS `/usr/bin/time -l` 对每次探针进程独立测量。被链接的域库来自已通过完整测试的干净 debug target；探针也未启用 release 优化。因此本表不与生产 App 内存或发布性能混用。
- SDK 固定为 Xcode 26.5。首次在继承的 CLT SDK 27 环境链接失败，失败日志保留在 `build/package-size-reduction/media-install-probe-build-sdk-failed.log`，不计入成功实验。
- 单次读取上限 64 KiB 在每次调用中观测并断言；这不代表 HTTP 层、解码层或整个 App 的所有缓冲区同样只有 64 KiB。

## 复现与证据

[探针源码](assets/2026-10-10-package-size-implementation/media-install-probe.rs)、[完整 JSON](assets/2026-10-10-package-size-implementation/media-install-probe.json)、[逐次 CSV](assets/2026-10-10-package-size-implementation/media-install-probe.csv) 与原始 stdout/time 日志位于 `assets/2026-10-10-package-size-implementation/media-install-probe-raw/`。

先在独立 target 完成 `cargo test --locked --offline -p media_store`；用该 target 的唯一 `libmedia_store-*.rlib` 通过 `rustc --edition=2021 --extern media_store=<rlib> -L dependency=<target/debug/deps>` 链接探针。每次传入以下六个参数：源文件绝对路径、此前不存在的缓存绝对路径、清单相对路径、清单字节数、清单 SHA-256、`install` / `repair` / `cancel`。运行时用 `/usr/bin/time -l` 记录峰值 RSS。完整证据还记录了探针、域库及各域层源文件 SHA，便于核对使用的版本。重复运行与 CSV/JSON 汇总使用的 [Python runner](assets/2026-10-10-package-size-implementation/media-install-probe-runner.py) 需先复制到本 worktree 的 `build/package-size-reduction/`；该目录同时放置探针源码与已链接程序。

真实 App 的 HTTP 下载、并发、取消与退出、离线播放、作用范围和视觉状态继续按 [执行计划](../exec-plans/2026-10-10-package-size-reduction.md) 验收，本实验不关闭这些待办。

首轮结果另存为 `media-install-probe-initial.*` 和 `media-install-probe-initial-raw/`。随后给修复重装也加入观察 Reader、完整读取计数及 64 KiB 缓冲断言，重新执行全部九次；上表对应新结果，不混合两次测量。
