# 灵动岛跨空间定位与保留项目

- `01-live.png`：真实脚本Agent在原PTY运行，记录唯一PID/NODE。
- `02-cross-workspace.png`：工作空间名和运行数量摘要；头像、智能体名、Claude Provider、任务标题分列；步骤和计时在第二行，状态和定位在右侧，无空态。
- `03-located.png`：主窗口回到原工作空间和项目，镜头聚焦同一张卡片。
- `04-retained.png`：真实主窗口关闭后岛保留运行任务；不是未加载摘要。
- `05-reopened.png`：定位打开新的原生主窗口并复用原PTY，不新增Agent。

硬断言：原生主窗口号变化，Agent PID不变、进程存在、启动记录恰好一条。手机服务使用隔离回环端口并真实启用，需要 `cargo build -p app --features e2e-test-support`。
