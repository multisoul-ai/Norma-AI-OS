import CryptoKit
import E2EKit
import Foundation

public enum VoiceCapabilitiesFixture {
    public static func preparedPython() throws -> String {
        guard let path = ProcessInfo.processInfo.environment["NORMA_A1_ACCEPTANCE_PYTHON"],
              FileManager.default.isExecutableFile(atPath: path) else {
            throw E2EError.env("需要NORMA_A1_ACCEPTANCE_PYTHON指向本机Python 3.12；固定依赖由App检查或准备")
        }
        return path
    }

    public static func requireJevCredential() throws {
        guard let secret = ProcessInfo.processInfo.environment["TYPESAFE_API_KEY"], !secret.isEmpty else {
            throw E2EError.env("真实Jev验收需要已授权的TYPESAFE_API_KEY；不会使用假服务代替")
        }
    }

    public static func keepLog(output: URL, name: String) throws {
        try FileManager.default.copyItem(at: output.appendingPathComponent("app.log"),
            to: output.appendingPathComponent(name))
    }

    public static func verifyA1(dataRoot: URL, output: URL) throws -> Data {
        let manifest = dataRoot.appendingPathComponent("models/a1/runtime.json")
        let bytes = try Data(contentsOf: manifest)
        guard let config = try JSONSerialization.jsonObject(with: bytes) as? [String: Any],
              config["protocol"] as? Int == 1,
              let python = config["python"] as? String,
              let pythonHash = config["python_sha256"] as? String,
              let zipformer = config["zipformer"] as? String,
              let qwen = config["qwen"] as? String else { throw E2EError.product("安装配置不符合A1协议v1") }
        let owned = dataRoot.appendingPathComponent("models/a1/packages").standardizedFileURL.path + "/"
        guard URL(fileURLWithPath: zipformer).standardizedFileURL.path.hasPrefix(owned),
              URL(fileURLWithPath: qwen).standardizedFileURL.path.hasPrefix(owned) else {
            throw E2EError.product("测试安装必须写入隔离模型目录，不能替换用户模型配置")
        }
        guard try sha256(URL(fileURLWithPath: python)) == pythonHash else {
            throw E2EError.product("已保存的Python完整性不匹配")
        }
        var repository = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { repository.deleteLastPathComponent() }
        let catalog = repository.appendingPathComponent("crates/asr/runtime/a1/artifacts.json")
        guard let files = try JSONSerialization.jsonObject(with: Data(contentsOf: catalog)) as? [[String: Any]],
              files.count == 14 else { throw E2EError.env("无法读取真实A1固定文件清单") }
        var total: UInt64 = 0
        for file in files {
            guard let role = file["role"] as? String, let name = file["name"] as? String,
                  let expected = file["sha256"] as? String, let size = file["bytes"] as? UInt64 else {
                throw E2EError.env("A1固定文件元数据不完整")
            }
            let path = URL(fileURLWithPath: role == "zipformer" ? zipformer : qwen).appendingPathComponent(name)
            let actual = try FileManager.default.attributesOfItem(atPath: path.path)[.size] as? UInt64
            guard actual == size, try sha256(path) == expected else {
                throw E2EError.product("安装后完整性复查失败：\(role)/\(name)")
            }
            total += size
        }
        let evidence: [String: Any] = ["protocol": 1, "artifacts_verified": files.count,
            "model_bytes": total, "python_sha256": pythonHash,
            "manifest_sha256": SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined(),
            "models_loaded": false, "provider_activated": false]
        try JSONSerialization.data(withJSONObject: evidence, options: [.prettyPrinted, .sortedKeys])
            .write(to: output.appendingPathComponent("a1-installed-integrity.json"), options: .atomic)
        return bytes
    }

    public static func assertNoPlaintextKey(dataRoot: URL) throws {
        guard let raw = ProcessInfo.processInfo.environment["TYPESAFE_API_KEY"] else {
            throw E2EError.env("缺少真实凭据，仅检查不输出其值")
        }
        let secret = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        for name in ["app.sqlite", "app.sqlite-wal", "app.sqlite-shm"] {
            let path = dataRoot.appendingPathComponent(name)
            if let bytes = try? Data(contentsOf: path), bytes.range(of: Data(secret.utf8)) != nil {
                throw E2EError.product("Jev敏感值进入SQLite；失败信息不包含密钥")
            }
        }
    }

    private static func sha256(_ url: URL) throws -> String {
        let file = try FileHandle(forReadingFrom: url)
        defer { try? file.close() }
        var digest = SHA256()
        while let chunk = try file.read(upToCount: 1_048_576), !chunk.isEmpty { digest.update(data: chunk) }
        return digest.finalize().map { String(format: "%02x", $0) }.joined()
    }
}
