#!/usr/bin/env python3
"""主安装包只携带可信A1下载描述，默认使用轻量媒体。"""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class PackageTests(unittest.TestCase):
    # 场景：不使用A1的用户不承担运行环境的下载和安装包体积。
    # 构造：两种真实打包配置与开发启动器；检查实际资源、前置构建命令及复制来源。
    # 预期：只携带小型下载描述，不复制或构建完整Python环境。
    def test_no_runtime_payload_or_build_in_main_package(self):
        for name in ("Packager.toml", "Packager.dev.toml"):
            config = (ROOT / name).read_text()
            resources = config
            self.assertNotIn("build/a1-engine", resources, f"{name} 不得内置运行环境")
            self.assertIn("packaging/a1-engine", resources, "必须携带可信下载描述")
            self.assertNotIn("prepare-a1-engine.py", config, "不能为主包构建环境")
        metadata = ROOT / "packaging/a1-engine"
        self.assertLess(sum(p.stat().st_size for p in metadata.iterdir()), 8 * 1024 * 1024,
                        "A1随包只能携带可信元数据，不得混入运行环境归档")
        self.assertFalse((metadata / "engine.tar.bz2").exists(), "不能把大归档重新打回主包")
        runner = (ROOT / "scripts/run-macos-dev-app.sh").read_text()
        self.assertNotIn("scripts/prepare-a1-engine.py", runner, "开发启动不构建环境")
        self.assertNotIn('cp -R "$repo_root/build/a1-engine"', runner, "开发包不复制环境")

    # 场景：普通打包应默认轻量，显式full仍能生成完整离线媒体包。
    # 构造：隔离目录内运行真实入口，Python边界夹具只输出收到的参数。
    # 执行：不指定profile，再指定full；预期分别为light和full。
    def test_media_defaults_light_with_explicit_full_override(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            script = root / "scripts/prepare-release-media.sh"
            script.write_bytes((ROOT / "scripts/prepare-release-media.sh").read_bytes())
            python = root / "python3"
            python.write_text('#!/bin/sh\nprintf \'%s\\n\' "$*"\n')
            python.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + ":" + os.environ["PATH"])
            env.pop("NORMA_MEDIA_PROFILE", None)
            result = subprocess.check_output(["bash", str(script), "--local-test"], env=env, text=True)
            self.assertIn("--profile light", result, "默认不能打全量媒体")
            env["NORMA_MEDIA_PROFILE"] = "full"
            result = subprocess.check_output(["bash", str(script), "--local-test"], env=env, text=True)
            self.assertIn("--profile full", result, "保留显式full")


if __name__ == "__main__":
    unittest.main()
