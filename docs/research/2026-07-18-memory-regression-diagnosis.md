# Norma OS 内存回归诊断报告

- 日期：2026-07-18；2026-07-19 追加 `0.1.40` 安装包复测
- 状态：旧的 DisplayLink 主泄漏已修住；最新包的核心问题已进一步定位为 Agent 全量恢复与 PTY 进程树退出失效叠加，另有 TTS 双实例残留
- 现象：程序内存占用从过去低于 1 GiB，突然上升到约 3 GiB
- 当前结论：旧包的主进程回归来自 TTS 双实例与 GPUI DisplayLink 泄漏；`0.1.40` 已压住 DisplayLink，但会把所有持久 Agent 卡片恢复为完整 Codex/MCP 进程树，且关闭窗口、删除卡片或切换 runtime 时只向 PTY 直接子进程发送一次 `SIGHUP`，不等待、不升级强杀、也不清理后代。于是已离开的 workspace 进程树继续存活并与当前卡片叠加，形成最新包约 3 GiB 级占用。

## 1. 执行摘要

本次问题不是单一提交造成的单点泄漏，而是两个问题叠加：

1. `b041ab3a` 为 Agent 任务完成播报新增了一套独立、持久缓存的 TTS client。它与 Dialogue 已有的 TTS client 并存，在当前 Kokoro INT8 模型下可能保留两份 ONNX 模型运行时，显著抬高常驻内存。
2. 当前钉死的 `gpui = 0.2.2` 在 macOS 上停止 `CVDisplayLink` 后使用 `mem::forget` 避免释放；窗口显示周期又会重复创建 DisplayLink。Terminal 光标、语音窗口等持续动画不断触发重绘，放大了这一原生资源泄漏。

当前运行实例实测 physical footprint 约 1.80 GiB、峰值约 1.86 GiB。此次诊断没有完整复现用户观察到的 3 GiB 瞬时峰值，但已经稳定复现“超过原先 1 GiB”这一回归，并发现了可以继续增长到更高占用的结构性路径。

优先修复顺序：

1. 合并 Dialogue 与 Agent announcement 的 TTS 模型所有权，进程内只保留一个串行 TTS worker/session。
2. 修补或升级 GPUI 的 DisplayLink 生命周期，确保旧 DisplayLink 被正确释放。
3. 收紧 Terminal 光标和语音动画的持续重绘条件。
4. 增加真实 macOS App 的内存回归测试和原生对象计数护栏。

## 2. 诊断范围和限制

本报告分析的是 2026-07-18 20:01 左右启动、位于 `/Applications/Norma OS.app` 的真实应用进程。

限制：

- 没有使用带 `MallocStackLogging=1` 的受控重启，因此不能为每一块 malloc 内存取得完整分配栈。
- Heap 检查会短暂挂起进程，可能干扰 DisplayLink 的精确增长速率，因此报告只使用对象数量级和源码生命周期作为泄漏证据，不给出“每分钟泄漏多少 MiB”的推断。
- 没有完整复现 3 GiB 瞬时值，所以不能断言用户看到的每一部分内存都来自同一条路径。

## 3. 可重复的红灯反馈回路

以下检查直接使用 macOS physical footprint，而不是 `ps` 的 RSS：

```bash
PID=32327
for run in 1 2 3; do
  mib=$(/usr/bin/footprint "$PID" 2>/dev/null | awk '/phys_footprint:/ {print $2; exit}')
  if test "$mib" -ge 1024; then
    echo "RED run=$run phys_footprint=${mib}MiB threshold=1024MiB"
  else
    echo "GREEN run=$run phys_footprint=${mib}MiB threshold=1024MiB"
  fi
done
```

三次结果均为 RED，约 1806 MiB，稳定超过过去 1 GiB 的基线要求。

注意：Activity Monitor 主要反映进程 physical footprint。`ps` RSS 可能只有几百 MiB，不能据此否定 Activity Monitor 中 1.8～3 GiB 的观察。

## 4. 当前进程的内存构成

| 指标 | 实测结果 |
|---|---:|
| Physical footprint | 约 1.80 GiB |
| Peak physical footprint | 约 1.86 GiB |
| Heap 分配 | 约 1.53 GiB |
| `IOAccelerator` / GPU | 约 433 MiB |
| `CVDisplayLink` | 约 3,120 个 |
| `NSMachPort` | 约 3,137 个 |
| `CFRunLoopSource` | 约 3,139 个 |
| `onnxruntime::InferenceSession` | 2 个 |

