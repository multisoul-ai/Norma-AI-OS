# 本地语音识别与派发实验

唯一协议：`docs/exec-plans/2026-10-31-voice-dispatch-accuracy-goal.md` v4。

`recording-list.tsv` 是供用户精选/替换的 50 条建议，8 类各 6–7 条。`annotation-template.json` 的 `text` 留空，必须填写真实录音的人工标准转写，不能直接把建议读稿或 ASR 输出当作标准答案。每条保留名单与桌面上下文快照；`terms` 用 `kind=name/term` 区分人名和术语，名字的 `agent` 指向规范名。

私有副本在 `~/.norma/voice-eval/annotations.json`。音频、实际转写、真人评测结果仅写仓库外；不要提交这些文件。苹果“语音备忘录”可以把本次录音拖出到 `~/.norma/voice-eval/audio/`。保留 M4A 原件，再本地转为 16 kHz、单声道、PCM16 WAV；长录音切分时保留原件 SHA-256 和起止时间，逐条人工确认。每个文件不一定等于一个样本，验收要求 50 条经过标注的独立指令。

开发集：

```bash
bash scripts/voice-eval/synthesize.sh ~/.norma/voice-eval/synthetic-dev-v1
python3 test/voice-eval/test_report.py
python3 test/voice-eval/test_synthesize.py
```

生产 A0 回放（先确认接电源、内存压力 NORMAL、机器空闲）：

```bash
VOICE_EVAL_CORPUS="$HOME/.norma/voice-eval/ascend304.jsonl" \
VOICE_EVAL_OUTPUT="$HOME/.norma/voice-eval/results/<本次唯一目录>" \
cargo test -p asr --test a1_eval evaluate_production_a0 -- --ignored --nocapture
```

默认每条重复 3 次，每 100 ms 推 3,200 B；`VOICE_EVAL_REALTIME=0` 可跳过墙钟等待以快速回放，模式写入配置哈希和每条记录。所有对照组须固定相同模式。按方案 §3.3，T_final 从 Finish 调用前开始，与推流速度无关；快速回放的 T_preview 仅描述文件送帧路径，不能代表真实麦克风字幕时延。prepare 的冷启动及第一条冷态解码单列，不混入暖模型分位数。Qwen RSS 每 20 ms 采样，不能当作 Metal 或进程树峰值。生产宿主仍会拒绝高内存压力并释放 worker；跑测器另外使用 `load1 <= logical_cpu/2` 作为“空闲”的保守操作阈值。

路由重放（纯函数不执行副作用；模型兜底标记未决）：

```bash
VOICE_EVAL_ASR="<结果目录>/asr-A0.jsonl" \
VOICE_EVAL_OUTPUT="<结果目录>" \
cargo test -p agent --test voice_eval_route replay_production_r0 -- --ignored --nocapture
python3 scripts/voice-eval/report.py \
  --corpus ~/.norma/voice-eval/synthetic-dev-v1/corpus.jsonl \
  --asr '<结果目录>/asr-A0.jsonl' \
  --routes '<结果目录>/route-A0-R0-L0.jsonl' --output '<结果目录>'
```

R0 重放只测 `plan_direct_address`，不包含 L0 的 `shared_context` / `memory_preamble`，也不能用 Skip 耗时冒充模型耗时。模型兜底需要后续真实模型调用才能出端到端目标正确率。样本缺失或重复时报告工具拒绝生成完整报告；实际 App 的麦克风、松键、卡片目标、T_e2e 另留真实验收证据。
