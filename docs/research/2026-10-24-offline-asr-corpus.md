# 离线中英混合 ASR 的真人评测语料

日期：2026-10-24。范围：读取公开文档、元数据及 16 字节 Parquet 文件头；本次研究没有下载语音主体、调用模型或上传音频/转写。结论：优先使用 ASCEND 官方固定修订的 validation/test 两个 Parquet，共 **212,288,700 字节（202.45 MiB）**；不下载约 1.22 GB 的完整数据集。

## 来源、版本与许可

- 官方 GitHub 是 **HLTCHKUST/ASCEND**，不是当前返回 404 的 CAiRE/ASCEND。GitHub README 明确将语料下载指向 Hugging Face 的 CAiRE/ASCEND。[C1]
- GitHub 当前 commit：`8c50c2e2c65f8555eb1d7655310dd585dc1f6710`。HF 数据 revision：`737e9800ae31be9932ba8464c80366559bd28424`，API 的 lastModified 为 2024-07-16T08:56:04Z，gated=false、disabled=false。[C2][C3]
- **数据许可是 CC-BY-SA-4.0**；GitHub 工程代码的 MIT 不能替代语料许可。保留作者、论文、下载来源、revision、许可及筛选/格式变更说明；若分发改编语料，遵守署名及相同方式共享条件。[C1][C4]
- 论文固定版本为 arXiv:2112.06223v6。作者报告真人自然多轮对话，23 位中英双语受试者，10.62 小时，录于香港；不是 TTS 合成。专业标注公司按语义边界/长停顿切句并人工转写，之后进行人工和自动复核。[C5]
- 原始录音标准：16 kHz、单声道、16-bit PCM、未压缩 WAV。HF `audio` feature 标注采样率 16000。导出时仍逐文件检查实际 WAV 头部，不能仅信 metadata。[C4][C5]

## 固定下载对象

| 官方 split | 文件 | 大小（字节） | SHA-256（HF LFS oid） |
|---|---|---:|---|
| validation | `main/validation-00000-of-00001.parquet` | 106532266 | `3bdec53d2abfd3dd4f0d86a6df4e27e60f20660edc9b66055ae0ef8ec05cf7e2` |
| test | `main/test-00000-of-00001.parquet` | 105756434 | `a4c81d2b5ed6124f052089a695972808c16e0ce0c365ec9773c5d1a8fcf043a7` |

固定 URL：

- https://huggingface.co/datasets/CAiRE/ASCEND/resolve/737e9800ae31be9932ba8464c80366559bd28424/main/validation-00000-of-00001.parquet
- https://huggingface.co/datasets/CAiRE/ASCEND/resolve/737e9800ae31be9932ba8464c80366559bd28424/main/test-00000-of-00001.parquet

大小和 SHA 来自固定 revision 的 HF tree API。[C6] test URL 实际 range 请求返回 HTTP 206、`Content-Range: bytes 0-15/105756434`，前四字节为 `PAR1`。HF viewer 提供的 `refs/convert/parquet` URL 是可变引用，正式下载使用上面的 commit URL。

可执行获取示例（下载由主实验执行，本研究没有运行以下命令）：

```bash
mkdir -p /tmp/norma-asr-ascend
curl --fail --location --retry 2 --max-filesize 107000000 --output /tmp/norma-asr-ascend/validation.parquet 'https://huggingface.co/datasets/CAiRE/ASCEND/resolve/737e9800ae31be9932ba8464c80366559bd28424/main/validation-00000-of-00001.parquet?download=true'
curl --fail --location --retry 2 --max-filesize 107000000 --output /tmp/norma-asr-ascend/test.parquet 'https://huggingface.co/datasets/CAiRE/ASCEND/resolve/737e9800ae31be9932ba8464c80366559bd28424/main/test-00000-of-00001.parquet?download=true'
python3 - <<'PY'
from pathlib import Path
import hashlib
expected = {
    'validation': (106532266, '3bdec53d2abfd3dd4f0d86a6df4e27e60f20660edc9b66055ae0ef8ec05cf7e2'),
    'test': (105756434, 'a4c81d2b5ed6124f052089a695972808c16e0ce0c365ec9773c5d1a8fcf043a7'),
}
for split, (size, digest) in expected.items():
    path = Path('/tmp/norma-asr-ascend') / f'{split}.parquet'
    checksum = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            checksum.update(chunk)
    if path.stat().st_size != size or checksum.hexdigest() != digest:
        raise SystemExit(f'{split}: size/checksum mismatch')
    print(split, 'verified', size)
PY
```

