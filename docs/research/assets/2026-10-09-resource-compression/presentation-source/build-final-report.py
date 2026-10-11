import json
from pathlib import Path
import statistics
from collections import Counter
from datetime import datetime
from zoneinfo import ZoneInfo
from report_helpers import failure_display

base = Path('build/compression-benchmark')
data = json.loads((base / '2026-10-10-current/results.json').read_text())
dmg = json.loads((base / 'dmg-current/results.json').read_text())
costs = json.loads((base / 'report-authoring/integration-costs.json').read_text())
assert data.get('finished_utc') and data.get('source_unchanged'), '矩阵未完成或源文件改变'
assert dmg.get('finished_utc') and dmg.get('source_unchanged') and len(dmg['results']) == 4, 'DMG 尚未完成'
report = Path('docs/research/2026-10-09-lossless-resource-compression-report.md')
old = report.read_text()
special = old[old.index('## H.264 专用重压缩工具'):old.index('## DMG 对照')]
primary = old[old.index('## 一手资料'):]
names = {'resources': '完整资源包', 'video': '视频', 'images': '图像'}
repeats = data['configuration']['repeats']
summaries = {}
for group in names:
    summaries[group] = {}
    for algorithm in data['configuration']['algorithms']:
        samples = [s for s in data['results'] if s['dataset'] == group and s['algorithm'] == algorithm]
        valid = [s for s in samples if s['status'] == 'ok']
        row = {'samples': samples, 'valid': valid, 'complete': len(valid) == repeats and len(samples) == repeats}
        if valid:
            row['bytes'] = statistics.median(s['compressed_bytes'] for s in valid)
            row['saving'] = 100 * (1 - row['bytes'] / data['datasets'][group]['source_bytes'])
            for phase in ['compress', 'decompress']:
                row[phase] = statistics.median(s[phase]['seconds'] for s in valid)
                row[phase + '_rss'] = max(s[phase]['max_rss_bytes'] for s in valid)
        summaries[group][algorithm] = row

def best(group, metric):
    eligible = [(row[metric], algorithm) for algorithm, row in summaries[group].items()
                if row['complete'] and algorithm != 'store']
    return min(eligible)[1]

started_local = datetime.fromisoformat(data.get('started_utc', data['environment']['time_utc'])).astimezone(ZoneInfo('Asia/Shanghai'))
finished_local = datetime.fromisoformat(data['finished_utc']).astimezone(ZoneInfo('Asia/Shanghai'))
lines = ['# Norma OS 媒体资源无损压缩选型报告', '',
         f"算法实验：{started_local:%Y-%m-%d %H:%M} 至 {finished_local:%Y-%m-%d %H:%M}（Asia/Shanghai）；原始记录使用 UTC。", '',
         '本次测试没有找到能将现有视频或完整媒体资源缩小至少 50% 的方案。所有成功样本均逐文件还原原始字节，并通过路径、数量、长度与 SHA-256 核验。', '',
         '[下载 Excel 选型表](assets/2026-10-09-resource-compression/Norma-无损压缩算法选型.xlsx)；[汇总 CSV](assets/2026-10-09-resource-compression/summary.csv)；[逐轮原始数据](assets/2026-10-09-resource-compression/results.json)。', '',
         '表格区分体积最优、速度最优与安装方案。排名限定在阶段 300 秒期限内、三轮全部还原通过的候选；超时或不完整组合不参与排名。指标仅统计成功轮次，耗时取中位数，峰值 RSS 取最大值；样本数逐行列出。体积 MB 使用十进制，内存 MiB 使用二进制。', '',
         '## 最优算法与推荐', '',
         '| 数据范围 | 原始 MB | 体积最小算法 | 压缩 MB | 净节省 | 最快压缩 | 最快解压 | 50% 目标 |',
         '|---|---:|---|---:|---:|---|---|---|']
