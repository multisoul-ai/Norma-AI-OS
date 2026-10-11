#!/usr/bin/env python3
"""核对冻结输入后合并补测结果，仅复用字节完全一致的视频实验。"""

import argparse
import copy
from datetime import datetime, timezone
import importlib.util
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'compression_benchmark', Path(__file__).with_name('benchmark-resource-compression.py'))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


def validate_group(data, group):
    expected = data['datasets'][group]['file_count']
    for algorithm in data['configuration']['algorithms']:
        samples = sorted([row for row in data['results']
                          if row['dataset'] == group and row['algorithm'] == algorithm],
                         key=lambda row: row['iteration'])
        if not samples or [row['iteration'] for row in samples] != list(range(1, len(samples) + 1)):
            raise ValueError('算法记录缺失或轮次异常：' + group + '/' + algorithm)
        failed = [row for row in samples if row['status'] != 'ok']
        if failed:
            if len(failed) != 1 or failed[0] is not samples[-1]:
                raise ValueError('失败后应停止此组合的重复实验')
        elif len(samples) != data['configuration']['repeats']:
            raise ValueError('成功组合没有完成指定重复轮次')
        if any(row['status'] == 'ok' and row['verified_files'] != expected for row in samples):
            raise ValueError('成功记录未核验全部文件：' + group + '/' + algorithm)


def merge_runs(previous, current, provenance):
    if not previous.get('finished_utc'):
        raise ValueError('旧实验尚未结束')
    if not current.get('finished_utc') or not current.get('source_unchanged'):
        raise ValueError('补测未完成或冻结输入已改变')
    if previous['configuration'] != current['configuration']:
        raise ValueError('两次实验配置不一致')
    if current['manifest'] != provenance['manifest'] or provenance['copied_files_verified'] != len(current['manifest']):
        raise ValueError('补测清单与复制验证证据不一致')
    old_video = bench.datasets(previous['manifest'])['video']
    new_groups = bench.datasets(current['manifest'])
    if old_video != new_groups['video']:
        raise ValueError('视频路径、数量、长度或 SHA-256 已改变')
    if (not provenance.get('video_matches_previous_run')
            or provenance['reused_video_tar_sha256'] != previous['datasets']['video']['tar_sha256']):
        raise ValueError('当前视频 tar 与旧实验输入不一致')
    for key in ['platform', 'cpu', 'memory_bytes', 'cpu_count']:
        if previous['environment'].get(key) != current['environment'].get(key):
            raise ValueError('硬件或系统环境不一致：' + key)
    if set(previous['environment']['tools']) != set(current['environment']['tools']):
        raise ValueError('工具集合不一致')
    for tool, old in previous['environment']['tools'].items():
        new = current['environment']['tools'][tool]
        first_line = lambda value: (value or '').splitlines()[:1]
        if old['path'] != new['path'] or first_line(old['version']) != first_line(new['version']):
            raise ValueError('工具路径或版本不一致：' + tool)
    if set(current['datasets']) != {'images', 'resources'}:
        raise ValueError('补测必须覆盖新的图像与完整资源组')
    for group in ['images', 'resources']:
        info = current['datasets'][group]
        if info['file_count'] != len(new_groups[group]) or info['source_bytes'] != sum(item['bytes'] for item in new_groups[group]):
            raise ValueError('补测数据集计数或字节数不一致')
        validate_group(current, group)
    validate_group(previous, 'video')
    result = copy.deepcopy(current)
    result['datasets'] = {'video': copy.deepcopy(previous['datasets']['video']), **result['datasets']}
    result['results'] = copy.deepcopy([row for row in previous['results'] if row['dataset'] == 'video']) + result['results']
    result['source_snapshot'] = copy.deepcopy(provenance)
    result['started_utc'] = previous['environment']['time_utc']
    result['source_unchanged_scope'] = '冻结的当前输入在补测前后不变；复用视频与当前清单及 tar 字节完全一致'
    result['previous_run_input_drift'] = {
        'source_unchanged': previous['source_unchanged'],
        'added_paths': sorted(set(item['path'] for item in current['manifest'])
                              - set(item['path'] for item in previous['manifest'])),
    }
    result['dataset_provenance'] = {
        'video': {'run_id': 'previous', 'environment': copy.deepcopy(previous['environment'])},
        'images': {'run_id': 'current', 'environment': copy.deepcopy(current['environment'])},
        'resources': {'run_id': 'current', 'environment': copy.deepcopy(current['environment'])},
    }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--previous', type=Path, required=True)
    parser.add_argument('--current', type=Path, required=True)
    parser.add_argument('--snapshot-provenance', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    previous = json.loads(args.previous.read_text())
    current = json.loads(args.current.read_text())
    provenance = json.loads(args.snapshot_provenance.read_text())
    snapshot = Path(provenance['snapshot_root'])
    if bench.inventory(snapshot) != provenance['manifest']:
        parser.error('冻结快照的文件已改变')
    if (args.output / 'results.json').exists():
        parser.error('合并目录已有结果，请使用新目录')
    data = merge_runs(previous, current, provenance)
    data['dataset_provenance']['video']['results_file'] = str(args.previous.resolve())
    for group in ['images', 'resources']:
        data['dataset_provenance'][group]['results_file'] = str(args.current.resolve())
    data['live_source_matches_snapshot'] = bench.inventory(Path(provenance['source_root'])) == provenance['manifest']
    data['assembled_utc'] = datetime.now(timezone.utc).isoformat()
    args.output.mkdir(parents=True, exist_ok=True)
    bench.save(data, args.output)
    bench.write_summary(data, args.output)
    print('已核对并合并三个当前输入数据集：', args.output / 'summary.csv')


if __name__ == '__main__':
    main()
