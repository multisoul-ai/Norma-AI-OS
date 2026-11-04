import importlib.util
import pathlib
import json
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("engine_builder", ROOT / "scripts/prepare-a1-engine.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)

class EnginePackageTests(unittest.TestCase):
    # 场景：旧缓存不能在内置解释器或构建配方改变后继续作为新引擎发布。
    # 构造：自报归档哈希正确，但Python来源哈希错误的旧包元数据。
    # 执行：调用构建缓存校验；预期拒绝复用，而非仅比较归档与自身哈希。
    def test_package_cache_rejects_wrong_python_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "engine.tar.bz2").write_bytes(b"not an engine")
            (root / "engine.json").write_text(json.dumps({"requirements_sha256": builder.digest(builder.LOCK), "archive_sha256": builder.digest(root / "engine.tar.bz2")}))
            (root / "build.json").write_text(json.dumps({"signing_identity":"-", "python_sha256":"wrong"}))
            self.assertFalse(builder.verified_output(root, "-"), "解释器来源或配方不匹配必须重建")

    # 场景：通用Mach-O的otool输出含多个构建路径标题，标题不是动态库依赖。
    # 构造：双架构标题与系统/@rpath依赖；另加一条Homebrew绝对依赖。
    # 执行：通过依赖分类入口判断两种真实输出形状。
    # 预期：忽略标题，允许系统库，拒绝依赖构建机安装的动态库。
    def test_native_dependency_classification_ignores_architecture_headers(self):
        linked = "/Users/build/lib.dylib (architecture x86_64):\n\t@rpath/lib.dylib (compatibility 0)\n/Users/build/lib.dylib (architecture arm64):\n\t/usr/lib/libSystem.B.dylib (compatibility 1)\n"
        self.assertFalse(builder.has_external_dependencies(linked), "架构标题不能误判为外部依赖")
        self.assertTrue(builder.has_external_dependencies(linked + "\t/opt/homebrew/lib/libssl.dylib (compatibility 1)\n"), "真实外部依赖必须阻止打包")

    # 场景：可搬迁包中的内部链接须物化，不能依赖构建机目录；外部链接必须拒绝。
    # 构造：三个隔离目录和分别指向源目录内部/外部的链接。
    # 执行：物化包，再修改源文件；最后尝试物化含外部链接的包。
    # 预期：新包保持原始字节、没有符号链接，外部链接不会被复制或发布。
    def test_materialization_preserves_internal_bytes_and_rejects_escape(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "real").write_bytes(b"abc")
            (source / "alias").symlink_to("real")
            builder.materialize(source, root / "package")
            (source / "real").write_bytes(b"changed")
            self.assertEqual((root / "package/alias").read_bytes(), b"abc", "发布包必须独立于构建源")
            self.assertFalse((root / "package/alias").is_symlink(), "受管安装不接受符号链接")
            (root / "outside").write_bytes(b"private")
            (source / "escape").symlink_to(root / "outside")
            with self.assertRaises(ValueError, msg="不得把构建机外部文件打进语音引擎"):
                builder.materialize(source, root / "rejected")
            self.assertFalse((root / "rejected").exists(), "失败不得发布半成品目录")

class EngineReleaseTests(unittest.TestCase):
    # 场景：独立运行环境发布前必须校验真实字节，不能只检查下载地址可达。
    # 构造：固定小型归档和可信描述；执行校验，再把归档替换为同长度坏内容。
    # 预期：正确资源通过，损坏归档和可变latest地址均被拒绝。
    def test_release_assets_require_exact_bytes_and_immutable_https(self):
        spec = importlib.util.spec_from_file_location("release_check", ROOT / "scripts/verify-a1-engine-release.py")
        check = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(check)
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            payload = b"verified-runtime"
            manifest = {"schema":1, "target":"aarch64-apple-darwin", "minimum_macos_major":26,
                "requirements_sha256":builder.digest(builder.LOCK), "archive_bytes":len(payload),
                "archive_sha256":builder.hashlib.sha256(payload).hexdigest()}
            raw = json.dumps(manifest).encode()
            (root / "engine.json").write_bytes(raw)
            (root / "engine.tar.bz2").write_bytes(payload)
            release = {k:manifest[k] for k in ("schema","target","minimum_macos_major","requirements_sha256")}
            release.update(base_url="https://github.com/multisoul-ai/norma-os-releases/releases/download/a1-runtime-v1",
                manifest_bytes=len(raw),manifest_sha256=builder.hashlib.sha256(raw).hexdigest())
            check.verify(release, ROOT, root)
            (root / "engine.tar.bz2").write_bytes(b"x" * len(payload))
            with self.assertRaises(ValueError, msg="同长度篡改也不能发布"):
                check.verify(release, ROOT, root)
            release["base_url"] = "https://github.com/multisoul-ai/norma-os-releases/releases/latest/download"
            with self.assertRaises(ValueError, msg="不能通过可变地址分发可执行运行环境"):
                check.verify(release, ROOT, root)

if __name__ == "__main__":
    unittest.main()
