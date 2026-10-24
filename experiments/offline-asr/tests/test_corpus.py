"""公开语料抽样契约；只有元数据，没有模型输出。"""

import unittest

from asr_lab.corpus import select_rows, validate_disjoint


class CorpusTests(unittest.TestCase):
    def test_balanced_sampling_is_order_invariant_and_excludes_unknown(self):
        """场景：源文件顺序不应决定样本，难懂真值不可悄悄评分。

        构造：两位speaker各四条mixed，另有[UNK]、纯中文和超长样本。
        执行：按固定seed每人取二条，再反转源顺序重复。
        预期：同一ID集合、每人两条，排除项不入选且不补到其他speaker。
        """
        rows = [{"id": f"s{s}-{i}", "original_speaker_id": s, "language": "mixed",
                 "duration": 3.0, "transcription": "这是 test"}
                for s in (1, 2) for i in range(4)]
        rows += [{**rows[0], "id": "unknown", "transcription": "[UNK] test"},
                 {**rows[0], "id": "not-mixed", "transcription": "这是中文"},
                 {**rows[0], "id": "long", "duration": 30.0}]
        selected = select_rows(rows, per_speaker=2, language="mixed", seed="fixed")
        reverse = select_rows(list(reversed(rows)), per_speaker=2, language="mixed", seed="fixed")
        self.assertEqual([x["id"] for x in selected], [x["id"] for x in reverse], "顺序不改变抽样")
        self.assertEqual([x["original_speaker_id"] for x in selected], [1, 1, 2, 2], "每人固定配额")
        self.assertTrue(all(x["id"] not in ("unknown", "long", "not-mixed") for x in selected),
                        "不可评分或范围外样本不应入选")

    def test_identical_audio_across_splits_is_rejected(self):
        """场景：speaker不同但音频完全重复也属于泄漏。

        构造：dev/test不同speaker，音频SHA相同。
        执行：调用跨split验证。
        预期：明确拒绝audio leakage，不能只检查speaker。
        """
        with self.assertRaisesRegex(ValueError, "audio leakage", msg="重复音频必须拒绝"):
            validate_disjoint([{"speaker": "1", "sha256": "abc"}],
                              [{"speaker": "2", "sha256": "abc"}])


if __name__ == "__main__":
    unittest.main()
