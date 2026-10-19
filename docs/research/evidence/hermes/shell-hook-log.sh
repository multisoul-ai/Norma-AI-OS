#!/bin/sh
# 把 stdin 的 JSON 原样追加到日志，并带上单调时间戳
{ printf '%s ' "$(date +%H:%M:%S)"; cat; printf '\n'; } >> /tmp/hermes-probe/hook-events.log