for group, name in names.items():
    algorithm = best(group, 'bytes')
    row = summaries[group][algorithm]
    target = all(s['compressed_bytes'] * 2 <= data['datasets'][group]['source_bytes'] for s in row['valid'])
    lines.append(f"| {name} | {data['datasets'][group]['source_bytes']/1e6:.6f} | **{algorithm}** | {row['bytes']/1e6:.6f} | {row['saving']:.4f}% | {best(group,'compress')} | {best(group,'decompress')} | {'达到' if target else '未达到'} |")
winner = best('resources', 'bytes')
zstd = summaries['resources']['zstd-19']
resource_best = summaries['resources'][winner]
lines.extend(['', '**综合安装方案：保留原媒体资源，优先评估镜像层的 ULMO 压缩。** 本次资源预压缩收益不足以支撑“节省 50%”目标；额外的解码依赖、后台解包与缓存管理需要另行实现。ULMO 仅在本机核验，采用前需在项目最低支持的 macOS 12 上验证挂载。', '',
              f"**如果产品仍需要独立资源归档，综合候选为 Zstd 19。** 完整资源产物为 {zstd['bytes']:,} bytes，比体积最小的 {winner} 只大 {zstd['bytes']-resource_best['bytes']:,} bytes；压缩 {zstd['compress']:.3f} 秒、解压 {zstd['decompress']:.3f} 秒、解压峰值 {zstd['decompress_rss']/2**20:.1f} MiB。这是后续集成候选，当前 App 未添加该解码器。", '',
              f"视频组体积最小的 {best('video','bytes')} 仍保留约 {100-summaries['video'][best('video','bytes')]['saving']:.3f}% 原体积。图像组最小的 {best('images','bytes')} 解压需 {summaries['images'][best('images','bytes')]['decompress']:.3f} 秒，不适合仅为少量安装包收益增加首用等待。", '',
              '## 资源分布', '',
              '范围为 Packager.toml 打包的 wallpapers 与 feature-guide 两个目录；不含主程序、其他 sidecar、图标、Sparkle、浏览器扩展或外置语音模型。媒体节省率不能直接称为整个 DMG 的节省率。', '',
              '| 类型 | 文件数 | 字节数 | MB | 占媒体资源 |', '|---|---:|---:|---:|---:|'])
count, byte_count = Counter(), Counter()
for item in data['manifest']:
    suffix = Path(item['path']).suffix.lower()
    count[suffix] += 1
    byte_count[suffix] += item['bytes']
total = data['datasets']['resources']['source_bytes']
for suffix in ['.mp4', '.gif', '.webp', '.jpg', '.png', '.md']:
    lines.append(f"| {suffix} | {count[suffix]} | {byte_count[suffix]:,} | {byte_count[suffix]/1e6:.6f} | {100*byte_count[suffix]/total:.2f}% |")
directories = Counter()
for item in data['manifest']:
    directories[Path(item['path']).relative_to('crates/app/assets').parts[0]] += item['bytes']
directory_text = '，'.join(f'{key} 为 {value:,} bytes（{100*value/total:.2f}%）' for key, value in directories.items())
lines.extend([f"| 合计 | {len(data['manifest'])} | {total:,} | {total/1e6:.6f} | 100.00% |", '',
              directory_text + '。上述“图像”组包含 GIF；完整资源组还包含一个说明文档，三组之间有重叠，不相加。', '',
              '## 输入版本与补测', '',
              f"本报告基于 {data['source_snapshot']['captured_utc']} 冻结的 {len(data['manifest'])} 文件快照，Git revision `{data['source_snapshot']['git_revision']}`。首轮完整实验结束时发现新增 desktop-earth.png；原有 29 个文件内容均未变化，原始 source_unchanged=false 的记录原样保留。", '',
              '图像和完整资源包按包含新增 PNG 的冻结输入重新运行全部参数与三轮。视频成员的路径、长度、SHA-256，以及统一 tar 的 SHA-256 与原实验完全一致，因此仅复用原视频三轮；两个时期使用相同算法配置、工具版本、系统和硬件，各数据集的真实来源在 dataset_provenance 中记录。旧图像和旧完整包不进入当前排名。', '',
              'source_unchanged 对合并数据表示冻结输入前后完整性与视频等价性成立；live_source_matches_snapshot 另记组装报告时仓库资源是否仍与快照一致。后续资源版本需要重新测量，不把不同版本的字节数混在同一组。', '',
              '## 各算法实测表', '', '标注只在三轮全部还原成功的编解码器之间比较；store 是固定 tar 与复制基线，不参与算法速度排名。超时不推断体积或解压指标。', ''])
