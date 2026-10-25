import Foundation
import E2EKit

/// 假 Agent 只接受本场景写入的命令，并通过继承的真实 CLI 来源上下文展示内容。
/// 使用随机测试凭据和独立用户目录，不读取用户凭据；应用和 socket 由 Session 隔离。
public enum CanvasPresentationFixture {
    public static func install(cli: String) throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp/norma-present-e2e-\(UUID().uuidString)")
        let bin = root.appendingPathComponent("bin")
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        let config = ["root": root.path, "cli": cli]
        try JSONSerialization.data(withJSONObject: config).write(to: root.appendingPathComponent("config.json"))
        let script = """
        #!/usr/bin/env python3
        import json, os, pathlib, subprocess, time
        root = pathlib.Path(__file__).resolve().parent.parent
        cfg = json.loads((root / "config.json").read_text())
        node = os.environ.get("MULTISOUL_NODE_ID", "unknown")
        (root / ("agent-" + node)).write_text(node)
        print("LAYOUT AGENT " + node, flush=True)
        seen = set()
        while True:
            for request in sorted(root.glob("request-*.json")):
                if request.name in seen: continue
                data = json.loads(request.read_text())
                if str(data["node"]) != node: continue
                seen.add(request.name)
                result = subprocess.run([cfg["cli"]] + data["args"], capture_output=True, text=True, timeout=25)
                response = {"code": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
                response_path = root / (request.stem.replace("request-", "response-") + ".json")
                pending_path = response_path.with_suffix(".pending")
                pending_path.write_text(json.dumps(response))
                os.replace(pending_path, response_path)
                request.unlink()
            time.sleep(.1)
        """
        for name in ["claude", "codex"] {
            let path = bin.appendingPathComponent(name)
            try Data(script.utf8).write(to: path)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        }
        try Data("export PATH=\"\(bin.path):$PATH\"\n".utf8).write(to: root.appendingPathComponent(".zshrc"))
        return root
    }

    public static func agents(_ root: URL) throws -> [String] {
        try FileManager.default.contentsOfDirectory(atPath: root.path)
            .filter { $0.hasPrefix("agent-") }.map { String($0.dropFirst(6)) }.sorted()
    }

    /// App 停止时准备独立 Provider 安装身份；仅保存随机测试 token 和对应 SHA256。
    /// App 下次启动经原有安装检查、授权缓存及来源检查消费该夹具。
    public static func authorize(database: URL, homeDirectory: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-c", #"""
import hashlib, json, os, pathlib, secrets, sqlite3, sys, time
db, home = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
root = home / 'Library/Application Support/Norma OS/orchestration'
root.mkdir(parents=True, exist_ok=True)
token = secrets.token_hex(32)
credential = root / 'claude.json'
credential.write_text(json.dumps({'managed_by':'Norma OS','provider':'claude','token':token,
    'schema_version':1,'installed_version':'desktop-e2e'}))
os.chmod(credential, 0o600)
connection = sqlite3.connect(db)
now = int(time.time())
connection.execute('insert or replace into orchestration_authorizations values(?,?,?,?,?,?,NULL)',
    ('claude',hashlib.sha256(token.encode()).hexdigest(),1,'desktop-e2e',now,now))
connection.commit()
"""#, database.path, homeDirectory.path]
        process.standardOutput = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw E2EError.env("隔离 Provider 授权夹具建立失败") }
    }

    public static func request(_ root: URL, node: String, args: [String]) throws -> URL {
        let id = UUID().uuidString
        let data = try JSONSerialization.data(withJSONObject: ["node": node, "args": args])
        try data.write(to: root.appendingPathComponent("request-\(id).json"), options: .atomic)
        return root.appendingPathComponent("response-\(id).json")
    }

    /// App 停止后，注入旧版本持久化的隐藏 Browser Master 和八张 62px 细条。
    /// 只操作 Session 的隔离数据库；重启后的 prune、恢复及首帧布局均由生产代码执行。
    public static func seedLegacyMaster(dataRoot: URL, project: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-c", #"""
import hashlib, json, pathlib, sqlite3, sys
root, project = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]).resolve()
catalog = sqlite3.connect(root / 'app.sqlite')
areas = catalog.execute('select desktop_workspace_id,id from desktop_project_areas where root_path=?', (str(project),)).fetchall()
assert len(areas) == 1, 'Expected one isolated project area'
scope = ':'.join(areas[0])
file = root / 'desktop-canvases' / (hashlib.sha256(scope.encode()).hexdigest()+'.sqlite')
assert file.is_file(), 'Missing isolated project canvas database'
db = sqlite3.connect(file)
doc = json.loads(db.execute('select json from canvas_snapshots where id=1').fetchone()[0])
nodes = doc['nodes']
assert len(nodes) == 8, 'Expected six Agents and two Terminals'
original = [{'node_id': n['id'], 'bounds': n['bounds']} for n in nodes]
for i,n in enumerate(nodes):
    n['bounds'] = {'origin': {'x': 969, 'y': 8+i*70}, 'size': {'x':476, 'y':62}}
master = max(n['id'] for n in nodes)+1
nodes.append({'id': master, 'kind': {'Browser': {'url':'https://example.invalid/', 'title':'Legacy hidden Master'}},
              'bounds': {'origin':{'x':8,'y':8},'size':{'x':945,'y':909}}, 'z_index':master})
db.execute('update canvas_snapshots set json=? where id=1', (json.dumps(doc),))
db.execute("insert or replace into layout_state values(1,'master_detail',?,?,0)", (master,json.dumps(original)))
db.commit()
"""#, dataRoot.path, project.path]
        process.standardOutput = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw E2EError.product("旧隐藏 Master 夹具写入失败") }
    }
}