`footprint` 中较大的区域包括：

- `MALLOC_MEDIUM`：约 487 MiB
- `IOAccelerator`：约 433 MiB
- `MALLOC_LARGE_REUSABLE`：约 367 MiB
- `MALLOC_LARGE`：约 239 MiB
- `MALLOC_NANO`：约 121 MiB
- `MALLOC_SMALL`：约 82 MiB

这说明当前异常既包括模型/普通堆内存，也包括图形和原生显示资源，并不是单纯的 Rust `Vec` 或 SQLite 数据膨胀。

## 5. 根因一：进程内存在两套独立的常驻 TTS cache

### 5.1 已有 Dialogue TTS cache

Dialogue 语音回复会从 `self.tts_client` 取出已有 client；若不存在则创建；合成完成后再把 client 放回字段中，长期驻留：

- `crates/app/src/app/dialogue/reply_stream.rs`
- 关键入口：`start_next_speech_synthesis`

### 5.2 新增 Agent announcement TTS cache

提交 `b041ab3a`（`feat: announce agent task completion with local TTS`）新增了第二套缓存：

- `crates/app/src/app/agent_announcements.rs`
- `crates/app/src/app/agent_announcements/playback.rs`
- 字段：`agent_announcement_playback.tts_client`

该 client 同样在合成后放回状态中，只有配置变化等特定条件才会丢弃。它没有与 Dialogue 共用模型所有权。

### 5.3 为什么这个提交高度可疑

当前设置为：

- `voice.agent_task_announcements_enabled = true`
- `voice.tts.model.id = kokoro-int8-multi-lang-v1_1`

Heap 中同时存在两个 `onnxruntime::InferenceSession`，与两套独立 TTS client 的代码结构高度吻合。由于本次进程没有用 malloc stack logging 启动，这一映射属于高置信度归因，而不是逐对象分配栈证明。

仓库已有本地 TTS 基准显示：

- Kokoro v1.1 INT8 单实例峰值 RSS：约 494 MiB
- Matcha zh-en 单实例峰值 RSS：约 284 MiB

基准来源：`docs/research/2026-07-16-local-tts-candidates.md`。

因此，两套 Kokoro client 可以解释数百 MiB 到接近 1 GiB 的额外模型开销，是“内存基线突然变高”的首要近期回归候选。

## 6. 根因二：GPUI 0.2.2 的 `CVDisplayLink` 被反复创建但不释放

### 6.1 上游生命周期问题

当前依赖钉死在 `gpui = 0.2.2`。其 macOS DisplayLink Drop 实现会停止原生 DisplayLink，但随后执行：

```rust
std::mem::forget(self.display_link.take())
```

对应源码：

```text
~/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/
  gpui-0.2.2/src/platform/mac/display_link.rs
```

与此同时，macOS window 实现中的 `start_display_link()` 会先停止当前 DisplayLink，再新建一个；显示周期完成后也可能再次停止和启动：

```text
~/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/
  gpui-0.2.2/src/platform/mac/window.rs
```

结果是旧原生对象未释放，新对象持续创建。

### 6.2 运行时证据

当前进程 Heap 中约有：

- 3,120 个 `CVDisplayLink`
- 3,137 个 `NSMachPort`
- 3,139 个 `CFRunLoopSource`

这些数量远高于单窗口程序的正常需求，与源码中的反复创建、故意不释放行为一致。

### 6.3 应用侧的泄漏放大器

提交 `6871e0b3`（`feat(app): add OpenCode agent functionality and related UI updates`）引入 Terminal 光标呼吸动画。

相关路径：

- `crates/app/src/app/frame_loop.rs`
  - 约 12ms 周期推进动画、Terminal cursor、语音 inset 等状态。
  - 状态变化时调用 `cx.notify()` 请求重绘。
- `crates/app/src/app/mod.rs`
  - Terminal 节点处于 Running 时启用 cursor animation。
- `crates/app/src/app/frame_throttle.rs`
  - 后续将 cursor repaint 限制到约 33ms，但仍会持续重绘。

