# 本地语音指令识别与派发准确率 · Goal 方案

更新时间：2026-10-31 · 版本：4（新增 §10 Mac 本地识别技术调研与 G0.5 引擎筛选） · 当前阶段：G0 阻塞，完整基线未完成。v3 按用户要求把对象从豆包对话改为**本地语音指令**（A1：Zipformer 预览 + Qwen3-ASR 终稿）。v2 的实验修正和「真人录音精选 50 条」的决定保留。豆包和 GPT-Live 不在本 Goal 内，实验工具留作以后复用。

所有优化都先做可追溯的实验对照：同一批语料、同一套指标、同一份基线。没有数据支撑的改动不合入。

## 1. 目标与成功标准

| 项目 | 内容 |
|---|---|
| 问题 1 | 中英混合识别不准：Agent 英文名和技术术语被识别成音近中文或别的英文词 |
| 问题 2 | 派错人：用户说「让 Daisy 去做」，实际派给了 Alan |
| 根因 1 | Qwen 终稿调用 `model.generate(..., system_prompt=None, hotwords=None)`（`crates/asr/runtime/a1/worker.py:144`），没有使用模型自带的上下文偏置；Zipformer 预览的热词写死为 `JAVA`、`KOTLIN`（`worker.py:87`） |
| 根因 2 | `crates/agent/src/direct_address.rs` 只认句首点名和「给 X 新任务」，且是精确前缀匹配；其余交给用户配置的 Norma 模型，`agent` 参数是自由字符串，派发前没有校验 |
| 根因 3（时延） | `crates/agent/src/master.rs:257` 先等 `shared_context` 和 `memory_preamble` 完成，再做直接点名判断；直接派发路径白白等了这两步（耗时待 G0 实测） |
| 约束 | 低延迟优先：不新增串行模型调用；新增代码层只能是微秒到毫秒级开销；音频和转写不出本机 |

成功标准（G0 基线出来后填写最终阈值）：

| 指标 | 门槛 | 性质 |
|---|---|---|
| 路由错派率（转写里的人名正确，仍派错人） | 测试集上 = 0 | 硬门槛 |
| 识别错派率（ASR 把人名识别成别人，导致派错） | 比基线下降，单独统计 | 收益指标，不承诺为 0 |
| 派发目标正确率 | ≥ 98% | 硬门槛 |
| 人名识别率 | 比基线提升，目标 ≥ 95% | 主要收益指标 |
| 术语识别率 | 比基线提升，目标 ≥ 90% | 主要收益指标 |
| 通用中英混合 MER（已有 304 条语料） | 不高于 A1 历史值 11.23% 太多（上限 +0.5 个百分点） | 防退化：热词不能伤害普通句子 |
| Qwen 终稿时延 T_final p95（暖模型） | 不比基线慢 10% 以上 | 硬门槛：热词会加长 prompt，要实测 |
| 路由耗时 T_route p95（直接派发路径） | < 5 ms | 硬门槛 |
| 端到端 T_e2e p50（直接派发路径） | 比基线更快 | 收益指标 |
| 反问率 | ≤ 5% | 防过度反问 |

错派一律按原始语音的人工标注判定，不按转写判定。派发前校验只能保证「目标与转写一致」，所以端到端错派拆成路由和识别两项分别统计。

## 2. 优化方案（被测对象）

```text
麦克风 → voice_input → A1 宿主
            ├─ Zipformer 预览 ← [Z] 名单热词文件（只影响字幕）
            └─ Qwen 终稿      ← [A1] hotwords 名单与术语 / [A2] system_prompt 上下文
                  ↓ 终稿
       handle_final_voice_text → NormaSession
                  ↓ [L1] 直接点名判断提前到 memory_preamble 之前
       [R2] 拼音与音近归一化（戴西 → Daisy）
       [R1] 点名解析：句首、让 / 叫 / 请 / 交给 / 安排给 X
            ├─ 唯一命中 → 代码直接派发（不经模型）
            ├─ 分数接近 → 反问「是 Daisy 还是 Dana？」
            └─ 没点名   → Norma 模型兜底，[R3] agent 参数为名单 enum
                  ↓
       [R4] 派发前校验：目标不在原话里就拒绝并反问
```

| 编号 | 改动 | 位置 | 预期收益 | 延迟 |
|---|---|---|---|---|
| A1 | Begin 消息带热词列表：名单 `name`、`speech_alias`、Agent 产品名、项目术语；worker 传给 `generate(hotwords=...)` | `crates/asr/src/providers/local/a1/transport.rs`、`host.rs`、`crates/asr/runtime/a1/worker.py` | 人名和术语识别率 | prompt 变长，需实测 |
| A2 | A1 + `system_prompt` 加一句上下文：当前画布卡片的 Agent 和任务标题 | 同上 | 续说与指代 | 同上 |
| Z | Zipformer 热词文件改为名单 | `worker.py` | 只改善字幕，不影响派发 | 约 0 |
| L1 | 直接点名判断提前，命中时跳过 `memory_preamble` | `crates/agent/src/master.rs` | 直接派发更快 | 负值 |
| R1 | 扩展点名句式 | `crates/agent/src/direct_address.rs` | 更多请求走确定性派发 | 微秒级 |
| R2 | 中文转拼音，英文名用音近编码（如 Double Metaphone）；最高分和第二名差距足够大才锁定 | 新文件 `crates/agent/src/name_match.rs` | 抵消识别错字；没有中文别名的自定义英文名也能对上 | 微秒级 |
| R3 | `assign_agent_task` / `reply_agent` 的 `agent` 参数改为当前名单 enum | `crates/agent/src/adapter/definitions.rs` | 模型不能编造名字 | 0 |
| R4 | 派发前校验：目标不在原话里（含 R2 归一化后的名字）就拒绝并反问；原话提到多人时（「不是 Alan，是 Daisy」「让 Daisy 告诉 Alan」）不判断，交给 R1 | 派发边界 `crates/agent/src/adapter/` | 路由错派率为 0 | 0 |

