#!/usr/bin/env python3
"""本地语音实验评分；仅 Python 标准库，不读取网络或凭据。"""
import argparse
import collections
import hashlib
import json
import math
from pathlib import Path
import random
import unicodedata

NORMALIZATION = "nfkc-casefold-cjkchar-word-v1"


def mixed_tokens(text):
    # 与 experiments/offline-asr/asr_lab/scoring.py 的冻结规则一致。
    text = unicodedata.normalize("NFKC", text).casefold().replace("’", "'")
    result, word = [], []
    for index, char in enumerate(text):
        c = ord(char)
        cjk = 0x3400 <= c <= 0x9FFF or 0xF900 <= c <= 0xFAFF or 0x20000 <= c <= 0x323AF
        if cjk:
            if word:
                result.append("".join(word))
                word = []
            result.append(char)
        elif char.isalnum():
            word.append(char)
        elif char == "'" and word and index + 1 < len(text) and text[index + 1].isalnum() and not (
                0x3400 <= ord(text[index + 1]) <= 0x9FFF or
                0xF900 <= ord(text[index + 1]) <= 0xFAFF or
                0x20000 <= ord(text[index + 1]) <= 0x323AF):
            word.append(char)
        elif word:
            result.append("".join(word))
            word = []
    if word:
        result.append("".join(word))
    return result


def edit_distance(reference, hypothesis):
    ref, hyp = mixed_tokens(reference), mixed_tokens(hypothesis)
    previous = list(range(len(hyp) + 1))
    for i, expected in enumerate(ref, 1):
        current = [i]
        for j, actual in enumerate(hyp, 1):
            current.append(min(previous[j] + 1, current[j - 1] + 1,
                               previous[j - 1] + (expected != actual)))
        previous = current
    return previous[-1], len(ref)


def contains_term(text, term):
    haystack, needle = mixed_tokens(text), mixed_tokens(term)
    return bool(needle) and any(haystack[i:i + len(needle)] == needle
                                for i in range(len(haystack) - len(needle) + 1))


def quantile(values, q):
    values = sorted(values)
    if not values:
        return None
    x = (len(values) - 1) * q
    lo = int(x)
    return values[lo] + (values[min(lo + 1, len(values) - 1)] - values[lo]) * (x - lo)


def distribution(values):
    values = [v for v in values if v is not None]
    return {"n": len(values), "p50": quantile(values, .5), "p95": quantile(values, .95),
            "max": max(values) if values else None}


def ratio(n, d):
    return n / d if d else None


def key(row):
    return row["id"], row["repeat"]


def index_rows(rows):
    result = {}
    for row in rows:
        k = key(row)
        if k in result:
            raise ValueError(f"重复记录：{k}")
        result[k] = row
    return result


def validate_asr_rows(corpus, rows, label, repeats):
    expected = {(sid, r) for sid in corpus for r in range(repeats)}
    actual = set(index_rows(rows))
    if actual != expected:
        raise ValueError(f"{label}: 不完整评测，缺少 {len(expected - actual)}，多余 {len(actual - expected)}")
    configs = set()
    for row in rows:
        if row.get("group") != label:
            raise ValueError(f"{label}: ASR 组名与缓存文件名不符")
        if row.get("tool_fixture"):
            continue
        if not corpus[row["id"]].get("sha256") or row.get("audio_sha256") != corpus[row["id"]]["sha256"]:
            raise ValueError(f'{label}/{row["id"]}: 音频哈希与语料不一致')
        config = row.get("config_sha256", "")
        if len(config) != 64:
            raise ValueError(f'{label}/{row["id"]}: 缺少配置 SHA-256')
        configs.add(config)
    if len(configs) > 1:
        raise ValueError(f"{label}: 同一 ASR 组混入不同配置")
    if len({row.get("realtime") for row in rows}) > 1:
        raise ValueError(f"{label}: 同组不能混合实时与快速回放")