后续动画提交进一步增加了重绘机会：

- `9e56acc0`：接入语音窗口 inset 基线。
- `200abcf4`：根据 PCM energy 动画化语音窗口 inset。
- 其他 card 初始化、光效和追逐动画也可能增加 DisplayLink 周期，但不是本报告确认的主要基线来源。

因此，应用代码不是 `mem::forget` 的源头，但持续动画显著提高了触发频率。

## 7. 提交归因

| 提交 | 判断 | 原因 |
|---|---|---|
| `b041ab3a` | 高置信度主要回归 | 新增独立且持久的 Agent announcement TTS cache，抬高模型常驻基线 |
| `6871e0b3` | 高置信度泄漏放大器 | 新增 Terminal 光标持续动画和重绘 |
| `9e56acc0` | 次要放大器 | 增加语音窗口 inset 动画状态 |
| `200abcf4` | 次要放大器 | PCM energy 驱动持续 inset 动画 |
| GPUI 0.2.2 依赖 | 上游结构性根因 | DisplayLink Drop 中 `mem::forget`，窗口周期反复重建 |
| `60bd0421` | 排除当前进程 | 提交时间晚于当前 App 二进制和进程启动时间 |
| `485a5e36` | 排除当前进程 | ZipVoice 提交时间晚于当前 App 二进制和进程启动时间 |

当前 `/Applications/Norma OS.app` 二进制生成时间约为 19:59，进程约在 20:01 启动；`60bd0421` 和 `485a5e36` 分别约在 21:06、21:40 完成，所以不可能影响被诊断的进程。它们在后续新构建中的内存表现需要单独验证。

## 8. 已排除或降级的假设

### 8.1 视频壁纸

当前数据库配置使用静态壁纸 `wallpapers/1411492.png`，没有运行 live video wallpaper。因此视频解码和逐帧纹理上传不是当前进程的主要原因。

### 8.2 Agent session 恢复提交

当前运行二进制早于 `60bd0421`，所以“恢复 Agent sessions 导致历史数据一次性载入”不能解释本次进程。新版本启动后仍应单独测试这个提交。

### 8.3 最新 ZipVoice 模型

当前运行进程使用 Kokoro，且二进制早于 `485a5e36`，所以 ZipVoice 不是本次已观察异常的来源。

### 8.4 SQLite、壁纸扫描或普通消息数据

当前内存构成以 ONNX、malloc 大块分配、GPU 和 DisplayLink 原生对象为主，暂未发现 SQLite 或普通 Agent 数据足以单独解释 1～2 GiB 增量的证据。

## 9. 建议的实施方案

### P0：统一 TTS 所有权

目标：同一个模型配置在进程内最多保留一套 TTS engine/session。

推荐方案：

- 在后台拥有一个串行 TTS worker。
- Dialogue reply、Agent completion announcement、设置页 preview 都向同一个 worker 提交合成任务。
- worker 独占非 `Sync` 的 Sherpa/ONNX client，避免为了并发复制模型。
- 配置变化时，等待或取消当前任务，然后统一卸载旧模型并加载新模型。
- App 退出时显式释放 worker 和模型资源。

不建议只在 Agent announcement 播放后无条件销毁模型，因为这会把内存问题转换成频繁加载和明显延迟；可以作为短期止血手段，但不是最终架构。

### P0：修复 DisplayLink 生命周期

需要在以下方案中选择一个并验证：

1. 升级到已修复该问题的 GPUI 版本，同时验证钉死 API 和现有行为。
2. 对 GPUI 0.2.2 做最小本地 patch/fork，正确释放 `CVDisplayLink`。
3. 如果上游保留 `mem::forget` 是为了规避析构期间回调竞态，需要设计安全的 stop、清空 callback、等待回调退出、release 顺序，不能直接机械删除 `mem::forget`。

仅降低动画帧率不能根治原生对象泄漏，只能放慢增长速度。

### P1：降低不必要的持续重绘

- Terminal cursor 只在可见、聚焦且确实需要动画时推进。
- 后台窗口、被遮挡卡片或非活动 workspace 不继续 cursor animation。
- 语音 inset 只在实际录音、播放或回落动画尚未结束时推进。
- 没有状态变化时不调用 `cx.notify()`。
- 评估静态 cursor 或显著更低频的视觉方案。

