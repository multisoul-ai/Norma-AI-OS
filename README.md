# Multisoul Desktop

macOS 空间工作台脚手架（垂直切片）。Live Node on Infinite Canvas，Terminal 为第一个 Live Node。

## gpui 来源（R1 验证结论）
- 采用：crates.io =0.2.2
- 验证日期：2026-06-20
- 备注：API 与 brief 完全吻合，无需额外 patch。构建需 macOS Metal Toolchain（`xcrun metal --version` 可验证）。`cargo build -p app` 在 Metal 就绪后零错误通过。`cargo run -p app` 弹出 1200×800 深色窗口（需图形环境，headless 下跳过）。

## 开发命令
- 运行：`cargo run -p app`
- 测试（域层）：`cargo test -p canvas -p terminal -p runtime`
