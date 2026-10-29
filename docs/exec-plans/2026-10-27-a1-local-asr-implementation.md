# A1 本地精准识别接入记录

状态：开发代码已接入，真实 App 验收尚未完成。完整 App 构建期间系统内存压力升至 WARN=2，已停止本任务构建以遵守用户资源约束，保留缓存。默认模式未改，未启用用户的 A1 设置，未加载真实模型或打开麦克风。设计依据为 `docs/design-docs/2026-10-27-a1-local-asr-integration.md`。

## 入口与边界

设置 → 语音 → 语音指令 → 识别来源 → 本地精准（A1）。先检查本地文件，用户显式点击「使用本地精准」后才后台准备模型并切换 provider。已有 A1 选择在返回本地单次模式时可恢复准备。A1 仅支持本地快捷键单次录音，最长 60 秒；不支持连续监听。

豆包混合与 GPT-Live 的音频、凭据及协议实现不更换。A1 不能在这两种模式激活；切换桌面对话模式会同步取消 A1 工作，在后台回收自有子进程。升级默认仍是 LocalSherpa。共享 VoiceControl 新增的取消令牌也防止旧录音启动结果在切换后生效。

Zipformer 双语 INT8 / sherpa-onnx CPU 负责预览，Qwen3-ASR 0.6B MLX 8-bit / Metal 负责终稿。只有有效会话的正常 Qwen 终稿进入 Agent；空终稿不提交，失败预览明确标为未发送。Rust 宿主最多持有一对 worker、一个活动录音槽；成功完成后复用模型。系统内存压力非 NORMAL 时拒绝准备并回收模型。

UI 沿用现有紧凑来源按钮和 setup row，保持浏览与启用两个状态，检查/准备/失败均有反馈；采用 Apple 设计技能中的清晰层级、即时状态反馈和现有组件一致性。云模式不展示 A1 文件检查页。

## 验收映射

| 要求 | 对应证据 | 状态 |
|---|---|---|
| 默认不变、模式隔离 | provider_registry；desktop_voice_mode_tests；voice_a1_settings_tests | ASR 测试通过；模式矩阵独立执行 2 项通过；App 设置测试基线编译通过，最新补充及实际执行待验证 |
| 取消、晚到结果、独占及回收 | a1_transport_tests；a1_host_tests；voice_recording_generation_tests | ASR 35 项通过（29 unit +3 registry +3 pipeline），包含最后一次取消槽修复；1 项真实 Azure 测试按要求 ignored |
| worker 帧协议、空结果 | crates/asr/tests/test_a1_worker.py | 2 项通过，不加载模型 |
| App 可编译 | cargo check -p app --tests | 本轮此前版本检查通过；最后新增 readiness 回归测试尚未复编，App 单测未实际执行 |
| UI crate 与新增词条 | cargo test -p ui；中英词条定向检查 | 定向检查通过；UI 测试构建等待共享锁后刚进入依赖编译，因系统再次 WARN 而取消，测试未执行 |
| 深浅主题入口、云页隔离 | settings-a1-configuration desktop E2E | 未运行：测试二进制尚未产出；完整构建因内存压力停止。已有场景不等于验收通过 |
| 工件固定与离线边界 | preparation-audit.json；config.rs；worker.py | 14/14 文件 size/SHA256 静态检查通过；真实 OS sandbox 下模型启动待验证 |
| 真麦克风、多轮、资源释放、延迟 | 后续最小 6 条 ×2 与真 App 验收 | 等用户明确负载窗口；本轮未执行 |
| 发布环境独立运行 | 固定 Python/native 库打包、发布构建 | 尚未实现与验收，不能称为 DMG 发布就绪 |

独立审查发现并修复了准备/录音任务排队后取消、跨窗口旧准备覆盖新选择、无新 PCM 时回收阻塞，以及释放后取消槽尚未退出导致恢复准备误报 busy。最后一项已静态复核，未扩大模型评测。