### P1：模型选择和预算

- 为每个本地模型维护实测 resident/peak memory 元数据。
- 在模型设置页展示内存等级，而不只展示下载大小。
- 在未完成 TTS 共享前，可临时使用较轻的 Matcha zh-en 降低风险，但这不是根因修复。

## 10. 后续验收标准

### 10.1 TTS 单实例验收

- 依次触发 Dialogue spoken reply 和 Agent task announcement。
- 两种功能都能正常播放。
- 同一模型配置下，进程内模型 client/session 数量不因调用入口增加而翻倍。
- 第二种播报完成后，physical footprint 不出现约一个完整模型大小的永久增量。
- 切换 TTS 模型后，旧模型资源被释放，新模型只保留一个实例。

### 10.2 DisplayLink 验收

- 启动真实 `.app`，记录初始 `CVDisplayLink`、`NSMachPort` 和 `CFRunLoopSource` 数量。
- 保持 Terminal cursor 动画和语音动画运行固定时间。
- 原生对象数量在初始化后保持稳定，不随每次 frame/display cycle 单调增长。
- 停止动画并等待稳定后，physical footprint 不继续单调增长。

### 10.3 内存门槛

具体阈值应在修复后的干净进程上重新标定。建议至少建立以下相对标准：

- 静置 10 分钟后，physical footprint 增量不超过新的明确预算。
- 连续运行动画 10 分钟后，DisplayLink 数量不随帧数增长。
- Dialogue 与 Agent announcement 各运行多次后，内存回到稳定平台，而不是按调用次数阶梯式上升。
- 模型配置不变时，重复播报不重复加载模型。

这类验收涉及真实 macOS 窗口和原生资源，必须使用真实 `.app` 的 L3 runner 或 Computer Use 驱动；headless GPUI 单测不能替代。

## 11. 建议的下一轮诊断手段

如需在实施前进一步确认每块内存的精确归属，可进行一次受控实验：

1. 使用 `MallocStackLogging=1` 启动新构建。
2. 分别记录冷启动、首次 Dialogue TTS、首次 Agent announcement、连续动画后的 heap snapshot。
3. 使用 Instruments Allocations 和 VM Tracker 对比四个阶段。
4. 在每个阶段记录 physical footprint、ONNX session、DisplayLink 和 GPU 区域。
5. 单独关闭 Agent announcements、切换轻量 TTS，执行 A/B 对照。

这可以把当前“高置信度归因”升级为逐分配栈的直接证明，并为修复后的内存预算提供基线。

## 12. 诊断状态

- 已完成：当前进程 footprint、heap、VM 和原生对象检查。
- 已完成：近期提交和运行二进制时间线比对。
- 已完成：TTS cache 所有权和 GPUI DisplayLink 生命周期源码检查。
- 已完成：Dialogue、Agent announcement、Settings Preview 共用单 resident TTS runtime。
- 已完成：GPUI 0.2.2 逐帧 DisplayLink 重建修补、重入保护和真实 Release 验证。
- 已完成：后台窗口暂停 Terminal cursor repaint。
- 尚未完成：精确复现 3 GiB 峰值。
- 尚未完成：带 malloc stack logging 的受控阶段性 A/B 实验。

## 13. 2026-07-19 实施与验收结果

### 13.1 已实施修复

1. 新增 `SharedTtsRuntime`，用一个后台互斥 resident slot 串行服务 Dialogue reply、Agent announcement 和 Settings Preview。相同模型与 Voice 不再因入口不同创建独立 cache；模型/Voice 变化时旧 runtime handle 移到后台释放。
2. 本地 patch `gpui 0.2.2`：同步 AppKit paint 前后暂停/恢复同一个 DisplayLink，不再逐帧 drop/recreate；`start/stop` 增加运行态幂等，window 增加 generation token，避免回调重入时恢复错误实例。
3. 窗口不 active 时停止 Terminal cursor 动画意图，前台业务效果保持不变。

### 13.2 真实 Release 观测

同机同时采样：