已核实的模型能力：实验环境里的 `mlx_audio` 中，`Qwen3ASR.generate` 支持 `hotwords: List[str]`，内部用 `merge_hotwords` 并入 `system_prompt`（`asr-experiment-evidence/.../mlx_audio/stt/models/qwen3_asr/qwen3_asr.py:1254`、`:1365`）。所以 A1 和 A2 走的是同一个机制，区别只是 prompt 内容。热词条数上限没有文档，G1 实测不同长度对识别率和时延的影响。正式运行时打包的 `mlx_audio` 版本必须与实验版本一致。

## 3. 实验设计

### 3.1 对照分组

| 组 | 内容 |
|---|---|
| A0 | 当前配置（基线）：无热词、无 system_prompt |
| A1 | 热词：只放名单 |
| A1+ | 热词：名单 + 产品名 + 项目术语 |
| A2 | A1+ 加上下文一句话 |
| R0 | 当前路由（基线） |
| R1–R4 | 逐层叠加：R1 → R1+R2 → R1+R2+R3 → 全部 |
| L0 / L1 | 直接点名判断在 `memory_preamble` 之后 / 之前 |

所有组都是**同一构建里的运行时配置**（A0 = 热词为空；R、L 组用开关逐层打开），不靠回退旧 commit 复现基线。

**ASR 结果落盘，路由离线重放。** 每条音频在每个 A 组识别后，把转写写入 JSONL。路由层是纯函数（`agent::plan_direct_address`），在缓存的转写上重放 R0–R4，零成本、可重复、结果确定。落到模型兜底的样本才需要真实调用模型（R3 组），每条跑 3 次统计波动。R3 必须调用生产的系统提示构造函数，输入每条样本录制时的名单和桌面上下文快照；模型的 provider 和型号固定，写进 manifest。

归因方式：

- 固定路由组，比较 A 组的差值：识别层的提升。
- 固定识别组，比较 R 组的差值：路由层的提升。
- L0 和 L1 的差值：时延优化的收益。

### 3.2 语料集

| 来源 | 规模 | 用途 | 存放 |
|---|---|---|---|
| 已有 304 条中英混合评测（2 位说话人，`ascend-v1` 数据和 `nfkc-casefold-cjkchar-word-v1` 归一化） | 304 条 | **通用回归**：确认热词不伤害普通句子 | 沿用 `/Users/openclawd/Documents/asr-experiment-evidence/` |
| 合成语音：macOS `say`，选 Tingting、Eddy、Flo、Reed 4 个 zh_CN 声音 | 每类 ≥ 15 条文本 × 4 个声音，约 480 条 | **开发集**：跑通工具链、调 R2 阈值、热词长度 | 文本清单进仓库，音频本地生成 |
| 用户真人录音（用户决定：精选 50 条） | 50 条，8 类每类 6–7 条 | **留出测试集**：只用来出结论，不用来调参 | 仅本地 `~/.norma/voice-eval/`，仓库只存清单和音频 SHA-256 |

每条样本的标注：`id`、`audio`、`sha256`、`text`（人工标准转写）、`targets`（期望 Agent，可为空）、`action`（`assign` / `reply` / `none` / `clarify`）、`terms`（必须识别对的人名和术语）、`category`、`mic`（内建麦克风 / AirPods / 其他）、`env`（安静 / 嘈杂）。

语料覆盖 8 类，每类有对应例句：

| 类别 | 例句 |
|---|---|
| 句首点名 | Daisy，把登录 bug 修一下 |
| 句中点名 | 让 Daisy 去修登录 bug / 这个交给黛西 |
| 中文别名 | 艾伦，跑一下测试 |
| 音近冲突（名单同时有 Daisy、Dana、Diana） | 让 Dana 看一下 PR |
| 中英混合术语 | 把 gpui-component 升到 0.5.1，然后跑 cargo test |
| 无点名，交给模型 | 帮我看看现在谁在忙 |
| 更正与否定 | 不是 Alan，是 Daisy 去做 |
| 多人 | Daisy 和 Alan 分别 review 一下 |

50 条里建议至少 10 条用 AirPods、10 条在嘈杂环境。这 50 条是精选难例，结果会比日常使用偏低，只用来比较组间差异。

名单夹具 `test/voice-eval/roster.json` 固定 8 个 Agent，包含上面的音近冲突组，其中 2 个是没有中文别名的自定义英文名。所有组共用这份名单。

### 3.3 指标定义

准确率（文本按 `nfkc-casefold-cjkchar-word-v1` 归一化，与已有 304 条评测一致）：

| 指标 | 定义 |
|---|---|
| MER | 混合错误率：中文按字、英文按词计算编辑距离 ÷ 参考单位数，与历史 A1 11.23% 口径一致 |
| 人名识别率 | `terms` 中的人名在转写里出现（或经 R2 归一化后命中）的比例 |
| 术语识别率 | `terms` 中的非人名术语原样出现的比例 |
| 目标正确率 | 最终工具目标集合 = `targets` 的样本比例 |
| 路由错派率 | 转写里的人名正确，最终目标仍包含 `targets` 以外 Agent 的比例 |
| 识别错派率 | 转写里的人名已经错成另一个 Agent，导致派错的比例 |
| 反问率 | 结果为澄清的比例（`action=clarify` 的样本反问算正确） |
| 模型兜底率 | 没走确定性路由、交给模型的比例 |

时延（同一时钟的 `Instant`，单位 ms，报告 p50 和 p95；冷启动和暖模型分开记录，不混算）：

| 指标 | 起点 → 终点 | 说明 |
|---|---|---|
| T_ready | 开始准备 → 两个 worker ready | 冷启动，只记录不比较 |
| T_preview | 第一帧音频 → 第一个非空 Zipformer 预览 | 字幕的跟手程度 |
| T_final | 发出 Finish → Qwen 终稿 | **主要识别时延**，A 组对比用这一项 |
| T_route | 拿到终稿 → 工具调用参数确定 | 路由层耗时；兜底路径包含模型耗时；L0/L1 对比用这一项 |
| T_e2e | 松开快捷键 → 工具回执 | 只在真实 App 验收时测 |

Qwen 先缓存整段音频，Finish 后才推理，所以 T_final 和推流速度无关。文件回放可以按实时速度推流（每 100 ms 发 3,200 B），这样 T_preview 也和真实麦克风一致。

资源：每组记录 Qwen 进程内存峰值，确认热词不让内存明显上升。

### 3.4 运行规则

