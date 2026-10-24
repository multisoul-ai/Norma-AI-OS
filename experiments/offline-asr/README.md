# 完全离线中英混合 ASR 实验

状态：P1 基准设施开发。当前 runner 仅覆盖 Zipformer 引擎与文件回放；不能据此宣称真实麦克风、自动断句或 App 端到端达标。完整目标见 `docs/exec-plans/2026-10-24-offline-mixed-asr-experiment.md`，语料来源见 `docs/research/2026-10-24-offline-asr-corpus.md`。

## 环境

Python 3.12、Apple Silicon macOS；依赖只安装在实验目录的 venv。音频、模型、原始转写和运行明细目录均由本目录 `.gitignore` 排除。下载公开权重/语料与识别分离，识别进程由 macOS sandbox 拒绝所有网络访问。

在仓库根目录执行一次：

```bash
cd experiments/offline-asr
/opt/homebrew/bin/python3.12 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
PYTHONPATH=. .venv/bin/python -m unittest discover -s tests -v
```

## 固定语料

先按语料研究文档下载 ASCEND 的固定 validation/test shard 到 `.cache/ascend/validation.parquet` 与 `.cache/ascend/test.parquet`。提取器也接受下载完成但尚未重命名的 `.part` 文件，只有大小与官方 SHA-256 完全匹配时才读取。

```bash
.venv/bin/python -m asr_lab.prepare_ascend --source .cache/ascend --output data/ascend-v1
```

抽样种子固定为 `norma-offline-asr-20261024-v1`：

- 开发集：每位 20 条 mixed，共 60 条；每位另取中/英各 8 条，共 48 条单语。
- 独立测试集：每位 120 条 mixed，共 240 条；每位另取中/英各 16 条，共 64 条单语。speaker 3 仅有 16 条满足时长与标注条件的英文样本，因此在任何模型评测前将单语配额统一降为 16。
- 仅 5 位说话者；真人自然对话及人工转写。保持官方 split，排除 `[UNK]`、不满足语言标签和 1–15 s 时长的样本，检查跨集 speaker/音频哈希/规范化全文重复。
- 这 300 条 mixed 是公开基准初筛集；尚不包含原计划中的个人口音、技术指令、噪声和长句专项。不能把它等同完整产品验收集。
- 音频 bytes 原样导出，不重采样，不把文件结束位置冒充人工 speech offset。数据许可 CC-BY-SA-4.0，署名见生成的 `dataset.json`。

## 运行现有 Zipformer 的引擎基线

```bash
bash run-offline.sh \
  --manifest data/ascend-v1/dev-mixed.jsonl \
  --model-dir "$HOME/Library/Application Support/Norma OS/models/zipformer-bilingual-zh-en-int8-2023-02-20" \
  --output runs/zipformer-dev-throughput-v1

bash run-offline.sh \
  --manifest data/ascend-v1/dev-mixed.jsonl \
  --model-dir "$HOME/Library/Application Support/Norma OS/models/zipformer-bilingual-zh-en-int8-2023-02-20" \
  --output runs/zipformer-dev-realtime-v1 \
  --realtime --repeats 3
```

目录必须是新目录，避免覆盖旧测量。每个输出包含 `metadata.json`、`results.jsonl` 和 `summary.json`。模型权重、热词、语料 manifest、实验源码、依赖版本与网络拒绝探针均进入记录。原始文字不打印到终端；只保存于本地结果文件。

默认配置对照当前 Sherpa 1.13.3：CPU、2 threads、双语 Zipformer、modified beam search、8 active paths、JAVA/KOTLIN 热词、score=2、同样的 BPE 词表。Python 引擎基线与生产 Rust/App 的采集和调度链路不同，正式对比还需补产品接入证据。可用 `--no-hotwords` 比较 greedy，无热词结果是独立配置，不能混入默认基线。

## 评分与计时口径

中文按字、英文按词，NFKC/casefold 后按固定规则计算 token 编辑距离；英文内部撇号保留。标点不计，不做翻译、简繁转换、数字口语改写。MER 使用总错误数/总参考 token 数，负例另报误出字率。失败/空输出仍计删除错误，不能只对成功样本求平均。

真实回放按 chunk 最后一个样本的采集时刻交付，默认 320 samples / 16 kHz = 20 ms。解码跟不上时记录积压并保留全部音频。文件回放只能给出从文件起点到 partial、文件终点到 final；没有人工起止标注时，真实 speech-onset/offset 指标为空。

`--realtime` 关闭时只测质量和计算 RTF，交互延迟字段为空。RTF 是解码墙钟时间/录音时长，不包括实时等待。模型构建时间单独记录；首个进程包含动态库加载，不能等同已经热身的每次识别成本。RSS 是单个实验进程的历史峰值，包含 Python/音频/模型，不能拿权重文件大小代替。

报告分位数为 nearest-rank，附有效计时数和缺失数；重复三轮不会增加独立录音或独立说话者数量。当前最终文本由输入文件 EOF 驱动，尚无 VAD 端点时延、键盘结束处理、UI 帧延迟或二遍提交控制器的证据。

## 当前验证边界

纯单测验证手算 MER、负例分母、慢消费者不丢样本、末块保留、计时原点、冻结音频哈希、抽样顺序无关和跨集泄漏。真实引擎冒烟/评测另存运行结果；模拟时钟和模拟 recognizer 的测试不计模型性能。后续更改会影响结果时，保留旧运行并建立新配置版本。