for group, name in names.items():
    lines.extend(['### ' + name, '',
                  '| 算法参数 | 压缩 MB | 净节省 | 压缩秒 | 解压秒 | 压缩峰值 MiB | 解压峰值 MiB | 核验轮数 | 标注 |',
                  '|---|---:|---:|---:|---:|---:|---:|---|---|'])
    for algorithm in data['configuration']['algorithms']:
        row = summaries[group][algorithm]
        if not row['valid']:
            failure = failure_display(row['samples'], data['configuration']['timeout_seconds'])
            metrics = ' | '.join(failure['times'] + failure['memory'])
            lines.append(f"| {algorithm} | 未知 | 未知 | {metrics} | 0/{repeats} | {failure['mark']}，排除 |")
            continue
        flags = []
        if algorithm == 'store':
            flags.append('归档基线')
        elif row['complete']:
            for metric, label in [('bytes','体积最优'), ('compress','最快压缩'), ('decompress','最快解压')]:
                if best(group, metric) == algorithm:
                    flags.append('**' + label + '**')
            if group == 'resources' and algorithm == 'zstd-19':
                flags.append('**综合归档候选**')
        else:
            flags.append(failure_display(row['samples'], data['configuration']['timeout_seconds'])['mark'] + '，轮次不完整，排除')
        lines.append(f"| {algorithm} | {row['bytes']/1e6:.6f} | {row['saving']:.4f}% | {row['compress']:.3f} | {row['decompress']:.3f} | {row['compress_rss']/2**20:.1f} | {row['decompress_rss']/2**20:.1f} | {len(row['valid'])}/{repeats} | {'；'.join(flags) or '已验证'} |")
    lines.append('')
lines.extend(['## 方法与可比性', '',
              'Apple M1，8 GiB 内存，8 核，macOS 27.0.1 arm64；CLI 压缩单线程，各参数与数据集三轮，串行运行并轮换算法起始顺序。每阶段超时 300 秒；组合第一次失败后跳过剩余重复，故失败组不是三次独立超时。7zz 解压命令未指定线程数，使用默认设置；解压指标代表所记录 CLI 配置，不是全算法统一解压线程的基准。完整 argv 以原始 JSON 为准。', '',
              '相同数据集均压缩同一个 USTAR：仓库相对路径、稳定排序、mtime=0、UID/GID=0、mode=0644。普通文件路径、数量、长度与内容为还原标准，未包含扩展属性。成功样本先核对整个 tar 的 SHA-256，再读取各成员字节核验，拒绝额外、缺失、重复、越界或非普通文件。纯解压计时只恢复单个 tar，不包含逐文件写入安装缓存或哈希核验；原始 JSON 另含解压及完整成员核验的总耗时。', '',
              '耗时为带文件 IO 的墙钟时间，文件缓存可能温热，未清空系统缓存；接近的耗时应结合原始各轮范围判断。RSS 来自 macOS /usr/bin/time -l 的编解码子进程。没有实测正式 App 后台解包、冷启动或 GPUI 帧率，CLI 数字不代表这些流程的运行保证。', '',
              '净节省率 = 100 × (1 - 压缩产物 bytes / 原文件总 bytes)，计入归档和压缩容器开销；负值保留为膨胀。50% 达标按全部成功轮次的产物 bytes × 2 ≤ 原文件总 bytes 判定，失败组排除。summary.csv 另保留相对于统一 tar 的算法节省率、吞吐、耗时范围及样本数。', '',
              '| 工具 | 版本 |', '|---|---|'])