1. 运行前机器空闲，接电源，记录内存压力状态；内存压力不正常时不跑。
2. 同一条样本的 A0 / A1 / A1+ / A2 交替运行，抵消机器状态的漂移。
3. 每条样本每组跑 3 次。Qwen 是 `temperature=0.0`，转写应该相同；不同就记为不稳定样本。时延取全部数据。
4. 准确率按样本配对比较（McNemar 检验）；时延 p50/p95 差值用 bootstrap 给出 95% 置信区间。
5. 真人测试集只有 50 条：只报告总体数字和配对差异清单（哪几条从错变对、哪几条从对变错），不做分类别结论。在 50 条上，提升达到约 15 个百分点以上才可能显著；更小的提升用合成开发集的趋势作辅证，并在报告中写明。
6. 热词长度和 R2 阈值只在合成开发集上调，调好后冻结，再跑真人测试集。看过真人测试集结果后又改了参数，必须补充新的真人样本再报告。

### 3.5 可追溯性

每次实验产出一个结果目录，任何一个数字都能追到代码、配置和音频：

```text
test/voice-eval/results/<日期>-<commit 短哈希>/
  manifest.json        # commit、语料版本与音频哈希、名单版本、各组配置哈希、
                       # 模型与 mlx_audio 版本、机器型号、内存压力、开始和结束时间
  asr-A0.jsonl         # 每条：样本 id、第几次、转写、T_preview、T_final、内存峰值
  asr-A1.jsonl
  route-A1-R4-L1.jsonl # 每条：样本 id、路由路径、目标、动作、T_route
  report.md            # 汇总表、组间差值、配对差异清单
```

结果目录只存转写和数字，不存音频。真人语料的转写含用户内容，结果目录不提交，只把 `report.md` 的汇总数字抄进本文 §6。合成语料的结果可以提交。

## 4. 实验工具（G0 交付）

| 组件 | 位置 | 职责 |
|---|---|---|
| 合成语料 | `scripts/voice-eval/synthesize.sh` | 用 `say` + `ffmpeg` 生成 16 kHz 单声道 PCM16 WAV |
| ASR 跑测 | `crates/asr/tests/a1_eval.rs`（`#[ignore]`，需要本机模型） | 直接驱动生产的 `A1EngineHost` / `A1Recognizer`，推 WAV，写 `asr-<组>.jsonl` |
| 路由重放 | `crates/agent/tests/voice_eval_route.rs`（`#[ignore]`） | 读 ASR 结果和名单夹具，调用 `plan_direct_address`，写 `route-<组>.jsonl` |
| 汇总报告 | `scripts/voice-eval/report.py`（只用 Python 标准库） | 计算 §3.3 全部指标、组间差值和配对差异清单 |

跑测必须走生产的 A1 宿主和 worker，不另写一套推理。热词由 Begin 消息传入，基线传空列表，保证「测的配置」就是「上线的配置」。已有 304 条评测的评分器可以复用其归一化规则，不必重写。

## 5. 阶段与门槛

| 阶段 | 内容 | 退出条件 |
|---|---|---|
| G0 基线 | 实验工具、合成开发集、50 条真人测试集与标注；跑 A0 × R0 × L0；用 A0 重跑 304 条，确认新协议和历史 11.23% 一致 | 有 `report.md`，基线写入 §6；304 条 MER 与历史值差距 ≤ 0.5 个百分点；同组重复跑准确率波动 ≤ 2 个百分点 |
| G0.5 引擎筛选 | 见 §10：Qwen3-ASR 1.7B、Fun-ASR-Nano、Apple SpeechAnalyzer、Whisper large-v3-turbo 与当前 A1 对比 | 选出接入的引擎，或确认保持 Qwen3-ASR 0.6B |
| G1 识别层 | 实现 A1、A1+、A2、Z；测热词长度的影响 | 人名和术语识别率提升；304 条 MER 不超过上限；T_final p95 不比基线慢 10% 以上 |
| G2 路由层 | 依次实现 L1、R1–R4，每层单独出数字 | 路由错派率 = 0，目标正确率 ≥ 98%，反问率 ≤ 5%，T_route p95 < 5 ms |
| G3 线上追踪 | 每轮语音记录本地结构化日志；`analytics` 加 `voice_dispatch` 事件 | 真实 App 验收通过，见 §7 |
| G4 收口 | 选定上线组合，更新回归基线 | 本文写明最终组合与全部数字 |

G1 和 G2 互不依赖，可以并行开发，但都要和同一份 G0 基线对比。

G3 的 `voice_dispatch` 事件只允许上报：路由路径（`direct` / `model` / `clarify` / `rejected`）、T_final、T_route、热词数量、是否触发 R4 拦截。按 `analytics` crate 的规则，不上传转写、人名或任务内容。

## 6. 结果记录

（G0 起每阶段填写，每行注明结果目录。）

| 日期 | commit | 组合 | 语料 | 人名识别率 | 术语识别率 | MER | 目标正确率 | 路由/识别错派率 | T_final p50/p95 | T_route p95 | 结果目录 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 2026-10-26 | 实验引擎 | 历史 A1 | 304 条 | — | — | 11.23% | — | — | — | — | `asr-experiment-evidence/b0-f1-a1-matched48-2026-10-26` |
| 2026-10-31 | `780b09fa5dda07c0cf18eb2cd2d162be026573ca` + 工作区源哈希 | A0（R0/L0 待重放） | 304 × 3，中断于 437/912 | 待完整评测 | 待完整评测 | 待完整评测 | 待重放 | 待重放 | 待完整评测 | 待重放 | `~/.norma/voice-eval/results/2026-10-31-9322105b-g0-304-run3/` |

### G0 执行日志（2026-10-31）