def summarize(corpus, asr, routes=None, roster=()):
    index_rows(asr)
    route_index = index_rows(routes or [])
    count = collections.Counter()
    by_sample = collections.defaultdict(set)
    outcomes = {}
    aliases = {a["name"]: {a["name"], a.get("speech_alias")} - {None, ""} for a in roster}
    for row in asr:
        sample = corpus[row["id"]]
        text = row["text"]
        errors, units = edit_distance(sample["text"], text)
        count.update(errors=errors, units=units)
        by_sample[row["id"]].add(text)
        terms = sample.get("terms", [])
        for term in terms:
            kind = term["kind"]
            hit = contains_term(text, term["text"]) if kind == "name" else term["text"] in text
            count[f"{kind}_total"] += 1
            count[f"{kind}_hits"] += hit
        route = route_index.get(key(row))
        if route is None:
            continue
        count["model"] += route["path"] == "model"
        count["clarify"] += route["path"] in ("clarify", "rejected")
        if route["targets"] is None:
            continue
        count["known"] += 1
        correct = set(route["targets"]) == set(sample.get("targets", []))
        count["target_correct"] += correct
        outcomes[f'{row["id"]}:{row["repeat"]}'] = correct
        count["action_correct"] += correct and route["action"] == sample.get("action")
        extra = set(route["targets"]) - set(sample.get("targets", []))
        if extra:
            forms = {name: set(values) for name, values in aliases.items()}
            for name in set(sample.get("targets", [])) | set(route["targets"]):
                forms.setdefault(name, {name})
            for term in terms:
                if term["kind"] == "name":
                    name = term.get("agent", term["text"])
                    forms.setdefault(name, {name}).add(term["text"])
            entities = lambda text: {name for name, values in forms.items()
                                     if any(contains_term(text, value) for value in values)}
            reference_names, heard_names = entities(sample["text"]), entities(text)
            if extra & (heard_names - reference_names):
                count["recognition_wrong"] += 1
            elif reference_names == heard_names:
                count["routing_wrong"] += 1
            else:
                count["unattributed_wrong"] += 1
    n = len(asr)
    complete = bool(routes) and count["known"] == n
    result = {
        "samples": len(by_sample), "observations": n,
        "fixture_only": any(r.get("tool_fixture") for r in asr),
        "replay_pacing": ("realtime" if all(r.get("realtime") is True for r in asr)
                          else "fast" if all(r.get("realtime") is False for r in asr)
                          else "see-manifest"),
        "mer": ratio(count["errors"], count["units"]),
        "errors": count["errors"], "reference_units": count["units"],
        "name_recall": ratio(count["name_hits"], count["name_total"]),
        "term_recall": ratio(count["term_hits"], count["term_total"]),
        "target_accuracy": ratio(count["target_correct"], n) if complete else None,
        "known_target_accuracy": ratio(count["target_correct"], count["known"]),
        "action_and_target_accuracy": ratio(count["action_correct"], n) if complete else None,
        "route_unresolved": n - count["known"],
        "routing_wrong_count": count["routing_wrong"],
        "recognition_wrong_count": count["recognition_wrong"],
        "unattributed_wrong_count": count["unattributed_wrong"],
        "routing_wrong_rate": ratio(count["routing_wrong"], n) if complete and not count["unattributed_wrong"] else None,
        "recognition_wrong_rate": ratio(count["recognition_wrong"], n) if complete and not count["unattributed_wrong"] else None,
        "clarify_rate": ratio(count["clarify"], n) if complete else None,
        "model_fallback_rate": ratio(count["model"], n) if routes else None,
        "unstable_samples": sorted(k for k, texts in by_sample.items() if len(texts) > 1),
        "t_final_ms": distribution(r.get("t_final_ms") for r in asr if r.get("warm")),
        "t_preview_ms": distribution(r.get("t_preview_ms") for r in asr if r.get("warm")),
        "t_ready_ms": distribution(r.get("t_ready_ms") for r in asr if not r.get("warm")),
        "t_route_ms": distribution(r.get("t_route_ms") for r in route_index.values()),
        "t_route_direct_ms": distribution(r.get("t_route_ms") for r in route_index.values()
                                          if r["path"] == "direct"),
        "t_e2e_ms": distribution(r.get("t_e2e_ms") for r in route_index.values()),
        "qwen_peak_rss_bytes": max((r.get("qwen_peak_rss_bytes") or 0 for r in asr), default=0) or None,
        "outcomes": outcomes,
    }
    repeat_mer = []
    for repeat in sorted({r["repeat"] for r in asr}):
        scores = [edit_distance(corpus[r["id"]]["text"], r["text"]) for r in asr if r["repeat"] == repeat]
        repeat_mer.append(ratio(sum(s[0] for s in scores), sum(s[1] for s in scores)))
    result["repeat_mer"] = repeat_mer
    result["repeat_mer_range_pp"] = ((max(repeat_mer) - min(repeat_mer)) * 100
                                      if repeat_mer and None not in repeat_mer else None)
    return result


