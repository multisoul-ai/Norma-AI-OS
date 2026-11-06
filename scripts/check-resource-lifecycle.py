#!/usr/bin/env python3
"""Guard known resource leaks; Clippy supplies type-aware Child/mem::forget checks.

This is a narrow source guard, not a Rust ownership analyser. In particular,
short-lived macOS `open` commands must use status/output, not spawn. Native
DisplayLink behaviour is also exercised by test-display-link-lifecycle.py.
"""

import argparse
import re
import subprocess
from pathlib import Path


DISPLAY_LINK = "vendor/gpui-0.2.2/src/platform/mac/display_link.rs"


def tokens(source):
    """Skip Rust comments; preserve strings as indivisible tokens (including raw strings)."""
    result = []
    i = 0
    while i < len(source):
        if source[i].isspace():
            i += 1
        elif source.startswith("//", i):
            end = source.find("\n", i)
            i = len(source) if end == -1 else end
        elif source.startswith("/*", i):
            depth = 1
            i += 2
            while i < len(source) and depth:
                if source.startswith("/*", i):
                    depth += 1
                    i += 2
                elif source.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
        else:
            raw = re.match(r'(?:br|cr|r)(#*)"', source[i:])
            if raw:
                end_marker = '"' + raw[1]
                end = source.find(end_marker, i + raw.end())
                end = len(source) if end == -1 else end + len(end_marker)
            else:
                match = re.match(r'''(?:b|c)?"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])'|[A-Za-z_][A-Za-z_0-9]*|::|[^\s]''', source[i:])
                end = i + len(match[0])
            result.append(source[i:end])
            i = end
    return result


def check_source(path, source):
    display_link = path.startswith("vendor/gpui-0.2.2/src/platform/mac/display_link")
    if not display_link and not ("Command" in source and "open" in source):
        return []
    code = tokens(source)
    # Raw and ordinary literals denote the same executable.
    code = [re.sub(r'^r(#+)?"(.*)"\1?$', r'"\2"', item, flags=re.S) if item.startswith('r') else item for item in code]
    code = ['"<literal>"' if re.match(r'^(?:br|cr|b|c)?"|^(?:br|cr)#', item)
            and item not in ('"open"', '"/usr/bin/open"') else item for item in code]
    failures = []
    names = ["Command", *re.findall(r'\bCommand as (\w+)', " ".join(code))]
    if spawned_open(code, names):
        failures.append("open-child")
    if display_link:
        joined = " ".join(code)
        if re.search(r'\b(?:forget|leak) \(', joined):
            failures.append("forgotten-resource")
    return sorted(set(failures))


def spawned_open(code, names):
    """Follow direct method chains and simple local bindings within lexical scopes."""
    pairs, stack = {}, []
    for i, token in enumerate(code):
        if token in ("(", "[", "{"):
            stack.append(i)
        elif token in (")", "]", "}") and stack:
            pairs[stack.pop()] = i
    starts = {i: i + 6 for i in range(len(code) - 5)
              if code[i] in names and code[i + 1:i + 4] == ["::", "new", "("]
              and code[i + 4] in ('"open"', '"/usr/bin/open"') and code[i + 5] == ")"}

    def chain_spawns(i):
        while i + 2 < len(code) and code[i] == "." and code[i + 2] == "(":
            end = pairs.get(i + 2)
            if end is None:
                break
            if code[i + 1] == "spawn":
                return True
            i = end + 1
        return False

    scopes = [{}]
    for i, token in enumerate(code):
        if token == "{":
            scopes.append({})
        elif token == "}" and len(scopes) > 1:
            scopes.pop()
        elif token == "let" and i + 2 < len(code):
            name_at = i + 1 + (code[i + 1] == "mut")
            name, equal = code[name_at], name_at + 1
            if equal < len(code) and code[equal] == ":":
                while equal < len(code) and code[equal] not in ("=", ";", "{", "}"):
                    equal += 1
            if equal < len(code) and code[equal] == "=" and re.fullmatch(r'\w+', name):
                rhs = equal + 1
                while rhs + 1 < len(code) and code[rhs + 1] == "::" and code[rhs] not in names:
                    rhs += 2
                # A new local declaration shadows an outer or previous binding,
                # including declarations of unrelated commands.
                scopes[-1][name] = rhs in starts
        is_open = next((scope[token] for scope in reversed(scopes) if token in scope), False)
        if (i in starts and chain_spawns(starts[i])) or (is_open and chain_spawns(i + 1)):
            return True
    return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--staged", action="store_true", help="Read the Git index, not unstaged fixes.")
    args = parser.parse_args()
    root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
    options = ["--cached"] if args.staged else ["--cached", "--others", "--exclude-standard"]
    paths = set(subprocess.check_output(["git", "ls-files", "-z", *options], cwd=root).decode().split("\0"))
    failures = []
    for path in sorted(paths):
        if not path.endswith(".rs") or not (path.startswith("crates/") or path.startswith(DISPLAY_LINK[:-3])):
            continue
        if args.staged:
            source = subprocess.check_output(["git", "show", f":{path}"], cwd=root).decode()
        elif (root / path).exists():
            source = (root / path).read_text()
        else:
            continue
        failures.extend(f"{path}: {rule}" for rule in check_source(path, source))
    for failure in failures:
        print(f"ERROR: {failure}")
    if failures:
        print("Use background Command::status/output for open; retain shared display links and release window sources.")
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