- Goal 已启动，严格按 G0 → G0.5 → G1/G2 → G3 → G4 推进；G0 尚未达到退出条件，不提前报告收益。
- 工作基准 commit：`9322105b375234f4408e8e726576a99b9658398d`，工作区已有其他任务未提交改动，保留原状；实验代码也尚未提交，不能将 commit 单独当作完整构建标识，运行时补源文件哈希。
- G0 配置 SHA-256：`68c4f73f3f8c05d38061d63b78cc46446b2904736d6a38cbb897a53d4c15ece6`；结果目录：`test/voice-eval/results/2026-10-31-9322105b-g0`；名单 SHA-256：`af32ba4ecccdd40d7a2db08a1feb3aefb5ab91836227570f496e33b8c13cd93f`。
- 已准备 8 Agent 名单（Daisy/Dana/Diana 音近组；Rowan/Quinn 无中文别名）、8 类 × 15 = 120 条开发文本。音频待用指定 4 声音生成，共 480 条。
- 50 条真人录音建议清单：`test/voice-eval/recording-list.tsv`；空白转写模板：`test/voice-eval/annotation-template.json`；私有工作副本：`~/.norma/voice-eval/annotations.json`。每类 6–7 条，建议 10 条 AirPods、10 条嘈杂环境。须由用户精选、录制、人工标注，不用合成语音替代，不用其调参。
- 验收映射：G0 工具用已知编辑距离/配对差异测试与生产宿主 WAV 回放；路由调用生产 `plan_direct_address`。A1 真 App 单次麦克风验收单列，文件回放和注入转写不能替代。
- 前置观察：接电源；系统空闲内存比例 55%；可用磁盘约 13 GiB。正式推理前另查内存压力等级与机器负载，候选下载前核算空间，不回收其他任务缓存。
- G0.5 按默认 4 候选全测，尚未开始。G1/G2/G3/G4 未开始。
- TDD：报告工具已按“失败→实现→通过”验证混合分词/MER、英文名边界、错派归因、兜底未决、精确 McNemar、按音频簇的配对 bootstrap、真人输出目录保护。真实 `say` + `ffmpeg` smoke：1 文本 × 4 声音通过，480 条正在生成；A1 WAV 输入测试通过。
- 历史评分器兼容性核验：608 次历史观测（304 音频 × 2 次），884 / 7872 = 11.2296748%；逐条与冻结评分器结果一致。证据：`test/voice-eval/results/2026-10-31-9322105b-g0/scorer-compatibility.json`。**这不是新 A0 跑测，不满足 G0 的 ≤0.5pp 重跑门槛。**
- 首次 A0 实跑在模型加载前被前置检查阻止：压力已恢复 NORMAL，但一分钟负载 21.7808、逻辑 CPU=10，不满足本次保守空闲阈值 `load1 <= CPU/2`。命令：`VOICE_EVAL_CORPUS=~/.norma/voice-eval/ascend304.jsonl VOICE_EVAL_OUTPUT=~/.norma/voice-eval/results/2026-10-31-9322105b-g0-304 cargo test -p asr --test a1_eval evaluate_production_a0 -- --ignored --nocapture`，退出 101，无模型推理/新准确率；前置记录：`~/.norma/voice-eval/results/2026-10-31-9322105b-g0-304/a0-manifest.json`。
- 用户表示已使用 Apple 语音备忘录录音，并授权从 Mac 默认目录查找。只核对 UI 元信息，看到当日 05:07–05:10 的 9 个文件（9–18 秒），尚未核实独立指令条数/人工转写；不可记为 50 条已就绪。默认目录 `~/Library/Group Containers/group.com.apple.VoiceMemos.shared/Recordings` 被 macOS 隐私保护拒绝（Operation not permitted），正常 UI 导出在尝试中；未更改隐私权限、未读取其他录音转写。


### G0 增量记录：语料导入与工具就绪

- 480 条合成开发音频全部生成：8 类各 60 条（15 文本 × Tingting/Eddy/Flo/Reed）。本地音频目录 `~/.norma/voice-eval/synthetic-dev-v1/`；公开哈希清单 `test/voice-eval/results/2026-10-31-9322105b-g0/synthetic-inventory.json`，未提交音频。合成数据只用于开发和回归。
- 用户将本次录音导出到 `/Users/openclawd/Documents/demo`；已保留原件并转换 10 个 M4A，共 121.386667 秒。私有导入目录 `~/.norma/voice-eval/imports/2026-10-31-voice-memos/`；源/WAV 哈希及候选停顿保存在该目录的 `import-manifest.json`。按 -35dB / 0.4s 停顿仅检测到 34 个候选片段，未确认是否对应 50 条独立指令，未运行 ASR、未把自动转写当人工标准；正在等待用户说明录制顺序、麦克风和环境。
- R0 重放测试已观察失败后实现并通过：句首 Daisy → 生产 `plan_direct_address` → reply/Daisy；句中“让 Daisy…” → model/未决，目标与模型总时延为 null。直接路由计时范围明确只含纯函数，不含 L0 记忆加载；不能据此宣布 L1 收益或端到端目标正确率通过。
- G0 仍缺：304 × 3 的新 A0 数字、开发集 A0 × R0 × L0 的完整报告、50 条人工确认的真人测试标注、A1 真 App 单次录音/松键/卡片验收。阶段保持 G0；G0.5 四引擎未开跑。

### G0 增量记录：验证与基线模型恢复

- 受影响 crate 验证：`cargo test -p asr -p agent -j 2` 退出 0，52 个测试套件合计 200 passed / 0 failed / 5 ignored；日志 `~/.norma/voice-eval/crate-tests.log`。随后新增的结果目录防覆盖测试已观察失败并修复，A1 聚焦测试 2 passed / 1 ignored。`check-max-file-lines.sh`、`check-no-allow.sh`、`check-performance-guardrails.sh` 均退出 0。以上为工具/回归验证，非真实模型基线。
- A0 第 2 次尝试：空闲检查通过（压力 NORMAL，load1=3.91），但 `factory.prepare()` 报 No such file or directory。已定位：现有 `a1/runtime.json` 指向的 `experiments/offline-asr/models/qwen3-asr-0.6b-mlx8/` 整个目录缺失；Python 和 Zipformer 仍存在。结果目录 `~/.norma/voice-eval/results/2026-10-31-9322105b-g0-304-run1/`，配置哈希见该目录 `a0-manifest.json`，日志 `~/.norma/voice-eval/a0-304-run1.log`，无新准确率。
- 根据历史冻结记录恢复同一模型：`mlx-community/Qwen3-ASR-0.6B-8bit@89e96d92ba34aca20b3e29fb10cc284097d1219f`，9 个文件全部必须匹配 `crates/asr/runtime/a1/artifacts.json` 的大小与 SHA-256；不改生产默认配置、不换模型版本、不覆盖已有文件。下载/核验记录 `~/.norma/voice-eval/qwen-restoration-audit.json`；完成后才能重新跑测。
- 暖/冷口径：prepare 的 T_ready 单列，第一条真实解码标为 cold；其余才进入暖模型 T_final 分位数。T_preview 从第一帧实际交付开始计时。Qwen 内存以 20ms 间隔采样 resident size，不能当作 GPU/Metal 或进程树峰值。

