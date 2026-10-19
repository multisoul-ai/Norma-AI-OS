#!/usr/bin/env python3
"""通过 CLI 和真实小型 Cargo 项目验证任务缓存隔离与回收。"""
import fcntl
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


SCRIPT = Path(__file__).with_name("task-build-cache.py")


class TaskBuildCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="norma-task-cache-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.workspace = self.root / "workspace with spaces"
        self.workspace.mkdir()
        (self.workspace / "src").mkdir()
        (self.workspace / "Cargo.toml").write_text(
            '[package]\nname = "cache-fixture"\nversion = "0.1.0"\nedition = "2021"\n')
        (self.workspace / "src/main.rs").write_text('fn main() { println!("fixture"); }\n')
        self.environment = dict(os.environ, NORMA_TASK_CACHE_ROOT=str(self.root / "cache"))
        self.protected = self.workspace / "target/desktop-workspace-e2e/NormaE2E.app/sentinel"
        self.protected.parent.mkdir(parents=True)
        self.protected.write_bytes(b"e2e-permission-bundle")
        self.sentinels = [self.protected]
        for relative in ("target/debug/norma-os", "target/desktop-e2e-spm/sentinel",
                "artifacts/desktop-workspace-e2e/owned.json", "artifacts/desktop-workspace-e2e/report.json"):
            path = self.workspace / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"protected-e2e-evidence")
            self.sentinels.append(path)

    def invoke(self, *arguments):
        return subprocess.run([sys.executable, str(SCRIPT), "--workspace", str(self.workspace),
            *arguments], env=self.environment, capture_output=True, text=True, timeout=30)

    def build_task(self):
        result = self.invoke("run", "demo", "--", "build", "--offline")
        self.assertEqual(result.returncode, 0, f"夹具构建必须成功：{result.stderr}")
        targets = list((self.root / "cache").glob("*/demo/target"))
        self.assertEqual(len(targets), 1, "任务必须登记唯一的专属 target")
        return targets[0]

    # 场景：临时任务构建不能覆盖桌面 E2E 的固定 target。
    # 构造含空格路径的小型 Cargo 项目和 E2E bundle 哨兵，经公开 CLI 真实构建；
    # 预期外置目录生成可运行程序，仓库 target 的授权 bundle 保持原样。
    def test_run_builds_in_owned_external_target_and_preserves_e2e(self):
        result = self.invoke("run", "demo", "--", "build", "--offline")
        self.assertEqual(result.returncode, 0, f"任务构建必须成功：{result.stderr}")
        binaries = list((self.root / "cache").glob("*/demo/target/debug/cache-fixture"))
        self.assertEqual(len(binaries), 1, "应只有该任务的外置二进制")
        output = subprocess.check_output([str(binaries[0])], text=True)
        self.assertEqual(output.strip(), "fixture", "必须运行任务本次编译的程序")
        self.assertFalse((self.workspace / "target/debug/cache-fixture").exists(), "不得写入 E2E 的默认构建目录")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "E2E bundle 必须保留")

    # 场景：构建完成后还需要验收，因此 finish 默认只能预览。
    # 真实构建后快照 Cargo 元数据与缓存，先预览再显式 apply；预期只有 apply 回收外置 target，
    # 任务登记和锁仍在，E2E 程序、SwiftPM 缓存、授权 bundle 与证据逐字节不变。
    def test_finish_previews_then_cleans_only_owned_target(self):
        target = self.build_task()
        record = target.parent / "task.json"
        original_record = record.read_bytes()
        evidence = {path: path.read_bytes() for path in self.sentinels}
        preview = self.invoke("finish", "demo")
        self.assertEqual(preview.returncode, 0, f"预览必须成功：{preview.stderr}")
        self.assertTrue((target / "debug/cache-fixture").exists(), "默认预览必须保留待验收程序")
        self.assertEqual(record.read_bytes(), original_record, "预览不能改变任务登记状态")
        applied = self.invoke("finish", "demo", "--apply")
        self.assertEqual(applied.returncode, 0, f"显式回收必须成功：{applied.stderr}")
        self.assertFalse(target.exists(), "应由 cargo clean 回收该任务的 target")
        self.assertTrue(record.exists(), "任务归属记录必须留在清理范围外")
        self.assertTrue((target.parent / "task.lock").exists(), "锁文件不能删除重建导致 inode 竞态")
        for path, content in evidence.items():
            self.assertEqual(path.read_bytes(), content, f"E2E 文件必须保持不变：{path}")

    # 场景：调用者参数可能把输出重定向到默认 target，破坏隔离。
    # 构造 target/build-dir、manifest 和 config 的两种参数形式，逐个调用公开 CLI；
    # 预期在启动 Cargo 前拒绝，E2E 哨兵不变，不产生可被误回收的任务缓存。
    def test_run_rejects_output_and_workspace_overrides(self):
        for flag, value in (("--target-dir", str(self.workspace / "target")),
                ("--build-dir", str(self.workspace / "target")),
                ("--manifest-path", str(self.workspace / "Cargo.toml")),
                ("--config", 'build.target-dir="target"')):
            for options in ([flag, value], [f"{flag}={value}"]):
                with self.subTest(options=options):
                    result = self.invoke("run", "escape", "--", "check", *options)
                    self.assertNotEqual(result.returncode, 0, "必须拒绝覆盖隔离边界的参数")
                    self.assertIn("不允许", result.stderr, "应明确说明是隔离参数被拒绝")
        self.assertFalse(list((self.root / "cache").glob("*/escape/target")), "拒绝请求不能启动构建")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "拒绝参数不能改动 E2E")

    # 场景：有人绕过 wrapper，在任务目录直接启动 Cargo。
    # 真实构建后持有该 profile 的 Cargo 锁，再请求回收；预期快速拒绝 busy，程序与 E2E 均保留。
    def test_finish_refuses_active_cargo_profile_lock(self):
        target = self.build_task()
        with (target / "debug/.cargo-lock").open("r+") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = self.invoke("finish", "demo", "--apply")
        self.assertNotEqual(result.returncode, 0, "不能删除 Cargo 正在使用的 target")
        self.assertTrue((target / "debug/cache-fixture").exists(), "忙碌构建的程序必须保留")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "忙碌检查不能影响 E2E")

    # 场景：调用环境和仓库 Cargo 配置都把缓存指向受保护目录。
    # 构造错误的 target/build-dir 与开启增量编译，真实构建并回收；预期两个输出位置均被隔离，
    # 默认 E2E 程序不变，不在继承目录生成任何中间产物。
    def test_inherited_output_settings_cannot_escape_task(self):
        wrong = self.workspace / "target/inherited"
        self.environment.update(CARGO_TARGET_DIR=str(wrong), CARGO_BUILD_BUILD_DIR=str(wrong),
            CARGO_INCREMENTAL="1")
        config = self.workspace / ".cargo/config.toml"
        config.parent.mkdir()
        config.write_text('[build]\ntarget-dir="target/configured"\nbuild-dir="target/configured"\n')
        target = self.build_task()
        self.assertFalse(wrong.exists(), "继承的外部 build-dir/target-dir 必须被覆盖")
        self.assertFalse((self.workspace / "target/configured").exists(), "Cargo 配置不能覆盖隔离输出")
        self.assertFalse(any((target / "debug/incremental").glob("*")), "短期任务必须关闭增量缓存")
        result = self.invoke("finish", "demo", "--apply")
        self.assertEqual(result.returncode, 0, f"清理也必须覆盖继承的 build-dir：{result.stderr}")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "环境变量不能让清理碰到 E2E")

    # 场景：Cargo test 的 -- 后面属于测试程序参数。
    # 构造一个有名测试，用 --exact 执行；预期任务 target 参数在分隔符前，实际运行一条测试。
    def test_test_arguments_keep_their_separator(self):
        with (self.workspace / "src/main.rs").open("a") as source:
            source.write('#[test] fn expected_test() { assert_eq!(2 + 2, 4, "fixture arithmetic"); }\n')
        result = self.invoke("run", "demo", "--", "test", "--offline", "--", "--exact", "expected_test")
        self.assertEqual(result.returncode, 0, f"测试参数必须正确透传：{result.stderr}")
        self.assertIn("running 1 test", result.stdout, "必须执行指定测试，不能把参数交给 Cargo")

    # 场景：尚未登记的目录和路径遍历不能触发删除。
    # 使用未知 ID、.. 与带斜杠的 ID 请求回收；预期全部拒绝，不创建缓存根目录，E2E 保留。
    def test_unknown_task_and_traversal_are_rejected_without_writes(self):
        for task in ("unknown", "..", "../target", "/tmp", "name/child"):
            with self.subTest(task=task):
                result = self.invoke("finish", task, "--apply")
                self.assertNotEqual(result.returncode, 0, "未知任务或路径遍历必须被拒绝")
        self.assertFalse((self.root / "cache").exists(), "拒绝回收不能创建任务元数据")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "未知任务不能影响 E2E")

    # 场景：归属记录被修改，或 target 被换成指向 E2E 的符号链接。
    # 构建后依次篡改 workspace、损坏 JSON、删除登记、替换 target；预期 run/finish 均拒绝。
    def test_owner_record_and_target_symlink_fail_closed(self):
        target = self.build_task()
        record = target.parent / "task.json"
        owner = record.read_text()
        for content in (json.dumps({"workspace": "/other", "task": "demo"}), "{broken"):
            record.write_text(content)
            self.assertNotEqual(self.invoke("finish", "demo", "--apply").returncode, 0, "无效归属不能回收")
            self.assertNotEqual(self.invoke("run", "demo", "--", "check").returncode, 0, "无效归属不能重用")
        record.unlink()
        self.assertNotEqual(self.invoke("run", "demo", "--", "check").returncode, 0, "不能接管无登记的已有 target")
        record.write_text(owner)
        target.rename(target.parent / "saved-target")
        target.symlink_to(self.workspace / "target", target_is_directory=True)
        self.assertNotEqual(self.invoke("finish", "demo", "--apply").returncode, 0, "不能回收符号链接 target")
        self.assertNotEqual(self.invoke("run", "demo", "--", "check").returncode, 0, "不能构建到符号链接 target")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "归属异常时 E2E 必须保留")

    # 场景：target 自身正常，但 profile 被符号链接重定向到 E2E 构建目录。
    # 将已构建的 debug 暂存，再链接到默认 target/debug；预期在 Cargo 执行前拒绝。
    def test_redirected_profile_is_rejected_before_build(self):
        target = self.build_task()
        (target / "debug").rename(target.parent / "saved-debug")
        (target / "debug").symlink_to(self.workspace / "target/debug", target_is_directory=True)
        result = self.invoke("run", "demo", "--", "build", "--offline")
        self.assertNotEqual(result.returncode, 0, "profile 符号链接不能绕过隔离边界")
        self.assertFalse((self.workspace / "target/debug/cache-fixture").exists(), "不能写入 E2E 构建目录")

    # 场景：显式 target triple 会让 deps/build 等结构比默认 profile 多一层。
    # 登记任务后把 <triple>/debug/deps 链到 E2E 目录，真实调用 check --target <host>；
    # 预期在 Cargo 启动前拒绝，不向默认 target 写入 rmeta 或 dep-info。
    def test_target_triple_cannot_redirect_deps_to_e2e(self):
        target = self.build_task()
        version = subprocess.check_output(["rustc", "-vV"], text=True)
        host = next(line.split(": ", 1)[1] for line in version.splitlines() if line.startswith("host: "))
        profile = target / host / "debug"
        profile.mkdir(parents=True)
        protected_deps = self.workspace / "target/debug/deps"
        protected_deps.mkdir()
        (profile / "deps").symlink_to(protected_deps, target_is_directory=True)
        result = self.invoke("run", "demo", "--", "check", "--offline", "--target", host)
        self.assertNotEqual(result.returncode, 0, "显式 target 的第三层目录也必须拒绝符号链接")
        self.assertEqual(list(protected_deps.iterdir()), [], "不能把 rmeta 或 dep-info 写入 E2E 目录")

    # 场景：正常 build.rs 只写 OUT_DIR，但其 out 结构被链接到 E2E 证据目录。
    # 先真实构建，再替换 build/<hash>/out 并修改 build.rs 触发重跑；预期 CLI 拒绝重定向，
    # 保留原有证据而非让正常生成逻辑覆盖它。
    def test_build_script_out_cannot_redirect_to_e2e_evidence(self):
        build_script = self.workspace / "build.rs"
        build_script.write_text('fn main() { let out = std::env::var("OUT_DIR").unwrap(); '
            'std::fs::write(std::path::Path::new(&out).join("generated.txt"), "generated").unwrap(); }\n')
        target = self.build_task()
        outputs = list((target / "debug/build").glob("*/out"))
        self.assertEqual(len(outputs), 1, "应定位本夹具唯一的 build-script 输出目录")
        out = outputs[0]
        out.rename(target.parent / "saved-out")
        evidence = self.workspace / "artifacts/desktop-workspace-e2e/generated.txt"
        evidence.write_text("original evidence")
        out.symlink_to(evidence.parent, target_is_directory=True)
        with build_script.open("a") as source:
            source.write("// trigger rebuild\n")
        result = self.invoke("run", "demo", "--", "build", "--offline")
        self.assertNotEqual(result.returncode, 0, "build-script 输出目录也必须拒绝符号链接")
        self.assertEqual(evidence.read_text(), "original evidence", "正常 OUT_DIR 写入不能覆盖 E2E 证据")

    # 场景：目录都正常，但 rustc 要重写的 dep-info 文件是指向 E2E 报告的链接。
    # 真实构建后替换 deps/*.d 并修改源码触发 check；预期拒绝 Cargo 元数据文件链接，报告不变。
    def test_dep_info_file_cannot_redirect_to_e2e_report(self):
        target = self.build_task()
        previous = set((target / "debug/deps").glob("*.d"))
        checked = self.invoke("run", "demo", "--", "check", "--offline")
        self.assertEqual(checked.returncode, 0, f"必须先生成 check 对应的 dep-info：{checked.stderr}")
        created = set((target / "debug/deps").glob("*.d")) - previous
        self.assertEqual(len(created), 1, "build/check 的不同编译单元应产生一份新的 dep-info")
        dep_info = created.pop()
        report = self.workspace / "artifacts/desktop-workspace-e2e/report.json"
        original = report.read_bytes()
        dep_info.unlink()
        dep_info.symlink_to(report)
        with (self.workspace / "src/main.rs").open("a") as source:
            source.write("// trigger dep-info update\n")
        result = self.invoke("run", "demo", "--", "check", "--offline")
        self.assertNotEqual(result.returncode, 0, "Cargo 元数据文件不能链接到 E2E 证据")
        self.assertEqual(report.read_bytes(), original, "不能把报告覆盖成 rustc dep-info")

    # 场景：native 库合法地在本任务内使用文件链接。
    # 在已登记 target 内创建一个 dylib 实体和相对链接，再运行 check；预期可以复用内部库链接，
    # 换成指向工作树证据的外部链接后必须拒绝，证据保持不变。
    def test_only_private_native_library_file_links_are_allowed(self):
        target = self.build_task()
        native = target / "debug/native"
        native.mkdir()
        (native / "libfixture.1.dylib").write_bytes(b"native fixture")
        link = native / "libfixture.dylib"
        link.symlink_to("libfixture.1.dylib")
        allowed = self.invoke("run", "demo", "--", "check", "--offline")
        self.assertEqual(allowed.returncode, 0, f"本任务内 native 文件链接应可保留：{allowed.stderr}")
        link.unlink()
        link.symlink_to(self.protected)
        refused = self.invoke("run", "demo", "--", "check", "--offline")
        self.assertNotEqual(refused.returncode, 0, "native 文件链接也不能越过任务边界")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "外链拒绝时 E2E 必须保留")

    # 场景：构建失败后需要保留现场。
    # 先成功构建，再写入无效 Rust 并重试；预期返回失败，已构建程序、登记与 E2E 都保留。
    def test_failed_build_preserves_cache_for_diagnosis(self):
        target = self.build_task()
        (self.workspace / "src/main.rs").write_text("not valid Rust\n")
        result = self.invoke("run", "demo", "--", "build", "--offline")
        self.assertNotEqual(result.returncode, 0, "编译错误必须透传失败")
        self.assertTrue((target / "debug/cache-fixture").exists(), "失败不能自动删除诊断现场")
        self.assertTrue((target.parent / "task.json").exists(), "失败任务仍需保留归属")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "失败构建不能触碰 E2E")

    # 场景：Cargo 已退出，但程序或其他验收进程仍打开缓存文件。
    # 构建后持有程序文件，申请回收；预期 lsof 拒绝删除，关闭文件后可以正常回收。
    def test_open_artifact_prevents_cleanup(self):
        target = self.build_task()
        with (target / "debug/cache-fixture").open("rb"):
            result = self.invoke("finish", "demo", "--apply")
            self.assertNotEqual(result.returncode, 0, "正在使用的验收产物不能被回收")
            self.assertTrue(target.exists(), "验收未结束必须保留 target")
        self.assertEqual(self.invoke("finish", "demo", "--apply").returncode, 0, "文件释放后应允许回收")

    # 场景：Cargo clean 返回失败，回收必须可重试而非假报完成。
    # 使用真实构建和返回 7 的假清理命令，随后恢复真实 Cargo；预期保留暂存目录、阻止新构建，
    # 重试只清理本任务，E2E 不受影响。
    def test_interrupted_cleanup_can_be_retried(self):
        target = self.build_task()
        tool = self.root / "tools/cargo"
        tool.parent.mkdir()
        tool.write_text("#!/bin/sh\nexit 7\n")
        tool.chmod(0o755)
        original_path = self.environment.get("PATH", "")
        self.environment["PATH"] = str(tool.parent) + os.pathsep + original_path
        result = self.invoke("finish", "demo", "--apply")
        self.assertEqual(result.returncode, 7, "必须透传失败的清理状态")
        self.assertTrue((target.parent / "cleaning").exists(), "失败回收应保留可重试目录")
        self.assertNotEqual(self.invoke("run", "demo", "--", "check").returncode, 0, "回收未完成不得新建同任务输出")
        self.environment["PATH"] = original_path
        self.assertEqual(self.invoke("finish", "demo", "--apply").returncode, 0, "应能重试原任务回收")
        self.assertFalse((target.parent / "cleaning").exists(), "重试成功后不应残留构建缓存")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "重试不能修改 E2E bundle")

    # 场景：完成空闲检查后、cargo clean 真正执行前，直接 Cargo 仍可能尝试同一目录。
    # 用代理在 clean 入口设置可控 barrier，在暂存目录尝试获取真实 profile 锁；
    # 预期删除期间锁仍忙，放行后清理成功，不能仅在检查时持锁。
    def test_profile_lock_is_held_until_clean_finishes(self):
        target = self.build_task()
        ready = self.root / "clean-ready"
        release = self.root / "clean-release"
        tool = self.root / "tools/cargo"
        tool.parent.mkdir()
        real_cargo = shutil.which("cargo")
        tool.write_text('#!/usr/bin/env python3\nimport os, sys, time\nfrom pathlib import Path\n'
            f'Path({str(ready)!r}).write_text("ready")\n'
            f'while not Path({str(release)!r}).exists(): time.sleep(0.01)\n'
            f'os.execv({real_cargo!r}, [{real_cargo!r}, *sys.argv[1:]])\n')
        tool.chmod(0o755)
        environment = dict(self.environment, PATH=str(tool.parent) + os.pathsep + self.environment.get("PATH", ""))
        process = subprocess.Popen([sys.executable, str(SCRIPT), "--workspace", str(self.workspace),
            "finish", "demo", "--apply"], env=environment, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 10
            while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(ready.exists(), "清理必须已进入 barrier，不能提前断言锁状态")
            with (target.parent / "cleaning/debug/.cargo-lock").open("r+") as lock:
                locked = False
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    locked = True
                self.assertTrue(locked, "profile 锁必须一直持有到 cargo clean 完成")
        finally:
            release.write_text("continue")
            stdout, stderr = process.communicate(timeout=20)
        self.assertEqual(process.returncode, 0, f"放行后真实 cargo clean 应成功：{stdout}\n{stderr}")
        self.assertFalse((target.parent / "cleaning").exists(), "真实清理完成后应移除暂存缓存")

    def slow_cargo(self, stubborn_child=False):
        tool = self.root / "tools/cargo"
        tool.parent.mkdir()
        ready = self.root / "cargo-ready"
        script = '#!/usr/bin/env python3\nimport os, signal, time, subprocess, sys\nfrom pathlib import Path\n'
        script += 'signal.signal(signal.SIGTERM, lambda *_: exit(0))\n'
        if stubborn_child:
            child_code = ('import os, signal, time\nfrom pathlib import Path\n'
                'signal.signal(signal.SIGTERM, signal.SIG_IGN)\n'
                'Path(os.environ["NORMA_CACHE_TEST_CHILD"]).write_text(str(os.getpid()))\n'
                'while True: time.sleep(0.1)\n')
            script += f'subprocess.Popen([sys.executable, "-c", {child_code!r}])\n'
            script += 'while not Path(os.environ["NORMA_CACHE_TEST_CHILD"]).exists(): time.sleep(0.01)\n'
        tool.write_text(script +
            'Path(os.environ["NORMA_CACHE_TEST_READY"]).write_text(str(os.getpid()))\n'
            'while True: time.sleep(0.1)\n')
        tool.chmod(0o755)
        environment = dict(self.environment, PATH=str(tool.parent) + os.pathsep + self.environment.get("PATH", ""),
            NORMA_CACHE_TEST_READY=str(ready), NORMA_CACHE_TEST_CHILD=str(self.root / "stubborn-pid"))
        process = subprocess.Popen([sys.executable, str(SCRIPT), "--workspace", str(self.workspace),
            "run", "demo", "--", "check"], env=environment, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True)
        # 10 秒为进程启动与调度预算；等待 ready 文件避免依赖固定 sleep 的假阳性。
        deadline = time.monotonic() + 10
        while not ready.exists() and process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertTrue(ready.exists(), "假 Cargo 必须已启动，才能验证并发和信号")
        return process, int(ready.read_text())

    # 场景：测试程序留下忽略 TERM 的孙进程，Cargo 自己先退出。
    # 用假 Cargo 在同一进程组启动该进程，向 wrapper 发送 TERM；预期 wrapper 完成后整个组已停止，
    # 不允许只等 Cargo 主进程而留下继续写缓存的测试进程。
    def test_term_stops_descendant_that_ignores_term(self):
        process, cargo_pid = self.slow_cargo(stubborn_child=True)
        try:
            process.terminate()
            process.wait(timeout=10)
            child_pid = int((self.root / "stubborn-pid").read_text())
            # 2 秒留给系统回收孤儿进程；ready 已证明忽略 TERM 的 handler 安装完毕。
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                try:
                    os.kill(child_pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.02)
            with self.assertRaises(ProcessLookupError, msg="退出前必须停止忽略 TERM 的孙进程"):
                os.kill(child_pid, 0)
        finally:
            if process.poll() is None:
                process.kill()
            try:
                os.killpg(cargo_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.communicate(timeout=10)

    # 场景：任务仍在运行时不能回收，TERM 必须停止 Cargo 后才释放租约。
    # 启动会持续等待的外部 Cargo 夹具，先尝试 finish，再向 wrapper 发 TERM；
    # 预期忙碌回收被拒绝，wrapper 返回 128+15=143，Cargo 已退出，任务记录与 E2E 保留。
    def test_busy_task_and_term_keep_lease_until_child_exits(self):
        process, cargo_pid = self.slow_cargo()
        try:
            busy = self.invoke("finish", "demo", "--apply")
            self.assertNotEqual(busy.returncode, 0, "运行中的任务必须拒绝回收")
            self.assertIn("仍在运行", busy.stderr, "应明确提示任务租约仍被持有")
            process.terminate()
            process.communicate(timeout=10)
            self.assertEqual(process.returncode, 143, "TERM 应透传为 128+15，而不是成功")
            with self.assertRaises(ProcessLookupError, msg="wrapper 退出前必须停止 Cargo"):
                os.kill(cargo_pid, 0)
            self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "中断不能影响 E2E")
        finally:
            if process.poll() is None:
                process.kill()
            try:
                os.killpg(cargo_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.communicate(timeout=10)

    # 场景：wrapper 被不可捕获的 SIGKILL 终止，Cargo 仍可能继续工作。
    # 让 Cargo 继承任务租约描述符，杀 wrapper 后请求 finish；预期租约仍忙，不误回收活跃任务。
    def test_killed_wrapper_does_not_release_live_cargo_lease(self):
        process, cargo_pid = self.slow_cargo()
        try:
            process.kill()
            process.wait(timeout=10)
            result = self.invoke("finish", "demo", "--apply")
            self.assertNotEqual(result.returncode, 0, "Cargo 存活时继承的租约必须阻止回收")
            self.assertIn("仍在运行", result.stderr, "不能把 wrapper 退出等同于任务完成")
        finally:
            try:
                os.killpg(cargo_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.communicate(timeout=10)

    # 场景：两个工作树使用同一个任务 ID。
    # 为第二个路径构造独立 Cargo 项目并依次构建，只回收第一个；预期命名空间隔离，另一份缓存不变。
    def test_same_task_id_is_isolated_between_worktrees(self):
        first = self.build_task()
        second_workspace = self.root / "second-worktree"
        second_workspace.mkdir()
        (second_workspace / "src").mkdir()
        (second_workspace / "Cargo.toml").write_bytes((self.workspace / "Cargo.toml").read_bytes())
        (second_workspace / "src/main.rs").write_bytes((self.workspace / "src/main.rs").read_bytes())
        result = subprocess.run([sys.executable, str(SCRIPT), "--workspace", str(second_workspace),
            "run", "demo", "--", "build", "--offline"], env=self.environment,
            capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, f"第二工作树必须能独立构建：{result.stderr}")
        targets = list((self.root / "cache").glob("*/demo/target"))
        self.assertEqual(len(targets), 2, "同 ID 的两个工作树必须有两份缓存")
        second = next(path for path in targets if path != first)
        self.assertEqual(self.invoke("finish", "demo", "--apply").returncode, 0, "第一个工作树回收应成功")
        self.assertFalse(first.exists(), "第一个工作树缓存必须回收")
        self.assertTrue((second / "debug/cache-fixture").exists(), "另一工作树的缓存必须保留")

    # 场景：回收外置任务缓存前后，真实 desktop-e2e.sh 入口都必须仍使用固定 App 和驱动路径。
    # 复制现有 shell 入口，用 Swift/codesign/driver 夹具代替 GUI；先运行一次，再回收任务，再运行。
    # 预期两次均成功，driver 收到相同的默认二进制与 artifacts 路径；不宣称真实 GUI 验收。
    def test_desktop_e2e_entrypoint_survives_task_cleanup(self):
        self.build_task()
        scripts = self.workspace / "scripts"
        scripts.mkdir()
        entrypoint = scripts / "desktop-e2e.sh"
        shutil.copy2(SCRIPT.with_name("desktop-e2e.sh"), entrypoint)
        tools = self.root / "e2e-tools"
        tools.mkdir()
        swift = tools / "swift"
        driver_source = self.root / "fake-driver"
        driver_source.write_text('#!/usr/bin/env python3\nimport os, sys\nfrom pathlib import Path\n'
            'if sys.argv[1] == "list": print("workspace-lifecycle")\n'
            'else:\n'
            '    root = Path(os.environ["NORMA_CACHE_TEST_WORKSPACE"])\n'
            '    assert sys.argv[3] == str(root / "target/debug/norma-os"), "wrong E2E app"\n'
            '    assert sys.argv[4] == str(root / "artifacts/desktop-workspace-e2e"), "wrong evidence root"\n'
            '    assert Path(sys.argv[3]).read_bytes() == b"protected-e2e-evidence", "changed E2E app"\n'
            '    print("fixture E2E contract passed")\n')
        driver_source.chmod(0o755)
        swift.write_text('#!/usr/bin/env python3\nimport os, shutil, sys\nfrom pathlib import Path\n'
            'out = Path(sys.argv[sys.argv.index("--build-path") + 1]) / "debug/norma-e2e"\n'
            'out.parent.mkdir(parents=True, exist_ok=True)\n'
            'shutil.copy2(os.environ["NORMA_CACHE_TEST_DRIVER"], out)\n')
        swift.chmod(0o755)
        codesign = tools / "codesign"
        codesign.write_text("#!/bin/sh\nexit 0\n")
        codesign.chmod(0o755)
        (self.workspace / "target/debug/norma-os").chmod(0o755)
        environment = dict(self.environment, PATH=str(tools) + os.pathsep + self.environment.get("PATH", ""),
            NORMA_CACHE_TEST_WORKSPACE=str(self.workspace), NORMA_CACHE_TEST_DRIVER=str(driver_source))
        for iteration in range(2):
            result = subprocess.run(["bash", str(entrypoint), "workspace"], env=environment,
                capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, f"第 {iteration + 1} 次 E2E 入口契约应通过：{result.stderr}")
            self.assertIn("通过率 1/1", result.stdout, "必须实际进入 driver，而非仅检查脚本静态内容")
            if iteration == 0:
                clean = self.invoke("finish", "demo", "--apply")
                self.assertEqual(clean.returncode, 0, f"E2E 两次运行之间回收外置缓存应成功：{clean.stderr}")
        self.assertEqual(self.protected.read_bytes(), b"e2e-permission-bundle", "固定授权 bundle 哨兵仍应存在")


if __name__ == "__main__":
    unittest.main()
