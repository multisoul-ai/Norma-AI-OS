#!/usr/bin/env python3
"""检查发布资源必须来自受控 staging，测试包保持生产核心依赖。"""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class MediaPackagingTests(unittest.TestCase):
    # 场景：开发 .app 与安装包采用相同可信清单，否则无法实际验收按需资源。
    # 预期 runner 签名前生成受控 profile，并携带清单及七个静态预览。
    def test_dev_app_also_carries_trusted_media_catalog(self):
        runner = (ROOT / "scripts/run-macos-dev-app.sh").read_text()
        self.assertIn("scripts/prepare-release-media.sh --local-test", runner,
            "开发 .app 必须通过同一受控资源准备入口")
        self.assertIn('cp "$media_resources/media-catalog.json"', runner,
            "开发 .app 必须签名前携带可信媒体清单")
        self.assertIn('cp -R "$media_resources/wallpaper-previews"', runner,
            "开发 .app 必须携带轻量动态壁纸预览")

    # 场景：两种签名模式都打包同一媒体 profile，避免源码大目录重新进入 DMG。
    # 同时验证测试包的 helper、Sparkle 与浏览器扩展，确保体积对照保持核心一致。
    def test_both_configs_use_staging_and_preserve_core_resources(self):
        for filename in ("Packager.toml", "Packager.dev.toml"):
            config = (ROOT / filename).read_text()
            for directory in ("wallpapers", "feature-guide", "wallpaper-previews"):
                self.assertIn('{ src = "build/release-media/' + directory + '", target = "' + directory + '" }',
                              config, f"{filename} 必须从受控 staging 打包 {directory}")
            self.assertIn('"build/release-media/media-catalog.json"', config, "可信清单必须在签名前进入包")
            self.assertIn("scripts/prepare-release-media.sh", config, "每次打包必须重新准备媒体 profile")
            self.assertNotIn('"crates/app/assets/wallpapers"', config, "不能再隐式复制原始壁纸大目录")
            self.assertNotIn('"crates/app/assets/feature-guide"', config, "不能再隐式复制全部教程视频")
            for core in ('path = "target/release/norma"', 'path = "target/release/norma-browser-auth-host"',
                         '"crates/app/assets/browser-auth-extension"', 'frameworks = ["build/sparkle/Sparkle.framework"]'):
                self.assertIn(core, config, f"{filename} 不能漏打包核心能力 {core}")


if __name__ == "__main__":
    unittest.main()