### G0 增量记录：正式 A0 重跑启动

- Qwen 0.6B 的 9 个固定文件已全部恢复并通过大小/SHA-256 校验；恢复审计 SHA-256：`7e88435ba0f48eeb6bc162825fc4c22ec4bab748c75afa437a0b7cbcd89f56d4`，完成时间 `2026-10-31T12:32:46.676305+00:00`。
- 正式尝试目录：`~/.norma/voice-eval/results/2026-10-31-9322105b-g0-304-run2/`；日志：`~/.norma/voice-eval/a0-304-run2.log`；实际配置哈希、完整 commit、生产 worker/宿主/transport/runner 哈希及语料清单快照均写入该目录。304 × 3 实时音频回放理论下限 55.7 分钟（不含推理）；不得在未完成时拿部分结果对比历史门槛。

- run2 在模型加载前再次被空闲检查阻止：load1=6.02539 > 5，压力 NORMAL，退出 101。仍无新 A0 转写/时延。模型已恢复完成，等短期负载降下来后使用新目录重试；不会覆盖 run1/run2 记录。

### G0 正式 A0：run3 已中断（不可用于门槛）

- 本轮已通过空闲/压力/接电源检查，生产 Zipformer 和 Qwen 双 worker 已就绪并开始逐条回放；准备时间（T_ready）详见本轮 manifest，正式分位数等全部 912 次观测结束后计算。
- 实际启动 commit：`780b09fa5dda07c0cf18eb2cd2d162be026573ca`（其他任务在准备期间更新了 HEAD；结果目录的 9322105b 仅为任务起始标识）。本轮配置 SHA-256：`a835face938192dd6fdd4f07e5f1266bfd572108d7cec0506fed3ed8b6a5cbfc`；语料 SHA-256：`46502f19d8fc17cc4ff9d6999e6018deb23233ce6ab6d9d6adcc253188650bf9`。
- 结果目录：`~/.norma/voice-eval/results/2026-10-31-9322105b-g0-304-run3/`。原始行与模型版本、宿主/worker/runner 源哈希、语料快照均在本机。§6 中不填写未完成评测的准确率。

- 该轮最终中断：437 / 912 次观测，146 个不同音频；不可作为完整基线。此为进度，不是完整准确率或时延结论；Goal 仍为 active。下一步完成 304 重跑后生成报告，接着补开发集基线与真实 App 单次链路；真人的分段/人工标准仍待确认。

### G0 增量记录：报告输入与真实 App 验收边界

- 报告输入新增 TDD 校验：完整重复次数、音频 SHA-256 与标注一致、每组配置唯一、行内组名与文件名一致。先观察失败，再实现，通过 5 个报告聚焦测试；人工设定转写/时延的工具夹具有显式标记，报告不能充当实验结果。
- 真实 App 验收前已查现有能力：`test/desktop-e2e/Scenarios/settings/SettingsA1Configuration.swift` 明确只浏览设置、不加载模型；`crates/app/src/e2e_control/mod.rs` 的 `execute_keys` 总是连续发送 down/up，`NsKey` 同样不能保持按住，现有场景不能证明 Cmd+E 录音松键链路。因此 G0 的该项使用真实 App + Computer Use/用户实际按住录音单列证据，不能拿配置场景或 `VoiceTranscript` 注入替代。等本轮基线结束、释放双 worker 后执行，避免 GPU/音频负载干扰当前批次。

### G0 增量记录：复现快照与快速文件送帧

- run3 的编译来源已保存为结果目录内 `source/` 的 10 份源文件/依赖文件；逐份核对生产 manifest 记录的编译哈希，其中 runner 为未提交源文件快照。`source-snapshot.json` SHA-256 为 `489fd2f87752bc63b9bf616cc74aed4553924ef7dc00397516ec2485f838a913`。正在运行的二进制/worker 不变，后续工具编辑不能覆盖本轮来源。
- 按 §3.3 增加 `VOICE_EVAL_REALTIME=0`（默认仍为 1）：后续开发回放可跳过墙钟等待，完全相同 PCM 仍只交给生产识别器一次，Finish 起计的 T_final 不变；配置和每条结果记录回放模式。快速模式 T_preview 只表示文件送帧路径，不能代表麦克风跟手程度。各对照组固定相同送帧模式，不混算。
- TDD 已观察“缺少 feed_audio”失败，再实现；1 秒/32000 B 输入经实时与快速模式均逐字节送达，送帧阶段不调用 Finish。`cargo test -p asr --test a1_eval -j 2`：3 passed / 1 ignored；报告校验继续 5 passed，新增同组混用回放模式拒绝测试。后续工具源码哈希已更新在 `test/voice-eval/results/2026-10-31-9322105b-g0/manifest.json`；本轮 run3 保持旧 runner 快照和实时配置。

### G0 增量记录：后续真实模型兜底的本机边界

- 只读核对当前 Norma 配置后发现：当前 provider 为云端兼容接口，并非本地 Ollama 服务。§1 又要求“音频和转写不出本机”，因此不能直接把真人转写送去云端完成 R3。
- 已向用户提出澄清：保持完全本机并准备本地路由模型，或明确允许仅派发文本使用现有 Norma 云端模型（录音仍不上传）。在答复前，R3 真实调用保持待定；当前 A0 生产 worker 继续由 OS 拒绝网络，仅本机推理。
- 该项不改 G0.5 默认四候选、不改变任何当前模型配置。真人录音的分段/顺序、麦克风/环境与人工标准转写也仍待用户确认；不以 ASR 输出代替人工标准。

### G0 中断与对抗审查（2026-10-31）

