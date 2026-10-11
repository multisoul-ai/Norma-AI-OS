#!/usr/bin/env python3
"""资源快照变化后，仅复用相同视频输入的实验结果。"""

import copy
import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).with_name('merge-resource-compression-results.py')
spec = importlib.util.spec_from_file_location('merge_compression', SCRIPT)
merge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(merge)


class MergeCompressionTests(unittest.TestCase):
    # 场景：旧实验期间只新增 PNG，视频成员与统一 tar 保持一致；构造旧视频三轮、
    # 新图像/资源各三轮，执行合并；预期只复用旧视频，保留原实验的漂移标志。
    def test_reuses_only_identical_video_and_keeps_drift_evidence(self):
        previous, current, provenance = self.fixture()
        result = merge.merge_runs(previous, current, provenance)
        self.assertEqual(len(result['results']), 9, '合并结果应含三个数据集各三轮')
        self.assertEqual(result['manifest'], current['manifest'], '应使用含新 PNG 的清单')
        self.assertEqual(result['datasets']['video'], previous['datasets']['video'],
                         '视频输入元数据只能来自已核对的旧实验')
        self.assertEqual(result['dataset_provenance']['video']['run_id'], 'previous',
                         '视频必须标记真实来源')
        self.assertEqual(result['dataset_provenance']['images']['run_id'], 'current',
                         '图像不得使用不含 PNG 的旧实验')
        self.assertFalse(previous['source_unchanged'], '不得修改原始漂移证据')
        self.assertFalse(result['previous_run_input_drift']['source_unchanged'],
                         '合并报告应保留旧实验的全局漂移标志')
        self.assertTrue(result['source_unchanged'], '当前冻结输入与视频等价性应成立')

    # 场景：视频长度相同但哈希变化；修改新清单的视频 SHA 并保留伪造的匹配标志，
    # 执行合并；预期拒绝，不能只信任布尔字段或长度。
    def test_rejects_changed_video_bytes(self):
        previous, current, provenance = self.fixture()
        current['manifest'][0]['sha256'] = 'd' * 64
        provenance['manifest'] = copy.deepcopy(current['manifest'])
        with self.assertRaises(ValueError, msg='等长视频篡改必须阻止复用'):
            merge.merge_runs(previous, current, provenance)

    # 场景：视频文件清单相同，但新快照的统一 tar 哈希不一致；修改证明字段，
    # 执行合并；预期拒绝，保证实际算法输入的字节等价。
    def test_rejects_different_video_tar(self):
        previous, current, provenance = self.fixture()
        provenance['reused_video_tar_sha256'] = 'e' * 64
        with self.assertRaises(ValueError, msg='统一 tar 不一致不得复用'):
            merge.merge_runs(previous, current, provenance)

    # 场景：补测尚未完成、快照变化、配置或工具版本不同；分别更改四种条件，
    # 执行合并；预期全部拒绝，不将不同协议的指标混在同一报告。
    def test_rejects_incomplete_or_incompatible_current_run(self):
        for condition in ['unfinished', 'changed', 'config', 'version']:
            previous, current, provenance = self.fixture()
            if condition == 'unfinished':
                current.pop('finished_utc')
            elif condition == 'changed':
                current['source_unchanged'] = False
            elif condition == 'config':
                current['configuration']['repeats'] = 1
            else:
                current['environment']['tools']['gzip']['version'] = 'gzip 2.0'
            with self.subTest(condition=condition):
                with self.assertRaises(ValueError, msg='补测完整性或协议异常必须拒绝'):
                    merge.merge_runs(previous, current, provenance)

    # 场景：视频只成功一轮而没有失败记录，或成功记录的核验文件数不足；分别
    # 删掉两轮、更改核验数，执行合并；预期拒绝不完整或未覆盖完整视频的证据。
    def test_rejects_missing_or_unverified_video_samples(self):
        for condition in ['missing', 'unverified']:
            previous, current, provenance = self.fixture()
            if condition == 'missing':
                previous['results'] = previous['results'][:1]
            else:
                previous['results'][0]['verified_files'] = 0
            with self.subTest(condition=condition):
                with self.assertRaises(ValueError, msg='缺轮次或缺完整核验的记录不能复用'):
                    merge.merge_runs(previous, current, provenance)

    @staticmethod
    def fixture():
        manifest = [
            {'path': 'crates/app/assets/wallpapers/v.mp4', 'bytes': 100, 'sha256': 'a' * 64},
            {'path': 'crates/app/assets/wallpapers/i.jpg', 'bytes': 20, 'sha256': 'b' * 64},
        ]
        video = {'file_count': 1, 'source_bytes': 100, 'tar_bytes': 10240, 'tar_sha256': '1' * 64}
        environment = {'time_utc': '2026-10-09T00:00:00+00:00',
                       'tools': {'gzip': {'path': '/bin/gzip', 'version': 'gzip 1.0'}}}
        config = {'algorithms': ['gzip-9'], 'repeats': 3, 'timeout_seconds': 300, 'threads': 1}
        def rows(group, count):
            return [{'dataset': group, 'algorithm': 'gzip-9', 'iteration': i,
                     'status': 'ok', 'verified_files': count} for i in range(1, 4)]
        previous = {'environment': environment, 'configuration': config, 'manifest': manifest,
                    'datasets': {'video': video}, 'results': rows('video', 1),
                    'source_unchanged': False, 'finished_utc': '2026-10-09T01:00:00+00:00'}
        current = copy.deepcopy(previous)
        current['manifest'].append({'path': 'crates/app/assets/wallpapers/new.png',
                                    'bytes': 10, 'sha256': 'c' * 64})
        current['datasets'] = {
            'images': {'file_count': 2, 'source_bytes': 30, 'tar_bytes': 10240, 'tar_sha256': '2' * 64},
            'resources': {'file_count': 3, 'source_bytes': 130, 'tar_bytes': 10240, 'tar_sha256': '3' * 64},
        }
        current['results'] = rows('images', 2) + rows('resources', 3)
        current['source_unchanged'] = True
        current['finished_utc'] = '2026-10-09T02:00:00+00:00'
        provenance = {'manifest': copy.deepcopy(current['manifest']),
                      'reused_video_tar_sha256': '1' * 64, 'copied_files_verified': 3,
                      'video_matches_previous_run': True}
        return previous, current, provenance


if __name__ == '__main__':
    unittest.main()
