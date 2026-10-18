#!/usr/bin/env python3
"""用独立目录运行临时 Cargo 任务；显式 finish 后才回收缓存。"""
import argparse
from contextlib import contextmanager, ExitStack
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys


def task_directory(workspace, task_id):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,79}", task_id):
        raise ValueError("任务 ID 必须以字母或数字开头，只包含字母、数字、点、下划线和短横线")
    cache = Path(os.environ.get("NORMA_TASK_CACHE_ROOT", Path.home() / ".cache/norma-build")).expanduser().resolve()
    if cache.is_relative_to(workspace) or workspace.is_relative_to(cache):
        raise ValueError("任务缓存根目录必须位于工作树之外，不能覆盖仓库 target 或 E2E 目录")
    namespace = hashlib.sha256(os.fsencode(workspace)).hexdigest()[:16]
    return cache / namespace / task_id


@contextmanager
def owned_task(workspace, directory, create=False):
    if directory.resolve() != directory:
        raise ValueError("任务目录不能通过符号链接指向其他位置")
    if create:
        directory.mkdir(parents=True, exist_ok=True)
    if not directory.is_dir():
        raise ValueError("任务未登记，不能回收未知目录")
    record = directory / "task.json"
    target = directory / "target"
    lock_path = directory / "task.lock"
    if record.is_symlink() or target.is_symlink() or lock_path.is_symlink():
        raise ValueError("任务登记、锁和 target 都不能是符号链接")
    if not create and not lock_path.is_file():
        raise ValueError("任务缺少登记锁，拒绝回收")
    with lock_path.open("a" if create else "r+") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError("任务仍在运行或回收，稍后重试") from None
        owner = {"workspace": str(workspace), "task": directory.name}
        if record.exists():
            if json.loads(record.read_text()) != owner:
                raise ValueError("任务缓存归属与当前工作树不一致")
        elif create and not target.exists():
            record.write_text(json.dumps(owner, ensure_ascii=False, indent=2) + "\n")
        else:
            raise ValueError("任务未登记，拒绝接管已有 target")
        yield target, lock.fileno()


def cargo_environment(target):
    return dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_BUILD_DIR=str(target),
        CARGO_INCREMENTAL="0")


def validate_output_layout(target):
    if not target.exists():
        return
    # 同时覆盖 triple/profile 和 OUT_DIR，只有本任务内的 native 库允许文件链接。
    def failed(error):
        raise error

    for parent, directories, files in os.walk(target, followlinks=False, onerror=failed):
        for name in directories:
            child = Path(parent) / name
            if child.is_symlink():
                raise ValueError(f"Cargo 输出目录不能是符号链接：{child}")
        for name in files:
            child = Path(parent) / name
            if not child.is_symlink():
                continue
            try:
                private = child.resolve().is_relative_to(target.parent)
            except RuntimeError:
                private = False
            if not private or not re.fullmatch(r".+\.(?:a|dylib|so(?:\.\d+)*)", name):
                raise ValueError(f"仅允许本任务内的 native 库文件链接：{child}")


def cargo_process(command, workspace, target, lease):
    child = subprocess.Popen(command, cwd=workspace, env=cargo_environment(target),
        start_new_session=True, pass_fds=(lease,))

    def interrupted(signum, _frame):
        raise KeyboardInterrupt(signum)

    previous = {number: signal.signal(number, interrupted) for number in (signal.SIGINT, signal.SIGTERM)}
    try:
        return child.wait()
    except KeyboardInterrupt as error:
        # 同时停止 Cargo、rustc 和测试进程；退出前保留任务锁并等待它们停止。
        for number in previous:
            signal.signal(number, signal.SIG_IGN)
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
        # Cargo 可能先退出，但同组的测试孙进程仍忽略 TERM。
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait()
        return 128 + (error.args[0] if error.args else signal.SIGINT)
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)


