#!/usr/bin/env python3
"""报告结论必须与输入证据同步，缺失测量不能沿用旧数字。"""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("size_report", Path(__file__).with_name("report-package-size.py"))
REPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPORT)


class ReportEvidenceTests(unittest.TestCase):
    def render(self, measured):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = root / "docs/research/assets/2026-10-10-package-size-implementation"
            evidence.mkdir(parents=True)
            profile = {"profile": "light", "original_bytes": 100000000,
                "originals_bundled_bytes": 49000000, "preview_bytes": 1000000,
                "catalog_bytes": 0, "payload_bytes": 50000000, "net_saved_percent": 50.0}
            (evidence / "media-profile-comparison.json").write_text(json.dumps([profile]))
            algorithms = evidence.parent / "2026-10-09-resource-compression"
            algorithms.mkdir()
            (algorithms / "summary.csv").write_text("dataset,algorithm,compressed_bytes\n")
            if measured:
                (evidence / "baseline.json").write_text(json.dumps({"binary_bytes": 2000000, "app_bytes": None, "dmg_bytes": None}))
                (evidence / "candidate-gpui-isolated.json").write_text(json.dumps({"binary_bytes": 1234000, "build_status": "passed", "gpui_test_support": False}))
                (evidence / "github-availability.json").write_text(json.dumps({"checked_at_utc": "fixture-time", "resources": [{"status": 200}]}))
            with patch.object(REPORT, "ROOT", root), patch.object(REPORT, "EVIDENCE", evidence), contextlib.redirect_stdout(io.StringIO()):
                REPORT.main()
            return (root / "docs/research/2026-10-10-package-size-implementation-report.html").read_text()

    # 场景：新测量与旧报告数字不同。用 50% 媒体、1.234 MB 主程序、一个 HTTP 200 构造新证据。
    # 生成完整页面；预期标题/表格同步，旧固定值和 404 结论均消失。
    def test_updated_evidence_changes_all_conclusions(self):
        page = self.render(True)
        self.assertGreaterEqual(page.count("50.00%"), 2, "标题和媒体表应使用同一净节省率")
        self.assertIn("1.234 MB", page, "主程序应读取新的实际测量")
        self.assertIn("fixture-time", page, "可用性结论应标出证据检查时间")
        self.assertNotIn("80.50%", page, "不得保留旧媒体标题")
        self.assertNotIn("当前为 404", page, "新的可用性证据不能仍称旧地址不可用")

    # 场景：尚无主程序、包和下载可用性证据。仅保留媒体测量；预期明确待测，不填旧数字。
    def test_missing_evidence_is_unmeasured(self):
        page = self.render(False)
        self.assertIn("未测量", page, "缺证据必须明确标为未测量")
        self.assertNotIn("58.245 MB", page, "不能借用旧主程序基线")
        self.assertNotIn("58.430 MB", page, "不能借用旧优化候选")
        self.assertNotIn("当前为 404", page, "没有可用性检查不能声称不可用")


if __name__ == "__main__":
    unittest.main()
