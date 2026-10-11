# -*- coding: utf-8 -*-
"""报告集中展示新证据，同时保持旧样本、局部实验和新包的测量范围。"""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("size_report_extra", ROOT / "scripts/report-package-size.py")
REPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPORT)


class ExtraEvidenceTests(unittest.TestCase):
    def render(self, extras=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = root / "docs/research/assets/2026-10-10-package-size-implementation"
            evidence.mkdir(parents=True)
            profile = {"profile": "light", "original_bytes": 21000000,
                "originals_bundled_bytes": 9500000, "preview_bytes": 1000000,
                "catalog_bytes": 0, "payload_bytes": 10500000, "net_saved_percent": 50.0}
            (evidence / "media-profile-comparison.json").write_text(json.dumps([profile]))
            algorithms = evidence.parent / "2026-10-09-resource-compression"
            algorithms.mkdir()
            (algorithms / "summary.csv").write_text("dataset,algorithm,compressed_bytes\n")
            if extras:
                fixtures = {
                    "installed-app-distribution.json": {"version": "0.0.9",
                        "regular_file_bytes": 93000000, "regular_files": 7,
                        "groups": [{"group": "main_binary", "files": 1, "bytes": 30000000,
                            "share_percent": 100 * 30 / 93}, {"group": "other_resources",
                            "files": 6, "bytes": 63000000, "share_percent": 100 * 63 / 93}]},
                    "source-media-distribution.json": {"total_bytes": 21000000,
                        "total_files": 2, "original_media_files": 2, "original_document_files": 0,
                        "groups": [{"scope": "wallpapers", "extension": ".mp4", "files": 2,
                            "bytes": 21000000, "share_percent": 100.0}]},
                    "media-install-probe.json": {"scope": "filesystem-only fixture",
                        "summary": {
                            "install": {"successful_runs": 3, "source_bytes": 1234567,
                                "median_install_seconds": 1.25, "median_verify_seconds": 0.125,
                                "median_repair_seconds": 0, "peak_rss_bytes": 8 * 1048576,
                                "max_read_buffer_bytes": 32768},
                            "repair": {"successful_runs": 3, "source_bytes": 1234567,
                                "median_install_seconds": 9.999, "median_verify_seconds": 0.125,
                                "median_repair_seconds": 2.75, "peak_rss_bytes": 12 * 1048576,
                                "max_read_buffer_bytes": 65536},
                            "cancel": {"successful_runs": 3, "source_bytes": 1234567,
                                "median_install_seconds": 0.005, "median_verify_seconds": 0,
                                "median_repair_seconds": 0, "peak_rss_bytes": 4 * 1048576,
                                "max_read_buffer_bytes": 32768}}},
                    "sparkle-arm64-probe.json": {"original_framework_bytes": 5000000,
                        "arm64_framework_bytes": 3000000, "saved_bytes": 2000000,
                        "saved_percent": 40.0, "runtime_status": "pending fixture"},
                }
                for name, record in fixtures.items():
                    (evidence / name).write_text(json.dumps(record))
            with patch.object(REPORT, "ROOT", root), patch.object(REPORT, "EVIDENCE", evidence), contextlib.redirect_stdout(io.StringIO()):
                REPORT.main()
            return (root / "docs/research/2026-10-10-package-size-implementation-report.html").read_text()

    # 场景：已有安装样本与当前新包没有相同来源证明。构造 93 MB 样本和 21 MB 源清单。
    # 执行：生成报告并读取样本与新包表；预期样本可见，新 App/DMG 仍各自未测量。
    def test_sample_and_source_distribution_do_not_replace_new_package_baseline(self):
        page = self.render()
        self.assertIn("93.000 MB", page, "样本必须读取新证据，不能沿用旧值")
        self.assertIn("0.0.9", page, "样本版本必须随证据变化")
        self.assertIn("原资源分布", page, "源资源分组应集中展示")
        for label in ("App", "DMG"):
            found = re.search(r"<tr><td>" + label + r"</td>(.*?)</tr>", page)
            self.assertIsNotNone(found, f"必须保留独立的新 {label} 基线行")
            self.assertEqual(found.group(1).count("未测量"), 2, f"新 {label} 两组缺数值时必须未测量")
            self.assertNotIn("93.000", found.group(1), "旧样本不能成为新的包体积证据")

    # 场景：修复包含预先安装步骤，二者耗时不同。构造 9.999 秒预装和 2.750 秒修复。
    # 执行：生成局部安装表；预期用修复耗时、逐模式峰值，不将探针当作 App 性能。
    def test_local_probe_uses_repair_time_and_keeps_its_scope(self):
        page = self.render()
        self.assertIn("2.750", page, "修复行必须读取 repair 的完整测量耗时")
        self.assertIn("1.250", page, "首次安装行必须读取自己的耗时")
        self.assertNotIn("9.999", page, "修复行不能错用预先安装耗时")
        for expected in ("8.00 MiB", "12.00 MiB", "4.00 MiB"):
            self.assertIn(expected, page, "每种模式的峰值必须按实际记录展示")
        self.assertIn("HTTP/GPUI", page, "必须说明未覆盖网络/UI/生产程序性能")

    # 场景：框架副本可以缩小，但正式打包尚未采用。构造 5 MB → 3 MB、40% 静态收益。
    # 执行：生成报告；预期使用新数字，并明确未采用，不把框架收益计入新 DMG。
    def test_framework_probe_remains_a_static_candidate(self):
        page = self.render()
        for expected in ("5.000 MB", "3.000 MB", "40.00%"):
            self.assertIn(expected, page, "框架大小及收益必须来自新测量")
        self.assertIn("未采用", page, "静态框架副本尚未通过正式采用验收")

    # 场景：本次没有额外证据，保留媒体 profile。生成报告；预期不借用既有实测常量。
    def test_missing_extra_evidence_never_borrows_old_numbers(self):
        page = self.render(False)
        self.assertIn("未测量", page, "缺失证据需明确待测")
        for stale in ("236.230 MB", "93.000 MB", "2.30 MiB", "44.82%"):
            self.assertNotIn(stale, page, "不得把旧样本或探针数字写死进报告")


if __name__ == "__main__":
    unittest.main()