def mcnemar(before, after):
    if set(before) != set(after):
        raise ValueError("配对样本集合不一致")
    improved = sorted(k for k in before if not before[k] and after[k])
    regressed = sorted(k for k in before if before[k] and not after[k])
    n = len(improved) + len(regressed)
    tail = sum(math.comb(n, i) for i in range(min(len(improved), len(regressed)) + 1))
    return {"paired_samples": len(before), "improved": improved, "regressed": regressed,
            "p_exact": min(1.0, 2 * tail / 2 ** n) if n else 1.0}


def latency_difference_ci(before, after, iterations=1000):
    if len(before) != len(after) or not before:
        raise ValueError("时延配对集合为空或不一致")
    rng = random.Random(20261031)
    draws = {"p50": [], "p95": []}
    # 以音频为簇重采样，保留同一音频的全部重复观测。
    for _ in range(iterations):
        indices = rng.choices(range(len(before)), k=len(before))
        a = [v for i in indices for v in before[i]]
        b = [v for i in indices for v in after[i]]
        for label, q in [("p50", .5), ("p95", .95)]:
            draws[label].append(quantile(b, q) - quantile(a, q))
    return {label: [quantile(values, .025), quantile(values, .975)]
            for label, values in draws.items()}


def read_rows(path):
    text = Path(path).read_text()
    if text.lstrip().startswith("{") and Path(path).suffix == ".json":
        data = json.loads(text)
        return [dict(r, source=r.get("source", data.get("source"))) for r in data["samples"]]
    return [json.loads(line) for line in text.splitlines() if line.strip()]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate_route_binding(asr_path, route_path, routes):
    manifest_path = route_path.with_suffix(".manifest.json")
    if not manifest_path.exists():
        manifest_path = route_path.parent / "route-manifest.json"
    if not manifest_path.exists():
        raise ValueError(f"{route_path}: 缺少绑定 ASR 来源的路由 manifest")
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("asr_sha256") != sha(asr_path):
        raise ValueError(f"{route_path}: 路由缓存与实际 ASR 源哈希不一致")
    config = manifest.get("config_sha256")
    if not config or any(r.get("config_sha256") != config for r in routes):
        raise ValueError(f"{route_path}: 路由配置哈希与 manifest 不一致")


def check_private_output(samples, output):
    repo = Path(__file__).resolve().parents[2]
    human = any(r.get("source") == "human-holdout" or r["id"].startswith("human-") or
                r.get("annotation_status") is not None for r in samples)
    if human and (output.resolve() == repo or repo in output.resolve().parents):
        raise ValueError("真人录音与转写结果必须位于仓库外的本机私有目录")


