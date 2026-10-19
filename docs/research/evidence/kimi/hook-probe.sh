#!/bin/zsh
# 记录 stdin 载荷、cwd、KIMI 相关环境变量与 pid 链
{ print -n "{\"ts\":\"$(date +%H:%M:%S.%3N 2>/dev/null || date +%H:%M:%S)\",\"cwd\":\"$PWD\",\"env\":\"$(env | grep -i -E '^(KIMI|NORMA|MULTISOUL)' | tr '\n' ' ')\",\"payload\":"; cat; print "}"; } >> /tmp/kimi-iso/hooks.jsonl
exit 0