def run_task(workspace, directory, arguments):
    if arguments[:1] == ["--"]:
        arguments = arguments[1:]
    if not arguments or arguments[0] not in ("build", "check", "test", "clippy"):
        raise ValueError("run 仅支持 Cargo build/check/test/clippy；桌面 E2E 使用原入口")
    cargo_options = arguments[:arguments.index("--")] if "--" in arguments else arguments
    forbidden = ("--target-dir", "--build-dir", "--manifest-path", "--config", "-Z", "-C")
    if any(option == flag or option.startswith(flag + "=") or
            (flag in ("-Z", "-C") and option.startswith(flag))
            for option in cargo_options for flag in forbidden):
        raise ValueError("不允许覆盖 target/build-dir、manifest、Cargo 配置或不稳定参数")
    with owned_task(workspace, directory, create=True) as (target, lease):
        if (directory / "cleaning").exists() or (directory / "cleaning").is_symlink():
            raise ValueError("上次回收尚未结束，请先重试 finish")
        validate_output_layout(target)
        print(f"任务缓存：{target}", flush=True)
        return cargo_process(["cargo", arguments[0], "--target-dir", str(target), *arguments[1:]],
            workspace, target, lease)


@contextmanager
def quiet_target(target):
    with ExitStack() as locks:
        paths = set(target.glob("*/.cargo-lock")) | set(target.glob("*/*/.cargo-lock"))
        for path in sorted(paths):
            if path.is_symlink():
                raise ValueError("Cargo 锁不能是符号链接")
            lock = locks.enter_context(path.open("r+"))
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise ValueError(f"Cargo 仍在使用该缓存：{path}") from None
        # Cargo 锁不覆盖已经启动的程序；同时拒绝仍有进程打开缓存文件的目录。
        result = subprocess.run(["lsof", "-nP", "-a", "-p", f"^{os.getpid()}",
            "+D", str(target), "-F", "pn"], capture_output=True, text=True, timeout=20)
        if result.stdout or result.stderr or result.returncode not in (0, 1):
            raise ValueError("缓存仍被进程使用或无法确认空闲；未清理：" + result.stdout + result.stderr)
        yield


def finish_task(workspace, directory, apply):
    with owned_task(workspace, directory) as (target, lease):
        cleaning = directory / "cleaning"
        if cleaning.is_symlink() or (cleaning.exists() and target.exists()):
            raise ValueError("回收暂存目录异常，拒绝清理")
        if cleaning.exists():
            target = cleaning
        if not target.exists():
            print("该任务已经没有构建缓存")
            return 0
        validate_output_layout(target)
        with quiet_target(target):
            if apply and target != cleaning:
                # 私有改名后再调用 Cargo，避免新构建与删除同一个路径发生竞态。
                target.rename(cleaning)
                target = cleaning
            command = ["cargo", "clean", "--manifest-path", str(workspace / "Cargo.toml"),
                "--target-dir", str(target)]
            if not apply:
                command.append("--dry-run")
            print(f"{'回收' if apply else '仅预览'}任务缓存：{target}", flush=True)
            return cargo_process(command, workspace, target, lease)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, default=Path(__file__).resolve().parents[1])
    commands = parser.add_subparsers(dest="command", required=True)
    run = commands.add_parser("run", help="在专属外置 target 中运行 Cargo，不自动清理")
    run.add_argument("task")
    run.add_argument("arguments", nargs=argparse.REMAINDER)
    finish = commands.add_parser("finish", help="任务验收结束后回收；默认仅预览")
    finish.add_argument("task")
    finish.add_argument("--apply", action="store_true", help="显式调用 cargo clean 删除该任务缓存")
    args = parser.parse_args()
    try:
        workspace = args.workspace.resolve(strict=True)
        if not (workspace / "Cargo.toml").is_file():
            raise ValueError("工作树根目录缺少 Cargo.toml")
        directory = task_directory(workspace, args.task)
        if args.command == "run":
            return run_task(workspace, directory, args.arguments)
        return finish_task(workspace, directory, args.apply)
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(f"任务缓存操作失败：{error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
