import unittest
from report_helpers import failure_display


class ReportHelpersTests(unittest.TestCase):
    # 场景：工具缺失与解压超时；构造两个失败样本，生成表格标签，
    # 预期缺失不会虚构耗时，解压超时保留 0.2 秒实际压缩观测与未知 RSS。
    def test_failure_stage_and_observations(self):
        absent = failure_display([{'status': 'unavailable'}], 300)
        self.assertEqual(absent['mark'], '工具缺失', '缺失 CLI 应单独标注')
        self.assertEqual(absent['times'], ['未执行', '未执行'], '缺失 CLI 没有耗时')
        failed = failure_display([{'status': 'timeout',
                                  'compress': {'status': 'ok', 'seconds': .2, 'max_rss_bytes': 1048576},
                                  'decompress': {'status': 'timeout', 'seconds': 300.01, 'max_rss_bytes': None}}], 300)
        self.assertEqual(failed['mark'], '解压超时', '超时阶段应准确')
        self.assertEqual(failed['times'], ['0.200', '>300'], '保留实际压缩观测')
        self.assertEqual(failed['memory'], ['1.0', '未知'], 'RSS 缺失不能填零')


if __name__ == '__main__':
    unittest.main()