| 进程 | 运行状态 | Physical footprint | `CVDisplayLink` |
|---|---|---:|---:|
| 旧安装版 `/Applications/Norma OS.app` | 历史异常进程 | 约 1,854 MiB | 4,352 |
| 修复版 Release Dev App | 运行 12 分 17 秒，经历动画启动页、真实 workspace、前后台切换 | 约 219 MiB（峰值 418 MiB） | 20 |

本次同时观测的 physical footprint 降幅约为 `(1854 - 219) / 1854 = 88.2%`，超过 50% 目标；DisplayLink 对象减少约 99.5%，并且不再按 frame 增长。需要说明：两个进程的 TTS 加载状态并非严格同构，因此 88.2% 是真实用户态对照，不是受控的单变量 TTS benchmark。TTS 单实例结论由共享 runtime 的 factory 计数回归测试和真实 Matcha 合成测试补充证明。

### 13.3 功能与质量证据

- `cargo build -p app --release`：通过。
- `cargo check -p app`：通过。
- 共享 TTS、配置失效、Agent announcement 聚焦测试：通过（13 项）。
- Voice inset / Preview / Agent announcement E2E：通过（5 项）。
- TTS model lifecycle E2E：隔离运行通过。
- 真实 Matcha 22,050 Hz 合成 smoke：通过。
- storage、performance、video wallpaper、文件行数、禁止 `allow`、Agent guide、`git diff --check` 护栏：全部通过。

全量 `app` 为 522 passed / 3 failed / 5 ignored；3 个失败位于本次未修改的 Agent 15 字截断和默认主题断言。全量 `e2e` 为 85 passed / 2 failed / 4 ignored；TTS lifecycle 隔离复跑通过，表明全量并行失败来自全局测试状态竞争，另一个失败为现有 ZipVoice 草稿状态。以上失败没有被计作本次修复通过项。

### 13.4 后续加强项

- 增加同一模型、同一 workspace 下 Dialogue → Agent → Preview 的受控 ONNX session/footprint 阶段性 A/B。
- 配置切换时若旧合成任务仍持有旧 runtime，新配置可短暂加载新 session；后续可在 worker 中加入 epoch barrier，使模型切换严格等待旧任务退出。
- window 遮挡/换屏仍沿用 GPUI 上游的安全泄漏折中；当前修复消除了逐帧主增长路径，后续可评估 callback drain 后安全 release。

## 14. 2026-07-19 `0.1.40` 安装包复测

### 14.1 结论

用户安装并运行的 `/Applications/Norma OS.app` 为 `0.1.40`，构建号 `20260719.024051`。该二进制包含 `SharedTtsRuntime` 和本地 GPUI patch；因此这次高占用不是“用户仍在运行旧包”。

本轮复测将内存分成“Norma 主进程”和“Agent 后代进程树”两层后，主因变得清晰：

1. **Norma 主进程内部的旧 DisplayLink 泄漏已被压住。** 当前 PID `41672` 的 `CVDisplayLink` 在多轮采样中维持 `29～31` 个，没有恢复到旧包 `4,352` 个的异常量级。
2. **最新包会把 workspace 中的每个 Term/Agent 卡片恢复成 live session。** 当前仓库有 5 张 Agent 卡片，现场最终观察到 5 条当前 workspace 的 Codex 启动链；每条链还带 Node launcher、MCP server、Computer Use/Code Mode 等子进程。
3. **关闭旧 workspace 后，它的进程树没有退出。** 当前只有一个可访问的标准窗口，标题为 `norma-ai-os - Norma OS`，但同一个 Norma PID 下仍保留 `/norma-ai-os-site resume 019f70fe-...` 的旧启动链。该旧树单独有 16 个进程、约 `774 MiB RSS`，并在当前窗口打开前后持续运行。
4. **当前每张 Agent 卡片的成本不是一个轻量 UI 对象。** 5 条当前 workspace 进程树的 RSS 分别约为 `418 MiB`、`576 MiB`、`375 MiB`、`433 MiB`、`266 MiB`；旧 workspace 树约 `774 MiB`。六棵树合计约 `2.77 GiB RSS`。RSS 包含共享页，不能直接等同于物理内存，但它能解释为什么卡片数量和历史窗口会让整机占用迅速进入数 GiB。
5. **Norma 主进程仍有独立的约 1 GiB 基线问题。** 追加采样中主进程 physical footprint 为约 `1.1 GiB`、峰值约 `1.2 GiB`；Heap 仍有 2 个 `OfflineTtsKokoroImpl` 和 2 个 `InferenceSession`。因此不能把全部占用都归给子进程。

