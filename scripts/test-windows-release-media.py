#!/usr/bin/env python3
"""Exercise Windows checkout bytes and the real release media staging command."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class WindowsReleaseMediaTests(unittest.TestCase):
    # 场景：可信媒体清单按Git原始字节生成，Windows checkout不能改变随包README。
    # 构造：真实仓库blob、工作目录资源及独立清单；执行字节数/哈希/原字节比较。
    # 预期：保留清单的完整性保护，不能通过重写清单或宽松换行归一化掩盖差异。
    def test_hashed_checkout_resource_preserves_the_trusted_git_bytes(self):
        relative = "crates/app/assets/feature-guide/README.md"
        canonical = subprocess.run(
            ["git", "show", f"HEAD:{relative}"], cwd=ROOT,
            check=True, capture_output=True,
        ).stdout
        manifest = json.loads((ROOT / "packaging/media/resources.json").read_text(encoding="utf-8"))
        entry = next(item for item in manifest["resources"] if item["path"] == "feature-guide/README.md")
        self.assertEqual(len(canonical), entry["bytes"], "可信清单必须先匹配Git原始资源字节数")
        self.assertEqual(hashlib.sha256(canonical).hexdigest(), entry["sha256"], "可信清单必须先匹配Git原始资源哈希")
        checked_out = (ROOT / relative).read_bytes()
        self.assertEqual(checked_out, canonical,
                         f"Windows checkout不能改变资源：git={len(canonical)}B, working={len(checked_out)}B, CRLF={checked_out.count(bytes([13, 10]))}")

    # 场景：媒体目录含中文路径，生成catalog必须是UTF-8/LF且实际字节与报告一致。
    # 构造：唯一core README资源，原内容为UTF-8文本，可信清单独立计算长度及哈希。
    # 执行：真实prepare-release-media.py light入口；预期原内容不变、JSON可UTF-8解析、字节报告准确。
    def test_staged_unicode_catalog_has_exact_reported_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            relative = "feature-guide/说明.md"
            data = "可信资源\n".encode("utf-8")
            source = root / "crates/app/assets" / relative
            source.parent.mkdir(parents=True)
            source.write_bytes(data)
            manifest = root / "packaging/media/resources.json"
            manifest.parent.mkdir(parents=True)
            manifest.write_text(json.dumps({
                "schema_version": 1, "resource_version": "media-v1",
                "download_base_url": "https://example.test/media-v1",
                "resources": [{"path": relative, "policy": "core", "bytes": len(data),
                               "sha256": hashlib.sha256(data).hexdigest()}],
            }), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/prepare-release-media.py"),
                 "--root", str(root), "--profile", "light"],
                capture_output=True, text=True, encoding="utf-8", check=False,
            )
            self.assertEqual(result.returncode, 0, f"真实媒体准备必须处理中文路径：{result.stderr}")
            output = root / "build/release-media"
            catalog = (output / "media-catalog.json").read_bytes()
            report = json.loads(result.stdout)
            self.assertEqual(report["catalog_bytes"], len(catalog), "实际catalog字节不能与报告的UTF-8长度不同")
            self.assertNotIn(b"\r\n", catalog, "发布catalog应跨平台保持LF原字节")
            parsed = json.loads(catalog.decode("utf-8"))
            self.assertEqual(parsed["resources"][0]["path"], relative, "UTF-8目录名必须原样保留")
            self.assertEqual((output / relative).read_bytes(), data, "媒体准备不得改写可信资源内容")


if __name__ == "__main__":
    unittest.main(verbosity=2)
