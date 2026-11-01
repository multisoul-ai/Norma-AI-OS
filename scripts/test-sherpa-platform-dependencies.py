#!/usr/bin/env python3
"""Check the real default-target dependency graph of the built-in Sherpa consumers."""
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


class SherpaPlatformDependencies(unittest.TestCase):
    def test_default_consumers_keep_native_sherpa_on_windows_and_macos(self):
        """场景：三个公开 Sherpa 消费者默认构建；数据为 Windows/MSVC 与 macOS/ARM64；
        执行 Cargo 的直接 normal 依赖图查询；预期两平台都依赖真实 Sherpa，不靠测试 feature。
        """
        for target in ("x86_64-pc-windows-msvc", "aarch64-apple-darwin"):
            for package in ("voice_input", "asr", "voice_sherpa"):
                with self.subTest(target=target, package=package):
                    run = subprocess.run([
                        "cargo", "tree", "--locked", "--target", target,
                        "-p", package, "--edges", "normal", "--depth", "1",
                        "--prefix", "none",
                    ], cwd=ROOT, capture_output=True, text=True, timeout=120)
                    self.assertEqual(run.returncode, 0, f"Cargo graph must resolve: {run.stderr}")
                    dependencies = run.stdout.splitlines()[1:]
                    self.assertTrue(
                        any(line.startswith("sherpa-onnx v") for line in dependencies),
                        f"{package} must retain the native Sherpa dependency on {target}: {dependencies}",
                    )
                    self.assertFalse(
                        any(line.startswith("gpui v") for line in dependencies),
                        f"{package} must remain independent of GPUI",
                    )


if __name__ == "__main__":
    unittest.main()
