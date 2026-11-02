#!/usr/bin/env python3
"""Behavior tests for the public feasibility report CLI; no Windows behavior is mocked as passed."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

PROBE = Path(__file__).parent / "windows/probe.py"
REQUIRED = ["environment", "compile", "graphics", "agents", "speech", "control", "platform", "app-adapters"]


def summary_env(**values):
    env = os.environ.copy()
    for name in ("GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_STEP_SUMMARY"):
        env.pop(name, None)
    return {**env, **values}


class FeasibilityReportTests(unittest.TestCase):
    def invoke(self, reports, metadata=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, checks in reports.items():
                folder = root / name
                folder.mkdir()
                (folder / "report.json").write_text(json.dumps({
                    "task": name, "source_sha": "fixture-sha", "checks": checks,
                    "run_id": "123", "run_attempt": "1", **(metadata or {}).get(name, {}),
                }))
            run = subprocess.run([sys.executable, str(PROBE), "summary", "--output", str(root)],
                                 capture_output=True, text=True,
                                 env=summary_env(GITHUB_SHA="fixture-sha", GITHUB_RUN_ID="123",
                                                 GITHUB_RUN_ATTEMPT="1"))
            report = root / "feasibility.json"
            self.assertTrue(report.exists(), f"CLI must persist its verdict: {run.stderr}")
            return run.returncode, json.loads(report.read_text())

    def reports(self):
        reports = {name: [{"id": name, "status": "PASS", "detail": "fixture observation"}]
                   for name in REQUIRED}
        for task, names in {
            "control": ["identity_tests", "control_tests", "runtime_control_tests",
                        "opencode_transport_tests", "opencode_runtime_tests"],
            "platform": ["memory_platform_tests", "review_platform_tests", "a1_platform_tests"],
            "app-adapters": ["app_adapter_tests"],
        }.items():
            reports[task] = [{"id": name, "status": "PASS", "exit_code": 0,
                              "counts": {"passed": 1, "failed": 0, "ignored": 0}}
                             for name in names]
        return reports

    def passing_reports(self):
        reports = self.reports()
        reports["compile"] = [{"id": name, "status": "PASS", "exit_code": 0}
                              for name in ("norma_compile", "norma_cli_compile", "app_compile", "norma_platform_tests")]
        reports["graphics"] = [{"id": name, "status": "PASS"}
                               for name in ("gpui_window", "gpui_os_keyboard_close")]
        reports["speech"] = [{"id": name, "status": "PASS", "exit_code": 0}
                             for name in ("voice_input_compile", "speech_tests")]
        reports["compile"].append({"id": "canvas_tests", "status": "PASS", "exit_code": 0})
        return reports

    def test_native_feasibility_can_pass_without_claiming_full_product_acceptance(self):
        """全部原生检查通过才允许CI成功；真实模型与候选包仍明确未验。"""
        code, report = self.invoke(self.passing_reports())
        self.assertEqual(code, 0, "原生可行性检查不应永久固定返回失败")
        self.assertFalse(report["goal_complete"], "CI通过不能冒充完整产品迁移验收")
        self.assertEqual(report["goal_gates"]["live_agents"], "NOT_RUN", "不得冒充真实模型调用")
        self.assertEqual(report["goal_gates"]["candidate_package"], "NOT_RUN", "不得冒充候选包验收")

    def test_canvas_is_a_required_zero_exit_gate(self):
        """场景：其余原生证据全绿；分别移除Canvas、置FAIL或PASS+101；
        经公开summary CLI汇总；三者都须非零，不能漏掉画布失败。
        """
        for variant in ("missing", "failed", "nonzero"):
            with self.subTest(variant=variant):
                reports = self.passing_reports()
                canvas = reports["compile"].pop()
                if variant != "missing":
                    canvas.update(status="FAIL" if variant == "failed" else "PASS",
                                  exit_code=101)
                    reports["compile"].append(canvas)
                code, report = self.invoke(reports)
                self.assertNotEqual(code, 0, "Canvas缺失或失败必须阻止可行性CI通过")
                self.assertNotEqual(report["goal_gates"].get("canvas_checks"), "PASS",
                                    "Canvas必须有独立且真实的非PASS状态")

    def test_control_failure_is_a_central_gate(self):
        """场景：八类报告齐全而真实控制测试失败；构造失败与非零PASS两种报告；
        执行summary；预期中央control门槛均为FAIL，不能只把记录藏在checks数组。
        """
        for status in ("FAIL", "PASS"):
            with self.subTest(status=status):
                reports = self.reports()
                reports["control"][1].update(status=status, exit_code=101)
                _, report = self.invoke(reports)
                self.assertEqual(report["goal_gates"].get("control_transport_checks"), "FAIL",
                                 "控制失败和状态/退出码矛盾必须显示为独立FAIL门槛")

    def test_native_reports_must_match_current_source_and_attempt(self):
        """场景：其余七份报告均为本次来源；单独替换任一新增job的SHA或attempt；
        执行公共summary；预期来源或代次不一致，并明确关闭evidence_integrity门槛。
        """
        for task in ("control", "platform", "app-adapters"):
            for field, value in (("source_sha", "wrong-sha"), ("run_attempt", "0")):
                with self.subTest(task=task, field=field):
                    _, report = self.invoke(self.reports(), {task: {field: value}})
                    self.assertEqual(report["goal_gates"]["evidence_integrity"], "FAIL",
                                     "任何一个新增job的旧来源都不能获得完整性PASS")
                    key = "source_consistent" if field == "source_sha" else "provenance_consistent"
                    self.assertFalse(report[key], "必须核验该新增报告本身的身份")

    def test_control_check_cannot_override_an_app_compile_failure(self):
        """场景：控制报告混入与编译同名的check；构造App失败与伪造的同名PASS；
        执行summary；预期编译门槛仍FAIL，task间不能覆盖检查名称。
        """
        reports = self.reports()
        reports["compile"] = [{"id": "norma_compile", "status": "PASS", "exit_code": 0},
                              {"id": "app_compile", "status": "FAIL", "exit_code": 101}]
        reports["control"].append({"id": "app_compile", "status": "PASS", "exit_code": 0})
        _, report = self.invoke(reports)
        self.assertEqual(report["goal_gates"]["product_compile"], "FAIL",
                         "其他task同名检查不能覆盖实际App失败")

    def test_cli_and_helper_failure_cannot_pass_library_compile_gate(self):
        """场景：norma库与App检查成功，但默认CLI/helper失败；构造三个明确退出码；
        执行summary；预期产品编译门槛FAIL，不能仅凭--lib推断默认二进制成功。
        """
        reports = self.reports()
        reports["compile"] = [{"id": name, "status": "PASS", "exit_code": 0}
                              for name in ("norma_compile", "app_compile")]
        reports["compile"].append({"id": "norma_cli_compile", "status": "FAIL", "exit_code": 101})
        _, report = self.invoke(reports)
        self.assertEqual(report["goal_gates"]["product_compile"], "FAIL",
                         "CLI/helper默认检查失败必须关闭编译门槛")

    def test_record_control_checks_require_actual_test_execution(self):
        """场景：PowerShell记录五个退出码；构造真实格式日志、缺日志、零执行及忽略；
        执行公共record-control；预期规范化报告保留退出码与统计，只有实际无忽略执行通过可标PASS。
        """
        for variant in ("pass", "nonzero", "missing_log", "zero_tests", "ignored"):
            with self.subTest(variant=variant), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                folder = root / "control"
                folder.mkdir()
                raw = {"source_sha": "fixture-sha", "run_id": "123", "run_attempt": "1"}
                for field, log in (("identity", "identity"), ("control", "control"),
                                   ("runtime", "runtime"), ("opencode_transport", "opencode_transport"),
                                   ("opencode_runtime", "opencode_runtime")):
                    raw[field + "_exit"] = 101 if variant == "nonzero" and field == "control" else 0
                    (folder / (log + ".log")).write_text(
                        "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n")
                if variant == "missing_log":
                    (folder / "control.log").unlink()
                if variant in ("zero_tests", "ignored"):
                    (folder / "control.log").write_text(
                        f"test result: ok. 0 passed; 0 failed; {int(variant == 'ignored')} ignored;\n")
                path = folder / "report.json"
                path.write_text(json.dumps(raw))
                run = subprocess.run([sys.executable, str(PROBE), "record-control", "--output", str(root)],
                                     capture_output=True, text=True, env=summary_env(GITHUB_SHA="fixture-sha"))
                self.assertEqual(run.returncode, 0, f"规范化必须完成并保留原结果：{run.stderr}")
                data = json.loads(path.read_text())
                self.assertEqual(data["task"], "control", "报告必须明确属于control任务")
                checks = {row["id"]: row for row in data["checks"]}
                expected = {"pass": "PASS", "nonzero": "FAIL", "missing_log": "NOT_RUN",
                            "zero_tests": "NOT_RUN", "ignored": "FAIL"}[variant]
                self.assertEqual(checks["control_tests"]["status"], expected,
                                 f"{variant}必须按实际执行记录判定")
                self.assertEqual(data["control_exit"], raw["control_exit"], "不得覆盖原始退出码")

    def test_app_check_includes_default_binary_targets(self):
        """场景：App 默认构建包含主程序和 helper；采集命令计划；
        预期不只检查 lib，也不靠禁用默认能力或额外 feature 换取通过。
        """
        spec = importlib.util.spec_from_file_location("windows_probe", PROBE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with patch.object(module, "command", return_value={"status": "NOT_RUN"}) as command:
            module.compile_probe(Path("unused-test-evidence"))
        args = next(call.args[2] for call in command.call_args_list if call.args[1] == "app_compile")
        self.assertNotIn("--lib", args, "App evidence must cover the default main/helper targets")
        self.assertNotIn("--no-default-features", args, "Product capabilities must not be disabled")
        self.assertNotIn("--features", args, "Default product evidence cannot use a test-support feature")

    def test_missing_reports_never_pass(self):
        """场景：runner 未产生任何证据；执行 summary；预期非零并列出八类缺失报告。"""
        code, report = self.invoke({})
        self.assertNotEqual(code, 0, "empty evidence must fail the gate")
        self.assertEqual(len(report["missing_reports"]), 8, "all eight report classes are required")

    def test_native_job_reports_cannot_be_silently_missing(self):
        """场景：旧五类报告齐全，但控制、平台与App适配三类未上传；执行公共summary；
        预期精确列出三类缺失，不得把五类报告的来源一致当成全部必跑证据齐全。
        """
        reports = {name: checks for name, checks in self.reports().items()
                   if name not in ("control", "platform", "app-adapters")}
        _, report = self.invoke(reports)
        self.assertEqual(set(report["missing_reports"]), {"control", "platform", "app-adapters"},
                         "三个新增必跑job必须进入中央缺失检查")

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

    def test_speech_gate_requires_current_successful_zero_exit_checks(self):
        """场景：采集已完成后 gate 只读报告；构造成功、非零退出码、失败及旧 attempt；
        执行公共 CLI；预期仅本轮两个检查均零退出成功时通过，不重跑命令掩盖原结果。
        """
        for variant in ("pass", "nonzero", "failed", "stale", "missing_check"):
            with self.subTest(variant=variant), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                folder = root / "speech"
                folder.mkdir()
                checks = [{"id": name, "status": "PASS", "exit_code": 0}
                          for name in ("voice_input_compile", "speech_tests")]
                if variant == "nonzero":
                    checks[1]["exit_code"] = 101
                if variant == "failed":
                    checks[1]["status"] = "FAIL"
                if variant == "missing_check":
                    checks.pop()
                (folder / "report.json").write_text(json.dumps({
                    "task": "speech", "source_sha": "fixture-sha", "run_id": "123",
                    "run_attempt": "0" if variant == "stale" else "1", "checks": checks,
                }))
                result = subprocess.run([
                    sys.executable, str(PROBE), "speech-gate", "--output", str(root)],
                    capture_output=True, text=True,
                    env=summary_env(GITHUB_SHA="fixture-sha", GITHUB_RUN_ID="123", GITHUB_RUN_ATTEMPT="1"))
                self.assertEqual(result.returncode == 0, variant == "pass",
                                 f"gate must preserve the captured result for {variant}: {result.stdout}")

    def test_summary_rejects_speech_pass_with_nonzero_exit_code(self):
        """场景：语音报告出现 PASS 与非零退出码矛盾；构造 exit=101 的 PASS；
        执行汇总；预期与只读 gate 一致显示 FAIL，不能只相信状态标签。
        """
        reports = self.reports()
        reports["speech"] = [{"id": "voice_input_compile", "status": "PASS", "exit_code": 0},
                             {"id": "speech_tests", "status": "PASS", "exit_code": 101}]
        _, report = self.invoke(reports)
        self.assertEqual(report["goal_gates"]["speech_default_checks"], "FAIL",
                         "nonzero exit codes must not be shown as a passed speech sub-gate")


if __name__ == "__main__":
    unittest.main()
