#!/usr/bin/env python3
"""Windows bundle invariants use isolated binary fixtures; no fixture is product acceptance."""
import importlib.util
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
    unittest.main()
