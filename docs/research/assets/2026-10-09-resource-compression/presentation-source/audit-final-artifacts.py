"""以原始实验记录核对已导出的工作簿、正文表格及归档证据。"""

from collections import Counter
import hashlib
import json
from pathlib import Path
import statistics
import xml.etree.ElementTree as ET
import zipfile

root = Path.cwd()
destination = root / 'docs/research/assets/2026-10-09-resource-compression'
data = json.loads((destination / 'results.json').read_text())
dmg = json.loads((destination / 'dmg/results.json').read_text())
output = root / 'outputs/01a120ef-resource-compression'
workbook = output / 'Norma-无损压缩算法选型.xlsx'
assert data['source_unchanged'] and data['finished_utc'], '主实验未完成'
assert len(data['manifest']) == 30, '本次冻结输入应为 30 文件'
assert data['datasets']['resources']['source_bytes'] == sum(item['bytes'] for item in data['manifest']), '字节总数不一致'
assert dmg['source_unchanged'] and dmg['finished_utc'] and len(dmg['results']) == 4, 'DMG 未完成'
assert workbook.read_bytes() == (destination / workbook.name).read_bytes(), 'Excel 两份交付路径必须字节完全一致'
for label, live in [('initial', '2026-10-09-run1'), ('current', '2026-10-10-run2')]:
    assert (destination / 'runs' / label / 'results.json').read_bytes() == (root / 'build/compression-benchmark' / live / 'results.json').read_bytes(), '不得修改原始运行记录'
initial = json.loads((destination / 'runs/initial/results.json').read_text())
assert initial['source_unchanged'] is False, '首轮输入漂移的失败标志应保留'
manifest = {item['path']: item for item in data['manifest']}
assert all(manifest[item['path']] == item for item in initial['manifest']), '原有文件应全部保持相同'
assert len(data['manifest']) - len(initial['manifest']) == 1, '本次仅新增一张 PNG'
assert data['datasets']['video']['tar_sha256'] == initial['datasets']['video']['tar_sha256'], '复用视频必须归档字节相同'
probe = json.loads((destination / 'avrecode-probe/probe.json').read_text())
assert len(probe['results']) == 3 and probe['asan_enabled'], '专用工具探测证据不完整'
for sample in probe['results']:
    assert sample['source_bytes'] == manifest[sample['source']]['bytes'] and sample['source_sha256'] == manifest[sample['source']]['sha256'], '专用工具样本与当前视频字节不同'
    assert sample['status'] == 'error' and sample['compress']['exit_code'] == -6, '专用工具失败状态不符'
    log = (destination / 'avrecode-probe' / Path(sample['compress']['log']).name).read_text()
    assert 'h264_cabac.c:1789' in log and 'zero page' in log, '专用工具失败原因缺少原始日志'
for name, digest in json.loads((destination / 'source/sha256.json').read_text()).items():
    assert hashlib.sha256((destination / 'source' / name).read_bytes()).hexdigest() == digest, '归档源码哈希不一致：' + name
assert all(row['exit_code'] == 0 for row in json.loads((destination / 'test-results.json').read_text())), '聚焦测试未全部通过'
for sample in data['results']:
    if sample['status'] == 'ok':
        assert sample['verified_files'] == data['datasets'][sample['dataset']]['file_count'], '成功样本缺少成员核验'
    for phase in ['compress', 'decompress']:
        if phase in sample:
            path = destination / 'logs' / sample['dataset'] / sample['algorithm'] / str(sample['iteration']) / (phase + '.log')
            assert path.is_file(), '实验阶段缺少日志：' + str(path)

