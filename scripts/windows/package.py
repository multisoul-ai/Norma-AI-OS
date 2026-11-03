#!/usr/bin/env python3
"""Create a versioned Windows portable candidate with helper/resource integrity evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[2]
BINARIES = ('norma-os.exe', 'norma.exe', 'multisoul-agent-hook.exe', 'norma-browser-auth-host.exe')
CRT = ('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll')
RESOURCES = ('Resources/home/earth-horizon.png', 'Resources/boot/boot-sync.mp4',
             'Resources/boot/boot-awaken.mp4', 'Resources/browser-auth-extension/manifest.json',
             'Resources/browser-auth-extension/service_worker.js', 'Resources/media-catalog.json')


def file_digest(path):
    sha = hashlib.sha256()
    with path.open('rb') as stream:
        first = stream.read(128)
        if first.startswith(b'version https://git-lfs.github.com/spec/v1'):
            raise ValueError(f'LFS pointer is not a release resource: {path}')
        sha.update(first)
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            sha.update(chunk)
    return sha.hexdigest()


def required_binaries(folder):
    paths = [folder / name for name in BINARIES]
    for path in paths:
        if not path.is_file():
            raise ValueError(f'Missing Windows binary: {path.name}')
    for path in paths:
        with path.open('rb') as stream:
            if stream.read(2) != b'MZ':
                raise ValueError(f'Not a Windows executable: {path.name}')
    return paths


def verify(folder):
    manifest = json.loads((folder / 'manifest.json').read_text(encoding='utf-8'))
    required_binaries(folder)
    for name in (*BINARIES, *RESOURCES, *CRT):
        if name not in manifest['files'] or not (folder / name).is_file():
            raise ValueError(f'Missing required package content: {name}')
        if not (folder / name).stat().st_size:
            raise ValueError(f'Empty required package content: {name}')
    for name, digest in manifest['files'].items():
        path = folder / name
        if not path.resolve().is_relative_to(folder.resolve()):
            raise ValueError(f'Invalid package path: {name}')
        if file_digest(path) != digest:
            raise ValueError(f'Package content hash mismatch: {name}')
    print(json.dumps({'verified_files': len(manifest['files']), 'source_sha': manifest['source_sha']}))
    return manifest


def package(binaries, output, crt):
    paths = required_binaries(binaries)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'], cwd=ROOT))
    version = next(package['version'] for package in metadata['packages'] if package['name'] == 'app')
    sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    name = f'norma-os-{version}-windows-x64-{sha[:8]}'
    output.mkdir(parents=True, exist_ok=True)
    folder = output / name
    if folder.exists():
        raise ValueError(f'Candidate directory already exists: {folder}')
    folder.mkdir()
    for path in paths:
        shutil.copy2(path, folder / path.name)
    for source in (binaries, crt):
        if source is not None:
            for path in source.glob('*.dll'):
                shutil.copy2(path, folder / path.name)
    resources = folder / 'Resources'
    resources.mkdir()
    for group in ('home', 'boot', 'browser-auth-extension'):
        shutil.copytree(ROOT / 'crates/app/assets' / group, resources / group)
    media = ROOT / 'build/release-media'
    for path in media.iterdir():
        if path.is_dir():
            shutil.copytree(path, resources / path.name)
        else:
            shutil.copy2(path, resources / path.name)
    shutil.copy2(ROOT / 'THIRD_PARTY_NOTICES.md', folder / 'THIRD_PARTY_NOTICES.md')
    shutil.copy2(ROOT / 'docs/references/windows-preview.md', folder / 'README.md')
    files = {p.relative_to(folder).as_posix(): file_digest(p)
             for p in sorted(folder.rglob('*')) if p.is_file()}
    manifest = {'version': version, 'source_sha': sha, 'run_id': os.environ.get('GITHUB_RUN_ID'),
                'architecture': 'x86_64-pc-windows-msvc', 'signed': False, 'files': files}
    (folder / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    verify(folder)
    archive = output / f'{name}.zip'
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as stream:
        for path in sorted(folder.rglob('*')):
            if path.is_file():
                stream.write(path, Path(name) / path.relative_to(folder))
    digest = file_digest(archive)
    archive.with_suffix('.zip.sha256').write_text(f'{digest}  {archive.name}\n', encoding='ascii')
    print(json.dumps({'archive': str(archive), 'sha256': digest, 'size': archive.stat().st_size}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binaries', type=Path, default=ROOT / 'target/release')
    parser.add_argument('--output', type=Path, default=ROOT / 'build/windows')
    parser.add_argument('--crt-dir', type=Path)
    parser.add_argument('--verify', type=Path)
    args = parser.parse_args()
    if args.verify:
        verify(args.verify)
    else:
        package(args.binaries, args.output, args.crt_dir)

if __name__ == '__main__':
    main()