for tool in ['gzip', 'bzip2', 'xz', 'zstd', 'brotli', '7zz', 'zpaq']:
    version = (data['environment']['tools'][tool]['version'] or '工具缺失 / 版本未知').splitlines()[0]
    lines.append(f'| {tool} | {version} |')
lines.extend(['', special.rstrip(), '', '## DMG 对照', '',
              f"四组均使用同一本机现有程序文件快照（{dmg['program']['bytes']:,} bytes，mtime {dmg['program']['mtime_utc']}）和相同资源清单，生成独立模拟 App 载荷；这不是当前正式发布构建。程序 SHA-256：`{dmg['program']['sha256']}`。", '',
              f"原资源与 {dmg['resource_algorithm']} 预压缩资源分别使用 UDZO（zlib-level=9）和 ULMO；压缩载荷只含归档，没有同时携带原资源。四组各测一次，均通过 hdiutil verify、只读挂载与载荷 SHA-256；从压缩镜像恢复后又核验全部 {len(data['manifest'])} 个资源文件。", '',
              'DMG 制作峰值来自 hdiutil CLI 的 /usr/bin/time -l 记录，不代表镜像服务、内核缓存或整个打包流程的总峰值内存。', '',
              '| 资源载荷 | 镜像格式 | DMG bytes | DMG MB | 相对同格式原资源节省 | 制作秒 | 制作峰值 MiB | 核验 |', '|---|---|---:|---:|---:|---:|---:|---|'])
for row in dmg['results']:
    original = next(s for s in dmg['results'] if s['format'] == row['format'] and s['payload'] == 'plain')
    saving = 100 * (1 - row['bytes'] / original['bytes'])
    lines.append(f"| {'原资源' if row['payload']=='plain' else dmg['resource_algorithm']} | {row['format']} | {row['bytes']:,} | {row['bytes']/1e6:.6f} | {saving:.4f}% | {row['create']['seconds']:.3f} | {row['create']['max_rss_bytes']/2**20:.1f} | 镜像与资源通过 |")