- 已从 exec 句柄确认 run3 终止，退出码 101；437 / 912 次观测（146 个不同音频）后在 `finish()` 返回 `SessionClosed`。日志记录总耗时 2111.40 秒。完整原始行不改动，`run-outcome.json` 保存 SHA-256 并标记 `use_for_gate=false`；本轮不计算完整基线、不与历史 11.23% 比门槛。
- 当前检查内存压力已是 NORMAL，原轮未逐时采压力，不能断言瞬时压力是已证实根因。生产资源观察线程会在压力异常时取消宿主；其他取消来源须以证据区分，不关闭资源保护。本轮 manifest 在进程退出后仍留 running 是已确认的工具记录缺陷，先补失败测试修复，再为下一轮增加只读资源采样。
- 按 AGENTS.md 的独立 subagent 对抗审查要求，仅审查 G0 工具（只读、无模型/重型构建、未读私有语料），确认 2 个待修复项：report 未验证路由 manifest 的 ASR 源哈希；无点名样本 ASR 幻觉增添人名被误归因路由错误。主 Agent 将先复现再修复，并保留测试证据。

### G0 恢复准备（修复已复核）

- 两个独立审查发现已由主 Agent 复现并修复，报告回归测试 7 passed：路由缓存必须匹配 ASR 源 SHA-256 和配置哈希；英文/中文别名的 ASR 幻觉归因识别，无法归因时报告未决，不能用 0 通过门槛。独立 subagent 复核未发现新的可确认缺陷。
- `RunGuard` 的失败后残留 running 已先失败后修复；成功提交保持 complete。只读 `PressureWatch` 在后台每 100ms 采样，仅状态变化时写 `resource-events.jsonl`，不控制生产宿主、不修改内存策略。聚焦测试 6 passed / 1 ignored；因此尚未证明旧轮 SessionClosed 的根因。
- 当前 commit `d83d824e869a637d27b0fc350e382d2d2eab2676`；`cargo test -p asr -p agent -j 2` 的完整日志共 205 passed / 0 failed / 5 ignored，完成到最终 doc tests。日志与源码哈希证据：`test/voice-eval/results/2026-10-31-9322105b-g0/after-review-test-evidence.json`。
- 机器当前压力 NORMAL、接电，但 load1 仍高于既定空闲阈值 5；下一轮使用新目录、完整 304×3、快速文件送帧，不拼接旧失败行。G0 未退出，四候选 G0.5 未开始；真人标注和真实 App 单次录音验收仍待完成。

- 独立复核与两个发现的修复证据已保存为 `test/voice-eval/results/2026-10-31-9322105b-g0/review-resolution.json`，列出基准/当前 commit、精确源码 SHA-256 和回归测试名。
- 下一轮完整 304×3 已进入有时限的空闲前置等待，exec session 53977；状态 `~/.norma/voice-eval/a0-fast1-gate.json`，资源前置采样 `~/.norma/voice-eval/a0-fast1-gate-events.jsonl`。只有压力 NORMAL 且 load1≤5 连续满足约 15 秒才启动生产宿主；180 秒仍不满足则退出 75、不加载模型。计划结果目录 `~/.norma/voice-eval/results/2026-10-31-d83d824e-g0-304-fast1/`；正式配置哈希以实际启动生成的 manifest 为准。

### G0 当前外部前置未满足

- 有时限的 fast1 前置任务已实际退出 75（idle-timeout），没有加载模型、没有生成新的 ASR 行。最后采样压力 WARN(2)，load1=122.26；因此不能开始或继续正式基线，也不启动真实 App 模型验收。
- 状态与前置采样：`~/.norma/voice-eval/a0-fast1-gate.json`、`~/.norma/voice-eval/a0-fast1-gate-events.jsonl`。再次核对本机标注：50 条模板的非空人工标准转写仍为 0，导入的 10 个录音标注仍待确认；仅在本机计算标注完成数量，未输出转写正文。
- 本次开始累计“环境与必需人工输入阻塞”的审计轮次 1，记录 `~/.norma/voice-eval/blocked-audit.json`。Goal 仍 active，尚未达到连续三轮阻塞阈值；下一轮先复核实际环境，条件满足即可继续，不会绕过资源限制或把未标注录音当评测答案。

- 本轮最终轻量检查已观察退出码 0：`check-max-file-lines.sh`、`check-no-allow.sh`、`check-performance-guardrails.sh` 与报告 7 个测试均通过。当前没有运行中的 ASR 批次；fast1 前置已超时，下一轮先重新检查真实资源状态。

- 连续第 2 轮前置复核仍未满足：压力 `1`，load1 `47.46`；50 条模板的人工标准转写/确认数仍为 0，R3 模型边界仍待答复。未启动新模型。commit、前置配置哈希和结果：`test/voice-eval/results/2026-10-31-9322105b-g0/preflight-audit-2.json`。Goal 保持 active，尚未到三轮阻塞阈值。

- 压力恢复 NORMAL 后进行第二次有界前置等待（fast2，最多 300 秒），已从 exec 句柄确认实际退出 75，仍未启动 ASR。记录 `~/.norma/voice-eval/a0-fast2-gate.json` 和 `~/.norma/voice-eval/a0-fast2-gate-events.jsonl`；当前复核压力 `2`、load1 `58.35`，仍不符合既定空闲条件。没有运行中的本 Goal ASR 批次；阻塞审计仍计当前第 2 轮，不把同一轮里的多次轮询凑成三轮。

### 当前状态：G0 阻塞（连续第三轮已复核）

- 当前压力 `2`、load1 `109.33`，仍不满足 §3.4 的正常压力与空闲条件；50 条真人标准转写确认数仍为 0，录音分段/设备环境及 R3 本地/云端文本边界待答复。两个前置等待进程均已真实退出 75，没有后台 ASR 批次。
- 可独立完成的 G0 工具、修复和验证已完成；继续所需的是环境变化和必需人工输入。按连续三轮阻塞规则将 Goal 标记 blocked，不跳到 G0.5，不将失败/未标注数据作为成功结果。
- commit `d83d824e869a637d27b0fc350e382d2d2eab2676`；本次前置配置 SHA-256 `0e70421b0aa69cdbe170cfd61b69755bf1f9b720981c3e77966d759cd35ad804`；证据目录 `test/voice-eval/results/2026-10-31-9322105b-g0/`，具体 `preflight-audit-3.json` 与 `report.md`；私有详细审计 `~/.norma/voice-eval/blocked-audit.json`。
- 恢复后先完成完整 304×3 A0、开发集基线与真实 App 单次录音验收，并补齐 50 条人工标注；通过 G0 后仍按四候选 G0.5 → G1/G2 → G3 → G4 的原目标推进。

