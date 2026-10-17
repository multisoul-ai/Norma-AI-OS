# 无损压缩实验汇总数据

结论与方法见[选型报告](../../2026-10-09-lossless-resource-compression-report.md)。本目录只保存报告与报表脚本仍需读取的汇总数据：

| 文件 | 内容 |
| --- | --- |
| `results.json` | 逐组命令、指标、哈希和输入来源 |
| `summary.json` / `summary.csv` | 按数据集与算法汇总；`summary.csv` 供 `scripts/report-package-size.py` 使用 |
| `dmg/results.json` | 模拟镜像对照 |
| `input-snapshot-provenance.json` | 冻结输入与等价性证据 |
| `Norma-无损压缩算法选型.xlsx` | 可筛选的结论表 |
| 其他顶层 JSON | 完成审计、测试结果与清理核对 |

逐轮日志、源码快照和临时实验记录已从当前文档树移除；需要复核历史实验时，从 Git 提交 `366284b4` 读取原目录。报告中的数值仍对应当时的冻结输入，后续资源版本需要重新测量。
