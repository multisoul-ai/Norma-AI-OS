#!/bin/sh
# 记录 hook 原始 stdin 与相关环境变量
d=/tmp/zc-hooks; n=$(ls $d | wc -l | tr -d ' ')
f=$d/ev-$(date +%s%N 2>/dev/null || date +%s)-$n.json
cat > "$f"
env | grep -E '^(ZCODE|MULTISOUL|NORMA)_' | sort > "$f.env"
exit 0
