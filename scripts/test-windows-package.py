#!/usr/bin/env python3
"""Windows bundle invariants use isolated binary fixtures; no fixture is product acceptance."""
import importlib.util
import hashlib
import json
from pathlib import Path
import tempfile
import shutil
import zipfile
from unittest.mock import patch
import unittest

class PackageTests(unittest.TestCase):
    def load(self):
        spec = importlib.util.spec_from_file_location("windows_package", Path(__file__).parent / "windows/package.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_required_helpers_cannot_be_silently_omitted(self):
        """Only main.exe is present: packaging must fail rather than hand off a broken helper layout."""
        module = self.load()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'norma-os.exe').write_bytes(b'MZfixture')
            with self.assertRaisesRegex(ValueError, 'norma.exe'):
                module.required_binaries(root)

    def complete_fixture(self, root):
        # Deliberately tiny files exercise bundle structure only, never App acceptance.
        names = ('norma-os.exe', 'norma.exe', 'multisoul-agent-hook.exe', 'norma-browser-auth-host.exe',
                 'vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll',
                 'Resources/home/earth-horizon.png', 'Resources/boot/boot-sync.mp4',
                 'Resources/boot/boot-awaken.mp4', 'Resources/browser-auth-extension/manifest.json',
                 'Resources/browser-auth-extension/service_worker.js', 'Resources/media-catalog.json',
                 'README.md', 'THIRD_PARTY_NOTICES.md')
        for name in names:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'MZfixture' if name.endswith('.exe') else b'fixture')
        self.write_manifest(root)

    def write_manifest(self, root):
        files = {path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
                 for path in root.rglob('*') if path.is_file() and path != root / 'manifest.json'}
        (root / 'manifest.json').write_text(json.dumps({'source_sha': 'fixture', 'files': files}),
                                            encoding='utf-8')

    def test_complete_bundle_layout_passes_integrity_verification(self):
        """构造完整的目录和独立计算的哈希；verify必须接受完整布局，不能恒定拒绝。"""
        module = self.load()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.complete_fixture(root)
            manifest = module.verify(root)
            self.assertEqual(manifest['source_sha'], 'fixture', '完整布局须保留来源身份')

    def test_missing_crt_is_rejected_even_when_manifest_is_self_consistent(self):
        """场景：有完整Resources和exe但遗漏MSVC运行库；重写自洽清单后verify；
        预期明确拒绝缺失的CRT，不能依赖构建runner上全局安装的运行库。
        """
        module = self.load()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.complete_fixture(root)
            (root / 'vcruntime140_1.dll').unlink()
            self.write_manifest(root)
            with self.assertRaisesRegex(ValueError, 'vcruntime140_1.dll'):
                module.verify(root)

    def test_self_consistent_binaries_cannot_replace_required_resources(self):
        """场景：四个exe及其哈希自洽，但完全缺少Resources和CRT；
        构造恶化后的候选目录，执行verify；预期拒绝而不是把自洽清单当完整发布包。
        """
        module = self.load()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in module.BINARIES:
                (root / name).write_bytes(b'MZfixture')
            manifest = {'source_sha': 'fixture', 'files': {
                name: module.file_digest(root / name) for name in module.BINARIES}}
            (root / 'manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'Resources'):
                module.verify(root)

    def test_archive_name_and_root_keep_the_release_identity(self):
        """场景：资源复制循环不能把发行包改名为browser-auth-extension。
        构造：完整微型资源/PE布局，Cargo/git边界返回固定版本2.3.4和40位SHA。
        执行：调用真实package创建ZIP并读取条目；预期文件名及所有条目根目录保留版本身份。
        夹具只验证打包结构，不作为真实App执行证据。
        """
        module = self.load()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / 'binaries'
            binaries.mkdir()
            self.complete_fixture(binaries)
            source = root / 'source'
            assets = source / 'crates/app/assets'
            for group in ('home', 'boot', 'browser-auth-extension'):
                shutil.copytree(binaries / 'Resources' / group, assets / group)
            media = source / 'build/release-media'
            media.mkdir(parents=True)
            shutil.copy2(binaries / 'Resources/media-catalog.json', media / 'media-catalog.json')
            docs = source / 'docs/references'
            docs.mkdir(parents=True)
            (docs / 'windows-preview.md').write_text('fixture', encoding='utf-8')
            (source / 'THIRD_PARTY_NOTICES.md').write_text('fixture', encoding='utf-8')
            metadata = json.dumps({'packages': [{'name': 'app', 'version': '2.3.4'}]})
            output = root / 'output'
            with patch.object(module, 'ROOT', source), patch.object(
                    module.subprocess, 'check_output', side_effect=[metadata, 'a' * 40]):
                module.package(binaries, output, None)
            name = 'norma-os-2.3.4-windows-x64-aaaaaaaa'
            archives = list(output.glob('*.zip'))
            self.assertEqual([path.name for path in archives], [name + '.zip'],
                             '发行包文件名必须保留版本、平台和源码身份')
            with zipfile.ZipFile(archives[0]) as archive:
                roots = {Path(path).parts[0] for path in archive.namelist()}
            self.assertEqual(roots, {name}, 'ZIP内部根目录必须与发行包身份一致')

    def test_lfs_pointer_is_rejected_as_a_resource(self):
        """An unhydrated Git LFS pointer must never be included as usable image/video bytes."""
        module = self.load()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'earth.png'
            path.write_text('version https://git-lfs.github.com/spec/v1\noid sha256:fixture\n')
            with self.assertRaisesRegex(ValueError, 'LFS'):
                module.file_digest(path)

if __name__ == '__main__':
    unittest.main(verbosity=2)
