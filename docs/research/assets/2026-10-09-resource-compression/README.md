# 无损压缩实验数据

正文见 [选型报告](../../2026-10-09-lossless-resource-compression-report.md)。数据集为当前打包媒体目录，原文件还原以路径、数量、长度与逐文件 SHA-256 为准。

| 文件 / 目录 | 内容 |
|---|---|
| `results.json` | 主实验环境、manifest、统一 tar 哈希、逐轮命令、耗时、峰值 RSS、还原结果 |
| `summary.json` / `summary.csv` | 按数据集与算法汇总；耗时中位数、范围、最大峰值 RSS、样本数及 50% 判定 |
| `logs/` | 主实验逐组逐轮的压缩、解压与计时日志 |
| `runs/initial/` / `runs/current/` | 原始两次实验及日志，保留首轮 `source_unchanged=false`；主表只使用已核验等价的视频与当前快照图像、完整包 |
| `input-snapshot-provenance.json` | 30 文件冻结输入、Git revision、逐文件复制验证及视频 tar 等价证据 |
| `dmg/results.json` | 相同程序快照和媒体资源的 UDZO / ULMO、原资源 / 预压缩资源四组模拟镜像对照 |
| `dmg/*.log` | 创建、镜像验证、只读挂载、资源恢复与卸载日志 |
| `avrecode-probe/` | 固定源码版本、编译日志、C++17 / libc++ 兼容补丁、H.264 盘点与三个真实样本崩溃日志 |
| `integration-costs.json` | 本机解码相关动态库文件大小；不是实测 App 体积增量 |
| `source/` | 此次使用的 CLI 与测试源码快照、SHA-256 清单 |
| `presentation-source/` | 最终 Markdown / Excel 生成器、边界测试与独立产物核对源码及哈希 |
| `test-results.json` / `test-*.log` | 对归档源码快照执行聚焦测试的退出状态与原始输出 |
| `cli-smoke/` | 12 组 CLI 的单文件小样本往返，验证工具可运行；不计入真实资源压缩排名 |
| `Norma-无损压缩算法选型.xlsx` | 汇总选型、可筛选排序的指标表和逐轮观测 |
| `final-artifact-audit.json` | 最终导出工作簿与原始指标、公式缓存、筛选、冻结窗格及证据完整性的核对结果 |
| `cleanup-verification.json` | 原资源不变、实验镜像全部卸载与大体积临时副本清理核对 |
| `goal-completion-audit.json` | 按目标、报告交付和实验协议逐项记录的完成证据 |

体积使用 bytes 或十进制 MB（1 MB = 1,000,000 bytes），内存展示使用 MiB（1 MiB = 1,048,576 bytes）。原始 RSS 字段的单位为 bytes。

`configuration.threads` 记录压缩端的单线程策略。7zz 解压命令未显式限定线程，使用默认设置；各阶段实际配置以 `argv` 为准。

JSON 内的绝对临时路径保留当时执行上下文。查阅日志时，主实验按 `logs/<dataset>/<algorithm>/<iteration>/` 定位；DMG 与专用工具按同名日志定位。临时归档、还原副本和模拟镜像由实验入口清理。

最终已核对两张工作表的四个预览区域，调整相邻数字与文字的对齐，确认指标和最优标注可读。冻结输入的完整副本与专用工具临时构建目录已清理；manifest、输入版本、命令、失败日志与源代码快照保留。复现时须使用 manifest 对应的资源版本，并将归档源码 CLI 的 `--root` 指向该资源根目录。

主表依据 2026-10-10 冻结的 30 文件输入。首轮新增 PNG 导致输入检查失败，原记录不修改；图像和完整包全量补测，视频仅在成员与固定 tar 完全等价后复用。主 JSON 的 `dataset_provenance` 记录各组来源，`source_unchanged_scope` 说明冻结输入完整性，`live_source_matches_snapshot` 单独记录合并时的仓库资源状态。

合并数据必须具备 `finished_utc`、通过冻结输入和等价性检查；所选算法须在相应数据集三轮全部逐字节还原通过。超时或不完整组合不参与最优排名；安装后的资源占用恢复为原体积。纯解压耗时只恢复一个 tar，不包含各成员写入安装缓存或 SHA-256 核验。