因此，当前问题不是“卡片 UI 变重”，而是三项叠加：所有持久 Agent 自动变为 live、每个 live Agent 是数百 MiB 的完整工具进程树、旧树退出失败继续累加。前两项是缺少休眠/预算导致的设计性高占用，第三项是明确的生命周期缺陷。

### 14.2 近期提交归因

不能把本轮问题简单归因于 `60bd0421`。`git blame` 和提交差异表明：

1. `e2598c4a` / `87fc6f0`（2026-06-20）建立 PTY 生命周期时，就已经只保存 `child.clone_killer()` 并在 `kill`/`Drop` 中调用一次 `kill()`。这是本轮确认的旧潜伏缺陷。
2. `4167bfa0` 所在的 2026-07-02 变更已经引入 `PendingLiveSessionRestore`：收集 document 中全部 Term/Agent 节点，并逐节点调用 `restore_live_session_for_node_with_node_sessions`。因此“打开 workspace 就启动全部卡片”不是 7 月 18 日才出现。
3. `24b4e282`（2026-07-14）加入 voice-addressable persistent agents，使 Agent 定义和卡片成为更稳定、长期保留的工作区资产，增加了重开时被恢复的 live Agent 数量。
4. `60bd0421`（2026-07-18）只增加“持久化 session id 缺失时，从 provider 历史发现 id”的恢复路径。该提交之前，缺 id 的 Agent 也会通过 `agent_command(profile)` 启动一个新 CLI；该提交让历史会话恢复更可靠、更自动，但不是全量启动行为的起点。

所以近期回归是**旧退出缺陷被新的持久化/恢复使用模式放大**，而不是某一个最近提交单独引入了 3 GiB 泄漏。

恢复的 Agent 卡片不是静态 UI 快照：runtime 会为每个节点重建 PTY 命令，并对 Codex 执行 `resume <session_id>`。当前进程命令行与该实现吻合，例如：

```text
codex ... --cd /Users/alan/Documents/mybook/norma-ai-os resume <session-id>
```

进程树中同时存在多个 `codex-native`、`start-mcp.mjs`、`mcp/server.mjs` 和 `node_repl`，直接证明恢复动作已经实例化了完整运行时，而不只是恢复卡片外观。当前 Codex 为 `0.144.4`，安装包修改时间为 2026-07-15；单张新建、尚未执行复杂任务的卡片已经约 `266 MiB RSS`，执行过程中加载更多工具后可达约 `400～600 MiB`。

### 14.3 核心生命周期缺陷：`kill()` 只发一次 SIGHUP

关闭窗口、删除节点、替换 runtime 的调用链表面完整：

```text
MultisoulApp::drop / enter_workspace_for_path / Runtime::remove_node
  -> Runtime::kill_all / LiveNode::kill
  -> TerminalSession::kill
  -> PtyHandle::kill
```

真正的问题在 `PtyHandle` 的所有权拆分：

- 真正的 `portable_pty::Child` 被移动到 `pty-reader` 线程，用于 EOF 后 `wait()`。
- `PtyHandle` 本身只保存 `child.clone_killer()`。
- Unix 下这个 clone 的实际类型是 `portable_pty::ProcessSignaller`。
- `ProcessSignaller::kill()` 只执行 `libc::kill(pid, SIGHUP)` 并立即返回；它不检查进程是否退出，不等待，不在超时后发 `SIGKILL`，也不处理后代进程。
- 与之对比，`portable-pty 0.9` 对真正的 `std::process::Child::kill()` 会发 SIGHUP、最多等待 5 次 × 50ms，仍存活时再调用系统强杀；Norma 的关闭路径没有使用到这个实现。

此外，`portable-pty` 在 spawn 前调用 `setsid()`，所以每张卡片的启动器是独立 session/process-group leader。Codex 后续启动的 MCP/Computer Use 工具还会建立各自的 process group。即使只杀最外层 PID 或最外层 PGID，也不能保证所有后代都被回收。

