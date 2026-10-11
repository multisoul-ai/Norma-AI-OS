#!/usr/bin/env python3
"""将实际媒体与历史无损压缩数据生成可离线查看的 HTML 表格。"""
import csv
import html
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "docs/research/assets/2026-10-10-package-size-implementation"


def optional_evidence(name):
    path = EVIDENCE / name
    return json.loads(path.read_text()) if path.exists() else {}


def size_label(row, key):
    value = row.get(key)
    return f"{value / 1e6:.3f} MB" if value is not None else "未测量"


def availability_label(evidence):
    if not evidence:
        return "GitHub 下载可用性未测量。"
    resources = evidence.get("resources", [])
    counts = {}
    for resource in resources:
        status = str(resource.get("status", "未知"))
        counts[status] = counts.get(status, 0) + 1
    details = "、".join(f"HTTP {status}：{count} 个" for status, count in sorted(counts.items()))
    return f'GitHub 地址检查：{len(resources)} 个，{details}；检查时间 {evidence.get("checked_at_utc", "未记录")}。HEAD 可用性检查不替代发布前内容校验。'


def table(headers, rows):
    cells = lambda row, tag: "".join(f"<{tag}>{html.escape(str(value))}</{tag}>" for value in row)
    return '<div class="scroll"><table><thead><tr>' + cells(headers, "th") + '</tr></thead><tbody>' + "".join(
        "<tr>" + cells(row, "td") + "</tr>" for row in rows) + "</tbody></table></div>"


def measure_label(row, key, scale=1, digits=3, suffix=""):
    value = row.get(key)
    return f"{value / scale:.{digits}f}{suffix}" if value is not None else "未测量"


def resource_distribution_tables():
    source = optional_evidence("source-media-distribution.json")
    installed = optional_evidence("installed-app-distribution.json")
    page = '<h2 id="source-resources">原资源分布</h2>'
    if source:
        page += '<p>原清单：' + html.escape(size_label(source, "total_bytes"))
        page += '；包括媒体及原有 README，不含新增预览/catalog、程序或 DMG。</p>'
        names = {"wallpapers": "壁纸", "feature-guide": "教程"}
        page += table(["用途", "格式", "文件数", "原大小", "占原清单"], [
            [names.get(row["scope"], row["scope"]), row["extension"].lstrip(".").upper(),
                row["files"], size_label(row, "bytes"), measure_label(row, "share_percent", digits=2, suffix="%")]
            for row in sorted(source.get("groups", []), key=lambda row: row["bytes"], reverse=True)])
    else:
        page += '<p>原资源分布未测量。</p>'
    page += '<h2 id="installed-sample">已安装 App 参考样本</h2>'
    if installed:
        page += '<p>版本 ' + html.escape(str(installed.get("version", "未记录")))
        page += '；普通文件合计 ' + html.escape(size_label(installed, "regular_file_bytes"))
        page += '。只读清点、排除 symlink 别名；构建来源未与新基线确认一致，不能代替本轮 App/DMG 基线或运行内存。</p>'
        names = {"main_binary": "主程序", "companion_binaries": "companion binaries",
            "frameworks": "框架", "wallpaper_resources": "壁纸资源", "tutorial_resources": "教程资源",
            "icons": "图标", "other_resources": "其他资源", "app_signature_metadata": "签名元数据",
            "other_bundle_metadata": "其他 bundle 元数据"}
        page += table(["内容", "文件数", "大小", "占该样本"], [
            [names.get(row["group"], row["group"]), row["files"], size_label(row, "bytes"),
                measure_label(row, "share_percent", digits=2, suffix="%")]
            for row in sorted(installed.get("groups", []), key=lambda row: row["bytes"], reverse=True)])
    else:
        page += '<p>已安装参考样本未测量。</p>'
    return page


def local_install_probe_table():
    probe = optional_evidence("media-install-probe.json")
    page = '<h2 id="install-probe">局部文件安装探针</h2>'
    page += '<p>仅覆盖 debug 安装域库的本地文件流；HTTP/GPUI/生产 App 性能未测量。源文件缓存为热缓存，每次使用隔离的空安装目录。峰值 RSS 是独立探针进程，不是整个 App。</p>'
    if not probe:
        return page + '<p>局部安装实验未测量。</p>'
    rows = []
    for mode, label in (("install", "首次安装"), ("repair", "损坏修复"), ("cancel", "中途取消")):
        row = probe.get("summary", {}).get(mode, {})
        elapsed = "median_repair_seconds" if mode == "repair" else "median_install_seconds"
        rows.append([label, size_label(row, "source_bytes"), measure_label(row, elapsed),
            measure_label(row, "median_verify_seconds") if mode != "cancel" else "不适用",
            measure_label(row, "peak_rss_bytes", scale=1048576, digits=2, suffix=" MiB"),
            measure_label(row, "max_read_buffer_bytes", scale=1024, digits=0, suffix=" KiB"),
            row.get("successful_runs", "未测量")])
    return page + table(["步骤", "原文件大小", "步骤中位秒", "离线 SHA 中位秒", "探针峰值 RSS", "最大读取缓冲", "预期结果通过次数"], rows)


