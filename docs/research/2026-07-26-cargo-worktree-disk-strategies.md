# Cargo + Git worktree 磁盘占用：社区常见策略

日期：2026-07-26

## 结论

社区没有一个同时满足“最省磁盘、完全并行、零额外维护”的通用方案，实践通常分成两条路线：

1. **磁盘优先**：多个 worktree 共用一个 `CARGO_TARGET_DIR`，再定期运行 `cargo-sweep`。
2. **并行优先**：每个 worktree 保留独立 target，使用 `sccache` 降低冷启动成本，并定期删除已废弃 worktree 的 target。

共享 `CARGO_TARGET_DIR` 是历史最久、讨论最频繁的方案，但同一 profile 下 Cargo 会使用 `.cargo-lock`，并行构建会相互等待；不同 worktree 的本地 crate、最终二进制和 feature 组合也可能反复重建。

## 常见方案排序

### 1. 共享 `CARGO_TARGET_DIR`

Cargo 官方支持通过 `CARGO_TARGET_DIR`、`build.target-dir` 或 `--target-dir` 改变产物目录。社区常把同一仓库的多个 worktree 指向一个公共目录，以复用 fingerprint 相同的依赖产物。

优点：

- 最直接地消除 worktree 间的大量重复依赖产物。
- 无需额外工具。

缺点：

- 同一 profile 的并行 Cargo 命令会竞争目录锁。
- 最终二进制也共用目录，分支间可能互相覆盖或触发重建。
- 依赖硬编码 `./target` 的脚本或 crate 可能不兼容。

来源：

- [Cargo Build Cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)
- [Rust Forum: So many crates break when you set target-dir](https://users.rust-lang.org/t/so-many-crates-break-when-you-set-target-dir-in-cargo-config/75574)
- [Sharing Rust Build Cache：多 Agent worktree 实践](https://blog.howardjohn.info/posts/shared-rust-build/)

### 2. `cargo-sweep` / `cargo clean`

社区常用 `cargo clean` 立即释放空间，用 `cargo-sweep` 按时间、工具链或大小清理旧产物。Cargo 自带的全局缓存 GC 当前仍不清理 workspace 的 target 目录，因此需要显式维护。

来源：

- [cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)
- [cargo-sweep](https://docs.rs/crate/cargo-sweep/latest)
- [Rust Blog: Cargo cache cleaning](https://blog.rust-lang.org/2023/12/11/cargo-cache-cleaning/)

### 3. 独立 target + `sccache`

多 Agent 需要真正并行时，保留每个 worktree 的独立 target 更稳妥。`sccache` 可以复用可缓存的 rustc 结果，但它主要改善重编译时间，不会消除每个 target 中必须存在的链接产物。

注意：

- 不同 worktree 的绝对路径默认影响 cache key，需要配置 `SCCACHE_BASEDIRS`。
- 增量编译的 crate 不能进入 sccache；若希望缓存 workspace crate，需要评估关闭 incremental。
- `bin`、`dylib`、`cdylib` 和 `proc-macro` 等调用链接器的 crate 不能缓存。

来源：

- [sccache README](https://github.com/mozilla/sccache)
- [Cargo Build Cache：Shared cache](https://doc.rust-lang.org/cargo/reference/build-cache.html#shared-cache)

### 4. Cargo 1.91+ `build.build-dir`

Cargo 1.91 可以把中间产物与最终产物分离。Cargo 团队正在探索把 build-dir 默认移到 Cargo cache，并为未来的跨 workspace 缓存、更细粒度锁和自动清理铺路。

当前限制：

- Cargo 团队尚未正式背书多个 workspace 直接共享同一个 build-dir。
- profile 级 `.cargo-lock` 仍存在。
- 官方建议的集中布局使用 `{workspace-path-hash}` 隔离 workspace；这便于统一清理，但不会消除 worktree 间重复。

来源：

- [Cargo config: build.build-dir](https://doc.rust-lang.org/cargo/reference/config.html#buildbuild-dir)
- [Rust Blog: Build Dir Layout v2](https://blog.rust-lang.org/2026/03/13/call-for-testing-build-dir-layout-v2/)
- [Cargo #16147](https://github.com/rust-lang/cargo/issues/16147)

### 5. 硬链接 / APFS clone

近期多 Agent 实践中有人只复制或硬链接 crates.io/git 依赖产物，同时保持 workspace crate 独立，以兼顾并行和空间。该方案依赖自定义脚本、Cargo 内部布局和文件不可变假设，属于高级优化，不是社区默认做法。

来源：

- [Sharing Rust Build Cache](https://blog.howardjohn.info/posts/shared-rust-build/)

## 对 Norma OS 的含义

Norma OS 如果同一时间通常只运行一个 Cargo，社区传统方案是“仓库级共享 `CARGO_TARGET_DIR` + 定期 sweep”。如果多个 Agent 会并行跑 `cargo check/test`，更符合社区实践的是“独立 target + sccache + worktree 生命周期清理”，并通过减少 debug info、评估关闭 incremental 来控制单份体积。