现场反证非常直接：旧 `norma-ai-os-site` 窗口已经不存在，当前只有 `norma-ai-os` 标准窗口，但旧 Node launcher、native Codex、4 组 MCP/Computer Use 工具和 code-mode host 仍全部存活。现有测试只验证短命 `/bin/sh` 能自行退出，以及测试 backend 的 `kill` 被调用；没有测试“忽略 SIGHUP 的直接子进程”或“另建进程组的孙进程”在节点/窗口关闭后必须消失。

### 14.4 进程内仍存在的次要问题

主进程 Heap 仍稳定存在：

- `2` 个 `sherpa_onnx::OfflineTtsKokoroImpl`
- `2` 个 `onnxruntime::InferenceSession`（语音识别活跃时会短暂增至 `5` 个，随后回落）
- `Kokoro INT8`，voice 为 `af_heart`

`SharedTtsRuntime` 只统一了 Dialogue、Agent announcement 和 Settings Preview 三个合成入口；启动时 `refresh_tts_model_states` 仍调用 `validate_tts_selection`，后者会完整执行 `create_tts_client_for_selected`。因此“进程内最多一个 TTS engine”的验收实际上没有满足。当前源码入口与两个 live Kokoro 对象高度吻合，但由于本进程未以 `MallocStackLogging=1` 启动，本报告把“第二个实例具体由启动校验还是旧 runtime clone 持有”保留为待 A/B 证明，而不冒充逐分配栈结论。

该问题解释主进程约数百 MiB 的额外基线，但不是当前 3 GiB 级应用总占用的最大来源。

### 14.5 已排除项

- **DisplayLink 再泄漏：排除。** 三轮间隔采样稳定为 `29`，后续窗口状态变化后仅为 `31`，没有按帧增长。
- **视频壁纸：排除。** 数据库仍配置静态内置壁纸 `wallpapers/1411492.png`。
- **CEF renderer 进程组：当前排除。** 没有发现 Norma 自身的 Chromium renderer/helper 子进程；CEF cache 仅约 `636 KiB`。
- **用户安装了未修复旧包：排除。** 安装包二进制含共享 TTS 修复标记与对应符号，构建时间也与本地修复构建一致。
- **同一 session 被重复恢复：当前不支持。** 现场 native Codex 的 session id 没有完全相同的重复项；当前是不同卡片加一个已关闭 workspace 的旧树，不是同一卡片被重复 spawn 的直接证据。
- **Terminal 可见快照复制是 GiB 主因：降级。** `GridSnapshot` 只复制可见行，当前实现没有把完整 scrollback 复制到 UI；它有分配成本，但不足以解释每张卡片数百 MiB 的独立外部进程树。

### 14.6 建议实施顺序

1. **P0：先修进程树所有权和确定性退出。** 关闭、删除、切换 workspace 与退出 App 时必须执行“请求优雅退出 → 有界 wait → 整棵受管进程树升级终止 → reap”。不能只向 launcher 发一次 SIGHUP。验收要覆盖忽略 SIGHUP 的直接子进程、另建 PGID 的孙进程、Codex/MCP/Node 和 Agent 启动的 dev server。
2. **P0：恢复卡片状态，不自动恢复全部 Agent 进程。** workspace 打开时先恢复视觉卡片与 session metadata；只有用户聚焦、派发任务或显式继续时再 lazy resume。
3. **P0：增加 Agent runtime 并发和内存预算。** 对同时常驻的 Codex/Claude/Cursor/OpenCode 会话设明确上限；超限节点进入 Dormant 状态，UI 清楚标记，而不是静默启动。
4. **P1：修掉第二个常驻 TTS engine。** 启动校验只做文件、manifest、voice/reference 检查；真正合成时再由共享 runtime 创建唯一 engine，并验证旧 runtime clone 能及时释放。
5. **P1：建立“主进程 + 后代进程”的真实 App 内存指标。** 不能只监控 `norma-os` PID；应同时记录进程数、各 runtime 类型数量、进程树 RSS/physical footprint 和 Dormant/Active Agent 数量。

### 14.7 本轮未实施事项

本轮仅完成诊断并更新报告，没有修改程序逻辑、停止用户的 Agent 会话或提交代码。当前修复仍位于 `dev` 工作区的未提交变更中；安装包 `0.1.40` 是从该工作区构建的产物，而 Git `HEAD`/`origin/dev` 仍为 `720e9139`。
