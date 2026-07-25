#!/usr/bin/env python3
"""Unit tests for the deterministic CPU performance result gate."""

from __future__ import annotations

import importlib.util
import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "cpu-performance-gate.py"
SPEC = importlib.util.spec_from_file_location("cpu_performance_gate", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load CPU performance gate from {SCRIPT_PATH}")
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class CpuPerformanceGateTests(unittest.TestCase):
    def test_cpu_statistics_uses_nearest_rank_p95(self) -> None:
        """场景：统计函数必须产生独立可复算的 median/P95/max。

        数据构造：
          samples = 1..20，共 20 个样本
          median = (10 + 11) / 2 = 10.5
          P95 nearest-rank index = ceil(20 × 0.95) - 1 = 18 → value 19
          max = 20

        执行过程：
          1. 将逆序样本交给 cpu_statistics，排除调用方预排序的隐含前提。
          2. 分别读取 sample_count、median、P95 和 max。

        预期结果：
          - 正断言：四项统计量与手工推导完全一致。
          - 负断言：P95 不应误用最大值 20。
        """
        stats = GATE.cpu_statistics(list(reversed(range(1, 21))))

        self.assertEqual(stats["sample_count"], 20, "统计结果必须保留全部 20 个样本")
        self.assertEqual(stats["median"], 10.5, "1..20 的中位数应为 10.5")
        self.assertEqual(stats["p95"], 19, "nearest-rank P95 应为第 19 个值")
        self.assertEqual(stats["max"], 20, "最大值应为 20")
        self.assertNotEqual(stats["p95"], 20, "P95 不应被错误实现为最大值")

    def test_compare_requires_at_least_fifty_percent_reduction(self) -> None:
        """场景：候选 CPU 恰好降低 50% 时通过，低于 50% 时失败。

        数据构造：
          baseline median = 40.0
          passing candidate = 20.0 → (40 - 20) / 40 = 50%
          failing candidate = 20.4 → (40 - 20.4) / 40 = 49%
          场景、主机、时长、节点数和活跃 Agent 数保持相同。

        执行过程：
          1. 写入 baseline、50% candidate 和 49% candidate 三份 summary。
          2. 用 min_reduction_percent=50 分别执行 compare。
          3. 读取报告中的 passed 和 reduction_percent。

        预期结果：
          - 正断言：50% 候选通过，报告精确记录 50%。
          - 负断言：49% 候选失败，不能用四舍五入放宽门禁。
        """
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            baseline = self._write_summary(root / "baseline.json", 40.0)
            passing = self._write_summary(root / "passing.json", 20.0)
            failing = self._write_summary(root / "failing.json", 20.4)

            passing_report = self._compare(root, baseline, passing, "passing-report.json")
            failing_report = self._compare(root, baseline, failing, "failing-report.json")

            self.assertTrue(passing_report["passed"], "CPU 恰好降低 50% 应满足发布门禁")
            self.assertEqual(
                passing_report["reduction_percent"],
                50.0,
                "通过报告必须记录精确的 50% 降幅",
            )
            self.assertFalse(failing_report["passed"], "CPU 仅降低 49% 必须阻断验收")
            self.assertEqual(
                failing_report["reduction_percent"],
                49.0,
                "失败报告应明确记录实际 49% 降幅",
            )

    def test_compare_rejects_mismatched_scenarios(self) -> None:
        """场景：不同场景或机器的结果不得产生虚假的 50% 性能结论。

        数据构造：
          baseline = idle-seven-cards / host-a / 7 nodes / 0 active agents
          candidate = two-active-agents / host-a / 7 nodes / 2 active agents
          median 40 → 10，表面降幅 = 75%，但场景不兼容。

        执行过程：
          1. 构造性能数字更好的候选结果。
          2. 改变 scenario 和 active_agent_count。
          3. 执行 compare 并检查兼容性条目。

        预期结果：
          - 正断言：报告保留 75% 数值，便于诊断。
          - 负断言：总门禁不得通过，两个不兼容字段都必须失败。
        """
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            baseline = self._write_summary(root / "baseline.json", 40.0)
            candidate = self._write_summary(
                root / "candidate.json",
                10.0,
                scenario="two-active-agents",
                active_agent_count=2,
            )

            report = self._compare(root, baseline, candidate, "mismatch-report.json")
            checks = {check["name"]: check for check in report["checks"]}

            self.assertEqual(
                report["reduction_percent"],
                75.0,
                "报告应保留表面 75% 降幅作为诊断信息",
            )
            self.assertFalse(report["passed"], "不同性能场景不得通过总门禁")
            self.assertFalse(
                checks["same_scenario"]["passed"],
                "scenario 不同必须使兼容性检查失败",
            )
            self.assertFalse(
                checks["same_active_agent_count"]["passed"],
                "活跃 Agent 数不同必须使兼容性检查失败",
            )

    def test_repeatability_requires_medians_within_twenty_percent(self) -> None:
        """场景：同一基线场景连续两次的 median 差异必须不超过 20%。

        数据构造：
          reference median = 40.0
          stable rerun median = 48.0 → |48 - 40| / 40 = 20%
          unstable rerun median = 48.4 → |48.4 - 40| / 40 = 21%
          其余场景身份字段完全相同。

        执行过程：
          1. 构造 reference、边界稳定和超界不稳定三份 summary。
          2. 以 max_median_delta_percent=20 运行 repeatability。
          3. 检查两份报告的边界计算和总状态。

        预期结果：
          - 正断言：差异恰好 20% 时通过。
          - 负断言：差异 21% 时失败，禁止用不稳定基线证明优化。
        """
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            reference = self._write_summary(root / "reference.json", 40.0)
            stable = self._write_summary(root / "stable.json", 48.0)
            unstable = self._write_summary(root / "unstable.json", 48.4)

            stable_report = self._repeatability(
                root, reference, stable, "stable-repeatability.json"
            )
            unstable_report = self._repeatability(
                root, reference, unstable, "unstable-repeatability.json"
            )

            self.assertTrue(
                stable_report["passed"],
                "连续两次 median 差异恰好 20% 应满足基线稳定性门槛",
            )
            self.assertEqual(
                stable_report["median_delta_percent"],
                20.0,
                "稳定报告应记录手工推导的 20% 差异",
            )
            self.assertFalse(
                unstable_report["passed"],
                "连续两次 median 差异 21% 必须阻断优化阶段",
            )
            self.assertEqual(
                unstable_report["median_delta_percent"],
                21.0,
                "不稳定报告应记录实际 21% 差异",
            )

    def _write_summary(
        self,
        path: Path,
        median: float,
        *,
        scenario: str = "idle-seven-cards",
        active_agent_count: int = 0,
    ) -> Path:
        payload = {
            "schema_version": 1,
            "stage": "baseline",
            "scenario": scenario,
            "commit": "fixture",
            "host_id": "host-a",
            "duration_seconds": 60,
            "node_count": 7,
            "active_agent_count": active_agent_count,
            "active_terminal_count": 0,
            "app": {
                "sample_count": 60,
                "median": median,
                "p95": median,
                "max": median,
            },
            "window_server": {
                "sample_count": 60,
                "median": 5.0,
                "p95": 5.0,
                "max": 5.0,
            },
        }
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def _compare(
        self,
        root: Path,
        baseline: Path,
        candidate: Path,
        report_name: str,
    ) -> dict:
        args = type(
            "Args",
            (),
            {
                "baseline": baseline,
                "candidate": candidate,
                "output": root / report_name,
                "min_reduction_percent": 50.0,
                "candidate_max_median": None,
            },
        )()
        with redirect_stdout(io.StringIO()):
            GATE.compare(args)
        return json.loads((root / report_name).read_text(encoding="utf-8"))

    def _repeatability(
        self,
        root: Path,
        reference: Path,
        rerun: Path,
        report_name: str,
    ) -> dict:
        args = type(
            "Args",
            (),
            {
                "reference": reference,
                "rerun": rerun,
                "output": root / report_name,
                "max_median_delta_percent": 20.0,
            },
        )()
        with redirect_stdout(io.StringIO()):
            GATE.repeatability(args)
        return json.loads((root / report_name).read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
