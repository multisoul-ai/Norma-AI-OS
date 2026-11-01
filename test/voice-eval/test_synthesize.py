"""真实 say/ffmpeg 的小范围工具验收，不作为真人识别证据。"""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
import wave

ROOT = Path(__file__).resolve().parents[2]


class SynthesisTest(unittest.TestCase):
    def test_four_voices_generate_pcm_and_hashes(self):
        """场景：1 条开发文本由指定 4 声音合成。构造临时仓库外输出目录；
        执行脚本；预期四条音频均为 16k/mono/16bit，并含来源及 SHA-256。"""
        with tempfile.TemporaryDirectory(prefix="voice-eval-smoke-") as temporary:
            subprocess.run(["bash", str(ROOT / "scripts/voice-eval/synthesize.sh"), temporary, "1"], check=True)
            rows = [json.loads(line) for line in (Path(temporary) / "corpus.jsonl").read_text().splitlines()]
            self.assertEqual(len(rows), 4, "每条文本应有四个指定声音")
            self.assertEqual({r["voice"] for r in rows}, {"Tingting", "Eddy", "Flo", "Reed"}, "声音须按方案固定")
            for row in rows:
                with wave.open(str(Path(temporary) / row["audio"])) as audio:
                    self.assertEqual((audio.getframerate(), audio.getnchannels(), audio.getsampwidth()),
                                     (16000, 1, 2), "音频必须能直接推给生产 A1")
                    self.assertGreater(audio.getnframes(), 0, "不得生成空音频")
                self.assertEqual(len(row["sha256"]), 64, "每条必须记录哈希")
                self.assertEqual(row["source"], "synthetic-dev", "合成不能冒充真人测试集")


if __name__ == "__main__":
    unittest.main()
