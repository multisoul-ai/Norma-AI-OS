"""实验计时公共接口，模拟模型只验证时钟口径。"""

import unittest

from asr_lab.measurement import measure_utterance
from test_replay import Clock


class TimedRecognizer:
    """模型系统边界的确定性夹具，绝不作为真实模型测量。"""
    def __init__(self, clock):
        self.clock = clock

    def accept(self, samples):
        self.clock.sleep(0.005)
        return "你好"

    def finish(self):
        self.clock.sleep(0.010)
        return "你好"


class MeasurementTests(unittest.TestCase):
    def test_replay_and_measurement_share_audio_origin(self):
        """场景：计时初始化开销不应重置音频时钟。

        构造：首次取时后经过5ms，40ms音频、每块解码5ms、finish10ms。
        执行：从同一个音频原点回放，首块仍应在20ms交付。
        预期：首字25ms、终稿55ms；初始化开销不能重复加在时钟上。
        """
        clock = Clock()
        first_call = True

        def now():
            nonlocal first_call
            timestamp = clock.now()
            if first_call:
                first_call = False
                clock.sleep(0.005)
            return timestamp

        result = measure_utterance(TimedRecognizer(clock), [0.0] * 640,
                                   realtime=True, now=now, sleep=clock.sleep)
        self.assertEqual(result["first_partial_from_file_start_ms"], 25, "首字基于同一个音频原点")
        self.assertEqual(result["elapsed_ms"], 55, "初始化开销不应偏移回放截止时间")

    def test_file_end_latency_excludes_audio_and_includes_decode_tail(self):
        """场景：不将整条录音时长计成收尾，也不隐藏最后解码耗时。

        构造：40ms录音，两块20ms；每块解码5ms，finish10ms。
        执行：真实节奏的可控时钟回放，speech边界未知。
        预期：首字25ms，总55ms，文件末尾到final15ms；不伪造speech指标。
        """
        clock = Clock()
        result = measure_utterance(TimedRecognizer(clock), [0.0] * 640,
                                   realtime=True, now=clock.now, sleep=clock.sleep)
        self.assertEqual(result["first_partial_from_file_start_ms"], 25, "包含首块采样及解码")
        self.assertEqual(result["elapsed_ms"], 55, "总时长为40+5+10ms")
        self.assertEqual(result["after_file_end_ms"], 15, "收尾必须包括末块解码与finish")
        self.assertIsNone(result["after_speech_end_ms"], "缺人工边界不得伪造语音收尾")
        self.assertEqual(result["status"], "ok", "有效终稿应成功")

    def test_empty_retraction_does_not_count_as_stable_prefix(self):
        """场景：文本被撤回为空后重现，不应算作一直稳定。

        构造：三块20ms，模型依次给出“你好”、空、“你好”。
        执行：记录真实节奏的结果变更，最后定稿“你好”。
        预期：首字20ms、稳定前缀60ms；空撤回必须出现在事件轨迹中。
        """
        class RetractingRecognizer:
            def __init__(self):
                self.texts = iter(["你好", "", "你好"])

            def accept(self, samples):
                return next(self.texts)

            def finish(self):
                return "你好"

        clock = Clock()
        result = measure_utterance(RetractingRecognizer(), [0.0] * 960, realtime=True,
                                   now=clock.now, sleep=clock.sleep)
        self.assertEqual(result["first_partial_from_file_start_ms"], 20, "首字仍是最早非空结果")
        self.assertEqual(result["first_stable_prefix_from_file_start_ms"], 60, "撤回前的字不稳定")
        self.assertEqual([event["text"] for event in result["partials"]], ["你好", "", "你好"],
                         "结果轨迹应保留空撤回")


if __name__ == "__main__":
    unittest.main()