原始日志、工件审计副本和源码哈希保存在 `/Users/openclawd/Documents/asr-experiment-evidence/a1-integration-2026-10-27/`。其中 ASR 与 Python 日志对应已执行的测试；App 构建、UI 测试构建与资源停止记录区分了实际执行和取消。第一次停止时 swap used 为 4742 MiB；恢复 NORMAL 后 ASR 测试通过，随后系统再次 WARN、swap used 约 5818.62 MiB，故取消 UI 构建。这些是系统瞬时值，不是 A1 模型内存结果。没有结束其他任务或用户 App。

全仓文件行数护栏、禁止 allow 护栏、diff 空白检查通过。A1 新增 10 项中英词条齐全；全仓 i18n 检查另有其他工作区改动的问题，未擅自修复。App 编译检查有其他任务的 Effects 重复匹配警告。共享工作区问答类型在构建过程中改变曾造成一次旧依赖报错，当前导出已存在；没有为此改动其他任务源码。

## 工件准备

本机注册文件：`/Users/openclawd/Library/Application Support/Norma OS/models/a1/runtime.json`。

完整静态审计：`/Users/openclawd/Library/Application Support/Norma OS/models/a1/preparation-audit.json`。已有有效工件复用，无下载或模型导入。14 个文件逐一记录真实大小及 SHA256；固定依赖保存在 `crates/asr/runtime/a1/requirements.lock`，工件清单保存在 `crates/asr/runtime/a1/artifacts.json`。

```bash
python3 scripts/prepare-a1-local.py \
  --python /Users/openclawd/Documents/asr-experiment-evidence/b0-f1-a1-matched48-2026-10-26/runtime/.venv/bin/python \
  --zipformer '/Users/openclawd/Library/Application Support/Norma OS/models/zipformer-bilingual-zh-en-int8-2023-02-20' \
  --qwen experiments/offline-asr/models/qwen3-asr-0.6b-mlx8 \
  --model-root '/Users/openclawd/Library/Application Support/Norma OS/models'
```

此脚本是开发环境注册，引用外部 Python 和固定模型目录。正式分发仍须自带受控运行时与原生库、安装入口及独立环境校验，不能依赖这台机器的实验目录。

## 可复跑验证

所有 Cargo 操作串行，未并行加载模型。以下只用假 worker，不需要下载模型：

```bash
CARGO_BUILD_JOBS=1 python3 scripts/task-build-cache.py run a1-integration-20261027 -- test -p asr
python3 -B crates/asr/tests/test_a1_worker.py
CARGO_BUILD_JOBS=1 python3 scripts/task-build-cache.py run a1-integration-20261027 -- check -p app --tests
```

最后一次 ASR 复跑额外传入 `-- --test-threads=1`，结果 35 项通过。UI 验证为复用已经生成的 E2E 依赖缓存而使用工作区 target；完整 App、UI 构建都显式使用 jobs=1 / debug=0 / incremental=0。UI 构建中止，不能记为测试成功。App 最新补充的「缺失 A1 不影响云模式 readiness」单测尚未实际执行。

本任务私有构建缓存和工作区 E2E 缓存均保留用于后续验收；未执行缓存清理或发布构建。后续在资源允许时顺序补：App 聚焦单测、UI crate 测试、debug App 构建、配置页 desktop E2E。真实模型 6×2 和麦克风验证仍须等待用户明确运行窗口。

真实配置页面使用独立 E2E 数据根目录，不操作用户已打开的 Norma，不点击 A1 启用、不连接云、不打开麦克风：

```bash
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
  CARGO_TARGET_DIR="$PWD/target" CARGO_BUILD_BUILD_DIR="$PWD/target" \
  cargo build -p app --bin norma-os
bash scripts/desktop-e2e.sh settings-a1-configuration
```

## 仍需保留的限制

- 历史 304 条准确率属于实验引擎；新进程协议尚未做真实模型转写一致性对照。
- 当前 finish 先做预览收尾，再请求 Qwen，共用 10 秒故障预算。历史并行编排的延迟不能搬作集成后的性能结论。
- 静态注册、假 worker 和配置页测试不能证明模型可启动、麦克风端到端延迟或真实模型内存成本。
- 本轮不启动 304 条或 48×3，不清理其他任务文件，不修改用户默认配置。
