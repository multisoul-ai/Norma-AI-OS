#!/usr/bin/env python3
"""Behavior tests for the public feasibility report CLI; no Windows behavior is mocked as passed."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

PROBE = Path(__file__).parent / "windows/probe.py"
REQUIRED = ["environment", "compile", "graphics", "agents", "speech"]


def summary_env(**values):
    env = os.environ.copy()
    for name in ("GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_STEP_SUMMARY"):
        env.pop(name, None)
    return {**env, **values}


class FeasibilityReportTests(unittest.TestCase):
    def invoke(self, reports):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, checks in reports.items():
                folder = root / name
                folder.mkdir()
                (folder / "report.json").write_text(json.dumps({
                    "task": name, "source_sha": "fixture-sha", "checks": checks,
                }))
            run = subprocess.run([sys.executable, str(PROBE), "summary", "--output", str(root)],
                                 capture_output=True, text=True, env=summary_env())
            report = root / "feasibility.json"
            self.assertTrue(report.exists(), f"CLI must persist its verdict: {run.stderr}")
            return run.returncode, json.loads(report.read_text())

    def reports(self):
        return {name: [{"id": name, "status": "PASS", "detail": "fixture observation"}]
                for name in REQUIRED}

    def test_missing_reports_never_pass(self):
        """场景：runner 未产生任何证据；执行 summary；预期非零并列出五类缺失报告。"""
        code, report = self.invoke({})
        self.assertNotEqual(code, 0, "empty evidence must fail the gate")
        self.assertEqual(len(report["missing_reports"]), 5, "all five report classes are required")

    def test_unconfigured_live_agents_and_candidate_remain_not_run(self):
        """场景：环境探针均成功但未提供模型凭据和产品包；执行 summary；
        预期仍非零，真实 Agent 和候选包独立保留 NOT_RUN，不由 CLI version 代替。
        """
        code, report = self.invoke(self.reports())
        self.assertNotEqual(code, 0, "readiness does not prove live Agent or candidate acceptance")
        self.assertEqual(report["goal_gates"]["live_agents"], "NOT_RUN",
                         "no live model invocation was authorized")
        self.assertEqual(report["goal_gates"]["candidate_package"], "NOT_RUN",
                         "the GPUI sample must never be labeled a product candidate")

    def test_compile_failure_and_unsupported_graphics_are_preserved(self):
        """场景：Windows 编译失败且无图形桌面；构造两个明确失败项后执行 summary；
        预期原样保留 FAIL/ENV_UNSUPPORTED，整体不可通过。
        """
        reports = self.reports()
        reports["compile"][0]["status"] = "FAIL"
        reports["graphics"][0]["status"] = "ENV_UNSUPPORTED"
        code, report = self.invoke(reports)
        statuses = {row["id"]: row["status"] for row in report["checks"]}
        self.assertNotEqual(code, 0, "failed or unsupported gates must remain non-green")
        self.assertEqual(statuses["compile"], "FAIL", "compiler failure must remain a failure")
        self.assertEqual(statuses["graphics"], "ENV_UNSUPPORTED", "missing desktop is not PASS")

    def test_reports_from_different_commits_are_rejected(self):
        """场景：一份证据来自另一提交；经 CLI 汇总；预期报告身份校验失败。"""
        # A second invocation writes deliberately inconsistent source identities.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, checks in self.reports().items():
                p = root / name
                p.mkdir()
                (p / "report.json").write_text(json.dumps({
                    "task": name, "source_sha": "other" if name == "graphics" else "fixture-sha",
                    "checks": checks,
                }))
            run = subprocess.run([sys.executable, str(PROBE), "summary", "--output", str(root)],
                                 capture_output=True, text=True, env=summary_env())
            p = root / "feasibility.json"
            self.assertTrue(p.exists(), f"identity mismatch must still leave evidence: {run.stderr}")
            report = json.loads(p.read_text())
        self.assertNotEqual(run.returncode, 0, "mixed source commits must be rejected")
        self.assertFalse(report["source_consistent"], "evidence cannot be assembled across commits")

    def test_older_run_attempt_is_not_current_evidence(self):
        """场景：四份报告源码相同但均属于 attempt 1；要求 attempt 2 后汇总；
        预期身份校验拒绝旧证据，不能仅凭 SHA 一致就接受。
        """
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, checks in self.reports().items():
                p = root / name
                p.mkdir()
                (p / "report.json").write_text(json.dumps({
                    "task": name, "source_sha": "fixture-sha", "checks": checks,
                    "run_id": "123", "run_attempt": "1",
                }))
            run = subprocess.run([sys.executable, str(PROBE), "summary", "--output", str(root)],
                                 capture_output=True, text=True,
                                 env=summary_env(GITHUB_RUN_ID="123", GITHUB_RUN_ATTEMPT="2"))
            report = json.loads((root / "feasibility.json").read_text())
        self.assertNotEqual(run.returncode, 0, "stale attempts cannot pass the evidence gate")
        self.assertFalse(report["provenance_consistent"], "same-SHA older attempt must be rejected")

    def test_window_pixels_do_not_override_failed_keyboard_input(self):
        """场景：GPUI RGB 像素可见但 OS 键盘关闭失败；经汇总后图形交互门槛仍失败，
        不可只读取 window PASS 就忽略同一场景的输入失败。
        """
        reports = self.reports()
        reports["graphics"] = [{"id": "gpui_window", "status": "PASS"},
                               {"id": "gpui_os_keyboard_close", "status": "FAIL"}]
        _, report = self.invoke(reports)
        self.assertEqual(report["goal_gates"]["gpui_graphics"], "FAIL",
                         "visible pixels alone do not prove OS input support")

    def test_speech_test_failure_is_preserved_after_voice_input_compiles(self):
        """场景：VAD 默认构建通过，但相关原生测试失败；经 CLI 汇总；
        预期单独保留语音检查 FAIL，不能只看编译或已有图形结果。
        """
        reports = self.reports()
        reports["speech"] = [{"id": "voice_input_compile", "status": "PASS"},
                             {"id": "speech_tests", "status": "FAIL"}]
        code, report = self.invoke(reports)
        self.assertNotEqual(code, 0, "failed speech tests must not produce green acceptance")
        self.assertEqual(report["goal_gates"].get("speech_default_checks"), "FAIL",
                         "speech runtime test failures must remain visible")


if __name__ == "__main__":
    unittest.main()
