#!/usr/bin/env python3
"""防止不同输入的发布构建结果混入同一个体积矩阵。"""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("release_size", Path(__file__).with_name("benchmark-release-size.py"))
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


class MatrixEvidenceTests(unittest.TestCase):
    # 场景：Cargo 使用显式 target triple，普通 release 目录仍有克隆的旧程序。
    # 日志指向 triple 子目录的新程序；预期选择 Cargo 本次报告的 executable，缺日志时拒绝旧缓存。
    def test_cargo_artifact_selects_current_target_instead_of_stale_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "target"
            stale = target / "release/norma-os"
            fresh = target / "aarch64-apple-darwin/release/norma-os"
            stale.parent.mkdir(parents=True)
            fresh.parent.mkdir(parents=True)
            stale.write_bytes(b"OLD-CACHE")
            fresh.write_bytes(b"newly-built" * 8)
            log = Path(directory) / "build.log"
            log.write_text(json.dumps({"reason": "compiler-artifact", "target": {"name": "norma-os", "kind": ["bin"]},
                "profile": {"test": False}, "executable": str(fresh)}) + "\n")
            self.assertEqual(BENCHMARK.cargo_executable(log, target), fresh.resolve(), "必须读取实际目标目录中的本次程序")
            self.assertNotEqual(BENCHMARK.cargo_executable(log, target), stale.resolve(), "不得冒用克隆的旧缓存")
            log.write_text("Finished release profile\n")
            with self.assertRaisesRegex(ValueError, "compiler-artifact"):
                BENCHMARK.cargo_executable(log, target)

    # 场景：跨次使用不同 Cargo target 配置。读取实际输入上下文；预期 target 变化会改变上下文。
    def test_build_target_is_part_of_comparison_context(self):
        with patch.dict(os.environ, {"CARGO_BUILD_TARGET": "aarch64-apple-darwin"}):
            arm = BENCHMARK.build_context()
        with patch.dict(os.environ, {"CARGO_BUILD_TARGET": "x86_64-apple-darwin"}):
            intel = BENCHMARK.build_context()
        self.assertEqual(arm["environment"].get("CARGO_BUILD_TARGET"), "aarch64-apple-darwin", "记录实际 target 配置")
        self.assertNotEqual(arm, intel, "不同目标不可共用一个构建矩阵")

    # 场景：同一 Rust 版本配合不同 Apple SDK/工具链。分别覆盖进程级 SDK，不更改系统配置。
    # 读取上下文；预期 SDK 和 Developer 路径都进入证据，不同配置不能共用基线。
    def test_apple_sdk_selection_is_part_of_comparison_context(self):
        # /usr/bin/git 本身使用 xcrun；本用例只验证环境证据，不执行虚构 Developer 目录下的工具。
        def tool_metadata(args, **kwargs):
            return "fixed-tool-metadata" if kwargs.get("text") else b"Cargo.toml\0Cargo.lock\0"
        with patch.object(BENCHMARK.subprocess, "check_output", side_effect=tool_metadata):
            with patch.dict(os.environ, {"SDKROOT": "/sdk/first", "DEVELOPER_DIR": "/developer/first"}):
                first = BENCHMARK.build_context()
            with patch.dict(os.environ, {"SDKROOT": "/sdk/second", "DEVELOPER_DIR": "/developer/second"}):
                second = BENCHMARK.build_context()
        self.assertEqual(first["environment"].get("SDKROOT"), "/sdk/first", "记录本次 SDK 选择")
        self.assertEqual(first["environment"].get("DEVELOPER_DIR"), "/developer/first", "记录本次 Apple 工具链选择")
        self.assertNotEqual(first, second, "Apple SDK 变化必须拒绝混入同一矩阵")

    # 场景：同一矩阵分次测量，第二次源码或工具链已变。
    # 首次写入基线，再继续相同输入和不同输入；预期仅相同输入可继续，原证据不被覆盖。
    def test_resume_rejects_changed_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            context = {"source_sha256": "a" * 64, "rustc": "rustc test", "features": ["production-data"]}
            BENCHMARK.guard_context(output, context)
            original = (output / "context.json").read_bytes()
            BENCHMARK.guard_context(output, dict(context))
            for changed in ({**context, "source_sha256": "b" * 64}, {**context, "rustc": "rustc changed"}):
                with self.assertRaisesRegex(ValueError, "different build inputs"):
                    BENCHMARK.guard_context(output, changed)
            self.assertEqual((output / "context.json").read_bytes(), original, "拒绝混用不能改写原矩阵基线")

    # 场景：旧结果没有输入证据。构造非空 results，预期拒绝继续，不补造一份当前输入当旧基线。
    def test_legacy_results_require_new_experiment_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            (output / "results.json").write_text(json.dumps([{"variant": "3-thin"}]))
            with self.assertRaisesRegex(ValueError, "missing build context"):
                BENCHMARK.guard_context(output, {"source_sha256": "a" * 64})
            self.assertFalse((output / "context.json").exists(), "无证据的旧结果不能被追认")

    # 场景：文件内容改变或同字节换路径，都会影响 Rust 编译输入。
    # 构造两个三字节源码并分别修改内容/路径；预期排序无关，内容与路径变化均改变指纹。
    def test_fingerprint_covers_content_and_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.rs").write_bytes(b"abc")
            (root / "b.rs").write_bytes(b"def")
            original = BENCHMARK.source_fingerprint(root, ["a.rs", "b.rs"])
            self.assertEqual(original, BENCHMARK.source_fingerprint(root, ["b.rs", "a.rs"]), "文件枚举顺序不能改变同一输入的指纹")
            (root / "a.rs").write_bytes(b"xyz")
            self.assertNotEqual(original, BENCHMARK.source_fingerprint(root, ["a.rs", "b.rs"]), "源码内容改变必须被识别")
            (root / "a.rs").write_bytes(b"abc")
            (root / "a.rs").rename(root / "renamed.rs")
            self.assertNotEqual(original, BENCHMARK.source_fingerprint(root, ["renamed.rs", "b.rs"]), "相同字节换路径也不是同一编译输入")


if __name__ == "__main__":
    unittest.main()
