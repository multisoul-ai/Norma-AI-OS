#!/usr/bin/env python3
"""行为测试：发布资源 profile 保持原字节并准确外置可选资源。"""

import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("prepare-release-media.py")


class ReleaseMediaTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.output = self.root / "build/media"
        self.files = {
            "wallpapers/default.webp": (b"0123456789", "core"),
            "wallpapers/optional.mp4": (b"video-bytes", "dynamic-wallpaper"),
            "wallpapers/optional.gif": (b"gifdata", "dynamic-wallpaper"),
            "feature-guide/tutorial.mp4": (b"tutorial-data", "tutorial-video"),
            "feature-guide/poster.webp": (b"cover", "core"),
        }
        resources = []
        for rel, (data, policy) in self.files.items():
            target = self.root / "crates/app/assets" / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            resources.append({"path": rel, "policy": policy, "bytes": len(data),
                              "sha256": hashlib.sha256(data).hexdigest()})
        self.manifest = {
            "schema_version": 1, "resource_version": "media-v1",
            "download_base_url": "https://github.com/example/releases/download/media-v1",
            "resources": resources,
        }
        self.manifest_path = self.root / "packaging/media/resources.json"
        self.manifest_path.parent.mkdir(parents=True)
        self.write_manifest()

    def write_manifest(self):
        self.manifest_path.write_text(json.dumps(self.manifest))

    def prepare(self, profile):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(self.root),
             "--output", str(self.output), "--profile", profile],
            capture_output=True, text=True, check=False,
        )

    # 场景：仅动态壁纸外置。构造 46 字节媒体，其中动态视频 11、GIF 7。
    # 执行 wallpapers-on-demand；预期保留静态 10、封面 5、教程 13，共 28。
    # 外置 18 字节，原源文件保持原字节，安装包 catalog 仍登记可选媒体。
    def test_wallpapers_on_demand_keeps_core_and_offline_tutorial(self):
        result = self.prepare("wallpapers-on-demand")
        self.assertEqual(result.returncode, 0, f"资源准备应成功：{result.stderr}")
        self.assertEqual((self.output / "wallpapers/default.webp").read_bytes(),
                         b"0123456789", "默认壁纸必须保持原文件字节")
        self.assertEqual((self.output / "feature-guide/tutorial.mp4").read_bytes(),
                         b"tutorial-data", "首阶段教程视频必须仍可离线使用")
        self.assertFalse((self.output / "wallpapers/optional.mp4").exists(),
                         "可选视频壁纸不能仍进入轻量目录")
        self.assertFalse((self.output / "wallpapers/optional.gif").exists(),
                         "可选 GIF 不能仍进入轻量目录")
        report = json.loads((self.output / "media-report.json").read_text())
        self.assertEqual(report["source_bytes"], 46, "源字节必须含全部五文件")
        self.assertEqual(report["bundled_bytes"], 28, "随包媒体应为 10+5+13 字节")
        catalog = json.loads((self.output / "media-catalog.json").read_text())
        self.assertEqual(len(catalog["resources"]), 5, "外置资源仍必须有可信登记")
        for rel, (data, _) in self.files.items():
            self.assertEqual((self.root / "crates/app/assets" / rel).read_bytes(),
                             data, "准备发布目录不得重写源资源")

    # 场景：用户误把输出指定为已有普通文件。构造 sentinel 原字节。
    # 执行完整 profile；预期明确失败，原文件与内容不被替换成 staging 目录。
    def test_existing_output_file_is_rejected_without_replacement(self):
        self.output.parent.mkdir(parents=True)
        self.output.write_bytes(b"keep-original-output")
        result = self.prepare("full")
        self.assertNotEqual(result.returncode, 0, "普通文件不能作为可替换目录")
        self.assertTrue(self.output.is_file(), "失败准备不得把原文件替换成目录")
        self.assertEqual(self.output.read_bytes(), b"keep-original-output",
                         "失败准备必须保留原输出内容")

    # 场景：资源目录被软链接指向仓库内其他目录。构造完整、哈希正确媒体。
    # 执行 full；预期拒绝软链接祖先，不因源内容正确而绕过路径边界。
    def test_symlink_asset_ancestor_is_rejected(self):
        assets = self.root / "crates/app/assets"
        outside = self.root / "outside-assets"
        assets.rename(outside)
        assets.symlink_to(outside, target_is_directory=True)
        result = self.prepare("full")
        self.assertNotEqual(result.returncode, 0, "资源目录祖先软链接必须被拒绝")
        self.assertFalse(self.output.exists(), "不安全源目录不得生成发布目录")

    # 场景：从 full 切换到 light 后再次执行；五文件共 46 字节，core 共 15。
    # 预期外置文件从输出清除，反复准备仍正确，源文件未删除。
    def test_full_to_light_replacement_clears_optional_files(self):
        self.assertEqual(self.prepare("full").returncode, 0, "full 必须可准备")
        for _ in range(2):
            result = self.prepare("light")
            self.assertEqual(result.returncode, 0, f"light 必须可重复准备：{result.stderr}")
            report = json.loads((self.output / "media-report.json").read_text())
            self.assertEqual(report["bundled_bytes"], 15, "light 保留默认壁纸 10+封面 5")
            self.assertFalse((self.output / "feature-guide/tutorial.mp4").exists(),
                             "旧 full 的教程视频必须从新 light 目录清除")
            self.assertFalse((self.output / "wallpapers/optional.gif").exists(),
                             "旧 full 的 GIF 必须从新 light 目录清除")

    # 场景：已有可用输出，随后源视频损坏。预期准备失败，旧输出原字节不动。
    def test_invalid_source_preserves_previous_output(self):
        self.assertEqual(self.prepare("full").returncode, 0, "先生成有效完整输出")
        (self.root / "crates/app/assets/wallpapers/optional.mp4").write_bytes(b"broken")
        result = self.prepare("light")
        self.assertNotEqual(result.returncode, 0, "哈希不匹配的源资源必须使准备失败")
        self.assertEqual((self.output / "wallpapers/optional.mp4").read_bytes(),
                         b"video-bytes", "失败后旧完整输出必须仍可用")

    # 场景：旧输出的只读子目录令备份清理失败，提升新输出本身已成功。
    # 预期命令报告成功并明确清理警告，新 light 内容可用，备份可定位。
    def test_backup_cleanup_failure_does_not_report_publication_failure(self):
        self.assertEqual(self.prepare("full").returncode, 0, "先生成可用旧输出")
        protected = self.output / "protected"
        protected.mkdir()
        (protected / "old.txt").write_bytes(b"old")
        protected.chmod(0o500)
        try:
            result = self.prepare("light")
            self.assertEqual(result.returncode, 0, f"已提升成功不能再报告失败：{result.stderr}")
            self.assertIn("backup cleanup", result.stderr, "必须明确报告未清理备份")
            self.assertFalse((self.output / "feature-guide/tutorial.mp4").exists(),
                             "已提交的新 light 输出应可用")
        finally:
            for directory in self.output.parent.glob(".media-backup-*/protected"):
                directory.chmod(0o700)
            if protected.exists():
                protected.chmod(0o700)

    # 场景：轻量包内置独立静态缩略图，原动态资源外置且仍保持源字节。
    # 构造 7 字节派生预览；预期单独计费，不把预览混算为原始媒体压缩收益。
    def test_light_keeps_verified_static_preview_and_accounts_overhead(self):
        preview = self.root / "packaging/media/previews/optional.webp"
        preview.parent.mkdir(parents=True)
        preview.write_bytes(b"preview")
        self.manifest["previews"] = [{"resource": "wallpapers/optional.mp4",
            "path": "wallpaper-previews/optional.webp", "bytes": 7,
            "sha256": hashlib.sha256(b"preview").hexdigest()}]
        self.write_manifest()
        result = self.prepare("light")
        self.assertEqual(result.returncode, 0, f"light 预览必须可准备：{result.stderr}")
        self.assertEqual((self.output / "wallpaper-previews/optional.webp").read_bytes(),
                         b"preview", "没有下载原视频时仍须有静态预览")
        report = json.loads((self.output / "media-report.json").read_text())
        self.assertEqual(report["source_bytes"], 46, "原媒体基线不能混入派生预览")
        self.assertEqual(report["bundled_bytes"], 15, "随包原媒体仍为 10+5 字节")
        self.assertEqual(report["preview_bytes"], 7, "新增静态预览开销必须单列")
        self.assertGreater(report["media_payload_bytes"], 22, "实际媒体负载还须包括可信 catalog")


if __name__ == "__main__":
    unittest.main()
