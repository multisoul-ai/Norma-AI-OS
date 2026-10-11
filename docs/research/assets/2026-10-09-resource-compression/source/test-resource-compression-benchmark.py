#!/usr/bin/env python3
"""无损压缩基准的聚焦回归测试。"""

import hashlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("benchmark-resource-compression.py")
spec = importlib.util.spec_from_file_location("resource_compression_benchmark", SCRIPT)
bench = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = bench
spec.loader.exec_module(bench)


class ResourceCompressionTests(unittest.TestCase):
    # 场景：总字节口径需要处理恰好节省一半及压缩后膨胀；构造 100 字节源数据，
    # 计算 50/101 字节产物；预期分别为 50% 与 -1%，空输入必须报错。
    def test_savings_keeps_negative_values(self):
        self.assertEqual(bench.savings_percent(100, 50), 50.0, "一半体积应为 50%")
        self.assertAlmostEqual(bench.savings_percent(100, 101), -1.0,
                               msg="膨胀应保留负节省率")
        with self.assertRaises(ValueError, msg="空数据集不应计算压缩收益"):
            bench.savings_percent(0, 0)

    # 场景：三个类型的文件位于两个打包目录；创建 MP4、GIF 与说明文档，
    # 生成清单并分类；预期 video/images/resources 数量为 1/1/3，顺序固定。
    def test_manifest_and_groups(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.populate(root)
            manifest = bench.inventory(root)
            groups = bench.datasets(manifest)
            self.assertEqual(len(groups["video"]), 1, "MP4 单独进入视频组")
            self.assertEqual(len(groups["images"]), 1, "GIF 进入图片组")
            self.assertEqual(len(groups["resources"]), 3, "说明文档计入完整资源")
            self.assertEqual([item["path"] for item in manifest],
                             sorted(item["path"] for item in manifest), "路径必须稳定排序")

    # 场景：源文件 mtime 变化不应影响算法输入；制作两次 tar，中间改 mtime，
    # 比较 SHA-256；预期相同且成员能逐字节通过验证。
    def test_archive_is_deterministic(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.populate(root)
            manifest = bench.inventory(root)
            first, second = root / "first.tar", root / "second.tar"
            bench.make_tar(root, manifest, first)
            for item in manifest:
                os.utime(root / item["path"], (12345, 12345))
            bench.make_tar(root, manifest, second)
            self.assertEqual(bench.sha256(first), bench.sha256(second),
                             "mtime 不得影响统一 tar 输入")
            self.assertEqual(bench.verify_tar(first, manifest), 3,
                             "全部成员应通过路径、长度和哈希验证")

    # 场景：归档成员内容被替换为等长字节；制作只有一个成员的错误 tar，
    # 验证清单；预期因哈希不一致失败，而不是仅凭长度成功。
    def test_verification_rejects_same_length_corruption(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "corrupt.tar"
            original = b"good"
            manifest = [{"path": "asset.mp4", "bytes": 4,
                         "sha256": hashlib.sha256(original).hexdigest()}]
            self.write_tar(path, [("asset.mp4", b"evil")])
            with self.assertRaises(ValueError, msg="同长度篡改必须被哈希验证发现"):
                bench.verify_tar(path, manifest)

    # 场景：归档出现额外、缺失、重复或越界成员；分别构造四种错误归档，
    # 对照只有 a.mp4 的清单；预期每种都失败，不能被解包到源目录。
    def test_verification_rejects_member_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bad.tar"
            manifest = [{"path": "a.mp4", "bytes": 1,
                         "sha256": hashlib.sha256(b"a").hexdigest()}]
            for members in [[("a.mp4", b"a"), ("extra", b"x")], [],
                            [("a.mp4", b"a"), ("a.mp4", b"a")],
                            [("../a.mp4", b"a")]]:
                with self.subTest(members=members):
                    self.write_tar(path, members)
                    with self.assertRaises(ValueError,
                                           msg="成员集合异常必须被拒绝"):
                        bench.verify_tar(path, manifest)

    # 场景：成功、未完成与验证失败样本混在一起；仅成功样本耗时为 2/9/3 秒，
    # 汇总时预期中位数为 3 秒、样本数为 3，不将失败样本计为已验证。
    def test_summary_only_uses_verified_samples(self):
        samples = [{"status": "ok", "compressed_bytes": 50,
                    "compress": {"seconds": value, "max_rss_bytes": 1000},
                    "decompress": {"seconds": 1, "max_rss_bytes": 500}}
                   for value in [2, 9, 3]]
        samples.append({"status": "invalid", "compressed_bytes": 1})
        result = bench.summarize(samples, 100)
        self.assertEqual(result["successful_runs"], 3, "只统计完整还原成功的样本")
        self.assertEqual(result["compress_seconds"], 3, "耗时采用成功样本中位数")
        self.assertEqual(result["compress_min_seconds"], 2, "保留最小耗时")
        self.assertEqual(result["compress_max_seconds"], 9, "保留最大耗时")
        self.assertTrue(result["meets_50_percent"], "50 字节对 100 字节恰好达标")
        self.assertIsNone(bench.summarize([{"status": "timeout"}], 100),
                          "没有成功样本不得生成成功汇总")

    # 场景：实验进程超时；启动后台 sleep 进程并设置很短期限，
    # 预期返回 timeout 及非成功退出码，进程已停止。
    def test_timeout_is_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            result = bench.run_command([sys.executable, "-c", "import time; time.sleep(10)"],
                                       Path(directory), Path(directory) / "log", 0.1)
            self.assertEqual(result["status"], "timeout", "超时不能被标记为成功")
            self.assertNotEqual(result["exit_code"], 0, "超时应停止进程")

    # 场景：真实 gzip CLI 在小资源集完整往返；使用含空格目录执行脚本，
    # 读取落盘结果；预期三个数据集都有成功记录且所有原文件保持不变。
    def test_gzip_cli_roundtrip(self):
        with tempfile.TemporaryDirectory(prefix="compression test ") as directory:
            root = Path(directory) / "source"
            self.populate(root)
            before = bench.inventory(root)
            output = Path(directory) / "output"
            result = subprocess.run([sys.executable, str(SCRIPT), "--root", str(root),
                                     "--output", str(output), "--repeats", "1",
                                     "--algorithms", "gzip-9"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, "小数据 gzip 往返必须完成：" + result.stderr)
            import json
            data = json.loads((output / "results.json").read_text())
            self.assertEqual(len(data["results"]), 3, "三个数据集均需实验")
            self.assertTrue(all(row["status"] == "ok" for row in data["results"]),
                            "每个数据集都必须通过完整还原")
            self.assertEqual(before, bench.inventory(root), "CLI 不得改变原文件")

    @staticmethod
    def populate(root):
        for relative, data in [("wallpapers/a.mp4", b"video" * 40),
                               ("wallpapers/b.gif", b"image" * 20),
                               ("feature-guide/README.md", b"description")]:
            path = root / "crates/app/assets" / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)

    @staticmethod
    def write_tar(path, members):
        with tarfile.open(path, "w", format=tarfile.USTAR_FORMAT) as archive:
            for name, data in members:
                info = tarfile.TarInfo(name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))


if __name__ == "__main__":
    unittest.main()
