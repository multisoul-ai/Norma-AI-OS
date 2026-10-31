"""实验报告的公开计算接口；独立手算小样本，防止缺失结果被当作成功。"""
import importlib.util
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "voice_report", Path(__file__).resolve().parents[2] / "scripts/voice-eval/report.py")
report = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = report
SPEC.loader.exec_module(report)


class ReportTests(unittest.TestCase):
    def test_route_cache_is_bound_to_the_actual_asr_file(self):
        """场景：新旧 ASR 可以具有相同 id/repeat，但转写不同。
        构造有源哈希的路由旁车；执行一次合法校验，再替换 ASR 内容。
        预期：正确配对允许，旧路由不能被新转写复用，配置哈希也必须一致。"""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            asr = root / "asr-A0.jsonl"
            route = root / "route-A0-R0-L0.jsonl"
            asr.write_text('{"id":"a","repeat":0,"text":"Daisy"}\n')
            routes = [{"id": "a", "repeat": 0, "config_sha256": "2" * 64}]
            route.write_text(json.dumps(routes[0]) + "\n")
            manifest = {"asr_sha256": hashlib.sha256(asr.read_bytes()).hexdigest(),
                        "config_sha256": "2" * 64}
            route.with_suffix(".manifest.json").write_text(json.dumps(manifest))
            report.validate_route_binding(asr, route, routes)
            asr.write_text('{"id":"a","repeat":0,"text":"Alan"}\n')
            with self.assertRaisesRegex(ValueError, "ASR"):
                report.validate_route_binding(asr, route, routes)

    def test_mixed_mer_and_name_boundaries(self):
        """场景：全角和大小写归一化，中文逐字、英文逐词。构造 4 单位，
        替换其中 1 单位；执行评分与名字匹配；预期 MER=1/4，Alanine 不算 Alan。"""
        self.assertEqual(report.mixed_tokens("ＡＰＩ，修复 don't"),
                         ["api", "修", "复", "don't"], "应复用历史归一化口径")
        self.assertEqual(report.edit_distance("API 修复 bug", "API 修复 PR"),
                         (1, 4), "一个替换除以四个参考单位")
        self.assertFalse(report.contains_term("Alanine 检查", "Alan"), "英文名必须有词边界")
        self.assertTrue(report.contains_term("让戴西去做", "戴西"), "中文名字可以嵌在句中")

    def test_model_fallback_is_unknown_and_wrong_dispatch_is_attributed(self):
        """场景：三个样本包含路由错派、识别错派、未执行模型兜底。
        构造前两条期望 Daisy，分别转写 Daisy/Alan 却都派 Alan；第三条 Skip。
        执行汇总；预期各一种错派，未知样本不冒充正确，也不计作零延迟模型调用。"""
        corpus = {str(i): {"text": "Daisy，检查", "targets": ["Daisy"], "action": "assign",
                           "terms": [{"text": "Daisy", "kind": "name", "agent": "Daisy"}]}
                  for i in range(3)}
        asr = [{"id": str(i), "repeat": 0, "text": t, "t_final_ms": 10, "warm": True}
               for i, t in enumerate(["Daisy 检查", "Alan 检查", "Daisy 检查"])]
        routes = [{"id": str(i), "repeat": 0, "path": "direct", "targets": ["Alan"],
                   "action": "assign", "t_route_ms": 1} for i in range(2)]
        routes.append({"id": "2", "repeat": 0, "path": "model", "targets": None,
                       "action": None, "t_route_ms": None})
        result = report.summarize(corpus, asr, routes)
        self.assertEqual(result["routing_wrong_count"], 1, "转写正确仍派错应归因路由")
        self.assertEqual(result["recognition_wrong_count"], 1, "名字已错应归因识别")
        self.assertIsNone(result["target_accuracy"], "模型未调用时不可声称最终目标正确率")
        self.assertEqual(result["known_target_accuracy"], 0, "已知的两个结果均错误")
        self.assertEqual(result["route_unresolved"], 1, "兜底必须显式记为未决")
        self.assertEqual(result["t_route_ms"]["n"], 2, "不得把 Skip 耗时当模型时延")

    def test_hallucinated_agent_names_are_recognition_errors(self):
        """场景：无点名原话被 ASR 增添 Alan/艾伦，R0 依照新名字派发。
        构造同一 Agent 的英文名和中文别名；执行完整汇总。
        预期：两条都归因识别，不污染路由错派率；归因不明时不得宣称零错派。"""
        roster = [{"name": "Alan", "speech_alias": "艾伦"}, {"name": "Daisy", "speech_alias": "黛西"}]
        corpus = {"a": {"text": "帮我检查", "targets": [], "action": "none", "terms": []}}
        route = [{"id": "a", "repeat": 0, "path": "direct", "targets": ["Alan"],
                  "action": "assign", "t_route_ms": 1}]
        for text in ["Alan，帮我检查", "艾伦，帮我检查"]:
            with self.subTest(text=text):
                result = report.summarize(corpus, [{"id": "a", "repeat": 0, "text": text}], route, roster)
                self.assertEqual(result["recognition_wrong_count"], 1, "新增人名来自识别幻觉")
                self.assertEqual(result["routing_wrong_count"], 0, "按 ASR 新名字路由不是代码另选目标")
        corpus["a"] = {"text": "Daisy，检查", "targets": ["Daisy"], "action": "assign", "terms": []}
        unknown = report.summarize(corpus, [{"id": "a", "repeat": 0, "text": "检查"}], route, roster)
        self.assertEqual(unknown["unattributed_wrong_count"], 1, "ASR 漏名字、路由又派无关人时因果未决")
        self.assertIsNone(unknown["routing_wrong_rate"], "归因未决不能用零路由错派通过门槛")

    def test_paired_statistics_and_missing_trials(self):
        """场景：八条配对样本全部由错变对。构造 8 个独立配对；
        执行双侧精确 McNemar 和配对 bootstrap；预期 p=2/256，时延差恒为 2。"""
        result = report.mcnemar({str(i): False for i in range(8)},
                                {str(i): True for i in range(8)})
        self.assertEqual(result["improved"], [str(i) for i in range(8)], "应列出全部改进样本")
        self.assertEqual(result["p_exact"], 0.0078125, "双侧二项分布尾部为 2/256")
        ci = report.latency_difference_ci([[10, 10], [20, 20]], [[12, 12], [22, 22]], 100)
        self.assertEqual(ci["p95"], [2, 2], "整对重采样应保留同一音频的关联")
        with self.assertRaisesRegex(ValueError, "配对"):
            report.mcnemar({"a": True}, {"b": True})

    def test_human_transcript_reports_stay_outside_repository(self):
        """场景：用户可替换录音 ID。构造 source=human-holdout 的自定义 ID；
        执行输出目录检查；预期仓库内拒绝，仓库外本机私有目录允许。"""
        samples = [{"id": "my-hard-example", "source": "human-holdout"}]
        repo = Path(__file__).resolve().parents[2]
        with self.assertRaisesRegex(ValueError, "真人"):
            report.check_private_output(samples, repo / "test/voice-eval/results/oops")
        report.check_private_output(samples, Path.home() / ".norma/voice-eval/results/private")

    def test_cache_audio_identity_and_configuration_must_match(self):
        """场景：同名样本可能来自另一版音频或混入热词配置。
        构造完整三次 A0 观测，先验证合法输入，再分别篡改音频哈希和配置。
        预期：合法完整输入允许，错音频/混配置均拒绝，不能生成看似完整的报告。"""
        corpus = {"a": {"sha256": "1" * 64}}
        rows = [{"id": "a", "repeat": r, "group": "A0", "audio_sha256": "1" * 64,
                 "config_sha256": "2" * 64} for r in range(3)]
        report.validate_asr_rows(corpus, rows, "A0", 3)
        rows[2]["audio_sha256"] = "3" * 64
        with self.assertRaisesRegex(ValueError, "音频"):
            report.validate_asr_rows(corpus, rows, "A0", 3)
        rows[2]["audio_sha256"] = "1" * 64
        rows[2]["config_sha256"] = "4" * 64
        with self.assertRaisesRegex(ValueError, "配置"):
            report.validate_asr_rows(corpus, rows, "A0", 3)
        rows[2]["config_sha256"] = "2" * 64
        for row in rows:
            row["realtime"] = True
        rows[2]["realtime"] = False
        with self.assertRaisesRegex(ValueError, "回放"):
            report.validate_asr_rows(corpus, rows, "A0", 3)


if __name__ == "__main__":
    unittest.main()
