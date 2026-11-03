#!/usr/bin/env python3
"""Require an isolated probe to use package versions/checksums already frozen by the product."""
from pathlib import Path
import sys
import tomllib

product, probe = (tomllib.loads(Path(path).read_text(encoding='utf-8'))
                  for path in sys.argv[1:])
known = {(package['name'], package['version'], package.get('source')): package.get('checksum')
         for package in product['package']}
for package in probe['package']:
    if package['name'] == 'app-keyboard-probe' and 'source' not in package:
        continue
    key = (package['name'], package['version'], package.get('source'))
    if key not in known or package.get('checksum') != known[key]:
        raise SystemExit(f'Probe dependency is outside the product lock: {key}')
print(f'PASS: {len(probe["package"])} probe packages retain the product dependency identities')