## 字段与实际分布

官方 schema：`id:string`、`path:string`、`audio:Audio(16000)`、`transcription:string`、`duration:float32`、`language:string`、`original_speaker_id:int64`、`session_id:int64`、`topic:string`。[C4] 标准 HF Parquet 的 audio 通常以 bytes/path struct 保存，提取前通过 PyArrow schema 确认；`path` 记录的是上游机器路径，不能在本机直接打开，也不能用作导出路径。导出文件名使用受控的 split + id，数据清单保留原 path 供溯源。

官方 split 行数：train 9869、validation 1130、test 1315。[C4] 本次读取 HF viewer 全部 validation/test 行的 **JSON 元数据** 共约 2.56 MB，统计如下。Viewer 是当前服务视图，正式实验须以校验过的固定 Parquet 再计算一遍，不能用它替代本地证据。[C7]

| split | 总条数 | 中文 / 英文 / mixed | speaker → 全部条数 | speaker → mixed 条数 |
|---|---:|---|---|---|
| validation | 1130 | 655 / 262 / 213 | 20 → 335；12 → 646；7 → 149 | 20 → 60；12 → 126；7 → 27 |
| test | 1315 | 685 / 257 / 373 | 17 → 861；3 → 454 | 17 → 150；3 → 223 |

验证集和测试集 speaker 集合分别为 `{7,12,20}` 与 `{3,17}`，交集为空。**两 split 合计仅 5 位说话者**；不能声称满足原计划“至少 6 位”的覆盖目标，也不能把 train 的人随机搬入 test 来凑数。论文明确按 speaker 不重叠切分，保留官方边界。[C5]

`session_id` 在不同 speaker 下重复使用 1–4，并非全局唯一会话 ID。组合 `(original_speaker_id, session_id)` 可区分当前元数据中的声道/话题序列；manifest 另保存 original_split 与原始 WAV basename。不能因两 split 都有 session=1 就宣称泄漏，也不能只靠这些字段声称已证明原始双人对话完全独立；至少检查 speaker 不交叉、音频 SHA 不重复，保留论文的 speaker-independent 范围声明。

validation 总计约 3323.60 秒，其中 mixed 939.08 秒；test 总计约 3302.97 秒，其中 mixed 1554.92 秒。纯 mixed 且 1–15 秒的候选分别有 210 / 369 条。测试中仅两位说话者，bootstrap 句子得到的窄区间也不能代表跨口音/跨用户可靠性。

## 60–300 条获取与冻结方案

1. 先只读 validation metadata，保留所有 mixed 作候选；至少同时含汉字与拉丁字母作一致性检查，异常项记录原因。按 speaker、时长分层，以固定 seed 的 SHA 排序选样，避免直接取前 60 条偏到同一说话者或话题。
2. 开发集建议 60 条 mixed，每位 speaker 20 条；元数据中每位均足够。模型/端点/热词调试只用这部分。可另取中文、英文样本作单语退化检查，清单分开报告。
3. 预注册独立测试建议 120–240 条 mixed，speaker 3/17 各半；1–15 秒区间两人分别有 223/146 条，故最多各取 120 条是可行的。60 开发 + 240 测试可得到 300 条真人混合语料，但仅覆盖 5 位说话者。根据执行预算也可先 60 + 120。
4. 抽样只看引用文本、speaker/时长/topic，不看任何模型输出；冻结 selection manifest、seed、音频 SHA256、引用文本及其哈希后才比较模型。正式 test 不用于热词构造、模型参数选择或文本归一化规则修补。
5. 用 PyArrow 从本地 Parquet 读取 audio bytes，原样写受控 WAV；无需 `datasets` 自动解码，也不需要将公开音频提交给远程模型。用 Python 标准库 `wave` 校验 mono/16k/16bit、非空 frames；duration 与 frame_count/16000 允许小数舍入误差。若实际编码不符，明确转换并记录来源及转换命令。
6. 只有下载阶段联网。进入 ASR 执行时关闭网络依赖/自动下载，结果、录音和文本均留本机。把 cache 与导出 WAV 放实验目录；两 shard 加导出预计远低于现有 24 GB 空间，不拉 train 三个 shard。

