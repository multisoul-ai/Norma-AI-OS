#!/usr/bin/env python3
"""Focused regression checks for the translation guardrail."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("check_i18n", Path(__file__).with_name("check-i18n.py"))
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class CatalogTests(unittest.TestCase):
    # 场景：不完整双语键与命名占位符不一致必须被拦截。
    # 数据构造：英文 2 个键，中文 1 个键；same 的英文参数 path，中文参数 reason。
    # 执行过程：写入隔离词库 → 调用校验 → 读取两条具体错误。
    # 预期结果：missing 缺译与 same 参数错误均存在，不能被报告为通过。
    def test_detects_missing_key_and_mismatched_arguments(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "test.en.json").write_text(json.dumps({"test.same": "%{path}", "test.missing": "Missing"}))
            (root / "test.zh-CN.json").write_text(json.dumps({"test.same": "%{reason}"}))
            errors, _ = checker.check_catalogs(root)
            self.assertIn("unpaired bilingual key: test.missing", errors, "缺少中文翻译必须失败")
            self.assertIn("placeholder mismatch: test.same", errors, "占位符名称不一致必须失败")
            self.assertNotEqual(errors, [], "非法词库不能通过")

    # 场景：新增硬编码产品文案必须失败，品牌与 URL 必须保留原文。
    # 数据构造：源文件含 1 条产品句子、1 个品牌、1 个既定示例 URL。
    # 执行过程：写入隔离 app source → 调用 source 校验 → 检查错误内容。
    # 预期结果：仅产品句子报错；品牌与 URL 不报错。
    def test_product_literal_is_rejected_and_brand_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "crates/app/src/view.rs"
            source.parent.mkdir(parents=True)
            source.write_text('.child("Open your project").child("Norma").placeholder("https://github.com/owner/repository.git")')
            errors = checker.check_sources(root, set())
            self.assertEqual(len(errors), 1, "三项中仅产品句子应被拦截")
            self.assertIn("Open your project", errors[0], "必须定位产品硬编码")
            self.assertNotIn("hardcoded UI copy 'Norma'", errors[0], "品牌不应翻译")

    # 场景：UI 的 match/if 分支硬编码也必须拦截。
    # 数据构造：match 的 IMAGE 与 GIF，if 的 On-device 与已翻译分支，共两条产品文案。
    # 执行过程：写入隔离 source → 扫描分支 → 核验 IMAGE 和 On-device。
    # 预期结果：两条产品文案报错；GIF 和翻译键不能被误报。
    def test_conditional_ui_literals_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "crates/app/src/view.rs"
            source.parent.mkdir(parents=True)
            source.write_text('.child(match kind { Static => "IMAGE", Gif => "GIF" })'
                              '.child(if active { "On-device" } else { ui::i18n::text("voice.on_device") })')
            errors = checker.check_sources(root, {"voice.on_device"})
            self.assertEqual(len(errors), 2, "match 和 if 中的产品文案都必须被拦截")
            self.assertTrue(any("IMAGE" in error for error in errors), "必须检出壁纸类型标签")
            self.assertTrue(any("On-device" in error for error in errors), "必须检出条件按钮文案")
            self.assertFalse(any("'GIF'" in error for error in errors), "文件格式标识不应翻译")

    # 场景：卡片类型标签 helper 的返回值也必须受文案护栏保护。
    # 数据构造：node_type_label 返回一个硬编码 TERMINAL；另一个分支调用合法翻译键。
    # 执行过程：在隔离 canvas helper 中写入两个分支 → 调用 source 校验。
    # 预期结果：TERMINAL 报错，翻译键不被当作硬编码文案。
    def test_node_label_helper_literals_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "crates/app/src/canvas/node_label.rs"
            source.parent.mkdir(parents=True)
            source.write_text('match kind { Term => "TERMINAL", Browser => ui::i18n::text("canvas.type_browser") }')
            errors = checker.check_sources(root, {"canvas.type_browser"})
            self.assertEqual(len(errors), 1, "仅硬编码类型标签应被拦截")
            self.assertIn("TERMINAL", errors[0], "必须检出返回值中的英文类型标签")
            self.assertNotIn("hardcoded UI copy 'canvas.type_browser'", errors[0], "翻译键不能误报")


if __name__ == "__main__":
    unittest.main()
