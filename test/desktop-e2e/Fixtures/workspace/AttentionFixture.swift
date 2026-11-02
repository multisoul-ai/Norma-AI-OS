import E2EKit
import Foundation

/// App 停止后写入真实冻结交付；使用生产序列化顺序生成内容指纹。
public enum AttentionFixture {
    public static func seed(database: URL, root: URL, cycle: String) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-c", #"""
import hashlib, json, pathlib, sqlite3, sys, time
db = sqlite3.connect(sys.argv[1])
root, cycle = pathlib.Path(sys.argv[2]), sys.argv[3]
workspace, area = db.execute('SELECT desktop_workspace_id,id FROM desktop_project_areas LIMIT 1').fetchone()
scope = f'desktop:{workspace}:{area}'
encode = lambda value: json.dumps(value, ensure_ascii=False, separators=(',', ':'))
for node, names in [(7, ['alpha.txt', 'beta.txt']), (8, ['gamma.txt'])]:
    key = dict(node_id=node, profile='Codex', run_id=f'run-{node}-{cycle}', session_id=f'session-{node}', turn_id=f'turn-{node}-{cycle}')
    outcome = dict(key=key, state='completed')
    files = []
    for name in names:
        text = f'Frozen {name} {cycle}'
        (root / name).write_text(text)
        files.append(dict(path=name, status='??', before=None,
                          after=dict(kind='text', executable=False, bytes=list(text.encode())),
                          index=None, diff=None, staged_diff=None))
    content = dict(status='available', review=dict(base_commit=None, files=files, conflicted=False))
    fingerprint = hashlib.sha256(encode([scope, outcome, content]).encode()).hexdigest()
    saved = dict(workspace_id=scope, outcome=outcome, content=content, fingerprint=fingerprint, saved_at_unix=int(time.time()))
    db.execute('INSERT INTO mobile_turn_outcomes VALUES(?,?,?,?,?)', (scope,key['run_id'],key['turn_id'],encode(outcome),int(time.time())))
    db.execute('INSERT INTO mobile_task_deliveries(workspace_id,run_id,turn_id,data_json,node_id) VALUES(?,?,?,?,?)', (scope,key['run_id'],key['turn_id'],encode(saved),node))
db.commit()
"""#, database.path, root.path, cycle]
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw E2EError.product("冻结成果夹具写入失败") }
    }
}