### 用户决定：A，全部本机处理

- 用户明确选择 A：ASR 与 R3 路由兜底均保持本机，原始录音和转写不发送云端。现有生产 Norma 云端配置保持原状；实验将另备本地路由模型及隔离配置。
- 当前未检测到 Ollama 可执行文件或 11434 服务。本地路由准备放在 ASR 基线完成后的独立资源窗口，避免并行加载模型干扰时延。
- 本次资源检查压力 NORMAL、接电、load1=3.97，已恢复到 G0 操作阈值；优先继续可独立完成的完整 304×3 A0。真人分段、人工标准和设备环境仍待确认，不把选择 A 当作标注确认。

### G0 恢复执行：先预跑用户的 10 个录音文件

- 用户选择 A 后进一步要求“先测这个10个的”。先按原文件做 A0 预跑，保留 10 个 M4A/WAV 原件；不猜测分段、不将文件数当作已确认独立指令数。仅生成本机待核对转写，不用 ASR 草稿充当人工标准，不据此调参；50 条正式测试集的完成状态仍待确认。
- 上一轮 fast3 已确认退出 101：编译成功，但随后空闲检查实际 load1=8.0762 > 5，未加载模型、0 条新观测。实际 commit `d6eae17529c1465817779bb0fbc4f7232ce850de`，配置 SHA-256 `a84c4303d315167194c750e232bc7fcae1abd7815c6c053ee0b2aa68be79aa1e`；结果目录 `~/.norma/voice-eval/results/2026-10-31-87c18842-g0-304-fast3/`，目录标签早于实际运行 commit，以 manifest 为准。`run-outcome.json` 明确 `use_for_gate=false`。
- 已冻结编译完成的跑测二进制，避免重编译抬高临跑前负载：`~/.norma/voice-eval/runners/a0-d6eae175-fast3/`；二进制 SHA-256 `1e675902c00059b6bf7c070ef3835990d38216ff30b95755b94801ebf8c06e0e`，另保留 64 份源码/配置快照。复核相关源码与编译时 commit 一致；正式结果同时记录当前 commit、编译源码哈希和二进制哈希。
- 10 文件私有预跑清单 `~/.norma/voice-eval/imports/2026-10-31-voice-memos/human10-pilot-input.jsonl`，清单 SHA-256 `cd433bc66ecbfa22bad5a368e3a381d0b73946ae8c208cda2834aa49a08e4334`。人工标准确认数仍为 0，准确率暂不可计算。human10-pilot1 前置最多等待 300 秒，压力 NORMAL、接电、load1≤5 连续满足 15 秒才加载模型；状态与采样分别在 `~/.norma/voice-eval/human10-pilot1-gate.json` 和 `~/.norma/voice-eval/human10-pilot1-gate-events.jsonl`。
- 隔离本地路由运行时 Ollama v0.40.13 已下载并核对官方 SHA-256 `bae29252232b9e1358cf5801a7d65a07442af18fb5467cb68c7fc3711f59c511`；位置 `~/.norma/voice-eval/local-router/ollama-v0.40.13/`，尚未加载路由模型、未发送音频或转写。仅用于后续本机兜底实验，不改 App 现有 provider 设置。

## 7. 验收映射

| 验收标准 | 证据 |
|---|---|
| 路由错派率 = 0 | 路由重放报告；R4 单元测试覆盖「模型给错目标 → 拒绝」和「原话提到两人 → 不误拦」 |
| 句式和音近匹配正确 | `crates/agent` 单元测试，§3.2 每类至少一例，先写失败测试（TDD） |
| 热词传到模型 | `crates/asr` 的假 worker 测试断言 Begin 消息带热词；`a1_eval` 真实模型跑通 |
| L1 不改变非点名请求的行为 | `master.rs` 相关单元测试：没点名时仍先加载记忆再调模型 |
| 反问文案中英文 | 走 `ui::i18n` 键，运行 `python3 scripts/check-i18n.py` 和 `python3 scripts/test-i18n.py` |
| 真实链路 | 真实 App 选本地语音指令，按住快捷键说 §3.2 每类各 1 句，核对卡片目标和日志里的时延字段 |
| 机械约束 | `check-max-file-lines.sh`、`check-no-allow.sh`、`check-performance-guardrails.sh` |

## 8. 改动白名单

| 范围 | 允许改动 |
|---|---|
| A1 热词通道 | `crates/asr/src/providers/local/a1/transport.rs`、`host.rs`、`crates/asr/runtime/a1/worker.py`；App 侧把名单交给 factory 的接线（`crates/app/src/settings/voice/state/asr_provider.rs`、`crates/app/src/app/voice/`） |
| 路由 | `crates/agent/src/direct_address.rs`、`direct_address_turn.rs`、`master.rs`、新增 `name_match.rs`、`adapter/definitions.rs`、派发边界 `adapter/` |
| 依赖 | R2 需要汉字转拼音，评估是否引入 `pinyin` crate（离线字表），引入前在本文记录理由 |
| 实验工具 | §4 的 4 个文件、`test/voice-eval/` 下的文本清单、名单夹具和合成结果 |
| 埋点 | `crates/analytics` 的事件定义与白名单属性 |
| 文案 | `crates/ui/locales/` 的反问和拒绝文案 |

`asr` crate 不能读 Storage 或 Keychain：名单由 App 读出，作为热词列表交给 A1 factory。不改豆包、GPT-Live、TTS 和 UI。

## 9. 风险与未决

| 风险 | 处理 |
|---|---|
| 热词加长 prompt，拖慢 Qwen 终稿 | T_final 是 G1 的硬门槛；按 G1 实测结果限制热词条数 |
| 热词让普通词被识别成人名（过度偏置） | 304 条 MER 和「无点名」类的误派数作为防退化指标 |
| 名单变了，热词要更新 | 每次 Begin 都带当前名单，不在 worker 里缓存 |
| 运行时打包的 `mlx_audio` 和实验版本不一致 | manifest 记录版本；发布前用 304 条回归 |
| 合成语音和真人差距大 | 结论只看真人测试集；合成语音只用于开发和回归 |
| R2 音近匹配本身产生误派 | 阈值只在开发集上调；拿不准时反问，不猜 |
| 真人测试集只有 50 条，统计力有限 | 只报总体和配对差异清单；需要分类别结论时再扩充 |
| 只有用户一位说话人 | 结论只对用户本人成立；推广前至少再录 1 位说话人；304 条的 2 位说话人作通用补充 |
| A1 真实 App 链路本身还没验收（`docs/design-docs/2026-10-27-a1-local-asr-integration.md` §6） | G0 先完成 A1 单次录音的真实 App 验收，否则 T_e2e 没有基线 |
| 现有 `DirectPlan::Say` 反问文案是硬编码中文 | G2 一并改为 i18n 键 |

