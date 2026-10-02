# 桌面统一场景验收证据索引

更新：2026-10-01。Goal active；用户处理后同一隔离 App 已恢复窗口和输入，旧启动阻塞解除。G0 未通过，G1–G5 未通过；本文不是最终验收报告。

## 真实基线

原基线为 PID 16449、SHA256 `7e9512d177ea9106da81e8d78dbebb42abd18285bfe49131c7a88e7a6478c386`，已退出。原二进制保留在 `g0/pre-timing-baseline-norma-os.bin`。隔离 `g1/Norma Layout Review.app` 已更新为有界帧诊断构建，PID 69255 经 Computer Use 核验可导航并有真实输入回显；复用前重新核实。[当前构建与环境](../../../artifacts/desktop-unified-scene/g0/frame-timing-review-build.json)。

两工作空间各两项目、每项目两张真实 Terminal，共八会话；同项目跨空间复用同一路径。最终目录各 `4 live`：[截图](../../../artifacts/desktop-unified-scene/g0/resumed-eight-live-directory.png)。四区域与进程：[构建和负载身份](../../../artifacts/desktop-unified-scene/g0/resumed-s1-build-and-processes.json)。

两路真实 PTY producer 各完成 900 秒、54,000 行、4,320,000 源字节，已自然退出；后续测量必须通过真实 UI 重新启动。[W2/A 结果](../../../artifacts/desktop-unified-scene/g0/fixtures/W2-A-1-producer.json)、[W3/B 结果](../../../artifacts/desktop-unified-scene/g0/fixtures/W3-B-1-producer.json)。

## CPU 占用基线

W3/A 展开且空闲，W2/A 与 W3/B 后台持续输出，静态壁纸开启；预热超过 10 秒，每轮 60 个 ps 样本。三轮采样期间未进行导航或屏幕采样。

| 轮次（UTC 起始） | 中位数 | P95 | 最大值 |
| --- | --- | --- | --- |
| 21:48:53 | 4.0% | 4.5% | 4.8% |
| 21:50:59 | 3.9% | 4.6% | 4.7% |
| 21:52:06 | 3.9% | 4.3% | 4.7% |

[汇总与原始结果路径](../../../artifacts/desktop-unified-scene/g0/performance/cpu-baseline-index.json)。机器：macOS 15.6.1（24G90）、Mac16,1、10 逻辑核、32GiB 内存。

这些百分比不是帧时间。原脚本的 CPU 占用门槛不等于本 Goal 的 16.67ms／1% 呈现目标；WindowServer 数据包含其他应用影响。CPU 提交、GPU 执行、呈现掉帧、输入延迟及视频壁纸尚未通过。

## 本轮修复与验证

CPU 采样脚本修复 BSD awk 的路径正则及 Bash 3 的空可选数组；修改三行，复用已有测试文件。[2 项失败证据](../../../artifacts/desktop-unified-scene/g0/cpu-runner-red.log) → [8 项通过](../../../artifacts/desktop-unified-scene/g0/cpu-runner-green.log)。

验证命令：`python3 -m unittest scripts.tests.test_cpu_performance_gate`、`bash -n scripts/run-cpu-performance-l3.sh`、`git diff --check`。未修改生产 Rust，未运行旧 e2e。三轮真实采样均有完整 summary；首次汇总失败的 CSV 不计入报告。

## 未通过项

1. [历史启动等待采样](../../../artifacts/desktop-unified-scene/g0/frame-timing-review-startup.sample.txt)已不代表当前状态，用户处理后同进程恢复；没有冷启动根因修复证据。[总览仍显示 session gone](../../../artifacts/desktop-unified-scene/g0/frame-timing-resumed-project-overview.png)、展开后恢复，待聚焦测试与修复。当前另有空工作空间，不是严格的 S1 目录。
2. [16 帧导航记录](../../../artifacts/desktop-unified-scene/g0/resumed-next-workspace-frames.json)首帧返回较晚，不能证明完整动画或实际 FPS。
3. Metal trace 因磁盘不足异常退出，[错误与清理记录](../../../artifacts/desktop-unified-scene/g0/performance/metal-trace-failure.json)；[Hitches 探针](../../../artifacts/desktop-unified-scene/g0/performance/hitches-capability-probe.log)亦保存失败。有界原生回调已取得，但完整动画区间与呈现无效值尚需处理，不能判正式性能通过。
4. 原生帧记录确认 2560×1600 像素、backing scale=2，即 1280×800 逻辑尺寸。完整导航／反向画面、视频与输入延迟仍缺证据。
5. 清理本 worktree 的重复诊断备份与一份可重建增量缓存后恢复写入。Agent 默认项目目录已修复并验证三种真实入口及 cwd；最新窗口外观与统一场景生产切片仍待实施；完整完成条件沿用[主计划](../../exec-plans/2026-09-30-desktop-unified-scene-goal.md)。

## 有界帧采集入口（真实探针，非正式 S1 性能结论）

GPUI 增加 `GPUI_FRAME_TIMINGS=1`，复用现有日志，最多 300 秒／36,000 次提交，按 PID／提交 ID 关联 CPU、GPU、实际呈现和像素尺寸。正常 CPU 占用采样必须关闭。口径与限制见 [GPUI 补丁说明](../../../vendor/gpui-0.2.2/NORMA_PATCH.md)。

[新增单测](../../../artifacts/desktop-unified-scene/g0/frame-timing-gpui-test.log) 1 项、[scene 回归](../../../artifacts/desktop-unified-scene/g0/frame-timing-scene-regression.log) 4 项通过，check／Release／签名与护栏通过。启动等待期间的[旧探针](../../../artifacts/desktop-unified-scene/g0/performance/bounded-frame-probe.json)为 0 条；用户处理后同进程获得 7,268 组 CPU／GPU／present 记录，无缺失或重复 ID，GPU 均 Completed。

[真实探针汇总](../../../artifacts/desktop-unified-scene/g0/performance/resumed-bounded-frame-summary.json)：draw＋submit 墙钟 P95 12.754ms，GPU P95 7.278ms；2,144 条 presentedTime 为零，不能当零延迟。该段未固定 S1 负载，未独立标注连续动画区间，不计算呈现错过率或判门槛通过。真实输入已[回显 project-b 路径](../../../artifacts/desktop-unified-scene/g0/frame-timing-resumed-input.png)，input-to-visible 延迟未测。
