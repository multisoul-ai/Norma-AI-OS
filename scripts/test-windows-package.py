#!/usr/bin/env python3
"""Windows bundle invariants use isolated binary fixtures; no fixture is product acceptance."""
import importlib.util
import hashlib
import json
from pathlib import Path
import tempfile
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