**评测真值注意事项：**作者保留重复、缩写、口语停顿词和英文 contractions，数字按单词写，通常不带标点，[UNK] 表示不可辨识。[C5] 首版 MER 必须事先定义汉字逐字、英语逐词、大小写/标点及缩写处理；保留原文和归一化后真值。不能在看见错误后为了某个模型改答案。ASCEND 属于安静教室、领夹麦克风、年轻双语学生自然对话，不能替代用户 Mac 麦克风、技术标识符、噪声和长指令场景；本阶段结论应限定为公开混合语音基准的初筛结果。

## 替代来源核验

| 来源 | 已核验情况 | 本次决定 |
|---|---|---|
| DynamicSuperb/CodeSwitchingSpeechIdentification_ASCEND | 固定 revision `e44f58901a4e43719f949f8d61b1d6eb891d287d`；500 条、53,752,387 字节，CC-BY-SA-4.0，字段含 audio/file/transcription/instruction/label，但无独立 speaker 字段。[C8] | 是 ASCEND 派生子集，不是独立外部验证；原版只多约150MB且保留 split/speaker，优先原版。 |
| SEAME 官方 LDC2015S04 | 真人中英混合自然对话；156 speakers、约192小时，16k/16bit FLAC，人工转写；官方要求 LDC 使用协议并提供会员/非会员获取流程。[C9] | 后续有合法许可时可增加口音覆盖；当前不把公开镜像等同已获得原始数据授权。 |
| AudioLLMs/seame_dev_man / seame_dev_sge | 维护者卡分别有2610/3222条，下载约278.57/278.60MB；revision 为 `4a510d3e42acdf71a08441219b70fab0df9d255d` / `9b41e3e4b96ac23a05636b46a5792ed3ef54378b`，仅 context/instruction/answer，无 speaker 字段，卡内未给独立许可声明。[C10] | 当前可访问不等于许可和speaker来源已核清；不作为本次自动下载首选。 |

## 主来源

- [C1] https://github.com/HLTCHKUST/ASCEND/blob/8c50c2e2c65f8555eb1d7655310dd585dc1f6710/README.md
- [C2] https://api.github.com/repos/HLTCHKUST/ASCEND/commits/main
- [C3] https://huggingface.co/api/datasets/CAiRE/ASCEND
- [C4] https://huggingface.co/datasets/CAiRE/ASCEND/blob/737e9800ae31be9932ba8464c80366559bd28424/README.md
- [C5] https://arxiv.org/html/2112.06223v6 ；论文正式入口：https://aclanthology.org/2022.lrec-1.788/
- [C6] https://huggingface.co/api/datasets/CAiRE/ASCEND/tree/737e9800ae31be9932ba8464c80366559bd28424/main?recursive=true&expand=false
- [C7] https://datasets-server.huggingface.co/rows?dataset=CAiRE%2FASCEND&config=main&split=test&offset=0&length=100 （按 offset 每次100读取；validation同理）
- [C8] https://huggingface.co/datasets/DynamicSuperb/CodeSwitchingSpeechIdentification_ASCEND/blob/e44f58901a4e43719f949f8d61b1d6eb891d287d/README.md
- [C9] https://catalog.ldc.upenn.edu/LDC2015S04
- [C10] https://huggingface.co/datasets/AudioLLMs/seame_dev_man/blob/4a510d3e42acdf71a08441219b70fab0df9d255d/README.md ；https://huggingface.co/datasets/AudioLLMs/seame_dev_sge/blob/9b41e3e4b96ac23a05636b46a5792ed3ef54378b/README.md
