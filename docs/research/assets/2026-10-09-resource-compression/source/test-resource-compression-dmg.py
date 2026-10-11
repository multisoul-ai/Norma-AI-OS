#!/usr/bin/env python3
"""模拟 DMG 对照载荷与算法选择的聚焦测试。"""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).with_name("benchmark-resource-compression-dmg.py")
spec = importlib.util.spec_from_file_location("resource_compression_dmg", SCRIPT)
dmg = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dmg)


class ResourceCompressionDmgTests(unittest.TestCase):
    # 场景：hdiutil 已产生部分挂载，但 attach 返回错误；模拟创建、验证成功和
    # attach 失败，调用单镜像流程；预期仍尝试 detach，并保留原挂载错误。
    def test_failed_attach_still_attempts_detach(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            output = work / "logs"
            output.mkdir()
            calls = []

            def command(argv, cwd, log, timeout, stdout=None):
                calls.append(argv[1])
                if argv[1] == "create":
                    Path(argv[-1]).write_bytes(b"image")
                return {"status": "error" if argv[1] == "attach" else "ok", "log": str(log)}

            with mock.patch.object(dmg.bench, "run_command", side_effect=command):
                with self.assertRaisesRegex(RuntimeError, "只读挂载", msg="应报告实际挂载失败"):
                    dmg.test_image("UDZO", "plain", work / "payload", work, output,
                                   work / "source.tar", [], "gzip-9", "resources.tar.gz", 1)
            self.assertEqual(calls, ["create", "verify", "attach", "detach"],
                             "attach 错误后仍必须执行卸载清理，不能留下实验挂载")

    # 场景：输出目录可能错误地指向源资源；分别传入资源根目录和其子目录，
    # 执行输出范围验证；预期全部拒绝，而 build 目录可以通过。
    def test_output_cannot_write_into_resources(self):
        root = Path("/tmp/fixture-repo")
        for relative in ["crates/app/assets/wallpapers", "crates/app/assets/wallpapers/results",
                         "crates/app/assets/feature-guide/results"]:
            with self.subTest(relative=relative):
                with self.assertRaises(ValueError, msg="实验输出不能污染打包资源"):
                    dmg.validate_output(root, root / relative)
        self.assertIsNone(dmg.validate_output(root, root / "build/benchmark"),
                          "独立 build 输出目录应允许使用")

    # 场景：部分组合只测过一轮或还原失败；构造三轮全成功的 gzip、部分完成的
    # zstd 和还原失败的 brotli；执行选择；预期只选全部轮次通过的候选。
    def test_selects_only_complete_verified_resource_case(self):
        rows = []
        for algorithm, count, size, status in [("gzip-9", 3, 60, "ok"),
                                               ("zstd-19", 1, 40, "ok"),
                                               ("brotli-11", 3, 20, "invalid")]:
            for _ in range(count):
                rows.append({"algorithm": algorithm, "dataset": "resources",
                             "compressed_bytes": size, "status": status})
        data = {"configuration": {"repeats": 3}, "results": rows,
                "finished_utc": "finished", "source_unchanged": True}
        self.assertEqual(dmg.choose_algorithm(data), "gzip-9",
                         "不完整或还原失败的组合不得用于 DMG 推荐对照")
        data["source_unchanged"] = False
        with self.assertRaises(ValueError, msg="源资源改变的结果必须拒绝"):
            dmg.choose_algorithm(data)

    # 场景：用 3 字节资源和 7 字节程序生成原资源、压缩资源两个载荷；检查目录，
    # 预期程序 SHA-256 一致，压缩载荷只携带归档，原文件保持不变。
    def test_payloads_keep_program_and_do_not_duplicate_resources(self):
        with tempfile.TemporaryDirectory(prefix="dmg test ") as directory:
            root = Path(directory)
            source = root / "source"
            asset = source / "crates/app/assets/wallpapers/a.mp4"
            asset.parent.mkdir(parents=True)
            asset.write_bytes(b"abc")
            program = root / "program"
            program.write_bytes(b"program")
            compressed = root / "resources.tar.zst"
            compressed.write_bytes(b"compressed")
            manifest = [{"path": "crates/app/assets/wallpapers/a.mp4", "bytes": 3,
                         "sha256": dmg.bench.sha256(asset)}]
            plain = root / "plain"
            packed = root / "packed"
            dmg.build_payload(source, manifest, program, plain)
            dmg.build_payload(source, manifest, program, packed, compressed)
            self.assertEqual(dmg.tree_manifest(plain)["Norma Compression Fixture.app/Contents/MacOS/fixture"],
                             dmg.tree_manifest(packed)["Norma Compression Fixture.app/Contents/MacOS/fixture"],
                             "两个载荷必须使用完全一致的程序字节")
            self.assertEqual((plain / "Norma Compression Fixture.app/Contents/Resources/wallpapers/a.mp4").read_bytes(),
                             b"abc", "原资源载荷需要实际原文件")
            self.assertFalse((packed / "Norma Compression Fixture.app/Contents/Resources/wallpapers").exists(),
                             "压缩载荷不能同时携带原资源")
            self.assertEqual((packed / "Norma Compression Fixture.app/Contents/Resources/resources.tar.zst").read_bytes(),
                             b"compressed", "压缩载荷应只携带对应归档")
            self.assertEqual(asset.read_bytes(), b"abc", "构建模拟载荷不得改动源文件")

    # 场景：挂载结果可能缺文件或篡改字节；创建载荷清单后删除资源或改为等长数据，
    # 比较树清单；预期能够发现两类异常。
    def test_tree_manifest_detects_missing_and_changed_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "file"
            path.write_bytes(b"abc")
            before = dmg.tree_manifest(root)
            path.write_bytes(b"def")
            self.assertNotEqual(before, dmg.tree_manifest(root), "等长内容变化必须被发现")
            path.unlink()
            self.assertNotEqual(before, dmg.tree_manifest(root), "缺失文件必须被发现")


if __name__ == "__main__":
    unittest.main()
