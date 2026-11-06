#!/usr/bin/env python3
"""Regression fixtures for the resource-lifecycle source guard."""

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "resource_lifecycle", Path(__file__).with_name("check-resource-lifecycle.py")
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class ResourceLifecycleTests(unittest.TestCase):
    def test_open_bindings_respect_scope_and_shadowing(self):
        """同名 builder 的独立绑定不能串联，嵌套参数也不能隐藏 spawn。

        数据构造：安全 open 后跟另一个同名命令，分别置于独立块及同一块；危险命令参数内有分号。
        执行过程：检查两种安全绑定，再检查含参数块的直接 spawn 和嵌套作用域中的真实 open。
        预期结果：安全绑定通过；两种实际未等待的 open 均拒绝。
        """
        opened = 'let mut cmd = Command::new("open"); let _ = cmd.status();'
        waited = 'let mut cmd = Command::new("true"); let mut child = cmd.spawn().unwrap(); child.wait().unwrap();'
        blocks = 'fn run() { { ' + opened + ' } { ' + waited + ' } }'
        shadowed = 'fn run() { ' + opened + waited + ' }'
        nested_arg = 'fn run() { Command::new("open").arg({ let name = "url"; name }).spawn(); }'
        nested_use = 'fn run() { let mut cmd = Command::new("open"); { if let Err(e) = cmd.spawn() {} } }'
        self.assertEqual(CHECK.check_source("crates/app/src/a.rs", blocks), [],
                         "不相交作用域的同名命令不能误认为 open")
        self.assertEqual(CHECK.check_source("crates/app/src/b.rs", shadowed), [],
                         "被新绑定遮蔽的旧 open 不能污染后续命令")
        self.assertIn("open-child", CHECK.check_source("crates/app/src/c.rs", nested_arg),
                      "参数块内的分号不能隐藏链末尾的 spawn")
        self.assertIn("open-child", CHECK.check_source("crates/app/src/d.rs", nested_use),
                      "嵌套块仍须检查外层捕获的 open builder")

    def test_raw_literal_examples_are_not_executable_code(self):
        """多行及 byte raw 文本中的命令示例不能触发检查。

        数据构造：多行 raw 示例与 byte raw 示例各一个，另含真实 raw open 调用。
        执行过程：分别检查两个纯文字样例，再检查实际执行的 raw 命令。
        预期结果：两个文字样例通过，真实丢弃 Child 的命令拒绝。
        """
        multiline = '''fn run() { let example = r#"
            Command :: new ( "open" ) . spawn ( )
        "#; }'''
        byte_literal = '''fn run() { let example = br#"Command :: new ( "open" ) . spawn ( )"#; }'''
        self.assertEqual(CHECK.check_source("crates/app/src/a.rs", multiline), [],
                         "多行 raw 字符串内容不应作为 Rust 代码检查")
        self.assertEqual(CHECK.check_source("crates/app/src/b.rs", byte_literal), [],
                         "byte raw 字符串内容不应作为 Rust 代码检查")
        self.assertIn("open-child", CHECK.check_source("crates/app/src/c.rs", 'fn run() { Command::new(r#"open"#).spawn(); }'),
                      "真正调用 open 的 raw 字符串仍必须拒绝")

    def test_typed_open_binding_cannot_bypass_guard(self):
        """带类型标注的 open builder 仍须等待回收。

        数据构造：一个显式 std::process::Command 类型变量，spawn 的成功 Child 被丢弃。
        执行过程：检查 if let Err 分支，再将 spawn 改成 status。
        预期结果：丢弃 Child 必须拒绝，等待回收必须通过。
        """
        bad = '''fn run() {
            let mut cmd: std::process::Command = std::process::Command::new("open");
            if let Err(e) = cmd.spawn() { eprintln!("{e}"); }
        }'''
        self.assertIn("open-child", CHECK.check_source("crates/app/src/a.rs", bad),
                      "类型标注不能让被丢弃的 Child 绕过检查")
        self.assertEqual(CHECK.check_source("crates/app/src/a.rs", bad.replace('.spawn()', '.status()')), [],
                         "有类型标注且等待回收的 open 应通过")

    def test_equivalent_open_builders_and_unrelated_spawns(self):
        """同义构造不能绕过检查，其他 builder 不应误报。

        数据构造：raw literal、Command 别名、局部无参 spawn 各一例。
        执行过程：检查三种危险变体，再检查安全 open 与其他 builder 共存。
        预期结果：三个变体均拒绝；不相关 spawn 和纯文字示例均通过。
        """
        raw = 'fn a() { Command::new(r#"open"#).spawn(); }'
        alias = 'use std::process::Command as Proc; fn b() { Proc::new("open").spawn(); }'
        bound = 'fn c() { let mut cmd = Command::new("open"); cmd.spawn(); }'
        safe = 'fn d() { Command::new("open").status(); custom_builder.spawn(); }'
        text = 'fn e() { let example = r#"Command :: new ( "open" ) . spawn ( )"#; }'
        self.assertIn("open-child", CHECK.check_source("crates/app/src/a.rs", raw), "raw open 必须拒绝")
        self.assertIn("open-child", CHECK.check_source("crates/app/src/b.rs", alias), "Command 别名必须拒绝")
        self.assertIn("open-child", CHECK.check_source("crates/app/src/c.rs", bound), "局部变量无参 spawn 必须拒绝")
        self.assertEqual(CHECK.check_source("crates/app/src/d.rs", safe), [], "不同 builder 的 spawn 不应误报")
        self.assertEqual(CHECK.check_source("crates/app/src/e.rs", text), [], "带空格的 raw 示例不是代码")

    def test_index_cannot_be_hidden_by_unstaged_fix(self):
        """提交检查读取 index，而不是未暂存的修复。

        数据构造：index 中一条危险 open；工作树改为等待回收。
        执行过程：临时 git 仓库暂存旧代码 → 修改工作树 → 分别运行两种模式。
        预期结果：--staged 拒绝旧代码；工作树模式通过修复。
        """
        script = Path(__file__).with_name("check-resource-lifecycle.py").resolve()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", directory], check=True)
            source = root / "crates/app/src/example.rs"
            source.parent.mkdir(parents=True)
            source.write_text('fn open() { Command::new("open").spawn(); }')
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            source.write_text('fn open() { Command::new("open").status(); }')
            staged = subprocess.run(["python3", str(script), "--staged"], cwd=root, capture_output=True, text=True)
            working = subprocess.run(["python3", str(script)], cwd=root, capture_output=True, text=True)
        self.assertEqual(staged.returncode, 1, "未暂存修复不能让危险 index 通过")
        self.assertIn("open-child", staged.stdout, "拒绝原因必须是未回收 open")
        self.assertEqual(working.returncode, 0, "工作树中已等待的命令必须通过")
        self.assertNotIn("open-child", working.stdout, "已修复工作树不能保留旧告警")

    def test_open_children_must_be_waited(self):
        """系统打开命令不得丢弃 Child。

        数据构造：2 种命令构造（链式/局部变量），各含一次 spawn。
        执行过程：检查旧实现，再把 spawn 改成同步 status 后检查。
        预期结果：旧实现报告 open-child；等待后的实现不报告问题。
        """
        chained = '''fn launch() { std::thread::spawn(|| {
            if let Err(e) = std::process::Command::new("open").arg(url).spawn() {}
        }); }'''
        bound = '''fn launch() {
            let mut command = Command::new("open");
            command.arg("-R");
            if let Err(e) = command.arg(path).spawn() {}
        }'''
        self.assertIn("open-child", CHECK.check_source("crates/app/src/a.rs", chained),
                      "丢弃链式 open Child 必须失败")
        self.assertIn("open-child", CHECK.check_source("crates/app/src/b.rs", bound),
                      "局部 command 不能绕过回收检查")
        self.assertEqual(CHECK.check_source("crates/app/src/a.rs", chained.replace('.spawn()', '.status()')), [],
                         "后台线程中的 status 必须通过")
        self.assertEqual(CHECK.check_source("crates/app/src/b.rs", bound.replace('.spawn()', '.status()')), [],
                         "局部 command 的 status 必须通过")

    def test_non_process_spawns_and_text_are_ignored(self):
        """异步任务和文档中的示例不能被当成子进程。

        数据构造：线程 spawn、raw string、嵌套注释各一个；真实 open 一个。
        执行过程：先检查纯文本/线程，再追加真实的丢弃 Child。
        预期结果：前者无告警，后者必须报 open-child。
        """
        safe = '''fn run() {
            /* outer /* Command::new("open").spawn(); */ comment */
            let text = r##"Command::new("open").spawn();"##;
            thread::spawn(|| work());
            command.spawn();
        }'''
        self.assertEqual(CHECK.check_source("crates/app/src/a.rs", safe), [],
                         "注释、raw string 和无关 spawn 不能产生误报")
        unsafe = safe + '\nfn open() { Command::new("/usr/bin/open").spawn().ok(); }'
        self.assertIn("open-child", CHECK.check_source("crates/app/src/a.rs", unsafe),
                      "绝对路径 open 与 .ok() 不能绕过检查")

    def test_display_link_cannot_forget_native_resources(self):
        """显示资源销毁不能重新引入 mem::forget。

        数据构造：一个 Drop 中遗忘原生句柄，另一个正常释放。
        执行过程：分别检查危险和正常 Drop。
        预期结果：危险 Drop 报 forgotten-resource；正常 Drop 通过。
        """
        path = "vendor/gpui-0.2.2/src/platform/mac/display_link.rs"
        bad = "impl Drop for DisplayLink { fn drop(&mut self) { std::mem::forget(self.link.take()); } }"
        good = "impl Drop for DisplayLink { fn drop(&mut self) { drop(self.link.take()); } }"
        self.assertIn("forgotten-resource", CHECK.check_source(path, bad),
                      "DisplayLink 遗忘原生对象必须失败")
        self.assertEqual(CHECK.check_source(path, good), [],
                         "正常资源释放不应误报")


if __name__ == "__main__":
    unittest.main()