def percentage(value):
    return "未测/未决" if value is None else f"{value * 100:.2f}%"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--asr", type=Path, nargs="+", required=True)
    parser.add_argument("--routes", type=Path, nargs="*", default=[])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--roster", type=Path, default=Path(__file__).resolve().parents[2] / "test/voice-eval/roster.json")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--bootstrap", type=int, default=1000)
    args = parser.parse_args()
    samples = read_rows(args.corpus)
    check_private_output(samples, args.output)
    corpus = {r["id"]: r for r in samples}
    roster = json.loads(args.roster.read_text())["agents"]
    roster_hash = sha(args.roster)
    if any(s.get("roster_sha256", roster_hash) != roster_hash for s in samples):
        raise ValueError("语料名单快照与评测名单 SHA-256 不一致")
    if len(corpus) != len(samples):
        raise ValueError("语料 id 重复")
    groups, summaries, route_files, matched_routes = {}, {}, {}, set()
    for path in args.routes:
        rows = read_rows(path)
        label = path.stem.removeprefix("route-")
        route_files[label] = (path, rows)
    for path in args.asr:
        rows = read_rows(path)
        label = path.stem.removeprefix("asr-")
        validate_asr_rows(corpus, rows, label, args.repeats)
        actual = set(index_rows(rows))
        groups[label] = rows
        summaries[label] = summarize(corpus, rows, roster=roster)
        for route_label, (route_path, routes) in route_files.items():
            if route_label.startswith(label + "-"):
                validate_route_binding(path, route_path, routes)
                if set(index_rows(routes)) != actual:
                    raise ValueError(f"{route_label}: 路由/ASR 配对集合不一致")
                summaries[route_label] = summarize(corpus, rows, routes, roster)
                matched_routes.add(route_label)
    if matched_routes != set(route_files):
        raise ValueError("路由缓存没有匹配的 ASR 输入组")
    comparisons = {}
    labels = list(groups)
    if labels:
        base = labels[0]
        for label in labels[1:]:
            before, after = index_rows(groups[base]), index_rows(groups[label])
            # 不把重复跑的同一音频冒充独立样本；三轮全部正确才算样本正确。
            exact = lambda data: {sid: all(edit_distance(corpus[sid]["text"],
                                                       data[sid, r]["text"])[0] == 0
                                             for r in range(args.repeats)) for sid in corpus}
            comparison = {"exact_text": mcnemar(exact(before), exact(after))}
            for kind in ("name", "term"):
                ids = [sid for sid in corpus if any(t["kind"] == kind for t in corpus[sid].get("terms", []))]
                def term_success(data):
                    return {sid: all((contains_term(data[sid, r]["text"], t["text"]) if kind == "name"
                                      else t["text"] in data[sid, r]["text"])
                                     for r in range(args.repeats) for t in corpus[sid]["terms"] if t["kind"] == kind)
                            for sid in ids}
                if ids:
                    comparison[kind] = mcnemar(term_success(before), term_success(after))
            common = {sid: [r for r in range(args.repeats) if before[sid, r].get("warm") and after[sid, r].get("warm")]
                      for sid in corpus}
            paired = [sid for sid in corpus if common[sid]]
            if paired:
                comparison["t_final_difference_95ci_ms"] = latency_difference_ci(
                    [[before[sid, r]["t_final_ms"] for r in common[sid]] for sid in paired],
                    [[after[sid, r]["t_final_ms"] for r in common[sid]] for sid in paired], args.bootstrap)
            comparisons[f"{base}-vs-{label}"] = comparison
    route_labels = [label for label, s in summaries.items() if s["target_accuracy"] is not None]
    if route_labels:
        def target_success(label):
            outcomes = summaries[label]["outcomes"]
            return {sid: all(outcomes[f"{sid}:{r}"] for r in range(args.repeats)) for sid in corpus}
        for label in route_labels[1:]:
            comparisons[f"{route_labels[0]}-vs-{label}"] = {
                "target": mcnemar(target_success(route_labels[0]), target_success(label))}
    result = {"normalization": NORMALIZATION, "corpus_sha256": sha(args.corpus), "roster_sha256": roster_hash,
              "inputs": {str(p): sha(p) for p in args.asr + args.routes},
              "groups": summaries, "comparisons": comparisons,
              "scope": "文件回放/路由重放；纯函数耗时不包括 L0 memory_preamble；不代表真实麦克风或 T_e2e"}
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "summary.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    lines = ["# 本地语音实验报告", "", f"归一化：{NORMALIZATION}；每条 {args.repeats} 次。", "",
             "未知模型结果不计为正确，指标为未决时不能通过阶段门槛。", "",
             "| 组 | MER | 人名识别率 | 术语识别率 | 目标正确率 | 路由错派率 | 识别错派率 | 反问率 | 模型兜底率 | T_final p50/p95 ms | T_route direct p95 ms | 未决(模型/归因) |",
             "|---|---|---|---|---|---|---|---|---|---|---|---|"]
    if any(s["fixture_only"] for s in summaries.values()):
        lines[2:2] = ["**仅工具夹具 smoke，含人为设定转写/时延；不能作为任何阶段准确率或性能证据。**", ""]
    for label, s in summaries.items():
        rates = [percentage(s[k]) for k in ["mer", "name_recall", "term_recall", "target_accuracy",
                                           "routing_wrong_rate", "recognition_wrong_rate", "clarify_rate", "model_fallback_rate"]]
        timing = s["t_final_ms"]
        lines.append("| " + " | ".join([label] + rates + [f'{timing["p50"]}/{timing["p95"]}',
                     str(s["t_route_direct_ms"]["p95"]), f'{s["route_unresolved"]}/{s["unattributed_wrong_count"]}']) + " |")
    lines.extend(["", result["scope"], "", "配对差异与 95% CI：", "", "```json",
                  json.dumps(comparisons, ensure_ascii=False, indent=2), "```", "",
                  "真人集仅报告总体；50 条难例的统计力有限，小幅提升不能宣称显著。", ""])
    if any(s["replay_pacing"] == "fast" for s in summaries.values()):
        lines.extend(["快速文件回放的 T_preview 仅为文件送帧路径时延，不能代表真实麦克风字幕时延；T_final 仍从 Finish 起计。", ""])
    (args.output / "report.md").write_text("\n".join(lines))
    print(json.dumps({"groups": list(summaries), "output": str(args.output)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