namespace = {'x': 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}
with zipfile.ZipFile(workbook) as archive:
    strings = []
    if 'xl/sharedStrings.xml' in archive.namelist():
        shared = ET.fromstring(archive.read('xl/sharedStrings.xml'))
        strings = [''.join(item.itertext()) for item in shared]
    sheets = [ET.fromstring(archive.read(f'xl/worksheets/sheet{i}.xml')) for i in [1, 2]]
    tables = [ET.fromstring(archive.read(name)) for name in archive.namelist() if name.startswith('xl/tables/') and name.endswith('.xml')]
    assert len(tables) == 4, '应包含主选型、DMG 及两张原始数据表'
    assert all(table.find('x:autoFilter', namespace) is not None for table in tables), '数据表应有原生筛选'
    cells = []
    for sheet in sheets:
        pane = sheet.find('x:sheetViews/x:sheetView/x:pane', namespace)
        assert pane is not None and float(pane.get('xSplit', '0')) == 2, '应冻结前两列'
        cells.append({cell.get('r'): cell for cell in sheet.findall('.//x:c', namespace)})
        assert not any(cell.get('t') == 'e' for cell in cells[-1].values()), '导出工作簿不得包含公式错误'

    def value(sheet, address):
        cell = cells[sheet].get(address)
        if cell is None:
            return None
        if cell.get('t') == 'inlineStr':
            return ''.join(cell.find('x:is', namespace).itertext())
        text = cell.findtext('x:v', default=None, namespaces=namespace)
        if text is None:
            return None
        if cell.get('t') == 's':
            return strings[int(text)]
        if cell.get('t') in {'str', 'b'}:
            return text if cell.get('t') == 'str' else text == '1'
        return float(text)

    def number(sheet, address, expected, tolerance=1e-7):
        actual = value(sheet, address)
        assert isinstance(actual, (int, float)) and abs(actual - expected) < tolerance, f'{address} 数字错误：{actual} / {expected}'

    assert float(sheets[0].find('x:sheetViews/x:sheetView/x:pane', namespace).get('ySplit', '0')) == 14, '选型表应冻结标题行'
    assert float(sheets[1].find('x:sheetViews/x:sheetView/x:pane', namespace).get('ySplit', '0')) == 6, '原始表应冻结标题行'
    names = {'resources': '完整资源包', 'video': '视频', 'images': '图像'}
    row = 15
    for group, name in names.items():
        eligible = {}
        for algorithm in data['configuration']['algorithms']:
            samples = [sample for sample in data['results'] if sample['dataset'] == group and sample['algorithm'] == algorithm]
            good = [sample for sample in samples if sample['status'] == 'ok']
            assert value(0, f'A{row}') == name and value(0, f'B{row}') == algorithm, '选型行对应错误'
            if good:
                size = statistics.median(sample['compressed_bytes'] for sample in good)
                expected = [size / 1e6, 1 - size / data['datasets'][group]['source_bytes']]
                expected += [statistics.median(sample[phase]['seconds'] for sample in good) for phase in ['compress', 'decompress']]
                expected += [max(sample[phase]['max_rss_bytes'] for sample in good) / 2**20 for phase in ['compress', 'decompress']]
                for column, metric in zip('CDEFGH', expected):
                    number(0, f'{column}{row}', metric)
                number(0, f'P{row}', max(sample['compressed_bytes'] for sample in good))
                if len(good) == data['configuration']['repeats'] and len(samples) == len(good) and algorithm != 'store':
                    eligible[algorithm] = (size, row, good)
            else:
                assert value(0, f'J{row}') is None and str(value(0, f'K{row}')).startswith('排除：'), '失败组合不得参与排名'
            row += 1
        winner = min(eligible, key=lambda key: eligible[key][0])
        for algorithm, (size, position, _) in eligible.items():
            number(0, f'J{position}', 1 + sum(candidate[0] < size for candidate in eligible.values()))
        position = 8 + list(names).index(group)
        assert value(0, f'C{position}') == winner, '摘要体积最优错误'
        for column, phase in [('E', 'compress'), ('F', 'decompress')]:
            fastest = min(eligible, key=lambda key: statistics.median(sample[phase]['seconds'] for sample in eligible[key][2]))
            assert value(0, f'{column}{position}') == fastest, '摘要速度最优错误'
        passed = all(sample['compressed_bytes'] * 2 <= data['datasets'][group]['source_bytes'] for sample in eligible[winner][2])
        assert value(0, f'G{position}') == ('达到' if passed else '未达到'), '50% 判定错误'
    for index, sample in enumerate(data['results'], start=7):
        assert value(1, f'A{index}') == names[sample['dataset']] and value(1, f'B{index}') == sample['algorithm'], '逐轮记录对应错误'
        assert value(1, f'D{index}') == sample['status'], '逐轮状态错误'
        metrics = {'C': sample['iteration'], 'E': data['datasets'][sample['dataset']]['source_bytes'],
                   'F': data['datasets'][sample['dataset']]['tar_bytes'], 'G': sample.get('compressed_bytes'),
                   'H': sample.get('compress', {}).get('seconds'), 'I': sample.get('decompress', {}).get('seconds'),
                   'J': sample.get('compress', {}).get('max_rss_bytes'), 'K': sample.get('decompress', {}).get('max_rss_bytes'),
                   'L': sample['verified_files'], 'M': data['datasets'][sample['dataset']]['file_count'],
                   'N': sample.get('decompress_and_verify_seconds')}
        for column, metric in metrics.items():
            if metric is None:
                assert value(1, f'{column}{index}') is None, '缺失的原始指标不得补零'
            else:
                number(1, f'{column}{index}', metric)
        assert value(1, f'O{index}') == sample.get('compressed_sha256'), '原始归档哈希错误'
        assert value(1, f'P{index}') == data['datasets'][sample['dataset']]['tar_sha256'], '统一输入哈希错误'
    for index, sample in enumerate(dmg['results'], start=57):
        number(0, f'C{index}', sample['bytes'] / 1e6)
        original = next(item for item in dmg['results'] if item['payload'] == 'plain' and item['format'] == sample['format'])
        number(0, f'D{index}', 1 - sample['bytes'] / original['bytes'])
        number(0, f'E{index}', sample['create']['seconds'])
        number(0, f'F{index}', sample['create']['max_rss_bytes'] / 2**20)
        number(0, f'G{index}', len(data['manifest']))

