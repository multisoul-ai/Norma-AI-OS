#!/usr/bin/env python3
"""Focused tests for the cross-file domain contract checker."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


CHECKER_PATH = Path(__file__).with_name("check-domain-contracts.py")
SPEC = importlib.util.spec_from_file_location("check_domain_contracts", CHECKER_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"could not load domain contract checker from {CHECKER_PATH}")
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class AgentProfileContractTests(unittest.TestCase):
    def write_fixture(self, root: Path, *, include_grok_in_schema: bool) -> None:
        profile_variants = """pub enum AgentProfile {
    Claude,
    Codex,
    Cursor,
    Grok,
    OpenCode,
}
"""
        schema_profiles = (
            '["claude", "codex", "cursor", "grok", "opencode"]'
            if include_grok_in_schema
            else '["claude", "codex", "cursor", "opencode"]'
        )
        files = {
            "crates/norma/src/protocol.rs": profile_variants,
            "crates/canvas/src/node/agent_profile.rs": profile_variants,
            "crates/agent/src/adapter.rs": f'''definition(
    "create_agent",
    object_schema(json!({{
        "profile": {{"type": "string", "enum": {schema_profiles}}}
    }})),
),
definition("assign_agent_task", object_schema(json!({{}}))),
''',
            "crates/agent/src/adapter/commands.rs": """match args.profile.as_str() {
    "claude" => AgentProfile::Claude,
    "codex" => AgentProfile::Codex,
    "cursor" => AgentProfile::Cursor,
    "grok" => AgentProfile::Grok,
    "opencode" => AgentProfile::OpenCode,
}
""",
            "crates/storage/src/agent_definition_store.rs": """match profile {
    AgentProfile::Claude => "claude",
    AgentProfile::Codex => "codex",
    AgentProfile::Cursor => "cursor",
    AgentProfile::Grok => "grok",
    AgentProfile::OpenCode => "opencode",
}
match value {
    "claude" => Ok(AgentProfile::Claude),
    "codex" => Ok(AgentProfile::Codex),
    "cursor" => Ok(AgentProfile::Cursor),
    "grok" => Ok(AgentProfile::Grok),
    "opencode" => Ok(AgentProfile::OpenCode),
}
""",
        }
        for relative_path, source in files.items():
            path = root / relative_path
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(source)

    def test_split_adapter_sources_preserve_agent_profile_contract(self) -> None:
        """当前布局：schema 与 parser 拆分后仍应通过完整 AgentProfile 契约检查。

        数据构造（含关键数值的推导过程）：
          AgentProfile variants = 5（Claude + Codex + Cursor + Grok + OpenCode）
          schema profiles       = 5（与 variants 一一对应）
          parser mappings       = 5（与 variants 一一对应）

        执行过程（逐步说明系统如何处理）：
          1. 在临时仓库创建 adapter.rs schema 与 adapter/commands.rs parser
          2. checker 分别读取两个 canonical 文件
          3. checker 对比 wire、canvas、schema、parser 与 storage 映射

        预期结果：
          - 正断言：完整的五种 profile 不产生失败项
          - 负断言：不得再尝试读取已删除的 crates/agent/src/tools.rs
        """
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_fixture(root, include_grok_in_schema=True)

            failures = CHECKER.check_agent_profile_contract(root)
            legacy_tools_exists = (root / "crates/agent/src/tools.rs").exists()

        self.assertEqual(
            failures,
            [],
            "split adapter schema/parser sources should satisfy the AgentProfile contract",
        )
        self.assertFalse(
            legacy_tools_exists,
            "fixture must not recreate the deleted legacy tools.rs path",
        )

    def test_split_adapter_schema_reports_missing_profile(self) -> None:
        """缺失映射：schema 少一个 profile 时应返回明确失败，而不是静默通过。

        数据构造（含关键数值的推导过程）：
          AgentProfile variants = 5
          schema profiles       = 4（5 - Grok = 4）
          parser mappings       = 5

        执行过程（逐步说明系统如何处理）：
          1. 构造不含 grok 的 create_agent schema
          2. 保持 parser 与 storage 的五种映射完整
          3. checker 比较 schema 集合与 wire variants

        预期结果：
          - 正断言：失败信息明确指向 create_agent schema profiles
          - 负断言：缺失 grok 的 schema 不得通过契约检查
        """
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_fixture(root, include_grok_in_schema=False)

            failures = CHECKER.check_agent_profile_contract(root)

        report = "\n".join(failures)
        self.assertIn(
            "create_agent schema profiles",
            report,
            "missing Grok should produce a concrete create_agent schema failure",
        )
        self.assertNotEqual(
            failures,
            [],
            "a schema missing Grok must not pass the AgentProfile contract",
        )


if __name__ == "__main__":
    unittest.main()
