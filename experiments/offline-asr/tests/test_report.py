"""报告必须保留失败分母，不将加速回放伪装实时延迟。"""

import unittest

from asr_lab.report import summarize_results


class ReportTests(unittest.TestCase):
    def test_failure_is_in_accuracy_and_latency_denominators(self):
        """场景：一个成功和一个失败时，不能仅报告成功样本100%准确。

        构造：你好识别成功，再见失败且hyp为空；各有2个中文字符。
        执行：汇总所有请求，其中仅成功请求有12ms收尾数据。
        预期：MER=2/4，失败1/2，延迟n=1且分母=2，明确有缺失。
        """
        result = summarize_results([
            {"reference": "你好", "hypothesis": "你好", "language": "zh", "status": "ok",
             "realtime": True, "after_file_end_ms": 12},
            {"reference": "再见", "hypothesis": "", "language": "zh", "status": "failed",
             "realtime": True, "after_file_end_ms": None},
        ])
        self.assertEqual(result["quality"]["mer"], 0.5, "失败结果应产生删除错误")
        self.assertEqual(result["failed_samples"], 1, "失败样本必须保留")
        self.assertEqual(result["samples"], 2, "请求总数不得缩水")
        latency = result["latency"]["after_file_end_ms"]
        self.assertEqual(latency["measured_samples"], 1, "只存在一个有效延迟")
        self.assertEqual(latency["missing_samples"], 1, "必须明示缺测分母")
        self.assertEqual(latency["p95"], 12, "单样本分位数不能冒充更多样本")


if __name__ == "__main__":
    unittest.main()