report = (root / 'docs/research/2026-10-09-lossless-resource-compression-report.md').read_text()
assert not any(text in report for text in ['状态：实验进行中', '待结果完成后', '待算法矩阵完成后']), '正文仍有草稿占位'
counts = Counter()
group = None
for line in report.splitlines():
    if line.startswith('## '):
        group = None
    if line.startswith('### '):
        group = next((key for key, name in names.items() if line == '### ' + name), None)
    if group and line.startswith('| '):
        fields = [field.strip() for field in line.split('|')[1:-1]]
        if fields[0] in data['configuration']['algorithms']:
            counts[group] += 1
            good = [sample for sample in data['results'] if sample['dataset'] == group
                    and sample['algorithm'] == fields[0] and sample['status'] == 'ok']
            assert fields[7] == f"{len(good)}/{data['configuration']['repeats']}", '正文核验轮数错误'
            if good:
                size = statistics.median(sample['compressed_bytes'] for sample in good)
                expected = [size / 1e6, 100 * (1 - size / data['datasets'][group]['source_bytes'])]
                expected += [statistics.median(sample[phase]['seconds'] for sample in good) for phase in ['compress', 'decompress']]
                expected += [max(sample[phase]['max_rss_bytes'] for sample in good) / 2**20 for phase in ['compress', 'decompress']]
                tolerances = [0.00000051, 0.000051, 0.00051, 0.00051, 0.051, 0.051]
                for text, metric, tolerance in zip(fields[1:7], expected, tolerances):
                    assert abs(float(text.rstrip('%')) - metric) <= tolerance, '正文表格指标与原始记录不一致：' + fields[0]
            else:
                assert fields[1:3] == ['未知', '未知'] and '排除' in fields[8], '正文失败行必须准确标注'
assert counts == {group: 12 for group in names}, '正文必须包含三个完整参数表'
audit = {'workbook': str(workbook), 'workbook_sha256': hashlib.sha256(workbook.read_bytes()).hexdigest(),
         'verified_comparison_rows': 36, 'verified_observation_rows': len(data['results']),
         'verified_dmg_rows': 4, 'source_files': len(data['manifest']),
         'live_source_matches_snapshot': data['live_source_matches_snapshot'], 'checks_passed': True}
(destination / 'final-artifact-audit.json').write_text(json.dumps(audit, ensure_ascii=False, indent=2) + '\n')
print(json.dumps(audit, ensure_ascii=False, indent=2))
