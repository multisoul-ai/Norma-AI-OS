#!/usr/bin/env python3
"""验证测试专用 GPUI feature 不再随正式构建启用。"""
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ReleaseFeatureTests(unittest.TestCase):
    # 场景：仅构建 production-data 正式 App，测试平台与 leak detection 应留在测试依赖。
    # 执行 Cargo 实际 normal/build feature tree；预期没有 GPUI test-support 依赖边。
    def test_production_does_not_enable_gpui_test_support(self):
        result = subprocess.run(["cargo", "tree", "--offline", "-p", "app",
            "--no-default-features", "--features", "production-data",
            "-e", "normal,build,features", "-i", "gpui"], cwd=ROOT,
            capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, f"feature tree 必须可解析：{result.stderr}")
        self.assertNotIn('gpui feature "test-support"', result.stdout,
            "正式 App 不应启用测试执行器与测试平台")


if __name__ == "__main__":
    unittest.main()