plain = {row['format']: row['bytes'] for row in dmg['results'] if row['payload'] == 'plain'}
lines.extend(['', f"固定模拟载荷只将 UDZO 改为 ULMO，DMG 缩小 {plain['UDZO']-plain['ULMO']:,} bytes（{100*(1-plain['ULMO']/plain['UDZO']):.4f}%）。资源预压缩与镜像层压缩的收益不能相加。上述差值来自一次镜像实验，包含文件系统 metadata；不作为百字节级算法优劣的证据。", '',
              '## 集成成本与安装占用', '',
              '| 方案 | 当前工程情况 | 后续工作 / 成本 | 本次选择 |', '|---|---|---|---|',
              '| 原资源 + ULMO | 无需媒体解码依赖 | 修改镜像格式前需验证最低支持系统与正式 release 完整性 | 综合安装方案候选 |',
              '| Zstd 19 | 未声明直接媒体解包依赖 | 引入解码库、完整性清单、后台解包和原子缓存；完整资源解压内存较低 | 综合归档候选 |',
              f'| {winner} | 当前未实现资源归档解码 | 压缩更慢，需对应解码器与缓存流程 | 完整资源体积最小 |',
              '| Zstd 22 | 相比 19 的字节收益很小 | 本次编码和解码峰值明显增加 | 不推荐默认极限等级 |',
              '| bzip2 | model_store/asr 已依赖 bzip2 与 tar | 现有模型安装器不等于已支持壁纸解包；本次收益和速度均不足 | 不选 |',
              '| gzip / xz / 7z | 需确定库与容器读取方案 | gzip 收益低；xz/7z 部分组合耗时或内存高；PPMd 对视频会膨胀 | 不选 |',
              '| ZPAQ | 新解码与容器维护路径 | 视频和完整包 300 秒超时；图像解压慢、内存高 | 不选 |',
              '| avrecode / losslessh264 / Precomp | 未用于当前 App | 本次 avrecode 构建的原视频试验崩溃，其余格式不匹配，缺少可验证结果 | 不进入推荐 |', '',
              '解码器增量 App 体积未测，因为本任务未制作正式集成分支。作为库体积参考，本机 Homebrew arm64 文件实测如下，不包含链接、裁剪、签名等影响，不能直接当作安装包增量：', '',
              '| 本机库文件 | bytes | 说明 |', '|---|---:|---|',
              f"| libzstd | {costs['libraries']['libzstd']['bytes']:,} | 包含编解码能力 |",
              f"| libbrotlidec + libbrotlicommon | {costs['libraries']['libbrotlidec']['bytes']+costs['libraries']['libbrotlicommon']['bytes']:,} | 解码侧两库合计 |",
              f"| liblzma | {costs['libraries']['liblzma']['bytes']:,} | 库文件大小 |", '',
              'Zstd 为 BSD，Brotli 为 MIT；xz 区分 liblzma 与 CLI，7-Zip 区分 LGPL、BSD 与 unRAR 相关部分。正式采用时按实际库与固定版本核对许可，实验 CLI 不直接进入 App。对应一手许可链接见文末。', '',
              f"安装后恢复的媒体仍占 {total:,} bytes（{total/1e6:.6f} MB）。如果压缩包随签名 App 保留，另在用户目录缓存原文件，按本次最小完整验证归档估算，媒体相关总占用约 {total+resource_best['bytes']:,} bytes（{(total+resource_best['bytes'])/1e6:.6f} MB），还未计文件系统和缓存 metadata；不能把下载体积收益称为安装后空间收益。", '',
              '## 复现与证据', '',
              '执行计划见 [Goal 计划](../exec-plans/2026-10-09-lossless-resource-compression-goal.md)。', '',
              '```bash', 'HOMEBREW_NO_AUTO_UPDATE=1 brew install zstd xz brotli sevenzip zpaq',
              'python3 scripts/test-resource-compression-benchmark.py', 'python3 scripts/test-resource-compression-dmg.py',
              'python3 scripts/test-merge-resource-compression-results.py',
              'python3 scripts/benchmark-resource-compression.py \\', '  --output build/compression-benchmark/reproduction \\', '  --repeats 3 --timeout 300',
              'python3 scripts/benchmark-resource-compression-dmg.py \\', '  --results build/compression-benchmark/reproduction/results.json \\', '  --output build/compression-benchmark/reproduction-dmg', '```', '',
              '系统须有 gzip、bzip2、hdiutil 与 Python 3；源文件以 manifest 为准。输出目录不能已有 results.json，也不能放进原资源目录。算法缺失记录为 unavailable，不自动安装。DMG 入口默认程序为 target/release/norma-os；没有该历史程序时需通过 --program 指定固定程序快照，不要求重新构建正式 App。', '',
              '[实验 JSON](assets/2026-10-09-resource-compression/results.json)、[汇总 JSON](assets/2026-10-09-resource-compression/summary.json)、[DMG JSON](assets/2026-10-09-resource-compression/dmg/results.json) 保存工具版本、原文件清单、逐轮命令、耗时、哈希与 RSS。日志和专用工具失败证据随目录保存，原数据内绝对临时路径是当时执行上下文，查看保存副本时按同名日志定位。', '',
              '临时归档、还原副本与模拟 DMG 已由实验入口清理；原始资源与正式打包流程未修改。', '', primary.rstrip(), ''])
report.write_text('\n'.join(lines))
print(report, '已生成最终实测报告')
