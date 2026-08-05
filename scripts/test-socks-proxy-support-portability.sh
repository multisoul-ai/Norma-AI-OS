#!/usr/bin/env bash
set -euo pipefail

# 场景：SOCKS 代理护栏在 GitHub macOS runner 未安装 ripgrep 时仍可执行。
#
# 数据构造：
#   - 创建只包含 Rust 工具链和 macOS 系统目录的隔离 PATH。
#   - 明确验证隔离 PATH 中不存在 rg，避免本机 Homebrew 环境掩盖 CI 问题。
#
# 执行过程：
#   1. 在隔离 PATH 中运行 check-socks-proxy-support.sh。
#   2. 护栏通过 cargo tree 检查构建期与运行时的代理 features。
#
# 预期结果：
#   - 正向：脚本退出码为 0，并输出明确的通过消息。
#   - 负向：输出中不应出现“rg: command not found”或误报 feature 缺失。

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

isolated_bin="$(mktemp -d)"
trap 'rm -rf "$isolated_bin"' EXIT

for tool_name in cargo rustc rustup; do
    tool_path="$(command -v "$tool_name")"
    ln -s "$tool_path" "$isolated_bin/$tool_name"
done

isolated_path="$isolated_bin:/usr/bin:/bin:/usr/sbin:/sbin"

if PATH="$isolated_path" command -v rg >/dev/null 2>&1; then
    echo "Test setup failed: rg must not exist in the isolated PATH" >&2
    exit 1
fi

set +e
output="$(PATH="$isolated_path" /bin/bash scripts/check-socks-proxy-support.sh 2>&1)"
status=$?
set -e

if [[ $status -ne 0 ]]; then
    echo "SOCKS proxy guard must pass without rg, but exited with status $status" >&2
    echo "$output" >&2
    exit 1
fi

if ! grep -Fq "Build-time and runtime SOCKS proxy support check passed." <<<"$output"; then
    echo "SOCKS proxy guard did not emit its success message" >&2
    echo "$output" >&2
    exit 1
fi

if grep -Fq "rg: command not found" <<<"$output"; then
    echo "SOCKS proxy guard still depends on rg" >&2
    exit 1
fi

if grep -Fq "must support SOCKS proxies" <<<"$output"; then
    echo "SOCKS proxy guard misreported a missing feature when rg was unavailable" >&2
    exit 1
fi

echo "SOCKS proxy portability regression test passed."
