#!/usr/bin/env bash
# 音频只写仓库外；第二参数用于聚焦 smoke，正式开发集不传 limit。
set -euo pipefail
script_root=$(cd "$(dirname "$0")/../.." && pwd)
python3 - "$script_root" "${1:?用法: synthesize.sh 仓库外输出目录 [文本数]}" "${2:-120}" <<'PY'
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import wave

repo, destination, limit = Path(sys.argv[1]), Path(sys.argv[2]).expanduser().resolve(), int(sys.argv[3])
if destination == repo or repo in destination.parents:
    raise SystemExit('音频必须输出到仓库外（建议 ~/.norma/voice-eval/synthetic-dev）')
if not 1 <= limit <= 120:
    raise SystemExit('文本数必须在 1–120 之间')
destination.mkdir(parents=True, exist_ok=True)
(destination / 'audio').mkdir(exist_ok=True)
voices = ['Tingting', 'Eddy', 'Flo', 'Reed']
rows = [json.loads(line) for line in (repo / 'test/voice-eval/synthetic-texts.jsonl').read_text().splitlines()][:limit]
output = destination / 'corpus.jsonl'
if output.exists():
    raise SystemExit('corpus.jsonl 已存在，请选择新目录；不会覆盖已有语料')
with output.open('x') as manifest:
    for row in rows:
        for voice in voices:
            sample_id = row['id'] + '-' + voice.lower()
            path = destination / 'audio' / (sample_id + '.wav')
            if path.exists():
                raise SystemExit(f'禁止覆盖已有音频：{path}')
            with tempfile.TemporaryDirectory(prefix='voice-eval-say-') as temporary:
                aiff = Path(temporary) / 'source.aiff'
                subprocess.run(['say', '-v', voice + ' (Chinese (China mainland))', '-o', str(aiff), row['text']], check=True)
                subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-n', '-i', str(aiff),
                                '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', str(path)], check=True)
            with wave.open(str(path)) as audio:
                if (audio.getframerate(), audio.getnchannels(), audio.getsampwidth()) != (16000, 1, 2) or not audio.getnframes():
                    raise SystemExit(f'音频格式错误：{path}')
                duration = audio.getnframes() / 16000
            entry = dict(row, id=sample_id, audio=str(path.relative_to(destination)),
                         sha256=hashlib.sha256(path.read_bytes()).hexdigest(), voice=voice,
                         mic='synthetic', env='synthetic', source='synthetic-dev', duration_s=duration)
            manifest.write(json.dumps(entry, ensure_ascii=False) + '\n')
            manifest.flush()
        print(f'{row["id"]}: 4 voices complete', flush=True)
print(json.dumps({'samples': len(rows) * 4, 'corpus': str(output)}, ensure_ascii=False))
PY
