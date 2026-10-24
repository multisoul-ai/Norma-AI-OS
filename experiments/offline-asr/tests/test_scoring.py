"""评分公共接口的独立手算对照；不依赖模型或网络。"""

import unittest

from asr_lab.scoring import aggregate_pairs, score_pair


class ScoringTests(unittest.TestCase):
    def test_mixed_chinese_characters_and_english_words(self):
        """场景：中英混合改错，避免将英文按字母计分。

        构造：参考为六 token：把 / react / 组 / 件 / 打 / 开。
        执行：与末两字改成“关掉”的文本比较。
        预期：两次替换，MER=2/6；大小写和标点不增加错误。
        """
        result = score_pair("把 React 组件打开。", "把 REACT 组件关掉！")
        self.assertEqual(result.reference_tokens, 6, "React 应作为一个英文词")
        self.assertEqual(result.substitutions, 2, "打开→关掉是两次替换")
        self.assertEqual(result.deletions, 0, "该样本没有删除")
        self.assertEqual(result.insertions, 0, "该样本没有插入")
        self.assertAlmostEqual(result.error_rate, 1 / 3, msg="MER 应为 2/6")

    def test_corpus_micro_average_and_negative_false_output(self):
        """场景：长短句和负例混合时不能用句均值或丢弃幻觉。

        构造：1 token 错 1 个、9 token 全对，另有两个空参考。
        执行：汇总四条结果，其中一个空参考输出了文字。
        预期：正例 MER=1/10，负例误出字率=1/2；空参考无 MER。
        """
        result = aggregate_pairs([
            ("你", "他"), ("one two three four five six seven eight nine",
                          "one two three four five six seven eight nine"),
            ("", "phantom"), ("", ""),
        ])
        self.assertAlmostEqual(result["mer"], 0.1, msg="必须按参考token数加权")
        self.assertEqual(result["positive_samples"], 2, "负例不加入正例分母")
        self.assertEqual(result["negative_samples"], 2, "两个负例都应保留")
        self.assertEqual(result["false_output_rate"], 0.5, "不能丢弃幻觉样本")
        self.assertIsNone(score_pair("", "phantom").error_rate, "空参考不得除零或记零错")


if __name__ == "__main__":
    unittest.main()
