#!/usr/bin/env python3
"""检查随包静态预览的覆盖、完整性与 640px 上限。"""
import hashlib
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class MediaPreviewAssetsTests(unittest.TestCase):
    # 场景：七个外置动态壁纸在断网且未下载时仍有可查看预览。
    # 校验登记覆盖 5 视频+2 GIF，原字节哈希与 lossless WebP 640px 上限。
    def test_every_dynamic_wallpaper_has_a_bounded_verified_static_preview(self):
        manifest = json.loads((ROOT / "packaging/media/resources.json").read_text())
        expected = {entry["path"] for entry in manifest["resources"]
                    if entry["policy"] == "dynamic-wallpaper"}
        previews = manifest.get("previews", [])
        self.assertEqual({entry["resource"] for entry in previews}, expected,
                         "全部外置动态壁纸必须保留静态预览")
        self.assertEqual(len(previews), 7, "五视频与两个 GIF 必须分别登记预览")
        total = 0
        for entry in previews:
            data = (ROOT / "packaging/media/previews" / Path(entry["path"]).name).read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), entry["sha256"], "预览必须符合可信哈希")
            self.assertEqual(len(data), entry["bytes"], "预览体积必须可审计")
            self.assertEqual(data[:4], b"RIFF", "预览必须为 WebP 容器")
            self.assertEqual(data[8:16], b"WEBPVP8L", "预览须采用 lossless WebP")
            self.assertEqual(data[20], 0x2f, "VP8L 图像头必须有效")
            bits = int.from_bytes(data[21:25], "little")
            width, height = (bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1
            self.assertLessEqual(max(width, height), 640, "预览最长边不得超过 640px")
            total += len(data)
        # M2 的 33,000,000 - 31,530,913 = 1,469,087 字节余量；预览预算
        # 为 1,400,000 字节，保留 69,087 字节给 catalog 等元数据。
        self.assertLessEqual(total, 1_400_000, "七预览必须在 M2 实际负载 33 MB 预算之内")


if __name__ == "__main__":
    unittest.main()
