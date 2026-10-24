"""冻结音频输入清单的文件边界测试。"""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import wave

from asr_lab.manifest import load_manifest


class ManifestTests(unittest.TestCase):
    def test_changed_audio_cannot_be_silently_scored(self):
        """场景：跑过的语料被改写后，必须拒绝沿用旧SHA清单。

        构造：16kHz单声道PCM16，320 samples=20ms，SHA对应原文件。
        执行：先加载有效清单，再篡改WAV末字节重新加载。
        预期：第一次返回一条，第二次明确报哈希错误而非继续评分。
        """
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wav = root / "one.wav"
            with wave.open(str(wav), "wb") as writer:
                writer.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
                writer.writeframes(bytes(640))
            row = {"id": "one", "audio": "one.wav", "sha256": hashlib.sha256(wav.read_bytes()).hexdigest(),
                   "reference": "你好", "speaker": "s1", "split": "dev", "language": "zh",
                   "source": "unit-fixture", "license": "test-only", "duration_s": 0.02}
            manifest = root / "samples.jsonl"
            manifest.write_text(json.dumps(row) + "\n")
            self.assertEqual(len(load_manifest(manifest)), 1, "合法文件应可读")
            wav.write_bytes(wav.read_bytes()[:-1] + b"x")
            with self.assertRaisesRegex(ValueError, "SHA-256", msg="改动音频必须失效"):
                load_manifest(manifest)


if __name__ == "__main__":
    unittest.main()
