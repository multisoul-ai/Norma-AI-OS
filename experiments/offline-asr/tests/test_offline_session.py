"""离线模型只能在收齐原音频后定稿，不能伪造逐块partial。"""

from types import SimpleNamespace
import unittest

from asr_lab.engines import BufferedSession, OfflineSession


class OfflineSessionTests(unittest.TestCase):
    def test_callable_backend_receives_original_audio_only_after_finish(self):
        """场景：MLX一类函数式后端也必须处理完整原音频。

        构造：接受浮点PCM的系统边界函数，输入三样本分为两块。
        执行：先分块accept，再finish。
        预期：收音期间无partial或解码；定稿时精确收到原顺序三样本。
        """
        received = []

        def transcribe(samples):
            received.append(list(samples))
            return "中英 mixed"

        session = BufferedSession(transcribe)
        self.assertEqual(session.accept([0.25, -0.5]), "", "离线回调不伪造partial")
        self.assertEqual(session.accept([0.125]), "", "尾块只缓冲")
        self.assertEqual(received, [], "录音结束前不应调用全句解码")
        self.assertEqual(session.finish(), "中英 mixed", "使用后端真实终稿")
        self.assertEqual(received, [[0.25, -0.5, 0.125]], "不得重排、丢尾或重复音频")

    def test_offline_engine_receives_all_samples_once_at_finish(self):
        """场景：分块输入离线引擎时，尾块和顺序必须完整保留。

        构造：真实后端系统边界夹具，前块[1,2]、末块[3]。
        执行：accept两次然后finish；后端只在decode时返回终稿。
        预期：accept无partial，后端收到三样本16kHz，finish返回完整结果。
        """
        class Backend:
            def create_stream(self):
                self.stream = SimpleNamespace(result=SimpleNamespace(text=""))
                self.stream.accept_waveform = lambda rate, samples: setattr(
                    self, "received", (rate, list(samples)))
                return self.stream

            def decode_stream(self, stream):
                stream.result.text = "测试 result"

        backend = Backend()
        session = OfflineSession(backend)
        self.assertEqual(session.accept([1.0, 2.0]), "", "离线模型不应伪造partial")
        self.assertEqual(session.accept([3.0]), "", "末块仍只缓冲")
        self.assertFalse(hasattr(backend, "received"), "finish前不应重复全句解码")
        self.assertEqual(session.finish(), "测试 result", "结束时输出真实后端终稿")
        self.assertEqual(backend.received, (16000, [1.0, 2.0, 3.0]), "样本与采样率应完整准确")


if __name__ == "__main__":
    unittest.main()
