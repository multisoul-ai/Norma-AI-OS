# 临时 Coding Agent 构建缓存

`scripts/task-build-cache.py` 为临时任务登记独立 Cargo target，并在验收结束后定向回收。主开发和真实桌面 E2E 继续使用工作树的 `target/`。脚本依赖 Python 3、Cargo 和系统 `lsof`，支持 macOS/Linux 的文件锁。

## 一个任务的完整流程

在对应工作树中执行；任务 ID 使用字母、数字、点、下划线或短横线，以字母或数字开头，最长 80 字符。

```bash
python3 scripts/task-build-cache.py run settings-20261018 -- check -p app
python3 scripts/task-build-cache.py run settings-20261018 -- test -p runtime
```

`run` 只接受 `build/check/test/clippy`。同一工作树、同一任务 ID 复用缓存；不同工作树的同名任务分开存放。每次调用都等待 Cargo 退出并透传退出码，不因命令成功或失败自动删除缓存。

专属任务目录始终通过 `run` 使用，让所有构建参与同一任务锁；打印出的 target 路径用于定位与归档，不交给其他进程另行启动并发构建。完成全部验收后，先回收，再按代码保存情况移除工作树。

任务目录为 `~/.cache/norma-build/<工作树路径哈希>/<任务ID>/`，含：

- `task.json`：工作树和任务归属；回收后保留。
- `task.lock`：串行化该任务的构建与回收；回收后保留。
- `target/`：该任务的 Cargo 产物。
- `cleaning/`：显式回收中的暂存目录；清理失败时保留，重试 `finish` 处理。

脚本同时固定子进程的 `CARGO_TARGET_DIR` 和 `CARGO_BUILD_BUILD_DIR`，设置 `CARGO_INCREMENTAL=0`。这些设置只作用于本次 Cargo 及其子进程，不修改当前 shell、仓库配置或用户全局配置。

需要迁移到其他缓存卷时，可以在每次调用中设置相同的 `NORMA_TASK_CACHE_ROOT`。该目录必须位于当前工作树之外，且不能是工作树的祖先。已经登记的任务应继续使用原缓存根目录。

## 先验收，再回收

将需要长期保留的程序、日志、截图和报告存到 `target/` 之外，例如工作树的 `artifacts/<任务ID>/`。确认构建、测试、实际程序和验收均已结束，再执行：

```bash
# 只预览，不删除缓存或改变登记内容。
python3 scripts/task-build-cache.py finish settings-20261018

# 显式声明任务收尾，调用 Cargo 删除该任务的构建产物。
python3 scripts/task-build-cache.py finish settings-20261018 --apply
```

回收会先取得任务锁，非阻塞取得 Cargo profile 锁，并用 `lsof` 检查是否仍有进程使用缓存。两类锁都持有到清理子进程退出。无法证明空闲时保留缓存并返回失败。执行删除时先把私有 `target/` 改名为 `cleaning/`，再调用 `cargo clean`，避免直接删除一个新构建可能重新创建的原路径。

任务缓存不接受目录符号链接，包括显式 target triple 和 build-script `OUT_DIR` 下的目录；Cargo 输出和元数据文件的符号链接也会拒绝。仅允许解析后仍在本任务私有目录内的 native 库文件链接（`.a/.dylib/.so`），避免 dep-info 等写入重定向覆盖证据。检测到不符合约束的链接时返回失败，不调用 Cargo 构建或清理。

构建失败、暂停、等待验收时保留缓存。脚本没有 EXIT trap、定时任务或全盘扫描清理；调用 `finish --apply` 是任务执行者在完成全部验收后的显式收尾步骤。清理失败可重试同一个命令，后续 `run` 会拒绝绕过未完成的回收。

`INT/TERM` 会转发给该次 Cargo 的进程组，等待后返回中断状态；任务锁也传给 Cargo，因此 wrapper 被 `SIGKILL` 后，仍存活并持有描述符的 Cargo 不会被误判为已结束。遇到 busy 提示时等待任务退出，不删除锁文件，也不结束其他任务或用户 App。

## 真实桌面 E2E

外置任务构建完成后，先为 E2E 更新其固定使用的工作树程序：

```bash
CARGO_TARGET_DIR="$PWD/target" CARGO_BUILD_BUILD_DIR="$PWD/target" \
  cargo build -p app --target-dir "$PWD/target"
bash scripts/desktop-e2e.sh workspace
```

使用默认开发 features。`production-data` 不接受 E2E 的隔离数据根目录，不用于这套验收。

以下路径由现有 E2E 入口维护，任务回收脚本不接管：

| 内容 | 路径 |
|---|---|
| 被测 App | `target/debug/norma-os` |
| SwiftPM 驱动缓存 | `target/desktop-e2e-spm/` |
| TCC 授权关联的固定 bundle | `target/desktop-workspace-e2e/NormaE2E.app` |
| 日志、截图、SQLite、owned.json | `artifacts/desktop-workspace-e2e/` |

真实 GUI 前置检查、键鼠锁和失败分诊仍按 [desktop-e2e.md](../quality/desktop-e2e.md) 执行。脚本不修改 `TMPDIR`，不替换 E2E 锁，不启动 App、不签名、不移除工作树。现有任意命名的外置缓存没有登记证据，脚本不会认领或批量清理；这些目录需另行核对任务归属。

## 验证脚本

```bash
python3 scripts/test-task-build-cache.py
```

测试使用临时项目和真实 Cargo 编译/清理，覆盖归属、输出隔离、符号链接、并发、中断、回收失败恢复及 E2E 路径保留。E2E shell 入口使用 Swift/codesign/driver 夹具验证调用契约；它不等于真实 GUI 验收。
