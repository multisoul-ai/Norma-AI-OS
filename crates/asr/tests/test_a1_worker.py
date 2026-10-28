"""协议边界测试：用模型替身处理真实二进制帧，不加载任何 ASR 模型。"""
import importlib.util
import io
import json
from pathlib import Path
import struct
import unittest

SPEC = importlib.util.spec_from_file_location(
    "a1_worker", Path(__file__).parents[1] / "runtime/a1/worker.py")
worker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(worker)


def frame(op, seq, payload=b"", session=1):
    data = json.dumps(dict(v=1, session=session, seq=seq, op=op,
                           bytes=len(payload))).encode()
    return struct.pack("<I", len(data)) + data + payload


class Backend:
    def begin(self):
        self.data = bytearray()
    def accept(self, pcm):
        self.data.extend(pcm)
        return "预览"
    def finish(self):
        return "准确 final" if self.data else ""


class ProtocolTests(unittest.TestCase):
    # 场景：同进程两轮；构造有音频/空音频两轮；执行真实帧；预期唯一准确终稿与空终稿。
    def test_rounds_keep_empty_final_and_identity(self):
        source = io.BytesIO(frame("begin", 1) + frame("audio", 2, b"\x01\x00")
                            + frame("finish", 3) + frame("begin", 1, session=2)
                            + frame("finish", 2, session=2))
        output = io.BytesIO()
        worker.serve(source, output, Backend(), "qwen")
        output.seek(0)
        results = []
        while raw := output.read(4):
            results.append(json.loads(output.read(struct.unpack("<I", raw)[0])))
        self.assertEqual([r["text"] for r in results], ["", "预览", "准确 final", "", ""])
        self.assertEqual(results[-1]["source"], "qwen")
        self.assertEqual(results[-1]["session"], 2)
        self.assertFalse(results[-1]["fallback"])

    # 场景：协议重放/越界/终态重复；执行损坏帧；预期抛错且不读取无界音频。
    def test_invalid_requests_fail_closed(self):
        cases = [frame("audio", 1, b"xx"),
                 frame("begin", 1) + frame("finish", 2) + frame("finish", 3),
                 frame("begin", 1) + frame("audio", 3, b"xx"),
                 frame("begin", 1) + frame("audio", 2, b"x"),
                 struct.pack("<I", 4097)]
        for data in cases:
            with self.subTest(data=data[:30]), self.assertRaises(ValueError):
                worker.serve(io.BytesIO(data), io.BytesIO(), Backend(), "qwen")


if __name__ == "__main__":
    unittest.main()