## 10. Mac 本地语音识别技术调研（2026-10-31，v4）

结论：主线仍是 Qwen3-ASR + 热词。新增 G0.5「引擎筛选」：用同一批语料对比 4 个候选，只把胜出者接入生产宿主。v3 写的「英伟达模型 Mac 跑不了」不准确：英文和欧洲语言的 Parakeet 能通过 FluidAudio（CoreML / ANE）和 parakeet-mlx 在 Mac 上运行，但它们不支持中文；只有中英混合版 Parakeet CTC 限定 NIM 和 CUDA。

| 技术 | 中文 / 中英混合 | 热词偏置 | Mac 运行方式 | 结论 |
|---|---|---|---|---|
| Qwen3-ASR 0.6B（当前 A1） | 支持 52 种语言和方言 | system prompt 上下文偏置；Argmax 英文测试中 1.7B 的关键词召回从 0.53 升到 0.87 | MLX / Metal，sherpa-onnx | 主线 |
| Qwen3-ASR 1.7B | 同上，准确率更高 | 同上；Argmax 称 3000 个关键词内不明显变慢 | MLX | 筛选候选：准确率上限，时延和内存要实测 |
| Fun-ASR-Nano（阿里，2025-12） | 中英日；第三方称中英混合优于 SenseVoice | 热词通过 prompt 传入，sherpa-onnx 已支持 | sherpa-onnx，macOS arm64，CPU | 筛选候选：更轻，不占 GPU |
| Apple SpeechAnalyzer（macOS 26） | 中文需按 locale 验证；没有中英混合数据 | SpeechTranscriber 不支持自定义词表；DictationTranscriber 的 contextualStrings 效果未公开 | 系统自带，免安装 | 筛选候选：零部署成本的对照组 |
| OpenAI Whisper large-v3-turbo（2024 年开源，此后 OpenAI 没有新的开源语音模型） | 支持中文；中英混合容易偏向单一语言 | 只有 prompt；Argmax 测试中加关键词后 WER 从 15.4 变差到 21.24 | WhisperKit（CoreML / ANE）、whisper.cpp、mlx-whisper | 筛选候选：业内参照 |
| OpenAI GPT-Transcribe / gpt-live-transcribe（2026-07） | 支持 | — | 仅云端 API | 不选：不是本地方案 |
| 小米 MiMo-V2.5-ASR（8B） | 官方称中英混合 WER 14.07 | — | 8B，太大 | 不选：时延不满足 |
| NVIDIA Parakeet v3 / Ultra | 不支持中文（25 种欧洲语言） | NeMo 有 TurboBias | FluidAudio、parakeet-mlx | 不选 |
| NVIDIA Parakeet 0.6B CTC 中英混合 | 专门的中英混合模型 | 文档未提 | 仅 NIM + CUDA | 不选，除非出现 Mac 可用的导出版本 |
| NVIDIA Nemotron 3.5 Transcribe / ASR Streaming | 报告自认中英混合是短板 / 未写明混合 | — | CUDA | 不选 |
| Mistral Voxtral、Kyutai STT、Moonshine | 不支持中文 | — | — | 不选 |

可借鉴的方法：

| 方法 | 用法 |
|---|---|
| NVIDIA TurboBias：按条传热词，解码时用热词树加分 | 与 A1、Z 思路一致；Z 用 sherpa-onnx 的热词树实现 |
| LLM-HB（Interspeech 2026，arXiv 2610.16579）：面向中英混合的热词偏置 | ASRU2019 上相对 MER 降低 20.30%；代码是否开源未知，G1 结果不理想时再评估 |
| Argmax 关键词评测口径（无关键词 / 本段关键词 / 整文件关键词） | 采用到 G1：热词分「只放本句相关」和「放全名单」两种，测过度偏置 |

### G0.5 引擎筛选

在 G0 基线之后、G1 之前执行。筛选只回答「哪个引擎值得接入」，不改生产代码：

1. 用独立的离线脚本（`scripts/voice-eval/engines/`，每个引擎一个文件），按「WAV 进、文本出」跑 50 条测试集和 304 条回归集。
2. 每个引擎测两种配置：无热词、全名单热词（引擎支持时）。
3. 记录 MER、人名识别率、术语识别率、T_final p50/p95、内存峰值，统一由 `report.py` 汇总。
4. 胜出条件：人名识别率最高，且 T_final p95 不大于当前 A1 的 1.2 倍。没有引擎胜过 Qwen3-ASR 0.6B 就保持现状。
5. 胜出引擎再接入生产 A1 宿主，按 G1 的流程复测。筛选脚本的数字不直接作为上线依据。

资料来源：[Argmax OpenBench](https://github.com/argmaxinc/OpenBench/blob/main/BENCHMARKS.md)、[Qwen3-ASR 技术报告](https://arxiv.org/html/2601.21337)、[sherpa-onnx FunASR Nano](https://k2-fsa.github.io/sherpa/onnx/funasr-nano/index.html)、[NVIDIA NIM 支持矩阵](https://docs.nvidia.com/nim/speech/latest/reference/support-matrix/asr.html)、[Nemotron 3.5 Transcribe 报告](https://research.nvidia.com/labs/nemotron/files/NVIDIA-Nemotron-3.5-Transcribe-Technical-Report.pdf)、[TurboBias](https://arxiv.org/abs/2508.07014)、[LLM-HB](https://arxiv.org/abs/2610.16579)、[MiMo-V2.5-ASR](https://github.com/XiaomiMiMo/MiMo-V2.5-ASR)、[MacParakeet](https://macparakeet.com/blog/whisper-to-parakeet-neural-engine/)。表中英文关键词数据来自 Argmax 的英文财报语料，不代表中文表现。
