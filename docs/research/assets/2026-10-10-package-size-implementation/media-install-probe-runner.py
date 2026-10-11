from pathlib import Path
import csv
import datetime
import hashlib
import json
import re
import shutil
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = Path(__file__).resolve().parent
EVIDENCE = ROOT / 'docs/research/assets/2026-10-10-package-size-implementation'


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()


manifest = json.loads((ROOT / 'packaging/media/resources.json').read_text())
resource = max(manifest['resources'], key=lambda row: row['bytes'])
source = ROOT / 'crates/app/assets' / resource['path']
assert digest(source) == resource['sha256'], 'largest original video must match pinned metadata'
identifier = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
runroot = OUTPUT / ('media-install-probe-runs-' + identifier)
runroot.mkdir()
results = []
for mode in ('install', 'repair', 'cancel'):
    for repeat in range(1, 4):
        cache = runroot / f'{mode}-{repeat}'
        command = ['/usr/bin/time', '-l', str(OUTPUT / 'media-install-probe'), str(source),
            str(cache), resource['path'], str(resource['bytes']), resource['sha256'], mode]
        started = time.monotonic()
        result = subprocess.run(command, capture_output=True, text=True)
        (runroot / f'{mode}-{repeat}.stdout.json').write_text(result.stdout)
        (runroot / f'{mode}-{repeat}.stderr.log').write_text(result.stderr)
        if result.returncode:
            print(result.stderr, flush=True)
            raise SystemExit(result.returncode)
        metrics = json.loads(result.stdout)
        memory = re.search(r'(\d+)\s+maximum resident set size', result.stderr)
        assert memory, 'Darwin time must report each successful process peak RSS'
        if mode == 'repair':
            assert metrics['repair_received_bytes'] == resource['bytes'], 'repair must stream all original bytes'
        metrics.update(repeat=repeat, wall_seconds=time.monotonic() - started,
            peak_rss_bytes=int(memory.group(1)), exit_code=result.returncode)
        results.append(metrics)
        print(json.dumps(metrics), flush=True)
        (runroot / 'results.json').write_text(json.dumps(results, indent=2) + '\n')

summary = {}
for mode in ('install', 'repair', 'cancel'):
    runs = [row for row in results if row['mode'] == mode]
    summary[mode] = {'successful_runs': len(runs), 'source_bytes': resource['bytes'],
        'median_install_seconds': statistics.median(row['install_seconds'] for row in runs),
        'median_verify_seconds': statistics.median(row.get('verify_seconds', 0) for row in runs),
        'median_repair_seconds': statistics.median(row.get('repair_seconds', 0) for row in runs),
        'peak_rss_bytes': max(row['peak_rss_bytes'] for row in runs),
        'max_read_buffer_bytes': max(row['max_read_buffer_bytes'] for row in runs)}

rlibs = list((OUTPUT / 'media-store-clean-target/debug/deps').glob('libmedia_store-*.rlib'))
assert len(rlibs) == 1, 'library evidence must refer to one clean-target library'
record = {'checked_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'source': resource, 'probe': 'media_store public API linked from verified debug clean-target rlib',
    'scope': 'filesystem installer only; excludes HTTP, GPUI and production release performance',
    'cache_condition': 'isolated empty cache per run; warm OS source cache; no network',
    'measurement': 'Darwin /usr/bin/time -l peak RSS; median of three runs; maximum measured RSS',
    'sdk': '26.5', 'run_directory': str(runroot),
    'probe_sha256': digest(OUTPUT / 'media-install-probe'),
    'probe_source_sha256': digest(OUTPUT / 'media-install-probe.rs'),
    'library_sha256': digest(rlibs[0]),
    'media_store_source_sha256': {str(path.relative_to(ROOT)): digest(path)
        for path in sorted((ROOT / 'crates/media_store/src').glob('*.rs'))},
    'summary': summary, 'runs': results}
for destination in (OUTPUT, EVIDENCE):
    (destination / 'media-install-probe.json').write_text(json.dumps(record, indent=2) + '\n')
shutil.copy2(OUTPUT / 'media-install-probe.rs', EVIDENCE / 'media-install-probe.rs')
raw = EVIDENCE / 'media-install-probe-raw'
raw.mkdir(exist_ok=True)
for path in runroot.glob('*.*'):
    if path.is_file():
        shutil.copy2(path, raw / path.name)
fields = ['mode', 'repeat', 'source_bytes', 'received_bytes', 'repair_received_bytes', 'install_seconds',
    'repair_seconds', 'verify_seconds', 'peak_rss_bytes', 'max_read_buffer_bytes', 'remaining_files',
    'byte_identical', 'exit_code']
with (EVIDENCE / 'media-install-probe.csv').open('w', encoding='utf-8-sig', newline='') as stream:
    writer = csv.DictWriter(stream, fieldnames=fields, extrasaction='ignore')
    writer.writeheader()
    writer.writerows(results)
assert digest(source) == resource['sha256'], 'all probe modes must leave the original video untouched'
print('COMPLETE', json.dumps(summary), flush=True)
