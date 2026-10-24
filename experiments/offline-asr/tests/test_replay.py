"""样本时钟回放的公共接口测试。"""

import unittest

from asr_lab.replay import replay_chunks


class Clock:
    def __init__(self):
        self.ns = 0

    def now(self):
        return self.ns

    def sleep(self, seconds):
        self.ns += round(seconds * 1e9)


class ReplayTests(unittest.TestCase):
    def test_chunks_wait_for_their_last_sample_and_preserve_tail(self):
        """场景：实时回放不得提前供应尚未采集的样本或丢末块。

        构造：725 samples / 16000 Hz，chunk=320，长度为320、320、85。
        执行：由可控时钟推进，收集回放事件和拼接样本。
        预期：20/40/45.3125 ms交付，末85 samples完整，不一次喂完。
        """
        clock = Clock()
        frames = list(replay_chunks(list(range(725)), 16000, 320,
                                   realtime=True, now=clock.now, sleep=clock.sleep))
        self.assertEqual([frame.delivered_ns for frame in frames],
                         [20_000_000, 40_000_000, 45_312_500], "交付时间应依照样本末尾")
        self.assertEqual([x for frame in frames for x in frame.samples],
                         list(range(725)), "回放应保留全部样本及顺序")
        self.assertEqual(len(frames[-1].samples), 85, "不应丢弃不足一个chunk的尾部")

    def test_slow_consumer_reports_lag_without_dropping_audio(self):
        """场景：解码慢于实时速度，回放不能删音频伪造低延迟。

        构造：三个20ms chunk，消费第一块后时钟增加50ms。
        执行：继续消费回放；后两块的deadline仍为40和60ms。
        预期：交付为20/70/70ms，最后deadline=60ms，保留960 samples。
        """
        clock = Clock()
        iterator = replay_chunks(list(range(960)), 16000, 320, realtime=True,
                                 now=clock.now, sleep=clock.sleep)
        first = next(iterator)
        clock.sleep(0.05)
        rest = list(iterator)
        self.assertEqual([first.delivered_ns] + [x.delivered_ns for x in rest],
                         [20_000_000, 70_000_000, 70_000_000], "必须暴露解码积压")
        self.assertEqual(rest[-1].deadline_ns, 60_000_000, "不可重置样本时钟")
        self.assertEqual(sum(len(x.samples) for x in [first] + rest), 960, "不得丢音频")


if __name__ == "__main__":
    unittest.main()