def framework_candidate_table():
    framework = optional_evidence("sparkle-arm64-probe.json")
    page = '<h2 id="framework-probe">框架静态候选</h2>'
    page += '<p>独立副本中保留 arm64 的框架自身收益。未采用：真实 updater、最低 macOS 和新 App 验收仍待执行；正式配置继续使用原框架，这些字节没有计入 App/DMG 收益。</p>'
    if not framework:
        return page + '<p>框架切片实验未测量。</p>'
    return page + table(["候选", "原框架", "arm64 副本", "减少字节", "框架自身节省"], [
        ["Sparkle arm64", size_label(framework, "original_framework_bytes"),
            size_label(framework, "arm64_framework_bytes"), framework.get("saved_bytes", "未测量"),
            measure_label(framework, "saved_percent", digits=2, suffix="%")]])


def main():
    profiles = json.loads((EVIDENCE / "media-profile-comparison.json").read_text())
    baseline = optional_evidence("baseline.json")
    before = optional_evidence("candidate-before-gpui-isolation.json")
    candidate = optional_evidence("candidate-gpui-isolated.json")
    availability = optional_evidence("github-availability.json")
    matrix = optional_evidence("compiler-matrix.json")
    light = next((row for row in profiles if row["profile"] == "light"), None)
    headline = f'light 随包媒体净减少 {light["net_saved_percent"]:.2f}%。' if light else "轻量媒体收益未测量。"
    source = ROOT / "docs/research/assets/2026-10-09-resource-compression/summary.csv"
    with source.open() as stream:
        algorithms = [row for row in csv.DictReader(stream) if row["dataset"] == "resources"]
    algorithms.sort(key=lambda row: int(row["compressed_bytes"]) if row["compressed_bytes"] else float("inf"))
    algorithm_rows = []
    for index, row in enumerate(algorithms):
        if not row["compressed_bytes"]:
            algorithm_rows.append([row["algorithm"], "167.313", "未完成", "—", "—", "—", "—", "—", row["status"]])
            continue
        algorithm_rows.append([row["algorithm"], f'{int(row["source_bytes"])/1e6:.3f}',
            f'{int(row["compressed_bytes"])/1e6:.3f}', f'{float(row["saving_percent"]):.2f}%',
            f'{float(row["compress_seconds"]):.3f}', f'{float(row["decompress_seconds"]):.3f}',
            f'{int(row["compress_max_rss_bytes"])/1048576:.1f}', f'{int(row["decompress_max_rss_bytes"])/1048576:.1f}',
            "体积最优" if index == 0 else ("归档综合候选" if row["algorithm"] == "zstd-19" else "")])
    profile_rows = [[row["profile"], f'{row["original_bytes"]/1e6:.3f}',
        f'{row["originals_bundled_bytes"]/1e6:.3f}', f'{(row["preview_bytes"]+row["catalog_bytes"])/1e6:.3f}',
        f'{row["payload_bytes"]/1e6:.3f}', f'{row["net_saved_percent"]:.2f}%',
        "媒体最小；待上线/验收" if row["profile"] == "light" else ("离线完整；默认" if row["profile"] == "full" else "待上线/验收")]
        for row in profiles]
    page = '''<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Norma OS 包体积优化实施报告</title><style>
    :root{color-scheme:light dark}body{font:15px/1.65 -apple-system,BlinkMacSystemFont,sans-serif;margin:0;padding:32px;background:#f4f5f7;color:#202631}main{max-width:1240px;margin:auto}h1{font-size:28px;line-height:1.3}h2{margin-top:32px;font-size:20px}.note{padding:16px 20px;background:#fff2cf;border:1px solid #e8c972;border-radius:12px}.scroll{overflow:auto;border:1px solid #d7dce3;border-radius:10px;background:#fff}table{width:100%;border-collapse:collapse;white-space:nowrap}th,td{text-align:right;padding:12px 14px;border-bottom:1px solid #e4e7ed;font-variant-numeric:tabular-nums}th{background:#e9edf3}th:first-child,td:first-child,td:last-child{text-align:left}tbody tr:hover{background:#eff4ff}code{overflow-wrap:anywhere}a{color:#1b59c4}@media(prefers-color-scheme:dark){body{background:#171b21;color:#e8eaf0}.scroll{background:#222832;border-color:#414957}th{background:#303844}th,td{border-color:#414957}.note{background:#3a301b;border-color:#756035}tbody tr:hover{background:#303c52}a{color:#9cbfff}}@media(max-width:700px){body{padding:16px}}</style>
    <main><h1>Norma OS 包体积优化实施报告</h1><p>2026-10-10 · 独立 worktree：<code>codex/package-size-reduction</code> · 阶段报告，真实产物验收未完成</p><div class="note"><b>当前可确认：'''
    page += html.escape(headline) + '</b><br>仅统计媒体与新增预览/清单，尚不能作为整个 DMG 的收益。'
    page += html.escape(availability_label(availability))
    page += '默认 full 保留全部离线资源；新 App/DMG 数值见下表，真实 App 验收需独立证据。</div><h2>实际随包媒体分布</h2><p>MB 为十进制；所有原始 MP4/GIF 字节保持不变。预览是新增静态展示文件。</p>'
    page += table(["方案", "原始媒体 MB", "随包原媒体 MB", "预览＋清单 MB", "实际负载 MB", "净节省", "状态"], profile_rows)
    page += resource_distribution_tables()
    page += '<h2>无损压缩实验对照</h2><p>引用 2026-10-09 完整媒体实验；耗时为重复实验中位数，内存为最高峰值。这些是媒体归档体积，不是新 DMG；最优仅在表内有完整测量的算法间比较。</p>'
    page += table(["算法", "原始媒体 MB", "压缩后 MB", "节省空间", "压缩秒", "解压秒", "压缩峰值 MiB", "解压峰值 MiB", "选择"], algorithm_rows)
    page += '<h2>主程序与安装包基线</h2>' + table(["项目", "原基线", "优化候选", "说明"], [
        ["主程序：下载实现", size_label(baseline, "binary_bytes"), size_label(before, "binary_bytes"), before.get("build_status", "未测量")],
        ["主程序：再隔离 GPUI 测试依赖", size_label(baseline, "binary_bytes"), size_label(candidate, "binary_bytes"), candidate.get("build_status", "未测量")],
        ["App", size_label(baseline, "app_bytes"), size_label(candidate, "app_bytes"), "缺数值时不推算总包收益"],
        ["DMG", size_label(baseline, "dmg_bytes"), size_label(candidate, "dmg_bytes"), "缺数值时不推算镜像收益"]])
    page += '<h2 id="compiler-matrix">构建参数矩阵</h2><p>3/s/z × thin/fat；主程序体积最小并不代表最终发布最优。日志采用热缓存，耗时不能作为冷构建排名；真实 App 性能另验。</p>'
    if matrix:
        measured = [row for row in matrix if row.get("exit_code") == 0 and row.get("binary_bytes") is not None]
        smallest = min((row["binary_bytes"] for row in measured), default=None)
        page += table(["参数", "主程序 MB", "构建秒", "状态", "说明"], [
            [row["variant"], size_label(row, "binary_bytes"), f'{row["build_seconds"]:.1f}',
                "通过" if row.get("exit_code") == 0 else f'失败（{row.get("exit_code")}）',
                "已完成组中主程序最小；性能待验" if row.get("exit_code") == 0 and row.get("binary_bytes") == smallest else "性能待验"]
            for row in matrix])
    else:
        page += '<p>矩阵未测量。</p>'
    page += local_install_probe_table() + framework_candidate_table()
    page += '''<h2>已落地内容</h2><ul><li>三种资源 profile、固定可信清单、七份预览及 GitHub 上传材料。</li><li>后台原文件下载、长度/SHA 校验、原子安装、共享并发 2、取消和重试。</li><li>壁纸旧选择保留，教程封面与文字离线可用，迟到结果只刷新当前内容。</li><li>轻量正式打包先验证实际下载；full 为默认，尚未上传或发布。</li></ul><p>全部下载后会恢复原媒体的磁盘占用。本方案优化首次安装包，不能声称原媒体总占用减少 80%。</p>
    <h2>剩余验收</h2><p>构建参数的真实 App 性能对照；新 App/DMG 字节、哈希和签名检查；真实 App 下载、离线、取消、缓存修复、快速切换及浅深主题操作。</p>
    <p>后续差分更新通过 Sparkle 减少升级下载量，首次安装包仍单独比较；当前保留 <code>--maximum-deltas 0</code>。</p>
    <p>本地签名模式遵循 <a href="../../.agents/skills/release-dmg/SKILL.md">release-dmg/SKILL.md</a>：<q>Use ./scripts/release-local.sh --ad-hoc only when the user explicitly asks for an ad-hoc local test package</q>。</p>
    <p><a href="../exec-plans/2026-10-10-package-size-reduction.md">执行计划</a> · <a href="2026-10-09-lossless-resource-compression-report.md">完整无损压缩报告</a> · <a href="assets/2026-10-10-package-size-implementation/media-profile-comparison.csv">媒体表格 CSV</a> · <a href="2026-10-10-source-resource-distribution.md">资源分布方法</a> · <a href="2026-10-10-media-install-probe.md">安装探针方法</a> · <a href="2026-10-10-sparkle-arm64-size-probe.md">框架切片方法</a></p></main></html>'''
    destination = ROOT / "docs/research/2026-10-10-package-size-implementation-report.html"
    destination.write_text(page)
    print(destination)


if __name__ == "__main__":
    main()
